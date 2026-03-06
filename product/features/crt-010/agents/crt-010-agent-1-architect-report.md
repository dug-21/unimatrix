# Agent Report: crt-010-agent-1-architect

## Task
Produce architecture for crt-010 (Status-Aware Retrieval). Resolve three open design decisions from SCOPE.md. Store ADRs in Unimatrix.

## Outputs

### ARCHITECTURE.md
`product/features/crt-010/architecture/ARCHITECTURE.md`

### ADRs (Unimatrix)

| ADR | Title | Unimatrix ID | Resolves |
|-----|-------|-------------|----------|
| ADR-001 | RetrievalMode Enum for Strict vs Flexible Search | #481 | SCOPE decision #3, SR-09 |
| ADR-002 | Successor Similarity via Stored Embedding Cosine | #482 | SCOPE decision #2, SR-01 |
| ADR-003 | Single-Hop Supersession Traversal | #483 | SR-05 |
| ADR-004 | Co-Access Deprecated Exclusion via ID Set | #484 | SR-07 |
| ADR-005 | Deprecated 0.7x and Superseded 0.5x Penalty Multipliers | #485 | SCOPE decision #1, SR-02 |

### Design Decisions Summary

1. **Strict vs Flexible (ADR-001):** `RetrievalMode` enum on `ServiceSearchParams`, default `Flexible`. UDS uses `Strict` (drop all non-Active), MCP uses `Flexible` (penalize deprecated). No new MCP parameters.
2. **Successor similarity (ADR-002):** Cosine from stored embedding via new `VectorIndex::get_embedding()` method. Falls back to re-embedding if hnsw_rs doesn't support point retrieval.
3. **Penalty multipliers (ADR-005):** `DEPRECATED_PENALTY = 0.7`, `SUPERSEDED_PENALTY = 0.5`. Multiplicative on re-rank score. Integration tests assert active > deprecated at equal similarity.
4. **Supersession traversal (ADR-003):** Single-hop only. Successor must be Active and not itself superseded.
5. **Co-access filtering (ADR-004):** `HashSet<u64>` parameter on engine crate functions. Minimal cross-crate coupling.

### SR Risk Traceability

| Risk | Addressed By |
|------|-------------|
| SR-01 (successor latency) | ADR-002: dot product on L2-normalized vectors is O(dim), bounded to at most k lookups |
| SR-02 (arbitrary penalties) | ADR-005: validated with numerical examples, integration test invariants |
| SR-03 (pruning removes embeddings) | ARCHITECTURE.md documents re-embedding as explicit restore cost |
| SR-04 (empty results in strict mode) | ARCHITECTURE.md: no fallback, returns empty gracefully |
| SR-05 (transitive chains) | ADR-003: codified as single-hop with documented limitation |
| SR-06 (no schema changes) | Confirmed: RetrievalMode is in-memory per-request only |
| SR-07 (cross-crate dependency) | ADR-004: HashSet<u64> keeps engine decoupled from server types |
| SR-08 (combinatorial test surface) | ADR-001: two modes, behavioral matrix bounded |
| SR-09 (API ripple) | ADR-001: default Flexible for backward compatibility |

### Integration Surface Summary

**New types:** `RetrievalMode` enum (pub(crate) in server), `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` constants (pub in engine), `cosine_similarity` function (pub in engine), `VectorIndex::get_embedding` method (pub in vector).

**Changed signatures:** `compute_search_boost` and `compute_briefing_boost` gain `deprecated_ids: &HashSet<u64>` parameter.

**Unchanged:** Store schema, MCP tool parameters, confidence formula, HNSW search API.

### Open Questions

1. BriefingService callers also need to thread `deprecated_ids` through `compute_briefing_boost`.
2. hnsw_rs `get_point_indexation().get_point_data()` API needs verification during implementation.

## Status
Complete.
