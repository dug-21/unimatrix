# C9 — Markdown render `render_tags_section`

**File:** `crates/unimatrix-server/src/mcp/response/retrospective.rs` (NEW fn; parity `render_goal_section` :203-217; call after it at :49)
**ADR:** ADR-004. **Risks:** R-12. **AC:** AC-05d, FR-10.

## Purpose

Render a run's tags as a dedicated `## Tags` markdown section in `context_cycle_review` output. A
cycle WITH tags renders the section; a **tag-less cycle renders NOTHING** — no `## Tags` header, no
fallback line, no blank block (empty string returned).

## PINNED empty-render behavior (AC-05d — supersedes goal parity)

**A tag-less cycle renders NO section at all.** This INTENTIONALLY DIVERGES from
`render_goal_section` parity. Verified at HEAD (`retrospective.rs:203-217`): `render_goal_section`
ALWAYS emits `## Goal` plus a "No goal recorded for this cycle." fallback even when `goal` is `None`
— i.e. it renders an empty section. AC-05d / FR-10 explicitly require **"no spurious section"** for a
tag-less cycle, so `render_tags_section` deliberately does the opposite of goal for the empty case:
it returns an empty `String`. This is the one-line divergence — documented so it is not "corrected"
back to goal parity. (Gate 3a rework item, R-12; tester's `test_render_no_spurious_section_when_empty`
asserts absence of the section, aligned to this pin.)

> Note (R-12 / #3337): the header text (`## Tags`) is illustrative in ARCHITECTURE.md; the LOAD-BEARING
> contract is the empty-case behavior pinned here (no section when tag-less) + the present-case escaping.

## Pseudocode

```
fn render_tags_section(report: &RetrospectiveReport) -> String {
    // PINNED: tag-less cycle → render NOTHING (AC-05d "no spurious section").
    // Deliberate divergence from render_goal_section, which emits an empty section with a fallback.
    if report.tags.is_empty() {
        return String::new();                                 // no header, no fallback, no blank block
    }
    let mut out = String::new();
    out.push_str("## Tags\n");
    for tag in &report.tags {
        let safe = escape_md_text(tag);                       # SAME escaper render_goal_section uses (:208)
        let _ = writeln!(out, "- {}", safe);                  # one bullet per tag; verbatim (opaque)
    }
    out.push('\n');
    out
}
```

### Call site (`format_retrospective_markdown`, after `render_goal_section` at :49)

```
output.push_str(&render_goal_section(report));
output.push_str(&render_tags_section(report));     # NEW — immediately after goal
```

### Notes

- **Escape with `escape_md_text`** (already used by `render_goal_section` :208) — tags are
  attacker-controllable opaque strings; escaping is a downstream-renderer safety measure, not
  validation (value-opacity preserved). Colon-prefixed tags render verbatim (no namespace grouping).
- **Empty case returns `String::new()` (PINNED).** No header, no fallback, no blank block — the
  caller's `output.push_str(&render_tags_section(report))` appends nothing. This satisfies AC-05d's
  "no spurious section" and is the deliberate divergence from `render_goal_section` (which renders an
  empty section). Do NOT re-add a "No tags recorded" fallback.

## Data flow

- **Input:** `&RetrospectiveReport` (with `report.tags` populated by C8).
- **Output:** markdown `String` appended to the review output.

## Error handling

None — pure string building; `writeln!` into a `String` cannot fail (`let _ =` on the fmt Result,
parity :209).

## Key test scenarios (hints)

1. Cycle WITH tags `["arm:A","workflow:v1.3"]` → `## Tags` section lists both as bullets, verbatim,
   deterministic order (C3 sorts).
2. **Cycle WITHOUT tags → `render_tags_section` returns `""`; NO `## Tags` header appears anywhere in
   the review output** (AC-05d "no spurious section"). Tester's `test_render_no_spurious_section_when_empty`
   asserts absence of the section string.
3. The empty-case behavior (no section) is the LOAD-BEARING contract; treat ARCHITECTURE's `## Tags`
   sample as illustrative (R-12/#3337). Do NOT assert goal-parity fallback text for the empty case.
4. A tag with markdown metacharacters is escaped via `escape_md_text` (no injection into the review).
