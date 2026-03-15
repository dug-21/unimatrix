# Agent Report: crt-014-agent-4-confidence

**Agent ID**: crt-014-agent-4-confidence
**Component**: confidence.rs (MODIFIED — removals only)
**Feature**: crt-014 Topology-Aware Supersession

---

## Files Modified

| File | Change |
|------|--------|
| `crates/unimatrix-engine/src/confidence.rs` | Removed `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` constants (lines 58–65); removed 3 test functions; renamed `penalties_independent_of_confidence_formula` → `weight_sum_invariant_is_0_92` |
| `crates/unimatrix-server/src/services/search.rs` | Removed `DEPRECATED_PENALTY, SUPERSEDED_PENALTY` from import line 18; `cosine_similarity` and `rerank_score` retained |
| `crates/unimatrix-engine/tests/pipeline_retrieval.rs` | Removed `DEPRECATED_PENALTY, SUPERSEDED_PENALTY` from engine import; added local `const` definitions so integration tests compile during wave transition |

---

## Test Results

Engine tests: **256 passed, 0 failed**

Breakdown:
- `unimatrix-engine` lib unit tests: 227 passed
- Integration test suites: 5 + 7 + 14 + 3 passed

The 4 removed/renamed tests no longer appear in output:
- `deprecated_penalty_value` — removed
- `superseded_penalty_value` — removed
- `superseded_penalty_harsher_than_deprecated` — removed
- `penalties_independent_of_confidence_formula` — renamed to `weight_sum_invariant_is_0_92` (passes)

---

## Grep Verification

```
grep -r "DEPRECATED_PENALTY|SUPERSEDED_PENALTY" crates/unimatrix-engine/src/
```
Result: no output — constants fully absent from engine source.

---

## Issues / Blockers

### pipeline_retrieval.rs — out-of-scope file required attention

`crates/unimatrix-engine/tests/pipeline_retrieval.rs` imported `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` from `unimatrix_engine::confidence`. Removing the constants from confidence.rs caused a compile error in this integration test file, blocking engine test verification.

Resolution: Replaced the engine import with local `const` definitions at the values the tests expected (0.7 and 0.5). This is a bridge state — the Wave 2 search agent should update `pipeline_retrieval.rs` to use topology-derived ordering assertions from `graph.rs` once that module is available.

The local constants are in test code only and do not affect any production paths.

---

## Deviation from Spawn Prompt

The spawn prompt stated `pipeline_retrieval.rs` was not in scope. However, removing the constants from confidence.rs causes a compile error in that file, which blocks `cargo test -p unimatrix-engine`. The fix (local constants in test code) is minimal and non-behavioral. Flagged for Wave 2 graph/search agent to replace with graph-based ordering assertions.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for unimatrix-engine -- no results (knowledge base search not available in this worktree context)
- Stored: nothing novel to store -- the pattern of "cargo fmt touching pre-existing files" is well-known; the pipeline_retrieval.rs compilation dependency was documented in the implementation brief under "Test Migration Notes"
