# Agent Report — crt-055 Component 9: Review pipeline ordering

**Agent ID:** crt-055-agent-3-review_pipeline
**Crate:** unimatrix-server (+ small extensions in unimatrix-observe)
**Wave:** 3 (integration spine for Components 3–8)
**ADRs honored:** ADR-002 (#5037 single writer/no-clobber), ADR-005 (#5047 basis-points), ADR-006 (#5048 clock/unit), ADR-007 (#5042 read-before-purge), ADR-010 (#5045 auto_close)

## Summary

Wired the full six-step `context_cycle_review` pipeline in
`tools.rs` in the binding order, integrating the already-landed Components 3–8.
The pipeline is driven by a new accumulator module `mcp/review_aggregates.rs`
(`ReviewAggregateState`) populated incrementally as the handler loads each source,
then finalized into one `CycleReviewRecord` (all 16 v5 columns) via the SINGLE
full-pipeline `store_cycle_review()` writer, plus a rendered fail-loud presentation
block appended at assembly level.

### Pipeline order wired (verified by source line ordering)

1. **auto_close** (`maybe_auto_close`, pre-existing C8) writes `cycle_stop` BEFORE rank-1 reads the timeline.
2. **Read-before-purge fold** — `review_agg.land_fold(...)` calls C6's `land_activity_fold` STRICTLY BEFORE every `purge_cycle_transcripts` site (full-pipeline purge fires only at the very end). Removed C6's `#![allow(dead_code)]`.
3. **Aggregate reckoning** — rank-2/3 (`populate_ranks_2_3`) where `session_records` + query/injection logs are in scope; rank-1 (`populate_rank_1`) after `cycle_events_vec` is built (so it includes any auto_close `cycle_stop` — not a false #556 never-closed).
4. **Reload reckoning** — `populate_reload` (`context_reload_pct` basis-points i64, no float bind) + `populate_compaction` (per-session `MIN(compacted_at)` boundaries via `min_compacted_at`, `compaction_count_for_sessions`, then `reckon_compaction_reread`).
5. **Per-metric presence flags** — `review_agg.availability()` (C7 `compute_availability`) → rendered block wires into the response so empty sources read "unavailable" not "0" and behavioral signals render coarse/directional (`~N (directional)`).
6. **Persist** — `build_cycle_review_record(..., review_agg.aggregates())` → the SINGLE `store_cycle_review()` at the full-pipeline return ONLY. The three other returns (force+purged, cached-empty, memo-hit) never reach the builder (no-clobber). Guarded-recompute (#758) clears the memo and falls through to this same writer.

### #206-4 / leak gate

#206-4 knowledge-that-helped stays response-time enrichment — NO durable column added (AC-16, ADR-009). No content field on the record or report; `CycleAggregates` carries integers + a class-count map only (AC-19).

## Files modified

- `crates/unimatrix-server/src/mcp/review_aggregates.rs` (NEW — `ReviewAggregateState` orchestration: land_fold, populate_ranks_2_3, populate_rank_1, populate_reload, populate_compaction, availability, aggregates, render_block)
- `crates/unimatrix-server/src/mcp/review_aggregates_tests.rs` (NEW — 11 unit tests)
- `crates/unimatrix-server/src/mcp/mod.rs` (registered `review_aggregates` module)
- `crates/unimatrix-server/src/mcp/tools.rs` (wired the six steps into the handler full-pipeline block; extended `build_cycle_review_record` to take `&CycleAggregates` and land all 16 v5 columns; updated 14 test callers; added 2 builder tests)
- `crates/unimatrix-server/src/mcp/activity_fold_handler.rs` (removed `#![allow(dead_code)]`; updated header)
- `crates/unimatrix-server/src/server.rs` (added `transcript_signal_class_names` snapshot field + `retention_config_signal_class_names()` accessor)
- `crates/unimatrix-server/src/main.rs` (set `transcript_signal_class_names` from `config.transcript_signals.enabled_class_names()` in both daemon + stdio paths)
- `crates/unimatrix-observe/src/fail_loud_guard.rs` (extended `CycleAggregates` with `transcript_bytes_total`, `transcript_delta_count`, `signal_class_counts_json` so the single writer lands all 16 columns 1:1 from one struct; fixed `agg_all_present()` test helper)

## Design notes / deviations (flagged, not silent)

- **`CycleAggregates` extension**: the C7 presentation struct carried only 13 of the 16 columns (missing `transcript_bytes_total`/`transcript_delta_count`/`signal_class_counts_json`). I added them so `build_cycle_review_record` is a clean 1:1 copy per pseudocode 2c (`CycleAggregates` is the value bundle for all 16). They are content-free aggregate fields.
- **rank split**: `populate_rank_1_2_3` couples rank-1 (cycle_events) with rank-2/3, but cycle_events is read LATER than session_records in the live handler. I split into `populate_ranks_2_3` (called with empty events to fill rank-2/3) + `populate_rank_1` (overwrites rank-1 from the real timeline). The observe reckoner stays the single path.
- **class_names source**: `TranscriptSignalsConfig::enabled_class_names()` exists (C6 agent added it) but wasn't on the server. Added a startup snapshot (`transcript_signal_class_names`) mirroring `store_config`/`retention_config`, wired in both main.rs paths. Empty in tests → `signal_class_counts_json == "{}"`.
- **fail-loud render attach**: appended as a Content item on the `CallToolResult` (pattern #4866 assembly-level attach), NOT a serde field on `RetrospectiveReport` — avoids touching the many report literal sites and keeps the leak gate trivial.

## Tests

- `review_aggregates` unit tests: **11 passed, 0 failed** (rank-2/3 pair + union dedup + ctx, rank-1 #556 never-closed + auto_close-clears-unclosed + cycle_events_count, reload single/two-session availability, AC-01 empty→unavailable-never-0, AC-21 directional, measured-zero distinct).
- New `build_cycle_review_record` tests: **2 passed** — AC-17 all-16-columns-from-aggregates (basis-points integer), AC-19/AC-16 no-content/no-knowledge-that-helped column.
- Existing handler tests (#5022 a/b/c, stale-recompute, force-purged-no-clobber, purged-retain): **24 passed** (`context_cycle_review`), auto_close: **7 passed**, activity_fold: **17 passed**, leak gate `test_candidates_structurally_absent_from_memoized_report`: **passed**.
- Full `cargo test -p unimatrix-server --lib`: **4143 passed, 0 failed, 1 ignored**.
- `cargo test -p unimatrix-observe --lib`: **574 passed, 0 failed**.
- `cargo test -p unimatrix-store --lib cycle_review`: **35 passed** (incl. `test_update_path_binds_all_v5_columns`).
- `cargo build -p unimatrix-server --bins`: clean (main.rs config wiring compiles).
- `cargo clippy` on touched files: 0 warnings. `cargo fmt`: applied.

## Ordering invariants verified in source

- INVARIANT A (R-14): `maybe_auto_close` (line 2298) before `populate_rank_1` (2927).
- INVARIANT B (R-03): `land_fold` (2320) before every `purge_cycle_transcripts` reachable on the full-pipeline path (3288).
- INVARIANT C (R-01): `store_cycle_review(&record)` with v5 columns reached ONLY at the full-pipeline return (3032); the three other returns never call `build_cycle_review_record`.

## Deferred to Stage 3c (pytest, NOT touched)

- AC-08 full read-before-purge inversion, AC-09 silent-zero held-route, AC-22 clock/unit boundary — integration/pytest. The wiring makes them satisfiable: fold reads before purge, transcript columns land non-zero on the held route, and the seconds-normalized gate is driven per-session.

## Issues / blockers

None. No git operations performed (Delivery Leader owns git).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern: four-returns #4750, memoization #3800, derived-aggregates #4178, assembly-attach #4866) + (decision topic crt-055: ADR-002 #5037, ADR-007 #5042) — applied the single-writer/four-returns discipline, the assembly-level attach for the fail-loud block, and the read-before-purge ordering. All confirmed against the brief §"Review Pipeline Order".
- Stored: entry #5066 "crt-055 review pipeline: incremental CycleAggregates accumulator threads single-writer discipline across multi-scope handler stages" via `/uni-store-pattern` (topic `unimatrix-server`) — the gotcha that source data for the 16 columns becomes available at DIFFERENT handler scopes (fold at top, rank-2/3 with session_records, rank-1 after cycle_events, persist at the end), so a single mutable accumulator populated in pipeline order is the way to keep one writer + load-bearing orderings intact without hoisting every input.
