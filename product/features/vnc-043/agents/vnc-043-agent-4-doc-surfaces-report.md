# Agent Report — vnc-043-agent-4-doc-surfaces (Stage 3b Wave 2)

Component: discoverable-contract doc surfaces (closes #903 — filter ships in code; contract mis-documented it as "neighbors only").

## Files modified
- `crates/unimatrix-server/src/mcp/graph_read.rs` — schemars docs for `direction` (:82) and `edge_types` (:84).
- `crates/unimatrix-server/src/mcp/tools.rs` — both twin description literals (`CONTEXT_GRAPH_DESCRIPTION` const + live `#[tool(description=…)]`), edited identically; extended substring assertions in `test_context_graph_description_contains_staleness_text`.
- `crates/unimatrix-server/src/mcp/graph_read_tests.rs` — added `test_graphparams_schemars_docs_state_subgraph_applies`.

## Edits
1. `direction` schemars doc — now: applies to chain, neighbors, AND subgraph (neighbors/subgraph = incoming/outgoing/both). Contains "subgraph".
2. `edge_types` schemars doc — now: "neighbors, subgraph, and path: edge types to traverse (absent/[] = all except Supersedes)." Dropped "neighbors only".
3&4. Subgraph section of BOTH literals (byte-identical): added filter-availability sentence + replaced flat staleness line with depth-1-live / depth>1-cache split. All existing asserted phrases preserved (`direction:"outgoing"`, `depth_reached`, `truncated`, `empty result`, `200`, `values above 200 are rejected`, no `graph_rebuilt_at`).

## New substring phrases (for Stage 3c alignment)
- `subgraph honors an edge_types filter and a direction`  (filter availability, AC-13)
- `subgraph max_depth=1 queries the live database`  (staleness depth-1 live, AC-09)
- `subgraph max_depth>1 queries the in-memory graph cache`  (staleness depth>1 cache, AC-09)

Schemars test asserts: `direction` desc contains "subgraph"; `edge_types` desc contains "subgraph" and NOT "neighbors only".

## Tests
- `cargo test -p unimatrix-server --lib`: 4373 passed, 0 failed, 1 ignored.
- #869 `test_graph_tool_attr_description_matches_const`: GREEN (twins byte-identical).
- `test_context_graph_description_contains_staleness_text` (extended): GREEN.
- `test_graphparams_schemars_docs_state_subgraph_applies` (new): GREEN.
- clippy `-p unimatrix-server --lib --tests`: no warnings.
- Snapshot pins (FR-5 / Open Q4): confirmed NONE (`insta`/`assert_snapshot`/`.snap` grep empty under crate) — negative holds.

## Flags
- rustfmt gotcha: crate is edition 2024 (let-chains). A bare `rustfmt --edition 2021` errors AND rewrites out-of-scope `#[path]` sibling files (reordered a `use …GraphParams` in graph_read_cross_surface_tests.rs). Reverted that churn; used `rustfmt --edition 2024`. Only my 3 files are modified. Leader: verify no stray churn before wave commit.
- No `GraphParams` wire/struct change — doc-string edits only. Path-mode text ("same staleness contract as … subgraph mode") left intact (out of scope, still accurate).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search — surfaced ADR-002 vnc-043 (#5449, twin-literal + byte-equality guard, do not collapse) and #4479 (neighbors depth-split staleness); applied both.
- Stored: entry #5457 "Editing the CONTEXT_GRAPH_DESCRIPTION twin literals: whitespace-stripping line continuations + edition-matched fmt" via context_store (pattern, topic unimatrix-server).
