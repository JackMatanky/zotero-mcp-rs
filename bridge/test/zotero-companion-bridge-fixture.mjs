import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const script = fs.readFileSync(
  new URL("../zotero-companion-bridge.js", import.meta.url),
  "utf8",
);

/**
 * @typedef {object} LoadBridgeOptions
 * @property {boolean} [withBetterNotes=true] Whether to expose
 * `Zotero.BetterNotes.api` in the VM.
 * @property {Record<string, unknown>} [prefs] Values returned by
 * `Zotero.Prefs.get`.
 * @property {string} [storagePath] Path returned by Zotero storage lookup.
 * Ignored when `storageDirectory` is provided.
 * @property {{ path?: string } | undefined} [storageDirectory] Raw object
 * returned by `Zotero.getStorageDirectory`.
 * @property {boolean} [storageThrows=false] Whether storage lookup throws.
 * @property {string[]} [throwPrefs] Preference names whose lookup throws.
 * @property {string[]} [missingItemKeys] Item keys that resolve to `null`.
 * @property {Record<string, object | null>} [itemsByKey] Per-key item lookup
 * overrides.
 * @property {object} [item] Default item returned by item lookup.
 * @property {number} [newNoteId=2] ID assigned to created note fixture items.
 * @property {string} [newNoteKey="NEWNOTE"] Key assigned to created notes.
 * @property {boolean} [saveTxThrows=false] Whether every note save throws.
 * @property {string} [html2mdResult="markdown"] Raw-HTML conversion result.
 * @property {string} [md2noteResult="parsed"] Markdown import conversion
 * result.
 * @property {boolean} [md2noteThrows=false] Whether Markdown import throws.
 * @property {string} [note2htmlResult="<p>note</p>"] HTML export result.
 * @property {string} [note2mdResult="note"] Markdown export result.
 * @property {object | undefined} [noteStatus] Better Notes note-status result.
 * @property {string} [templateResult="ok"] Template render result.
 * @property {unknown[]} [outboundRelations] Outbound relation result.
 * @property {unknown[]} [inboundRelations] Inbound relation result.
 * @property {unknown} [noteTree] Note-tree result.
 */

/**
 * Loads the generated bridge script in an isolated VM with a minimal Zotero
 * global.
 *
 * `npm test` rebuilds the emitted `.js` before loading this fixture, so tests
 * exercise the same generated script Zotero loads.
 *
 * @param {LoadBridgeOptions} [options] Fixture behavior overrides.
 * @returns {{ calls: Record<string, number>, bridge: object, createdNotes: object[] }}
 * Counters, the registered bridge object, and notes constructed by the test.
 */

export function loadBridge(options = {}) {
  const calls = {
    eraseTx: 0,
    html2md: 0,
    itemConstructed: 0,
    itemLookup: 0,
    md2note: 0,
    note2html: 0,
    note2md: 0,
    noteTree: 0,
    runTemplate: 0,
    saveTx: 0,
    setNote: 0,
  };
  const defaultItem = { id: 1, key: "NOTE1", libraryID: 1 };
  const prefs = options.prefs ?? {};
  const createdNotes = [];
  const zotero = {
    debug() {},
    getStorageDirectory() {
      if (options.storageThrows) {
        throw new Error("storage failed");
      }
      return options.storageDirectory ?? {
        path: options.storagePath ?? "/Users/alice/Zotero/storage",
      };
    },
    Prefs: {
      get(name) {
        if (options.throwPrefs?.includes(name)) {
          throw new Error(`pref ${name} failed`);
        }
        return prefs[name];
      },
    },
    Libraries: { userLibraryID: 1 },
    Items: {
      async getByLibraryAndKeyAsync(_libraryId, key) {
        calls.itemLookup += 1;
        if (options.missingItemKeys?.includes(key)) {
          return null;
        }
        return options.itemsByKey?.[key] ?? options.item ?? defaultItem;
      },
    },
    Item: class {
      constructor(type) {
        calls.itemConstructed += 1;
        this.id = options.newNoteId ?? 2;
        this.key = options.newNoteKey ?? "NEWNOTE";
        this.libraryID = 1;
        this.type = type;
        this.notes = [];
        createdNotes.push(this);
      }
      setNote(note) {
        calls.setNote += 1;
        this.note = note;
        this.notes.push(note);
      }
      async saveTx() {
        calls.saveTx += 1;
        if (options.saveTxThrows) {
          throw new Error("save failed");
        }
        return true;
      }
      async eraseTx() {
        calls.eraseTx += 1;
        this.erased = true;
        return true;
      }
    },
  };
  if (options.withBetterNotes !== false) {
    zotero.BetterNotes = {
      api: {
        convert: {
          async html2md() {
            calls.html2md += 1;
            return options.html2mdResult ?? "markdown";
          },
          async md2note() {
            calls.md2note += 1;
            if (options.md2noteThrows) {
              throw new Error("md2note failed");
            }
            return options.md2noteResult ?? "parsed";
          },
          async note2html() {
            calls.note2html += 1;
            return options.note2htmlResult ?? "<p>note</p>";
          },
          async note2md() {
            calls.note2md += 1;
            return options.note2mdResult ?? "note";
          },
        },
        sync: {
          getNoteStatus() {
            return options.noteStatus ?? { meta: "", tail: "" };
          },
          getMDStatusFromContent(markdown) {
            return { markdown };
          },
        },
        template: {
          async runItemTemplate() {
            calls.runTemplate += 1;
            return options.templateResult ?? "ok";
          },
        },
        relation: {
          async getNoteLinkOutboundRelation() {
            return options.outboundRelations ?? [];
          },
          async getNoteLinkInboundRelation() {
            return options.inboundRelations ?? [];
          },
        },
        note: {
          async getNoteTree() {
            calls.noteTree += 1;
            return options.noteTree ?? {};
          },
        },
      },
    };
  }
  const context = { TextEncoder, Zotero: zotero };
  vm.runInNewContext(script, context);
  return { calls, bridge: context.Zotero.BetterNotesBridge, createdNotes };
}

/**
 * Asserts that an async bridge call rejects with an error message matching
 * `match`.
 */
export async function rejects(match, fn) {
  await assert.rejects(fn, (err) => {
    assert.match(err.message, match);
    return true;
  });
}

/**
 * Converts VM-created objects into this test file's JavaScript realm so
 * `assert.deepEqual` compares plain objects instead of VM prototypes.
 */
export function plain(value) {
  return JSON.parse(JSON.stringify(value));
}
