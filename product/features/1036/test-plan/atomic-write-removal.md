# Test Plan — C16 removed (`lib/atomic-write.js`)

Risks: R-10, R-06, AC-10 grep.

- File-check: `lib/atomic-write.js` does not exist; no `require(".*atomic-write")` in lib/test.
- Grep S-5: none of `isUnimatrixOpencodePlugin`, `DanglingSymlinkError`, `opencodeMcp`, `otherLocations`, `atomicWriteText`, `GITIGNORE_WARNING`, `gitIgnored`, `removeOwnedClaudeLegacy`, `isWithinProject` in `lib`/`test`.
- Grep AC-10: no `.pid`-suffixed temp names; no `renameSync`/`writeFileSync`/`copyFileSync` on FR-20 paths outside `trust.js` core and `safe-write.js` (package-dir `renameWithRetry` exempt).
- Tests that previously imported `atomicWriteText` are rewritten to the writer, not deleted (`check-test-count.js`).
