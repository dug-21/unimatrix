# Agent Report: crt-042-agent-1-architect

## Deliverables

### ARCHITECTURE.md
`product/features/crt-042/architecture/ARCHITECTURE.md`

### ADR Files

| ADR | File | Unimatrix ID |
|-----|------|--------------|
| ADR-001 | `architecture/ADR-001-graph-expand-submodule-placement.md` | #4049 |
| ADR-002 | `architecture/ADR-002-phase-0-insertion-point.md` | #4050 |
| ADR-003 | `architecture/ADR-003-cosine-similarity-source-for-expanded-entries.md` | #4051 |
| ADR-004 | `architecture/ADR-004-config-validation-unconditional.md` | #4052 |
| ADR-005 | `architecture/ADR-005-timing-instrumentation-approach.md` | #4053 |
| ADR-006 | `architecture/ADR-006-traversal-direction-outgoing-only.md` | #4054 |

## Key Decisions

1. **graph_expand placement**: new `graph_expand.rs` as `#[path]` submodule of `graph.rs`, re-exported via `pub use`. Mirrors `graph_ppr.rs` / `graph_suppression.rs` pattern. Not inline in search.rs (wrong layer) or graph.rs (500-line limit). ADR-001 (#4049).

2. **Phase 0 insertion**: first block inside `if !use_fallback` in Step 6d, before Phase 1 (personalization vector). Combined ceiling: 20 + 200 + 50 = 270 entries maximum post-expansion. Phase 5 still runs; the two mechanisms are complementary and disjoint. ADR-002 (#4050).

3. **Cosine similarity source**: true cosine via `vector_store.get_embedding()`. Delivery agent must investigate O(1) `entry_id → data_id → Vec<f32>` path in `VectorIndex` before falling back to the O(N) HNSW scan. Skip-if-None policy matches crt-014 pattern. ADR-003 (#4051).

4. **Config validation**: unconditional always — both `expansion_depth` and `max_expansion_candidates` validated at server start regardless of `ppr_expander_enabled`. Prevents NLI conditional-validation trap. ADR-004 (#4052).

5. **Timing instrumentation**: single `debug!` event per Phase 0 completion: `expanded_count`, `fetched_count`, `elapsed_ms`, `expansion_depth`, `max_expansion_candidates`. Gate condition: P95 elapsed_ms < 50ms before flag can become default. ADR-005 (#4053).

6. **Traversal direction**: `Direction::Outgoing` only. Behavioral contract stated in module-level doc comment alongside enum value (SR-06 mandate, citing lesson #3754). S1/S2 single-direction write is a write-side deficiency — the fix is a back-fill migration, not a traversal direction change. ADR-006 (#4054).

## Blocking Gates Surfaced

### SR-03 (Hard Gate — Do Not Write Phase 0 Code Until Resolved)
S1/S2 Informs edges are written single-direction in crt-041 (`source_id < target_id`). S8 CoAccess edges are also written single-direction by `run_s8_tick` (`a = min(ids), b = max(ids)`). Outgoing-only traversal from the higher-ID seed reaches nothing via these edges.

**Delivery agent action before implementation**:
1. Query live GRAPH_EDGES: does a bidirectional S1/S2 back-fill already exist (from any prior migration)?
2. For S8: confirm whether `run_co_access_promotion_tick` (crt-035) covers all S8 CoAccess pairs bidirectionally or only promotion-tick pairs.
3. If S1/S2 is single-direction in GRAPH_EDGES: file a new issue for a back-fill migration (pattern: crt-035, entry #3889) and block crt-042 ship on it.

### SR-01 (Latency Gate — Investigation Before Default Enablement)
Delivery agent must investigate O(1) embedding lookup via `VectorIndex.id_map.entry_to_data` before implementing the O(N) `get_embedding` path. If O(1) is feasible, implement it. If not feasible in this feature, document it and open a follow-up issue. The O(N) path proceeds with the feature flag gate; the 50ms P95 ceiling (ADR-005) is the enforcement mechanism.

## Open Questions

1. Is there a path from `entry_id → data_id → Vec<f32>` in `VectorIndex` that bypasses the HNSW layer scan? (delivery agent investigation — determines Phase 0 latency profile)
2. What is the actual bidirectionality state of S1/S2 Informs edges in the live GRAPH_EDGES table? (delivery agent pre-implementation verification — SR-03 gate)
3. Does `run_s8_tick` need to write both `(a, b)` and `(b, a)` CoAccess edges, or does `run_co_access_promotion_tick` cover all pairs bidirectionally? (delivery agent confirmation)
4. Who owns the eval gate failure scenario (MRR regression or P@5 flat after correct implementation)? (delivery brief must name an owner — SR-05)
