# Agent Report — vnc-037-agent-3-store-neighbor-source

## Task
Implement store-neighbor-source (ADR-004, additive, NO migration): add `source` to the
plain neighbor read path so the get-edge projection can derive `authored = (source == "agent")`.
Co-owned with `context_graph` neighbors — change is ADDITIVE only.

## Files Modified
- `crates/unimatrix-store/src/graph_queries.rs` — `RawEdgeRow` gains `pub source: String`
  and `pub target_confidence: Option<f64>` (additive; doc comment records the ADR-004/ADR-006
  rationale and the C-10/SR-05 NLI-revival trigger). Existing 3 fields untouched.
- `crates/unimatrix-store/src/graph_queries_neighbors.rs` — `map_edge_row` reads `source` via
  `try_get` (`None` for `target_confidence` on the plain path); `source` added to ALL 4 plain
  SELECTs (`run_outgoing_query`/`run_incoming_query`, empty-type + IN-type branches).
- `crates/unimatrix-store/src/graph_queries_tests.rs` — extended cumulatively: `insert_edge`
  now delegates to a new `insert_edge_with_source` helper (existing callers unchanged); 4 new
  tests added.

## Scope Discipline
- Did NOT add the confidence LEFT JOIN or symmetric `↔` canonicalization here — those are the
  ranked variant (`graph_queries_ranked.rs`, a different component). The plain path always sets
  `target_confidence: None`.
- `target_confidence` field was added now per OVERVIEW.md shared-type definition and the brief's
  RawEdgeRow shape (one shared row type, no later shape drift). Plain path leaves it `None`.
- Did NOT touch integration tests; did NOT run git.
- R-05 (`test_carried_forward_edge_source_is_agent`, `test_context_edge_write_source_is_agent`)
  and the C-10 `grep_authored_precondition_documented` check target the WRITE path / get-edge
  projection site — outside this component's two files. Documented the NLI trigger on `RawEdgeRow`
  regardless. These belong to other components / integration tests.

## Tests
- New unit tests (4), all passing:
  - `test_map_edge_row_populates_source_all_4_branches` (R-08/#4166) — source populated correctly
    across all 4 SELECT branches.
  - `test_source_values_present_for_all_live_sources` (R-09) — `agent`/`co_access`/`cosine`/
    `behavioral`/`S8` all carry through verbatim.
  - `test_source_string_retained_beneath_boolean` (R-20) — near-miss `Agent`/` agent` retained
    verbatim, not coerced.
  - `test_no_canon_or_confidence_leak_into_plain_query` (R-08 surface 3 / SR-06) — symmetric pair
    returns TWO rows on the plain path; `target_confidence` always `None`.
- `cargo test -p unimatrix-store --lib graph_queries`: 20 passed, 0 failed.
- `cargo test -p unimatrix-server --lib neighbor` (SR-02/AC-09 empirical, UNEDITED): 17 passed,
  0 failed — `context_graph` neighbors contract green with zero edits.
- `cargo build --workspace`: passes (pre-existing warnings only).
- `cargo fmt` applied; clippy clean on changed code (the one `graph_queries_tests.rs:134`
  double-ref warning is pre-existing, not mine).

## Issues / Blockers
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- SKIPPED (Unimatrix MCP disconnected per spawn note);
  read ADR-004 file directly instead.
- Stored: nothing novel to store -- Unimatrix MCP is disconnected (cannot store); and the work was
  a straightforward additive-column change already well-documented by ADR-004 + the #4831
  (additive blast-radius front-load) and #4876 (empirical re-verify) lessons it cites.
