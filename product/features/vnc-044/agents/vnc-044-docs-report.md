# vnc-044 uni-docs Report

Feature: vnc-044 (#913, PR #920) — split `context_graph` `format` overload into serialization (`format`) + verbosity (`detail`) axes; add lean node projection.

## Blast Radius Determination

- **README.md** — documents `context_graph` and the common `format` parameter at row/intro level. In blast radius.
- **docs/** — grep for `context_graph`, `detail=summary`, `format ... summary` across `docs/` returned no matches. No `docs/` file documents this behavior. Out of blast radius; left untouched (no full-tree audit).

## Sections Modified (README.md)

1. **MCP Tool Reference intro (line ~506)** — Added `context_graph` as a second exception to the "all tools accept `format: summary|markdown|json`" statement: `format` is serialization-only (`json`; `markdown` rejected until a graph renderer ships), plus a separate `detail: summary|full` verbosity axis defaulting to `summary`, and legacy `format=summary` retained as a deprecated alias for `detail=summary`. Traces to SPEC FR-1, FR-3, FR-8, FR-9; SCOPE D-1, D-2.

2. **`context_graph` row — Purpose column** — Added a "Response axes" clause documenting: serialization-only `format` (`markdown` rejected with `ERROR_INVALID_PARAMS`, no silent JSON fallback); `detail` axis (`summary|full`, default `summary`); the lean summary node projection field set `{id,title,category,tags,status,confidence,content_preview,content_truncated}`; `content_preview` = first ≤256 bytes on a UTF-8 char boundary, no ellipsis; `content_truncated` flag; edge projection `{source_id,target_id,relation_type,depth}`; the lifecycle-vs-delivery `status` caveat; `detail=full` returns full payload; `neighbors`/`path` accept-and-ignore `detail`. Traces to SPEC FR-4, FR-5, FR-6, FR-7, FR-8, FR-10, FR-11, FR-12; SCOPE D-3, D-6, AC-03/03b/08.

3. **`context_graph` row — Key params** — Added `detail` (`summary` default | `full`) and `format` (`json`; `markdown` rejected) to the parameter list. Traces to SPEC FR-1, AC-09.

## Claims Verified Against Artifacts

Every edit traces to SPECIFICATION.md functional requirements or SCOPE.md settled decisions (cited above). No source code read; no invented parameters. The default-summary behavior change, the exact 8-field projection set, the 256-byte UTF-8-floored preview, the lifecycle-not-delivery status caveat, and the markdown-rejection are all stated verbatim in the artifacts.

## Not Done / Out of Scope

- No `docs/` edits (no `docs/` file in blast radius).
- Did not document the suite-wide ADR migration of other context tools (`context_get`, `context_search`, etc.) — explicitly deferred (SCOPE Non-Goal 1; vnc-044 is graph-only). The intro's "all tools accept `format: summary|markdown|json`" remains accurate for the unmigrated tools.
- No git operations performed (leader owns git).

## Self-Check

- [x] Read SCOPE.md and SPECIFICATION.md before editing
- [x] Read current README.md sections; grepped docs/ for blast radius
- [x] All edits trace to specific SCOPE/SPEC claims
- [x] No general source-code audit; understanding from artifacts
- [x] Only README.md modified; docs/ untouched (blast-radius only)
- [x] No aspirational language ("will"/"planned"/"future")
- [x] Terminology: Unimatrix, context_graph, ERROR_INVALID_PARAMS consistent
- [x] Tool count (14) unchanged and still matches; no rows added
- [x] No git commands run

## Knowledge Stewardship

Exempt per agent definition — uni-docs reads feature artifacts and proposes README edits; it does not generate or query Unimatrix knowledge entries. No storage or query expected or performed.
