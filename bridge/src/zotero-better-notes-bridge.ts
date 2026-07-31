/**
 * Zotero Better Notes companion bridge script.
 *
 * Exposes a slice of `Zotero.BetterNotes.api` over the Zotero MCP server's
 * loopback HTTP interface (`/better-notes/*`) so the Rust MCP server
 * (`src/better_notes/client.rs`) can export notes, convert Markdown/HTML,
 * run templates, and read note relations/trees.
 *
 * @remarks
 * `Zotero.BetterNotes.api` is an in-process JavaScript object that only
 * exists inside Zotero's own Firefox/Gecko-based extension runtime — unlike
 * Better BibTeX, Better Notes exposes no HTTP or RPC endpoint of its own.
 * Reaching it requires executing JS in that same process, sharing its
 * `Zotero`/`Zotero.Items` globals, Web Workers, and IndexedDB-backed note
 * storage. There is no FFI, socket, or other cross-process interface a
 * separate Rust binary could call instead, so this script must run as a
 * Zotero-loaded script (via Developer Utilities or a startup script), with
 * the Rust MCP server talking to it over HTTP like any other Zotero
 * companion bridge.
 *
 * Compiled from `zotero-better-notes-bridge.ts` — see `bridge/README.md`
 * for the build command. Do not edit the emitted `.js` directly.
 */

/** Parsed JSON object posted to a bridge endpoint. */
type BridgeBody = Record<string, unknown>;

/**
 * Handles one bridge endpoint request and returns the JSON-serializable
 * response body.
 */
type BridgeHandler = (body: BridgeBody) => Promise<unknown>;
/** Output format accepted by `/notes/export`. */
type NoteExportFormat = "markdown" | "html";
const MAX_MARKDOWN_BYTES = 2 * 1024 * 1024;
const MAX_HTML_BYTES = 2 * 1024 * 1024;
const MAX_TEMPLATE_NAME_BYTES = 128;


if (typeof Zotero !== "undefined" && Zotero.BetterNotes?.api) {
    Zotero.debug("[BetterNotesBridge] Initializing HTTP endpoint handlers...");

    if (!Zotero.BetterNotesBridge) {
        const api = Zotero.BetterNotes.api;

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
         * Reports whether the Better Notes API is loaded and ready.
         *
         * @returns `{ online: true, ready: true }`.
         */
        async function handleStatus(): Promise<unknown> {
            return {
                online: true,
                // Better Notes doesn't expose a version field on `api`; report
                // readiness instead of fabricating a version string.
                ready: true,
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
            const item = await requireItem(itemKey);
            const format = readNoteExportFormat(body);
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
         * invalid or does not resolve to an item, or the new note's status
         * cannot be read back after saving.
         */
        async function handleNoteFromMarkdown(
            body: BridgeBody,
        ): Promise<unknown> {
            const parentKey = readStringBodyField(body, "parentKey");
            const markdown = readStringBodyField(body, "markdown");
            if (markdown === undefined) {
                throw new Error("Missing markdown");
            }
            assertMaxUtf8Bytes(
                markdown,
                "markdown",
                MAX_MARKDOWN_BYTES,
            );

            const noteItem = new Zotero.Item("note");
            if (parentKey) {
                const parent = await requireItem(parentKey);
                noteItem.parentID = parent.id;
                noteItem.libraryID = parent.libraryID;
            }
            noteItem.setNote("");
            await noteItem.saveTx();

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
            noteItem.setNote(noteStatus.meta + parsedContent + noteStatus.tail);
            await noteItem.saveTx();

            return { itemKey: noteItem.key };
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
            return { tree: await api.note.getNoteTree(item) };
        }

        /** Supported bridge endpoint handlers, keyed by request path. */
        const handlers: Readonly<Partial<Record<string, BridgeHandler>>> = {
            "/status": handleStatus,
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
             * @param _method - HTTP method of the originating request
             * (unused; every endpoint here is invoked the same way regardless
             * of method).
             * @param path - Bridge endpoint path, e.g. `"/notes/export"` for
             * note export or `"/notes/to-markdown"` for raw HTML conversion.
             * @param body - Parsed JSON request body.
             * @returns The handler's JSON-serializable result.
             * @throws Error if no handler is registered for `path`.
             */
            async handleRequest(
                _method: string,
                path: string,
                body: BridgeBody,
            ): Promise<unknown> {
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
