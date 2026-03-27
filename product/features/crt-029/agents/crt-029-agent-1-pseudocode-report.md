# Agent Report: crt-029-agent-1-pseudocode

## Task

Produce per-component pseudocode files for crt-029 — Background Graph Inference (Supports Edges).

## Files Produced

| File | Lines | Notes |
|------|-------|-------|
| `product/features/crt-029/pseudocode/OVERVIEW.md` | ~80 | Component map, data flow, shared types, sequencing constraints |
| `product/features/crt-029/pseudocode/inference-config.md` | ~200 | Four new fields, Default, validate(), merge, pub(crate) promotions, mod.rs declaration |
| `product/features/crt-029/pseudocode/store-query-helpers.md` | ~150 | Two new Store methods, SQL, normalisation contract, test cases |
| `product/features/crt-029/pseudocode/nli-detection-tick.md` | ~320 | Primary deliverable: run_graph_inference_tick (8 phases), select_source_candidates, write_inferred_edges_with_cap |
| `product/features/crt-029/pseudocode/background-call-site.md` | ~100 | Two-line insertion, import, sequencing invariant, test scenarios |

## Components Covered

1. **inference-config** — `infra/config.rs` four new fields + Default + validate + merge; `nli_detection.rs` three pub(crate) promotions; `services/mod.rs` pub mod declaration
2. **store-query-helpers** — `unimatrix-store/src/read.rs` two new async query methods
3. **nli-detection-tick** — `services/nli_detection_tick.rs` new module (three functions + inline tests)
4. **background-call-site** — `background.rs` two-line insertion after bootstrap promotion

## Self-Check Results

- [x] Architecture read before writing any pseudocode
- [x] No invented interface names — all names traced to ARCHITECTURE.md integration surface and codebase grep
- [x] Output is per-component (OVERVIEW.md + four component files), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO or TBD sections — all gaps flagged explicitly with IMPLEMENTATION NOTE markers
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/crt-029/pseudocode/`

## Critical Constraints Called Out

- **C-13 / AC-10a**: `write_inferred_edges_with_cap` is Supports-only — no `contradiction_threshold` parameter, no Contradicts writes. Called out in module header, function pseudocode, and test scenario AC-10a.
- **C-14 / R-09**: Rayon closure in Phase 7 is sync-only. Called out with explicit CRITICAL block in the tick pseudocode, in the module doc comment, and as a pre-merge grep gate.
- **AC-06c / R-02**: Source candidate cap applied in Phase 3 (metadata only) before Phase 4 calls `get_embedding`. Ordering is explicit in the pseudocode with phase comments.
- **C-11 / R-07**: 52 `InferenceConfig {` occurrences require update. Flagged in inference-config.md and OVERVIEW.md.

## Implementation Notes Flagged

1. **`NliThresholdInvariantViolated` error variant**: Two options (reuse with repurposed field names vs. new variant `GraphInferenceThresholdInvariantViolated`). Delivery agent should prefer Option B for clarity.

2. **`write_nli_edge` return semantics**: The existing implementation returns `true` even for `INSERT OR IGNORE` conflicts. Implementation agent must verify whether this means duplicate rows count against the cap.

3. **Import paths for `EntryRecord`, `NliScores`, `CrossEncoderProvider`**: Must be verified against existing `nli_detection.rs` imports to avoid divergence.

4. **`pub(crate) mod` vs `pub mod`**: Pseudocode recommends `pub(crate) mod nli_detection_tick;` to match siblings, based on the pattern that `background.rs` uses `use crate::services::...`.

5. **Pair normalisation contract**: `query_existing_supports_pairs()` normalises pairs to `(min, max)` at the SQL-to-Rust boundary. Phase 4 deduplication must use the same normalisation. Both pseudocode files document this contract explicitly.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 15 entries. Key findings: entries #3655, #3653, #3656–3659, #2728, #2730, #2800, #3591 confirmed architectural patterns used in this pseudocode. Entry #3655 (two-bound pattern for tick candidate cap before embedding) is the primary basis for Phase 3 / Phase 4 ordering. Entry #3653 (single rayon dispatch per tick) confirmed the W1-2 single-spawn constraint. Entry #3591 confirmed `EDGE_SOURCE_NLI` constant location.
- Queried: `mcp__unimatrix__context_search(category=pattern)` — confirmed entries #3655, #3653, #2728 as most relevant.
- Queried: `mcp__unimatrix__context_search(category=decision, topic=crt-029)` — all four ADRs (#3656–3659) confirmed consistent with architecture documents.
- Deviations from established patterns: none. All patterns follow existing conventions:
  - `maybe_run_bootstrap_promotion` as precedent for tick guard and infallible error handling
  - `write_edges_with_cap` as precedent for cap pattern (named variant, not reuse)
  - `query_by_status` as precedent for store query error propagation
  - Entry #2742 (collect owned data before rayon spawn) applied in Phase 7
