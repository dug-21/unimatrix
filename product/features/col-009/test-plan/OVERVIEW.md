# Test Plan Overview: col-009 Closed-Loop Confidence

## Overall Test Strategy

col-009 spans four crates and introduces a new signal pipeline. Testing follows three levels:

| Level | Scope | Tool |
|-------|-------|------|
| Unit | Per-component logic: rework threshold, dedup, cap enforcement, field extraction, serialization | `cargo test` |
| Integration (Rust) | Cross-crate flows: schema migration with real redb, signal insert/drain, confidence increment | `cargo test --workspace` |
| Integration (Python/MCP) | MCP-visible behavior: context_retrospective returns entries_analysis | `pytest suites/` |

## Risk-to-Test Mapping

| Risk ID | Priority | Component | Test Location |
|---------|----------|-----------|---------------|
| R-01 | High | session-signals | Unit: drain_and_signal_session atomicity; concurrent call test |
| R-02 | High | signal-store | Integration: migrate v3 → v4 with 10 pre-populated entries |
| R-03 | High | session-signals | Unit: has_crossed_rework_threshold boundary cases |
| R-04 | High | session-signals | Unit: ExplicitUnhelpful excluded from helpful set |
| R-09 | High | hook-posttooluse | Unit: JSON field extraction for all tool types |
| R-05 | Med | signal-store | Unit: drain_signals idempotent on empty queue |
| R-06 | Med | signal-store | Unit: cap drops oldest (lowest signal_id) |
| R-07 | Med | signal-dispatch | Unit: PendingEntriesAnalysis cap at 1000 entries |
| R-08 | Med | session-signals | Unit: stale sweep boundary at 4h threshold |
| R-10 | Med | signal-store | Unit: SignalRecord bincode roundtrip |
| R-11 | Low | signal-dispatch | Integration: missing entry_id handled without crash |
| R-12 | Low | entries-analysis | Unit: entries_analysis absent from JSON when None |
| R-13 | Low | session-signals | Unit: empty injection_history produces no signal |

## Cross-Component Test Dependencies

1. `signal-store` tests must run before signal-dispatch integration tests (need Store methods)
2. `session-signals` tests must run before signal-dispatch tests (need SignalOutput types)
3. `entries-analysis` tests are independent of other components

## Integration Harness Plan

### Applicable Suites

| Suite | Why |
|-------|-----|
| `smoke` | Mandatory gate — ensures server still starts and basic MCP handshake works after schema v4 migration |
| `lifecycle` | Schema change (v3 → v4) affects existing lifecycle flows; schema migration tested |
| `tools` | `context_retrospective` has new entries_analysis parameter — verify response format |
| `confidence` | col-009 calls the existing confidence pipeline (helpful_count increment) — verify crt-002 still works |

### New Integration Tests Needed

The following scenarios are only testable through the MCP interface:

| Test | Suite | Scenario |
|------|-------|----------|
| `test_retrospective_entries_analysis_present` | `test_lifecycle.py` | Store entry, simulate flagged signal accumulation, call context_retrospective, assert entries_analysis in response |
| `test_retrospective_entries_analysis_absent_when_none` | `test_lifecycle.py` | Fresh server with no signals, call context_retrospective, assert "entries_analysis" key absent from JSON |
| `test_schema_v4_migration_preserves_data` | `test_lifecycle.py` | Open v3 db, verify v4 migration ran, existing data intact |

### Suite Selection Summary

```bash
# Mandatory
python -m pytest suites/ -v -m smoke --timeout=60

# Feature-relevant suites
python -m pytest suites/test_lifecycle.py -v --timeout=60
python -m pytest suites/test_tools.py -v --timeout=60
python -m pytest suites/test_confidence.py -v --timeout=60
```

## Acceptance Criteria Coverage

| AC-ID | Test Location | Test Name |
|-------|---------------|-----------|
| AC-01 | signal-store integration | `test_migration_v3_to_v4` |
| AC-02 | signal-dispatch integration | `test_signal_generation_success_session` |
| AC-03 | session-signals unit | `test_drain_and_signal_idempotent` |
| AC-04 | signal-dispatch integration | `test_confidence_consumer_helpful_count` |
| AC-05 | session-signals unit | `test_abandoned_session_no_signals` |
| AC-06 | session-signals + signal-dispatch | `test_rework_signals_flagged_only` |
| AC-07 | signal-dispatch + entries-analysis | `test_entries_analysis_in_retrospective` |
| AC-08 | session-signals unit | `test_rework_threshold_three_cycles` |
| AC-09 | session-signals unit | `test_stale_session_sweep` |
| AC-10 | signal-store unit | `test_signal_queue_cap` |
| AC-11 | `cargo test --workspace` | All tests pass |
| AC-12 | signal-dispatch integration | `test_confidence_consumer_performance` |
| AC-13 | entries-analysis unit | `test_entries_analysis_absent_when_none` |
