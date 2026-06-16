# Agent Report — vnc-037-agent-3-get-edge-vocabulary

## Task
Implement get-edge-vocabulary: the `GetEdge`/`EdgeTotals`/`EdgesView` discovery-list types
in a new file `response/edges.rs`, register the module in `response/mod.rs`. Types + trivial
constructors only — no render helpers (next wave).

## Files Created / Modified
- `crates/unimatrix-server/src/mcp/response/edges.rs` (new — 304 lines)
- `crates/unimatrix-server/src/mcp/response/mod.rs` (added `pub mod edges;` only)

## What was built
- `GetEdge { edge_type: String, direction: &'static str, target_id: u64,
  target_title: Option<String>, authored: bool }` — EXACTLY the 5 fields, no
  source_id/depth/metadata/source/weight/target_confidence (ADR-002 guardrail).
- `EdgeTotals { inbound: usize, outbound: usize }` (uncapped, ↔ once).
- `EdgesView { edges: Vec<GetEdge>, totals: EdgeTotals }`.
- `GetEdge::new(..)` trivial constructor that computes `authored = source == EDGE_SOURCE_AGENT`
  (exact match; no `.unwrap()`). Direction constants `DIRECTION_{BOTH,OUTBOUND,INBOUND}`.
- All `#[derive(Debug)]`+; `Serialize` for the JSON shape contract.

## Key decisions
- `authored` predicate reuses the existing `crate::mcp::edge_write::EDGE_SOURCE_AGENT`
  (`pub(crate)`, same crate) per pseudocode — NOT an inlined `"agent"` literal — so it stays
  byte-identical to the SQL `(source='agent')` rank term (R-09). The pseudocode referenced an
  `EDGE_SOURCE_AGENT` re-exported from `unimatrix-store`; that re-export does not exist, but
  the in-crate `edge_write` constant has the same value and is in scope, so no store change
  was needed (and store changes are out of my scope).
- Direction values kept as `&'static str` (not an enum) per OVERVIEW/ADR-002 for trivial JSON.
- `DIRECTION_*` consts carry `#[allow(dead_code)]`: their non-test consumers (get-edge-assembly
  projection, serializer-seam glyph selection) land in the next wave. Removing the allow once
  those wire up is a clean follow-up.

## Tests: 9 passed / 0 failed
`cargo test -p unimatrix-server --lib mcp::response::edges` — all green:
- test_get_edge_exact_five_fields (AC-02 / FR-4 shape guardrail)
- test_edges_view_caps_at_limit (references GET_EDGE_DISPLAY_LIMIT, not 3)
- test_authored_true_only_for_agent (AC-03 / R-09)
- test_authored_exact_match_no_near_miss (R-09 predicate parity)
- test_direction_strings_inbound_outbound_both (R-10)
- test_projection_matches_edgerecord_mapping (FR-15)
- test_edge_totals_inbound_outbound_object (OQ-01 / ADR-005)
- test_target_confidence_not_in_get_edge (R-20)
- test_target_title_none_serializes_null (edge case)

`cargo build -p unimatrix-server` passes; `cargo clippy -p unimatrix-server --tests` clean on
edges.rs (no warnings attributable to my file). File is 304 lines (≤500).

## Issues / Blockers
None. Did not run git (per spawn-prompt rule). Did not touch `entries.rs`,
`entry_to_json`/`format_entry_markdown_section`, or integration tests.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- SKIPPED, Unimatrix MCP disconnected per spawn
  note; read ADR-002/ADR-005 files + OVERVIEW directly instead.
- Stored: nothing novel to store -- pure data-type vocabulary; the only non-obvious point
  (reuse EDGE_SOURCE_AGENT for the authored predicate so it matches the SQL rank term) is
  already documented in the ADR-002/test-plan and inline in the source.
