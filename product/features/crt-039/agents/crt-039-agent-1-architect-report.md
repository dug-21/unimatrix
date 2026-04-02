# Agent Report: crt-039-agent-1-architect

## Outputs

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/crt-039/architecture/ARCHITECTURE.md`

### ADR Files

| File | Unimatrix ID |
|------|--------------|
| `product/features/crt-039/architecture/ADR-001-control-flow-split.md` | #4017 |
| `product/features/crt-039/architecture/ADR-002-composite-guard-simplification.md` | #4018 |
| `product/features/crt-039/architecture/ADR-003-cosine-floor-raise.md` | #4019 |

## Key Decisions

**ADR-001 — Control flow split (Option Z)**
`run_graph_inference_tick` is restructured internally — not split into two public functions.
Phase 4b runs unconditionally as Path A. `get_provider()` moves to Path B entry point (after
Phase 4b + Informs write loop complete). The outer `if inference_config.nli_enabled` gate in
`background.rs:760` is removed. Function signature unchanged. SR-04 is structurally enforced:
no code path from `get_provider() Err` reaches Phase 8.

**ADR-002 — Composite guard simplification**
`apply_informs_composite_guard` loses 3 of 5 guards. Signature drops `nli_scores` and `config`
parameters. Two guards retained: temporal ordering and cross-feature. Guards 1/4/5 (all NLI)
are removed. `NliCandidatePair::Informs` and `PairOrigin::Informs` variants are removed as dead
code. `format_nli_metadata_informs` replaced by `format_informs_metadata` recording structural
fields only.

**ADR-003 — Cosine floor raise**
`default_nli_informs_cosine_floor` raised from 0.45 to 0.50. Inclusive floor semantics preserved.
No validation change. Implementor must run corpus pre-condition measurement (SR-02) before
committing the default.

## SR-03 Resolution (candidate set separation)
Informs and Supports candidate sets are disjoint by construction: Phase 4 uses strict `>`
at 0.50, Phase 4b uses inclusive `>=` at 0.50 with category pair filter. The category filter
is the structural separator — the same pair cannot appear in both sets because Supports is
category-agnostic and Informs requires `informs_category_pairs` membership.

## SR-01 Resolution (dedup pre-filter)
`query_existing_informs_pairs` is loaded in Phase 2 unconditionally. The directional
`contains(&(source_id, neighbor_id))` check is evaluated inside Phase 4b candidate
construction, before any candidate enters `informs_metadata`. The Phase 5 cap (25 slots)
applies to net-new candidates only — already-written pairs do not consume slots.

## Open Questions
None. SR-02 (corpus measurement) and SR-06 (observability log line) are implementation-phase
risk mitigations, not architectural blockers. The spec writer should formalize SR-06 as an
observability AC.
