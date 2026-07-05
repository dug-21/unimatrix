# Agent Report — vnc-044-agent-1-pseudocode

**Role:** Pseudocode specialist (Session 2 Stage 3a)
**Feature:** vnc-044 — `context_graph` two-axis output (serialization + verbosity)

## Deliverables

Per-component pseudocode under `product/features/vnc-044/pseudocode/`:

- `OVERVIEW.md` — component interaction, data flow, the serialization seam, shared types, build order
- `verbosity.md` — component 1: shared verbosity primitives (`response/verbosity.rs`, new)
- `graph_read_projection.md` — component 2: graph projection (`graph_read_projection.rs`, new)
- `graph_read.md` — component 3: output resolver + seam threading (`graph_read.rs`, modify)
- `tools.md` — component 4: tool description twin-literals (`tools.rs`, modify)

Components covered: 4 (verbosity primitives, graph projection, resolver+threading, tool
description) + a documentation-only note for `graph_read_validation.rs` folded into graph_read.md.

## Interface fidelity

All names/signatures traced to source, not invented:
- `handle_graph`, `GraphParams`, `EdgeRecord`, and the 7 envelopes read from `graph_read.rs`
  (envelope metadata fields confirmed: SubgraphResponse `truncated`/`seed_ids`/`depth_reached`;
  ChainResult `truncated:Truncated{forward,backward}`; CurrentResponse single `entry`;
  Inverse/FilterResponse `total_returned`).
- `status_str(Status)->&'static str` is `pub(crate)` in `crate::mcp::response` (mod.rs:110);
  `EntryRecord` fields (`content:String`, `confidence:f64`, `tags:Vec<String>`) confirmed.
- `ServerError::InvalidInput{field,reason}` → `ErrorData(ERROR_INVALID_PARAMS)` via existing
  `From` impl (error.rs:251) — so `parse_detail` returns `ServerError`, resolver adapts via
  `.map_err(ErrorData::from)`.
- Twin-literal locations confirmed: `CONTEXT_GRAPH_DESCRIPTION` const (tools.rs:76) + live
  `#[tool]` attribute (tools.rs:3985); #869 byte-equality guard.
- `response` is `pub mod` in mcp/mod.rs; `graph_read` is `pub(crate) mod`. verbosity added as
  `pub mod verbosity;` under `response`; projection as a `#[path]` child of `graph_read`.

## Critical details pinned (per spawn instructions)

- `content_preview` uses the exact codebase char-boundary loop (`while end>0 && !is_char_boundary(end){end-=1}`) — explicitly NOT `&s[..256]`, NOT nightly `floor_char_boundary`, NOT `.chars().take()`.
- `content_truncated == content.len() > 256` (byte compare), decoupled from the flooring index; 257B-ASCII-floors-to-256 false-negative trap called out (R-02).
- `GraphSummaryProjection` trait impl for all FIVE node-bearing envelopes, each preserving its own metadata; neighbors/path do NOT implement it.
- `resolve_graph_output` rejects `format=markdown` for all seven modes pre-dispatch; legacy `format=summary`+explicit `detail` conflict rejected.
- `detail` accept-and-ignore on neighbors/path; no per-mode validation arm.
- Shared `EntryRecord`/`EdgeRecord`/`ResponseFormat`/`parse_format` untouched; projection is a distinct type + `serde_json::Value` edge builder.
- `256` single-sourced as `CONTENT_PREVIEW_BYTES`.

## Open questions / gaps

1. **`graph_read.rs` line budget (non-blocking, C-7).** Currently 389 lines; the five edits add
   ~70 → ~460. Under 500 but close. Escape hatch documented: relocate `resolve_graph_output` +
   `GraphSerialization` to `graph_read_validation.rs` if it crosses. Implementer should measure.
2. **`parse_detail` case-sensitivity (decision, low-risk).** I mirrored the established
   `parse_format` idiom (`to_lowercase()`, case-insensitive). ADR-001 §2 lists lowercase
   `summary|full`; case-insensitive is a strict superset and consistent with the suite. Flag if
   the tester expects strict-case rejection.
3. **Summary envelope key order not contractual.** `to_summary_json` builds a `serde_json::Value`
   via `json!`; only `detail=full` (untouched struct path) carries the byte-for-byte guarantee.
   AC-03 asserts key SET (present+absent), not order — consistent, but noted for the tester.
4. **Pre-existing over-limit `graph_read_subgraph.rs` (742 lines).** Out of scope; must not be
   added to. Flagged for a future cleanup feature (carries the architect's OQ-2).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` (task: two-axis projection pseudocode) — surfaced
  ADR-001 #5509, ADR-002 #5510 (the ratified contract), and the twin-literal pattern #5457
  (context_graph description = two byte-identical literals + #869 byte-equality guard; edition-2024
  fmt; schemars field docs not guarded). Also relevant: #4518 (extract graph module at line
  limit), #4491/#4490 (GraphParams layout lock), #4478 (EdgeRecord placement), #5449 (vnc-043
  twin-literal source-of-truth). All applied — the twin-literal editing rules are encoded in
  tools.md; the projection-in-new-module + resolver-relocation escape hatch encode #4518.
- Deviations from established patterns: none. The design extends existing idioms
  (`parse_format`/`status_str` reuse, `#[path]` submodule decomposition, twin-literal +
  byte-equality guard, additive `Option<T>` GraphParams field per ADR-003).
- Storage: none expected (read-only tier).
