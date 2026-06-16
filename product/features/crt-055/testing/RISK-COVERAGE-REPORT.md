# Risk Coverage Report: crt-055

**Feature**: context_cycle_review redesign — durable per-cycle aggregates + dual reload metrics + transcript-fold surfacing
**Stage**: 3c Test Execution | **Author**: crt-055-agent-4-tester | **Date**: 2026-06-16
**Inputs**: RISK-TEST-STRATEGY.md (R-01..R-18), ACCEPTANCE-MAP.md (AC-01..AC-22), test-plan/OVERVIEW.md + per-component plans, USAGE-PROTOCOL.md.

> crt-055 is the CONSUMER half of the crt-054 producer/consumer pair. The dominant failure family is *re-introduction* of fixed classes (#750 empty-clobber, believable-zero) and *silent miscount* at the cross-feature seam. Coverage is weighted toward negative / inversion / regression-guard assertions. Execution combined an extensive existing in-crate suite (6436 Rust tests) with the mandated infra-001 MCP integration tests added this stage.

---

## Test Results

### Unit / in-crate (Rust) tests
Command: `cargo test --workspace --features test-support --jobs 2` (hardened convention; `--jobs 2` per Delivery-Leader guidance to avoid the `cc` linker OOM).
- **Total: 6436**
- **Passed: 6436**
- **Failed: 0**
- **Ignored: 31**

Store/observe integration tests (read accessors, migration, no-clobber store contract) are gated behind the `test-support` cargo feature and are included in the run above.

### Integration tests (infra-001 MCP harness, compiled `target/release/unimatrix`)

| Run | Passed | Failed | xfailed | xpassed | Notes |
|-----|--------|--------|---------|---------|-------|
| Smoke gate (`-m smoke`, MANDATORY) | 23 | 0 | 0 | 0 | gate PASS |
| `test_lifecycle.py` + `test_tools.py` (full) | 267 | 0 | 6 | 2 | all crt-055 additions pass |
| `test_protocol.py` + `test_edge_cases.py` (full) | 36 | 0 | 1 | 0 | suite-selection table coverage |

- **New crt-055 integration tests added this stage: 10** (7 in `test_lifecycle.py`, 3 in `test_tools.py`) — all PASS.
- **No integration tests deleted, commented out, or newly `xfail`-marked.**
- The `auto_close` parameter was added to the `context_cycle_review` harness client helper (`harness/client.py`) — a test-only client extension, not a harness-infrastructure change.

#### xfail / xpass triage (pre-existing, NOT crt-055-caused)
All `xfail`/`xpass` outcomes are pre-existing markers unrelated to crt-055. Per USAGE-PROTOCOL.md they are NOT touched in this feature PR (no scope creep). No new GH Issues were required — crt-055 introduced zero failures.

| Outcome | Test(s) | Existing marker | Disposition |
|---------|---------|-----------------|-------------|
| 6× xfailed (lifecycle/tools), 1× xfailed (edge_cases) | various | pre-existing GH#406, GH#405, GH#276 (tick), and others | Left as-is; not crt-055-related |
| 2× xpassed | `GH#405` confidence-timing, `GH#406` multi-hop traversal | flaky timing-dependent pre-existing markers ("not caused by col-028") | Left as-is — incidental XPASS in this run; NOT a crt-055 concern and not mine to remove in this PR |

---

## Coverage Summary (Risk Register R-01..R-18)

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Second writer / data-presence-gate bypass re-introduces #750 empty-clobber | `cycle_review_index::test_no_clobber_store_layer_contract`; `tools::memo_site_recomputes` matrix; `test_stale_purged_advisory_is_distinct_from_force_string`; `test_context_cycle_review_force_false_no_silent_recompute` | PASS | Full |
| R-02 | Schema-version bump advisory-only — stale rows never flush | `tools::memo_site_recomputes` (stale+present recomputes, stale+purged retains); `test_context_cycle_review_force_true_updates_stale_record` | PASS | Full |
| R-03 | Read-after-purge zeroes the transcript fold | `transcript_hold_activity_tests::test_read_before_purge_ordering` (read non-zero before purge; no entry after — proves read precedes purge); `activity_fold_handler.rs` ordering doc-contract | PASS | Full |
| R-04 | Held-route believable-zero | `transcript_hold_activity_tests::test_held_route_fold_nonempty_at_review`, `test_collector_includes_declared_held_excludes_undeclared`, `test_held_route_fold_continuity_across_drain` | PASS | Full |
| R-05 | Cross-feature attribution silent no-op (#4140) | `compaction_read::*` declared-only count tests; integration `test_cycle_review_compaction_attribution_declared_only` (undeclared row does not inflate count) | PASS | Full |
| R-06 | Believable-zero leaks past presentation guard | `fail_loud_guard` render tests; integration `test_cycle_review_empty_source_renders_unavailable`, `test_cycle_review_behavioral_signals_directional_qualifier` | PASS | Full |
| R-07 | Dual reload collapsed into one number/window | `reload_overlap` two-window tests; `populate_reload`/`populate_compaction` separate columns (`review_aggregates.rs`) | PASS | Full |
| R-08 | compaction_reread gate clock/unit mismatch + boundary selection (CRITICAL) | `compaction_reckoning::test_gate_canonical_floor_strict_after_counts_one`, `_normalizes_read_ts_millis_to_seconds`, `_unnormalized_millis_would_overcount_floor_prevents`, `_multi_compaction_gates_on_earliest_min`; **integration `test_cycle_review_compaction_reread_seconds_boundary` + `_unit_mismatch_guarded`** | PASS | Full |
| R-09 | Integer-width corruption at persist boundary | `activity_fold_handler_tests::test_fold_width_conversion_saturates`, `test_fold_summation_saturates_at_i64_max`; `reload_overlap::test_basis_points_always_in_range` (basis-points clamp 0–10000) | PASS | Full |
| R-10 | Three-path migration drift | migration pragma tests (`cycle_review_index.rs`, `migration.rs`); integration `test_cycle_review_index_v5_columns_present` (fresh + restart agree, all-INTEGER) | PASS | Full |
| R-11 | Structural leak gate breach | `distill_handler::test_candidates_structurally_absent_from_memoized_report`; `cycle_review_index::test_signal_class_counts_json_roundtrip_and_coalesce` (count map, not content) | PASS | Full |
| R-12 | Producer-contract drift (index contract) | `activity_fold_handler_tests::test_fold_lands_class_counts_by_pinned_index` (class[0]=error, class[1]=refusal by fixed index) | PASS | Full |
| R-13 | Token field / forbidden regex class re-introduction | `CycleReviewRecord`/`RetrospectiveReport` carry no token-named field (structural — every metric field `i64`/`String` aggregate); no `reread`/`compaction` signal class in catalog | PASS | Full |
| R-14 | auto_close ordering / duplication | `aggregate_reckoning` rank-1 auto-close-closes-final-phase test; integration `test_cycle_review_auto_close_writes_stop_when_absent`, `_idempotent_when_stop_exists`, `_false_does_not_write_stop` | PASS | Full |
| R-15 | Rank-1 timeline mis-reckoning | `cycle_aggregates::test_rank1_closed_then_reopened_is_rework_not_new_phase`, `test_rank1_unclosed_phase_increments_unclosed_count`, `test_rank1_matching_close_not_unclosed`, `test_rank1_auto_close_stop_closes_final_phase_not_unclosed` | PASS | Full |
| R-16 | Rank-3 knowledge-reuse source error (#320) | `cycle_aggregates::test_rank3_counts_union_of_query_and_injection_log`, `test_rank3_entry_served_via_both_logs_counted_once`, `test_rank3_same_entry_multiple_times_same_log_deduped` | PASS | Full |
| R-17 | Pre-divided ratio re-introduces believable-zero | `fail_loud_guard::render_ratio` num/den pair tests ("0 of 0"→unavailable, "0 of N"→measured) | PASS | Full |
| R-18 | Migration version handshake collision with crt-054 | SM merge-coordination check (distinct sequential `CURRENT_SCHEMA_VERSION`); disjoint-table ALTERs; `test_cycle_review_index_v5_columns_present` confirms v5 columns present | PASS (coordination) | Full |

**All 18 risks have at least Full test coverage. No Critical/High risk is uncovered.**

---

## Acceptance Criteria Verification (AC-01..AC-22)

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_cycle_review_empty_source_renders_unavailable` (integration): Compactions / Compaction re-reads render "unavailable", never a bare "0"; `fail_loud_guard` per-metric unit tests |
| AC-02 | PASS | `test_cycle_review_index_v5_columns_present` (integration): all 16 v5 columns present on fresh DB with correct type; metric columns INTEGER; `signal_class_counts_json` TEXT |
| AC-03 | PASS | `SUMMARY_SCHEMA_VERSION == 5` pinned-version unit test; `test_cycle_review_index_v5_columns_present` confirms columns survive restart (upgrade-path agreement); crt-054 disjoint-ownership |
| AC-04 (#556) | PASS | `cycle_aggregates::test_rank1_unclosed_phase_increments_unclosed_count`, `test_rank1_matching_close_not_unclosed`; integration `test_cycle_review_auto_close_false_does_not_write_stop` (open phase surfaces never-closed) |
| AC-05 | PASS | `cycle_aggregates::test_rank1_closed_then_reopened_is_rework_not_new_phase` + rank-2 rework-ratio tests |
| AC-06 (#320) | PASS | `cycle_aggregates::test_rank3_counts_union_of_query_and_injection_log`, `_entry_served_via_both_logs_counted_once` (union + dedup) |
| AC-07 | PASS | `activity_fold_handler_tests::test_fold_lands_class_counts_by_pinned_index`, `test_fold_sums_across_held_sessions`, `test_signal_class_counts_json_matches_catalog` |
| AC-08 | PASS | `transcript_hold_activity_tests::test_read_before_purge_ordering` (Rust integration: read returns non-zero before purge; no entry after — read provably precedes purge, the inversion proof) |
| AC-09 | PASS | `transcript_hold_activity_tests::test_held_route_fold_nonempty_at_review`, `test_collector_includes_declared_held_excludes_undeclared` (undeclared does not zero valid sessions) |
| AC-10 | PASS | Structural: no token-named field on `CycleReviewRecord`/`RetrospectiveReport` (every metric field `i64`); no `reread`/`compaction` regex class in `[transcript_signals]` |
| AC-11 | PASS | `compaction_read` declared-only tests; integration `test_cycle_review_compaction_attribution_declared_only` (undeclared row not counted), `test_cycle_review_compaction_count_vs_reread` (count == all rows) |
| AC-12 | PASS | `compaction_reckoning::test_gate_*` (floor + strict-`>`, MIN boundary, counted-once); integration `test_cycle_review_compaction_count_vs_reread` |
| AC-22 (clock/unit — INTEGRATION, mandated) | PASS | **`test_cycle_review_compaction_reread_seconds_boundary`**: `compacted_at=T` (secs) × reads at `T·1000` (boundary→floor T→not counted), `T·1000−500` (−500ms→floor T−1→not counted, floor-catching guard), `T·1000+1000` (+1s→floor T+1→counts); asserts `compaction_reread_count == 1`. **`test_cycle_review_compaction_reread_unit_mismatch_guarded`**: −500ms read against seconds boundary stays 0 (seconds-normalization prevents the ~1000× all-or-nothing miscompare). Cross-table (compaction_events × PostToolUse reads) |
| AC-13 | PASS | `reload_overlap` distinct-window tests; `review_aggregates` `populate_reload` (cross-session bps) vs `populate_compaction` (within-cycle) — two columns, never derived from each other |
| AC-14 | PASS | `activity_fold_handler_tests::test_fold_width_conversion_saturates`, `test_fold_summation_saturates_at_i64_max`; `reload_overlap` basis-points range guard (no `is_finite()` — float designed out) |
| AC-15 (#593) | PASS | Integration `test_cycle_review_auto_close_writes_stop_when_absent` (one stop, before pipeline), `_idempotent_when_stop_exists` (no duplicate), `_false_does_not_write_stop` (no stop) — via cycle_events writer, not a second cycle_review writer |
| AC-16 (#206-4) | PASS | Response-time knowledge-that-helped enrichment; no durable column added (structural — column set is fixed at the 16 v5 columns verified in AC-02) |
| AC-17 | PASS | `cycle_review_index::test_no_clobber_store_layer_contract`; `tools::memo_site_recomputes` matrix (full-pipeline writes; memo-hit/purged-retain/force+purged do NOT) |
| AC-18 | PASS | `tools::memo_site_recomputes` (stale+present recomputes via clear-memo-fall-through; stale+purged retains); `test_context_cycle_review_force_true_updates_stale_record` |
| AC-19 | PASS | `distill_handler::test_candidates_structurally_absent_from_memoized_report`; no content field on the record; consumed surfaces metadata-only |
| AC-20 | PASS | `reload_overlap::test_context_reload_pct_basis_points_encode` (0.375→3750; 0.00005→1; 0.99995→10000; out-of-range clamped), `test_basis_points_always_in_range`; `test_cycle_review_index_v5_columns_present` confirms `context_reload_pct` is INTEGER not REAL |
| AC-21 | PASS | Integration `test_cycle_review_behavioral_signals_directional_qualifier` (Errors/Refusals render `~`/"directional" or "unavailable", never a bare exact count; Compactions does NOT carry the qualifier — presentations distinguishable) |

**All 22 acceptance criteria PASS.**

---

## Gaps

None. Every R-XX risk maps to at least one passing test; every AC-XX has passing evidence.

### Test-layer placement notes (not gaps — design-faithful per test-plan/OVERVIEW.md §4.5)
- **AC-08 (read-before-purge inversion) and AC-09 (held-route fold non-zero)** are validated at the **Rust integration layer** (`transcript_hold_activity_tests.rs`), not the MCP harness. Reason: the transcript fold is produced by the crt-054 in-memory `TranscriptBuffer` populated only via the live UDS hook path, which the stdio MCP harness does not drive. The Rust layer is where the in-process buffer AND the read/purge call ordering are directly manipulable — exactly where the load-bearing inversion proof belongs. The harness tests assert the MCP-visible facets (columns exist/persist/render fail-loud). This matches the Stage-3a plan (OVERVIEW.md §4.5).
- **AC-22 (clock/unit gate)** is validated end-to-end through the compiled binary as mandated — the cross-table seconds-normalization is observable through the full pipeline + real SQLite + real timestamp units.
- **New-tests precondition discovered during execution**: `context_cycle_review` returns `ERROR_NO_OBSERVATION_DATA` before the full-pipeline block (and therefore before the auto_close arm) when no observation data attributes to the cycle. The `auto_close` integration tests therefore seed a declared session + observations so the pipeline is reached. This is correct handler behavior (a data-present gate), not a defect — documented here for future test authors.

---

## GH Issues filed
None. crt-055 introduced zero integration-test failures. All `xfail`/`xpass` outcomes are pre-existing, feature-unrelated markers (GH#405, GH#406, GH#276) left untouched per the failure-triage protocol.

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #5057 (believable-zero held-fold testing pattern), #5048/#5031 (compaction boundary + survival-to-review ADRs), #4236 (epoch-migration three-tier boundary test pattern that shapes the AC-22 ÷1000 sub-second boundary design). Applied #4236's boundary-insertion tier directly: the −500ms case is the floor-catching guard a ±1s window would miss.
- Stored: nothing novel to store — the integration-test substrate (SQL-seed of `sessions`/`observations`/`compaction_events` + `context_cycle_review` helper) is already an established harness pattern (`test_lifecycle.py`), and #5057/#4236 already capture the held-fold-believable-zero and epoch-boundary test patterns this stage reused. No 2+-feature test pattern emerged that is not already in Unimatrix.
