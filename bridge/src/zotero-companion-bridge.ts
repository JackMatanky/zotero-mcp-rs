/**
 * Zotero companion bridge script.
 *
 * Exposes Zotero in-process capabilities over the Zotero MCP server's loopback
 * HTTP interface. Better Notes handlers proxy a slice of
 * `Zotero.BetterNotes.api`; `/file-roots` reports Zotero-managed attachment
 * directories for PDF path policy.
 *
 * @remarks
 * `Zotero.BetterNotes.api` and Zotero preference/storage APIs only exist
 * inside Zotero's own Firefox/Gecko-based extension runtime. Reaching them
 * requires executing JS in that same process, sharing its `Zotero` globals,
 * Web Workers, and IndexedDB-backed note storage. There is no FFI, socket, or
 * other cross-process interface a separate Rust binary could call instead, so
 * this script must run as a Zotero-loaded script (via Developer Utilities or a
 * startup script), with the Rust MCP server talking to it over HTTP like any
 * other Zotero companion bridge.
 *
 * Compiled from `zotero-companion-bridge.ts` — see `bridge/README.md` for the
 * build command. Do not edit the emitted `.js` directly.
 */

/** Parsed JSON object posted to a bridge endpoint. */
type BridgeBody = Record<string, unknown>;
/** Root kinds returned by `/file-roots` and consumed by Rust PDF policy code. */
type FileRootKind = "zotero-storage" | "zotero-linked-base" | "attanger-dest";
/** One absolute Zotero-managed filesystem root returned by `/file-roots`. */
type FileRoot = { kind: FileRootKind; path: string };

/**
 * Handles one bridge endpoint request and returns the JSON-serializable
 * response body.
 */
type BridgeHandler = (body: BridgeBody) => Promise<unknown>;
/** Output format accepted by `/notes/export`. */
type NoteExportFormat = "markdown" | "html";
/** Maximum accepted Markdown request body size, in UTF-8 bytes. */
const MAX_MARKDOWN_BYTES = 2 * 1024 * 1024;
/** Maximum accepted HTML request body size, in UTF-8 bytes. */
const MAX_HTML_BYTES = 2 * 1024 * 1024;
/** Maximum accepted Better Notes template name size, in UTF-8 bytes. */
const MAX_TEMPLATE_NAME_BYTES = 128;

if (typeof Zotero !== "undefined") {
    Zotero.debug("[BetterNotesBridge] Initializing HTTP endpoint handlers...");

    if (!Zotero.BetterNotesBridge) {
        /**
         * Looks up a Zotero library item by key.
         *
         * @param itemKey - Zotero item key to resolve.
         * @returns The resolved user-library item.
         * @throws Error if no user-library item exists for `itemKey`.
         */
        async function requireItem(itemKey: string): Promise<Zotero.Item> {
            const item = await Zotero.Items.getByLibraryAndKeyAsync(
                Zotero.Libraries.userLibraryID,
                itemKey,
            );
            if (!item) {
                throw new Error(`Item ${itemKey} not found`);
            }
            return item;
        }

        /**
         * Returns the Better Notes API after confirming the plugin has loaded.
         *
         * @returns The loaded Better Notes API object.
         * @throws Error if Better Notes is missing or not initialized yet.
         */
        function requireBetterNotesApi(): BetterNotesApi {
            const api = Zotero.BetterNotes?.api;
            if (!api) {
                throw new Error("Better Notes API is not loaded");
            }
            return api;
        }

        /**
         * Appends a root only when Zotero returned a non-empty string path.
         *
         * Zotero prefs can be absent, booleans, or other values depending on
         * installed plugins and initialization state; those are intentionally
         * ignored rather than serialized.
         *
         * @param roots - Mutable result list being assembled for `/file-roots`.
         * @param kind - Root category understood by Rust PDF path policy.
         * @param path - Candidate path returned by Zotero or plugin prefs.
         */
        function pushRoot(
            roots: FileRoot[],
            kind: FileRootKind,
            path: unknown,
        ): void {
            if (typeof path === "string" && path.length > 0) {
                roots.push({ kind, path });
            }
        }

        /**
         * Reads a Zotero pref without letting unavailable optional plugins
         * break `/file-roots`.
         *
         * @param name - Zotero preference name.
         * @returns The preference value, or `undefined` when lookup fails.
         */
        function readPref(name: string): unknown {
            try {
                return Zotero.Prefs?.get(name);
            } catch {
                return undefined;
            }
        }

        /**
         * Reads Zotero's storage directory without failing the whole root list.
         *
         * @returns The storage path, or `undefined` when Zotero cannot provide
         * it in the current runtime state.
         */
        function readStoragePath(): unknown {
            try {
                return Zotero.getStorageDirectory?.()?.path;
            } catch {
                return undefined;
            }
        }

        /**
         * Reports Zotero-managed PDF roots. This endpoint must work even when
         * Better Notes is not installed.
         *
         * @param _body - Ignored request body; `/file-roots` takes no input.
         * @returns Zotero-managed PDF roots available in this runtime.
         */
        async function handleFileRoots(
            _body: BridgeBody,
        ): Promise<{ roots: FileRoot[] }> {
            const roots: FileRoot[] = [];
            pushRoot(roots, "zotero-storage", readStoragePath());
            pushRoot(
                roots,
                "zotero-linked-base",
                readPref("baseAttachmentPath"),
            );
            const attangerEnabled =
                readPref("extensions.zotero.zoteroattanger.enable") !== false;
            const attangerLinking =
                readPref("extensions.zotero.zoteroattanger.attachType") ===
                "linking";
            if (attangerEnabled && attangerLinking) {
                pushRoot(
                    roots,
                    "attanger-dest",
                    readPref("extensions.zotero.zoteroattanger.destDir"),
                );
            }
            return { roots };
        }

        /**
         * Reports whether the Better Notes API is loaded and ready.
         *
         * @returns `{ online: true, ready }`, where `ready` reports whether
         * Better Notes' API is currently loaded.
         */
        async function handleStatus(): Promise<unknown> {
            return {
                online: true,
                ready: Boolean(Zotero.BetterNotes?.api),
            };
        }

        /**
         * Reads a string field from a bridge request body.
         *
         * @param body - Parsed JSON request body.
         * @param field - Field name to read.
         * @returns The string value, or `undefined` when the field is absent.
         * @throws Error if `field` is present but is not a string.
         */
        function readStringBodyField(
            body: BridgeBody,
            field: string,
        ): string | undefined {
            const value = body[field];
            if (value === undefined) {
                return undefined;
            }
            if (typeof value === "string") {
                return value;
            }
            throw new Error(`Invalid ${field}`);
        }

        /**
         * Rejects oversized string input by UTF-8 byte length.
         *
         * @param value - Input string.
         * @param field - User-visible field name.
         * @param maxBytes - Maximum accepted UTF-8 bytes.
         * @throws Error if `value` exceeds `maxBytes`.
         */
        function assertMaxUtf8Bytes(
            value: string,
            field: string,
            maxBytes: number,
        ): void {
            if (new TextEncoder().encode(value).length > maxBytes) {
                throw new Error(`${field} exceeds ${maxBytes} bytes`);
            }
        }

        /**
         * Reads and validates `body.format` for note export.
         *
         * @param body - Parsed JSON request body.
         * @returns The requested export format, defaulting to `"markdown"`.
         * @throws Error if `format` is not a string or is unsupported.
         */
        function readNoteExportFormat(body: BridgeBody): NoteExportFormat {
            const format = readStringBodyField(body, "format") ?? "markdown";
            if (format === "markdown" || format === "html") {
                return format;
            }
            throw new Error(
                `Unsupported note export format: ${String(format)}`,
            );
        }

        /**
         * Exports a note (`body.itemKey`) as Markdown or HTML.
         *
         * @param body - `{ itemKey: string; format?: "markdown" | "html" }`;
         * `format` defaults to `"markdown"`.
         * @returns `{ content: string }`.
         * @throws Error if `itemKey` is missing or invalid, the item does not
         * exist, or `format` is invalid or unsupported.
         */
        async function handleNoteExport(body: BridgeBody): Promise<unknown> {
            const itemKey = readStringBodyField(body, "itemKey");
            if (!itemKey) {
                throw new Error("Missing itemKey");
            }
            const format = readNoteExportFormat(body);
            const item = await requireItem(itemKey);
            const api = requireBetterNotesApi();
            if (format === "html") {
                return { content: await api.convert.note2html(item) };
            }
            const content = await api.convert.note2md(item, "", {
                skipSavingImages: true,
            });
            return { content };
        }

        /**
         * Converts raw HTML (`body.html`) to Markdown.
         *
         * @param body - `{ html: string }`.
         * @returns `{ markdown: string }`.
         * @throws Error if `html` is missing or invalid.
         */
        async function handleNoteToMarkdown(
            body: BridgeBody,
        ): Promise<unknown> {
            const html = readStringBodyField(body, "html");
            assertMaxUtf8Bytes(html ?? "", "html", MAX_HTML_BYTES);
            const api = requireBetterNotesApi();
            if (html !== undefined) {
                return { markdown: await api.convert.html2md(html) };
            }
            throw new Error("Missing html");
        }

        /**
         * Creates a note from `body.markdown`, optionally under
         * `body.parentKey`.
         *
         * @param body - `{ parentKey?: string; markdown: string }`.
         * @returns `{ itemKey: string }`, the created note item's key.
         * @throws Error if `markdown` is missing or invalid, `parentKey` is
         * invalid or does not resolve to an item, the new note's status cannot
         * be read back after saving, or Better Notes conversion fails. If
         * conversion fails after the empty note is saved, the note is erased.
         */
        async function handleNoteFromMarkdown(
            body: BridgeBody,
        ): Promise<unknown> {
            const parentKey = readStringBodyField(body, "parentKey");
            const markdown = readStringBodyField(body, "markdown");
            if (markdown === undefined) {
                throw new Error("Missing markdown");
            }
            assertMaxUtf8Bytes(markdown, "markdown", MAX_MARKDOWN_BYTES);
            const api = requireBetterNotesApi();

            const noteItem = new Zotero.Item("note");
            if (parentKey) {
                const parent = await requireItem(parentKey);
                noteItem.parentID = parent.id;
                noteItem.libraryID = parent.libraryID;
            }
            noteItem.setNote("");
            await noteItem.saveTx();

            try {
                const noteStatus = api.sync.getNoteStatus(noteItem.id);
                if (!noteStatus) {
                    throw new Error(
                        `Failed to read note status for ${noteItem.key}`,
                    );
                }
                const mdStatus = api.sync.getMDStatusFromContent(markdown);
                const parsedContent = await api.convert.md2note(
                    mdStatus,
                    noteItem,
                    {
                        isImport: true,
                    },
                );
                noteItem.setNote(
                    noteStatus.meta + parsedContent + noteStatus.tail,
                );
                await noteItem.saveTx();

                return { itemKey: noteItem.key };
            } catch (error) {
                await noteItem.eraseTx().catch(() => undefined);
                throw error;
            }
        }

        /**
         * Runs Better Notes template `body.name` against `body.itemKey`.
         *
         * @param body - `{ name: string; itemKey: string }`.
         * @returns `{ result: string }`, the rendered template output.
         * @throws Error if `name` or `itemKey` is missing or invalid, or
         * `itemKey` does not resolve to an item.
         */
        async function handleRunTemplate(body: BridgeBody): Promise<unknown> {
            const name = readStringBodyField(body, "name");
            const itemKey = readStringBodyField(body, "itemKey");
            if (!name || !itemKey) {
                throw new Error("Missing name or itemKey");
            }
            assertMaxUtf8Bytes(name, "template name", MAX_TEMPLATE_NAME_BYTES);
            const item = await requireItem(itemKey);
            const api = requireBetterNotesApi();
            const result = await api.template.runItemTemplate(name, {
                itemIds: [item.id],
            });
            return { result };
        }

        /**
         * Fetches inbound and outbound note-link relations for `body.itemKey`.
         *
         * @param body - `{ itemKey: string }`.
         * @returns `{ relations }`, where `relations.outbound` and
         * `relations.inbound` contain note-link relation arrays.
         * @throws Error if `itemKey` is missing, invalid, or does not resolve
         * to an item.
         */
        async function handleGetRelations(body: BridgeBody): Promise<unknown> {
            const itemKey = readStringBodyField(body, "itemKey");
            if (!itemKey) {
                throw new Error("Missing itemKey");
            }
            const item = await requireItem(itemKey);
            const api = requireBetterNotesApi();
            const [outbound, inbound] = await Promise.all([
                api.relation.getNoteLinkOutboundRelation(item.id),
                api.relation.getNoteLinkInboundRelation(item.id),
            ]);
            return { relations: { outbound, inbound } };
        }

        /**
         * Builds the heading/note-link tree for `body.itemKey`.
         *
         * @param body - `{ itemKey: string }`.
         * @returns `{ tree: unknown }`, the note tree returned by Better Notes.
         * @throws Error if `itemKey` is missing, invalid, or does not resolve
         * to an item.
         */
        async function handleGetNoteTree(body: BridgeBody): Promise<unknown> {
            const itemKey = readStringBodyField(body, "itemKey");
            if (!itemKey) {
                throw new Error("Missing itemKey");
            }
            const item = await requireItem(itemKey);
            const api = requireBetterNotesApi();
            return { tree: await api.note.getNoteTree(item) };
        }

        /** Supported bridge endpoint handlers, keyed by request path. */
        const handlers: Readonly<Partial<Record<string, BridgeHandler>>> = {
            "/status": handleStatus,
            "/file-roots": handleFileRoots,
            "/notes/export": handleNoteExport,
            "/notes/to-markdown": handleNoteToMarkdown,
            "/notes/from-markdown": handleNoteFromMarkdown,
            "/templates/run": handleRunTemplate,
            "/relations/get": handleGetRelations,
            "/notes/tree": handleGetNoteTree,
        };

        Zotero.BetterNotesBridge = {
            /**
             * Dispatches an HTTP-bridged request to the handler registered
             * for `path`.
             *
             * @param method - HTTP method of the originating request.
             * @param path - Bridge endpoint path, e.g. `"/notes/export"` for
             * note export or `"/notes/to-markdown"` for raw HTML conversion.
             * @param body - Parsed JSON request body.
             * @returns The handler's JSON-serializable result.
             * @throws Error if `method` is not `POST` or no handler is
             * registered for `path`.
             */
            async handleRequest(
                method: string,
                path: string,
                body: BridgeBody,
            ): Promise<unknown> {
                if (method !== "POST") {
                    throw new Error(`Unsupported bridge method: ${method}`);
                }
                const handler = handlers[path];
                if (!handler) {
                    throw new Error(`Unknown bridge endpoint: ${path}`);
                }
                return handler(body);
            },
        };
    }
    Zotero.debug("[BetterNotesBridge] Ready.");
}
