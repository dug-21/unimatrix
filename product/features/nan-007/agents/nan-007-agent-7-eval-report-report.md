# Agent Report: nan-007-agent-7-eval-report

**Feature**: nan-007 — Evaluation Harness
**Component**: eval/report (D4: Markdown Report Generation)
**Agent ID**: nan-007-agent-7-eval-report

## Summary

Implemented the `eval/report` module as a module directory (four files, each under 500 lines) providing offline Markdown report generation from per-scenario JSON result files. The module is sync-only — no tokio, no sqlx — and always returns `Ok(())` per C-07.

## Files Created / Modified

- `crates/unimatrix-server/src/eval/report/mod.rs` (267 lines) — public API, JSON deserialization types, internal aggregate types, `run_report`, `load_scenario_query_map`
- `crates/unimatrix-server/src/eval/report/aggregate.rs` (286 lines) — `compute_aggregate_stats`, `find_regressions`, `compute_latency_buckets`, `compute_entry_rank_changes`
- `crates/unimatrix-server/src/eval/report/render.rs` (295 lines) — `render_report`, `find_notable_ranking_changes`, `render_entry_analysis`, `chrono_now`
- `crates/unimatrix-server/src/eval/report/tests.rs` (490 lines) — 16 unit tests

`eval/mod.rs` already had `pub mod report;` committed by another wave-3 agent.

## Test Results

16 unit tests — all pass.

Tests cover:
- All five section headers present in output (AC-08)
- MRR-only regression detection (AC-09 OR semantics)
- P@K-only regression detection (AC-09 OR semantics)
- Both-regression detection
- Explicit "No regressions detected." indicator when list is empty (AC-09, FR-28)
- Equal-boundary metrics: not a regression (strict less-than)
- `run_report` always returns `Ok(())` regardless of regression count (C-07)
- Empty results directory handled gracefully
- Malformed JSON files skipped with WARN, valid files continue
- Summary table has per-profile rows
- Latency distribution section present
- Entry-level analysis promotion/demotion
- Aggregate stats: baseline has zero deltas
- Regressions sorted worst-MRR-delta first
- Latency bucket correct placement
- Single-profile scenario produces no regressions

## Key Design Decisions

**WARN-C (stable baseline selection)**: `HashMap` iteration order is non-deterministic. All profile-comparison functions collect keys into a `Vec`, sort alphabetically, then force "baseline" (case-insensitive match) to position 0. Applied identically in `compute_aggregate_stats`, `find_regressions`, and `find_notable_ranking_changes`.

**WARN-D (consistent headers)**: Numbered section style chosen — `## 1. Summary` through `## 5. Zero-Regression Check` — applied identically in `render_report` production code and all test string assertions.

**500-line rule**: Original single-file design would have been ~1385 lines. Converted to module directory pattern (`eval/report/` with `mod.rs` as root) before any file exceeded the limit.

**Serde dual derive**: All five JSON deserialization types (`ScoredEntry`, `RankChange`, `ComparisonMetrics`, `ProfileResult`, `ScenarioResult`) derive both `Deserialize` and `Serialize` so test helpers can write fixture files via `serde_json::to_string()`.

## Issues Encountered

None blocking. Pre-existing doctest failure in `infra/config.rs` confirmed pre-existing via `git stash && cargo test` before any changes.

## Commit

`886c566` — `impl(eval/report): markdown report generation with five sections and OR-semantics regression check (#321)`

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` eval/report patterns — no prior entries matched the specific findings
- Stored: entry #2610 "HashMap profile iteration order is non-deterministic — always sort keys before selecting baseline" via `/uni-store-pattern`
- Stored: entry #2611 "Derive both Serialize and Deserialize on JSON result types when tests write fixture files" via `/uni-store-pattern`
