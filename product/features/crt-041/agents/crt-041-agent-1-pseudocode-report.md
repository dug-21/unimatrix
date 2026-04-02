# Agent Report: crt-041-agent-1-pseudocode

## Summary

Produced five pseudocode files for crt-041 graph enrichment feature.
All components covered. One critical deviation from the IMPLEMENTATION-BRIEF
was found in the existing codebase — documented below.

## Files Produced

- `product/features/crt-041/pseudocode/OVERVIEW.md`
- `product/features/crt-041/pseudocode/edge_constants.md`
- `product/features/crt-041/pseudocode/config.md`
- `product/features/crt-041/pseudocode/graph_enrichment_tick.md`
- `product/features/crt-041/pseudocode/background.md`

## Components Covered

1. `graph_enrichment_tick` — new module with `run_graph_enrichment_tick`, `run_s1_tick`,
   `run_s2_tick`, `run_s8_tick`, `S8_WATERMARK_KEY`, row types, helper
2. `config` — five new InferenceConfig fields at four modification sites (struct, Default,
   default_*() functions, validate(), merge_configs())
3. `edge_constants` — EDGE_SOURCE_S1/S2/S8 in read.rs, re-exported from lib.rs
4. `background` — import and call site in run_single_tick, updated ordering comment,
   services/mod.rs module registration

## Critical Finding: `write_graph_edge` Signature Deviation

The IMPLEMENTATION-BRIEF specifies:
```
weight: f64, ..., metadata: Option<&str>
```

The actual crt-040 implementation at `services/nli_detection.rs:78` uses:
```
weight: f32, ..., metadata: &str
```

Additionally, `created_by` is NOT a separate parameter — the SQL binds `source` to
both `?6` slots, so `created_by` always equals `source`. The per-source `created_by`
values listed in the IMPLEMENTATION-BRIEF table ('s1', 's2', 's8') will not be written;
the column will contain 'S1', 'S2', 'S8' respectively. This is acceptable for GNN feature
construction since the `source` column is the authoritative discriminator.

All call sites in the pseudocode use the actual signature:
- `weight as f32` (not f64)
- `""` for metadata (empty string, not None)

Delivery agent MUST verify this against the live file before writing call sites.

## Prerequisite Gate Result

`write_graph_edge` EXISTS in `crates/unimatrix-server/src/services/nli_detection.rs`.
Confirmed by grep. The crt-040 prerequisite gate passes. No conditional add is needed.

## Open Questions

None. All design decisions in the ARCHITECTURE.md and IMPLEMENTATION-BRIEF.md are
resolved. The only deviation is the `write_graph_edge` signature finding above,
which is documented in OVERVIEW.md and graph_enrichment_tick.md.

The S1 GROUP BY materialization concern (R-04 / OQ-01) is marked in the pseudocode
with a note that delivery agent must run EXPLAIN QUERY PLAN and document the result
in the PR description. This is not a blocker for pseudocode but is a mandatory
implementation verification step.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for "background tick implementation patterns
  co_access promotion" (category: pattern) — found #3897 (helper extraction pattern for
  infallible bidirectional tick writes), #3822 (near-threshold oscillation), #1542
  (consecutive counter error semantics)
- Queried: `mcp__unimatrix__context_search` for "crt-041 architectural decisions"
  (category: decision, topic: crt-041) — found ADRs #4031, #4034, #4035
- Queried: `mcp__unimatrix__context_briefing` for full context — found all five crt-041
  ADRs and confirmed write_graph_edge delegation contract from crt-040 (#4027)
- Deviations from established patterns:
  - `write_graph_edge` actual signature (`f32` weight, non-optional `&str` metadata) differs
    from the IMPLEMENTATION-BRIEF specification. This is a crt-040 as-shipped deviation,
    not a crt-041 design error. All pseudocode call sites reflect the actual signature.
  - No other deviations found. Module structure, infallible tick pattern, counters usage,
    QueryBuilder for dynamic SQL, and dual-endpoint quarantine guard all follow established
    codebase patterns.
