# Gate Bugfix Report: bugfix-286

> Gate: Bugfix Validation
> Date: 2026-03-15
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Fix addresses root cause | PASS | `get_embedding` now iterates all HNSW layers via `IterPoint`; diagnosis confirmed correct |
| No placeholder code | PASS | No `todo!()`, `unimplemented!()`, TODO, FIXME in changed files |
| All tests pass | PASS | 105 passed, 1 ignored (GH#288); workspace 2527 passed, 0 failed |
| No new clippy warnings | PASS | `unimatrix-vector` clips clean; pre-existing failures unrelated to this fix |
| No unsafe code | PASS | No `unsafe` introduced |
| Fix is minimal | PASS | 1 line changed in `get_embedding` body + doc comment; no unrelated changes |
| New tests would have caught the original bug | PASS | `test_get_embedding_returns_some_for_all_points_regardless_of_layer` fails deterministically with old code |
| Integration smoke tests passed | PASS | 19 passed, 1 xfailed (pre-existing GH#111) |
| xfail markers have corresponding GH Issues | PASS | GH#288 filed and open; GH#111 pre-existing |
| xfail for GH#286 removed | PASS | No `xfail` referencing #286 present in test_lifecycle.py |
| Knowledge stewardship — investigator | PASS | `## Knowledge Stewardship` block present; Queried + Stored entries |
| Knowledge stewardship — rust-dev | PASS | `## Knowledge Stewardship` block present; Queried + Stored entries |
| Knowledge stewardship — tester | WARN | Queried skipped ("server status unclear"); Stored with reason given |

---

## Detailed Findings

### Fix Addresses Root Cause

**Status**: PASS

**Evidence**: `crates/unimatrix-vector/src/index.rs` lines 301–331. The fix replaces `point_indexation.get_layer_iterator(0)` with `for point in point_indexation`, using `IntoIterator for &PointIndexation` (`IterPoint`), which traverses all layers from 0 through `entry_point_level`. The doc comment at lines 301–311 explicitly explains the hnsw_rs storage invariant and references GH#286. The root cause (layer-0-only iteration missing ~6% of points) is directly corrected.

The investigator report traces the exact code path and confirms the hnsw_rs internal storage model (`points_by_layer[L]` where L is the randomly assigned insertion level). The fix is a precise correction of the wrong assumption.

### No Placeholder Code

**Status**: PASS

**Evidence**: `grep` over `index.rs` and `test_lifecycle.py` returns no matches for `todo!()`, `unimplemented!()`, `TODO`, or `FIXME`.

### All Tests Pass

**Status**: PASS

**Evidence**:
- `cargo test -p unimatrix-vector`: 105 passed, 0 failed, 1 ignored (GH#288 pre-existing flaky test).
- Fix agent reported 106 passed; the difference of 1 is accounted for by `test_compact_search_consistency` being moved to `#[ignore]` by the verify agent after the fix agent ran.
- Workspace total: 2527 passed, 0 failed (verify agent report).
- `test_search_multihop_injects_terminal_active` passes hard (xfail removed and confirmed passing in isolation and full lifecycle suite).

### No New Clippy Warnings

**Status**: PASS

**Evidence**: `cargo clippy -p unimatrix-vector -- -D warnings` finishes clean (no warnings, no errors). Pre-existing `collapsible_if` failures in `unimatrix-engine/auth.rs` confirmed as not introduced by this fix (commit touches only `crates/unimatrix-vector/src/index.rs` and the lifecycle test file).

### No Unsafe Code

**Status**: PASS

**Evidence**: `grep -n "unsafe"` in `index.rs` returns no results.

### Fix Is Minimal

**Status**: PASS

**Evidence**: Changed files per rust-dev report: `crates/unimatrix-vector/src/index.rs` (1 functional line changed in `get_embedding`, doc comment updated, 2 new tests added, 1 `#[ignore]` added by verify agent) and `product/test/infra-001/suites/test_lifecycle.py` (xfail removed). No unrelated logic, no refactors, no scope additions.

### New Tests Would Have Caught the Original Bug

**Status**: PASS

**Evidence**: `test_get_embedding_returns_some_for_all_points_regardless_of_layer` inserts 200 points, ensuring statistical certainty that at least one lands at layer ≥ 1 (P(all at layer 0) < 10⁻⁶). With the old `get_layer_iterator(0)` implementation, `get_embedding` would return `None` for those points, causing the `assert!(missing.is_empty(), ...)` assertion to fail. The test is a deterministic regression guard. `test_get_embedding_value_matches_inserted_vector` additionally verifies round-trip fidelity.

### Integration Smoke Tests

**Status**: PASS

**Evidence**: 19 passed, 1 xfailed (`test_store_1000_entries`, GH#111 — pre-existing rate limit issue, unrelated to this fix), 0 failed.

### xfail Markers Have Corresponding GH Issues

**Status**: PASS

**Evidence**: `test_compact_search_consistency` annotated `#[ignore = "Pre-existing: GH#288 — flaky, HNSW non-determinism with 5-point dataset"]`. GH#288 confirmed open and correctly titled. The two remaining xfails in `test_lifecycle.py` reference GH#238 and an undated tick-interval issue — both pre-existing, not introduced by this fix.

### xfail for GH#286 Removed

**Status**: PASS

**Evidence**: `grep` for `xfail.*286` in `test_lifecycle.py` returns no results. `test_search_multihop_injects_terminal_active` (line 702) has no `@pytest.mark.xfail` decorator.

### Knowledge Stewardship — Investigator

**Status**: PASS

**Evidence**: `## Knowledge Stewardship` block present in `286-investigator-report.md`. Queried `/uni-query-patterns` and `/uni-knowledge-search`. Stored entry #1712 "hnsw_rs: points stored only at assigned layer, not at layer 0" via `/uni-store-lesson`.

### Knowledge Stewardship — Rust-Dev

**Status**: PASS

**Evidence**: `## Knowledge Stewardship` block present in `286-agent-1-fix-report.md`. Queried `/uni-query-patterns` for the vector/hnsw domain. Stored entry #1724 "get_embedding: use IntoIterator for &PointIndexation (all layers)" via `/uni-store-pattern`.

### Knowledge Stewardship — Tester

**Status**: WARN

**Evidence**: `## Knowledge Stewardship` block present in `286-agent-2-verify-report.md`. Queried entry skipped with reason "server status unclear — non-blocking per protocol." Stored entry says "nothing novel to store — the HNSW layer-assignment flakiness pattern is an instance of general well-understood flakiness." Reason is provided; not a missing block. Skipping the query rather than attempting it is a minor deviation but the stored entry rationale is sound and the block is present. Does not block delivery.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — this gate found no systemic failure patterns; all checks passed. The hnsw_rs lesson (entry #1712) and pattern (entry #1724) were correctly stored by the delivering agents. No recurring gate failure pattern to record.
