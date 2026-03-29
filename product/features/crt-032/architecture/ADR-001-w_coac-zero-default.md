# ADR-001 crt-032: w_coac Default Zeroed — PPR Subsumes Co-Access Signal

**Status**: Accepted
**Feature**: crt-032
**Date**: 2026-03-29

## Context

The fusion scoring formula in `compute_fused_score` combines six weighted signals:

```
score = w_sim * sim + w_nli * nli + w_conf * conf + w_coac * coac_norm + w_util * util + w_prov * prov
```

`w_coac` weights the direct co-access affinity boost: for a given query's candidate set, pairs that co-occur frequently in the CO_ACCESS table receive a proportional additive lift.

When PPR (Personalized PageRank) was introduced in crt-030, co-access pairs were added as `GRAPH_EDGES.CoAccess` edges in the graph traversal. The PPR score therefore already incorporates co-access co-occurrence structure — the same signal that `w_coac * coac_norm` was providing — via graph propagation through `ppr_blend_weight`.

Phase 1 measurement (crt-030 follow-up, 2026-03-29) ran two stable eval rounds comparing three scoring profiles over 4,349 and 4,467 scenarios respectively:

| Profile | CC@5 | ICD | Avg latency |
|---------|------|-----|------------|
| pre-ppr | 0.4252 | 0.6376 | 7.8ms |
| ppr-plus-direct (w_coac=0.10) | 0.4252 | 0.6376 | 7.9ms |
| ppr-only (w_coac=0.0) | 0.4252 | 0.6376 | 7.8ms |

Zero measurable difference between ppr-plus-direct and ppr-only in aggregate metrics (CC@5, ICD) and per-query analysis. The direct co-access boost is redundant.

## Decision

Change `default_w_coac()` to return `0.0` and update the compiled-defaults struct literal to `w_coac: 0.0`.

The `w_coac` field remains in `InferenceConfig` — operators may still set it above `0.0` via config file if they wish. Only the compiled default changes.

## PPR as the Sole Co-Access Carrier

With `w_coac=0.0` (default), `ppr_blend_weight` is the sole mechanism by which co-access signal influences scoring. This means:

- When PPR is enabled (default): co-access signal is present via graph edges in the PPR score, blended at `ppr_blend_weight` (default: see crt-030 ADR-007).
- When PPR is disabled by an operator (`ppr_blend_weight=0.0`): co-access signal is absent from scoring entirely. This is a valid operator choice and is accepted — it is not a regression from this feature since the operator must explicitly zero out PPR.

ADR-007 from crt-030 (Unimatrix entry #3737) documents the dual role of `ppr_blend_weight` as both a similarity blending factor and the effective carrier of graph-derived signals including co-access.

## Consequences

**Easier:**
- Query latency: `compute_search_boost` spawn_blocking call still runs (Phase 3 removes it), but its result contributes zero to the score — effectively a no-op at scoring time.
- Config: operators do not need to change config files; the field default changes without breaking existing explicit configurations.
- Test expectations: default-assertion tests now expect `0.0`; the sum drops from 0.95 to 0.85, comfortably within the `≤ 1.0` invariant.

**Harder / Watch For:**
- `compute_search_boost` still executes on every search query despite contributing nothing. This wasted work is accepted for Phase 2 and deferred to Phase 3 (code removal).
- Operators who have explicitly set `w_coac > 0.0` in a config file retain that behaviour — but this creates a regime where direct co-access and PPR co-access are both active, potentially double-counting the signal. The operator is responsible for that choice.

## Rejected Alternatives

**Keep w_coac=0.10**: The Phase 1 measurement shows zero benefit. The redundant `compute_search_boost` call wastes CPU on every search query with no scoring benefit.

**Remove w_coac field entirely**: Premature — Phase 3 handles the full cleanup including compute_search_boost call removal. Removing the field now would require more invasive changes than the default zeroing.

## Relationship to Prior ADRs

- ADR-001 crt-013 (Unimatrix #701): removed the `W_COAC` constant (static weight). crt-032 is the next step: zeroing the default of the configurable `w_coac` field. Phase 3 will complete the cleanup.
- ADR-007 crt-030 (Unimatrix #3737): established `ppr_blend_weight` as the sole PPR control parameter. This ADR explicitly relies on that decision — PPR carries co-access signal, making the direct additive term redundant.
