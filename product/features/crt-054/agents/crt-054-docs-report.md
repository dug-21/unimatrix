# crt-054-docs — Agent Report

**Feature**: crt-054 (#752) — transcript-fold producer
**Branch**: feature/crt-054
**Commit**: 8cde35be — `docs: update README for crt-054 (#752)`

## Sections modified (README.md only)

1. **Configuration** — added a `[transcript_signals]` config block immediately
   after the `[retention]` block. Documents: sibling-to-`[retention]` placement,
   `#[serde(default)]`, per-entry `{ class_name, pattern, enabled }`, the single
   shared-RegexSet scan, loud startup validation (>16 enabled classes / invalid
   regex / duplicate `class_name` aborts startup), the default catalog
   (`error` index 0, `refusal` index 1; domain-neutral, no SDLC literals), and the
   directional-not-precise content-opacity caveat.

2. **Security Model → Transcript Handling** — added a paragraph on the content-free
   fold (running byte total, delta count, per-class match counts), that it is a
   scalar counter and never a query over the assembled transcript (no content
   escapes), counts are directional not precise, and that the held-buffer store
   is a verified fail-loud startup precondition because the fold must survive to
   cycle review.

## Deliberately NOT changed

- **Storage / Data Layout schema-version + table-count lines** ("Schema version 25",
  "21 tables", "schema v27"). These are already mutually inconsistent and predate
  crt-054 (none reflect the pre-existing v28). The feature's schema bump to v29 is on
  a NEW table (`compaction_events`); the README has no accurate table inventory or
  schema-version surface to extend, and silently bumping one stale number to 29 while
  leaving the others wrong would be incorrect and outside this feature's lane. No
  edit made — flagged here for a future docs-consistency pass.
- **No `compaction_events` table-inventory row** — the README documents no per-table
  inventory to add a row to; `compaction_events` is server-internal (no MCP tool,
  skill, CLI flag, or knowledge category), so it has no user-visible surface to
  document beyond the schema numbers above.

## Tracing

All edits trace to SCOPE In-scope items 4–6 / Migration; SPECIFICATION FR-A1,
FR-B1/B3/B7, FR-C1/C2/C3, NFR-1, NFR-7, AC-10/10a/11; and CALIBRATION.md (directional
counts). No source code read. Only README.md modified.
