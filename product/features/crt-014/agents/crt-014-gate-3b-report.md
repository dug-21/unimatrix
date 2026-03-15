# Agent Report: crt-014-gate-3b

> Agent: crt-014-gate-3b (Gate 3b Validator)
> Feature: crt-014 — Topology-Aware Supersession
> Date: 2026-03-15
> Gate Result: PASS

## Summary

Completed Gate 3b (Code Review) for crt-014. All six primary checks pass. Two findings logged as WARN (safe unwrap, cargo-audit not installed). File size FAIL noted as structural (test-code volume, NFR-07 conflict) and does not block gate.

## Artifacts Reviewed

- `crates/unimatrix-engine/src/graph.rs` (1037 lines, new)
- `crates/unimatrix-engine/src/lib.rs` (pub mod graph added)
- `crates/unimatrix-engine/Cargo.toml` (petgraph + thiserror added)
- `crates/unimatrix-engine/src/confidence.rs` (constants + 3 tests removed, 1 renamed)
- `crates/unimatrix-server/src/services/search.rs` (graph integration)
- `crates/unimatrix-engine/tests/pipeline_retrieval.rs` (shim removed, graph constants used)
- Three rust-dev agent reports (agents 3, 4, 5)

## Key Validation Findings

### QueryFilter::default() Deviation (CORRECT)

`Store::query(QueryFilter::default())` at `read.rs:289-292` returns only `Status::Active` entries when the filter is empty. The agent discovered this and substituted four `store.query_by_status(status)` calls covering all Status variants. This correctly implements IR-01 (full-store graph includes all statuses). Pattern stored to Unimatrix as entry #1588 by agent-5.

### ServiceError Variants (CORRECT)

`ServiceError` has no `Internal(String)` variant (that identifier belongs to `CallerId::Internal`). Agent used `ServiceError::EmbeddingFailed` for join errors and `ServiceError::Core(CoreError::Store(e))` for store errors — both appropriate.

### confidence.rs Test 4 Rename (CORRECT)

`penalties_independent_of_confidence_formula` renamed to `weight_sum_invariant_is_0_92`. Test body had no reference to removed constants; rename removes the misleading "penalty constants" from the test name. Permitted by AC-15 and pseudocode specification.

### AC-11 Depth Cap Semantics (CORRECT)

Test plan says "chain of 11 entries → None." The implementation correctly determines this requires 12 entries because the `depth + 1 > MAX_TRAVERSAL_DEPTH` check allows visiting nodes at depth 10 (the 11th hop from start). The implementation provides two tests: `terminal_active_depth_cap` (12-entry chain, terminal at depth 11 → None) and `terminal_active_depth_boundary` (11-entry chain, terminal at depth 10 → Some). Both tests pass and correctly characterize the depth cap boundary.

### Unsafe unwrap() (WARN)

Lines 337 and 377 of search.rs call `graph_opt.as_ref().unwrap()` inside `if !use_fallback` blocks. The invariant (graph_opt is Some when use_fallback is false) is maintained by the match on lines 295-304 and documented in pseudocode. Logically infallible but not idiomatic. Does not block gate.

### File Size (FAIL — structural)

`graph.rs` is 1037 lines. Production code is ~338 lines (within limit); test code accounts for ~699 lines (43 unit test functions required by test plan). NFR-07 mandates inline tests with no isolated scaffolding. The file size limit and NFR-07 are in structural tension for test-heavy pure-function modules. This is the only FAIL and is not addressable by rework within crt-014 scope.

## Build and Test Results

- `cargo build --workspace`: Zero errors. Six pre-existing warnings in unimatrix-server.
- `cargo test --workspace`: 2,360 tests, 0 failures, 18 ignored.
- `DEPRECATED_PENALTY` / `SUPERSEDED_PENALTY` absent from production code (only appear in string literals in test assertions and comments).

## Knowledge Stewardship

- Queried: `/uni-query-patterns` — not applicable at validation gate level; reviewed implementation agent reports for evidence of pre-implementation queries.
- Stored: nothing novel to store — the file-size/NFR-07 tension is feature-specific; the QueryFilter::default() Active-only gotcha was already stored by agent-5 (entry #1588).
