# Test Plan — serializer-seam (`format_single_entry` + 3-format render)

`format_single_entry` gains `edges: Option<&EdgesView>`; `entry_to_json` /
`format_entry_markdown_section` signatures **unchanged**; the `edges` key / `### Related` section
/ summary digest injected by the get path only. Owns **R-07 (byte-identity, High)**, **R-17
(format-string drift, Medium)**, the `↔` glyph render, and the `…N more` pointer (referencing
`GET_EDGE_DISPLAY_LIMIT`). Server unit tests + the integration byte-identity golden.

## Unit Test Expectations

### R-07 — Byte-identity & `None ⇒ key absent` (High, SR-01)

**`test_none_edges_key_absent_structural`** (C-4, the invariant)
`format_single_entry(entry, fmt, None)` for each format produces output with **no `edges` key**
(json), **no `### Related`** (markdown), **no `edges:` digest** (summary). This is structural —
the key is never added, the section never appended when `edges == None`.

**`test_entry_to_json_signature_unchanged`** (ADR-003)
Assert (compile + inspection) `entry_to_json` and `format_entry_markdown_section` signatures are
unchanged — the `edges` injection is layered by the get path, not threaded through these helpers.

**`byte_identity_via_real_producer`** (R-07/AC-07, #1268) — primarily integration:
Captured through the **real** serializer / tool handler (the infra-001 MCP harness IS the real
producer). Lives in the integration suite as `test_list_view_tools_no_edges_key` across
`context_search`/`lookup`/`store`/`correct` × 3 formats, asserting byte-equality vs a pre-vnc-037
baseline. NOT hand-crafted expected strings.

### R-17 — Format render contract (Medium, NFR-7 acceptance surface)

**`test_summary_digest_arrow_split`** (FR-14, OQ-02 form)
Summary/null digest shows the true split distinguishing asymmetric direction from symmetric plus
the authored tally — e.g. `… | edges: 5↑ 2↓ ↔3 (2 authored)` (architect's exact OQ-02 form).
Zero-edge → `edges: none`.

**`test_markdown_flat_ranked_no_subsplit`** (FR-14/AC-08)
`### Related` after the footer, a **flat ranked ≤3** list, each line `- {edge_type} {→|←|↔}
#{target_id} "{target_title}"`. **Assert the Author-asserted/Inferred sub-split is ABSENT**
(dropped under the reframe — explicitly verify it does not appear). `↔` on symmetric lines, `→`/`←`
on asymmetric.

**`test_markdown_capped_pointer_references_constant`** (FR-12/FR-14/AC-13)
When `total > GET_EDGE_DISPLAY_LIMIT`, a single `_…and N more — use context_graph_` pointer with
`N = total − GET_EDGE_DISPLAY_LIMIT` — **no literal 3** in the threshold or arithmetic (references
the constant). When `total <= GET_EDGE_DISPLAY_LIMIT`, **no** pointer.

**`test_json_edges_and_totals_shape`** (FR-14/AC-08)
JSON: `"edges": [{edge_type,direction,target_id,target_title,authored}]` (ranked ≤3) plus
`"edge_totals": {"inbound": N, "outbound": M}` (uncapped, symmetric-once). Both keys present iff
edges surfaced.

**`test_zero_edge_empty_state_all_formats`** (R-17/AC-06/DNB-3)
Zero-edge: summary `edges: none`; markdown `### Related` + `No related entries.`; json
`"edges": []` + `"edge_totals": {"inbound":0,"outbound":0}`.

**`test_symmetric_renders_arrow_glyph_no_directional`** (R-10/R-17)
A `direction="both"` edge renders `↔` and emits **no** `→`/`←`. An asymmetric edge renders the
directional arrow.

**`test_dangling_title_renders_null_all_formats`** (R-15/DNB-1)
`target_title: None` renders as JSON `null` and a graceful markdown/summary form, no panic, edge
retained.

## Integration Expectations (through MCP)
- `test_list_view_tools_no_edges_key` (AC-07/R-07) — the byte-identity golden via the real
  producer (4 tools × 3 formats).
- `test_get_zero_edge_empty_state_all_formats` (AC-06).
- `test_get_capped_pointer_when_more_than_cap` (AC-08).
- `test_get_symmetric_canonicalized_one_arrow` (AC-08 `↔` render).

## Cross-Component Dependency
- The `…N more` threshold references `GET_EDGE_DISPLAY_LIMIT` (store-display-cap-constant);
  the cap-isolation test (override → rendered set shrinks) observes its effect **here**.
- The totals rendered come from store-split-count; the symmetric-once value is asserted there,
  the render shape here.

## Edge Cases
- Zero edges (all three formats).
- `total > cap` → pointer; `total <= cap` → no pointer.
- Symmetric `↔` vs asymmetric arrow.
- Dangling `null` title.
- `None` edges → byte-identical to list-view (the SR-01 invariant).

## R-18 — File size (cross-cutting, Low)
**`line_count_le_500`** — `response/entries.rs`, `response/edges.rs`, `tools.rs`,
`graph_queries.rs`, `graph_queries_ranked.rs`, `get_edges.rs`, `read.rs` each ≤ 500 lines; any
split landed on the pre-authorized sibling modules (OQ-B).
