# Test Plan — store-neighbor-source (additive `source` on plain path)

`RawEdgeRow` gains `pub source: String`; `map_edge_row` reads it via `try_get`; the `source`
column is added to **all 4 plain SELECTs** (`run_outgoing_query` / `run_incoming_query`, empty-type
and IN-type branches). This is the **shared** path co-owned by `context_graph` neighbors. Owns
**R-08 (Critical-adjacent High)**, **R-09 (authored predicate source)**, **R-05 (source stamp)**,
**R-20 (precondition doc)**. The rank/JOIN/canon logic must live ONLY in the ranked variant.

## Unit / Empirical Expectations

### R-08 — Additive `source` must not break `context_graph` neighbors (High)

**`neighbors_suite_passes_unedited`** (AC-09, empirical, #4876)
Run the **existing** `context_graph` neighbors tests after the `RawEdgeRow`/SELECT extension —
**green with zero edits**. This is the headline SR-02 guarantee: the neighbors contract and
`EdgeRecord` wire bytes are unchanged. Captured in the integration run (existing
`test_tools.py` neighbors tests + any `context_graph` unit tests).

**`test_map_edge_row_populates_source_all_4_branches`** (#4166 — audit ALL passes)
Assert the new `source` field is correctly populated for:
1. `run_outgoing_query` empty-`edge_types` branch,
2. `run_outgoing_query` IN-type branch,
3. `run_incoming_query` empty-`edge_types` branch,
4. `run_incoming_query` IN-type branch.
Seed edges with distinct `source` values and assert each branch returns the right value (column
index correct, no row-shape drift). A wrong column index in `map_edge_row` fails here.

**`test_no_canon_or_confidence_leak_into_plain_query`** (R-08 surface 3 / SR-06)
Assert the plain `query_direct_neighbors` returns edges with **no canonicalization** (a symmetric
pair returns **two** rows on the plain path — canon is get-only) and **no** `↔` / no
`target_confidence` semantics leak into the shared output. The `↔` and confidence JOIN belong to
the ranked variant only.

### R-09 — `source` value drives `authored` (High)

**`test_source_values_present_for_all_live_sources`**
Seed edges with `source` ∈ {`agent`, `co_access`, `cosine`, `behavioral`, `S8`}; assert
`RawEdgeRow.source` carries the exact string for each (the boolean projection in
get-edge-vocabulary depends on this exact value).

### R-05 — Carry-forward / context_edge stamp `source='agent'` (High)

**`test_carried_forward_edge_source_is_agent`** (FR-17/SR-10)
Carry an authored edge forward via the vnc-035 path; read the row; assert `source == 'agent'`.
(The slot-priority consequence is asserted in store-ranked-query / get-edge-assembly.)

**`test_context_edge_write_source_is_agent`** (SR-10)
Write an edge via `context_edge`; assert the stored `source == 'agent'`.

### R-20 — Precondition documented, source retained (Low)

**`grep_authored_precondition_documented`** (C-10/SR-05)
Assert a comment/doc at the `authored` computation site names **NLI revival** as the documented
trigger to revisit D-03.

**`test_source_string_retained_beneath_boolean`**
Assert `RawEdgeRow.source` (the string) is preserved underneath the `authored` boolean — no
information loss (the boolean is derived, the string is kept).

## Integration Expectations (through MCP)
- `neighbors_suite_passes_unedited` is the primary integration gate (existing suites green).
- `test_get_authored_flag_agent_vs_inferred` (tools) — AC-03 through the MCP surface.
- `test_get_carried_forward_classifies_authored` (lifecycle) — R-05/FR-17.

## Edge Cases
- All 5 live `source` values populate correctly.
- Symmetric pair on the **plain** path returns two rows (no get-only canon leak).
- Near-miss `source` strings (`'Agent'`, `' agent'`) — exact-match assertion lives in
  get-edge-vocabulary (the boolean), but the raw string is preserved verbatim here.

## Security
- The `source` column is read additively; no new input surface. Positional binds on the plain
  SELECTs are unchanged (pre-existing).
