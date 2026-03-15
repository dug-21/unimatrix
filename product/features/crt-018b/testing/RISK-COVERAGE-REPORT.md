# Risk Coverage Report: crt-018b — Effectiveness-Driven Retrieval

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Double-lock ordering in ADR-001 snapshot causes deadlock | `test_generation_read_write_no_simultaneous_locks` (effectiveness.rs), `test_snapshot_read_guard_dropped_before_mutex_lock` (search.rs) | PASS | Full |
| R-02 | Utility delta not applied at all four `rerank_score` call sites | `test_effective_outranks_ineffective_at_close_similarity`, `test_effective_outranks_ineffective_at_max_weight`, `test_utility_delta_inside_deprecated_penalty`, `test_utility_delta_inside_superseded_penalty`, `test_utility_delta_absent_entry_zero` (search.rs) | PASS | Full |
| R-03 | Bulk auto-quarantine causes partial quarantine with inconsistent counters | `test_auto_quarantine_fires_at_threshold`, `test_auto_quarantine_fires_at_threshold_1`, `test_tick_write_with_no_quarantine_candidates_is_noop`, `test_auto_quarantine_disabled_when_env_zero` (integration L-E04) | PASS | Full (unit); Partial (integration — tick not drivable externally, see Gaps) |
| R-04 | `consecutive_bad_cycles` incremented by `context_status` | `test_context_status_does_not_advance_consecutive_counters` (integration L-E03) | PASS | Full |
| R-05 | Utility delta placed outside status_penalty multiplication | `test_utility_delta_inside_deprecated_penalty`, `test_utility_delta_inside_superseded_penalty` (search.rs) | PASS | Full |
| R-06 | Generation cache not shared across rmcp clones | `test_cached_snapshot_shared_across_clones` (search.rs), `test_effectiveness_snapshot_generation_match` (effectiveness.rs), `test_briefing_service_clones_share_snapshot` (briefing.rs) | PASS | Full |
| R-07 | Absent entry receives non-zero delta | `test_utility_delta_none_zero`, `test_utility_delta_absent_entry_zero` (search.rs), `test_effectiveness_priority_none` (briefing.rs), `test_effectiveness_state_new_returns_empty` (effectiveness.rs) | PASS | Full |
| R-08 | tick_skipped audit event not emitted on `compute_report()` error | `test_emit_tick_skipped_audit_detail_fields` (background.rs) | PASS | Full |
| R-09 | Briefing sort uses effectiveness as primary key | `test_injection_sort_confidence_is_primary_key`, `test_injection_sort_effectiveness_is_tiebreaker`, `test_injection_sort_three_entries_mixed`, `test_convention_sort_effectiveness_tiebreaker_no_feature` (briefing.rs) | PASS | Full |
| R-10 | SETTLED_BOOST exceeds co-access max (0.03) | `test_utility_constants_values` — asserts `SETTLED_BOOST < 0.03` (search.rs) | PASS | Full |
| R-11 | Auto-quarantine fires for Settled/Unmatched entries | `test_auto_quarantine_does_not_fire_for_settled`, `test_auto_quarantine_does_not_fire_for_unmatched`, `test_auto_quarantine_does_not_fire_for_effective`, `test_consecutive_bad_cycles_three_tick_sequence_no_quarantine` (background.rs) | PASS | Full |
| R-12 | `auto_quarantined_this_cycle` field not populated | `test_tick_write_with_no_quarantine_candidates_is_noop`, `test_emit_auto_quarantine_audit_detail_fields` (background.rs) | PASS | Partial (field populated in unit path; visibility via context_status requires tick to fire — see Gaps) |
| R-13 | Write lock held across auto-quarantine SQL write | `test_write_lock_not_held_after_tick_write_block` (background.rs), `test_snapshot_read_guard_dropped_before_mutex_lock` (search.rs) | PASS | Full |
| R-14 | crt-019 adaptive confidence_weight not exercised | `test_effective_outranks_ineffective_at_max_weight` (confidence_weight=0.25 ceiling), `test_effective_outranks_ineffective_at_close_similarity` (confidence_weight=0.15 floor) (search.rs) | PASS | Full (unit) |

---

## Test Results

### Unit Tests

| Scope | Passed | Failed | Ignored | Notes |
|-------|--------|--------|---------|-------|
| Full workspace | 2472 | 0 | 18 | All pass. 18 ignored are pre-existing in other crates. |
| unimatrix-engine | 230 | 0 | 0 | All effectiveness module tests pass. |
| unimatrix-server | 1295 | 0 | 0 | All server tests pass, incl. all crt-018b tests. |

**Note on flaky test**: `test_compact_search_consistency` in `unimatrix-vector` failed once during an early run (non-deterministic HNSW ordering), passed on retry and in all subsequent runs. This is a pre-existing intermittent failure not caused by this feature. The vector crate was last modified in `nan-004` (commits: `0fd53af`, `88dd79c`, `ff14bcb`). It is tracked as a known pre-existing issue.

### Integration Tests

#### Smoke Gate (mandatory)

| Result | Details |
|--------|---------|
| PASS | 18 passed, 1 xfailed (pre-existing GH#111 volume test rate limit) |

#### Lifecycle Suite (`test_lifecycle.py`)

| Test | Result |
|------|--------|
| All pre-existing lifecycle tests (17) | 16 PASS, 1 xfailed (pre-existing GH#238) |
| `test_effectiveness_search_ordering_after_cold_start` (L-E01) | PASS |
| `test_briefing_effectiveness_tiebreaker` (L-E02) | PASS |
| `test_context_status_does_not_advance_consecutive_counters` (L-E03) | PASS |
| `test_auto_quarantine_disabled_when_env_zero` (L-E04) | PASS |
| `test_auto_quarantine_after_consecutive_bad_ticks` (L-E05) | XFAIL (expected — see Gaps) |

**Lifecycle total**: 20 passed, 2 xfailed

#### Security Suite (`test_security.py`)

| Test | Result |
|------|--------|
| All pre-existing security tests (15) | 15 PASS, 4 xfailed (pre-existing) |
| `test_auto_quarantine_cycles_invalid_large_value_rejected_at_startup` (S-31) | PASS |
| `test_auto_quarantine_cycles_zero_accepted_at_startup` (S-32) | PASS |

**Security total**: 17 passed, 4 xfailed

#### Tools Suite (`test_tools.py`)

| Result | Details |
|--------|---------|
| PASS | 53 passed, 4 xfailed (pre-existing) |

### Integration Test Totals

| | Count |
|-|-------|
| Total integration tests (new + existing) | 196 |
| Passed | 188 |
| XFailed (pre-existing) | 7 |
| XFailed (new — known gap, see Gaps) | 1 |
| Failed | 0 |

---

## Gaps

### G-01: Auto-Quarantine Fires After N Consecutive Ticks (AC-17 item 3, AC-10, R-03)

**Risk**: R-03 (Critical) partial integration coverage.

**Gap**: The background tick fires every 15 minutes in production. The integration harness cannot drive the tick externally through the MCP interface. The full end-to-end flow — `store entry → N bad ticks → auto-quarantine → entry status becomes Quarantined → audit event in context_status` — cannot be exercised at integration test time.

**Mitigation**: Unit tests in `background.rs` cover the trigger logic completely:
- `test_auto_quarantine_fires_at_threshold` — confirms quarantine fires at counter == threshold
- `test_auto_quarantine_fires_at_threshold_1` — confirms threshold=1 works
- `test_auto_quarantine_does_not_fire_below_threshold` — confirms counter < threshold does not fire
- `test_tick_write_updates_categories_from_report` — confirms tick write path populates state
- `test_emit_auto_quarantine_audit_detail_fields` — confirms audit event fields

The existing integration test `test_store_quarantine_restore_search_finds` (L-08) confirms the `quarantine_entry()` store method works correctly end-to-end. Auto-quarantine calls the same store method — the integration gap is the tick trigger, not the quarantine store path.

**Marker**: `test_auto_quarantine_after_consecutive_bad_ticks` is marked `@pytest.mark.xfail` with the gap explanation. No GH Issue is required (the gap is intentional and architectural — the feature scope does not include a test-mode tick trigger). A future enhancement could add `UNIMATRIX_TICK_INTERVAL_SECONDS=0` for test-time tick driving.

### G-02: `auto_quarantined_this_cycle` Visibility in context_status (R-12, AC-13 integration)

**Risk**: R-12 (Low) integration visibility gap.

**Gap**: The `auto_quarantined_this_cycle` field on `EffectivenessReport` is populated in the background tick (unit-tested). Its surfacing in `context_status` output (FR-14) requires the tick to have fired with auto-quarantine, which has the same tick-drivability constraint as G-01.

**Mitigation**: Unit tests `test_emit_auto_quarantine_audit_detail_fields` (background.rs) and `test_tick_write_with_no_quarantine_candidates_is_noop` confirm field behavior. The `context_status` response format for `auto_quarantined_this_cycle` is the same field path used in crt-018 (pre-existing) — no new serialization path was introduced.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_context_status_does_not_advance_consecutive_counters` (integration L-E03): entry remains Active after 10 `context_status` calls |
| AC-02 | PASS | `test_snapshot_read_guard_dropped_before_mutex_lock` (search.rs): snapshot placed at top of search pipeline before embedding/SQL |
| AC-03 | PASS | `test_utility_constants_values` (search.rs): asserts UTILITY_BOOST=0.05, SETTLED_BOOST=0.01, UTILITY_PENALTY=0.05, SETTLED_BOOST < 0.03 |
| AC-04 | PASS | `test_utility_delta_effective/settled/ineffective/noisy/unmatched_zero/none_zero` (search.rs): all 5 category + absent case |
| AC-05 | PASS | `test_effective_outranks_ineffective_at_close_similarity` (search.rs): sim_A=0.75 Effective ranks above sim_B=0.76 Ineffective |
| AC-06 | PASS | `test_utility_delta_none_zero` + `test_effectiveness_state_new_returns_empty` + `test_effectiveness_priority_none`: absent entry = 0.0 delta everywhere |
| AC-07 | PASS | `test_injection_sort_confidence_is_primary_key` + `test_injection_sort_effectiveness_is_tiebreaker` (briefing.rs): confidence is primary key, effectiveness is tiebreaker |
| AC-08 | PASS | `test_convention_sort_effectiveness_tiebreaker_no_feature` (briefing.rs): convention sort uses effectiveness as tiebreaker at equal confidence |
| AC-09 | PASS | `test_consecutive_bad_cycles_increment_for_ineffective/noisy`, `test_consecutive_bad_cycles_reset_on_recovery/settled/unmatched`, `test_consecutive_bad_cycles_three_tick_sequence_no_quarantine` (background.rs) |
| AC-10 | PASS | `test_auto_quarantine_fires_at_threshold` (background.rs): quarantine fires when counter >= threshold with Ineffective category |
| AC-11 | PASS | `test_auto_quarantine_fires_at_threshold_1` (background.rs): threshold=1 fires on first bad tick |
| AC-12 | PASS | `test_auto_quarantine_disabled_when_threshold_zero` (background.rs) + `test_auto_quarantine_cycles_zero_accepted_at_startup` (integration S-32) + `test_auto_quarantine_disabled_when_env_zero` (integration L-E04) |
| AC-13 | PASS (unit) | `test_emit_auto_quarantine_audit_detail_fields` (background.rs): all required audit event fields verified; integration visibility gap (G-01/G-02) |
| AC-14 | PASS | `test_auto_quarantine_does_not_fire_for_settled/unmatched/effective` (background.rs) |
| AC-15 | PASS | `test_consecutive_bad_cycles_remove_absent_entry` (background.rs): quarantined entry absent from tick report removes its counter |
| AC-16 | PASS | All constant tests in search.rs pass: all five categories + absent entry covered, SETTLED_BOOST < 0.03 |
| AC-17 | PASS (partial) | Item 1: `test_effectiveness_search_ordering_after_cold_start` (L-E01) — cold-start safe; Item 2: `test_briefing_effectiveness_tiebreaker` (L-E02) — briefing non-error; Item 3: XFAIL — tick not drivable (G-01); Item 4: `test_effective_outranks_ineffective_at_max_weight` (search.rs) at confidence_weight=0.25 |
| AC-18 | PASS | `cargo test --workspace`: 2472 unit tests pass, 0 failures. All pre-existing tests unmodified. |

---

## Integration Test xfail References

| Test | Mark | Reason |
|------|------|--------|
| `test_multi_agent_interaction` | `@pytest.mark.xfail` | Pre-existing: GH#238 — permissive auto-enroll grants Write to unknown agents |
| `test_store_1000_entries` (volume) | `@pytest.mark.xfail` | Pre-existing: GH#111 — rate limit blocks volume test |
| `test_auto_quarantine_after_consecutive_bad_ticks` | `@pytest.mark.xfail` | Known gap: background tick interval (15 min) not drivable externally through MCP |
| 4 pre-existing xfail in tools+security | (pre-existing) | Filed in prior features |

No new GH Issues were filed. The `test_auto_quarantine_after_consecutive_bad_ticks` gap is architectural (not a bug) and does not require a GH Issue — it is documented in this report and in the test docstring.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for category:procedure testing procedures gate verification integration test triage — MCP server unavailable at search time; proceeded without results (non-blocking per agent instructions).
- Stored: nothing novel to store. The integration test pattern for env-var startup validation (spawning a subprocess with custom env, asserting exit code and stderr content) is a general technique that applies to any server-level configuration test. It is not specific to crt-018b and has no unique aspect warranting a pattern entry. The xfail approach for background-tick-interval gaps is already documented in USAGE-PROTOCOL.md.
