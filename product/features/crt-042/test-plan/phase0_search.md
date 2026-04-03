# crt-042: Test Plan — Phase 0 (`search.rs` integration)

## Component Scope

Component 2 of 4. Phase 0 is the async orchestration block in `search.rs` that calls
`graph_expand`, fetches and scores expanded entries, enforces quarantine, and merges
results into `results_with_scores`. Tests in this file cover the Phase 0 *caller* behavior
in `search.rs`, not the pure `graph_expand` function (covered in `graph_expand.md`).

**Files under test**:
- `crates/unimatrix-server/src/services/search.rs` (Phase 0 block)
- Integration: `product/test/infra-001/suites/test_lifecycle.py` (AC-25 cross-category test)

**Test types**:
- Unit/integration tests within `unimatrix-server` crate test module
- One new integration test in `infra-001` harness (AC-25)
- One tracing subscriber test (AC-24)

---

## AC-01: Flag-Off Regression (Bit-Identical)

**Risk covered**: R-01 (Critical)

**Verification method**: Run the full existing search test suite with `ppr_expander_enabled =
false` (the default, which is the config default after crt-042 is merged). Every existing
test that passed pre-crt-042 must pass post-crt-042.

```bash
# Stage 3c execution:
cargo test --package unimatrix-server -- search 2>&1 | tail -30
```

Additionally, the infra-001 harness smoke tests exercise search end-to-end:
```bash
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

**Dedicated unit test**:

```rust
// test_search_flag_off_pool_size_unchanged
// Assert: with ppr_expander_enabled=false, after Phase 0 guard, results_with_scores
// contains exactly the HNSW k results (no expansion).
// Arrange: SearchService with ppr_expander_enabled=false; graph has entries reachable
//          from seeds via positive edges.
// Act: execute search.
// Assert: results_with_scores.len() == expected_hnsw_count (no Phase 0 entries added).
```

**Assertion**: `results_with_scores.len()` after Step 6d is identical to the pre-Phase-0
insertion point count. The Phase 0 guard is the first line of Step 6d. A result count
difference is a Critical R-01 failure.

**Additional**: assert `Instant::now()` is NOT called on the flag-false path. This can be
verified indirectly: if no `elapsed_ms` field appears in any tracing event during a flag=false
search, the timing code did not execute.

---

## AC-02: Phase 0 Invocation Before Phase 1

**Risk covered**: R-16 (High)

**Assertion**: After Phase 0 completes and before Phase 1 constructs `seed_scores`, the
`results_with_scores` slice contains at least one entry whose ID was NOT in the original
HNSW result set (i.e., an entry added by graph expansion).

```rust
// test_search_phase0_expands_before_phase1
// Arrange:
//   - Entry S with embedding close to query Q (will be in HNSW k=20)
//   - Entry E with embedding far from Q (will NOT be in HNSW k=20)
//   - Graph: S → E (CoAccess)
//   - ppr_expander_enabled = true
// Act: execute search with query Q
// Assert:
//   1. result set contains E
//   2. E has a non-zero cosine similarity score (Phase 0 scored it)
//   3. E appears in the PPR personalization input (Phase 1 sees it)
```

This test validates both R-16 (insertion point is before Phase 1) and the core functional
requirement: expanded entries receive PPR personalization mass.

**Code inspection supplement**: During Stage 3c, inspect `search.rs` Step 6d to confirm
Phase 0 block comment is the first block inside `if !use_fallback`, before any `seed_scores`
construction. Flag this in the risk coverage report.

---

## AC-13 / AC-14: Quarantine Safety

**Risk covered**: R-03 (High)

Two tests, covering direct (1-hop) and transitive (2-hop) reachability of quarantined entries.

```rust
// test_search_phase0_excludes_quarantined_direct
// Arrange:
//   - Entry A (active, in HNSW seeds)
//   - Entry Q (quarantined status)
//   - Graph: A → Q (CoAccess)
//   - ppr_expander_enabled = true
// Act: execute search with query that surfaces A as seed
// Assert:
//   - Q does not appear in results_with_scores
//   - No warning or error log emitted for the skip (R-03: silent skip)
//   - Results are not empty (A is still present)
#[test]
fn test_search_phase0_excludes_quarantined_direct() { ... }

// test_search_phase0_excludes_quarantined_transitive
// Arrange:
//   - Entry A (active, in HNSW seeds)
//   - Entry B (active, reachable from A)
//   - Entry Q (quarantined, reachable from B)
//   - Graph: A → B → Q (all positive edges)
//   - ppr_expander_enabled = true
// Act: execute search with query that surfaces A as seed
// Assert:
//   - Q does not appear in results_with_scores
//   - B does appear in results_with_scores (non-quarantined; entry between A and Q)
#[test]
fn test_search_phase0_excludes_quarantined_transitive() { ... }
```

**Quarantine check order invariant**: The check `SecurityGateway::is_quarantined()` must run
AFTER `entry_store.get()` and BEFORE `results_with_scores.push()`. The implementation review
in Stage 3c must confirm this order. A check before the fetch (using only graph-stored
metadata) or after the push (undoing a push) are both incorrect.

**infra-001 integration**: If the `test_search_excludes_quarantined` test in `test_security.py`
already constructs graph edges to quarantined entries, it covers AC-13/14 at the integration
level. If it does not (because it predates crt-042 graph expansion), add a targeted test to
`test_lifecycle.py`:

```python
# suites/test_lifecycle.py
def test_search_graph_expand_excludes_quarantined_entry(admin_server):
    """AC-14: quarantined entry reachable via graph edge must not appear in results."""
    # Store seed entry A
    # Store entry Q, then quarantine it
    # Insert GRAPH_EDGES row: A → Q (CoAccess)
    # Run search with query that surfaces A; assert Q absent
    ...
```

**Fixture**: `admin_server` (needed for quarantine operation).

---

## AC-15: Embedding Skip (None Path)

**Risk covered**: R-15 (Low)

```rust
// test_search_phase0_skips_entry_with_no_embedding
// Arrange:
//   - Entry A (active, has embedding, in HNSW seeds)
//   - Entry E (active, NO stored embedding — get_embedding returns None)
//   - Graph: A → E (CoAccess)
//   - ppr_expander_enabled = true
// Act: execute search
// Assert:
//   - E does not appear in results_with_scores (silently skipped)
//   - No error or warning logged for the skip
//   - Result is not empty (A is still present)
```

**Implementation note**: In the Phase 0 embedding lookup:
```rust
let Some(emb) = vector_store.get_embedding(expanded_id).await else { continue; };
```
The `else { continue; }` must skip the entry without any warn!/error! call. Verify this in
the code review.

**Code inspection (R-15)**: Confirm `vector_store.get_embedding()` uses `IntoIterator` over
all HNSW layers (not `get_layer_iterator(0)` which would miss non-layer-0 points). This is
the crt-014 fix (entry #1724) that must apply on the search path too. If the implementation
uses a different embedding retrieval path, flag this in the risk coverage report.

---

## AC-24: Tracing Instrumentation Emission

**Risk covered**: R-04 (High), R-10 (Med)

This test is mandatory. Entry #3935 documents a gate failure where tracing tests were
deferred. Do not defer.

```rust
// test_search_phase0_emits_debug_trace_when_enabled
// Arrange:
//   - SearchService with ppr_expander_enabled = true
//   - A graph with at least one reachable entry from seeds
//   - A tracing subscriber capturing DEBUG-level events
// Act: execute search
// Assert:
//   1. Exactly one debug event with message containing "Phase 0 (graph_expand) complete"
//      is emitted per search invocation.
//   2. Event contains field `seeds` (count of HNSW seed IDs passed to graph_expand)
//   3. Event contains field `expanded_count` (raw count from graph_expand)
//   4. Event contains field `fetched_count` (entries added after quarantine/embedding filter)
//   5. Event contains field `elapsed_ms` (wall-clock milliseconds, >= 0)
//   6. Event contains field `expansion_depth`
//   7. Event contains field `max_expansion_candidates`

// test_search_phase0_does_not_emit_trace_when_disabled
// Arrange: ppr_expander_enabled = false
// Act: execute search
// Assert: no event with "Phase 0" in the message is emitted
```

**Recommended implementation using `tracing-test` crate**:

```toml
# In unimatrix-server Cargo.toml [dev-dependencies]
tracing-test = "0.2"
```

```rust
#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    #[traced_test]
    #[tokio::test]
    async fn test_search_phase0_emits_debug_trace_when_enabled() {
        // ... arrange SearchService with ppr_expander_enabled=true ...
        // ... execute search ...
        assert!(logs_contain("Phase 0 (graph_expand) complete"));
        assert!(logs_contain("expanded_count"));
        assert!(logs_contain("elapsed_ms"));
    }
}
```

**Alternative** if `tracing-test` is not available: use a
`tracing_subscriber::fmt::TestWriter` with a custom `Layer` that captures structured fields.

**Macro level check**: In Stage 3c code review, assert the macro in `search.rs` is
`tracing::debug!`, NOT `tracing::info!`. A grep:
```bash
grep -n 'Phase 0.*graph_expand' crates/unimatrix-server/src/services/search.rs
```
Must show `debug!` in the surrounding line context.

---

## AC-25: Cross-Category Entry Visible With Flag On, Absent With Flag Off

**Risk covered**: R-07 (High)

This is the behavioral proof of the entire architecture. It is mandatory regardless of
eval gate outcome.

### Unit-level version (preferred for determinism)

```rust
// test_search_phase0_cross_category_entry_visible_with_flag_on
// Arrange:
//   - EntryRecord S: embedding aligned with query Q (HNSW seed), category "decision"
//   - EntryRecord E: embedding dissimilar to Q (would NOT appear in HNSW k=20),
//                    category "lesson-learned" (different category = cross-category)
//   - Mock or test vector store: get_nearest_k(Q, 20) returns {S}, NOT {E}
//   - Graph: S → E (Supports edge)
//   - SearchService config: ppr_expander_enabled = true
// Act: execute search with query Q
// Assert: E appears in final result set
//
// Then with ppr_expander_enabled = false:
//   - Same arrange, same query
//   - Assert: E does NOT appear in final result set
```

**Implementation note for test isolation**: This test requires control over the HNSW
k-nearest-neighbor results to guarantee E is outside the k=20 window. Options:
1. Use the actual vector store but store E with a maximally dissimilar embedding vector
   (all-zeros vs all-ones) — works if the vector store is initialized in the test.
2. Use a mock `AsyncVectorStore` that returns a fixed HNSW result set.

The delivery agent must advise which approach is feasible given the SearchService
constructor constraints.

### infra-001 integration version

If server-level config override per test is supported (separate server instance per config):

```python
# suites/test_lifecycle.py or suites/test_graph_expand.py
def test_search_graph_expand_surfaces_cross_category_entry(server):
    """AC-25 behavioral proof: cross-category entry visible with flag=true, absent with flag=false."""
    # This test requires two server instances with different configs.
    # Triage: if harness does not support per-test config override, mark SKIP
    # with a note to implement as a unimatrix-server crate integration test.
    ...
```

**If the harness does not support per-test config**: implement AC-25 as a `#[tokio::test]`
in `unimatrix-server/tests/` (the integration test layer within Rust, not MCP-level). File
this as a known constraint in the Risk Coverage Report.

---

## R-05: Combined Ceiling Verification

**Risk covered**: R-05

```rust
// test_search_phase0_phase5_combined_ceiling
// Arrange:
//   - HNSW k=20 seeds
//   - Graph with 200 reachable entries from seeds (Phase 0 fills to max_expansion_candidates)
//   - PPR produces 50 additional candidates above ppr_inclusion_threshold (Phase 5)
//   - ppr_expander_enabled = true, max_expansion_candidates = 200, ppr_max_expand = 50
// Act: execute search through Phase 0 and Phase 5
// Assert: results_with_scores.len() <= 270 (20 + 200 + 50)
//         Phase 5 does not re-inject Phase 0 entries (no duplicates)
```

**Phase 5 disjointness**: assert that after Phase 5 runs, no entry ID appears in
`results_with_scores` more than once. This validates that Phase 5's `NOT in results_with_scores`
check covers Phase 0 entries (ADR-002 consequence).

---

## Test Count Summary

| Test Name | AC | Risk |
|-----------|-----|------|
| test_search_flag_off_pool_size_unchanged | AC-01 | R-01 |
| infra-001 smoke suite (existing) | AC-01 | R-01 |
| test_search_phase0_expands_before_phase1 | AC-02 | R-16 |
| test_search_phase0_excludes_quarantined_direct | AC-13/14 | R-03 |
| test_search_phase0_excludes_quarantined_transitive | AC-13/14 | R-03 |
| infra-001: test_search_graph_expand_excludes_quarantined_entry | AC-14 | R-03 |
| test_search_phase0_skips_entry_with_no_embedding | AC-15 | R-15 |
| test_search_phase0_emits_debug_trace_when_enabled | AC-24 | R-04, R-10 |
| test_search_phase0_does_not_emit_trace_when_disabled | R-10 | R-10 |
| test_search_phase0_cross_category_entry_visible_with_flag_on | AC-25 | R-07 |
| infra-001: test_search_graph_expand_surfaces_cross_category_entry | AC-25 | R-07 |
| test_search_phase0_phase5_combined_ceiling | R-05 | R-05 |

**Total**: ~10 unit/integration tests + 2 infra-001 tests for Component 2.
