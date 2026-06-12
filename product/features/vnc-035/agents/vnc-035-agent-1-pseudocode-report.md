# Agent Report — vnc-035-agent-1-pseudocode

## Deliverables

Per-component pseudocode for vnc-035 `context_correct` outgoing-edge carry-forward:

- `product/features/vnc-035/pseudocode/OVERVIEW.md`
- `product/features/vnc-035/pseudocode/query_outgoing_edges.md`
- `product/features/vnc-035/pseudocode/run_carry_forward_loop.md`
- `product/features/vnc-035/pseudocode/context_correct_handler.md`
- `product/features/vnc-035/pseudocode/docs_cleanup.md`

## Components covered

1. `query_outgoing_edges` + `OutgoingEdgeRow` (unimatrix-store) — single-source SQL eligibility predicate.
2. `run_carry_forward_loop` + `CarrySummary` (tools.rs) — owns its write loop; counts `true` only; AC-07 seam.
3. `context_correct` handler step 8b′ + `edges_carried` ack (tools.rs / response/entries.rs).
4. docs_cleanup (uni-zero SKILL + agent docs) — doc-edit plan.

## Grounding

Pseudocode is grounded in the exact cited code: `query_incoming_edges` (read.rs:1694),
`run_redirect_loop` (tools.rs:4660), `context_correct` handler (tools.rs:1015-1185),
`validate_and_write_edges` / `EDGE_SOURCE_AGENT` (edge_write.rs:152/28), `write_graph_edge`
(nli_detection.rs:78), `format_redirect_summary` / `format_correct_success`
(response/entries.rs:265/301). Load-bearing details honored: 8→8b→8b′→8c order; carry owns its
write loop counting `write_graph_edge true` only (#4041); single SQL eligibility predicate with
inline superset-vs-incoming rationale; created_at=now (not preserved); Contradicts one logical
edge / two rows counted once; warn-and-continue never rolls back; AC-07 fault-injection seam specified.

## Knowledge Stewardship
- Queried: `context_search` (pattern) → #4041 (write_graph_edge true-only counting — load-bearing),
  #4056/#4417 (edge-write helpers), #4459 (Contradicts source-validation), #3883/#4468 (graph
  writes); `context_search` (decision, topic vnc-035) → #4983 ADR-001, #4988 ADR-004, #4987 ADR-005.
  Read all five ADR files directly for full text. All applied.
- Deviations from established patterns: none. The carry loop deliberately does NOT reuse
  `validate_and_write_edges` wholesale (it discards the bool — R-08); this is mandated by ADR-003,
  not a deviation. No new patterns to store (read-only tier).

## Open questions / gaps (flagged, non-blocking)
- **O-1 (index):** `idx_graph_edges_source_id` presence unverified — developer confirms; latency-only (R-09).
- **O-2 (module split):** `read.rs` is already >1570 lines; a new `read_outgoing.rs` is the likely
  home for `query_outgoing_edges` — developer decides on live count.
- **AC-07 seam shape:** ADR-003 leaves the seam mechanism to the developer; I specified a
  `#[cfg(test)]` injectable `carry_write_edge` indirection as the recommended seam with the fixed
  contract (one mid-loop write → false-SQL-error; failed++; warn; prior carries + correction persist).
- **`failed` exactness:** recommended impl (a) makes `carried` exact and `failed` exact-under-test
  (via the seam) / approximate-in-prod, no `write_graph_edge` signature change. Impl (b) (thin
  three-case wrapper) available if exact prod `failed` is wanted. Developer's call per ADR-003.
- **docs grep false positives:** several agent docs mention "carry"/"context_correct" generically;
  implementer must inspect and edit only genuine re-declaration instructions (the uni-zero SKILL is
  the confirmed primary surface).
