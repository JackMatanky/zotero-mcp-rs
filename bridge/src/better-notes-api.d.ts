/**
 * Ambient typings for the Better Notes plugin's public API surface
 * (`Zotero.BetterNotes.api`), covering only the members this bridge calls.
 *
 * Better Notes ships no type definitions of its own; these are hand-written
 * against its source at https://github.com/windingwind/zotero-better-notes
 * (`src/api.ts` and the modules it re-exports from `src/utils/convert.ts`,
 * `src/modules/sync/api.ts`, `src/modules/template/api.ts`,
 * `src/utils/relation.ts`, and `src/utils/note.ts`).
 */

interface BetterNotesMDStatus {
    meta: { $version?: number; [key: string]: unknown } | null;
    content: string;
    filedir: string;
    filename: string;
    lastmodify: Date;
}

interface BetterNotesNoteStatus {
    meta: string;
    content: string;
    tail: string;
    lastmodify?: Date;
}

interface BetterNotesRelationLink {
    fromLibID: number;
    fromKey: string;
    toLibID: number;
    toKey: string;
    fromLine: number;
    toLine: number | null;
    toSection: string | null;
    url: string;
}

interface BetterNotesApi {
    convert: {
        /** Converts a note's content to Markdown. */
        note2md(
            noteItem: Zotero.Item,
            dir: string,
            options?: {
                keepNoteLink?: boolean;
                withYAMLHeader?: boolean;
                skipSavingImages?: boolean;
                skipTemplate?: boolean;
                noteContent?: string;
            },
        ): Promise<string>;
        /** Converts raw HTML to Markdown. */
        html2md(html: string): Promise<string>;
        /**
         * Converts parsed Markdown (`mdStatus`) into Zotero note HTML content
         * for `noteItem`. Does not save `noteItem` itself.
         */
        md2note(
            mdStatus: BetterNotesMDStatus,
            noteItem: Zotero.Item,
            options?: { isImport?: boolean },
        ): Promise<string>;
    };
    sync: {
        /** Parses raw Markdown text (optionally with a YAML front-matter block) into an [`BetterNotesMDStatus`]. */
        getMDStatusFromContent(content: string): BetterNotesMDStatus;
        /**
         * Splits a note's stored HTML into its schema-version wrapper (`meta`),
         * inner `content`, and closing `tail`. Returns `undefined` if `noteId`
         * is not a note item.
         */
        getNoteStatus(noteId: number): BetterNotesNoteStatus | undefined;
    };
    template: {
        /** Runs template `key` against the items identified by `options.itemIds`. */
        runItemTemplate(
            key: string,
            options?: {
                itemIds?: number[];
                targetNoteId?: number;
                dryRun?: boolean;
            },
        ): Promise<string>;
    };
    relation: {
        /** Outbound note-link relations (links this note points to). */
        getNoteLinkOutboundRelation(
            noteID: number,
        ): Promise<BetterNotesRelationLink[]>;
        /** Inbound note-link relations (links pointing to this note). */
        getNoteLinkInboundRelation(
            noteID: number,
        ): Promise<BetterNotesRelationLink[]>;
    };
    note: {
        /** Builds the heading/note-link tree for `note`. */
        getNoteTree(note: Zotero.Item, parseLink?: boolean): Promise<unknown>;
    };
}

declare namespace Zotero {
    namespace BetterNotes {
        const api: BetterNotesApi | undefined;
    }

    let BetterNotesBridge:
        | {
              handleRequest(
                  method: string,
                  path: string,
                  body: Record<string, unknown>,
              ): Promise<unknown>;
          }
        | undefined;
}
