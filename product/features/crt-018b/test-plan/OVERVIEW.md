# crt-018b: Test Plan Overview — Effectiveness-Driven Retrieval

## Overall Test Strategy

Three test layers, each targeting a distinct failure class:

| Layer | Location | Runs via |
|-------|----------|----------|
| Unit | `#[cfg(test)]` modules in each modified file | `cargo test --workspace` |
| Integration (infra-001) | `product/test/infra-001/suites/` | `pytest` |
| Concurrency / lock ordering | `#[tokio::test]` in `services/effectiveness.rs` and `services/search.rs` | `cargo test --workspace` |

**Guiding principle**: the riskiest failures (R-01, R-02, R-03, R-13) are architectural — they require structural assertions at unit level AND integration-level end-to-end confirmation. Pure behaviour coverage (R-04 through R-12) can be satisfied by unit tests alone. AC-17's four integration scenarios anchor the feature from above.

No new isolated test scaffolding. All tests extend the existing cumulative infrastructure:
- `ConfidenceState` tests in `services/confidence.rs` (structural pattern to mirror)
- `tests_classify.rs` in `unimatrix-engine::effectiveness` (extend for constant tests)
- `read.rs` and search pipeline tests in `unimatrix-server` (extend for delta call-site tests)
- `product/test/infra-001/suites/test_lifecycle.py` (extend for AC-17 scenarios)

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component | Test Layer | AC / FR |
|---------|----------|-----------|------------|---------|
| R-01 | Critical | effectiveness-state, search-utility-delta | Unit + Concurrency | AC-02 |
| R-02 | Critical | search-utility-delta | Unit | AC-04, AC-05, AC-16 |
| R-03 | Critical | auto-quarantine-guard | Integration | AC-10, AC-13 |
| R-04 | High | background-tick-writer | Integration | AC-01, AC-09 |
| R-05 | High | search-utility-delta | Unit | AC-04, AC-05 |
| R-06 | High | search-utility-delta | Unit | AC-02 |
| R-07 | High | search-utility-delta, briefing-tiebreaker | Unit | AC-06 |
| R-08 | Medium | background-tick-writer, auto-quarantine-audit | Unit | FR-13 |
| R-09 | Medium | briefing-tiebreaker | Unit | AC-07, AC-08 |
| R-10 | Medium | effectiveness-state | Unit | AC-03 |
| R-11 | Medium | auto-quarantine-guard | Unit | AC-14 |
| R-12 | Low | auto-quarantine-guard | Integration | AC-13, FR-14 |
| R-13 | Critical | background-tick-writer, auto-quarantine-guard | Unit + Concurrency | NFR-02 |
| R-14 | Medium | search-utility-delta | Integration | AC-17 item 4 |

**Critical risks (R-01, R-02, R-03, R-13)** require a minimum of 3 test scenarios each. All four must have at least one integration-visible scenario confirming correct runtime behavior through the MCP interface.

---

## Cross-Component Test Dependencies

The six components have the following test dependencies:

```
effectiveness-state
    -> background-tick-writer (write path test depends on EffectivenessState struct)
    -> search-utility-delta (snapshot pattern depends on EffectivenessStateHandle)
    -> briefing-tiebreaker (same snapshot handle)

background-tick-writer
    -> auto-quarantine-guard (auto-quarantine triggered inside tick write path)
    -> auto-quarantine-audit (audit events emitted from tick write path)

auto-quarantine-guard
    -> auto-quarantine-audit (every quarantine fires an audit event)
```

Test ordering within a single crate: `effectiveness-state` unit tests must pass before any tests that depend on `EffectivenessStateHandle`. This is natural given Rust's compilation dependency graph — no explicit test ordering needed.

---

## Integration Harness Plan

### Applicable Existing Suites

| Suite | Applicability | Reason |
|-------|--------------|--------|
| `test_lifecycle.py` | **Primary** | Background tick write path, store→search ordering, quarantine lifecycle — all are multi-step lifecycle scenarios |
| `test_tools.py` | Secondary | `context_search` and `context_briefing` tools are the visible surfaces; ordering assertions go here if test fits single-tool scope |
| `test_security.py` | Secondary | Auto-quarantine DoS-via-env-var (UNIMATRIX_AUTO_QUARANTINE_CYCLES validation), audit event agent_id=system identity |
| `smoke` subset | Mandatory gate | All feature tests must pass the smoke gate before feature-specific suites |

Suites NOT needed: `protocol`, `volume`, `contradiction`, `confidence`, `edge_cases` — none of these touch the modified surfaces (search re-ranking internals, briefing sort, background tick, quarantine).

### New Integration Tests Required

The following new tests MUST be added to `product/test/infra-001/suites/test_lifecycle.py`. They cover AC-17 and the four Critical/High integration-visible risks.

#### AC-17 Item 1 — Background Tick Produces Correct Utility Deltas in Search Ordering

```python
# test_effectiveness_search_ordering_after_tick(server)
# Fixture: requires background tick to have fired. Strategy: use populated_server
# fixture + pre-seed entries with known helpfulness patterns, then inspect ordering.
# Since the tick runs every 15min in production but we cannot wait, this test
# validates the ordering contract: if we manually write EffectivenessState via
# a test helper that simulates the tick write, two entries differing only in
# effectiveness category must rank in the expected order.
#
# Alternative approach (preferred for integration tests): use context_status to
# confirm the effectiveness report is non-empty after a tick has fired in a
# long-running shared_server fixture.
```

This test requires that the integration fixture has a non-zero confidence spread (AC-17 item 4 prerequisite). The `populated_server` fixture pre-loads 50 entries via `make_entries()`, which is sufficient to generate spread when the background tick fires.

#### AC-17 Item 2 — Briefing Orders Effective Above Ineffective at Same Confidence

```python
# test_briefing_effectiveness_tiebreaker(server)
# Store two entries with identical content similarity but differing helpfulness
# vote patterns (one highly voted helpful, one voted unhelpful). Call context_briefing.
# Assert that the entry with more helpful votes appears earlier in the briefing
# output when confidence values are equal.
#
# This tests the BriefingService effectiveness_priority tiebreaker (AC-07, AC-08)
# through the MCP interface. The tiebreaker is only visible when two entries have
# equal confidence — the test must construct that condition explicitly.
```

#### AC-17 Item 3 — Auto-Quarantine Fires After N Consecutive Bad Ticks

```python
# test_auto_quarantine_after_consecutive_bad_ticks(server)
# This is the critical integration test for R-03 and AC-10.
# Since we cannot drive the background tick from outside the binary in the current
# harness, this test validates the end-to-end auto-quarantine behavior by:
# 1. Storing an entry that will accumulate bad classifications
# 2. Observing context_status after multiple background ticks have fired
# 3. Confirming that the entry's status becomes "quarantined" after N ticks
#
# NOTE: Given the 15-minute tick interval, this test will use @pytest.mark.xfail
# if the background tick cannot be driven externally. File a GH Issue for a
# test-mode tick trigger (e.g., UNIMATRIX_TICK_INTERVAL_SECONDS=0 env var).
# For Stage 3c, the implementer should determine if the background tick is
# test-drivable. If not, this scenario is covered by the unit test for the
# auto-quarantine trigger logic and the integration test is filed as a gap.
```

#### AC-17 Item 4 — crt-019 Confidence Spread Non-Zero in Fixture

```python
# test_crt019_spread_nonzero_prerequisite(shared_server)
# Assert context_status returns observed_spread > 0 (not cold-start default only).
# This is a prerequisite check. Uses shared_server (populated state).
# Reuses the existing status parsing infrastructure from test_lifecycle.py.
```

#### R-04 — context_status Does Not Write EffectivenessState

```python
# test_context_status_does_not_advance_consecutive_counters(server)
# Call context_status 5 times. Then call context_search. Confirm entries do not
# have unexpectedly changed ordering compared to a server that received 0 status
# calls. Since we cannot read consecutive_bad_cycles directly through MCP,
# the observable effect is: auto-quarantine must NOT fire solely from status calls.
# Proxy: store an entry, call context_status 20 times, confirm entry is still Active.
```

#### R-13 — Lock Not Held During SQL Quarantine (Concurrency Test)

This risk is verified by:
1. A unit test asserting the write guard scope ends before `quarantine_entry()` is called (code structure test)
2. A timing assertion in the integration test for AC-17 item 3: `context_search` issued concurrently during auto-quarantine must complete in < 100ms

### Fixture Selection

| Test | Fixture |
|------|---------|
| AC-17 items 1, 2, 4 | `shared_server` (needs pre-populated state for spread) |
| AC-17 item 3 | `server` (fresh DB, deterministic quarantine state) |
| R-04 context_status non-writing | `server` |
| R-13 concurrency | `server` |

### Tests NOT Needed in infra-001

The following are adequately covered by unit tests and do not require integration coverage:
- `utility_delta` function correctness (all five categories) — pure function, unit-testable
- `effectiveness_priority` correctness — pure function, unit-testable
- Constant values (UTILITY_BOOST, SETTLED_BOOST, UTILITY_PENALTY) — compile-time constants
- Briefing sort key order (primary vs. secondary) — requires controlled confidence fixture, easier as unit test
- `AUTO_QUARANTINE_CYCLES=0` disable path — state machine logic, unit-testable
- `consecutive_bad_cycles` increment/reset/remove semantics — unit-testable state transitions
- Lock poison recovery — unit-testable with `std::panic::catch_unwind`
