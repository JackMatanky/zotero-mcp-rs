import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import vm from "node:vm";

const script = fs.readFileSync(
  new URL("../zotero-better-notes-bridge.js", import.meta.url),
  "utf8",
);

function loadBridge() {
  const calls = {
    itemConstructed: 0,
    html2md: 0,
    itemLookup: 0,
    runTemplate: 0,
  };
  const item = { id: 1, key: "NOTE1", libraryID: 1 };
  const context = {
    TextEncoder,
    Zotero: {
      debug() {},
      Libraries: { userLibraryID: 1 },
      Items: {
        async getByLibraryAndKeyAsync() {
          calls.itemLookup += 1;
          return item;
        },
      },
      Item: class {
        constructor() {
          calls.itemConstructed += 1;
          this.id = 2;
          this.key = "NEWNOTE";
        }
        setNote() {}
        async saveTx() {}
      },
      BetterNotes: {
        api: {
          convert: {
            async html2md() {
              calls.html2md += 1;
              return "markdown";
            },
            async md2note() {
              return "parsed";
            },
            async note2html() {
              return "<p>note</p>";
            },
            async note2md() {
              return "note";
            },
          },
          sync: {
            getNoteStatus() {
              return { meta: "", tail: "" };
            },
            getMDStatusFromContent(markdown) {
              return { markdown };
            },
          },
          template: {
            async runItemTemplate() {
              calls.runTemplate += 1;
              return "ok";
            },
          },
          relation: {
            async getNoteLinkOutboundRelation() {
              return [];
            },
            async getNoteLinkInboundRelation() {
              return [];
            },
          },
          note: {
            async getNoteTree() {
              return {};
            },
          },
        },
      },
    },
  };
  vm.runInNewContext(script, context);
  return { calls, bridge: context.Zotero.BetterNotesBridge };
}

async function rejects(match, fn) {
  await assert.rejects(fn, (err) => {
    assert.match(err.message, match);
    return true;
  });
}

test("rejects oversized markdown before creating note", async () => {
  const { calls, bridge } = loadBridge();

  await rejects(/markdown/, () =>
    bridge.handleRequest("POST", "/notes/from-markdown", {
      markdown: "a".repeat(2 * 1024 * 1024 + 1),
    }),
  );

  assert.equal(calls.itemConstructed, 0);
});

test("rejects oversized html before converting", async () => {
  const { calls, bridge } = loadBridge();

  await rejects(/html/, () =>
    bridge.handleRequest("POST", "/notes/to-markdown", {
      html: "a".repeat(2 * 1024 * 1024 + 1),
    }),
  );

  assert.equal(calls.html2md, 0);
});

test("rejects oversized template name before item lookup", async () => {
  const { calls, bridge } = loadBridge();

  await rejects(/name/, () =>
    bridge.handleRequest("POST", "/templates/run", {
      name: "a".repeat(129),
      itemKey: "NOTE1",
    }),
  );

  assert.equal(calls.itemLookup, 0);
});

test("keeps normal template execution working", async () => {
  const { calls, bridge } = loadBridge();

  const result = await bridge.handleRequest("POST", "/templates/run", {
    name: "Export",
    itemKey: "NOTE1",
  });

  assert.equal(result.result, "ok");
  assert.equal(calls.itemLookup, 1);
  assert.equal(calls.runTemplate, 1);
});
