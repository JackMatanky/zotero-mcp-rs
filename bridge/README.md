# Zotero companion bridge

`zotero-companion-bridge.js` is the script Zotero loads. It is generated from
`src/zotero-companion-bridge.ts`; do not edit the generated `.js` by hand.

The bridge exposes Zotero in-process APIs to the Rust MCP server through the
existing Zotero HTTP server at `http://127.0.0.1:23119/better-notes/*` (or the
base URL configured with `BETTER_NOTES_URL`).

## Why this is a Zotero-loaded script

Zotero extension APIs only exist inside Zotero's Firefox/Gecko runtime. Better
Notes also exposes `Zotero.BetterNotes.api` only as an in-process JavaScript
object. Rust cannot call those APIs directly through FFI or a socket, so this
bridge must run inside Zotero and register a handler on the `Zotero` global.

## Files

|Path|Purpose|
|---|---|
|`src/zotero-companion-bridge.ts`|Source for the Zotero-loaded bridge script. Registers `Zotero.BetterNotesBridge.handleRequest`.|
|`src/better-notes-api.d.ts`|Hand-written ambient types for the small slice of `Zotero.BetterNotes.api` used by the bridge. Better Notes does not publish its own type package.|
|`zotero-companion-bridge.js`|Generated build artifact loaded into Zotero. Commit it after rebuilding so users do not need TypeScript to deploy the bridge.|
|`test/zotero-companion-bridge-fixture.mjs`|VM-based Zotero/Better Notes fixture used by all bridge tests.|
|`test/better-notes-handlers.test.mjs`|Behavior tests for status, dispatch, Better Notes handlers, note creation cleanup, relations, and trees.|
|`test/zotero-companion-file-roots.test.mjs`|Behavior tests for Zotero storage, linked attachment base, and Attanger root reporting.|
|`tsconfig.json`|Compiles `src/*.ts` into package-root `.js` files because Zotero loads plain scripts, not bundled modules.|

## Endpoints

All endpoint paths below are relative to the Rust `better_notes_url` base
(default: `http://127.0.0.1:23119/better-notes`). The Rust server posts JSON to
these paths and returns the JSON response to MCP tool handlers.

|Path|Requires Better Notes|Request|Response|Purpose|
|---|---:|---|---|---|
|`/file-roots`|No|`{}`|`{ roots: Array<{ kind, path }> }`|Reports Zotero storage, linked attachment base, and Attanger destination roots for PDF read policy. `kind` is `zotero-storage`, `zotero-linked-base`, or `attanger-dest`. Empty or non-string paths are omitted.|
|`/status`|No|`{}`|`{ online: true, ready: boolean }`|Reports that the bridge is loaded and whether `Zotero.BetterNotes.api` is currently available.|
|`/notes/export`|Yes|`{ itemKey, format? }`|`{ content }`|Exports an existing note as Markdown by default, or HTML when `format` is `html`.|
|`/notes/to-markdown`|Yes|`{ html }`|`{ markdown }`|Converts raw HTML to Markdown through Better Notes.|
|`/notes/from-markdown`|Yes|`{ markdown, parentKey? }`|`{ itemKey }`|Creates a Zotero note from Markdown, optionally under a parent item. If conversion fails after the empty note is saved, the bridge erases the created note.|
|`/templates/run`|Yes|`{ name, itemKey }`|`{ result }`|Runs a Better Notes item template for one Zotero item.|
|`/relations/get`|Yes|`{ itemKey }`|`{ relations: { outbound, inbound } }`|Returns Better Notes note-link relations for one note.|
|`/notes/tree`|Yes|`{ itemKey }`|`{ tree }`|Returns Better Notes' heading/link tree for one note.|

`handleRequest` accepts only `POST`. Unknown paths and unsupported methods throw
plain `Error`s; the Rust side converts those failures into MCP error content.

## File-root behavior

`/file-roots` intentionally does not require Better Notes. It reads only Zotero
and Attanger state:

- `zotero-storage`: `Zotero.getStorageDirectory().path`.
- `zotero-linked-base`: Zotero pref `baseAttachmentPath`.
- `attanger-dest`: Attanger pref `extensions.zotero.zoteroattanger.destDir`,
  only when `extensions.zotero.zoteroattanger.attachType` is `linking` and
  `extensions.zotero.zoteroattanger.enable` is not explicitly `false`.

Each root is optional. A failing storage or pref read omits that root instead of
failing the whole endpoint.

## Build and test

From `bridge/`:

```sh
npm install
npm run build
npm test
```

From the repository root, typecheck through the project task runner:

```sh
MISE_EXPERIMENTAL=1 mise run bridge:typecheck
```

`npm test` runs `npm run build` first, then executes the VM tests against the
generated `zotero-companion-bridge.js`. If the generated file changes, commit it
with the TypeScript source.

## Deploying

Load `zotero-companion-bridge.js` into a running Zotero via Developer Utilities
(Tools -> Developer -> Run JavaScript) or a Zotero startup script. The script is
idempotent: if `Zotero.BetterNotesBridge` already exists, it leaves the existing
handler in place.
