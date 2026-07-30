/**
 * Ambient typings for the Better Notes plugin's public API surface
 * (`Zotero.BetterNotes.api`), covering only the members this bridge calls.
 *
 * @remarks
 * Better Notes ships no type definitions of its own; these are hand-written
 * against its source at https://github.com/windingwind/zotero-better-notes
 * (`src/api.ts` and the modules it re-exports from `src/utils/convert.ts`,
 * `src/modules/sync/api.ts`, `src/modules/template/api.ts`,
 * `src/utils/relation.ts`, and `src/utils/note.ts`).
 */

/**
 * Parsed Markdown source, as returned by
 * {@link BetterNotesApi.sync.getMDStatusFromContent}.
 */
interface BetterNotesMDStatus {
    /** Parsed YAML front-matter, or `{ $version: -1 }` if the source had none. */
    meta: { $version?: number; [key: string]: unknown } | null;
    /** Markdown body with the YAML front-matter block (if any) stripped. */
    content: string;
    /** Directory the Markdown was read from, if it came from a file. Empty when built from a raw string. */
    filedir: string;
    /** File name the Markdown was read from, if it came from a file. Empty when built from a raw string. */
    filename: string;
    /** Last-modified timestamp of the source file, or the Unix epoch when built from a raw string. */
    lastmodify: Date;
}

/**
 * A note's stored HTML content split into its schema-version wrapper and
 * inner body, as returned by {@link BetterNotesApi.sync.getNoteStatus}.
 */
interface BetterNotesNoteStatus {
    /** Opening `<div data-schema-version="...">` wrapper tag. */
    meta: string;
    /** Note body HTML, with `meta` and `tail` stripped. */
    content: string;
    /** Closing wrapper tag, always `"</div>"`. */
    tail: string;
    /** Note's last-modified timestamp. */
    lastmodify?: Date;
}

/**
 * One directed note-link relation, as returned by
 * {@link BetterNotesApi.relation.getNoteLinkOutboundRelation} and
 * {@link BetterNotesApi.relation.getNoteLinkInboundRelation}.
 */
interface BetterNotesRelationLink {
    /** Library ID of the note the link originates from. */
    fromLibID: number;
    /** Item key of the note the link originates from. */
    fromKey: string;
    /** Library ID of the note the link points to. */
    toLibID: number;
    /** Item key of the note the link points to. */
    toKey: string;
    /** Line index of the link within the source note's content. */
    fromLine: number;
    /** Line index the link targets within the destination note, or `null` if it targets the whole note. */
    toLine: number | null;
    /** Heading section name the link targets within the destination note, or `null` if it targets the whole note. */
    toSection: string | null;
    /** Raw `zotero://note/...` link URL. */
    url: string;
}

/**
 * The slice of `Zotero.BetterNotes.api` this bridge calls.
 *
 * @remarks
 * See {@link BetterNotesMDStatus}, {@link BetterNotesNoteStatus}, and
 * {@link BetterNotesRelationLink} for the shapes these methods exchange.
 */
interface BetterNotesApi {
    convert: {
        /**
         * Converts a note's content to Markdown.
         *
         * @param noteItem - Note item to convert.
         * @param dir - Directory embedded images are saved to; ignored when
         * `options.skipSavingImages` is `true`.
         * @param options - Conversion options.
         * @returns The converted Markdown text.
         */
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
        /**
         * Converts raw HTML to Markdown.
         *
         * @param html - HTML source to convert.
         * @returns The converted Markdown text.
         */
        html2md(html: string): Promise<string>;
        /**
         * Converts parsed Markdown (`mdStatus`) into Zotero note HTML content
         * for `noteItem`. Does not save `noteItem` itself.
         *
         * @param mdStatus - Parsed Markdown source, from
         * {@link BetterNotesApi.sync.getMDStatusFromContent}.
         * @param noteItem - Note item providing library/attachment context for
         * the conversion (e.g. where imported images are attached).
         * @param options - Conversion options.
         * @returns The converted note HTML content.
         */
        md2note(
            mdStatus: BetterNotesMDStatus,
            noteItem: Zotero.Item,
            options?: { isImport?: boolean },
        ): Promise<string>;
    };
    sync: {
        /**
         * Parses raw Markdown text (optionally with a YAML front-matter block).
         *
         * @param content - Raw Markdown source.
         * @returns The parsed {@link BetterNotesMDStatus}.
         */
        getMDStatusFromContent(content: string): BetterNotesMDStatus;
        /**
         * Splits a note's stored HTML into its schema-version wrapper
         * (`meta`), inner `content`, and closing `tail`.
         *
         * @param noteId - `Zotero.Item.id` of the note.
         * @returns The note's {@link BetterNotesNoteStatus}, or `undefined` if
         * `noteId` does not identify a note item.
         */
        getNoteStatus(noteId: number): BetterNotesNoteStatus | undefined;
    };
    template: {
        /**
         * Runs template `key` against the items identified by
         * `options.itemIds`.
         *
         * @param key - Template name (e.g. `"[ExportMDFileContent]"`).
         * @param options - Items and target note to render the template
         * against.
         * @returns The rendered template output.
         */
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
        /**
         * Outbound note-link relations (links this note points to).
         *
         * @param noteID - `Zotero.Item.id` of the note.
         * @returns The note's outbound {@link BetterNotesRelationLink}s.
         */
        getNoteLinkOutboundRelation(
            noteID: number,
        ): Promise<BetterNotesRelationLink[]>;
        /**
         * Inbound note-link relations (links pointing to this note).
         *
         * @param noteID - `Zotero.Item.id` of the note.
         * @returns The note's inbound {@link BetterNotesRelationLink}s.
         */
        getNoteLinkInboundRelation(
            noteID: number,
        ): Promise<BetterNotesRelationLink[]>;
    };
    note: {
        /**
         * Builds the heading/note-link tree for `note`.
         *
         * @param note - Note item to build the tree for.
         * @param parseLink - Whether to include `zotero://note/...` links as
         * tree nodes alongside headings. Defaults to `true`.
         * @returns The root of the note's tree.
         */
        getNoteTree(note: Zotero.Item, parseLink?: boolean): Promise<unknown>;
    };
}

declare namespace Zotero {
    namespace BetterNotes {
        /**
         * The Better Notes plugin's public API, or `undefined` until the
         * plugin has finished loading.
         */
        const api: BetterNotesApi | undefined;
    }

    /**
     * Set by `zotero-better-notes-bridge.ts` once loaded, to guard against
     * double-registration if the script is run more than once.
     */
    let BetterNotesBridge:
        | {
              /**
               * Dispatches a bridge HTTP request to its registered handler.
               *
               * @param method - HTTP method of the originating request.
               * @param path - Bridge endpoint path.
               * @param body - Parsed JSON request body.
               * @returns The handler's JSON-serializable result.
               */
              handleRequest(
                  method: string,
                  path: string,
                  body: Record<string, unknown>,
              ): Promise<unknown>;
          }
        | undefined;
}
