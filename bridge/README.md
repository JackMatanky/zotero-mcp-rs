# Better Notes bridge

`zotero-better-notes-bridge.js` is what Zotero actually loads (via Developer
Utilities or a startup script). It is a **build artifact**, generated from
the TypeScript source in `src/` -- do not hand-edit it.

## Why this is TypeScript compiled to a plain script, not Rust

`Zotero.BetterNotes.api` is an in-process JavaScript object that only exists
inside Zotero's own Firefox/Gecko extension runtime. Better Notes, unlike
Better BibTeX, exposes no HTTP or RPC endpoint of its own -- the only way to
reach it is to run JS inside the same process, sharing Zotero's live
`Zotero.Items`, Web Worker, and note-storage globals. There is no FFI or
socket boundary a separate Rust binary could call instead, so this bridge
must ship as a script Zotero loads directly, with the Rust MCP server
(`src/better_notes/client.rs`) talking to it over loopback HTTP.

## Build

```sh
npm install
npm run build
```

Regenerates `zotero-better-notes-bridge.js` from `src/zotero-better-notes-bridge.ts`
via `tsc` (typed against `zotero-types`, the same ambient `Zotero` typings
Better Notes' own source uses) plus `src/better-notes-api.d.ts`, a
hand-written ambient interface for the slice of `Zotero.BetterNotes.api`
this bridge calls (Better Notes ships no types of its own).

## Deploying

Load the generated `zotero-better-notes-bridge.js` into a running Zotero via
Developer Utilities (Tools -> Developer -> Run JavaScript) or a Zotero
startup script. It registers `Zotero.BetterNotesBridge.handleRequest`,
which the Zotero MCP server calls over `http://127.0.0.1:23119/better-notes/*`.
