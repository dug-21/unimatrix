# Agent Report: crt-024-agent-1-architect

## Status: COMPLETE

## Files Produced

- `/workspaces/unimatrix/product/features/crt-024/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-024/architecture/ADR-001-six-term-formula-canonicalization.md`
- `/workspaces/unimatrix/product/features/crt-024/architecture/ADR-002-apply-nli-sort-removal.md`
- `/workspaces/unimatrix/product/features/crt-024/architecture/ADR-003-default-fusion-weights.md`
- `/workspaces/unimatrix/product/features/crt-024/architecture/ADR-004-formula-as-extractable-pure-function.md`

## ADRs Stored in Unimatrix

| Entry | Title |
|-------|-------|
| #2969 | ADR-001 crt-024: Six-Term Formula Canonicalization |
| #2970 | ADR-002 crt-024: apply_nli_sort Removal |
| #2971 | ADR-003 crt-024: Default Fusion Weights for the Six-Term Formula |
| #2972 | ADR-004 crt-024: Formula as Extractable Pure Function |

## Key Decisions

1. **Six-term formula is canonical** (ADR-001): The product vision's four-term formula was
   illustrative. The implementation target has six terms: sim, nli, conf, coac, util, prov.
   `utility_delta` normalization uses shift-and-scale `(val + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)`
   to map [-0.05, +0.05] → [0, 1].

2. **`apply_nli_sort` is removed** (ADR-002): `try_nli_rerank` returns `Option<Vec<NliScores>>`
   (raw scores per candidate) instead of a sorted/truncated vec. No intermediate sort at Step 7.
   All existing `apply_nli_sort` tests must be deleted and replaced with fused scorer tests.

3. **Default weights: nli=0.35, sim=0.25, conf=0.15, coac=0.10, util=0.05, prov=0.05**
   (ADR-003): Sum=0.95, leaving 0.05 headroom for WA-2. All three binding constraints
   verified numerically (AC-11, Constraint 9, Constraint 10).

4. **Formula as `compute_fused_score` pure function** (ADR-004): Extracted to
   `pub(crate) fn compute_fused_score(inputs: &FusedScoreInputs, weights: &FusionWeights) -> f64`.
   Status penalty applied by caller, not inside the function. WA-2 adds one field to each struct.

## Open Questions for Spec Writer

1. AC-05 and AC-10 must define `util_norm` normalization explicitly:
   `(utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)`.
2. AC-06 denominator must list all five remaining weights (not three), explicitly.
3. Spec should note `prov_norm` guard: if `PROVENANCE_BOOST == 0.0`, return 0.0 (no divide).
4. Co-access staleness cutoff computation moves with the prefetch — confirm no behavioral change intended.
5. `SearchService::new()` gains fusion weights; spec should specify whether to pass a
   `FusionWeights` struct or individual fields.
