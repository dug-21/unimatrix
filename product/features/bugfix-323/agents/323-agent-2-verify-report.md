# Agent Report: 323-agent-2-verify

**Bug:** GH #323 — eval snapshot missing vector index files
**Branch:** bugfix/323-eval-snapshot-vector-index
**Phase:** Test Execution (Bug Fix Verification)
**Date:** 2026-03-20

---

## Test Execution Summary

### 1. Bug-Specific Unit Test

**Test:** `test_from_profile_loads_vector_index_from_snapshot_dir`
**Command:** `cargo test --lib -p unimatrix-server "test_from_profile_loads_vector_index_from_snapshot_dir"`
**Result:** PASS (1 passed, 1588 filtered out, 0.23s)

This test validates the full round-trip:
- Seeds a SqlxStore with 10 entries + deterministic embeddings
- Dumps HNSW files into `{snap_parent}/vector/`
- Calls `EvalServiceLayer::from_profile()` against the snapshot
- Independently loads the persisted VectorIndex via `VectorIndex::load()`
- Asserts `point_count() == 10` and that `search()` returns non-empty results with `best > 0.0` similarity

### 2. Full Workspace Test Suite

**Command:** `cargo test --lib --workspace`

| Crate | Tests | Result |
|-------|-------|--------|
| unimatrix-core | 47 | PASS |
| unimatrix-vector | 12 | PASS |
| unimatrix-embed | 76 passed, 18 ignored | PASS |
| unimatrix-store | 291 | PASS |
| unimatrix-learn | 73 | PASS |
| unimatrix-adapt | 353 | PASS |
| unimatrix-server | 1589 | PASS |
| unimatrix-observe | 129 | PASS |
| unimatrix-engine | 106 | PASS |

**Total:** 2676 lib tests passed, 0 failed, 18 ignored.

**Doctest failure (pre-existing, not caused by this fix):**
`crates/unimatrix-server/src/infra/config.rs - infra::config (line 21)` — failing because the doc comment starts with `~/.unimatrix/config.toml` which the Rust doctest runner tries to parse as code. Last touched in PR #321, predates this branch. Not in any changed file.

### 3. Clippy Check

**Command:** `cargo clippy --workspace -- -D warnings`

All clippy errors are in `crates/unimatrix-store/src/` (analytics.rs, migration.rs, read.rs) — pre-existing, not touched by this fix. Zero clippy errors or warnings in:
- `crates/unimatrix-server/src/snapshot.rs`
- `crates/unimatrix-server/src/eval/profile/layer.rs`
- `crates/unimatrix-server/src/eval/profile/tests.rs`

### 4. Integration Smoke Tests (Mandatory Gate)

**Command:** `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`
**Result:** 20 passed, 185 deselected (174.74s)
**Gate:** PASS

### 5. Lifecycle Suite (Relevant — restart persistence, multi-step flows)

**Command:** `python -m pytest suites/test_lifecycle.py -v --timeout=60`
**Result:** 24 passed, 1 xfailed (210.85s)
**The xfail** is `test_retrospective_baseline_present` (GH#305, pre-existing, already marked)

### 6. Tools Suite (Relevant — all 12 tools, store/retrieval behavior)

**Command:** `python -m pytest suites/test_tools.py --timeout=60`
**Result:** 72 passed, 1 xfailed (605.05s)
**The xfail** is pre-existing (already marked before this fix)

---

## Integration Test Failure Triage

| Suite | Failures | Triage |
|-------|----------|--------|
| smoke | 0 | N/A |
| lifecycle | 0 (1 xfail) | Pre-existing GH#305, already marked |
| tools | 0 (1 xfail) | Pre-existing, already marked |

No integration test failures caused by this bug fix. No new GH Issues required. No xfail markers added.

---

## Verification Verdict

| Check | Result |
|-------|--------|
| Bug-specific test passes | PASS |
| No regression in lib tests (2676 tests) | PASS |
| No clippy issues in changed files | PASS |
| Smoke gate (20/20) | PASS |
| Lifecycle suite (24/24) | PASS |
| Tools suite (72/72) | PASS |
| Pre-existing issues unchanged | CONFIRMED |

**The fix is verified. All gates pass. No rework required.**

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "eval snapshot vector index testing procedures" — returned #750 (pipeline validation), #2326 (bug fix verification async pattern), #487 (workspace test without hanging). No results directly about eval/snapshot testing specifically.
- Stored: entry via `mcp__unimatrix__context_store` — "VectorIndex snapshot round-trip test pattern for eval offline pipeline (GH-323)" — topic: `testing`, category: `pattern`.
