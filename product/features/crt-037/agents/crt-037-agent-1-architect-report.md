# Agent Report: crt-037-agent-1-architect

## Status: Complete

## Artifacts Produced

### ARCHITECTURE.md
`product/features/crt-037/architecture/ARCHITECTURE.md`

### ADR Files
- `product/features/crt-037/architecture/ADR-001-discriminator-tag-struct.md` — Unimatrix #3938
- `product/features/crt-037/architecture/ADR-002-combined-cap-priority.md` — Unimatrix #3939
- `product/features/crt-037/architecture/ADR-003-directional-dedup.md` — Unimatrix #3940

## Key Design Decisions

### Crates and Files Touched (3 crates, 5 files)
| Crate | File | Change |
|---|---|---|
| unimatrix-engine | graph.rs | Add RelationType::Informs variant; extend as_str() and from_str() |
| unimatrix-engine | graph_ppr.rs | Fourth edges_of_type call (Informs, Outgoing) in personalized_pagerank and positive_out_degree_weight |
| unimatrix-server | config.rs | 3 new InferenceConfig fields + validate() + default functions |
| unimatrix-server | nli_detection_tick.rs | NliCandidatePair struct, PairOrigin enum, Phase 4b, Phase 5 merged cap, Phase 8b write loop |
| unimatrix-store | read.rs | query_existing_informs_pairs directional query |

### NliCandidatePair Discriminator (ADR-001, #3938)
Module-private struct in nli_detection_tick.rs. PairOrigin enum: SupportsContradict / Informs. Ten fields: source_id, target_id, similarity, origin, source_category, target_category, source_feature_cycle, target_feature_cycle, source_created_at, target_created_at (last six are Option<T> for Informs pairs only). Eliminates SR-08 index-misalignment risk.

### Combined Cap Priority (ADR-002, #3939)
Sequential reservation in Phase 5: Supports/Contradicts get the full cap first; Informs get remaining_capacity = max_graph_inference_per_tick - supports_pairs.len(). No new config field. Cap-drop count logged at debug level (SR-03).

### Directional Dedup (ADR-003, #3940)
query_existing_informs_pairs returns directional (source_id, target_id) — no min/max normalization. Matches the directed edge semantic. INSERT OR IGNORE backstop preserved.

### Phase 4b Composite Guard
All five predicates required before writing Informs edge: (1) origin == Informs, (2) neutral > 0.5, (3) source_created_at < target_created_at, (4) source_feature_cycle != target_feature_cycle, (5) category pair in informs_category_pairs (verified in Phase 4b, implicit via origin tag in Phase 8b).

### PPR Direction (SR-07)
Informs edge A→B (lesson→decision). Direction::Outgoing in PPR implements reverse walk: when B (decision) is seeded, A (lesson) gains mass. Fourth edges_of_type call uses Direction::Outgoing — identical to the existing three calls. AC-05 must assert the lesson node specifically receives non-zero PPR mass.

### InferenceConfig New Fields
- informs_category_pairs: Vec<[String; 2]>, default 4 SW-eng pairs (frozen at v1)
- nli_informs_cosine_floor: f32, default 0.45, range (0.0, 1.0) exclusive
- nli_informs_ppr_weight: f32, default 0.6, range [0.0, 1.0] inclusive
Domain vocabulary lives only in default functions — never in detection logic (AC-22).

## Open Questions
None. All SCOPE.md OQs resolved. All SR-01 through SR-08 addressed.
