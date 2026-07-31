/**
 * Behavior coverage for the Better Notes-dependent bridge endpoints and shared
 * request dispatcher.
 */
import assert from "node:assert/strict";
import test from "node:test";

import {
  loadBridge,
  plain,
  rejects,
} from "./zotero-companion-bridge-fixture.mjs";

test("reports Better Notes readiness", async () => {
  const ready = loadBridge();
  const missing = loadBridge({ withBetterNotes: false });

  assert.deepEqual(
    plain(await ready.bridge.handleRequest("POST", "/status", {})),
    { online: true, ready: true },
  );
  assert.deepEqual(
    plain(await missing.bridge.handleRequest("POST", "/status", {})),
    { online: true, ready: false },
  );
});

test("rejects unsupported methods and unknown endpoints", async () => {
  const { bridge } = loadBridge();

  await rejects(/Unsupported bridge method/, () =>
    bridge.handleRequest("GET", "/status", {}),
  );
  await rejects(/Unknown bridge endpoint/, () =>
    bridge.handleRequest("POST", "/missing", {}),
  );
});

for (const [path, body] of [
  ["/notes/export", { itemKey: "NOTE1" }],
  ["/notes/to-markdown", { html: "<p>note</p>" }],
  ["/notes/from-markdown", { markdown: "# note" }],
  ["/templates/run", { name: "Export", itemKey: "NOTE1" }],
  ["/relations/get", { itemKey: "NOTE1" }],
  ["/notes/tree", { itemKey: "NOTE1" }],
]) {
  test(`${path} requires Better Notes`, async () => {
    const { bridge } = loadBridge({ withBetterNotes: false });

    await rejects(/Better Notes API is not loaded/, () =>
      bridge.handleRequest("POST", path, body),
    );
  });
}

test("exports notes as markdown by default and html when requested", async () => {
  const { calls, bridge } = loadBridge({
    note2htmlResult: "<p>html</p>",
    note2mdResult: "markdown",
  });

  assert.deepEqual(
    plain(
      await bridge.handleRequest("POST", "/notes/export", {
        itemKey: "NOTE1",
      }),
    ),
    { content: "markdown" },
  );
  assert.deepEqual(
    plain(
      await bridge.handleRequest("POST", "/notes/export", {
        itemKey: "NOTE1",
        format: "html",
      }),
    ),
    { content: "<p>html</p>" },
  );
  assert.equal(calls.note2md, 1);
  assert.equal(calls.note2html, 1);
});

test("rejects unsupported export format before item lookup", async () => {
  const { calls, bridge } = loadBridge();

  await rejects(/Unsupported note export format/, () =>
    bridge.handleRequest("POST", "/notes/export", {
      itemKey: "NOTE1",
      format: "pdf",
    }),
  );

  assert.equal(calls.itemLookup, 0);
});

test("rejects oversized markdown before creating note", async () => {
  const { calls, bridge } = loadBridge();

  await rejects(/markdown/, () =>
    bridge.handleRequest("POST", "/notes/from-markdown", {
      markdown: "a".repeat(2 * 1024 * 1024 + 1),
    }),
  );

  assert.equal(calls.itemConstructed, 0);
});

test("creates note from markdown under parent item", async () => {
  const { calls, bridge, createdNotes } = loadBridge({
    md2noteResult: "parsed",
    noteStatus: { meta: "<div>", tail: "</div>" },
  });

  const result = await bridge.handleRequest("POST", "/notes/from-markdown", {
    parentKey: "NOTE1",
    markdown: "# note",
  });

  assert.deepEqual(plain(result), { itemKey: "NEWNOTE" });
  assert.equal(calls.itemConstructed, 1);
  assert.equal(calls.saveTx, 2);
  assert.equal(calls.eraseTx, 0);
  assert.equal(calls.md2note, 1);
  assert.equal(createdNotes[0].parentID, 1);
  assert.equal(createdNotes[0].libraryID, 1);
  assert.deepEqual(createdNotes[0].notes, ["", "<div>parsed</div>"]);
});

test("erases created note when markdown conversion fails", async () => {
  const { calls, bridge, createdNotes } = loadBridge({
    md2noteThrows: true,
  });

  await rejects(/md2note failed/, () =>
    bridge.handleRequest("POST", "/notes/from-markdown", {
      markdown: "# note",
    }),
  );

  assert.equal(calls.saveTx, 1);
  assert.equal(calls.eraseTx, 1);
  assert.equal(createdNotes[0].erased, true);
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

test("rejects missing items", async () => {
  const { bridge } = loadBridge({ missingItemKeys: ["MISSING"] });

  await rejects(/Item MISSING not found/, () =>
    bridge.handleRequest("POST", "/templates/run", {
      name: "Export",
      itemKey: "MISSING",
    }),
  );
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

test("returns note relations and tree", async () => {
  const outbound = [{ fromKey: "A", toKey: "B" }];
  const inbound = [{ fromKey: "C", toKey: "A" }];
  const tree = { name: "root" };
  const { calls, bridge } = loadBridge({
    inboundRelations: inbound,
    noteTree: tree,
    outboundRelations: outbound,
  });

  assert.deepEqual(
    plain(
      await bridge.handleRequest("POST", "/relations/get", {
        itemKey: "NOTE1",
      }),
    ),
    { relations: { outbound, inbound } },
  );
  assert.deepEqual(
    plain(
      await bridge.handleRequest("POST", "/notes/tree", { itemKey: "NOTE1" }),
    ),
    { tree },
  );
  assert.equal(calls.noteTree, 1);
});
