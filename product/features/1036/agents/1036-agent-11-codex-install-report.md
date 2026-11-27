# 1036-agent-11-codex-install — C9 Codex

## Files
- packages/unimatrix/lib/codex-install.js (modified)
- packages/unimatrix/test/codex-install.test.js (extended, 9 tests)

## Changes
- All writes (W3 hooks.json, W4 config.toml MCP, R7 config.toml [hooks], R8 project .codex/hooks.json, R9 project .codex/config.toml) via `updateFileSync`; compute re-run on `res.before` for leg detail.
- R8 passes `containWithin: projectRoot` to `removeOwnedHooks` (C8 must honour it); R9 uses `containWithin`; `isWithinProject` dropped; same-file checks use `sameRealFile`.
- `codexHome` imported from config-dirs.js and re-exported; local copy deleted. Optional `o.dirs` drives paths; with `o.dirs` the CODEX_HOME warning is not re-printed to stderr (still on leg reasons).
- `SafeWriteError` rethrown from `maybeWireCodex` and R8 catch; parse failures stay `skipped-malformed`.
- W4 action semantics preserved (`created` when table absent even if file present).

## Tests
- codex-install 60 pass / 0 fail / 1 skip (#1053 note); codex-toml-surgical 31/0; c14-codex 8/0.
- codex-detect (7) and init-integration fail with `o.fault is not a function` at merge-settings.js:450 — concurrent C8 work in progress, not C9.
- Size gate: OK (hook-client untouched). package.json/lockfile unchanged.

## Issues
- Gotcha: mutate returning "" (table removal empties file) must not be coerced to null — `orNull` helper.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- ADR-002 #5960 (one resolver), #5903 codex-install gotchas, #5774 trust-note leg placement.
- Stored: nothing novel to store -- empty-string mutate trap is generic; deferring to gate review.

Unimatrix entries consulted: #5960, #5903, #5774
