"use strict";
/**
 * Zotero Better Notes companion bridge script.
 *
 * Exposes a slice of `Zotero.BetterNotes.api` over the Zotero MCP server's
 * loopback HTTP interface (`/better-notes/*`) so the Rust MCP server
 * (`src/better_notes/client.rs`) can convert notes, run templates, and read
 * note relations/trees.
 *
 * @remarks
 * `Zotero.BetterNotes.api` is an in-process JavaScript object that only
 * exists inside Zotero's own Firefox/Gecko-based extension runtime -- unlike
 * Better BibTeX, Better Notes exposes no HTTP or RPC endpoint of its own.
 * Reaching it requires executing JS in that same process, sharing its
 * `Zotero`/`Zotero.Items` globals, Web Workers, and IndexedDB-backed note
 * storage. There is no FFI, socket, or other cross-process interface a
 * separate Rust binary could call instead, so this script must run as a
 * Zotero-loaded script (via Developer Utilities or a startup script), with
 * the Rust MCP server talking to it over HTTP like any other Zotero
 * companion bridge.
 *
 * Compiled from `zotero-better-notes-bridge.ts` -- see `bridge/README.md`
 * for the build command. Do not edit the emitted `.js` directly.
 */
if (typeof Zotero !== "undefined" && Zotero.BetterNotes?.api) {
    Zotero.debug("[BetterNotesBridge] Initializing HTTP endpoint handlers...");
    if (!Zotero.BetterNotesBridge) {
        const api = Zotero.BetterNotes.api;
        /**
         * Looks up a note or attachment item by key in the user's library.
         *
         * @param itemKey - Zotero item key to resolve.
         * @returns The resolved item.
         * @throws Error if no item with `itemKey` exists in the user's
         * library.
         */
        async function requireItem(itemKey) {
            const item = await Zotero.Items.getByLibraryAndKeyAsync(Zotero.Libraries.userLibraryID, itemKey);
            if (!item) {
                throw new Error(`Item ${itemKey} not found`);
            }
            return item;
        }
        /** Maps each supported bridge endpoint path to its request handler. */
        const handlers = {
            /**
             * Reports whether the Better Notes API is loaded and ready.
             *
             * @returns `{ online: true, ready: true }`.
             */
            "/status": async () => ({
                online: true,
                // Better Notes doesn't expose a version field on `api`; report
                // readiness instead of fabricating a version string.
                ready: true,
            }),
            /**
             * Converts a note (`body.itemKey`) or raw HTML (`body.html`) to
             * Markdown.
             *
             * @param body - `{ itemKey?: string; html?: string }`.
             * @returns `{ markdown: string }`.
             * @throws Error if neither `itemKey` nor `html` is given, or
             * `itemKey` does not resolve to an item.
             */
            "/notes/to-markdown": async (body) => {
                const itemKey = body.itemKey;
                const html = body.html;
                if (itemKey) {
                    const item = await requireItem(itemKey);
                    // No sync folder in this headless context, so pass an empty `dir`
                    // and skip writing embedded images to disk.
                    const markdown = await api.convert.note2md(item, "", {
                        skipSavingImages: true,
                    });
                    return { markdown };
                }
                if (html) {
                    return { markdown: await api.convert.html2md(html) };
                }
                throw new Error("Missing itemKey or html");
            },
            /**
             * Creates a note from `body.markdown`, optionally as a child of
             * `body.parentKey`.
             *
             * @param body - `{ parentKey?: string; markdown: string }`.
             * @returns `{ itemKey: string }`, the key of the created note.
             * @throws Error if `markdown` is missing, `parentKey` does not
             * resolve to an item, or the new note's status cannot be read
             * back after saving.
             */
            "/notes/from-markdown": async (body) => {
                const parentKey = body.parentKey;
                const markdown = body.markdown;
                if (!markdown) {
                    throw new Error("Missing markdown");
                }
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
                    throw new Error(`Failed to read note status for ${noteItem.key}`);
                }
                const mdStatus = api.sync.getMDStatusFromContent(markdown);
                const parsedContent = await api.convert.md2note(mdStatus, noteItem, {
                    isImport: true,
                });
                noteItem.setNote(noteStatus.meta + parsedContent + noteStatus.tail);
                await noteItem.saveTx();
                return { itemKey: noteItem.key };
            },
            /**
             * Runs template `body.name` against the item identified by
             * `body.itemKey`.
             *
             * @param body - `{ name: string; itemKey: string }`.
             * @returns `{ result: string }`, the rendered template output.
             * @throws Error if `name` or `itemKey` is missing, or `itemKey`
             * does not resolve to an item.
             */
            "/templates/run": async (body) => {
                const name = body.name;
                const itemKey = body.itemKey;
                if (!name || !itemKey) {
                    throw new Error("Missing name or itemKey");
                }
                const item = await requireItem(itemKey);
                const result = await api.template.runItemTemplate(name, {
                    itemIds: [item.id],
                });
                return { result };
            },
            /**
             * Fetches inbound and outbound note-link relations for
             * `body.itemKey`.
             *
             * @param body - `{ itemKey: string }`.
             * @returns `{ relations: { outbound: BetterNotesRelationLink[];
             * inbound: BetterNotesRelationLink[] } }`.
             * @throws Error if `itemKey` is missing or does not resolve to an
             * item.
             */
            "/relations/get": async (body) => {
                const itemKey = body.itemKey;
                if (!itemKey) {
                    throw new Error("Missing itemKey");
                }
                const item = await requireItem(itemKey);
                const [outbound, inbound] = await Promise.all([
                    api.relation.getNoteLinkOutboundRelation(item.id),
                    api.relation.getNoteLinkInboundRelation(item.id),
                ]);
                return { relations: { outbound, inbound } };
            },
            /**
             * Builds the heading/note-link tree for `body.itemKey`.
             *
             * @param body - `{ itemKey: string }`.
             * @returns `{ tree: unknown }`, the tree produced by
             * `note.getNoteTree`.
             * @throws Error if `itemKey` is missing or does not resolve to an
             * item.
             */
            "/notes/tree": async (body) => {
                const itemKey = body.itemKey;
                if (!itemKey) {
                    throw new Error("Missing itemKey");
                }
                const item = await requireItem(itemKey);
                return { tree: await api.note.getNoteTree(item) };
            },
        };
        Zotero.BetterNotesBridge = {
            /**
             * Dispatches an HTTP-bridged request to the handler registered
             * for `path`.
             *
             * @param _method - HTTP method of the originating request
             * (unused; every endpoint here is invoked the same way
             * regardless of method).
             * @param path - Bridge endpoint path, e.g.
             * `"/notes/to-markdown"`.
             * @param body - Parsed JSON request body.
             * @returns The handler's JSON-serializable result.
             * @throws Error if no handler is registered for `path`.
             */
            async handleRequest(_method, path, body) {
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
