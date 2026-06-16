# Agent Report — crt-055-agent-1-pseudocode

**Agent**: crt-055-agent-1-pseudocode | **Stage**: Session 2 Stage 3a (pseudocode) | **Date**: 2026-06-16

## Deliverables

Per-component pseudocode under `product/features/crt-055/pseudocode/` (OVERVIEW + 9 component files matching the Component Map filenames):

- `pseudocode/OVERVIEW.md` — component interactions, data flow, shared types (`CycleReviewRecord` new fields, `CycleAggregates`, `MetricAvailability`, `ReloadWindow`), verified codebase anchors, wave order, binding constraints.
- `pseudocode/cycle_review_index_schema.md` — Component 1 (schema v5 + record + v29→v30 migration).
- `pseudocode/store_cycle_review.md` — Component 2 (single writer, four returns, no zero-clobber).
- `pseudocode/aggregate_reckoning.md` — Component 3 (rank 1/2/3).
- `pseudocode/reload_overlap_engine.md` — Component 4 (one engine, two windows, basis-points conversion).
- `pseudocode/compaction_reckoning.md` — Component 5 (seconds-normalized gate + read accessor).
- `pseudocode/activity_fold_landing.md` — Component 6 (read-before-purge, width conversion, JSON).
- `pseudocode/fail_loud_guard.md` — Component 7 (per-metric availability + coarse/directional).
- `pseudocode/auto_close.md` — Component 8 (#593 synchronous cycle_stop).
- `pseudocode/review_pipeline.md` — Component 9 (ordered pipeline, ordering invariants).

## Components covered

All 9 from the brief's Component Map. Every binding constraint reflected: every metric column INTEGER (basis-points i64, no f64/REAL, no is_finite guard); seconds-normalization (read ts millis ÷ 1000 floor before seconds-vs-seconds gate); read-before-purge ordering (asserted, inversion zeroes); single writer / no zero-clobber (four-return discipline); coarse/directional behavioral signals; structural leak gate (no content field, integers/JSON-count-map only).

## Grounding (interfaces verified against live code, not invented)

crt-054's producer surfaces are ALREADY LANDED in `crates/`: `ActivitySnapshot` (`infra/transcript_activity.rs:106`, `MAX_SIGNAL_CLASSES=16`), `activity_snapshots_for_feature` (`infra/session.rs:560`), `compaction_events` table + `insert_compaction_event` (`write_ext.rs:195`), and `CURRENT_SCHEMA_VERSION = 29` (crt-054 took v29). All `store_cycle_review`/`CycleReviewRecord`/migration-template/`Staleness`/`build_cycle_review_record`/`compute_context_reload_pct` anchors read from source and cited with line numbers in OVERVIEW.

## Open questions / gaps flagged (none left as placeholders)

1. **`context_reload_pct` encoding multiplier (load-bearing).** ADR-005 (#5047) and the brief say `compute_context_reload_pct` "returns a percentage" and basis points = `round(pct × 100)`. The LIVE function (`session_metrics.rs:47`) returns a **fraction in [0.0, 1.0]** (e.g. `2.0/3.0`), not a 0–100 percentage. Correct basis-points encoding from a fraction is `round(fraction × 10000)` (0.375 → 3750), NOT `round(fraction × 100)`. Pseudocode uses `round(fraction × 10000)` and flags the confirm-at-impl. The worked example 37.5% → 3750 holds either way; only the multiplier wording differs. Tester's 37.5%→3750 round-trip is the guard. **Recommend the architect/spec confirm the wording so impl doesn't ship a 100× error.**

2. **Rank-3 injection_log table/column names + "served to this cycle" predicate** (Architecture §10 Q1, R-16). `query_log` is established (`query_log.phase`, `migration.rs:576`); `injection_log` surface must be verified against the current schema at implementation — a wrong table name yields a silent zero. Pseudocode defines the union/dedup logic; the exact names are a confirm-at-impl.

3. **Phase-level `cycle_events` event_type literals** for rank-1 (phase start/end). The handler uses `"cycle_start"`/`"cycle_stop"`; phase-level literals (e.g. `cycle_phase_start`/`cycle_phase_end`) must be confirmed against the live writer and mapped — not invented.

4. **`signal_class_counts_json` catalog name source** — how crt-055 obtains the `[transcript_signals]` `class_name` list at review (config read vs a producer accessor). v1 fallback names `error`/`refusal` are pinned; confirm reachability at impl.

5. **`auto_close` `cycle_stop` writer signature** — reuse the existing `insert_cycle_event`; confirm a synchronous-callable helper (Architecture §10 Q2). No second writer.

6. **`CURRENT_SCHEMA_VERSION` integer (30 assumed)** — SM merge-coordination point vs crt-054 (lesson #4095); v29→v30 assumed since crt-054 took v29. Confirm next-free at merge.

7. **`signal_class_counts_json` Default mismatch** — DB column DEFAULT is `'{}'` but `String::default()` is `""`. Pseudocode mandates writer always binds a valid JSON object and read mapper coalesces `""`→`"{}"` to keep the NOT NULL '{}' contract honest.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get — retrieved ADR-001..010 (#5037/5039/5042/5044/5045/5046/5047/5048/5051), patterns #4750 (four returns), #4178 (derived aggregates), #5052 (intra-stamp on now-prior migration block), #4373/#4092 (migration cascade/idempotent ALTER). Applied: single-writer four-return discipline, crt-047 ALTER template, intra-stamp requirement on crt-054's v28→v29 block, basis-points integer encoding, seconds-normalization gate.
- Deviations from established patterns: none. One source-of-truth wording discrepancy flagged (Open Q1: fraction-vs-percentage multiplier for basis points) — a documentation/spec precision item, not a pattern deviation.
- Stored: nothing (read-only tier — pseudocode-stage agent).
