# C8 — Review handler populate — ASSEMBLED PATH (read side)

> File: `crates/unimatrix-server/src/mcp/tools.rs` cycle_review handler (~:3409-3428, beside goal
> populate). `report.tags = store.get_cycle_tags(&fc).await.unwrap_or_default()` (degrade + warn).
> Risks: **R-03 (Critical)**, R-04 (degrade). ACs: **AC-05 [assembled-path]**, AC-EXTRA-2 (degrade).
>
> **AC-05 `proven_by` MUST cite an assembled test driving hook→persist→review, NOT a store getter
> (R-03/SR-08).**

## rmcp seam constraint (see OVERVIEW §7 OQ-2, entry #5389)
`context_cycle_review` is an rmcp `#[tool]` handler; `RequestContext<RoleServer>` cannot be built in
unit scope. The tag-populate logic MUST be reachable via an extracted `pub(crate)` seam so the
assembled test drives REAL `get_cycle_tags` + REAL report population — NOT a hand-built
`RetrospectiveReport` asserting its own literal (the #917/#918/#930 holed-capability trap). Flag if
the handler is not seam-extractable.

## AC-05 — assembled surfacing (the anchor test)
- `test_review_surfaces_tags_json_and_markdown_assembled` — (1) drive a Start `RecordEvent` with
  tags through `dispatch_request` (reuse listener-persistence.md helpers), settle; (2) invoke the
  review populate seam for `fc` (real `get_cycle_tags`); (3) assert `report.tags` equals the stored
  set AND the rendered markdown (via C9 `render_tags_section`) shows them AND the serialized JSON
  includes `tags`. This single test chains hook→persist→read→surface. **AC-05 anchor.**

## Source-of-truth (not stale mirror)
- `test_review_reads_cycle_tags_not_summary_json_mirror` — populate `cycle_tags`, then assert the
  review reflects the LIVE `cycle_tags` rows (read fresh each review), not a prior `summary_json`
  copy. A read that trusts a stale mirror over `cycle_tags` is a source-of-truth inversion.

## Degrade (R-04 s3 / AC-EXTRA-2)
- `test_review_degrades_to_empty_on_getter_error` — force `get_cycle_tags` to `Err` (closed pool,
  model T-RES-03 :8590) → `report.tags = []` + `tracing::warn`, review STILL succeeds (parity with
  the `get_cycle_start_goal` degrade arm at tools.rs:3425). Review never fails on tag read.

## Empty case
- `test_review_empty_tags_for_untagged_cycle` — a cycle with no tags → `report.tags == []` (feeds C9
  no-spurious-section).
