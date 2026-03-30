# Risk Coverage Report: crt-033

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Schema cascade miss — one or more of the seven v17→v18 touchpoints omitted | MIG-U-01 `test_current_schema_version_is_18`, MIG-U-02 `test_fresh_db_creates_schema_v18`, MIG-U-03 `test_v17_to_v18_migration_creates_table`, MIG-U-04 `test_v17_to_v18_migration_table_has_five_columns`, MIG-U-05 `test_v17_to_v18_migration_preserves_existing_data`, MIG-U-06 `test_v17_to_v18_migration_idempotent`; grep gates MIG-C-01 through MIG-C-06 | PASS | Full |
| R-02 | Synchronous write on write_pool_server() causes pool starvation under concurrent first-calls | CRS-I-10 `test_concurrent_store_same_cycle_last_writer_wins`; TH-I-10 concurrent first-call test | PASS | Full |
| R-03 | evidence_limit truncation applied at storage time instead of render time | TH-I-07 `test_cycle_review_evidence_limit_applied_at_render_time` | PASS | Full |
| R-04 | force=true + purged signals path falls through to ERROR_NO_OBSERVATION_DATA when a stored record exists | TH-I-05 `test_cycle_review_force_true_purged_signals_stored_record`, TH-I-06 `test_cycle_review_force_true_purged_signals_no_record` | PASS | Full |
| R-05 | Memoization hit path still executes observation load or computation steps | TH-I-02 `test_cycle_review_second_call_returns_stored_record`, TH-I-08 `test_cycle_review_force_true_skips_step_2_5` | PASS | Full |
| R-06 | serde deserialization fails on stored summary_json for records written by an older schema_version | CRS-U-01 `test_cycle_review_record_serde_round_trip`, CRS-U-05 `test_retrospective_report_serde_round_trip`, CRS-U-06 `test_retrospective_report_serde_missing_optional_fields`, TH-U-06 `test_check_stored_review_corrupted_json_does_not_panic` | PASS | Full |
| R-07 | pending_cycle_reviews query misidentifies or excludes valid pending cycles | CRS-I-04 through CRS-I-09 (K-window, DISTINCT, exclusions); SS-I-01 through SS-I-03 | PASS | Full |
| R-08 | Version advisory absent when schema_version differs; or handler silently recomputes | TH-U-03 `test_check_stored_review_matching_version_no_advisory`, TH-U-04 `test_check_stored_review_mismatched_version_produces_advisory`, TH-U-05 `test_check_stored_review_future_version_produces_advisory`, TH-I-03 | PASS | Full |
| R-09 | store_cycle_review called from spawn_blocking (ADR-001 violation) | TH-G-01 static grep — no `spawn_blocking` wrapping memoization functions confirmed | PASS | Full |
| R-10 | INSERT OR REPLACE on concurrent first-call for same cycle corrupts stored record | CRS-I-10 `test_concurrent_store_same_cycle_last_writer_wins` | PASS | Full |
| R-11 | summary_json exceeds 4MB ceiling; store layer panics instead of returning Err | CRS-U-03 `test_store_cycle_review_4mb_ceiling_exceeded`, CRS-U-04 `test_store_cycle_review_4mb_ceiling_boundary` | PASS | Full |
| R-12 | pending_cycle_reviews uses write_pool_server() instead of read_pool() | CRS-G-02 static grep — `pending_cycle_reviews` uses `read_pool()`, only `store_cycle_review` uses `write_pool_server()` confirmed | PASS | Full |
| R-13 | SUMMARY_SCHEMA_VERSION defined in wrong location | CRS-G-01 / AC-17 grep — single definition in `cycle_review_index.rs` confirmed; no numeric literal in `unimatrix-server` confirmed | PASS | Full |

---

## Mandatory Grep Gates

| Gate | Command | Result |
|------|---------|--------|
| No `schema_version == 17` in crates/ | `grep -r 'schema_version.*== 17' crates/` | PASS — zero matches |
| SUMMARY_SCHEMA_VERSION single definition | `grep -r 'SUMMARY_SCHEMA_VERSION' crates/` | PASS — single `pub const` definition in `cycle_review_index.rs`; all other occurrences are imports or uses |
| No inline numeric literal in unimatrix-server | `grep -r 'SUMMARY_SCHEMA_VERSION.*=.*[0-9]' crates/unimatrix-server/` | PASS — zero matches |
| No spawn_blocking wrapping store functions | `grep -n 'spawn_blocking.*store_cycle\|store_cycle.*spawn_blocking'` | PASS — zero matches |
| pool selection correct | `grep -n 'write_pool_server\|read_pool' crates/unimatrix-store/src/cycle_review_index.rs` | PASS — `get_cycle_review` and `pending_cycle_reviews` use `read_pool()`; only `store_cycle_review` uses `write_pool_server()` |
| CURRENT_SCHEMA_VERSION == 18 | `grep -n 'CURRENT_SCHEMA_VERSION' migration.rs` | PASS — `pub const CURRENT_SCHEMA_VERSION: u64 = 18` |
| if current_version < 18 block exists | `grep -n 'current_version < 18' migration.rs` | PASS — line 601 |
| server.rs version assertions reference 18 | `grep -n 'assert_eq!(version, 1' server.rs` | PASS — both assert `18` |
| cycle_review_index in db.rs DDL | `grep -n 'cycle_review_index' db.rs` | PASS — 2 matches |
| cycle_review_index in sqlite_parity.rs | grep in `sqlite_parity_specialized.rs` | PASS — table existence and 5-column schema assertions present |
| Migration test renamed | `test_current_schema_version_is_17` absent; `test_current_schema_version_is_at_least_17` present | PASS |
| PENDING_REVIEWS_K_WINDOW_SECS constant | `grep -n 'PENDING_REVIEWS_K_WINDOW_SECS' services/status.rs` | PASS — named const `= 90 * 24 * 3600; // 7_776_000` |

---

## Test Results

### Unit Tests (cargo test --workspace)

| Crate/Module | Passed | Failed |
|-------------|--------|--------|
| All crates combined | 4032 | 0 |

**Grand total: 4032 passed, 0 failed.**

Breakdown of new crt-033 unit tests (counted within workspace total above):

- `cycle_review_index.rs` unit tests: CRS-U-01 through CRS-U-06 (6 tests)
- `migration_v17_to_v18.rs` integration tests: MIG-U-01 through MIG-U-06 (6 tests)
- `sqlite_parity_specialized.rs`: table-count and column assertions (2 tests added)
- `tools.rs` unit tests: TH-U-01 through TH-U-07 plus TH-I-01 through TH-I-10 (full handler coverage)
- `response/status.rs` unit tests: SR-U-01 through SR-U-08 + SR-I-01
- `services/status.rs` unit tests: SS-U-01, SS-I-01 through SS-I-03

### Integration Tests (infra-001)

| Suite | Tests | Passed | Failed | Xfailed | Notes |
|-------|-------|--------|--------|---------|-------|
| smoke (`-m smoke`) | 20 | 20 | 0 | 0 | Mandatory gate — PASS |
| tools | 96 | 94 | 0 | 2 | Pre-existing xfails GH#405, GH#305 |
| lifecycle | 43 | 40 | 0 | 2 | Pre-existing xfails; 1 XPASS (GH#406 pre-existing bug now fixed) |

**New integration tests added (all pass):**
- `suites/test_tools.py::test_cycle_review_force_param_accepted` — PASS
- `suites/test_tools.py::test_status_pending_cycle_reviews_field_present` — PASS
- `suites/test_lifecycle.py::test_cycle_review_persists_across_restart` — PASS

**Total integration tests executed:** 20 (smoke) + 96 (tools) + 43 (lifecycle) = 159

---

## Xfail Inventory

All xfail markers are pre-existing with GH Issues — none introduced by crt-033:

| Test | Suite | GH Issue | Reason |
|------|-------|----------|--------|
| `test_confidence_deprecated_score_in_range` | tools | GH#405 | Deprecated confidence can exceed active due to background scoring timing |
| `test_retrospective_baseline_present` | tools | GH#305 | `baseline_comparison` null when synthetic features lack delivery counter registration |
| `test_auto_quarantine_after_consecutive_bad_ticks` | lifecycle | (tick-interval env var) | Requires `UNIMATRIX_TICK_INTERVAL_SECONDS` env var not present in harness |
| `test_dead_knowledge_entries_deprecated_by_tick` | lifecycle | (tick timing) | Dead-knowledge deprecation pass runs in background after tick interval |
| `test_search_multihop_injects_terminal_active` | lifecycle | GH#406 | Marked xfail for pre-existing bug; now XPASS — marker can be removed, GH#406 resolved |

**XPASS note:** `test_search_multihop_injects_terminal_active` is XPASS — it was expected to fail due to GH#406 but now passes. The xfail marker can be removed and GH#406 closed. This is not caused by crt-033.

---

## Gaps

None. All 13 risks from the RISK-TEST-STRATEGY.md have test coverage:

- R-01 through R-13: all covered by unit tests, store integration tests, and/or static grep gates.
- AC-16 (serde round-trip) covered by CRS-U-01 and CRS-U-05.
- AC-17 (SUMMARY_SCHEMA_VERSION location) covered by mandatory grep gate.
- R-06 corrupted-JSON fallthrough (ADR-003 defense): covered by TH-U-06.
- R-08 future schema_version advisory: covered by TH-U-05.
- The three required new infra-001 integration tests from OVERVIEW.md are implemented and pass.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_current_schema_version_is_18` asserts `CURRENT_SCHEMA_VERSION == 18`; `test_fresh_db_creates_schema_v18` asserts schema counter = 18 |
| AC-02 | PASS | `test_v17_to_v18_migration_creates_table` in `tests/migration_v17_to_v18.rs` |
| AC-02b | PASS | Cascade grep gate: `grep -r 'schema_version.*== 17' crates/` returns zero matches; all 7 touchpoints verified |
| AC-03 | PASS | TH-I-01: first call writes row with `raw_signals_available=1`, `schema_version=SUMMARY_SCHEMA_VERSION` |
| AC-04 | PASS | TH-I-02: second call returns stored record without recompute; `computed_at` unchanged |
| AC-04b | PASS | TH-U-04 + TH-I-03: schema_version mismatch produces advisory containing "use force=true to recompute" with both version numbers; no observation-load |
| AC-05 | PASS | TH-I-04: `force=true` with live signals overwrites row; `computed_at` > initial value |
| AC-06 | PASS | TH-I-05: `force=true` + purged signals + stored record returns Ok with "Raw signals have been purged" note; `raw_signals_available=false` |
| AC-07 | PASS | TH-I-06: `force=true` + purged signals + no stored record returns `ERROR_NO_OBSERVATION_DATA` |
| AC-08 | PASS | TH-I-07: stored `summary_json` has full evidence (5 items); MCP response with `evidence_limit=2` has 2 items; memoization hit without limit has full 5 |
| AC-09 | PASS | CRS-I-04 + SS-I-01 + `test_status_pending_cycle_reviews_field_present`: un-reviewed K-window cycles appear in list |
| AC-10 | PASS | CRS-I-05 + SS-I-02: all reviewed cycles → empty list |
| AC-11 | PASS | Covered by AC-03: `schema_version = SUMMARY_SCHEMA_VERSION = 1` in written row |
| AC-12 | PASS | TH-U-01 `test_retrospective_params_force_absent_is_none`; TH-U-02 `test_retrospective_params_force_true/false`; `test_cycle_review_force_param_accepted` integration test |
| AC-13 | PASS | Covered by AC-02: `tests/migration_v17_to_v18.rs` confirms table exists after migration |
| AC-14 | PASS | TH-I-02: `computed_at` unchanged on memoization hit confirms no recompute |
| AC-15 | PASS | TH-I-05: `force=true` + stored record + no live signals returns Ok with explanatory note |
| AC-16 | PASS | CRS-U-05 `test_retrospective_report_serde_round_trip` on fully-populated instance; `cargo build --workspace` compile-time gate passes |
| AC-17 | PASS | Mandatory grep: single `pub const SUMMARY_SCHEMA_VERSION: u32 = 1` definition in `cycle_review_index.rs`; no numeric literal in `unimatrix-server` |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries on delivery-process lessons (#3386, #2758), testing patterns (#3253, #238), and schema-cascade patterns (#3539 via RISK-TEST-STRATEGY). Applied to: confirmed R-01 critical priority is warranted; cascade grep gate is mandatory; confirmed spawn_blocking prohibition.
- Stored: nothing novel to store — the integration test assertion pattern using `resp.error` (vs `resp.is_error`) to distinguish JSON-RPC errors from tool-level errors is already implicit in the test harness conventions. The pattern of using direct SQL to seed `cycle_review_index` rows for memoization bypass tests follows established `_seed_observation_sql` conventions. No new cross-feature pattern emerged.
