# Agent Report: crt-037-agent-1-pseudocode

## Task

Produce per-component pseudocode files for crt-037 (Informs edge type — cross-feature
institutional memory bridge). Five components across three crates, two waves.

## Output Files

- `/workspaces/unimatrix/product/features/crt-037/pseudocode/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-037/pseudocode/graph.md`
- `/workspaces/unimatrix/product/features/crt-037/pseudocode/config.md`
- `/workspaces/unimatrix/product/features/crt-037/pseudocode/read.md`
- `/workspaces/unimatrix/product/features/crt-037/pseudocode/graph_ppr.md`
- `/workspaces/unimatrix/product/features/crt-037/pseudocode/nli_detection_tick.md`

## Components Covered

1. `graph.rs` (unimatrix-engine) — Wave 1: sixth RelationType variant, as_str/from_str arms, doc comment
2. `config.rs` (unimatrix-server) — Wave 1: three InferenceConfig fields with serde defaults and validate()
3. `read.rs` (unimatrix-store) — Wave 1: query_existing_informs_pairs directional dedup
4. `graph_ppr.rs` (unimatrix-engine) — Wave 2: fourth edges_of_type call in both PPR functions
5. `nli_detection_tick.rs` (unimatrix-server) — Wave 2: NliCandidatePair union, InformsCandidate struct,
   Phase 4b scan, Phase 5 cap, Phase 6 merged fetch, Phase 7 zip, Phase 8b write loop,
   format_nli_metadata_informs helper

## Open Questions Surfaced

**OQ-1 (config.rs validate error variant):** The pseudocode specifies
`ConfigError::NliFieldOutOfRange { path, field, value, min, max }` for the two new
validate() checks. The implementer must confirm this variant exists by inspecting the
existing `ConfigError` enum. If the shape differs (e.g., different field names or a more
generic variant), use the closest existing variant. Do not invent new error variants.

**OQ-2 (EntryRecord.created_at type in InformsCandidate):** `EntryRecord.created_at` is
`u64` in the schema (as confirmed in `read.rs` `entry_from_row`). `InformsCandidate` stores
it as `i64` for the temporal comparison (`source_created_at < target_created_at`). The cast
`created_at as i64` is safe for any realistic Unix timestamp. The implementer should verify
this is consistent with how other code uses `created_at` comparisons.

**OQ-3 (Phase 8 write path refactoring):** The pseudocode proposes replacing the
`write_inferred_edges_with_cap` call in Phase 8 with inline pattern matching on
`merged_pairs`. This changes Phase 8 from calling a private helper to iterating directly.
The helper can be preserved if the implementation agent prefers — both approaches produce
identical behavior. Either choice is valid. If the helper is preserved, Phase 8b must still
use inline pattern matching (the helper only handles `(u64, u64)` pairs with a flat score).

**OQ-4 (PairOrigin enum placement):** The pseudocode introduces a `PairOrigin` enum used
only during Phase 6 construction scaffolding. It is consumed when building
`Vec<NliCandidatePair>` at the end of Phase 7. This enum is a local implementation detail
not mentioned in the architecture. The implementer may instead use a tuple `(InformsCandidate,
NliScores)` and `((u64, u64, f32), NliScores)` approach, or keep it as the `PairOrigin` enum.
The critical constraint is: `NliCandidatePair` must be the tagged union as specified —
`PairOrigin` is just construction scaffolding.

## Deviations from Established Patterns

None. All pseudocode follows established patterns:
- `edges_of_type` boundary (crt-021, entry #2417) — respected; no `.edges_directed()` calls added
- `InferenceConfig` field pattern (crt-034, entry #3826) — followed exactly
- `query_existing_supports_pairs` mirror — `query_existing_informs_pairs` uses same structure
- W1-2 contract — rayon closure body unchanged; single spawn preserved
- Tick phase structure (crt-029, entry #3656) — extended, not restructured

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` (NLI inference graph edge patterns, category=pattern)
  — found #3937 (neutral-zone detection signal), #3675 (candidate selection pattern), #3727
  (nli_score_stats as pure helper), #3884 (graph edge INSERT-OR-IGNORE pattern)
- Queried: `mcp__unimatrix__context_search` (crt-037 architectural decisions, category=decision)
  — found #3939 (ADR-002 combined cap), #3940 (ADR-003 directional dedup), #3942 (ADR-001
  tagged union — corrected)
- Entry #3942 (ADR-001 corrected) confirmed NliCandidatePair must be a tagged enum with
  SupportsContradict and Informs variants — the flat-struct with Option fields mentioned in
  the ARCHITECTURE.md first draft is superseded by this ADR.
- Deviations from established patterns: none
