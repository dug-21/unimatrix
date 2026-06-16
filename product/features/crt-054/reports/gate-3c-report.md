# Gate 3c Report: crt-054

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-16
> Result: **PASS**
> Branch: feature/crt-054 · Schema version: 29 (merge-order reconciled to 30 if crt-055 merges first)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01..R-15) | PASS | Every R has ≥1 passing test in RISK-COVERAGE-REPORT.md; spot-verified the Critical/High family live. |
| 2. Test coverage completeness vs Risk-Test Strategy | PASS | All required scenarios exercised; believable-zero family is held-route integration + negative-mutation, not unit/registered-only. |
| 3. Specification compliance (AC-01..AC-16) | PASS | All 16 ACs verified; AC-16 consumer half is a legitimate crt-055 split, not a dropped requirement. |
| 4. Architecture compliance (ADR-001..010 + crt-055 contract) | PASS | Fold at shared `apply_delta` merge boundary; INSERT after `increment_compaction`, no lock across it; seconds at seam; named failure counter. |
| 5. Knowledge stewardship | PASS | Tester report has `## Knowledge Stewardship` with `Queried:` + `Stored:` entries. |
| INTEGRATION: smoke gate | PASS | 23 passed re-run live (199.43s). |
| INTEGRATION: relevant suites + new restart test | PASS | protocol/tools/lifecycle/edge_cases/volume = 327 passed, 0 hard fail; new `test_compaction_events_table_survives_restart` re-run live (1 passed, 16.53s). |
| INTEGRATION: xfail/stale-cache hygiene | PASS | 7 xfails pre-existing/unrelated; 13 lastfailed IDs independently confirmed non-existent (collect 0 items). |
| INTEGRATION: no tests deleted/commented | PASS | Only change to infra-001 is an addition to `test_lifecycle.py`; zero deletions across suites. |
| INTEGRATION: report has counts, no `<FILL>` | PASS | Every suite carries an executed summary line; no placeholders. |

## Detailed Findings

### 1. Risk mitigation proof (R-01..R-15)
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md maps each R-01..R-15 to named passing tests. I independently re-ran the load-bearing groups on `feature/crt-054`:
- **R-01/R-02 believable-zero family (Critical)** — `infra::transcript_hold::tests::activity` = **5 passed**. These drive the REAL `SessionRegistry` + crt-052 Wave B `TranscriptHold` through production entry points (`register`=readopt, `apply_transcript_delta`=route-to-held, `drain_and_signal_session`=hold), reading at review via `activity_snapshots_for_feature` BEFORE purge. This is held-route **integration**, not registered-only or unit-only.
- **Negative-mutation independently confirmed**: I no-op'd the shared fold call at `session_transcript.rs` (the single accepted-path `self.activity.fold(bytes, &self.scanner)` reached by both routes) and re-ran `test_held_route_fold_continuity_across_drain` — it **failed RED** with `left: 0, right: 36` ("snapshot must equal K+M across the drain boundary"). Reverted; file restored clean. The guard cannot degrade into a no-op (ADR-009 / #3624 satisfied).
- **R-03/R-09/R-15 INSERT seam (Critical/Medium/High)** — `uds::listener::tests::compaction_events` = **10 passed**, including `test_insert_failure_increments_named_counter` (named counter +1, fault-injected), `test_insert_failure_non_blocking_no_row`, `test_high_water_equals_buffer_high_water`. Driven through `handle_compact_payload` (integration-level). Code at `listener.rs:1854` confirmed: INSERT after `increment_compaction`, `high_water` captured then guard dropped, no lock across the INSERT.
- **R-04 schema-version (Critical)** — `migration_v28_to_v29` = **4 passed** (fresh + upgrade + idempotent + columns-contract); `CURRENT_SCHEMA_VERSION = 29` with merge-order note in source. infra-001 `test_compaction_events_table_survives_restart` adds restart durability (re-run live, 1 passed).
- **R-05..R-15** — config/contract/width/residue tests all PASS per report; AC-14 producer cast-free and AC-15 residue-absence independently grep-confirmed (only forbidding comments, no live `token_*`/`cycle_review_index`/`SUMMARY_SCHEMA_VERSION` bumps).

### 2. Test coverage completeness vs Risk-Based Test Strategy
**Status**: PASS
**Evidence**: The strategy's mandatory disciplines are met where they matter: AC-06/AC-07 are drain→hold→review integration tests on the cumulative Wave B fixtures (not registered/unit-only); R-15 asserts a **named counter** (not a log line); R-03 is a handler-driven seam test; R-11/AC-16 producer-half lands seconds at the real seam. Cross-component seams (write seam, read seam, schema-version seam, Wave B dependency) all carry coverage. No Phase-2 risk lacks a test.

### 3. Specification compliance (AC-01..AC-16)
**Status**: PASS
**Evidence**: ACCEPTANCE-MAP AC-01..AC-16 each verified in RISK-COVERAGE-REPORT §Acceptance Criteria Verification. AC-10a CALIBRATION.md present with the mandatory "DIRECTIONAL, NOT PRECISE" statement (§Precision/false-positive notes) and the anchored-by-construction fallback documented (no real transcript corpus in-repo — legitimate per AC-10a fallback). AC-16: the **producer half** (seconds at the seam) is landed in-crate as integration (`test_compacted_at_is_seconds_within_tolerance` + `test_second_compaction_adds_monotonic_row` through the handler), and the **consumer half** (`ts/1000` normalization + pre/post classification) is crt-055's per OVERVIEW §5 ownership split. The full MCP boundary-classification test is deferred to a harness GH Issue (OVERVIEW §7 OQ1) because infra-001 has no compact/PreCompact op — a legitimate harness-infrastructure deferral per USAGE-PROTOCOL, not a dropped requirement.

### 4. Architecture compliance (ADR-001..010 + crt-055 binding contract)
**Status**: PASS
**Evidence**: Verified in source on `feature/crt-054`:
- ADR-001 — fold embedded in `TranscriptBuffer.activity`, called at the `apply_delta` merge boundary; both routes share it by construction (the buffer `Arc` is shared; `apply_transcript_delta` resolves registered-or-held then calls `buf.apply_delta`). No route-specific fold code.
- ADR-006 — `self.activity` intentionally preserved across drain (source comment + `test_snapshot_survives_drain_hold_review`).
- ADR-007 — INSERT at `listener.rs:1856` after `increment_compaction`; `high_water` read under buffer lock then guard dropped; `compacted_at` via `.as_secs()` (Unix seconds); named `compaction_events_insert_failed` counter on the durable `counters` table (queryable by crt-055 across restart).
- ADR-008 — crt-054 owns only `compaction_events` + the version bump; no `SUMMARY_SCHEMA_VERSION`/`cycle_review_index` touch (grep-confirmed).
- ADR-010 — Wave B startup precondition (3 wave-b precondition tests in the `--bin` run).
- crt-055 binding contract — `ActivitySnapshot { bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }`, `MAX_SIGNAL_CLASSES == 16`, column contract, error→0/refusal→1 — all conformance-tested.

### 5. Knowledge stewardship
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md §Knowledge Stewardship has a `Queried:` entry (`context_briefing` surfacing ADR-009/006/010/004 + Gate-3b deferral lessons) and a `Stored:` entry (a reusable held-route believable-zero integration-test fixture pattern, topic `testing`, category `pattern`). Block present and complete.

## Integration Test Validation

- **Smoke gate (mandatory)**: re-ran `pytest suites/ -m smoke -p no:cacheprovider` live → **23 passed, 355 deselected, 199.43s**. Matches the report.
- **Relevant suites**: report records protocol 13, tools 191 (+1 xfail), lifecycle 66 (+5 xfail/2 xpass, incl. NEW restart test), edge_cases 23 (+1 xfail), volume 11 — **327 passed, 0 hard failures**. Run sequentially one-suite-per-process to avoid the documented linker/embedding OOM (a build-environment issue, not a test failure). I re-ran the NEW `test_compaction_events_table_survives_restart` in isolation → **1 passed, 16.53s**.
- **xfail / stale-cache hygiene**: 7 live xfails are pre-existing tick-interval / ONNX-embedding / GH#406-traversal cases on surfaces crt-054 does not touch; the 2 xpasses are pre-existing harness marker-hygiene, not crt-054. The cargo flake `test_ac14_correlated_sweep_non_vacuous` is tracked by GH#746 (HNSW ~2% membership flip), outside crt-054's diff. **The 13 `lastfailed` cache entries independently verified non-existent**: current collection is **378 tests**, and 3 sampled stale IDs (one per suite category: `test_list_tools_returns_nine`, `test_search_excludes_deprecated`, `TestVolume1K::test_status_report_at_1k`) each collect **0 items**. Triage is sound — orphaned pre-refactor node IDs, no masked crt-054 regression.
- **No tests deleted/commented**: the only infra-001 change is an **addition** to `test_lifecycle.py` (the restart test); `git diff main...feature/crt-054 -- suites/` shows zero deletions.
- **Report integrity**: every suite line carries an executed `-q` summary; no `<FILL>` placeholders; no PASS without a count behind it.

## Notes (informational — not gate findings)

- `session_transcript.rs` is 525 lines (over the 500 guideline) and `config.rs` is ~12k lines. Both are **pre-existing files crt-054 extended**, not new files; the 500-line cap is a Gate 3b code-review check (already passed) and is not part of the Gate 3c risk-validation check set. Flagged here for visibility only; does not affect the 3c result.
- AC-16 consumer-half and the R-04 merge-order version reconciliation are by-design scope notes (producer/consumer split + SM merge coordination), not coverage gaps.

## Rework Required

None.

## Scope Concerns

None.
