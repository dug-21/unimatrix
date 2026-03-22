# Agent Report: crt-024-agent-1-pseudocode

## Summary

Produced five pseudocode files for crt-024 (Ranking Signal Fusion — WA-0) covering all four
components across two files. All pseudocode was derived from the architecture, specification,
risk strategy, implementation brief, and direct inspection of the source files to be modified.

## Files Produced

| File | Lines | Component |
|------|-------|-----------|
| `pseudocode/OVERVIEW.md` | ~80 | Component interaction, data flow, wave plan |
| `pseudocode/inference-config.md` | ~160 | InferenceConfig fields + validation |
| `pseudocode/score-structs.md` | ~200 | FusedScoreInputs + FusionWeights structs |
| `pseudocode/compute-fused-score.md` | ~190 | compute_fused_score pure function |
| `pseudocode/search-service.md` | ~280 | SearchService pipeline rewrite |

## Components Covered

1. `InferenceConfig` additions — `infra/config.rs` (Wave 1)
2. `FusedScoreInputs` + `FusionWeights` structs — `services/search.rs` (Wave 2)
3. `compute_fused_score` pure function — `services/search.rs` (Wave 2)
4. `SearchService` pipeline rewrite — `services/search.rs` (Wave 2)

## Open Questions / Gaps

### OQ-1: SearchService::new signature for FusionWeights

The implementation agent must decide whether to thread `FusionWeights` into `SearchService::new`
as a parameter (constructed by the caller from `InferenceConfig`) or to pass `InferenceConfig`
directly and construct `FusionWeights` inside `new`. Both options are described in
`search-service.md`. The critical constraint is FR-14/AC-15: `EvalServiceLayer` must not use
default weights — it must pass the profile-specific `InferenceConfig`. The implementation agent
should inspect `EvalServiceLayer` construction before deciding.

### OQ-2: final_scores alignment after floor filtering

`search-service.md` describes two approaches (A: keep tuples together; B: split vecs). Approach A
is simpler and preferred, but requires restructuring the floor-filtering code more aggressively.
The implementation agent should pick A unless it conflicts with something in the floor step code
that was not visible in the reviewed portion of the file.

### OQ-3: confidence_weight in the NLI-absent fallback

With the fused formula, `confidence_weight` from `ConfidenceStateHandle` is no longer used in
the pipeline body — `entry.confidence` is used directly as a signal. The existing
`confidence_weight` snapshot at the top of `search()` may now be dead code in the pipeline path.
It must be retained if any test directly invokes the code path that uses it (e.g., the helper
`penalized_score` in tests references `rerank_score` with a hardcoded weight). Implementation
agent should check whether `confidence_weight` is still referenced after the rewrite and remove
the snapshot if not.

## ADRs Followed

- ADR-001: Six-term formula used (not four-term)
- ADR-002: `apply_nli_sort` removed; `try_nli_rerank` returns raw scores
- ADR-003: Default weights w_nli=0.35, w_sim=0.25, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05
- ADR-004: `compute_fused_score` as standalone pure function; `status_penalty` at call site

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `ranking scoring pipeline inference weights` (category: pattern) -- Entry #2964 (signal fusion pattern: sequential sort passes cause NLI override) directly describes the defect crt-024 fixes; Entry #724 (Behavior-Based Ranking Tests: assert ordering not scores) directly informs R-04 and R-05; Entry #2743 (Extract NLI sort logic as pure function) confirms the extraction pattern used in ADR-004.
- Queried: `/uni-query-patterns` for `crt-024 architectural decisions` (category: decision, topic: crt-024) -- All four ADRs found: ADR-001 through ADR-004, confirming architecture and pseudocode are aligned.
- Deviations from established patterns: none. The extraction of `compute_fused_score` as a pure function follows the same pattern established by `apply_nli_sort` in crt-023 (entry #2743). The `FusionWeights::effective()` re-normalization pattern mirrors the lambda dimension re-normalization from the Coherence Gate (entry #179, ADR-003 lambda dimensions).
