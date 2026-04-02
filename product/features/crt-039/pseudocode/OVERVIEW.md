# crt-039 Pseudocode Overview
# Tick Decomposition: Decouple Structural Graph Inference from NLI Gate

## Components Involved

| Component | File | Change Type |
|-----------|------|-------------|
| Tick orchestrator | `crates/unimatrix-server/src/background.rs` | Remove outer `nli_enabled` gate; add comments |
| Tick implementation | `crates/unimatrix-server/src/services/nli_detection_tick.rs` | Option Z control-flow split; remove dead enum variants; simplify guard |
| Config defaults | `crates/unimatrix-server/src/infra/config.rs` | Raise `nli_informs_cosine_floor` default 0.45 -> 0.50 |

## Why Each Component Is Touched

**background.rs**: The outer `if inference_config.nli_enabled` guard at line 760 prevents
`run_graph_inference_tick` from ever being called in production (default: `nli_enabled=false`).
Removing it makes the structural Phase 4b path run on every tick. Comments are added to make
the contradiction scan's independent gate and the overall tick ordering invariant explicit.

**nli_detection_tick.rs**: The inner Phase 1 guard (`get_provider()` early-return at line 129)
blocks the entire function on the same NLI availability condition. Restructuring as Option Z
(internal two-path split) allows Phase 4b to run unconditionally while Phase 8 (Supports)
remains NLI-gated. Associated dead code — `NliCandidatePair::Informs`, `PairOrigin::Informs`,
`apply_informs_composite_guard` NLI params, `format_nli_metadata_informs` — is removed.

**config.rs**: The `nli_informs_cosine_floor` default of 0.45 was calibrated assuming guard 1
(`nli_scores.neutral > 0.5`) would act as a quality filter in the 0.45-0.50 band. After guard 1
is removed, the floor itself becomes the sole semantic threshold. 0.50 aligns it with
`supports_candidate_threshold` and eliminates low-confidence connections.

## Data Flow Between Components

```
background.rs / run_single_tick
  |
  | calls unconditionally (no nli_enabled gate after crt-039)
  v
nli_detection_tick.rs / run_graph_inference_tick(store, nli_handle, vector_index, rayon_pool, config)
  |
  |-- Phase 2: DB reads (active entries, isolated IDs, existing pairs) ← same for both paths
  |-- Phase 3: source candidate selection
  |-- Phase 4: HNSW Supports candidates (cosine > supports_candidate_threshold)
  |-- Phase 4b: HNSW Informs candidates (cosine >= config.nli_informs_cosine_floor = 0.50)
  |             reads: config.nli_informs_cosine_floor ← from InferenceConfig (config.rs)
  |             explicit subtract: remove Phase 4 candidate_pairs from Informs set
  |-- Phase 5: caps (MAX_INFORMS_PER_TICK=25 from module const; max_graph_inference_per_tick from config)
  |
  |== PATH A (unconditional) ==
  |-- Phase 8b: write Informs edges via write_nli_edge for each InformsCandidate passing
  |             apply_informs_composite_guard (temporal + cross-feature only)
  |
  |== PATH B (gated by get_provider()) ==
  |   if candidate_pairs.is_empty(): return (no Supports work to do)
  |   get_provider() -> Err: return (no Phase 6/7/8 writes)
  |   get_provider() -> Ok(provider):
  |-- Phase 6: text fetch for Supports candidates only (SupportsContradict PairOrigins only)
  |-- Phase 7: rayon NLI score_batch (Supports only, W1-2 contract)
  |-- Phase 8: write Supports edges (entailment > threshold)
```

## Key Type Changes

### Removed: `NliCandidatePair::Informs` variant

Before:
```
enum NliCandidatePair {
    SupportsContradict { source_id, target_id, cosine, nli_scores }
    Informs { candidate: InformsCandidate, nli_scores }
}
```
After:
```
enum NliCandidatePair {
    SupportsContradict { source_id, target_id, cosine, nli_scores }
}
```

### Removed: `PairOrigin::Informs` variant

Before: `PairOrigin::SupportsContradict { ... } | Informs(InformsCandidate)`
After: `PairOrigin::SupportsContradict { source_id, target_id, cosine }` only

### Simplified: `apply_informs_composite_guard` signature

Before: `fn apply_informs_composite_guard(nli_scores: &NliScores, candidate: &InformsCandidate, config: &InferenceConfig) -> bool`
After: `fn apply_informs_composite_guard(candidate: &InformsCandidate) -> bool`

### New: `format_informs_metadata` (replaces `format_nli_metadata_informs`)

`fn format_informs_metadata(cosine: f32, source_category: &str, target_category: &str) -> String`
Emits JSON: `{ "cosine": f32, "source_category": str, "target_category": str }`
No NLI score fields. Old function `format_nli_metadata_informs` is deleted.

### Unchanged: `InformsCandidate` struct

All 9 fields retained. Consumed directly in Path A write loop. No Option fields.

### Unchanged: `run_graph_inference_tick` public signature

`pub async fn run_graph_inference_tick(store, nli_handle, vector_index, rayon_pool, config)`

## Sequencing Constraints

1. `config.rs` change is independent — can be implemented in any order.
2. `nli_detection_tick.rs` changes are self-contained within the file.
3. `background.rs` outer gate removal is the final mechanical step after nli_detection_tick.rs
   restructuring confirms the inner Phase 1 guard is moved to Path B entry.

## File Size Assessment (NFR-06 / OQ-04)

The production code section of `nli_detection_tick.rs` currently ends at line ~898 of 2163
(tests at 899+). That is already ~898 lines of production code, well above the 500-line
guidance. However, crt-039 is net-negative: removes Phase 6 Informs text-fetch (~30 lines),
Phase 1 guard (~4 lines), PairOrigin::Informs (~3 lines), NliCandidatePair::Informs (~5 lines),
apply_informs_composite_guard 3 guards (~6 lines), format_nli_metadata_informs (~8 lines),
Phase 8b NliCandidatePair::Informs match pattern (~20 lines). Adds: Path A Informs write loop
(~20 lines), format_informs_metadata (~6 lines), observability log (~8 lines), explicit
subtraction (~5 lines). Net: approximately -43 lines. File stays well under a theoretical 900
line limit and does not cross any new 500-line boundary requiring extraction. Submodule split
deferred per ADR-001 OQ-04: "no extraction needed as crt-039 is net-removal."
