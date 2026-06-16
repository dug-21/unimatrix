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
| `OVERVIEW.md` | Component interaction, data flow, shared types (`RawEdgeRow`, `EdgeCountSplit`, `GetEdge`, `EdgeTotals`, `EdgesView`, `GetParams`), shared canonicalization CTE, sequencing | ADR-001..007 cross-cut |
| `store-display-cap-constant.md` | `GET_EDGE_DISPLAY_LIMIT` constant in `read.rs`, re-exported via `lib.rs`; bound to `LIMIT ?`, referenced by render + tests; no literal `3` | FR-18, ADR-006 |
| `store-neighbor-source.md` | Plain neighbor path gains additive `source` only; `authored` derived from `EDGE_SOURCE_AGENT` exact match; carried-forward authored | FR-6, FR-16, FR-17, ADR-004 |
| `store-ranked-query.md` | Ranked variant: shared `deduped` CTE → locked `ORDER BY (d.source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?`; `LEFT JOIN entries t` for rank key; direction hint flows SQL→projection | FR-1, FR-9, ADR-001, ADR-006, ADR-007 |
| `store-split-count.md` | Byte-identical `deduped` CTE → `SUM(CASE …)` uncapped split count; `↔` bucketed to inbound (symmetric-once); parity with ranked query | FR-8, FR-10, FR-11, R-01/R-03, ADR-001, ADR-007 |
| `get-edge-vocabulary.md` | Exact 5-field discovery projection, no enrichment; batched title fetch via `fetch_titles_batch`; dangling neighbors retained | FR-4, FR-5, FR-15, ADR-002 |
| `serializer-seam.md` | `entry_to_json` / `format_entry_markdown_section` signatures UNCHANGED; `None ⇒ key/section never inserted` (structural); nested `edge_totals` JSON + flat markdown `### Related` + summary digest vocabulary | FR-13, FR-14, ADR-003, ADR-005 |
| `get-edge-assembly.md` | `build_edges_view` orchestration after successful primary read; FR-19 fail-loud mapping to `ServerError::Core(CoreError::Store(e))` (identical to `entry_store.get` at tools.rs:963-965); opt-out short-circuit | FR-2, FR-3, FR-19, ADR-002 |
| `get-params.md` | `include_edges` param parsing, opt-out (`Some(false)`) skip, default behavior | FR-2, FR-3 |

### Flagged in-artifact (non-blocking)

- **OQ-02 summary-digest `↔` sub-tally** — `serializer-seam.md` self-identifies that the
  locked `EdgeTotals{inbound,outbound}` shape cannot express a distinct symmetric count for
  the `5↑ 2↓ ↔3` digest form, and names two bounded resolutions: (a) add a `symmetric: usize`
  aggregate to `EdgeCountSplit`/`EdgeTotals`, or (b) render `N↑ M↓ (K authored)` without a
  `↔` sub-tally. Delegated to architect/spec per SCOPE OQ-02 + ADR-005; both options preserve
  every locked invariant. Flagged explicitly rather than invented.

## Knowledge Stewardship

- Queried: context tooling unavailable (Unimatrix MCP disconnected) — non-blocking per
  protocol. Read the 7 ADR files (ADR-001..007) + SPECIFICATION.md + RISK-TEST-STRATEGY.md
  directly as the authoritative source instead.
- Deviations from established patterns: none.
  - Additive `Option<T>` field per vnc-020 (`RawEdgeRow.source`, `target_confidence`).
  - Constants co-located in `read.rs` + re-exported via `lib.rs` per crt-034 / vnc-015
    (`GET_EDGE_DISPLAY_LIMIT`).
  - `fetch_nodes_batch` positional-bind title precedent followed for `fetch_titles_batch`.
  - `try_get` + `StoreError` mapping at the store boundary.
  - Primary-read `ServerError` mapping at tools.rs:963 reused for FR-19 fail-loud.
- Stored: nothing novel (read-only tier; MCP disconnected regardless).
