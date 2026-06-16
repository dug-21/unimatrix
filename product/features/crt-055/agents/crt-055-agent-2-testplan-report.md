# Agent Report — crt-055-agent-2-testplan (Stage 3a, Test Plan Design)

**Agent**: crt-055-agent-2-testplan | **Phase**: Test Plan Design (Stage 3a) | **Date**: 2026-06-16

## Deliverables

| File | Component |
|------|-----------|
| `product/features/crt-055/test-plan/OVERVIEW.md` | Test strategy, risk→test map, integration harness plan |
| `test-plan/cycle_review_index_schema.md` | v5 columns + migration + version pin (R-10/R-18) |
| `test-plan/store_cycle_review.md` | Single writer, four returns, width/basis-points, leak gate (R-01/R-09/R-11) |
| `test-plan/aggregate_reckoning.md` | Rank 1/2/3, #556 unclosed, #320 union (R-15/R-16/R-17) |
| `test-plan/reload_overlap_engine.md` | Dual reload not collapsed, basis-points (R-07) |
| `test-plan/compaction_reckoning.md` | Clock/unit gate AC-22, attribution #4140 (R-08/R-05) |
| `test-plan/activity_fold_landing.md` | Read-before-purge + inversion, held-route guard (R-03/R-04) |
| `test-plan/fail_loud_guard.md` | "unavailable" not "0", coarse-signal honesty (R-06) |
| `test-plan/auto_close.md` | #593 three paths, ordering (R-14) |
| `test-plan/review_pipeline.md` | Pipeline ordering + #5022 four returns (R-01/R-02) |

9 component test plans (1:1 with the brief Component Map) + OVERVIEW.

## Risk Coverage Mapping
All 18 risks (R-01..R-18) traced to ≥1 named test; all 22 ACs (AC-01..AC-22) covered. 7 Critical risks (R-01..R-06, R-08) carry the heaviest negative/inversion/regression-guard coverage. Mandated integration tests placed:
- **AC-22** (clock/unit seconds-normalization) → compaction_reckoning.md: `test_cycle_review_compaction_reread_seconds_boundary` (+500ms/+1s count, −500ms does not — sub-second ÷1000-floor boundary) + `_unit_mismatch_guarded`.
- **AC-08** (read-before-purge inversion zeroes columns) → activity_fold_landing.md: `test_inverted_order_zeroes_columns`.
- **AC-09** (held-route silent-zero guard) → activity_fold_landing.md: `test_cycle_review_held_route_fold_nonzero`.
- **AC-17** (#5022 three no-clobber assertions) → review_pipeline.md + store_cycle_review.md.

## Integration Suite Plan
- **Mandatory gate**: `pytest -m smoke`. **Targeted**: `test_lifecycle.py`, `test_tools.py`, `test_edge_cases.py`.
- **Extend, not re-scaffold** (test infra cumulative): harness already has `server.context_cycle_review()`, `_seed_cycle_events_lifecycle`, `_seed_observation_sql_lifecycle`, `_compaction_events_columns`, and the `sqlite3` direct-seed pattern. Precedents: `test_phase_tag_store_cycle_review_flow`, `test_cycle_review_knowledge_reuse_cross_feature_split`.
- **~15 new harness tests** specified in OVERVIEW §4.3 for Stage 3c (all use the `server` fixture). NOT-new rationale in §4.4 (basis-points arithmetic, width conversion, single-writer structural → unit suffices). No harness infrastructure change needed → no GH Issue required.

## Open Questions
1. **Rank-3 injection_log table/column names** (Open Q1) — `aggregate_reckoning.md` flags `test_rank3_wrong_table_name_yields_silent_zero_guard`; pseudocode/spec must confirm the actual `injection_log` schema (a wrong name = silent zero). Carries into Stage 3b.
2. **Knowledge-reuse dedup key** (#320) — union dedup key (entry id?) for an entry served multiple times in the same log; confirm at pseudocode time.
3. **`signal_class_counts_json` rendering location** — AC-21 directional qualifier is a presentation assertion; confirm whether it lives in the report formatter or the column read (affects whether AC-21 tests are unit-on-formatter or harness-on-rendered-output). Defaulted to both layers in fail_loud_guard.md.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search`(category=decision, topic=crt-055) + `context_get`(4236, 5022) — surfaced ADR-001/002/009 (#5051/#5037/#5044), #5022 (the #750 empty-clobber three-assertion lesson + single-writer-past-two-guards detail, shaped review_pipeline.md/store_cycle_review.md AC-17 tests), and #4236 (epoch-migration three-tier boundary test pattern — directly shaped the AC-22 ÷1000-floor sub-second boundary design in compaction_reckoning.md).
- Stored: nothing novel to store at plan time — the harness SQL-seed + `context_cycle_review` helper pattern is already established (`test_lifecycle.py`), and #4236 already captures the epoch-boundary test pattern AC-22 reuses. Any novel test-infra technique discovered during Stage 3c execution will be promoted then / at retro.
