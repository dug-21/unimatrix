# Pseudocode: `tools.rs` Changes

## Purpose

`tools.rs` requires one change: update the `context_graph` tool description string
in the `#[tool(description = "...")]` attribute to include the `subgraph` mode section
with mandatory disclosures (FR-19, ADR-004, AC-13).

No logic changes. The dispatch, capability check, and parameter handling in
`context_graph()` are unchanged.

---

## Current Tool Description (vnc-018)

Located at line 3347 (approximately, post-vnc-018):

```rust
#[tool(
    name = "context_graph",
    description = "Traverse the Unimatrix knowledge graph in three modes:\n\
        - chain: walk the supersession history of an entry (forward toward newer, \
          backward toward older, or both). forward: returns descendants (entries that \
          supersede X); backward: returns ancestors (entries X supersedes).\n\
        - current: resolve any entry to its terminal active successor, following \
          superseded_by links until an Active entry is found.\n\
        - neighbors: retrieve entries connected by typed graph edges. \
          Accepts edge_types filter, direction (incoming/outgoing/both), and depth (1..=10). \
          depth=1 queries the live database and reflects all committed writes immediately. \
          depth>1 queries the in-memory graph cache, which may lag recent writes by up to \
          one tick interval (typically 30-60 seconds). This asymmetry is intentional: \
          depth=1 is the precise lookup case where freshness matters; depth>1 is exploratory \
          multi-hop traversal where a tick-window lag is acceptable.\n\
        Requires Read capability. All three modes are read-only."
)]
```

---

## Required Change: Add Subgraph Mode Section

The description string is extended. The subgraph section must appear BEFORE the
trailing `"Requires Read capability. All modes are read-only."` closing line.

The `direction` field semantics and staleness warning MUST appear in the first two
sentences of the subgraph section (FR-19 explicit requirement — agents read the opening
of a tool description and stop; burying these facts at the end causes confusion when a
`direction="both"` caller sees every EdgeRecord labeled `direction: "outgoing"`).

### Exact Text for the Subgraph Mode Section

The following text must appear in the tool description for subgraph mode. It is derived
from FR-19 (SPECIFICATION.md) and IMPLEMENTATION-BRIEF.md §Tool Description Requirements,
which specify both required facts and the required ordering:

```
- subgraph: All returned EdgeRecords have direction: "outgoing" regardless of the \
  direction parameter you pass — this reflects the canonical stored edge direction \
  (source_id → target_id). A direction="both" traversal includes edges pointing TO \
  your seeds, but those edges are still labeled outgoing (i.e., they exist as A→seed \
  in storage). Use source_id / target_id to determine actual graph direction.\n\
  BFS uses the in-memory graph cache, rebuilt each tick (typically 30-60 seconds). \
  Edges written within the current tick interval may not appear. Same staleness \
  contract as neighbors mode at depth>1.\n\
  depth_reached: actual max depth traversed. truncated: true means the max_nodes cap \
  was hit before BFS completed — retry with a smaller max_depth or a specific \
  edge_types filter. Seed IDs absent from the graph return an empty result, not an error. \
  max_nodes must be in range 1..=200; values above 200 are rejected with a validation error.
```

### Full Updated Description String

```rust
#[tool(
    name = "context_graph",
    description = "Traverse the Unimatrix knowledge graph in four modes:\n\
        - chain: walk the supersession history of an entry (forward toward newer, \
          backward toward older, or both). forward: returns descendants (entries that \
          supersede X); backward: returns ancestors (entries X supersedes).\n\
        - current: resolve any entry to its terminal active successor, following \
          superseded_by links until an Active entry is found.\n\
        - neighbors: retrieve entries connected by typed graph edges. \
          Accepts edge_types filter, direction (incoming/outgoing/both), and depth (1..=10). \
          depth=1 queries the live database and reflects all committed writes immediately. \
          depth>1 queries the in-memory graph cache, which may lag recent writes by up to \
          one tick interval (typically 30-60 seconds). This asymmetry is intentional: \
          depth=1 is the precise lookup case where freshness matters; depth>1 is exploratory \
          multi-hop traversal where a tick-window lag is acceptable.\n\
        - subgraph: All returned EdgeRecords have direction: \"outgoing\" regardless of the \
          direction parameter you pass — this reflects the canonical stored edge direction \
          (source_id → target_id). A direction=\"both\" traversal includes edges pointing TO \
          your seeds, but those edges are still labeled outgoing (i.e., they exist as A→seed \
          in storage). Use source_id / target_id to determine actual graph direction. \
          BFS uses the in-memory graph cache, rebuilt each tick (typically 30-60 seconds). \
          Edges written within the current tick interval may not appear. Same staleness \
          contract as neighbors mode at depth>1. \
          depth_reached: actual max depth traversed. truncated: true means the max_nodes cap \
          was hit before BFS completed — retry with a smaller max_depth or a specific \
          edge_types filter. Seed IDs absent from the graph return an empty result, not an error. \
          max_nodes must be in range 1..=200; values above 200 are rejected with a validation error.\n\
        Requires Read capability. All modes are read-only."
)]
```

Notes on the string:
- `"three modes"` → `"four modes"` in the opening line.
- Escaped quotes inside the Rust string literal: `\"outgoing\"`, `\"both\"`, `\"outgoing\"`.
- The subgraph section is a continuation of the `\n\` multiline string pattern already
  used for existing modes.
- The `\n\` at the end of the subgraph section separates it from the closing line.
- The closing sentence changes from `"All three modes are read-only."` to
  `"All modes are read-only."` (or equivalent).

---

## Mandatory Facts (AC-13 Verification Checklist)

All four facts listed in AC-13 must appear in the tool description. The implementor
must verify all four are present before marking AC-13 complete:

| Fact | Where Required | Verified? |
|------|---------------|-----------|
| (a) In-memory BFS and tick-window staleness | Subgraph section | tick-window text present |
| (b) `depth_reached` and `truncated` semantics | Subgraph section | both fields described |
| (c) Unknown seed ID behavior (empty result, not error) | Subgraph section | "empty result, not an error" |
| (d) `EdgeRecord.direction` always `"outgoing"` | Subgraph section — first sentence | direction="outgoing" explained first |

Fact (d) must appear in the **first two sentences** of the subgraph section (FR-19
explicit ordering requirement).

---

## No Logic Changes

The `context_graph()` function body in `tools.rs` is unchanged:

```
async fn context_graph(
    &self,
    Parameters(params): Parameters<crate::mcp::graph_read::GraphParams>,
    request_context: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    // Step 1: build context (unchanged)
    // Step 2: require_cap(Read) (unchanged)
    // Step 3: acquire typed_graph_state (unchanged)
    // Step 4: delegate to handle_graph (unchanged)
    crate::mcp::graph_read::handle_graph(&self.store, &typed_graph_state, params, &ctx).await
}
```

The dispatch from `handle_graph` to `graph_read_subgraph::handle_subgraph` is inside
`graph_read.rs`, not in `tools.rs`.

---

## Existing Test: `test_context_graph_description_contains_staleness_text`

Located around line 4952 in `tools.rs`, this test asserts the current tool description
contains specific text strings. It must be extended to also assert the subgraph
disclosure text is present.

The test currently asserts (approximately):

```rust
assert!(description.contains("depth>1 queries the in-memory graph cache"));
assert!(description.contains("tick interval"));
assert!(description.contains("neighbors"));
// etc.
```

After vnc-019, extend to also assert (AC-13 verification):

```rust
assert!(description.contains("subgraph"));
assert!(description.contains("direction: \"outgoing\""));
assert!(description.contains("depth_reached"));
assert!(description.contains("truncated"));
assert!(description.contains("empty result, not an error"));
assert!(description.contains("max_nodes must be in range 1..=200"));
```

The implementor must update this existing test, not add a new one.

---

## Key Test Scenarios

1. Tool description string contains all AC-13 required facts:
   (a) staleness, (b) depth_reached + truncated, (c) empty result on unknown seed,
   (d) direction always "outgoing".
   Verification: extend existing `test_context_graph_description_contains_staleness_text`.

2. Tool description contains `"subgraph"` mode entry (AC-13).

3. The direction="outgoing" explanation and staleness warning appear in the subgraph
   section opening, not appended at the end (FR-19 ordering requirement).
   Verification: code review — check that the subgraph text begins with the direction
   semantics sentence.

4. `max_nodes > 200` rejection behavior documented in description (resolved from
   ALIGNMENT-REPORT.md variance FR-07).
   Verification: assert description contains "values above 200 are rejected".

5. "Four modes" (not "three modes") appears in the description opening.
