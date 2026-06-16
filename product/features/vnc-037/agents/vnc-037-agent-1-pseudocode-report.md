# Agent Report: vnc-037 Pseudocode (Stage 3a)

> Agent: uni-pseudocode
> Agent ID: vnc-037-agent-1-pseudocode
> Stage: 3a (Component Design / Pseudocode)
> Date: 2026-06-16

## Summary

Produced per-component pseudocode for vnc-037 (edge inclusion on the `get` path)
under `product/features/vnc-037/pseudocode/`: one `OVERVIEW.md` plus 8 component
files. All files trace to ADR-001..007 and cover FR-1..FR-19; the integration
surface (function signatures, shared types, SQL clauses) is taken verbatim from
the architecture, not invented.

### Files produced

| File | Component | Covers |
|------|-----------|--------|
| `OVERVIEW.md` | Component interaction, data flow, shared types (`RawEdgeRow`, `EdgeCountSplit{inbound,outbound,both,authored}`, `GetEdge`, `EdgeTotals{inbound,outbound,both}`, `EdgesView{…,authored_total}`, `GetParams`), shared canonicalization CTE, sequencing | ADR-001..007 cross-cut |
| `store-display-cap-constant.md` | `GET_EDGE_DISPLAY_LIMIT` constant in `read.rs`, re-exported via `lib.rs`; bound to `LIMIT ?`, referenced by render + tests; no literal `3` | FR-18, ADR-006 |
| `store-neighbor-source.md` | Plain neighbor path gains additive `source` only; `authored` derived from `EDGE_SOURCE_AGENT` exact match; carried-forward authored | FR-6, FR-16, FR-17, ADR-004 |
| `store-ranked-query.md` | Ranked variant: shared `deduped` CTE → locked `ORDER BY (d.source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?`; `LEFT JOIN entries t` for rank key; direction hint flows SQL→projection | FR-1, FR-9, ADR-001, ADR-006, ADR-007 |
| `store-split-count.md` | Byte-identical `deduped` CTE → `SUM(CASE …)` uncapped **3-bucket** split (`inbound` asymmetric-only, `outbound`, `both`=↔-once) + 4th digest-only `authored` aggregate; #744 regression note (↔→`both`, never inbound); parity with ranked query | FR-8, FR-10, FR-11, R-01/R-03, ADR-001, ADR-005, ADR-007 |
| `get-edge-vocabulary.md` | Exact 5-field discovery projection, no enrichment; `EdgeTotals` 3-key `{inbound,outbound,both}`; `EdgesView` gains digest-only `authored_total`; batched title fetch via `fetch_titles_batch`; dangling neighbors retained | FR-4, FR-5, FR-15, ADR-002, ADR-005 |
| `serializer-seam.md` | `entry_to_json` / `format_entry_markdown_section` signatures UNCHANGED; `None ⇒ key/section never inserted` (structural); **3-key** nested `edge_totals` JSON + flat markdown `### Related` (`…N more` = `inbound+outbound+both`) + **LOCKED** summary digest byte form `" | edges: {outbound}↑ {inbound}↓ ↔{both} ({K} authored)"` (`{K}`=`authored_total`, all-zero ⇒ `edges: none`) | FR-13, FR-14, ADR-003, ADR-005 |
| `get-edge-assembly.md` | `build_edges_view` orchestration after successful primary read; projects `EdgeCountSplit`→`EdgeTotals{in,out,both}` + threads `split.authored`→`EdgesView.authored_total`; FR-19 fail-loud mapping to `ServerError::Core(CoreError::Store(e))` (identical to `entry_store.get` at tools.rs:963-965); opt-out short-circuit | FR-2, FR-3, FR-19, ADR-002, ADR-005 |
| `get-params.md` | `include_edges` param parsing, opt-out (`Some(false)`) skip, default behavior | FR-2, FR-3 |

### Flagged in-artifact (non-blocking)

- **OQ-02 summary-digest `↔` sub-tally — RESOLVED 2026-06-16 (Gate 3a carry-over locked).** The
  architect amended ADR-005 to a **three-bucket** `EdgeTotals{inbound, outbound, both}` (option (a),
  generalized): `↔` gets its own `both` bucket (NOT folded into inbound — the #744 clean
  asymmetric-inbound-degree signal), and a 4th digest-only `authored` aggregate over the full
  uncapped set feeds `(K authored)`. The digest byte form is LOCKED:
  `" | edges: {outbound}↑ {inbound}↓ ↔{both} ({K} authored)"` (fixed arity, all-zero ⇒
  `edges: none`). The pseudocode previously flagging this is now updated to the locked contract;
  the old "widen vs drop ↔" dilemma is removed.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — Unimatrix MCP DISCONNECTED (no results), non-blocking
  per protocol. Read the amended ADR-005 (TOTALS BUCKET CONTRACT, locked 2026-06-16) + ARCHITECTURE.md
  (EdgeTotals/EdgeCountSplit signatures) directly as the authoritative source. NOTE: the
  `context_correct` of the stored ADR-005 entry is DEFERRED per the ADR amendment — Unimatrix re-sync
  required once MCP is back.
- **Targeted update 2026-06-16 (Gate 3a OQ-02 lock → Stage 3b coherence).** Updated 5 files to the
  three-bucket totals contract + locked digest: `OVERVIEW.md` (shared types), `store-split-count.md`
  (3+1 aggregates; ↔→`both` regression note replaces the retired ↔→inbound fold),
  `get-edge-vocabulary.md` (`EdgeTotals` gains `both`; `EdgesView` gains `authored_total`),
  `serializer-seam.md` (3-key JSON `edge_totals`; LOCKED digest byte form; `…N more`=sum of 3),
  `get-edge-assembly.md` (project→`EdgeTotals{in,out,both}`, thread `authored_total`). Unaffected
  files untouched: ranked-query SELECT, neighbor-source, cap-constant, get-params (RawEdgeRow +
  ranked select unaffected; canonicalization CTE stays byte-shared per ADR-007).
- Deviations from established patterns: none.
  - Additive `Option<T>` field per vnc-020 (`RawEdgeRow.source`, `target_confidence`).
  - Constants co-located in `read.rs` + re-exported via `lib.rs` per crt-034 / vnc-015
    (`GET_EDGE_DISPLAY_LIMIT`).
  - `fetch_nodes_batch` positional-bind title precedent followed for `fetch_titles_batch`.
  - `try_get` + `StoreError` mapping at the store boundary.
  - Primary-read `ServerError` mapping at tools.rs:963 reused for FR-19 fail-loud.
- Stored: nothing novel (read-only tier; MCP disconnected regardless).
