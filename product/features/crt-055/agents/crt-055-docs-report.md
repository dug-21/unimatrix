# crt-055 docs agent report

**Agent**: crt-055-docs | **Feature**: crt-055 | **Issue**: #755
**Scope**: README.md only (targeted edits to sections affected by crt-055).

## Inputs read
- `product/features/crt-055/SCOPE.md`
- `product/features/crt-055/specification/SPECIFICATION.md`
- `README.md` (current)

No source code read. All claims trace to SCOPE/SPECIFICATION.

## Feature (user-visible surface)
crt-055 modifies the `context_cycle_review` MCP tool:
- New `auto_close: bool` param (default false) — writes `cycle_stop` synchronously before the review when none exists (FR-18/FR-19, AC-15, folds #593).
- Changed report output: durable per-cycle aggregate columns (phase durations/transitions/rework incl. declared-but-never-closed hotspots; rework ratio; knowledge-reuse served-count union; transcript byte/delta throughput; behavioral-signal counts; compaction count) plus two distinct, never-collapsed reload metrics (`context_reload` cross-session + `compaction_reread` post-compaction) — FR-03..FR-17.
- Two presentation-honesty rules: "unavailable" instead of a believable `0` (FR-01/AC-01); coarse/directional qualifier on content-opaque behavioral-signal counts (FR-11b/AC-21).
- Bytes, not tokens; informs, never controls (NFR-07/NFR-08).

Schema-internal facts (SUMMARY_SCHEMA_VERSION 4→5, cycle_review_index migration, basis-points encoding, clock-unit gate) are implementation internals — deliberately NOT surfaced in README.

## Sections modified
1. **MCP Tool Reference** — `context_cycle_review` row (Purpose + Key params): added durable-aggregate / dual-reload / unavailable-not-zero / coarse-directional summary and the `auto_close` param. Tool count unchanged (14 — modifies an existing tool, adds no new tool/skill/category).
2. **Core Capabilities → Cycle Review Analysis** — added one paragraph describing the durable per-cycle aggregates, the two reload metrics (never collapsed), behavioral-signal counts, the two presentation-honesty guards, the bytes-not-tokens / informs-not-controls boundary, and `auto_close`.

## Sections intentionally left unchanged
- `[transcript_signals]` config block and the Security-section content-free fold paragraph already existed (landed by crt-054, the producer) and already state "directional, not precise" — no edit needed.
- Tips for Maximum Value item 4 — generic retrospective guidance; `auto_close` now covered in the tool reference; no rewrite warranted.
- Tool/Skill/Category counts unchanged (no additions).

## Self-check
- Read SCOPE.md and SPECIFICATION.md before editing — yes.
- Read current README before editing — yes.
- All edits trace to SCOPE/SPEC claims — yes.
- No source code read — yes.
- Only README.md modified (plus this report) — yes.
- No aspirational language ("will"/"planned"/"future") — yes.
- Terminology: Unimatrix, `context_cycle_review`, SQLite — consistent.
- Table row/count consistency: 14 tools unchanged; no count drift.

## Commit
README + this report committed to `feature/crt-055` with `docs:` prefix. Not pushed.
