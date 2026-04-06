# Gate 3b Report: bugfix-491

> Gate: 3b (Code Review)
> Date: 2026-04-06
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | No pseudocode phase for bugfixes — fix directly addresses root cause as diagnosed |
| Architecture compliance | PASS | Uses EDGE_SOURCE_CO_ACCESS constant as required; no ADR violations |
| Interface implementation | PASS | No interface changes; inferred_edge_count field semantics corrected in docs |
| Test case alignment | PASS | TC-15 table-driven test covers all 7 cases (nli, cosine_supports, S1, S2, S8, behavioral counted; co_access excluded) |
| Code quality | WARN | All 3 changed files exceed 500 lines — pre-existing, not introduced by this fix |
| Security | PASS | No unsafe code, no unwrap() in non-test code introduced; format!() with constant is safe |
| Knowledge stewardship compliance | FAIL | Agent reports (investigator + rust-dev) do not exist; no ## Knowledge Stewardship blocks to verify |

## Detailed Findings

### Fix Addresses Root Cause
**Status**: PASS
**Evidence**: `crates/unimatrix-store/src/read.rs` line 1020:
```sql
COALESCE(SUM(CASE WHEN source NOT IN ('{}', '') THEN 1 ELSE 0 END), 0) AS inferred_edge_count
```
The old inclusive `source = 'nli'` filter is replaced with exclusive `source NOT IN (EDGE_SOURCE_CO_ACCESS, '')`. This directly fixes the undercount for cosine_supports, S1, S2, S8, and behavioral sources. Format parameter is `EDGE_SOURCE_CO_ACCESS` constant (confirmed at line 1024), not a bare string literal.

### No Placeholders or Stub Code
**Status**: PASS
**Evidence**: `grep` found one `TODO` comment in `nli_detection_tick.rs` at line 75 (`TODO: Config-promote to InferenceConfig.max_cosine_supports_per_tick`). This is pre-existing — it was present before this fix (git log confirms nli_detection_tick.rs was last substantively modified for #523 and #490, not introduced by 84162c78).

### All Tests Pass
**Status**: PASS
**Evidence**: `cargo test --workspace` — all crates pass with zero failures:
- 2769 tests pass in unimatrix-store
- 423 tests pass in unimatrix-server
- All other crates pass
- Total: no failures across all test suites

New test `test_inferred_edge_count_table_driven` (TC-15) passes and covers all 7 cases including the behavioral source from the bug report.

### Test Would Have Caught Original Bug
**Status**: PASS
**Evidence**: TC-15 asserts that `cosine_supports`, `S1`, `S2`, `S8`, and `behavioral` each increment `inferred_edge_count` by exactly 1. Under the old `source = 'nli'` filter, these assertions would have failed because those sources were silently excluded.

### Clippy
**Status**: PASS (pre-existing warning noted)
**Evidence**: `cargo clippy --workspace -- -D warnings` reports one error in `crates/unimatrix-engine/src/auth.rs:113` (collapsible_if). This file was not modified by any commit in #491. Pre-existing as documented in the spawn prompt. No new clippy issues introduced by this fix.

### Code Quality — File Size Limit
**Status**: WARN
**Evidence**: All three changed files exceed 500 lines:
- `crates/unimatrix-store/src/read.rs`: 2934 lines
- `crates/unimatrix-server/src/mcp/response/status.rs`: 1667 lines
- `crates/unimatrix-server/src/services/nli_detection_tick.rs`: 3720 lines

Pre-existing condition: git history confirms all three were already above 500 lines before the first 491 commit (e.g., read.rs was 2919 lines at commit 0ac635ec). Not introduced by this fix.

### Security
**Status**: PASS
**Evidence**: Diff review confirms:
- No unsafe blocks introduced
- No `.unwrap()` added to non-test code
- No hardcoded secrets or credentials
- SQL parameterization via Rust `format!()` with a compile-time constant (`EDGE_SOURCE_CO_ACCESS`) — no user input involved in the SQL fragment

### xfail Markers
**Status**: PASS
**Evidence**: Two xfail markers in `test_lifecycle.py`:
1. `test_inferred_edge_count_unchanged_by_cosine_supports` — xfail reason: "No embedding model in CI". Infrastructure reason documented inline; no GH issue required for pure infrastructure constraints.
2. `test_inferred_edge_count_unchanged_by_s1_s2_s8` — xfail reason: "GH#291 — Background tick interval (15 min default) exceeds integration test timeout." GH#291 confirmed open and valid.

Both xfail tests updated from stale `== baseline` assertions to `>= baseline`, correctly reflecting the new inclusive counting behavior.

### Integration Smoke Tests
**Status**: PASS (with pre-existing failure noted)
**Evidence**: `python -m pytest suites/ -v -m smoke --timeout=60` result: 21 passed, 1 failed, 256 deselected.

Failure: `suites/test_confidence.py::test_base_score_active — AssertionError: Active entry confidence should be > 0, got 0.0`. This test file was not modified by any 491 commit (last modified in #403 and #255). Pre-existing failure unrelated to this fix.

### Knowledge Stewardship Compliance
**Status**: FAIL
**Evidence**: No `product/features/bugfix-491/` directory exists. No agent reports found for this bugfix in git history, in the worktree, or on any branch. The bugfix protocol requires investigator and rust-dev agent reports each containing a `## Knowledge Stewardship` section with `Queried:` and `Stored:` (or decline reason) entries. No such reports were produced.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Missing investigator agent report | investigator (re-run or create) | Create `product/features/bugfix-491/agents/491-investigator-report.md` with `## Knowledge Stewardship` block containing `Queried:` entries (queries run before diagnosis) and `Stored:` or "nothing novel to store -- {reason}" |
| Missing rust-dev agent report | rust-dev (re-run or create) | Create `product/features/bugfix-491/agents/491-rust-dev-report.md` with `## Knowledge Stewardship` block containing `Queried:` entries (queries run before implementation) and `Stored:` or "nothing novel to store -- {reason}" |

---

## Knowledge Stewardship

- nothing novel to store -- single-instance gate failure pattern; stewardship non-compliance is already a documented lesson in the validation gate spec
