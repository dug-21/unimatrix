# Report: nan-024-agent-3-resolve-runtime

Unimatrix entries consulted: #5900 (ADR-002), #5122, #1194, #1198

## Report
- Files: packages/unimatrix/lib/resolve-runtime.js (new, 200 lines), lib/resolve-binary.js (thin wrapper, PLATFORMS retained), test/resolve-runtime.test.js (new, 23 tests), test/resolve-binary.test.js (updated, 9 tests, count kept).
- Tests: 32 pass / 0 fail (explicit `node --test` on both files).
- Runner: top-level test/*.test.js is auto-discovered by the `package` set; no runner edit needed.
- Wrapper behaviour changes: win32/darwin return null (no throw); fake binaries in old tests now chmod 755 (X_OK validation); tests isolate HOME.
- Added optional `arch` param to resolveRuntime (default process.arch) for testability.
- Out of scope here: `effectiveEnv` tests (live in project-state, other agent). Callers of resolveBinary() (init.js:663, bin/unimatrix.js:252) must now handle null on macOS/Windows; not edited.
- No dependencies added; no commit.

## Knowledge Stewardship
- Queried: context_briefing, context_get #5900 -- ADR-002 precedence/validation rules applied.
- Stored: nothing novel to store -- implementation followed pseudocode.
