# crt-030 Researcher Report

## Summary

Researched the problem space for Personalized PageRank (PPR) over positive graph edges in
`context_search`. SCOPE.md written to `product/features/crt-030/SCOPE.md`.

## Key Findings

### 1. No prior work on #396 or #398 in codebase

Neither depth-1 Supports expansion (#396) nor PPR (#398) has any code landed. Both issues are
open. PPR supersedes #396 entirely — implementing crt-030 closes both.

### 2. TypedRelationGraph is fully ready for PPR

All five edge types (Supersedes, Contradicts, Supports, CoAccess, Prerequisite) are loaded into
`TypedRelationGraph.inner` at tick rebuild time via `build_typed_relation_graph` Pass 2b.
`CoAccess` edges from `GRAPH_EDGES` are present in the in-memory graph and currently unused by
retrieval. `petgraph = "0.8"` with `stable_graph` feature is already declared in
`unimatrix-engine/Cargo.toml`.

### 3. Search pipeline insertion point is Step 6d

Pipeline steps: 5(HNSW) → 6(fetch/quarantine) → 6a(penalty) → 6b(injection) → 6c(co-access
prefetch) → **6d(PPR, new)** → 7(NLI+fused score+sort+truncate) → 9(truncate) → 10(floors)
→ 10b(Contradicts suppression). The `use_fallback` guard (checked before 6a) must also gate 6d.

### 4. InferenceConfig pattern is well-established

crt-024, crt-026, crt-029, col-031 all added fields using the same pattern: `serde(default)` +
private `default_fn()` + `validate()` range check via `ConfigError::NliFieldOutOfRange` +
`Default::default()` update. The four PPR fields follow this exactly.

### 5. suppress_contradicts is the structural model for graph_ppr.rs

`graph_suppression.rs` is a 327-line submodule of `graph.rs`: pure function, `edges_of_type`
only, re-exported from `graph.rs`, unit tests inline. `graph_ppr.rs` follows the same pattern.

### 6. Edge direction for Supports in PPR is the key design open question

`Supports` edge `A→B` means A supports B. For PPR to surface A when B is a seed, traversal on
B must follow `Direction::Incoming` to find A. This "backward propagation from seeds" is the
intended semantics but needs explicit confirmation in the spec. The issue #398 algorithm
description is ambiguous on direction.

### 7. CoAccess edge directionality

CoAccess edges in `GRAPH_EDGES` are bidirectional (A→B and B→A rows both present from
bootstrap). Both directions must be traversed for CoAccess, same as Contradicts in
`suppress_contradicts`.

## Risks

- **Pool explosion**: PPR may surface many entries above threshold on dense graphs. A cap on
  new-entry count per query is not specified in #398 but is likely needed to bound latency.
  Open question 3 in SCOPE.md.
- **Fetch latency for PPR-surfaced entries**: sequential `entry_store.get()` calls for each
  new PPR entry are within `MCP_HANDLER_TIMEOUT` for small additions, but could be an issue
  if the pool grows by 50+ entries. Should be monitored or capped.
- **ppr_blend_weight + w_sim interaction**: blending modifies the `similarity` input before
  `compute_fused_score` applies `w_sim`. This is a non-obvious coupling. Open question 5 in
  SCOPE.md covers an alternative (additive offset to final score).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- returned 12 entries; entry #3650 (TypedRelationGraph pattern) was directly useful; entries #3659, #3658 (crt-029 ADRs) provided graph inference tick context.
- Stored: entry #3730 "Search pipeline step numbering, graph traversal module pattern, and use_fallback guard for new pipeline steps" via /uni-store-pattern
