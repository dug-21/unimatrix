# Scope Risk Assessment: vnc-018

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | SQLite recursive CTEs are used for `chain`/`current` modes. SQLite CTE performance degrades if the two new `entries` indexes (`idx_entries_supersedes`, `idx_entries_superseded_by`) are not applied before any CTE executes — e.g., if migration ordering puts them after tool registration. | High | Med | Architect must ensure migration adds the four indexes as a single atomic step before any `context_graph` handler is reachable. |
| SR-02 | `neighbors` depth=1 (SQL) vs depth>1 (in-memory graph) is a behavioral asymmetry baked into the scope. Agents calling `depth=2` immediately after a `context_edge` write will silently miss the new edge for up to one tick. This is documented as intended, but the tool description must be precise — vague language risks agents building incorrect mental models of freshness. | Med | High | Architect must produce exact tool description text for the behavioral split. Spec must include a test that writes an edge, immediately queries depth=2, and asserts the expected staleness behavior (not a bug). |
| SR-03 | `GraphParams` and `EdgeRecord` are forward-compat contracts for #597 and #598. Fields added now but unused (`seed_ids`, `from_id`, `to_id`, `max_nodes`, `metadata: Option<serde_json::Value>`) must not be silently dropped by serde — they must round-trip correctly or error on misuse. If #597/#598 later find the struct layout inadequate, the type change is a breaking wire contract change. | High | Med | Architect must lock the struct layout via a Unimatrix ADR before delivery begins. Forward-compat fields must be tested for correct error-on-misuse behavior in vnc-018, not deferred to #597. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `Supersedes` exclusion from `neighbors` has two paths: explicit request (error) and "all types" default (silent exclusion). The silent-exclusion path could cause confusion if an agent passes `edge_types=[]` expecting all types including supersession, and receives neighbors that omit what they consider structurally important edges — without any warning. The scope mandates silence here but the UX risk is real. | Med | Med | Spec must include a clear AC that silent exclusion produces no warning in the result payload. Architect should consider whether the response envelope should carry an `excluded_types: ["Supersedes"]` field to aid debuggability — or explicitly reject it. |
| SR-05 | The `truncated: bool` field on `chain` mode is required when the 50-hop cap fires. AC-03b requires distinguishing _which direction_ was capped. The scope says the response "must include a `truncated: bool` field" but a single bool cannot communicate per-direction truncation. If spec interprets this as a single bool, AC-03b is untestable. | High | High | Spec must define a `truncated` response structure that encodes per-direction truncation (e.g., `truncated: { forward: bool, backward: bool }`) — not a flat bool. This is a scope ambiguity that must be resolved before pseudocode. |
| SR-06 | `Advances` and `Motivates` PPR/BFS addition is bundled into vnc-018. These two changes touch `graph_ppr.rs` and `graph_expand.rs` — files outside the new `mcp/graph_read.rs` module. A scope-bundled change to PPR positive types could alter search re-ranking behavior for existing queries in ways that are not covered by the `context_graph` integration tests. | Med | Med | Spec must include unit tests specifically for the PPR and BFS changes (ACs 17–18 already require this). Delivery agent must treat these as a separate risk surface from the graph traversal implementation. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `chain`/`current` modes must NOT use the in-memory `TypedRelationGraph` (which derives Supersedes edges from `entries.supersedes` in Pass 2a). The scope correctly mandates SQL CTEs. Risk: if an implementer uses `find_terminal_active` (graph.rs:523) as a shortcut for `current` mode and the in-memory graph is stale or cold-cache, results will be wrong or unavailable. | High | Med | Architect must make the SQL-only constraint a hard implementation rule in the pseudocode, not a soft convention. The spec constraint section must call this out explicitly. Ref: SCOPE.md §Constraints. |
| SR-08 | The current branch is `feature/vnc-017`. vnc-018 depends on vnc-017 (auto-redirect) being merged first to get the post-vnc-017 codebase state (updated `graph.rs`, `edge_write.rs`, `query_incoming_edges`). If vnc-018 delivery begins on a branch cut from pre-vnc-017 state, the implementer gets wrong base code. | High | Med | Delivery must not begin until vnc-017 PR is merged to main and vnc-018 branches from that merged state. Design Leader must enforce this gate. |
| SR-09 | `tools.rs` is 9,610 lines. The `#[tool]` dispatch point for `context_graph` must be added there. Pattern #4436 (Unimatrix) warns that every call from `tools.rs` to a sibling module must use a fully-qualified module path. Risk: an implementer who follows the `context_cycle` pattern but forgets the path qualifier will get a silent compile error or wrong dispatch. | Med | Med | Architect must include the exact `tools.rs` wiring pattern in the pseudocode. Spec must include a test that exercises the full dispatch chain from the MCP handler, not just `graph_read.rs` unit functions. |

## Assumptions

- **SCOPE.md §Background Research / W1B-1**: Assumes `context_edge` (vnc-015, PR #600) is merged and all 16 `RelationType` variants are live in `graph.rs`. If vnc-015 is not merged, `from_str()` will not recognize the 10 new types and `neighbors` mode will error on valid inputs.
- **SCOPE.md §Proposed Approach / neighbors mode**: Assumes `TypedRelationGraph.edges_of_type()` and `TypedRelationGraph.node_index` are stable APIs available at the point of integration. If these APIs changed in vnc-017 or another in-flight feature, depth>1 BFS implementation breaks.
- **SCOPE.md §Schema migration**: Assumes schema migration version increment is available and no migration version collision exists with any in-flight feature (vnc-017 or otherwise).

## Design Recommendations

1. **SR-05 is a blocker** — resolve the `truncated` response structure (flat bool vs. per-direction object) before spec is written. A spec that leaves this ambiguous will produce a gate-3a rejection (ref: lesson #4043 — pending-decision language in test plans causes rework).
2. **SR-03 forward-compat contract** — record `GraphParams` and `EdgeRecord` struct layouts as a Unimatrix ADR before delivery. #597 and #598 must be blocked from changing these types without an ADR update; otherwise a breaking change arrives silently.
3. **SR-08 branch dependency** — make the vnc-017 merge a hard gate-0 check in the delivery protocol for this feature. The codebase state assumption is non-trivial.
4. **SR-06 PPR/BFS bundling** — consider whether `Advances`/`Motivates` PPR/BFS addition belongs in a separate sub-task with its own AC regression baseline, to prevent a search re-ranking regression from being masked by passing `context_graph` tests.
