# Test Plan: hook-ownership (`lib/hook-ownership.js`)  [Critical R-03]

File: `test/hook-ownership.test.js` (table-driven, runs on all OS, no guards).
- Positive table (one case per `OWNED_FORMS` arm and variant): L1 Rust string, L2 abs + `LD_LIBRARY_PATH`, L3 abs node, L4 npx-cache, L5 npx, L6 prior shim form, `$HOME`/`~` shim, abs shim single/double quoted, `/` and `\`, `.cmd`, path with space, `--provider codex-cli`, every KnownEvent (iterate the exported list; a newly-added event fails the test if not matched).
- Negative: `my-unimatrix-hook.sh`, `echo unimatrix hook`, `unimatrix-backup hook X`, `unimatrix hook` with unknown event, `hook-client/index.js` substring in unrelated path, empty string, non-string.
- Property: for all e in events, p in {undefined,'codex-cli'}, platform in {linux,darwin,win32}, home in {plain, space, non-ASCII, deep}: `isUnimatrixCommand(buildShimCommand(e,p,platform))`.
- `isUnimatrixMcpEntry(name, entry)`: owned shim forms (sh -c, cmd, D-047 fallback node+shim-entry.js), foreign command named `unimatrix` -> false.
- Every arm requires `hook\s+<KnownEvent>` or `hook-client/index.js` adjacency (regex assertion).
