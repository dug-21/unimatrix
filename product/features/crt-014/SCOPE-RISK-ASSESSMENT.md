# Scope Risk Assessment: crt-014

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | petgraph API surface is larger than needed — using only `stable_graph` but future contributors may reach for other features (`graphmap`, `matrix_graph`, `rayon`) without noticing the intentional feature restriction | Low | Med | Architect must document the feature restriction explicitly in `Cargo.toml` with a comment; spec must include a constraint against enabling additional petgraph features |
| SR-02 | Per-query full-store graph rebuild reads ALL entries from redb on every `context_search` call — at 500 entries this is ~1-2ms, but the cost grows linearly; current read path may not be optimized for full-table scans | Med | Med | Architect should clarify whether the full-store read uses an existing batch API or requires a new one; identify if `Store::list_all()` or equivalent exists |
| SR-03 | `graph_penalty` formula is judgment-call coefficients (just named constants now, not empirically derived) — if the ordering is wrong (e.g., orphan penalty ends up harsher than 1-hop penalty in practice), retrieval quality regresses silently | Med | Low | Spec must include behavioral ordering assertions (AC-06, AC-07, AC-08) as the primary correctness guarantee; architect must ensure `graph_penalty` is a pure function that can be unit tested in isolation |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | "Full multi-hop successor resolution" is underspecified for chains longer than 3 hops — the ASS-017 examples only go to depth 2; unbounded DFS over a large graph could exceed expected ~1-2ms budget | Med | Low | Architect must define a max traversal depth (e.g., depth cap at 10) as a defensive bound; it should be a named constant in `graph.rs` |
| SR-05 | Cycle fallback behavior (log + use flat constant) re-introduces the removed `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` constants in disguise — the fallback value needs to be defined somewhere | Low | Med | Spec must define the fallback constant value explicitly (e.g., same numeric values, or a new `CYCLE_FALLBACK_PENALTY`) so the constant removal is clean; architect should locate fallback constants in `graph.rs` not `confidence.rs` |
| SR-06 | `context_status` surface for cycle reporting is out of scope for crt-014 but the human's answer to OQ-4 requires it — this is a dependency on the status service interface | Med | Med | Architect must determine if cycle reporting into `context_status` requires a new field in the status response struct or is log-only; if struct change, flag as potential schema coupling |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Existing search tests (`search.rs:450–571`) assert exact `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` values — removing constants breaks these tests; constant removal and test migration are tightly coupled | High | High | Spec must enumerate which tests are deleted vs replaced; implementation must migrate tests atomically (no window where penalty tests are absent) |
| SR-08 | `find_terminal_active` in search changes which entry is injected as a successor — existing integration tests that assert specific injected successor IDs (A→B single-hop) will assert the wrong result (A→B→C multi-hop) | High | Med | Architect must identify all call sites and tests that depend on single-hop injection; spec must include regression scenarios for 1-hop chains to confirm existing behavior is preserved where the chain is already terminal |
| SR-09 | crt-017 (Contradiction Cluster Detection) depends on crt-014 being complete — the graph module's public API surface becomes a contract for crt-017 | Med | Low | Architect should design `graph.rs` public API with crt-017's likely needs in mind (contradiction edges will be a second edge type); avoid sealing the graph behind an opaque type |

## Assumptions

| Assumption | SCOPE.md Section | Risk if Wrong |
|-----------|-----------------|---------------|
| A store read of all entries for graph construction costs ~1-2ms | Proposed Approach | SR-02: graph construction becomes a search latency regression |
| `EntryRecord.supersedes`/`superseded_by` are consistently populated (no dangling references) | Constraints | `build_supersession_graph` silently drops edges for dangling refs; graph topology is incomplete |
| No existing supersession chains in production data contain cycles | Background Research | SR-03: cycle fallback fires on first production query; penalty constants must exist for fallback |

## Design Recommendations

- **SR-02, SR-04**: Architect must check if `Store` has a batch-all-entries read API. If not, this needs to be added in the engine layer (not a store schema change — a new query path). Cap traversal depth at a named constant.
- **SR-07, SR-08**: Spec writer must enumerate all affected test IDs from `confidence.rs` and `search.rs` so implementation agents can migrate atomically.
- **SR-05**: Fallback penalty constants belong in `graph.rs` alongside the `graph_penalty` function — not in `confidence.rs`. Keep `confidence.rs` clean of any penalty constants after crt-014.
- **SR-06**: If cycle reporting in `context_status` requires a struct field change, flag it to the architect as potential scope expansion; the human's intent may be log-only.
- **SR-09**: Design `graph.rs` public API with `SupersessionGraph` as a named type with room for additional edge types in crt-017.
