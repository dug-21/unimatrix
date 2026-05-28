# Agent Report: nxs-013-agent-3-C3-provenance-labels

## Task

Update log message strings in `log_config_provenance` (main.rs) to clarify config hierarchy: per-project is "primary", global is "defaults".

## Files Modified

- `crates/unimatrix-server/src/main.rs` (lines 1350, 1353, 1359, 1362 -- string literals only)

## Changes

Four string literal replacements inside tracing macros:

| Old | New |
|-----|-----|
| `"global config loaded"` | `"defaults config loaded (global)"` |
| `"global config not found; using compiled defaults"` | `"defaults config not found (global); using compiled defaults"` |
| `"project config loaded"` | `"primary config loaded (per-project)"` |
| `"project config not found; using compiled defaults"` | `"primary config not found (per-project); write default with 'unimatrix config'"` |

No changes to: match arms, log levels, control flow, env_override branch, function signature, or any other function.

## Test Results

- `cargo build --workspace`: PASS (0 errors, 21 pre-existing warnings)
- `cargo test --workspace`: 3302 passed, 4 failed (pre-existing `col018_*` failures unrelated to this change)
- Provenance tests in config.rs: assert on `SourceStatus` types, not log strings -- unaffected

## Self-Check

- [x] `cargo build --workspace` passes
- [x] `cargo test --workspace` -- no new failures
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK`
- [x] All modified files within scope
- [x] Code follows validated pseudocode -- exact string replacements as specified
- [x] Test cases match component test plan (existing tests pass, no new unit tests per test plan)
- [x] No source file exceeds 500 lines

## Issues

None.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-002 (no provenance summary line), ADR-001 (remove UNIMATRIX_CONFIG from Dockerfile), and pattern #4592 (dsn-001 startup config injection). Applied ADR-002 constraint (no summary line added).
- Stored: nothing novel to store -- this was a straightforward 4-line string literal replacement with no gotchas or non-obvious patterns discovered.
