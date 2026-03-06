# crt-010 Pseudocode Overview

## Component Interaction

```
SearchService.search(params)
  |
  v
Step 5: HNSW search (existing)
  |
  v
Step 6: Fetch entries, exclude quarantined (existing)
  |
  v
Step 6a: [NEW] Status filter/penalty marking (C1)
  |  - Strict mode: drop non-Active and superseded entries
  |  - Flexible mode: mark deprecated/superseded for penalty
  |  - Explicit status filter (QueryFilter.status == Deprecated): skip penalties
  |
  v
Step 6b: [NEW] Supersession injection (C2)
  |  - Skip if explicit status filter is Deprecated (FR-6.2)
  |  - Collect superseded_by IDs from remaining results
  |  - Batch-fetch successors from entry_store
  |  - Inject Active, non-superseded successors with cosine similarity
  |
  v
Step 7: Re-rank with penalty multipliers (C7 constants)
  |  - DEPRECATED_PENALTY (0.7) for deprecated entries
  |  - SUPERSEDED_PENALTY (0.5) for superseded entries
  |
  v
Step 8: Co-access boost (C3 — deprecated exclusion via HashSet<u64>)
  |
  v
Steps 9-12: Truncate, floors, build results, audit (existing)
```

## Data Flow

1. **UDS Listener (C4)** sets `retrieval_mode: Strict` on `ServiceSearchParams`
2. **MCP Tools (C5)** sets `retrieval_mode: Flexible` on `ServiceSearchParams`
3. **SearchService (C1)** reads `retrieval_mode` and applies status filtering in Step 6a
4. **Supersession Injection (C2)** uses `VectorIndex::get_embedding()` and `cosine_similarity()` from engine crate
5. **Co-access (C3)** receives `deprecated_ids: &HashSet<u64>` — excludes deprecated from both anchor and partner roles
6. **Compaction (C6)** already filters to Active — verification test only

## Shared Types

```rust
// ServiceSearchParams gains one new field:
pub(crate) struct ServiceSearchParams {
    // ... existing fields ...
    pub(crate) retrieval_mode: RetrievalMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RetrievalMode {
    Strict,
    #[default]
    Flexible,
}
```

## Dependency Order (implementation)

1. C7: Penalty constants in `unimatrix-engine/src/confidence.rs` (leaf — no deps)
2. C3: Co-access deprecated exclusion in `unimatrix-engine/src/coaccess.rs` (leaf — no deps)
3. `VectorIndex::get_embedding()` in `unimatrix-vector/src/index.rs` + async wrapper in `unimatrix-core` (leaf)
4. C1: RetrievalMode + SearchService status filtering (depends on C7 constants, get_embedding)
5. C2: Supersession injection (integrated into C1's SearchService changes)
6. C4: UDS path hardening (depends on C1 for RetrievalMode)
7. C5: MCP filter asymmetry fix (depends on C1 for RetrievalMode)
8. C6: Verification test only (no code changes)

## Integration Harness Plan

The integration tests for crt-010 will use the existing `product/test/infra-001/` infrastructure where applicable. Key integration test scenarios:

1. **Full pipeline tests** via SearchService: insert entries with various statuses + supersession chains, run search in Strict and Flexible modes, verify filtering/penalty/injection behavior
2. **Co-access integration**: verify deprecated exclusion flows end-to-end through SearchService
3. **UDS integration**: verify strict mode enforcement through the UDS listener path
4. **Compaction verification**: confirm existing col-013 behavior via background tick

Most AC tests are best implemented as unit tests within the affected crates, with a handful of integration tests validating cross-crate interactions.

## Patterns Used

- **spawn_blocking for sync-in-async**: All store and vector operations use `tokio::task::spawn_blocking` (established pattern in async_wrappers.rs)
- **Poison recovery**: `RwLock::write().unwrap_or_else(|e| e.into_inner())` (established in VectorIndex)
- **Fire-and-forget**: `spawn_blocking_fire_and_forget` for non-critical writes (established in listener.rs)
- **Additive scoring pipeline**: `rerank_score(sim, conf) + boost + provenance` (established in search.rs)
- **HashSet filtering at crate boundary**: Keeps engine crate decoupled from server types (ADR-004)
