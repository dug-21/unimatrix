# Test Plan Overview: crt-050
# Phase-Conditioned Category Affinity (Explicit Read Rebuild)

GH Issue: #542

---

## Overall Test Strategy

This feature replaces the `PhaseFreqTable` rebuild source from `query_log` to `observations`,
adds outcome-based weighting via a two-query Rust post-process, introduces a new public accessor
`phase_category_weights()`, renames an `InferenceConfig` field, and adds a coverage-threshold
diagnostic. All components are internally focused with no new MCP tool exposure.

**Three test layers:**

| Layer | Location | Scope |
|-------|----------|-------|
| Unit | `#[cfg(test)]` modules inline or `*_tests.rs` files | SQL logic, row deserialization, weighting math, config serde, constant values |
| Service-level | `phase_freq_table.rs` `#[cfg(test)]` | `outcome_weight()`, `apply_outcome_weights()`, `phase_category_weights()`, cold-start contracts |
| Integration (infra-001) | `suites/test_lifecycle.py`, smoke | End-to-end tick rebuild; NULL `feature_cycle` degradation (AC-15) |

---

## Risk-to-Test Mapping

| Risk | Priority | Test File(s) | Test Count |
|------|----------|--------------|------------|
| R-01: write-path contract — `json_extract(input, '$.id')` works for hook-path rows | Critical | `store-queries.md` (AC-SV-01 unit test in `query_log_tests.rs`) | 1 |
| R-02: `outcome_weight()` vocabulary coverage and priority order | High | `phase-freq-table.md` (unit tests in `phase_freq_table.rs`) | 3 |
| R-03: mixed-weight bucket ordering with per-phase mean aggregation | High | `phase-freq-table.md` (unit tests in `phase_freq_table.rs`) | 2 |
| R-04: `min_phase_session_pairs` threshold boundary at N-1 and N | High | `status-diagnostics.md` (unit tests in `status.rs`) | 3 |
| R-05: `MILLIS_PER_DAY` constant value and boundary arithmetic | Med | `store-queries.md` (unit tests in `query_log.rs` or `phase_freq.rs`) | 3 |
| R-06: config field rename — serde alias and struct-literal sites | Med | `config.md` (unit tests in `config.rs`) | 2 |
| R-07: `phase_category_weights()` breadth formula and documentation | Med | `phase-freq-table.md` (unit tests in `phase_freq_table.rs`) | 2 |
| R-08: NULL `feature_cycle` — graceful 1.0 degradation | Low | `store-queries.md` (integration test, AC-15) | 1 |
| R-09: `phase_category_weights()` visibility deferred to W3-1 | Low | Documented open item only — no blocking test | 0 |
| R-10: `hook` vs `hook_event` column name | Med | `store-queries.md` (AC-02 / AC-13f unit test) | 1 |
| R-11: Full-scan latency without index on `observations.hook` | Med | No test — operational/monitoring concern; note in gaps | 0 |
| R-12: Unknown outcome strings default to 1.0 | Low | `phase-freq-table.md` (covered by R-02 exhaustive test) | 0 (covered) |

**Total planned new tests: ~23 distinct test cases**

---

## Cross-Component Test Dependencies

1. `store-queries` unit tests require a live `SqlxStore` (in-memory SQLite) — extend the
   existing `query_log_tests.rs` test helper pattern: `open_test_store()` from
   `crate::test_helpers`, direct `sqlx::query` inserts for `observations` rows.

2. `phase-freq-table` service-layer tests (`apply_outcome_weights`, `outcome_weight`,
   `phase_category_weights`) are pure-Rust: no DB fixture needed. Extend the existing
   `#[cfg(test)] mod tests` block in `phase_freq_table.rs` using the existing `table_with()`
   and `rank_bucket()` helpers.

3. `config` tests extend the existing `InferenceConfig` serde tests in `config.rs` — no new
   fixtures needed.

4. `status-diagnostics` tests for `warn_observations_coverage()` need a mock or in-memory
   store count. Follow the existing `status.rs` test pattern.

5. AC-15 (NULL `feature_cycle` integration test) requires a live DB and is the only item
   that could not be fully covered by a pure unit test — route to infra-001 lifecycle suite.

---

## Integration Harness Plan (infra-001)

### Suite Selection

| Suite | Applicable? | Reason |
|-------|-------------|--------|
| `smoke` | YES — mandatory gate | Any change at all; minimum gate before Gate 3c |
| `lifecycle` | YES | Tick-rebuild end-to-end; NULL feature_cycle degradation (AC-15) |
| `tools` | No | No new MCP tools; no tool parameter changes |
| `confidence` | No | No scoring formula changes; `phase_affinity_score()` signature unchanged |
| `contradiction` | No | Not touched |
| `security` | No | No new input surfaces |
| `volume` | No | No schema changes |
| `edge_cases` | No | Covered by unit tests |
| `protocol` | No | No protocol changes |

**Required suites in Stage 3c:** `smoke` (mandatory), `lifecycle` (feature-relevant).

### Existing Suite Coverage

The `lifecycle` suite's existing tests cover `context_get`/`context_lookup` multi-step flows
that write observations rows. The smoke test exercises server startup and a basic store→search
roundtrip. Neither suite has a test for the tick-time `PhaseFreqTable` rebuild specifically,
but the rebuild runs in the background tick which fires during longer tests. The NULL
`feature_cycle` scenario (AC-15) is not covered by any existing lifecycle test.

### New Integration Tests Needed

**1. `test_phase_freq_rebuild_null_feature_cycle` (in `suites/test_lifecycle.py`)**

Verifies AC-15 / FR-10 / R-08: sessions with NULL `feature_cycle` do not cause rebuild
errors. Validates the graceful 1.0-weight degradation path visible only through the full
tick-rebuild cycle with a real DB.

- Fixture: `server` (fresh DB, function scope)
- Steps:
  1. Store several entries (to populate `entries` table)
  2. Issue `context_get` calls (via MCP) to produce `observations` rows; the server
     internally creates a session with NULL `feature_cycle` (pre-col-022 style) or
     exercises the normal session path
  3. Trigger a tick cycle (wait for background tick or call a tick-adjacent status check)
  4. Call `context_status` and assert no error; server remains responsive
  5. Assert `context_search` still returns results (scoring path unblocked)
- Note: The NULL `feature_cycle` condition depends on session state; if the harness cannot
  directly insert NULL sessions, this test may be implemented as a unit test instead and
  this integration test slot filed as a GH Issue for the harness infrastructure gap.

**When NOT to add new tests:**
- AC-01 through AC-14 are all unit-testable without the MCP interface
- AC-09 (grep for deleted function) is a CI grep check, not a test
- All `outcome_weight` and `apply_outcome_weights` scenarios are pure-Rust unit tests

---

## Acceptance Criteria Coverage Summary

| AC-ID | Component | Layer |
|-------|-----------|-------|
| AC-01 | store-queries, phase-freq-table | Unit (DB fixture) |
| AC-02 | store-queries | Unit (DB fixture) |
| AC-03 | store-queries | Unit (DB fixture) |
| AC-04 | phase-freq-table | Unit (pure Rust) |
| AC-05 | phase-freq-table | Unit (pure Rust) |
| AC-06 | phase-freq-table | Unit (pure Rust, existing tests) |
| AC-07 | store-queries | Unit (DB fixture) |
| AC-08 | phase-freq-table | Unit (pure Rust) |
| AC-09 | store-queries | Grep check (CI) |
| AC-10 | config | Unit (serde) |
| AC-11 | status-diagnostics | Unit (warn! capture) |
| AC-12 | phase-freq-table | Eval harness (separate, not in test-plan scope) |
| AC-13a–h | phase-freq-table, store-queries | Unit |
| AC-14 | status-diagnostics, phase-freq-table | Unit |
| AC-15 | store-queries | Integration (infra-001 lifecycle) |
| AC-SV-01 | store-queries | Unit (DB fixture, write-path contract) |
