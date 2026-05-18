# vnc-016 Test Plan Overview

## Feature Summary

vnc-016 delivers four coordinated changes: a one-line SQL fix in `read.rs`, a Rust unit test
for that function, a Python harness client extension, and a production usage-gate fix in
`usage.rs`/`tools.rs`. Two new integration tests in `test_tools.py` verify end-to-end wiring
for the `DependencyOnDeprecatedRule` detection path.

---

## Test Strategy

| Level | What Is Tested | Tooling |
|-------|----------------|---------|
| Rust unit (store) | `query_stale_prerequisite_edges_for_cycle` positive and negative paths directly against SQLite | `cargo test -p unimatrix-store` |
| Rust unit (server) | `UsageContext.write_capable` gate logic in isolation — both branches | `cargo test -p unimatrix-server` |
| Python integration | Full MCP wiring: `context_store` → `feature_entries` → SQL query → `context_cycle_review` JSON response | `pytest suites/test_tools.py` |
| Regression smoke | All existing behavior unchanged after `client.py` and `usage.rs` changes | `pytest -m smoke` |

---

## Risk-to-Test Mapping

| Risk ID | Priority | Risk Description | Tests |
|---------|----------|-----------------|-------|
| R-01 | Critical | Integration test vacuously passes — `feature_entries` row absent for entry A | `test_dependency_on_deprecated_e2e` (positive path fails without fix); deliberate omission of `feature_cycle` must make assertion fail |
| R-02 | Critical | Integration test vacuously passes — memoized result returned (`force=False`) | Both integration tests assert `force=True`; code inspection; unique cycle IDs per test |
| R-03 | Critical | Rust positive-path test assertion is structurally always-true | `test_query_stale_prerequisite_edges_for_cycle_returns_pair` must assert `is_ok()` + `len() == 1` + `[0] == (A, B)` — all three sub-assertions; must use `?` or `expect`, not `unwrap_or_else` |
| R-04 | Critical | Rust negative-path companion absent — broken JOIN scoping undetectable | `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry` must be present and assert `is_empty()` |
| R-05 | High | Future SQL regression re-concealed by `unwrap_or_else` in `tools.rs:2169` | Rust unit test provides direct store-layer regression guard; integration test provides end-to-end coverage; both required |
| R-06 | High | `context_store` with unenrolled/Restricted agent — old trust gate silently skips `feature_entries` | Positive test uses enrolled Restricted+Write agent (`test_agent_id`), NOT `"human"` for step 4; `test_write_capable_true_yields_feature_recording` confirms fixed gate |
| R-07 | High | Observation cycle_id mismatch — empty-cycle early-exit triggered | Single `cycle_id` binding at top of each test function; same variable passed to `_seed_observation_sql` and `context_cycle_review` |
| R-08 | Med | Negative-path assertion is too broad — misses always-fires regression | Negative test asserts `not any(h["rule_name"] == "dependency_on_deprecated" ...)` — not `hotspots == []` |
| R-09 | Low | `feature_cycle` forwarded as explicit JSON `null` | Code inspection: `if feature_cycle is not None: args["feature_cycle"] = feature_cycle` guard present |
| R-10 | Med | Existing call sites broken by `client.py` signature change | Run existing full pytest suite; all existing `context_store` call sites must continue to pass |

---

## Component-to-Test-Plan Mapping

| Component | Files Changed | Test Plan | AC-IDs Covered |
|-----------|--------------|-----------|----------------|
| SQL Fix | `read.rs:1618` | `test-plan/sql-fix.md` | AC-04 |
| Rust Unit Test | `read.rs mod tests` | `test-plan/rust-unit-test.md` | AC-09 |
| Harness Client | `harness/client.py` | `test-plan/harness-client.md` | AC-05, AC-06 |
| Usage Gate Fix | `usage.rs`, `tools.rs` | `test-plan/usage-gate-fix.md` | AC-10, AC-11, AC-12, AC-13 |
| Integration Tests | `suites/test_tools.py` | `test-plan/integration-tests.md` | AC-01, AC-02, AC-03, AC-07, AC-08 |

---

## Cross-Component Test Dependencies

1. The SQL fix (Component 1) must be applied before the Rust unit tests (Component 2) will
   pass. The unit tests are the primary regression guard for the fix.

2. The harness client extension (Component 3) must be applied before the integration tests
   (Component 5) can pass `feature_cycle` to `context_store`.

3. The usage gate fix (Component 4) must be applied before the integration tests (Component 5)
   will pass with a Restricted+Write agent. Tests that use `agent_id="human"` would pass
   without the gate fix (silent vacuous pass), which is why `test_agent_id` (Restricted+Write)
   is required for step 4 of the positive test.

4. All `UsageContext` construction sites must be updated (Component 4) before the workspace
   will compile. Omitting `write_capable` from any site is a compile error.

---

## Integration Harness Plan

### Suite Selection

This feature touches: server tool logic (tools.rs, usage.rs), store-layer SQL, lifecycle
behavior (feature_entries written during store, read during cycle_review).

| Suite to Run | Reason |
|-------------|--------|
| `smoke` | Minimum gate — mandatory |
| `tools` | Feature adds new tests here; client.py changes affect all tool calls |
| `lifecycle` | `feature_entries` write path is lifecycle behavior; cycle attribution persistence matters |
| `security` | `write_capable` gate change touches capability enforcement |

### New Integration Tests Required

Two new test functions must be added to `suites/test_tools.py` in the vnc-015 section
(after line 3048). Both use the `server` fixture (function scope — fresh DB per test).

**Test 1: `test_dependency_on_deprecated_e2e`**
- Fixture: `server` (fresh DB, no state leakage)
- Positive path: 9-step scenario per FR-05.2
- Key assertions: `assert "hotspots" in data` + `any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])`
- Critical constraint: `agent_id=test_agent_id` (Restricted+Write) at step 4, NOT `"human"`

**Test 2: `test_dependency_on_deprecated_no_finding_without_stale_edge`**
- Fixture: `server` (same fresh-DB guarantee, independent cycle ID)
- Negative path: store two entries without a stale edge; seed observations; assert rule absent
- Key assertion: `not any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])`

Both tests must use `force=True` on every `context_cycle_review` call (hard constraint — no
exception). Both must bind `cycle_id` once at the top of the function and reuse it everywhere.

### Integration Tests NOT Required (Existing Coverage)

- `protocol` suite: no protocol changes in vnc-016
- `volume` suite: no schema changes, no scale behavior changes
- `contradiction` suite: no contradiction logic changes
- `confidence` suite: no confidence scoring changes
- `edge_cases` suite: covered by the two new test functions directly

---

## Acceptance Criteria Summary

| AC-ID | Verification Method | Component |
|-------|--------------------|-----------| 
| AC-01 | pytest exit 0 | Integration Tests |
| AC-02 | Code inspection — 9 steps in order | Integration Tests |
| AC-03 | Code inspection — exact assertion string | Integration Tests |
| AC-04 | grep + cargo test | SQL Fix |
| AC-05 | Code inspection + pytest (backward compat) | Harness Client |
| AC-06 | cargo test --workspace + full pytest | All components |
| AC-07 | Code inspection — force=True present | Integration Tests |
| AC-08 | pytest exit 0 | Integration Tests |
| AC-09 | cargo test -p unimatrix-store exit 0 | Rust Unit Test |
| AC-10 | grep + cargo build | Usage Gate Fix |
| AC-11 | grep + code inspection | Usage Gate Fix |
| AC-12 | Code inspection | Usage Gate Fix + Integration Tests |
| AC-13 | cargo test -p unimatrix-server | Usage Gate Fix |
