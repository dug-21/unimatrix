# Scope Risk Assessment: vnc-020

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `path` mode uses in-memory `TypedRelationGraph` (tick-window staleness) while `inverse` and `filter` use live SQL — three modes on one tool, two freshness contracts. Agents may apply the wrong mental model. | High | High | Architect must produce exact tool-description text for `path` mode staleness disclosure before pseudocode. Pattern #4474: vague language causes agent misbehavior. ADR-004 vnc-019 (#4493) is the model to follow. |
| SR-02 | `filter` mode dynamic SQL clause construction (variable number of property filters + correlated subquery) is prone to double-count and deprecated-endpoint bugs under multi-JOIN scenarios — pattern from col-029 (#3621). | Med | Med | Spec must include explicit trace scenarios for `filter` mode: one with all-active entries, one with deprecated-endpoint edges. Gate 3a reviewer must verify counts before approving pseudocode. |
| SR-03 | `graph_read.rs` file-size limit (500 lines) forces module splits into `graph_read_inverse.rs`, `graph_read_filter.rs`, `graph_read_path.rs`. Module boundary decisions made at design time affect how `validate_no_unsupported_params` cross-references all six modes. | Low | High | Architect should specify which dispatch and validation logic stays in `graph_read.rs` vs. moves to sibling modules. Unclear boundaries risk duplicated validation or silent param acceptance. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `depth` field reuse for `path` mode (SCOPE.md §path-mode) while `max_depth` is owned by `subgraph` — and the scope also requires `depth` to be *rejected* on chain/current/subgraph/inverse/filter. Correcting the existing silent-ignore behavior is a behavior change that could break callers who currently pass `depth` to those modes (even if accidentally). | Med | Low | Spec must enumerate every mode's `depth` rejection behavior explicitly. Note that `depth` was previously silently ignored on non-`neighbors` modes — any existing caller relying on that tolerance will now get an error. |
| SR-05 | `resolve_supersessions` in `path` mode resolves endpoints before BFS, then again at intermediate hops. SCOPE.md §path-mode says "in addition to per-hop intermediate resolution" but intermediate resolution is not described in neighbors/subgraph ADRs. If intermediate resolution does not already exist in the graph traversal infrastructure, this is scope addition. | Med | Med | Architect should confirm whether per-hop intermediate resolution is already implemented in neighbors/subgraph modes or if `path` introduces it new. If new, it needs explicit spec coverage. |
| SR-06 | `inverse` mode AND semantics (entries missing ALL specified `missing_edge_types`) is a non-obvious default. Agents querying for entries missing ANY type will get unexpected narrow results. | Low | Med | Tool description and spec must state AND semantics explicitly with an example. Single-sentence clarification prevents repeated agent confusion. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | vnc-020 delivery is hard-blocked on vnc-019 merging first (SCOPE.md §Constraints C2). vnc-019 is described as "untracked" in the spawn context. If vnc-019 is delayed or has merge conflicts, vnc-020 delivery cannot begin regardless of design completion. | Med | Low | Design can proceed; delivery planning should confirm vnc-019 PR #597 is merged before scheduling implementation. No design action needed. |
| SR-08 | `GraphParams` gains 7+ new `Option<T>` fields for vnc-020 (`category`, `missing_edge_types`, `limit`, `min_age_days`, `max_confidence`, `min_confidence`, `min_edge_count`, `max_edge_count`). `validate_no_unsupported_params` must reject each field on every non-owning mode. Combinatorial rejection surface grows with each mode. Missed rejection = silent data corruption (wrong mode uses wrong param). | Med | Med | Spec must include a rejection matrix table: rows=params, columns=modes, cells=accept/reject. Tester must validate at least one wrong-mode rejection per new field. |

## Assumptions

- **SCOPE.md §Background / "Tick-window staleness"**: Assumes `TypedRelationGraph` is always populated within 30-60 seconds of a write. If the tick interval is configurable or was recently increased, the staleness window for `path` mode could be materially larger. The staleness disclosure text must match the actual configured tick interval, not a hardcoded claim.
- **SCOPE.md §Background / "Composite indexes already present"**: Assumes schema v27 is confirmed in the live DB when vnc-019 merges. If schema migration was skipped in any environment, `inverse` and `filter` will fall back to full table scans silently.
- **SCOPE.md §"path mode — petgraph BFS"**: Assumes `node_index_for(id)` is O(1) via `NodeIndex` map introduced in vnc-018. If that map is absent in vnc-019's `TypedRelationGraph` state, path mode has no efficient node lookup.

## Design Recommendations

- **SR-01**: Before spec is written, produce the exact staleness disclosure paragraph for the `path` mode tool description. Copy the ADR-004 vnc-019 (#4493) disclosure model verbatim and adapt it for `path` mode. Do not leave this to the implementer.
- **SR-02 + SR-08**: Include a param/mode rejection matrix in the specification. Trace the `filter` mode SQL against at least two explicit data scenarios (all-active, mixed deprecated) before Gate 3a. Reference lesson #3621.
- **SR-04**: Spec must list every mode and its `depth` stance (accept / reject with error / was-silently-ignored-now-rejected) so the tester can write one AC per affected mode.
- **SR-05**: Architect should resolve whether per-hop `resolve_supersessions` in `path` traversal requires new infrastructure or reuses what neighbors/subgraph already provide.
