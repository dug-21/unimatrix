# C16 — Removal (`lib/atomic-write.js` and private writers)

## Purpose
Single writer (D-010, FR-20). Runs last (Wave 4), after all consumers migrated and ported tests green (FR-33).

## Steps
1. Confirm no `require("./atomic-write.js")` remains (claude-json, opencode-install were the users).
2. Delete `lib/atomic-write.js`.
3. Delete `claude-json.atomicWriteText` export; fix any importer (merge-settings, codex-install already migrated).
4. Delete install-store `atomicWrite`, `tmpName`, local `renameWithRetry`/`sleepMs`.
5. Delete `recordTrustIdentity` private tmp+rename body (already replaced in C2).
6. Delete `wire.gitIgnored`, `wire.GITIGNORE_WARNING`, `status.codexHome`.
7. Move/retire tests that imported deleted symbols (e.g. any `atomic-write` unit test → its cases become C3 tests).

## Enforcement tests (new, in existing test dir)
- **FR-20 grep test**: scan `lib/**/*.js` excluding `trust.js`, `safe-write.js`; fail on `renameSync`, `writeFileSync`, `copyFileSync`, `openSync(..."w` occurrences, with an allowlist: install-store `copyPackage` dir rename + `.complete` marker open, hook-time runtime-state writers under `lib/hook-client/` (last-hook, queue, latches) per D-020. Allowlist entries are `file:function` pairs, reviewed in PR.
- **FR-01 grep test**: no lib file other than `config-dirs.js` contains a join of `".claude.json"`, `".config"`+`"opencode"`, or `".codex"` with a home variable.
- `npm pack --dry-run` file list no longer contains `lib/atomic-write.js`; `check-zero-deps.js` passes.
