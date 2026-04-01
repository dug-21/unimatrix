# Gate Bug Fix Report: bugfix-476

> Gate: Bug Fix Validation
> Date: 2026-04-01
> Result: PASS (with WARN)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | SQL JOIN fix targets the exact missing quarantine filter on both endpoints |
| No stubs / placeholders | PASS | No todo!(), unimplemented!(), TODO, FIXME in changed files |
| All tests pass | PASS | 4264 unit tests pass, 0 failed; 22/22 smoke; 41/41 lifecycle non-xfail |
| No new clippy warnings | PASS | Zero errors in changed files; pre-existing errors in other files confirmed pre-existing |
| No unsafe code | PASS | No unsafe blocks in any changed file |
| Fix is minimal | PASS | 3 files: SQL fix + bind + comment in main module; 3 new tests; 1 clarifying comment in analytics.rs |
| New tests catch original bug | PASS | All 3 quarantine tests would have failed against the pre-fix SQL |
| Integration smoke tests pass | PASS | 22/22 passed |
| xfail markers have GH issues | PASS | Existing xfail markers reference GH#406 and tick-timing issues; no new markers added |
| Bug-discovering integration test xfail removed | N/A | Bug discovered via observation, not an xfail integration test |
| Investigator report has Knowledge Stewardship | WARN | Investigator agent-1 report absent from product/features/bugfix-476/agents/ |
| Rust-dev report has Knowledge Stewardship | WARN | Rust-dev agent report absent from product/features/bugfix-476/agents/ |
| Verifier report has Knowledge Stewardship | PASS | 476-agent-2-verify-report.md has Queried/Stored entries |

---

## Detailed Findings

### Root Cause Addressed
**Status**: PASS

**Evidence**: The diagnosed root cause was a missing quarantine-exclusion JOIN in the batch SELECT of `run_co_access_promotion_tick`. The fix adds:
- `JOIN entries ea ON ea.id = ca.entry_id_a AND ea.status != ?3` and `JOIN entries eb ON eb.id = ca.entry_id_b AND eb.status != ?3` to the outer SELECT
- Identical JOINs on `ea2`/`eb2` in the scalar subquery for `max_count` normalization
- A third `.bind(Status::Quarantined as u8 as i64)` for `?3`

The pre-fix SQL had no JOIN against `entries` at all. The fix is structurally correct: INNER JOIN drops rows when either endpoint has `status = 3`, stopping both promotion and the delete/re-insert cycle. The `test_quarantine_mixed_batch_only_active_pairs_promoted_with_correct_weight` test explicitly validates the subquery filter (weight=1.0 not 0.5 when quarantined high-count pair is excluded from max).

### No Stubs / Placeholders
**Status**: PASS

No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` found in `co_access_promotion_tick.rs` or `analytics.rs`.

### All Tests Pass
**Status**: PASS

All three new regression tests pass:
- `test_quarantine_one_endpoint_no_edges_promoted`
- `test_quarantine_both_endpoints_no_edges_promoted`
- `test_quarantine_mixed_batch_only_active_pairs_promoted_with_correct_weight`

Full workspace: 4264 passed, 0 failed (independently verified in this run).

### No New Clippy Warnings
**Status**: PASS

Running `cargo clippy --package unimatrix-server --package unimatrix-store --no-deps -- -D warnings` produces errors only in files not touched by this fix (confirmed by cross-referencing error file paths against the diff). Zero clippy issues in `co_access_promotion_tick.rs`, `co_access_promotion_tick_tests.rs`, or `analytics.rs`.

Pre-existing clippy debt exists in `unimatrix-server` (confirmed present before the fix at commit `d10dbd0`). Not introduced by this PR.

### No Unsafe Code
**Status**: PASS

No `unsafe` keyword present in any changed file.

### Fix is Minimal
**Status**: PASS

The diff is exactly scoped to the root cause:
- `co_access_promotion_tick.rs`: +25/-8 lines (SQL rewrite + `Status` import + clarifying comments)
- `co_access_promotion_tick_tests.rs`: +136 lines (3 new tests in Group J + `seed_entry` guard in `seed_co_access`)
- `analytics.rs`: +8 lines (clarifying comment only, no logic change)

No unrelated changes present.

### New Tests Would Have Caught Original Bug
**Status**: PASS

- `test_quarantine_one_endpoint_no_edges_promoted`: Seeds a quarantined endpoint, asserts 0 edges promoted. Against the pre-fix SQL (no JOIN), the pair would have been selected and edges inserted, asserting `count == 0` would have failed.
- `test_quarantine_both_endpoints_no_edges_promoted`: Same pattern, both quarantined. Would have failed pre-fix.
- `test_quarantine_mixed_batch_only_active_pairs_promoted_with_correct_weight`: Asserts `weight == 1.0`. Pre-fix, `max_count` from the subquery would include the quarantined pair's count=10, producing weight=0.5. The `(edge.weight - 1.0).abs() < 1e-9` assertion would have failed.

### Integration Smoke Tests Passed
**Status**: PASS

22/22 smoke tests passed per the verifier report.

### xfail Markers
**Status**: PASS / N/A

No new xfail markers were added in this PR. One pre-existing XPASS noted (`test_search_multihop_injects_terminal_active` / GH#406) — pre-existing since bugfix-434, not caused by this fix. Marker cleanup is tracked separately.

### Knowledge Stewardship — Investigator and Rust-dev Reports
**Status**: WARN

Only one agent report exists in `product/features/bugfix-476/agents/`: `476-agent-2-verify-report.md`. No investigator (agent-1) or rust-dev agent reports were written to disk. The stewardship requirement cannot be verified for those agents.

The verifier report (`476-agent-2-verify-report.md`) does contain a complete `## Knowledge Stewardship` block with `Queried:` and `Stored:` entries.

This is a process gap (reports not persisted), not a code correctness issue. The fix itself demonstrates the investigator correctly diagnosed the root cause (the SQL change targets exactly the described problem).

---

## Rework Required

None. The code fix is correct and complete.

---

## Warnings (Non-blocking)

| Issue | Severity | Notes |
|-------|----------|-------|
| Investigator agent-1 report absent | WARN | Cannot verify knowledge stewardship for the investigator. Process gap only. |
| Rust-dev agent report absent | WARN | Cannot verify knowledge stewardship for the rust-dev agent. Process gap only. |
| `test_search_multihop_injects_terminal_active` XPASS | WARN | Pre-existing GH#406 marker drift. Cleanup tracked separately, not caused by this fix. |

---

## Knowledge Stewardship

- Stored: nothing novel to store — the quarantine JOIN pattern is a single-feature-specific fix; the general "filter quarantined entries in SQL by JOINing on entries.status" is already documented in Unimatrix conventions. No recurring gate failure pattern observed.
