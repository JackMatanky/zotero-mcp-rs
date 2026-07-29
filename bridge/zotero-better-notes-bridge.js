/**
 * Zotero Better Notes Companion Bridge Script
 * 
 * This script runs inside Zotero (via Developer Utilities or Zotero startup script)
 * to expose Zotero.BetterNotes.api methods over a loopback HTTP interface on port 23119
 * under /better-notes/.
 */

if (typeof Zotero !== 'undefined' && Zotero.BetterNotes) {
    Zotero.debug("[BetterNotesBridge] Initializing HTTP endpoint handlers...");

    if (!Zotero.BetterNotesBridge) {
        Zotero.BetterNotesBridge = {
            async handleRequest(method, path, body) {
                switch (path) {
                    case '/status':
                        return {
                            online: true,
                            version: Zotero.BetterNotes.api ? Zotero.BetterNotes.api.version || "1.0.0" : "unknown"
                        };

                    case '/notes/to-markdown': {
                        const { itemKey, html } = body;
                        if (itemKey) {
                            const item = await Zotero.Items.getAsync(itemKey);
                            if (!item) throw new Error(`Item ${itemKey} not found`);
                            const md = await Zotero.BetterNotes.api.convert.note2md(item);
                            return { markdown: md };
                        } else if (html) {
                            const md = await Zotero.BetterNotes.api.convert.html2md(html);
                            return { markdown: md };
                        }
                        throw new Error("Missing itemKey or html");
                    }

                    case '/notes/from-markdown': {
                        const { parentKey, markdown } = body;
                        const noteItem = await Zotero.BetterNotes.api.convert.md2note(markdown, parentKey);
                        return { itemKey: noteItem.key };
                    }

                    case '/templates/run': {
                        const { name, itemKey } = body;
                        const result = await Zotero.BetterNotes.api.template.runTemplate(name, itemKey);
                        return { result };
                    }

                    case '/relations/get': {
                        const { itemKey } = body;
                        const item = await Zotero.Items.getAsync(itemKey);
                        const relations = await Zotero.BetterNotes.api.relation.getAllNoteLinkRelations(item.id);
                        return { relations };
                    }

                    case '/notes/tree': {
                        const { itemKey } = body;
                        const item = await Zotero.Items.getAsync(itemKey);
                        const tree = await Zotero.BetterNotes.api.note.getNoteTree(item.id);
                        return { tree };
                    }

                    default:
                        throw new Error(`Unknown bridge endpoint: ${path}`);
                }
            }
        };
    }
    Zotero.debug("[BetterNotesBridge] Ready.");
}
