# C9 — Markdown render (`render_tags_section`)

> File: `crates/unimatrix-server/src/mcp/response/retrospective.rs` (NEW `render_tags_section`, call
> after `render_goal_section` ~:49).
> Risks: R-12 (Low). ACs: AC-05d.
> **#3337: assert against SPEC/`render_goal_section` PARITY, NOT the ARCHITECTURE illustrative `##
> Tags` header string. Architecture diagram strings are illustrative, not authoritative.**
>
> **PINNED empty-render behavior (reconciled with pseudocode/markdown-render.md + AC-05d):** a
> tag-LESS cycle renders NO `## Tags` section at ALL — no header, no "No tags recorded" fallback.
> `render_tags_section` returns an empty string when `report.tags` is empty, so nothing is emitted
> (satisfies AC-05d "no spurious section"). A cycle WITH tags renders the `## Tags` section (format
> parity with `render_goal_section`) AND the JSON carries `tags`. This is the single pinned contract;
> the pseudocode agent aligned to the same pin.

## Reuse
Pure function `fn(&RetrospectiveReport) -> String` — unit-testable directly (no rmcp context).
Model on existing `render_goal_section` tests in `retrospective.rs`.

## Unit test expectations
- `test_render_tags_section_present` — a report with `tags=["arm:A","workflow:v1.3"]` → the rendered
  string contains a `## Tags` section listing both tags verbatim, formatted parity with
  `render_goal_section`. Derive the expected header/format from the SPEC and `render_goal_section`
  parity — NOT from the ARCHITECTURE sample (#3337).
- `test_render_no_spurious_section_when_empty` — a report with `tags=[]` → `render_tags_section`
  returns an empty string; assert the rendered output contains NO `## Tags` header AND no "No tags"
  fallback text (PINNED: tag-less cycle renders no section at all, FR-10/AC-05d "no spurious
  section"). This diverges from `render_goal_section`'s "No goal recorded" fallback by design — the
  pin is absence, not a parity fallback.
- `test_render_tags_verbatim_no_derivation` — colon-prefixed and bare tags rendered as stored, no
  namespace splitting.
- `test_render_tags_order_deterministic` — rendered order follows the getter's `ORDER BY tag`
  (deterministic output).

## Contract
Empty vs present render both covered. The AC-05d assembled proof (markdown actually appears in a real
review) lives in review-handler.md; this file proves the render function in isolation.
