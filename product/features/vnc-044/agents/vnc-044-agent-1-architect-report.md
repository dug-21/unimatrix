# vnc-044 Architect Report — agent vnc-044-agent-1-architect

## Deliverables

- `product/features/vnc-044/architecture/ADR-001-two-axis-format-verbosity-contract.md` — suite-wide contract (Unimatrix #5509)
- `product/features/vnc-044/architecture/ADR-002-context-graph-adoption.md` — graph adoption (Unimatrix #5510)
- `product/features/vnc-044/architecture/ARCHITECTURE.md` — context_graph implementation of the contract

## Ratified axis (OQ-4)

**`detail`**, values **`summary | full`**. Default `summary`. Legacy `format=summary` = deprecated alias for `detail=summary` (serialization `json`). `format` = serialization only (`markdown | json`). Shared 256-byte constant = `CONTENT_PREVIEW_BYTES`. Summary field set = `{id, title, category, tags, status, confidence, content_preview, content_truncated}`.

## Key decisions

- Shared primitives (`Detail`, `parse_detail`, `CONTENT_PREVIEW_BYTES=256`, `content_preview()`) single-sourced in new `mcp/response/verbosity.rs`; per-tool summary field-set overrides explicitly allowed (SR-03).
- Graph uses its own `resolve_graph_output`, NOT shared `parse_format` — shared `ResponseFormat`/`parse_format`/`EntryRecord`/`EdgeRecord` untouched (SR-06/SR-07).
- Lean projection = distinct `NodeSummary` type + `GraphSummaryProjection` trait in new `mcp/graph_read_projection.rs` (NOT the already-742-line `graph_read_subgraph.rs`) (SR-07/SR-08).
- `content_preview` uses the codebase char-boundary idiom (`while end>0 && !is_char_boundary(end)`), not nightly `floor_char_boundary`; `truncated = len>256` byte compare (SR-02).
- `detail` is a universal `GraphParams` field (no per-mode rejection); markdown rejected loudly for all modes; neighbors/path accept-and-ignore.
- SR-09 made loud in both ADR-001 and ADR-002 and mandated in the tool description: summary `status` is lifecycle, not capability delivery status; delivery-status promotion is named follow-up #3.

## Findings surfaced

- `graph_read_subgraph.rs` is **already 742 lines** — pre-existing over-limit debt. vnc-044 must not add to it (flagged; splitting it is out of scope).
- `content` is still read from DB (`fetch_nodes_batch` unchanged); win is wire/context size, not DB I/O (SR-01) — do not promise a query-cost reduction.

## Open questions

- `graph_read.rs` (389 lines) line budget after resolver + arm branching — relocate resolver to `graph_read_validation.rs` if it nears 500 (non-blocking).
- Tester: confirm summary coverage spans all five node-bearing modes (current/inverse/filter), not only subgraph.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get -- reviewed crt-057 render-only `format` (#5434), vnc-002 ADR-004 format-selectable responses (#87), GraphParams locks ADR-003/019/020 (#4490/#4503/#4477), per-mode change pattern (#4500), markdown formatter module pattern (#3459). Applied: generalized crt-057's render-axis rather than superseding it; respected GraphParams Option<T>-add lock; placed projection in its own module per #4500/#4518.
- Stored: entry #5509 "ADR-001 vnc-044: Suite-Wide Two-Axis Output Contract" via context_store (edge Prerequisite->#5434 crt-057); entry #5510 "ADR-002 vnc-044: context_graph Adoption" via context_store (edge Prerequisite->#5509 ADR-001). No supersession/deprecation performed — crt-057 (#5434) is generalized, not replaced.
