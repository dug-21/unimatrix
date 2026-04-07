# Gate Bugfix Validation Report: GH #536

> Gate: Bug Fix Validation
> Date: 2026-04-07
> Feature: crt-048 (host feature for the fix)
> Issue: GH #536
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | All 3 match sites in `compute_phase_stats`/`categorize_tool_for_phase` now call `normalize_tool_name` before matching bare names |
| No placeholders or stubs | PASS | No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in any changed file |
| All tests pass | PASS | 4,357 unit tests, 0 failures across all suites |
| No new clippy warnings in changed files | PASS | Clippy errors are pre-existing in other files; no errors in `session_metrics.rs`, `tools.rs`, `lib.rs`, or `retrospective.rs` |
| No unsafe code introduced | PASS | `#![forbid(unsafe_code)]` in observe crate; no unsafe in changed files |
| Fix is minimal | PASS | Changes confined to 4 files; no unrelated scope |
| New test catches original bug | PASS | `test_phase_stats_mcp_prefix_normalized_correctly` uses production-prefix names and asserts non-zero counts that would have been 0 before the fix |
| `make_mcp_obs_at` is correct | PASS | Delegates to `make_obs_at` with `format!("mcp__unimatrix__{tool}")` — no silent wrong output possible |
| All 3 match sites confirmed | PASS | `categorize_tool_for_phase` + `knowledge_served` filter + `knowledge_stored` filter — all normalize before matching |
| No 4th site missed | PASS | Grep across tools.rs confirms no bare `context_*` string comparison outside normalized contexts |
| 5 existing test call sites updated | PASS | All `make_obs_at("...", "context_*")` in phase_stats tests replaced with `make_mcp_obs_at` |
| Label change ("Total served" → "Distinct entries served") | PASS | `retrospective.rs` line 1004 updated; 2 test assertions updated to match |
| Knowledge stewardship — rust-dev | PASS | `536-agent-1-fix-report.md` has `Queried:` (entries #4203, #918) and `Stored:` (entry #4204) |
| Knowledge stewardship — tester | WARN | `crt-048-agent-7-tester-report.md` is the crt-048 feature tester, not the bug-specific tester; GH #536 did not spawn a separate tester agent — coverage verified by rust-dev test results and gate validator |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

The root cause was three filter sites in `compute_phase_stats` and `categorize_tool_for_phase` that matched `ObservationRecord.tool` directly against bare names like `"context_search"` without first stripping the MCP prefix. Production hook records carry `"mcp__unimatrix__context_search"`, causing all three counts to always return zero.

Fix correctly:
1. `categorize_tool_for_phase` (tools.rs:3426) — calls `unimatrix_observe::normalize_tool_name` before the match
2. `knowledge_served` filter (tools.rs:3619) — calls `unimatrix_observe::normalize_tool_name` before the `matches!` guard
3. `knowledge_stored` filter (tools.rs:3634) — calls `unimatrix_observe::normalize_tool_name` before the equality check

`normalize_tool_name` in `session_metrics.rs` is the single canonical implementation:
```rust
pub fn normalize_tool_name(tool: &str) -> &str {
    tool.strip_prefix("mcp__unimatrix__").unwrap_or(tool)
}
```
It is now `pub` and re-exported from `unimatrix_observe::lib.rs`.

### No Placeholders or Stubs

**Status**: PASS

Grep across all four changed files found no `todo!()`, `unimplemented!()`, `TODO`, or `FIXME`.

### All Tests Pass

**Status**: PASS

Full workspace test run: 0 failures across all suites. Representative counts:
- `unimatrix-server` (lib): 2,824 passed
- `unimatrix-observe` (lib): 423 passed
- All integration suites: 0 failures

### No New Clippy Warnings in Changed Files

**Status**: PASS

Clippy errors exist in pre-existing files (`unimatrix-engine/src/auth.rs`, `unimatrix-observe/src/detection/`, etc.) but none in the changed files (`session_metrics.rs`, `tools.rs`, `lib.rs`, `retrospective.rs`). These are pre-existing and not introduced by this fix.

### Fix Is Minimal

**Status**: PASS

Changes confined to:
- `session_metrics.rs` — `pub fn` promotion + 3 `is_some_and` (clippy fix)
- `lib.rs` — one re-export line added
- `tools.rs` — 3 normalization call sites + `make_mcp_obs_at` helper + 5 test call site updates + 1 new test
- `retrospective.rs` — label change + 2 test assertions

No unrelated changes present.

### New Test Catches Original Bug

**Status**: PASS

`test_phase_stats_mcp_prefix_normalized_correctly` (tools.rs:5800) sends `mcp__unimatrix__context_search`, `mcp__unimatrix__context_lookup`, `mcp__unimatrix__context_get`, and `mcp__unimatrix__context_store` through `compute_phase_stats` and asserts:
- `knowledge_served == 3` (would have been 0 before fix)
- `knowledge_stored == 1` (would have been 0 before fix)
- `tool_distribution.search == 3` (would have been 0 before fix)

This test would have failed against the pre-fix code.

### `make_mcp_obs_at` Correctness

**Status**: PASS

```rust
fn make_mcp_obs_at(session_id: &str, ts_ms: u64, tool: &str) -> ObservationRecord {
    make_obs_at(session_id, ts_ms, &format!("mcp__unimatrix__{tool}"))
}
```

The helper is a thin delegation — it cannot produce wrong output silently. If `tool` is empty, the prefix is emitted alone, which `normalize_tool_name` would strip to `""`, correctly falling through to "other". If `tool` is already prefixed (caller error), it would produce a double prefix, which is also handled by `normalize_tool_name` (strip once, leaving the inner prefix, which wouldn't match any bare name — a test failure would surface this). No silent corruption path exists.

### All 3 Match Sites Confirmed, No 4th Site Missed

**Status**: PASS

Grep of tools.rs for `== "context_search"`, `== "context_lookup"`, `== "context_get"`, `== "context_store"` finds only the two filter chains inside `compute_phase_stats` (both normalized). The `categorize_tool_for_phase` match arm uses `matches!()` after normalization. No bare unprotected string comparisons remain in production code.

### Knowledge Stewardship — rust-dev

**Status**: PASS

`536-agent-1-fix-report.md`:
- `Queried:` entries #4203 and #918 consulted before implementing
- `Stored:` entry #4204 "Use normalize_tool_name from unimatrix-observe for all MCP tool-name match sites" — captures the pattern and the test-helper trap for future agents

### Knowledge Stewardship — tester

**Status**: WARN

GH #536 bugfix protocol did not spawn a separate tester agent report. The rust-dev agent included test results in their report, and this gate validator ran the full suite independently. No coverage gap exists — this is a process note only, not a quality issue.

## Rework Required

None.

## Knowledge Stewardship

- Queried: no query needed — gate validation of a targeted bug fix on known files.
- Stored: nothing novel to store -- gate-specific results belong in gate reports only; the pattern (normalize before match; bare-name test trap) was already stored by the rust-dev agent as entry #4204.
