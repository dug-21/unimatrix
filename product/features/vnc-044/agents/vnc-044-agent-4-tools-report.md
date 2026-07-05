# Agent Report — vnc-044-agent-4-tools

**Component:** Tool description (`crates/unimatrix-server/src/mcp/tools.rs`)
**Task:** Document the two-axis output contract on the `context_graph` tool description (both axes, `summary` default + per-tool divergence, `format=markdown` rejection, lifecycle-vs-delivery status caveat) per pseudocode/tools.md, honoring the twin-literal byte-equality guard.

## Result — DONE

Appended an "Output axes" block between the `path` mode paragraph and the closing
`Requires Read capability…` sentence in BOTH `context_graph` description literals, byte-identically:

1. `CONTEXT_GRAPH_DESCRIPTION` mirror const (tools.rs:~76)
2. the live `#[tool(name="context_graph", description = "…")]` attribute literal (tools.rs:~3985)

The new block documents:
- `format` = serialization only (`json` default | `markdown`); markdown rejected loudly → use `format=json`; legacy `format="summary"` deprecated alias for `detail=summary`, conflict with explicit `detail` rejected.
- `detail` = verbosity (`summary` default | `full`); summary default is context_graph-specific and differs from tools not yet migrated (SR-04); summary node/edge field sets; `content_preview` = first 256 bytes UTF-8-floored, no ellipsis; `content_truncated` → fetch via `context_get`; `detail=full` unchanged; `detail` accept-and-ignore on neighbors/path.
- The load-bearing SR-09 caveat: summary `status` is the entry LIFECYCLE status (active/deprecated/proposed/quarantined), NOT capability delivery status (missing/partial/proven/claimed, which lives in `content`); a capability subgraph shows `active` for every node; use `context_get` for delivery state.

Added three substring tests (R-13 discipline — running phrases, not spec wording):
- `test_graph_description_documents_detail_axis`
- `test_graph_description_states_markdown_rejection`
- `test_graph_description_states_lifecycle_status_caveat`

R-11 respected: no test asserts delivery-status absence; the lifecycle caveat is verified as PRESENT only.

## Files modified
- `crates/unimatrix-server/src/mcp/tools.rs` (both description literals + 3 new substring tests)

## Tests
`cargo test -p unimatrix-server --lib` (filtered): **5 passed, 0 failed**, incl.:
- `test_graph_tool_attr_description_matches_const` (twin-literal byte-equality guard) — **GREEN** (proves both literals are byte-identical after edit)
- `test_context_graph_description_contains_staleness_text` (pre-existing substring guard) — GREEN
- 3 new substring tests — GREEN

`rustfmt --edition 2024 --check crates/unimatrix-server/src/mcp/tools.rs` → exit 0 (no reflow needed; did not run workspace `cargo fmt`).

## Issues / flagged breakage
- None. Change is doc-string + test only; no logic touched. Scope held to tools.rs.
- No git commands run (leader owns git).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced the twin-literal trap (#5457), the vnc-043 twin-literal ADR (#5449), the byte-equality guard rationale (#869), the ADR-001 two-axis contract (#5509), and the "lying description" hazard (#4303). Applied all directly.
- Stored: nothing novel — the twin-literal edit-both-identically pattern and the byte-equality guard are already captured in entries #5457/#5449/#869, and this task applied that known pattern without discovering a new gotcha. No new failure mode or integration trap surfaced.
