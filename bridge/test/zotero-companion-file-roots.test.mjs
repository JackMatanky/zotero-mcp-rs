/**
 * Behavior coverage for `/file-roots`, the bridge endpoint that must work even
 * when Better Notes is not installed.
 */
import assert from "node:assert/strict";
import test from "node:test";

import { loadBridge, plain } from "./zotero-companion-bridge-fixture.mjs";

test("registers file roots without Better Notes", async () => {
  const { bridge } = loadBridge({
    withBetterNotes: false,
    prefs: {
      baseAttachmentPath: "/Users/alice/Linked Attachments",
      "extensions.zotero.zoteroattanger.enable": true,
      "extensions.zotero.zoteroattanger.attachType": "linking",
      "extensions.zotero.zoteroattanger.destDir": "/Users/alice/Attanger",
    },
  });

  const result = await bridge.handleRequest("POST", "/file-roots", {});

  assert.deepEqual(plain(result), {
    roots: [
      { kind: "zotero-storage", path: "/Users/alice/Zotero/storage" },
      { kind: "zotero-linked-base", path: "/Users/alice/Linked Attachments" },
      { kind: "attanger-dest", path: "/Users/alice/Attanger" },
    ],
  });
});

test("omits Attanger destination when not linking", async () => {
  const { bridge } = loadBridge({
    withBetterNotes: false,
    prefs: {
      baseAttachmentPath: "/Users/alice/Linked Attachments",
      "extensions.zotero.zoteroattanger.enable": true,
      "extensions.zotero.zoteroattanger.attachType": "importing",
      "extensions.zotero.zoteroattanger.destDir": "/Users/alice/Attanger",
    },
  });

  const result = await bridge.handleRequest("POST", "/file-roots", {});

  assert.deepEqual(plain(result), {
    roots: [
      { kind: "zotero-storage", path: "/Users/alice/Zotero/storage" },
      { kind: "zotero-linked-base", path: "/Users/alice/Linked Attachments" },
    ],
  });
});

test("omits Attanger destination when explicitly disabled", async () => {
  const { bridge } = loadBridge({
    withBetterNotes: false,
    prefs: {
      baseAttachmentPath: "/Users/alice/Linked Attachments",
      "extensions.zotero.zoteroattanger.enable": false,
      "extensions.zotero.zoteroattanger.attachType": "linking",
      "extensions.zotero.zoteroattanger.destDir": "/Users/alice/Attanger",
    },
  });

  const result = await bridge.handleRequest("POST", "/file-roots", {});

  assert.deepEqual(plain(result), {
    roots: [
      { kind: "zotero-storage", path: "/Users/alice/Zotero/storage" },
      { kind: "zotero-linked-base", path: "/Users/alice/Linked Attachments" },
    ],
  });
});

test("keeps Attanger destination when enable pref is absent", async () => {
  const { bridge } = loadBridge({
    withBetterNotes: false,
    prefs: {
      baseAttachmentPath: "/Users/alice/Linked Attachments",
      "extensions.zotero.zoteroattanger.attachType": "linking",
      "extensions.zotero.zoteroattanger.destDir": "/Users/alice/Attanger",
    },
  });

  const result = await bridge.handleRequest("POST", "/file-roots", {});

  assert.deepEqual(plain(result), {
    roots: [
      { kind: "zotero-storage", path: "/Users/alice/Zotero/storage" },
      { kind: "zotero-linked-base", path: "/Users/alice/Linked Attachments" },
      { kind: "attanger-dest", path: "/Users/alice/Attanger" },
    ],
  });
});

test("omits empty and non-string paths", async () => {
  const { bridge } = loadBridge({
    withBetterNotes: false,
    storagePath: "",
    prefs: {
      baseAttachmentPath: "",
      "extensions.zotero.zoteroattanger.enable": true,
      "extensions.zotero.zoteroattanger.attachType": "linking",
      "extensions.zotero.zoteroattanger.destDir": "",
    },
  });

  const result = await bridge.handleRequest("POST", "/file-roots", {});

  assert.deepEqual(plain(result), { roots: [] });
});

test("keeps remaining roots when storage and pref reads fail", async () => {
  const { bridge } = loadBridge({
    storageThrows: true,
    throwPrefs: ["extensions.zotero.zoteroattanger.destDir"],
    withBetterNotes: false,
    prefs: {
      baseAttachmentPath: "/Users/alice/Linked Attachments",
      "extensions.zotero.zoteroattanger.enable": true,
      "extensions.zotero.zoteroattanger.attachType": "linking",
      "extensions.zotero.zoteroattanger.destDir": "/Users/alice/Attanger",
    },
  });

  const result = await bridge.handleRequest("POST", "/file-roots", {});

  assert.deepEqual(plain(result), {
    roots: [
      { kind: "zotero-linked-base", path: "/Users/alice/Linked Attachments" },
    ],
  });
});
