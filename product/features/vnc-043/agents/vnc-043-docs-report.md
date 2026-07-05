# vnc-043 — Documentation Agent Report

Agent: vnc-043-docs
Issue: #903 · PR: #909

## Outcome

README updated. The `context_graph` MCP Tool Reference row (README.md line 523) described
the `subgraph` mode staleness and `direction` param in a way this feature made stale.
Two targeted edits to that single table row.

## Sections modified

1. **MCP Tool Reference — `context_graph` row, subgraph mode description.**
   - Before: "`subgraph`: … uses in-memory graph (tick-window staleness applies); …"
   - After: states subgraph honors `edge_types` and `direction` (`incoming`/`outgoing`/`both`)
     filtering during traversal, and splits freshness — `max_depth=1` reads the live DB (all
     committed writes reflected immediately), `max_depth>1` reads the in-memory graph (may lag
     by up to one tick). Mirrors the existing `neighbors` depth=1/depth>1 wording in the same row.
   - Traces to: SCOPE Goals 1,4; SPEC FR-3, FR-4, FR-7; AC-05, AC-06, AC-09; shipped
     `CONTEXT_GRAPH_DESCRIPTION` text (tools.rs).

2. **MCP Tool Reference — `context_graph` row, `direction` param note.**
   - Before: "`direction` (… chain and neighbors; ignored for mode semantics in subgraph where
     all EdgeRecords carry `"outgoing"`)" — incorrectly stated direction is ignored on subgraph.
   - After: direction applies to chain (`forward`/`backward`/`both`) and to neighbors + subgraph
     (`incoming`/`outgoing`/`both`); on subgraph it filters which edges are traversed/returned,
     though every returned EdgeRecord still carries the canonical `"outgoing"` label (filter
     affects inclusion, not label).
   - Traces to: SCOPE Goal 1 (direction schemars fix); SPEC FR-2, FR-7, AC-06; shipped
     `graph_read.rs` direction doc.

The `edge_types` param note already listed subgraph — no change needed there.

## docs/ blast radius

None. Grep of `docs/` for `context_graph`, `subgraph` (tool sense), `edge_types`, `max_depth`,
staleness/tick-cache found only unrelated matter — research architecture notes and mermaid
`subgraph` diagram blocks in workflow docs. No `docs/` file documents `context_graph` subgraph
behavior at this granularity. No `_Verified on vX_` executable-claim surfaces touched.

## Out-of-scope observation (flagged, not edited)

The same README row states `resolve_supersessions` (neighbors, subgraph, path) defaults **false**,
while the shipped tool description (tools.rs) and AC-08 state it defaults **true**. This is a
pre-existing discrepancy, NOT introduced or made stale by vnc-043, so it is outside this feature's
blast radius and was left untouched. Worth a follow-up doc-correctness pass.

## Process note

Initial edits were mistakenly applied to the main-checkout `README.md`
(`/workspaces/unimatrix/README.md`, branch `main`) instead of the worktree copy. Reverted with
`git checkout README.md` in the main checkout; re-applied to the worktree README before commit.
No stray change left on `main`.

## Knowledge Stewardship

Exempt per agent definition — documentation agent reads feature artifacts and proposes README
edits; it does not generate or query Unimatrix knowledge entries. No store/query performed.
