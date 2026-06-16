# Agent Report — vnc-037-agent-3-serializer-seam

## Scope delivered
Serializer seam (ADR-003) + 3-format render helpers + vocabulary type extensions for the
3-bucket TOTALS BUCKET CONTRACT (AMENDED ADR-005, 2026-06-16).

## Files modified / created
- `crates/unimatrix-server/src/mcp/response/edges.rs` — `EdgeTotals` gains `pub both: usize`
  (3 keys); `EdgesView` gains `pub authored_total: usize` (digest-only, `#[serde(skip)]`).
  Removed the `dead_code` allows on `DIRECTION_BOTH`/`DIRECTION_INBOUND` (now used by render).
  Updated the vocab test to assert a 3-key `edge_totals` incl. `both`.
- `crates/unimatrix-server/src/mcp/response/edges_render.rs` — **NEW** (OQ-B pre-authorized
  split off `edges.rs`): the 3 render helpers `render_summary_digest` (LOCKED byte form),
  `render_markdown_related` (flat ranked ≤cap, `↔` glyph, `…N more` referencing
  `GET_EDGE_DISPLAY_LIMIT`, no literal 3), `render_json_edges`, `render_json_edge_totals`
  (3-key object). Render-helper unit tests live here.
- `crates/unimatrix-server/src/mcp/response/entries.rs` — `format_single_entry` gains
  `edges: Option<&EdgesView>`; per-format branching; `None ⇒ key/section absent` structural.
- `crates/unimatrix-server/src/mcp/response/mod.rs` — registered `mod edges_render`; added
  seam tests (`test_none_edges_key_absent_structural`, `test_none_json_byte_identical_to_base_object`,
  `test_some_edges_injected_all_formats`, `test_get_zero_edge_empty_state_all_formats`);
  relocated the pre-existing `format_redirect_summary` tests here (to keep entries.rs ≤500).
- `crates/unimatrix-server/src/mcp/tools.rs` — both `format_single_entry` call sites
  (lookup-by-id :647, context_get handler :980) pass `None` (next wave flips :980 to `Some`).

## File sizes (R-18, all ≤500)
edges.rs 324 · edges_render.rs 346 · entries.rs 376.

## Tests
- server lib: **4172 passed / 0 failed** (`cargo test -p unimatrix-server --lib`).
- store lib: 389 passed / 0 failed (co-run, untouched by me).
- workspace (`-j 1` to avoid a parallel-link OOM): **rc=0, 0 failures, 59 suite groups OK**.
- New/updated tests cover: byte-identity `None ⇒ key absent` (all 3 formats) + json
  byte-identical to `entry_to_json`; locked digest byte form incl. fixed arity at 0,
  authored-from-full-set, all-zero `edges: none` sentinel; 3-key json `edge_totals` with
  `both` present and `authored_total` NOT a key; markdown flat-ranked no-subsplit, `↔` glyph,
  `…N more` referencing the constant with `both` load-bearing, no-pointer when total≤cap;
  zero-edge empty state in all 3 formats; dangling-title placeholder/null no-panic.

## Issues / blockers
- `cargo test --workspace` (default parallelism) fails at the **linker** with
  `ld terminated with signal 9 [Killed]` on a large integration-test binary — an environment
  OOM, NOT a code regression. All test binaries compile single-threaded and the full suite
  passes with `-j 1`. Flagging for the SM in case CI needs bounded link parallelism.
- Out-of-scope-but-noted: `EdgeTotals`/`EdgesView` are populated by get-edge-assembly (next
  wave); `authored_total` is currently only consumed by the digest render + tests.

## Knowledge Stewardship
- Queried: Unimatrix MCP was disconnected (per spawn note) — read ADR-002/003/005 + pseudocode
  files directly instead. No briefing available.
- Stored: nothing novel to store — the render-helper + seam patterns are fully captured in
  ADR-003/ADR-005 and the component pseudocode; no runtime-invisible gotcha surfaced beyond
  the known `#[serde(skip)]`-keeps-digest-field-out-of-JSON technique already documented in
  the type doc-comment. (MCP disconnected regardless.)
