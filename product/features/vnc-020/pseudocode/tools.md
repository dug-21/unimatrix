# Pseudocode: tools.rs (Wave 1)
# Modified file: crates/unimatrix-server/src/mcp/tools.rs

## Purpose

`tools.rs` contains the `CONTEXT_GRAPH_DESCRIPTION` constant string that is used as the
`#[tool(description = "...")]` attribute for the `context_graph` MCP tool (lines 51-76
in the current file). vnc-020 extends this description to cover the three new modes.

No logic changes are made to `tools.rs`. This is a description-only update.

---

## Location

The constant to update is:

```
pub(crate) const CONTEXT_GRAPH_DESCRIPTION: &str = "Traverse the Unimatrix knowledge graph \
    in four modes:\n\
    ...
    Requires Read capability. All modes are read-only.";
```

The `#[tool(description = CONTEXT_GRAPH_DESCRIPTION)]` attribute is at line ~3378 on
the `context_graph` handler function. The constant at line 51 is the single source of
truth — only update the constant, NOT the attribute literal directly.

---

## What to Change

Change "four modes" to "seven modes" in the opening sentence.

Then append three new mode sections to the existing description string, immediately
before the final `\n    Requires Read capability. All modes are read-only."` line.

### 1. inverse mode section

Add after the subgraph section:

```
    - inverse: Return entries of a given category that have no incoming edges of ALL \
      the specified missing_edge_types (AND semantics — entries missing ALL listed types). \
      Example: missing_edge_types=[\"Cites\",\"Supports\"] returns entries that have \
      NEITHER a Cites NOR a Supports incoming edge. To find entries missing ANY one type, \
      issue one inverse query per type. Requires category and missing_edge_types (both \
      required, missing_edge_types must be non-empty). Optional limit (default 100, \
      range [1,500]). Queries the live database — no staleness.\n\
```

### 2. filter mode section

Add after the inverse section:

```
    - filter: Return entries matching a category and optional property + edge-count \
      constraints. Required: category. Optional: limit (default 100, range [1,500]), \
      min_age_days (created at least N days ago), min_confidence, max_confidence, \
      min_edge_count (outgoing edges of edge_types >= N), max_edge_count (outgoing \
      edges of edge_types <= N, use max_edge_count=0 to find entries with zero \
      matching outgoing edges). When min_edge_count or max_edge_count is present, \
      edge_types must also be specified. Both edge-count bounds may be combined to \
      express a range. Queries the live database — no staleness.\n\
```

### 3. path mode section (mandatory staleness disclosure verbatim — R-01, AC-19)

Add after the filter section:

```
    - path: Find the shortest outgoing-edge path from from_id to to_id using BFS. \
      Required: from_id, to_id. Optional: edge_types (absent = all non-Supersedes \
      types), depth (default 5, range [1,10]), resolve_supersessions (default false — \
      when true, deprecated endpoints are resolved to their terminal active successors \
      before BFS begins and deprecated intermediate nodes are resolved per-hop). \
      path mode uses the in-memory graph cache for BFS traversal. The cache is rebuilt \
      each tick (typically 30-60 seconds). Edges written within the current tick \
      interval may not appear in the result. This is the same staleness contract as \
      neighbors mode at depth>1 and subgraph mode. If from_id or to_id is not present \
      in the current graph snapshot, the result is { found: false } — not an error. \
      Use resolve_supersessions=true to have deprecated endpoints resolved to their \
      active successors before BFS begins.\n\
```

---

## Full Updated Constant (Assembled)

The updated `CONTEXT_GRAPH_DESCRIPTION` constant reads as follows (implementation agent
must produce this as a single `&str` literal with `\n\` line continuations matching the
existing style):

```
"Traverse the Unimatrix knowledge graph in seven modes:\n\
    - chain: ...\n\      [EXISTING TEXT — DO NOT CHANGE]
    - current: ...\n\    [EXISTING TEXT — DO NOT CHANGE]
    - neighbors: ...\n\  [EXISTING TEXT — DO NOT CHANGE]
    - subgraph: ...\n\   [EXISTING TEXT — DO NOT CHANGE]
    - inverse: Return entries of a given category that have no incoming edges of ALL \
      the specified missing_edge_types (AND semantics — entries missing ALL listed types). \
      Example: missing_edge_types=[\"Cites\",\"Supports\"] returns entries that have \
      NEITHER a Cites NOR a Supports incoming edge. To find entries missing ANY one type, \
      issue one inverse query per type. Requires category and missing_edge_types (both \
      required, missing_edge_types must be non-empty). Optional limit (default 100, \
      range [1,500]). Queries the live database — no staleness.\n\
    - filter: Return entries matching a category and optional property + edge-count \
      constraints. Required: category. Optional: limit (default 100, range [1,500]), \
      min_age_days (created at least N days ago), min_confidence, max_confidence, \
      min_edge_count (outgoing edges of edge_types >= N), max_edge_count (outgoing \
      edges of edge_types <= N, use max_edge_count=0 to find entries with zero \
      matching outgoing edges). When min_edge_count or max_edge_count is present, \
      edge_types must also be specified. Both edge-count bounds may be combined to \
      express a range. Queries the live database — no staleness.\n\
    - path: Find the shortest outgoing-edge path from from_id to to_id using BFS. \
      Required: from_id, to_id. Optional: edge_types (absent = all non-Supersedes \
      types), depth (default 5, range [1,10]), resolve_supersessions (default false — \
      when true, deprecated endpoints are resolved to their terminal active successors \
      before BFS begins and deprecated intermediate nodes are resolved per-hop). \
      path mode uses the in-memory graph cache for BFS traversal. The cache is rebuilt \
      each tick (typically 30-60 seconds). Edges written within the current tick \
      interval may not appear in the result. This is the same staleness contract as \
      neighbors mode at depth>1 and subgraph mode. If from_id or to_id is not present \
      in the current graph snapshot, the result is { found: false } — not an error. \
      Use resolve_supersessions=true to have deprecated endpoints resolved to their \
      active successors before BFS begins.\n\
    Requires Read capability. All modes are read-only."
```

---

## Critical Constraints on the Description Text

1. **Staleness disclosure is mandatory verbatim** (R-01, AC-19). The exact sentence
   "path mode uses the in-memory graph cache for BFS traversal. The cache is rebuilt each
   tick (typically 30-60 seconds). Edges written within the current tick interval may not
   appear in the result." MUST appear in the path mode section. Do not paraphrase.

2. **inverse and filter descriptions must NOT contain staleness language** (R-01,
   RISK-TEST-STRATEGY §R-01 scenario 3). Do not include the words "tick", "cache",
   or "in-memory" in the inverse or filter sections.

3. **AND semantics example is mandatory for inverse** (ADR-003, SR-06). The description
   must clearly state that multiple `missing_edge_types` returns entries missing ALL
   listed types. Include the example.

4. **max_edge_count=0 use case must be called out** in the filter section (AC-29).
   The phrase "use max_edge_count=0 to find entries with zero matching outgoing edges"
   or equivalent must appear.

5. The `CONTEXT_GRAPH_DESCRIPTION` constant is `pub(crate)`. Tests at lines 4996-4998
   of tools.rs assert on the description text for staleness content. The test
   `test_context_graph_description_contains_staleness_text` must pass — it checks for
   the subgraph staleness text already present; no changes should break that test.
   The new path mode staleness text can be asserted in a new test (AC-19).

---

## Error Handling

No error handling — this is a constant string update only.

---

## Key Test Scenarios

- AC-19: Test that `CONTEXT_GRAPH_DESCRIPTION` contains the phrase "in-memory graph cache"
  or equivalent staleness disclosure for path mode. Implement as a `const_assert!` or a
  test function parallel to `test_context_graph_description_contains_staleness_text`.
- R-01 scenario 3: Test that `CONTEXT_GRAPH_DESCRIPTION` does NOT contain "tick" or "cache"
  in proximity to "inverse" or "filter" (assert inverse/filter are labeled live-DB).
- R-05 / ADR-003: Test that `CONTEXT_GRAPH_DESCRIPTION` contains "missing ALL" or "AND
  semantics" to confirm the inverse mode AND semantics are disclosed.
- Existing test `test_context_graph_description_contains_staleness_text` must continue
  to pass unchanged.
