# Gate 3b Report: bugfix-528

> Gate: 3b (Code Review — Bug Fix Validation)
> Date: 2026-04-06
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed (all four JOINs flipped) | PASS | All four conditions confirmed allowlist in source |
| Bind changed to Status::Active | PASS | Line 251: `.bind(Status::Active as u8 as i64)` |
| Comments updated (lines 215-224) | PASS | Updated with allowlist semantics and NULL failure mode |
| New test: deprecated pair with higher count | PASS | test_deprecated_endpoint_pair_not_promoted — count=10 deprecated vs. count=5 active |
| typed_graph.rs comment documents no-edges-post-compaction | PASS | Lines 95-109 explain intent and warn against removing the filter |
| No todo!/unimplemented!/TODO/FIXME | PASS | None found in any of the three changed files |
| No unsafe code | PASS | No unsafe blocks introduced |
| No .unwrap() in non-test code | PASS | co_access_promotion_tick.rs clean |
| File size (500-line limit — source files) | PASS | co_access_promotion_tick.rs: 368 lines; typed_graph.rs: 759 lines (existing); test file extracted by design |
| Build compiles | PASS | `cargo build --workspace` — Finished dev profile, zero errors |
| All workspace tests pass | PASS | Zero failures across all suites |
| Clippy (changed files) | PASS | Zero warnings in changed files; pre-existing collapsible_if in unimatrix-engine is out of scope |
| New test would have caught the original bug | PASS | Primary assertion: weight=0.5 would fail if subquery filter is missing |
| Fix is minimal | PASS | Three files, changes are surgical — four JOIN conditions + one bind + comments |
| Knowledge stewardship — rust-dev (528-agent-1-fix) | PASS | Queried entries #4161, #3980; Stored entry #4162 via context_correct |
| Knowledge stewardship — tester (528-agent-2-verify) | PASS | Queried entries #3882, #4162, #3979, #3822; declined store (entry #4162 already complete) |

---

## Detailed Findings

### Root Cause Addressed — All Four JOIN Conditions

**Status**: PASS

**Evidence**: `co_access_promotion_tick.rs` lines 239-244:

```sql
JOIN entries ea2 ON ea2.id = ca2.entry_id_a AND ea2.status = ?3
JOIN entries eb2 ON eb2.id = ca2.entry_id_b AND eb2.status = ?3
...
JOIN entries ea ON ea.id = ca.entry_id_a AND ea.status = ?3
JOIN entries eb ON eb.id = ca.entry_id_b AND eb.status = ?3
```

All four JOIN conditions use allowlist form (`= ?3`). This is the critical fix — the subquery aliases ea2/eb2 (lines 239-240) must also be filtered or max_count is inflated by non-Active pairs, deflating edge weights on all promoted edges even if the outer filter (lines 243-244) correctly excludes them from promotion.

Bind at line 251 confirms: `.bind(Status::Active as u8 as i64) // ?3: i64 active status (allowlist)`.

### Comments Updated (Lines 215-227)

**Status**: PASS

**Evidence**: The comment block at lines 215-231 was fully rewritten to reflect:
- Allowlist form with explicit reference to GH #476 and updated GH #528
- Explanation that allowlist excludes Deprecated, Proposed, Quarantined, and any future non-Active status by construction
- Description of the denylist form that was previously used and its oscillation consequence with bugfix-471 compaction
- Subquery weight-correctness rationale
- NULL failure mode documentation: `status = NULL` is always NULL (not true), silently promotes nothing

### New Test: test_deprecated_endpoint_pair_not_promoted

**Status**: PASS

**Evidence**: Group K test in co_access_promotion_tick_tests.rs (lines 1061-1113):
- Seeds D (deprecated, status=1) BEFORE seed_co_access to preserve status via INSERT OR IGNORE
- Deprecated pair A↔D seeded with count=10; active pair A↔B with count=5 — higher deprecated count is the discriminating signal
- PRIMARY assertion (lines 1104-1112): weight must be 1.0 (5/5). A broken subquery filter yields max_count=10 and weight=0.5 — this directly catches the missed subquery fix
- SECONDARY assertions (lines 1081-1102): only A↔B edges present; A↔D and D↔A absent

This test would have caught the original bug: before the fix, max_count would be computed including the A↔D pair (count=10), producing weight=0.5 for A↔B instead of 1.0. The weight assertion at `(edge.weight - 1.0).abs() < 1e-9` would fail.

### typed_graph.rs Documentation Comment

**Status**: PASS

**Evidence**: Lines 95-109 document:
- Deprecated entries are intentionally retained (not filtered) for SR-01 Supersedes-chain traversal
- After compaction removes deprecated-endpoint edges (bugfix-471), deprecated nodes appear in `all_entries` with no outgoing CoAccess edges — EXPECTED and CORRECT
- Explicit warning: "do not add a filter to exclude deprecated nodes from this snapshot"

### File Size

**Status**: PASS

co_access_promotion_tick.rs is 368 lines (within limit). The test file `co_access_promotion_tick_tests.rs` is 1113 lines, but its header comment (line 2) explicitly states it was extracted to keep the main module under the 500-line limit, and it is a dedicated test module. typed_graph.rs at 759 lines is pre-existing and not modified in this fix beyond the comment addition; its size is outside the scope of this bugfix.

### Knowledge Stewardship

**Status**: PASS

Both agent reports contain the required `## Knowledge Stewardship` block with Queried and Stored/Declined entries.

**528-agent-1-fix (rust-dev)**:
- Queried: entry #4161 (pre-created lesson), entry #3980 (tick batch SELECT JOIN pattern)
- Stored: entry #4162 via `context_correct` superseding #4161 — added subquery alias detail and test design rule requiring higher deprecated count

**528-agent-2-verify (tester)**:
- Queried: entries #3882, #4162, #3979, #3822
- Declined: nothing novel to store — entry #4162 is complete and covers the full pattern including all four test cases

---

## Rework Required

None.

---

## Validation Notes

The fix is precise and addresses the exact root cause described in the bug report. The diagnosis identified that oscillation is caused by the same denylist-vs-allowlist asymmetry that bugfix-471 fixed in the compaction DELETE — and this fix symmetrically applies the allowlist to the promotion INSERT side. The subquery-side filter (ea2/eb2) is the critical element: without it, max_count is inflated even when the outer filter correctly excludes deprecated endpoints from promotion, producing wrong weights on all promoted edges. The test design correctly uses a higher count on the deprecated pair to make this weight deflation detectable.
