# Gate 3b Report: bugfix-491

> Gate: 3b (Code Review) — Rework Round 1
> Date: 2026-04-06
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | Fix directly addresses root cause as diagnosed |
| Architecture compliance | PASS | Uses EDGE_SOURCE_CO_ACCESS constant; no ADR violations |
| Interface implementation | PASS | No interface changes; field semantics corrected in docs |
| Test case alignment | PASS | TC-15 table-driven covers all 7 cases (6 counted + co_access excluded) |
| Code quality | WARN | All 3 changed files exceed 500 lines — pre-existing, not introduced by this fix |
| Security | PASS | No unsafe code, no unwrap() in non-test code introduced |
| Knowledge stewardship compliance | PASS | Both agent reports present with Queried: and Stored: entries |

## Detailed Findings

### Fix Addresses Root Cause
**Status**: PASS
**Evidence**: `crates/unimatrix-store/src/read.rs` line 1020:
```sql
COALESCE(SUM(CASE WHEN source NOT IN ('{}', '') THEN 1 ELSE 0 END), 0) AS inferred_edge_count
```
Old inclusive `source = 'nli'` filter replaced with exclusive `source NOT IN (EDGE_SOURCE_CO_ACCESS, '')`. Format parameter is the `EDGE_SOURCE_CO_ACCESS` constant (line 1024), not a bare string literal.

### No Placeholders or Stub Code
**Status**: PASS
**Evidence**: One pre-existing `TODO` comment in `nli_detection_tick.rs` at line 75 — present before this fix and not introduced by any 491 commit.

### All Tests Pass
**Status**: PASS
**Evidence**: `cargo test --workspace` — all test results `ok`, 0 failed across all crates. New test `test_inferred_edge_count_table_driven` (TC-15) passes and covers all 7 cases.

### Test Would Have Caught Original Bug
**Status**: PASS
**Evidence**: TC-15 asserts cosine_supports, S1, S2, S8, and behavioral each increment `inferred_edge_count` by exactly 1. Under the old `source = 'nli'` filter, these assertions would have failed.

### Clippy
**Status**: PASS (pre-existing warning noted)
**Evidence**: One pre-existing error in `crates/unimatrix-engine/src/auth.rs:113` (collapsible_if). File not modified by any 491 commit. No new clippy issues introduced.

### Code Quality — File Size Limit
**Status**: WARN
**Evidence**: All three changed files exceed 500 lines (read.rs: 2934, status.rs: 1667, nli_detection_tick.rs: 3720). Pre-existing condition confirmed by git history — all were above 500 lines before the first 491 commit.

### Security
**Status**: PASS
**Evidence**: No unsafe blocks, no `.unwrap()` added to non-test code, no hardcoded secrets, SQL uses compile-time constant with no user input in the SQL fragment.

### xfail Markers
**Status**: PASS
**Evidence**: Two xfail markers in `test_lifecycle.py`:
1. `test_inferred_edge_count_unchanged_by_cosine_supports` — reason: "No embedding model in CI" (infrastructure constraint)
2. `test_inferred_edge_count_unchanged_by_s1_s2_s8` — reason: "GH#291 — Background tick interval (15 min default) exceeds integration test timeout"

Both updated from stale `== baseline` to `>= baseline` assertions.

### Integration Smoke Tests
**Status**: PASS (pre-existing unrelated failure noted)
**Evidence**: All smoke tests pass across all suites (test_lifecycle: 4 passed, test_tools: 6 passed, remaining suites: all passed). One pre-existing failure in `test_confidence.py::test_base_score_active` is unrelated to this fix (last modified in #403/#255).

### Knowledge Stewardship Compliance
**Status**: PASS
**Evidence**:

**491-investigator-report.md** (`product/features/bugfix-491/agents/491-investigator-report.md`):
- `Queried:` entries present: `context_briefing` (returned entries #3591, #4027, #4056, #4063, #4046) and `context_search` ("per-source edge count breakdown")
- `Stored:` entry present: Entry #4167 "Inclusive single-source SQL filter silently undercounts when new EDGE_SOURCE_* constants are added"

**491-rust-dev-report.md** (`product/features/bugfix-491/agents/491-rust-dev-report.md`):
- `Queried:` entries present: `context_briefing` (returned entries #4167, #3591, #4056, #4046) — used to confirm constant-over-literal requirement and TC-15 scope
- `Stored:` entry present: Entry #4168 "Reusable pattern for SQL exclusive-filter approach (`source NOT IN (EDGE_SOURCE_CO_ACCESS, '')`)"

Both reports contain full `## Knowledge Stewardship` blocks with evidence of pre-implementation query and post-implementation storage.

---

## Knowledge Stewardship

- nothing novel to store -- stewardship compliance failure recovered via rework; no new recurring pattern beyond what is already captured in gate spec
