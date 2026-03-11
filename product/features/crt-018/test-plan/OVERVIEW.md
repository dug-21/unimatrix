# crt-018: Knowledge Effectiveness Analysis — Test Plan Overview

## Test Strategy

Three layers, matching the three-component architecture:

| Layer | Component | Test Type | Location |
|-------|-----------|-----------|----------|
| Unit | effectiveness-engine | Pure function tests | `crates/unimatrix-engine/src/effectiveness.rs` (#[cfg(test)]) |
| Store integration | effectiveness-store | SQL + TestDb | `crates/unimatrix-store/src/read.rs` (#[cfg(test)]) |
| Server integration | status-integration | compute_report + format | `crates/unimatrix-server/src/services/status.rs` (#[cfg(test)]) |
| System integration | infra-001 harness | MCP JSON-RPC end-to-end | `product/test/infra-001/suites/` |

All Rust tests extend existing TestDb and TestEntry helpers. No isolated scaffolding.

## Risk-to-Test Mapping

| Risk ID | Priority | Component Test Plan | Test Scenario IDs |
|---------|----------|--------------------|--------------------|
| R-01 | Critical | effectiveness-engine | E-01 through E-05 |
| R-02 | Critical | effectiveness-store, effectiveness-engine | S-05 through S-08, E-06 |
| R-03 | High | effectiveness-store | S-01 through S-03 |
| R-04 | High | effectiveness-engine | E-07 through E-13 |
| R-05 | High | effectiveness-engine | E-14 through E-16 |
| R-06 | Medium | effectiveness-store | S-09, S-10 |
| R-07 | Medium | effectiveness-store | S-11 (code review) |
| R-08 | Medium | status-integration | I-04 through I-07 |
| R-09 | Medium | effectiveness-engine | E-17 through E-19 |
| R-10 | Low | effectiveness-engine | E-20, E-21 |
| R-11 | Medium | status-integration | I-08, I-09 |
| R-12 | Low | status-integration | I-10 |
| R-13 | Medium | effectiveness-engine | E-22 through E-24 |

## Cross-Component Test Dependencies

1. **Store-to-Engine contract** (Integration Risk): `EffectivenessAggregates` is the boundary. Store tests verify SQL produces correct aggregates; engine tests verify classification from those aggregates. The server integration tests validate the full pipeline end-to-end.
2. **Entry metadata JOIN consistency** (Integration Risk): Server integration tests must cover the case where `compute_effectiveness_aggregates()` returns an entry_id that has no matching `EntryClassificationMeta` (orphaned injection stats). The classifier must skip it.
3. **Phase 8 independence**: Server integration tests verify that Phase 8 effectiveness does not corrupt Phases 1-7 output and that Phase 8 failure degrades gracefully to `effectiveness = None`.

## Integration Harness Plan

### Suites to Run

Per the suite selection table, crt-018 touches server tool logic (context_status) and store/retrieval behavior:

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory minimum gate for any change |
| `tools` | context_status is a tool; new output fields must not break existing assertions |
| `lifecycle` | status_reflects_lifecycle_changes test may interact with new effectiveness field |

### Existing Test Coverage

- `test_status_empty_db` — verifies status on empty DB; should continue passing (effectiveness = None)
- `test_status_with_entries` — verifies status with entries; no injection_log data means effectiveness absent or "no injection data"
- `test_status_all_formats` — verifies summary/markdown/JSON formats; new effectiveness section must not break existing structure
- `test_status_report_at_volume` — volume suite; verifies status at scale

### Gap Analysis

Existing integration tests do **not** cover:
1. context_status with injection_log + sessions data producing effectiveness output
2. Effectiveness section presence/absence in JSON format (skip_serializing_if behavior)
3. Summary format one-liner with category counts

### New Integration Tests to Add

| Test Name | Suite | Fixture | Scenario |
|-----------|-------|---------|----------|
| `test_status_effectiveness_with_injection_data` | `test_tools.py` | `server` | Store entries, create sessions with outcomes, inject entries, call context_status JSON format, verify `effectiveness` object present with correct category counts |
| `test_status_effectiveness_absent_without_injections` | `test_tools.py` | `server` | Store entries but no injection_log data, call context_status JSON format, verify `effectiveness` key absent (skip_serializing_if) |
| `test_status_effectiveness_summary_format` | `test_tools.py` | `server` | Store entries + sessions + injections, call context_status summary format, verify "Effectiveness:" line present with counts |

These tests require creating sessions and injection_log entries through the MCP interface. If the MCP protocol does not expose direct session/injection_log creation (it does not — these are internal), the tests must be structured to: (a) use context_store + context_search to trigger injections naturally, and (b) may need a helper to record session outcomes. If this is not feasible through MCP alone, these integration tests become server-level Rust integration tests rather than infra-001 harness tests.

**Decision for Stage 3c**: If injection_log population requires internal Store access not available through MCP, these scenarios are covered by the Rust server integration tests (I-01 through I-03) and the infra-001 harness tests focus on non-regression of existing status behavior.
