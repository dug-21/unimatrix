# Component 4 — Tool description (`mcp/tools.rs`, MODIFY)

## Purpose

Document both axes, the `summary` default (with per-tool divergence during the suite migration
window), the `format=markdown` rejection, and the lifecycle-vs-delivery status caveat (FR-12,
AC-09, ADR-002 §7, SR-09/R-11). Prose only — no logic.

## The twin-literal hazard (entry #5457 / #5449 — do NOT collapse)

The `context_graph` description exists as TWO byte-identical `&str` literals that
`test_graph_tool_attr_description_matches_const` (#869) asserts equal to the byte:

1. `CONTEXT_GRAPH_DESCRIPTION` mirror const — `tools.rs:76`
2. the live `#[tool(name="context_graph", description = "…")]` attribute literal — `tools.rs:3985`

rmcp 1.7.0 cannot consume a const inside the attribute, so they cannot be single-sourced. Edit
**both** with identical wording. Rules from #5457:

- Each physical line ends with ` \` (space + backslash): the `\`<newline> continuation strips
  the newline AND all leading whitespace of the next line. So indentation need NOT match
  between the two literals (const uses 6-space continuation indent; the live literal uses
  14-space) — but the WORD TOKENS, the trailing space before each `\` (the inter-word
  separator across a break), and explicit `\n\` mode/section separators MUST match. A missing
  trailing-space-before-backslash silently glues two words and red-bars #869.
- After editing, format with `cargo fmt -p unimatrix-server` (edition 2024) — NOT a bare
  `rustfmt --edition 2021 <file>` (errors on let-chains AND rewrites out-of-scope sibling files
  via `#[path]` resolution). If stray churn lands, `git checkout --` the unintended files.

## Edit — append a two-axis block

Both literals currently end the modes list at `path`, then close with:
```
…active successors before BFS begins.\n\
Requires Read capability. All modes are read-only.
```

Insert a new `\n\`-separated block between the `path` mode paragraph and the final
`Requires Read capability…` sentence, identical in both literals. Wording (semantic content —
keep tokens identical across the two copies; error-copy is asserted by substring, not verbatim,
so exact sentence phrasing is not test-locked, but the two literals must match each other):

```
Output axes (two orthogonal parameters):\n\
- format: serialization only — "json" (default) or "markdown". context_graph currently \
  accepts only json; "markdown" is rejected with an invalid-params error (no graph-markdown \
  renderer exists yet — use format=json). Legacy format="summary" is a deprecated alias for \
  detail=summary (json); combining it with an explicit detail is rejected.\n\
- detail: verbosity — "summary" (default) or "full". Default is summary for context_graph, a \
  lean projection (this differs from tools not yet migrated to the two-axis model). summary \
  nodes carry {id, title, category, tags, status, confidence, content_preview, \
  content_truncated}; summary edges carry {source_id, target_id, relation_type, depth}. \
  content_preview is the first 256 bytes of content (UTF-8 boundary floored, no ellipsis); \
  content_truncated flags that the full content was elided — fetch it with context_get. \
  detail=full returns the complete records unchanged. On neighbors and path (edge-only modes) \
  detail is accepted and ignored.\n\
  NOTE: summary status is the entry LIFECYCLE status (active/deprecated/proposed/quarantined), \
  NOT capability delivery status (missing/partial/proven/claimed, which lives inside the entry \
  content). A subgraph of capability entries shows status "active" for every node; use \
  context_get for a node's delivery state.\n\
```

Must convey (checklist — FR-12/AC-09/SR-09):
- [x] both axes named, values, defaults
- [x] `format=summary` default divergence from unmigrated tools (SR-04)
- [x] `format=markdown` rejected + pointer to `format=json` (SR-05)
- [x] summary node + edge field sets
- [x] `content_preview` 256-byte UTF-8 floor, no ellipsis; `content_truncated` → `context_get`
- [x] lifecycle status ≠ delivery status, with the "active for every capability node" caveat
      (SR-09) — the single most load-bearing sentence

## Related surface (owned by graph_read.md, not edited here)

The `GraphParams.format` / new `detail` schemars field docs (`graph_read.rs`) are separate
single-source doc comments and NOT covered by the #869 byte-equality guard (#5457 gotcha 3).
They are updated in component 3 (E-1). Keep their wording consistent with this description but
do not attempt to unify them.

## Error handling

None — static string edit.

## Key test scenarios (hints; full plan in test-plan/tools.md)

- #869 byte-equality guard (`test_graph_tool_attr_description_matches_const`) stays green after
  editing both literals (proves they remain identical).
- Substring assertions on the description (existing `CONTEXT_GRAPH_DESCRIPTION` substring test,
  tools.rs ~:6241): assert presence of `detail`, `summary`, `format=json`, and a
  lifecycle-vs-delivery phrase — substrings, not the full sentence (R-13).
- Doc-review gate (R-11, not a functional test): confirm the lifecycle-vs-delivery caveat and
  the default-flip disclosure appear; AC-06 notes reference the same caveat. Do NOT write a
  test that treats delivery-status absence as a defect.

## Constraints honored

- Twin-literal integrity (#5457/#5449): both literals edited identically; #869 stays green.
- SR-04/SR-05/SR-09: default flip, markdown rejection, and the status caveat all disclosed.
