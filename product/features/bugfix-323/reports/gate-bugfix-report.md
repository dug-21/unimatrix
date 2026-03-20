# Gate Bug Fix Report: bugfix-323

> Gate: Bug Fix Validation (re-run after rework)
> Date: 2026-03-20
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed (not just symptoms) | PASS | Both defect sites fixed — HNSW copy in snapshot.rs, load-vs-new branch in layer.rs |
| No todo!/unimplemented!/TODO/FIXME/placeholder | PASS | None found in any changed file |
| Bug-specific test exists and passes | PASS | `test_from_profile_loads_vector_index_from_snapshot_dir` — 1 passed, 1588 filtered out |
| New test would have caught original bug | PASS | Directly validates `VectorIndex::load()` branch; fails on unfixed code (empty index, no results) |
| Existing suite passes | PASS | 2676 lib tests passed, 0 failed across all crates |
| No new clippy warnings in changed files | PASS | 0 warnings in all changed Rust files |
| No unsafe code introduced | PASS | Confirmed absent in snapshot.rs, layer.rs, layer_tests.rs |
| Fix is minimal (no unrelated changes) | PASS | Exactly 6 files: 3 Rust source, 1 mod.rs (mod declaration only), 1 test file, 1 doc |
| No file exceeds 500 lines | PASS | tests.rs: 285, layer_tests.rs: 309, snapshot.rs: 459, layer.rs: 284 — all under 500 |
| Integration smoke tests passed | PASS | 20/20 smoke (verify agent report — unchanged from prior run) |
| xfail markers have GH Issues | PASS | Pre-existing xfail markers reference GH #305; no new xfails added |
| Investigator report: Knowledge Stewardship block | WARN | No investigator report file; entry #2661 confirmed in Unimatrix (lesson-learned) |
| Rust-dev report: Knowledge Stewardship block | WARN | No rust-dev report file; entry #2673 confirmed in Unimatrix (pattern) |
| Verify agent report: Knowledge Stewardship block | WARN | Block present; query attributed to `/uni-knowledge-search` (non-standard tool name) |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: Two defect sites confirmed fixed on commits `00cb359` (fix) and `85fe7a6` (gate rework):

1. `snapshot.rs` — `run_snapshot()` calls `copy_vector_files()` after `VACUUM INTO`. Helper reads `unimatrix-vector.meta`, parses `basename=` field, copies `{basename}.hnsw.graph`, `{basename}.hnsw.data`, and `unimatrix-vector.meta` into `{out_parent}/vector/`. Silently skips when meta file is absent.

2. `layer.rs` — `EvalServiceLayer::from_profile()` Step 5 checks for `{db_parent}/vector/unimatrix-vector.meta`. When present: calls `VectorIndex::load()`. When absent: falls back to `VectorIndex::new()` for backward compatibility.

Both sites match the approved root cause diagnosis.

### No todo!/unimplemented!/TODO/FIXME/placeholder

**Status**: PASS

**Evidence**: Grep across snapshot.rs, layer.rs, layer_tests.rs — no matches.

### Bug-Specific Test

**Status**: PASS

**Evidence**: `test_from_profile_loads_vector_index_from_snapshot_dir` in `layer_tests.rs` (lines 157–271). Independently verified: `cargo test --lib -p unimatrix-server "test_from_profile_loads_vector_index_from_snapshot_dir"` → 1 passed, 0 failed. Test seeds 10 entries with deterministic embeddings, dumps HNSW files, calls `from_profile()`, then independently loads via `VectorIndex::load()` and asserts `point_count() == 10` and search returns non-empty results with `best > 0.0` similarity.

### New Test Would Have Caught Original Bug

**Status**: PASS

**Evidence**: The test directly exercises the `VectorIndex::load()` branch added by the fix. Before the fix, `from_profile()` called `VectorIndex::new()` unconditionally, producing an empty index. `loaded_vi.point_count()` would have returned 0 and `search()` would have returned empty results — both assertions would have failed.

### Existing Suite Passes

**Status**: PASS

**Evidence**: `cargo test --lib --workspace` independently verified:
- unimatrix-core: 47 passed
- unimatrix-vector: 12 passed
- unimatrix-embed: 76 passed, 18 ignored
- unimatrix-store: 291 passed
- unimatrix-learn: 73 passed
- unimatrix-adapt: 353 passed
- unimatrix-server: 1589 passed
- unimatrix-observe: 129 passed
- unimatrix-engine: 106 passed
- **Total: 2676 passed, 0 failed**

### No New Clippy Warnings

**Status**: PASS

**Evidence**: Verify agent report confirms zero warnings in all changed Rust files. `cargo build --workspace` produces only 6 pre-existing warnings in unimatrix-server — none in changed files.

### No Unsafe Code

**Status**: PASS

**Evidence**: `grep -n "unsafe"` across snapshot.rs, layer.rs, layer_tests.rs returns no matches.

### Fix Is Minimal

**Status**: PASS

**Evidence**: `git diff main..HEAD --name-only` returns exactly:
- `crates/unimatrix-server/src/eval/profile/layer.rs` (fix: conditional VectorIndex load)
- `crates/unimatrix-server/src/eval/profile/layer_tests.rs` (new: bug-specific test file)
- `crates/unimatrix-server/src/eval/profile/mod.rs` (one line: `mod layer_tests;`)
- `crates/unimatrix-server/src/eval/profile/tests.rs` (trimmed: moved from_profile tests out)
- `crates/unimatrix-server/src/snapshot.rs` (fix: copy_vector_files added)
- `docs/testing/eval-harness.md` (doc update)

No unrelated production files touched.

### No File Exceeds 500 Lines

**Status**: PASS

**Evidence**: Line counts independently verified:
- `tests.rs`: 285 lines (was 578 before rework — now 293 lines under limit)
- `layer_tests.rs`: 309 lines (new file, under limit)
- `snapshot.rs`: 459 lines (under limit)
- `layer.rs`: 284 lines (under limit)

**Previous FAIL resolved.** The rework agent correctly split `tests.rs` by moving all `EvalServiceLayer::from_profile` integration tests into the new `layer_tests.rs` sibling file.

### Integration Tests

**Status**: PASS

**Evidence**: Verify agent report (323-agent-2-verify-report.md): 20/20 smoke, 24/24 lifecycle (1 pre-existing xfail GH#305), 72/72 tools (1 pre-existing xfail). No new failures.

### xfail Markers

**Status**: PASS

**Evidence**: No new xfail markers added. Pre-existing xfails reference GH #305 (documented pre-existing issue).

### Knowledge Stewardship — Investigator Report

**Status**: WARN

**Evidence**: No investigator report file exists at `product/features/bugfix-323/agents/`. Spawn prompt states entry #2661 stored (`#2661 | Snapshot commands must copy all storage artifacts, not just the primary database | lesson-learned`). Stewardship action occurred but is unverifiable from a file. Does not block.

### Knowledge Stewardship — Rust-Dev Report

**Status**: WARN (re-run: same standing as prior gate)

**Evidence**: The rework agent (323-agent-1-fix-rework1) did write a report file with a proper `## Knowledge Stewardship` block. Original rust-dev agent report file absent. The rework report correctly documents the structural split and notes nothing novel to store with a reason. No new FAIL.

### Knowledge Stewardship — Verify Agent Report

**Status**: WARN

**Evidence**: `product/features/bugfix-323/agents/323-agent-2-verify-report.md` contains `## Knowledge Stewardship` with `Queried:` and `Stored:` entries. Query attributed to `/uni-knowledge-search` (non-standard name). Entry stored in Unimatrix confirmed. Minor protocol name gap only.

---

## Rework Required

None. The single FAIL from the prior gate (tests.rs exceeding 500 lines) is resolved.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the pass pattern for this re-run gate is routine; the systemic lesson about snapshot artifact completeness was already captured in entry #2661 and the report-file gap in entry #2687 by the prior gate run.
