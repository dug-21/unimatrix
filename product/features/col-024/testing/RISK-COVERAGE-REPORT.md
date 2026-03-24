# Risk Coverage Report: col-024

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Raw `* 1000` literal in window boundary construction | `load_cycle_observations_single_window` (positive inclusion + boundary exclusion via observations before window), AC-13 grep gate | PASS | Full |
| R-02 | `enrich_topic_signal` not applied at one of the four write sites | `test_enrich_fallback_from_registry`, `test_enrich_returns_extracted_when_some`, `test_enrich_no_registry_entry`, `test_enrich_registry_no_feature`, `test_enrich_explicit_signal_unchanged` (unit); per-site integration tests not implemented (see Gaps) | PASS (unit) | Partial |
| R-03 | Empty primary path treated as definitive — legacy fallback not activated | `context_cycle_review_fallback_to_legacy_when_primary_empty`, `context_cycle_review_primary_path_used_when_non_empty` | PASS | Full |
| R-04 | `enrich_topic_signal` overrides explicit extracted signal | `test_enrich_explicit_signal_unchanged` (returns "bugfix-342", not "col-024"; debug log fires with both values) | PASS | Full |
| R-05 | Three-step algorithm runs across multiple `block_sync` calls | `load_cycle_observations_multiple_windows` (runs inside `#[tokio::test(flavor="multi_thread")]`, no panic) | PASS | Full |
| R-06 | Open-ended window includes observations from subsequent cycle reuse | `load_cycle_observations_open_ended_window` | PASS | Full |
| R-07 | `load_cycle_observations` returns `Err` instead of `Ok(vec![])` on no-cycle-events | `load_cycle_observations_no_cycle_events` | PASS | Full |
| R-08 | Fallback log missing or at wrong tracing level | `context_cycle_review_no_cycle_events_debug_log_emitted` (tracing_test::traced_test; asserts `logs_contain("primary path empty")` and `logs_contain` cycle_id value) | PASS | Full |
| R-09 | Step 3 Rust window-filter absent — gap-period observations included | `load_cycle_observations_multiple_windows` (gap observation at T+5400s excluded; exact count == 2) | PASS | Full |
| R-10 | `parse_observation_rows` bypassed — security bounds not applied | Code inspection: Step 3 calls `parse_observation_rows(rows, &registry)` (line 465); exercised by `load_cycle_observations_single_window` | PASS | Full |
| R-11 | Session ID deduplication skipped across windows | `load_cycle_observations_multiple_windows` (HashSet dedup applied; exact count == 2, not 4) | PASS | Full |
| R-12 | Enrichment applied outside four scoped write paths | Code review: `enrich_topic_signal` is `fn` (not `pub`), 4 production call sites, all in `uds/listener.rs` | PASS | Full |

---

## Test Results

### Unit Tests

- **Total workspace**: 3,400 passed; 0 failed; 27 ignored
- **unimatrix-observe**: 6 passed; 0 failed
- **unimatrix-server (col-024-relevant)**:

| Test Name | Module | AC(s) | Result |
|-----------|--------|-------|--------|
| `load_cycle_observations_single_window` | `services::observation::tests` | AC-01, AC-11, R-01 | PASS |
| `load_cycle_observations_multiple_windows` | `services::observation::tests` | AC-02, R-05, R-09, R-11 | PASS |
| `load_cycle_observations_no_cycle_events` | `services::observation::tests` | AC-03, R-07 | PASS |
| `load_cycle_observations_no_cycle_events_count_check` | `services::observation::tests` | AC-15 (case A) | PASS |
| `load_cycle_observations_rows_exist_no_signal_match` | `services::observation::tests` | AC-15 (case B) | PASS |
| `load_cycle_observations_open_ended_window` | `services::observation::tests` | R-06 | PASS |
| `load_cycle_observations_phase_end_events_ignored` | `services::observation::tests` | E-02 | PASS |
| `load_cycle_observations_saturating_mul_overflow_guard` | `services::observation::tests` | E-05 | PASS |
| `context_cycle_review_primary_path_used_when_non_empty` | `mcp::tools::tests` | AC-04 (non-empty branch) | PASS |
| `context_cycle_review_fallback_to_legacy_when_primary_empty` | `mcp::tools::tests` | AC-04, AC-09, AC-12 | PASS |
| `context_cycle_review_no_cycle_events_debug_log_emitted` | `mcp::tools::tests` | AC-14, R-08 | PASS |
| `context_cycle_review_propagates_error_not_fallback` | `mcp::tools::tests` | FM-01 | PASS |
| `test_enrich_returns_extracted_when_some` | `uds::listener::tests` | AC-08 (no-mismatch) | PASS |
| `test_enrich_fallback_from_registry` | `uds::listener::tests` | AC-05/06/07 (unit) | PASS |
| `test_enrich_no_registry_entry` | `uds::listener::tests` | I-03 (FR-13) | PASS |
| `test_enrich_explicit_signal_unchanged` | `uds::listener::tests` | AC-08 (mismatch debug log, T-ENR-02+T-ENR-03) | PASS |
| `test_enrich_registry_no_feature` | `uds::listener::tests` | FR-13 | PASS |

### Integration Tests (infra-001)

- **Smoke suite** (mandatory gate): 20 passed; 0 failed
- **Tools suite**: 86 passed; 1 xfailed (pre-existing); 0 failed
- **Lifecycle suite**: 34 passed; 2 xfailed (pre-existing); 0 failed

All integration test failures are pre-existing xfailed tests unrelated to col-024. No new failures introduced by this feature.

---

## Code Review Gates

### AC-13: No raw `* 1000` in `load_cycle_observations` implementation block

```
grep -n '\* 1000' crates/unimatrix-server/src/services/observation.rs
```

Result: zero matches within lines 308–482 (the implementation block). All window boundary conversions use `cycle_ts_to_obs_millis()`. The four `* 1000` occurrences found (lines 854, 1522, 1570, 1730) are all in test code — constant definitions `const T_MS: i64 = T * 1000` for test fixture setup, which are not query-construction code.

**AC-13: PASS**

### NFR-01: Single `block_sync` entry in `load_cycle_observations`

Code inspection of lines 308–482: exactly one `block_sync(async move { ... })` call. The per-window Step 2 loop (lines 394–412) uses `.await` inside the single async block — no nested `block_sync`. The `load_cycle_observations_multiple_windows` test running inside `#[tokio::test(flavor="multi_thread")]` passes without panic, confirming the single-entry invariant.

**NFR-01: PASS**

### R-10 / NFR-05: `parse_observation_rows` called on Step 3 results

Code inspection: line 465 calls `parse_observation_rows(rows, &registry)?` on the Step 3 `fetch_all` result. The 7-column SELECT shape (`session_id, ts_millis, hook, tool, input, response_size, response_snippet`) matches the existing pattern. 64 KB input limit and JSON depth check apply unchanged.

**NFR-05: PASS**

### S-01: `cycle_id` bound as parameter, not interpolated via `format!`

Code inspection: `cycle_id` is bound via `.bind(&cycle_id)` at all query sites (lines 318, 402, 453). The `format!` usages in `load_cycle_observations` (lines 439, 443) format only placeholder indices (`?3`, `?4`, etc.), not `cycle_id` content. No SQL injection risk.

**S-01: PASS**

### R-12: `enrich_topic_signal` scope verification

```
grep -rn "enrich_topic_signal" crates/unimatrix-server/src/
```

Results:
- 1 definition: `listener.rs:124` (private `fn`)
- 4 production call sites: `listener.rs:643`, `listener.rs:738`, `listener.rs:844`, `listener.rs:892`
- 5 test call sites: all within `listener.rs` test module

All within `uds/listener.rs`. Not `pub`. Not called from any test helper or other production file.

**R-12: PASS**

### FM-04: No `.unwrap()` on registry read in `enrich_topic_signal`

Code inspection (lines 124–155): `session_registry.get_state(session_id).and_then(|state| state.feature)` — no `.unwrap()`. PASS.

---

## Gaps

### Missing Tests from Test Plan

Three unit tests planned in `test-plan/load-cycle-observations.md` were not implemented by the Stage 3b developer:

| Planned Test | AC / Risk | Impact |
|-------------|-----------|--------|
| `load_cycle_observations_excludes_outside_window` (T-LCO-07) | R-01 boundary exclusion (1ms before start, 1ms after stop) | Low — boundary exclusion is covered indirectly by `load_cycle_observations_single_window` which inserts an observation at `T_MS - 1_000` (before window) and asserts it is excluded. Dedicated precision boundary test absent. |
| `load_cycle_observations_empty_cycle_id` (T-LCO-10) | E-06 | Low — empty `cycle_id` falls through Step 0 count query returning 0, returning `Ok(vec![])`. No panic risk; SQL parameterized query handles empty string safely. |
| `cycle_ts_to_obs_millis_unit_test` (T-LCO-11) | R-01 (helper correctness) | Low — the helper is exercised indirectly by all `load_cycle_observations_*` tests. Direct assertion on `cycle_ts_to_obs_millis(1_000) == 1_000_000` etc. not present. |

Four per-site enrichment integration tests planned in `test-plan/enrich-topic-signal.md` were not implemented:

| Planned Test | AC | Impact |
|-------------|-----|--------|
| `enrich_record_event_path` (T-ENR-06) | AC-05 | Medium — the function is unit-tested; end-to-end write-then-query path for RecordEvent not exercised. Test plan notes these may require full UDS handler invocation. |
| `enrich_context_search_path` (T-ENR-07) | AC-06 | Medium — same as above for ContextSearch path |
| `enrich_rework_path` (T-ENR-08) | AC-07 | Medium — same for rework candidate path |
| `enrich_record_events_batch_path` (T-ENR-09) | AC-07 | Medium — same for RecordEvents batch path |

**Assessment**: The missing per-site tests (T-ENR-06 through T-ENR-09) represent partial coverage for AC-05, AC-06, and AC-07. The unit tests confirm the `enrich_topic_signal` helper itself is correct. Code review confirms it is called at all four write sites (lines 643, 738, 844, 892). The call-site coverage reduces the risk of per-site gaps, but end-to-end write path coverage for each site remains absent.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `load_cycle_observations_single_window` — in-window observation returned, before-window observation excluded. `records.len() == 1`, `records[0].ts_millis == T_MS + 60_000`. |
| AC-02 | PASS | `load_cycle_observations_multiple_windows` — two disjoint windows, gap observation (T+5400s) excluded, exact count == 2. |
| AC-03 | PASS | `load_cycle_observations_no_cycle_events` — `result.is_ok()` and `result.unwrap() == vec![]`. |
| AC-04 | PASS | `context_cycle_review_primary_path_used_when_non_empty` (mock: `load_feature_observations` NOT called when primary returns non-empty); `context_cycle_review_fallback_to_legacy_when_primary_empty` (mock: `load_feature_observations` called exactly once when primary is empty). |
| AC-05 | PARTIAL | Unit-level: `test_enrich_fallback_from_registry` (extracted=None, registry has feature → returns feature). Per-site integration test for RecordEvent not implemented. |
| AC-06 | PARTIAL | Unit-level: same as AC-05. Per-site integration test for ContextSearch not implemented. |
| AC-07 | PARTIAL | Unit-level: same. Per-site integration tests for rework and RecordEvents batch not implemented. |
| AC-08 | PASS | `test_enrich_explicit_signal_unchanged` with `#[tracing_test::traced_test]`: returns `"bugfix-342"` unchanged; `logs_contain("bugfix-342")` and `logs_contain("col-024")` both assert true. |
| AC-09 | PASS | `context_cycle_review_fallback_to_legacy_when_primary_empty` covers legacy path activation; full infra-001 lifecycle suite passes (34/34, 2 pre-existing xfail). |
| AC-10 | PASS | `cargo test -p unimatrix-observe`: 6 passed, 0 failed. Only one `ObservationSource` implementor (`SqlObservationSource`). Compilation succeeds with new trait method. |
| AC-11 | PASS | `load_cycle_observations_single_window` uses `store.insert_cycle_event(...)` (not raw SQL) for cycle_events fixtures. All `load_cycle_observations_*` tests use `insert_cycle_event`. |
| AC-12 | PASS | Full workspace: 3,400 passed, 0 failed. infra-001 tools suite: 86 passed, 1 pre-existing xfail. No pre-existing `context_cycle_review` tests deleted or modified. |
| AC-13 | PASS | Grep gate: zero `* 1000` occurrences inside the `load_cycle_observations` implementation block (lines 308–482). All window boundaries constructed via `cycle_ts_to_obs_millis()`. |
| AC-14 | PASS | `context_cycle_review_no_cycle_events_debug_log_emitted` (mock + `tracing_test::traced_test`): `logs_contain("primary path empty")` and `logs_contain` the feature_cycle value both assert true. Production code: `tracing::debug!` at line 1227, fields `cycle_id` and `path`. |
| AC-15 | PASS | `load_cycle_observations_no_cycle_events_count_check` (Step 0 count pre-check returns `Ok(vec![])` for zero rows); `load_cycle_observations_rows_exist_no_signal_match` (rows exist, no topic_signal match → `Ok(vec[])`). Both cases return `Ok(vec![])` to caller; fallback log differentiates at the `context_cycle_review` level. |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "gate verification testing procedures cargo test integration harness" (category: procedure) — found #487 (workspace test without hanging), #2957 (wave-based refactor scope testing), #750 (pipeline validation tests). Relevant context: #487 confirms `tail -30` truncation pattern; no new procedures needed.
- Stored: nothing novel to store — the test patterns exercised here (tracing_test::traced_test for debug log assertion, mock ObservationSource for three-path dispatch, tokio::test(flavor="multi_thread") for block_sync validation) are already captured in existing entries (#3040, #3371–#3375). The per-site enrichment integration test gap (T-ENR-06 through T-ENR-09) is noted in the Gaps section and may warrant a follow-up issue.
