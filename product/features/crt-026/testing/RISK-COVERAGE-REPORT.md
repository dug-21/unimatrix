# Risk Coverage Report: crt-026 — WA-2 Session Context Enrichment

GH Issue: #341

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Test must assert numerical floor on score delta | `test_histogram_boost_score_delta_at_p1_equals_weight`, `test_60_percent_concentration_score_delta`, `test_absent_category_phase_histogram_norm_is_zero` | PASS | Full |
| R-02 | Cold-start regression — empty histogram path produces different scores | `test_cold_start_search_produces_identical_scores`, `test_phase_histogram_norm_zero_when_category_histogram_none`, `test_service_search_params_empty_histogram_maps_to_none`, L-CRT026-02 integration | PASS | Full |
| R-03 | Duplicate store increments histogram | `test_duplicate_store_does_not_increment_histogram`, L-CRT026-03 integration | PASS | Full |
| R-04 | Unregistered session causes panic or side effect | `test_record_category_store_unregistered_session_is_noop`, `test_get_category_histogram_unregistered_returns_empty` | PASS | Full |
| R-05 | UDS search path omits histogram pre-resolution | `test_uds_search_path_histogram_pre_resolution`, `test_uds_search_path_empty_session_produces_none_histogram` | PASS | Full |
| R-06 | `FusionWeights::effective()` NLI-absent denominator includes `w_phase_histogram` | `test_fusion_weights_effective_nli_absent_excludes_phase_from_denominator`, `test_fusion_weights_effective_nli_absent_renormalizes_five_weights`, `test_fusion_weights_effective_nli_absent_sum_is_one` | PASS | Full |
| R-07 | `phase_explicit_norm` hardcoded 0.0 removed as dead code | `test_phase_explicit_norm_placeholder_fields_present`, `test_inference_config_default_phase_weights` | PASS | Full |
| R-08 | Status penalty applied before histogram boost | `test_status_penalty_applied_after_histogram_boost` | PASS | Full |
| R-09 | Division by zero in `p(category)` when total is zero | `test_phase_histogram_norm_zero_when_total_is_zero` | PASS | Full |
| R-10 | Histogram summary emitted when histogram is empty | `test_compact_payload_histogram_block_present_and_absent`, `test_compact_payload_histogram_only_categories_empty` | PASS | Full |
| R-11 | `w_phase_histogram` or `w_phase_explicit` range validation missing | `test_config_validation_rejects_out_of_range_phase_weights`, `test_inference_config_six_weight_sum_unchanged_by_phase_fields` | PASS | Full |
| R-12 | `ServiceSearchParams` construction sites not updated | Compilation gate (cargo build --release: 0 errors); code review of both construction sites in `tools.rs` (line 303) and `uds/listener.rs` (line 980) | PASS | Full |
| R-13 | Pre-resolution placed after an `await` point | Code review: `tools.rs` lines 324–329 precede first `.await` at line 336; `uds/listener.rs` lines 973–977 precede first `await` at line ~994 | PASS | Full |
| R-14 | WA-2 extension stubs not removed | `grep "WA-2 extension" services/search.rs` → 0 matches | PASS | Full |

---

## Test Results

### Unit Tests

All unit tests run via `cargo test --workspace --lib`.

| Crate | Passed | Failed | Ignored |
|-------|--------|--------|---------|
| unimatrix-store | 47 | 0 | 0 |
| unimatrix-embed | 16 | 0 | 0 |
| unimatrix-core | 101 | 0 | 27 |
| unimatrix-vector | 291 | 0 | 0 |
| unimatrix-server (partial) | 73 | 0 | 0 |
| unimatrix-server | 379 | 0 | 0 |
| unimatrix-observe | 1861 | 0 | 0 |
| unimatrix-server (config) | 144 | 0 | 0 |
| unimatrix-server (infra) | 106 | 0 | 0 |
| **Total** | **3018** | **0** | **27** |

The 27 ignored tests are pre-existing (unrelated to crt-026; from `unimatrix-core`).

### crt-026 Specific Unit Tests (subset of above)

| Component | Tests | All Pass |
|-----------|-------|----------|
| `infra/session.rs` — SessionState + SessionRegistry | 9 new tests | Yes |
| `mcp/tools.rs` — histogram recording + duplicate guard | 8 new tests | Yes |
| `services/search.rs` — FusedScoreInputs / FusionWeights / compute_fused_score | 15 new tests | Yes |
| `infra/config.rs` — InferenceConfig new weight fields | 6 new tests | Yes |
| `uds/listener.rs` — UDS pre-resolution + compact payload | 6 new tests | Yes |
| **Total new crt-026 unit tests** | **~44** | **Yes** |

### Gate-Blocking Tests (7 Required)

| # | Test Name | Location | Result |
|---|-----------|----------|--------|
| 1 | `test_histogram_boost_score_delta_at_p1_equals_weight` | `services/search.rs` | PASS |
| 2 | `test_duplicate_store_does_not_increment_histogram` | `mcp/tools.rs` | PASS |
| 3 | `test_cold_start_search_produces_identical_scores` | `services/search.rs` | PASS |
| 4 | `test_record_category_store_unregistered_session_is_noop` | `infra/session.rs` | PASS |
| 5 | `test_compact_payload_histogram_block_present_and_absent` | `uds/listener.rs` | PASS |
| 6 | `test_absent_category_phase_histogram_norm_is_zero` | `services/search.rs` | PASS |
| 7 | `test_fusion_weights_effective_nli_absent_excludes_phase_from_denominator` | `services/search.rs` | PASS |

**All 7 gate-blocking tests PASS.**

### Integration Tests (infra-001 harness)

Binary built via `cargo build --release` (0 errors, 9 warnings — pre-existing, unrelated to crt-026).

#### Smoke Gate (Mandatory)

```
pytest -m smoke: 20 passed in 174.93s
```

Smoke gate: PASS.

#### Suite Results

| Suite | Tests | Passed | Failed | xFailed | Time |
|-------|-------|--------|--------|---------|------|
| `protocol` | 13 | 13 | 0 | 0 | 100.92s |
| `tools` | 83 | 82 | 0 | 1 | 687.73s |
| `lifecycle` | 33 | 32 | 0 | 1 | 261.91s |
| `edge_cases` | 25 | 24 | 0 | 1 | 206.86s |
| **Total** | **154** | **151** | **0** | **3** | - |

The lifecycle suite count is 33 (30 pre-existing + 3 new crt-026 tests). New crt-026 tests all pass.

#### xFailed Tests (Pre-Existing, Not crt-026)

All xfail markers were present before crt-026 and reference tracked GH issues:

| Test | Suite | GH Issue | Reason |
|------|-------|----------|--------|
| `test_retrospective_baseline_present` | `tools` | GH#305 | baseline_comparison null with synthetic features |
| `test_confidence_tick_evolution` | `lifecycle` | (tick control env var) | Cannot drive tick in harness without env override |
| `test_concurrent_store_rate_limit` | `edge_cases` | GH#111 | Rate limit blocks rapid sequential stores |

None of these failures are caused by crt-026. No new xfail markers were added.

#### New Integration Tests Added (crt-026)

Three new tests added to `suites/test_lifecycle.py` per the integration harness plan in `test-plan/OVERVIEW.md`:

| Test | Fixture | Validates | Result |
|------|---------|-----------|--------|
| `test_session_histogram_boosts_category_match` | `server` | Store→histogram→search pipeline (AC-06, R-03) | PASS |
| `test_cold_start_session_search_no_regression` | `populated_server` | Cold-start parity via MCP interface (AC-08, R-02) | PASS |
| `test_duplicate_store_histogram_no_inflation` | `server` | Duplicate store guard through MCP (AC-02, R-03) | PASS |

---

## Code Review Assertions

| Check | Method | Result |
|-------|--------|--------|
| AC-04: `session_id: Option<String>` in `ServiceSearchParams` | `grep -n "session_id: Option<String>" services/search.rs` → line 256 | PASS |
| AC-14: No `WA-2 extension` stub comments in `search.rs` | `grep "WA-2 extension" services/search.rs` → 0 matches | PASS |
| R-12: `ServiceSearchParams` construction sites in `tools.rs` | Lines 303–330: both `session_id` and `category_histogram` explicitly set | PASS |
| R-12: `ServiceSearchParams` construction in `uds/listener.rs` | Lines 980–992: both fields explicitly set | PASS |
| R-13: `get_category_histogram` before first `await` in MCP handler | `tools.rs` lines 324–329 precede `.await` at line 336 | PASS |
| R-13: `get_category_histogram` before first `await` in UDS handler | `listener.rs` lines 973–977 precede first `await` in `handle_context_search` | PASS |
| R-03 guard ordering: `record_category_store` after `duplicate_of.is_some()` check | `tools.rs` duplicate check at line 569, record at line 583 | PASS |
| R-07 ADR-003 comment: `phase_explicit_norm` placeholder comment present | `search.rs` line 220: comment citing ADR-003 at call site | PASS |
| R-05 sanitization ordering: `sanitize_session_id` before `get_category_histogram` in UDS | `listener.rs` sanitize at lines 796–803 dispatch block; `handle_context_search` called after | PASS |

---

## Gaps

None. All 14 risks from RISK-TEST-STRATEGY.md have test coverage:
- R-01 through R-14: all covered by unit tests and/or integration tests as documented above.
- R-12 and R-13 are verified by code review (not automatable as unit tests) — documented explicitly.
- R-14 is verified by grep assertion — zero matches confirmed.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_register_session_category_counts_empty`: `category_counts.is_empty() == true` |
| AC-02 | PASS | `test_duplicate_store_does_not_increment_histogram` (gate): count = 1 after two stores; `test_duplicate_store_histogram_no_inflation` (integration) |
| AC-03 | PASS | `test_record_category_store_unregistered_session_is_noop` (gate): no panic, histogram unchanged; `test_get_category_histogram_unregistered_returns_empty` |
| AC-04 | PASS | Code review: `session_id: Option<String>` at `services/search.rs` line 256 |
| AC-05 | PASS | `test_context_search_handler_populates_service_search_params`: session_id and category_histogram both populated; `test_service_search_params_with_session_data` |
| AC-06 | PASS | Transitively via AC-12; L-CRT026-01 integration test confirms pipeline end-to-end |
| AC-07 | N/A | DROPPED — `w_phase_explicit=0.0` placeholder; see SPECIFICATION.md §AC-07 and ADR-003 |
| AC-08 | PASS | `test_cold_start_search_produces_identical_scores` (gate): bit-for-bit identical with zero histogram; L-CRT026-02 integration confirms parity via MCP |
| AC-09 | PASS | `test_phase_explicit_norm_placeholder_fields_present`: both fields exist and have correct defaults; `test_inference_config_default_phase_weights`: `w_phase_explicit=0.0, w_phase_histogram=0.02` |
| AC-10 | PASS | `test_status_penalty_applied_after_histogram_boost`: `final_score == (fused_with_boost) * penalty`; correct ordering confirmed |
| AC-11 | PASS | `test_compact_payload_histogram_block_present_and_absent` (gate): non-empty → block present; empty → block absent |
| AC-12 | PASS | `test_histogram_boost_score_delta_at_p1_equals_weight` (gate): `delta >= 0.02` and `(delta - 0.02).abs() < 1e-10` |
| AC-13 | PASS | `test_absent_category_phase_histogram_norm_is_zero` (gate): absent category → `phase_histogram_norm = 0.0` exactly |
| AC-14 | PASS | Code review: `grep "WA-2 extension" search.rs` → 0 matches |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: procedure) for "gate verification testing procedures integration test triage" — found `#553` (worktree isolation validation), `#487` (workspace tests without hanging), `#1259` (workflow-only scope delivery). No crt-026-specific testing procedures found; proceeded per system prompt instructions.
- Stored: nothing novel to store — the testing patterns used here (gate-blocking test verification, xfail triage with GH issue references, call_tool for session_id injection in integration tests) are instantiations of documented patterns. The `call_tool` direct invocation pattern for parameters not yet exposed in the typed client wrapper may be worth storing after confirming it recurs in future features.
