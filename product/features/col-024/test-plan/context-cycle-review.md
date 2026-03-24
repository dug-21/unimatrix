# col-024 Test Plan: context_cycle_review Three-Path Fallback
# File: `crates/unimatrix-server/src/mcp/tools.rs`

## Component Summary

The `context_cycle_review` handler's observation-loading block is restructured from two-path to
three-path:

```
1. load_cycle_observations(feature_cycle)      ← NEW primary path
   → non-empty: use; do NOT call load_feature_observations
   → empty: emit tracing::debug!, fall through

2. load_feature_observations(feature_cycle)    ← existing legacy-1
   → non-empty: use
   → empty: fall through

3. load_unattributed_sessions() + attribute_sessions(...)  ← existing legacy-2
```

The non-negotiable correctness property: when the primary path returns non-empty,
`load_feature_observations` must **not** be called. This prevents double attribution.

---

## Risk Coverage

| Risk | From RISK-TEST-STRATEGY | Test Below |
|------|------------------------|------------|
| R-03 (empty primary not forwarded) | High | T-CCR-01, T-CCR-02 |
| R-04 (branch verification) | High | T-CCR-01 (primary non-empty → legacy NOT called) |
| R-08 (fallback log missing/wrong level) | Med | T-CCR-03 |
| FM-01 (SQL error propagates, not falls back) | — | T-CCR-04 |
| AC-04 (three-path independence) | — | T-CCR-01 through T-CCR-03 together |
| AC-09/12 (backward compatibility) | — | T-CCR-05 |
| AC-14 (debug log on fallback) | — | T-CCR-03 |

---

## Unit Test Expectations

These tests operate either on a mock `ObservationSource` or via direct `services/observation.rs`
integration against a test store. The preferred approach is a mock because it allows verifying
call-site behavior (which methods were called) without setting up full store fixtures.

If a mock `ObservationSource` does not exist before col-024, the implementor must create one in
a `#[cfg(test)]` module within `tools.rs` or in a dedicated test module. The mock must implement
all five `ObservationSource` methods, including `load_cycle_observations`.

### T-CCR-01: `context_cycle_review_primary_path_used_when_non_empty`

**AC**: AC-04 (non-empty branch — `load_feature_observations` NOT called)
**Approach**: Mock `ObservationSource` with configurable return values and call tracking.

**Setup**:
```rust
// Mock returns a non-empty vec from load_cycle_observations
// Mock panics (or records a flag) if load_feature_observations is called
struct MockObservationSource {
    cycle_obs: Vec<ObservationRecord>,
    feature_obs_called: std::sync::atomic::AtomicBool,
}
impl ObservationSource for MockObservationSource {
    fn load_cycle_observations(&self, _: &str) -> Result<Vec<ObservationRecord>> {
        Ok(self.cycle_obs.clone())
    }
    fn load_feature_observations(&self, _: &str) -> Result<Vec<ObservationRecord>> {
        self.feature_obs_called.store(true, Ordering::SeqCst);
        Ok(vec![])
    }
    // ... other methods return Ok(vec![])
}
```

**Assertions**:
- Invoke `context_cycle_review` (or its inner observation-loading block) with the mock.
- `assert!(!mock.feature_obs_called.load(Ordering::SeqCst), "load_feature_observations must NOT be called when primary returns non-empty")`
- The returned report content is derived from `cycle_obs` (not empty).

---

### T-CCR-02: `context_cycle_review_fallback_to_legacy_when_primary_empty`

**AC**: AC-04 (empty primary → legacy fallback activates), AC-09, AC-12
**Approach**: Mock where `load_cycle_observations` returns `Ok(vec![])` and
`load_feature_observations` returns non-empty observations.

**Assertions**:
- `load_feature_observations` is called exactly once.
- The returned report is derived from the legacy observations (non-empty).

---

### T-CCR-03: `context_cycle_review_no_cycle_events_debug_log_emitted`

**AC**: AC-14, R-08
**Approach**: Mock + `tracing_test::traced_test`.

**Setup**:
- Mock returns `Ok(vec![])` from `load_cycle_observations`
- `feature_cycle = "legacy-feature-001"`

**Assertions**:
```rust
assert!(logs_contain("primary path empty"));
assert!(logs_contain("legacy-feature-001"),
    "log must contain the feature_cycle value");
```

**Log level assertion**: The log must appear at `DEBUG` level. Verify by running with
`RUST_LOG=info` — the log must NOT appear. With `RUST_LOG=debug` — it must appear.

---

### T-CCR-04: `context_cycle_review_propagates_error_not_fallback`

**AC**: FM-01 — SQL error propagates to caller, does not activate legacy fallback
**Approach**: Mock where `load_cycle_observations` returns `Err(ObserveError::Database("simulated"))`.

**Assertions**:
- The MCP handler returns an error response (not a report).
- `load_feature_observations` is NOT called (error is not treated as empty).

**Notes**: This tests the critical semantic boundary: `Err` ≠ `Ok(vec![])`. The legacy fallback
must only activate on `Ok(vec![])`, never on `Err(...)`.

---

### T-CCR-05: `context_cycle_review_existing_tests_unchanged`

**AC**: AC-09, AC-12
**Approach**: This is not a new test — it is the assertion that the full test suite for
`context_cycle_review` passes without modification.

**Stage 3c action**:
```bash
cargo test -p unimatrix-server context_cycle_review 2>&1 | tail -30
```

**Assertions**:
- All tests whose names contain `context_cycle_review` pass.
- No test is skipped, deleted, or marked as ignored.
- Test count is at least the pre-col-024 baseline (Stage 3c tester records the count before
  implementing col-024, then verifies it is unchanged or higher post-implementation).

---

## Integration Test Expectations (infra-001)

The `context_cycle_review` tool is exercised by:
- `suites/test_tools.py` — existing tests for the `context_cycle_review` MCP call
- `suites/test_lifecycle.py` — existing multi-step flows

No new integration tests are needed. The existing suite validates:
1. MCP wire format of the response is unchanged (AC-12)
2. Legacy path still works for features without `cycle_events` rows (AC-09)

If the `tools` suite has tests that explicitly call `context_cycle_review` for a feature that
predates `cycle_events`, those tests continue to serve as AC-09 regression coverage.

---

## Backward Compatibility Verification (AC-09/12)

Run the full `unimatrix-server` test suite and verify the pre-existing
`context_cycle_review` tests pass:

```bash
cargo test -p unimatrix-server 2>&1 | grep -E "test_.*cycle_review|FAILED|ok\." | tail -20
```

Additionally verify via infra-001 smoke + tools suites:
```bash
cd product/test/infra-001
python -m pytest suites/test_tools.py -v -k "cycle_review" --timeout=60
```

---

## tracing-test Dependency

AC-14 (T-CCR-03) requires log-capture. If `tracing-test` is not in dev-dependencies:

```toml
# crates/unimatrix-server/Cargo.toml
[dev-dependencies]
tracing-test = "0.2"
```

And the test requires:
```rust
#[tracing_test::traced_test]
fn test_fn() { ... }
```

The same dependency is also needed for T-ENR-03. Add once; both tests share it.

---

## Summary: AC Coverage from this Component

| AC-ID | Test Name | Location |
|-------|-----------|----------|
| AC-04 | T-CCR-01 (non-empty branch) | tools.rs test module |
| AC-04 | T-CCR-02 (empty → fallback) | tools.rs test module |
| AC-09 | T-CCR-05 (existing tests unchanged) | Full cargo test run |
| AC-12 | T-CCR-05 + infra-001 tools suite | cargo test + pytest |
| AC-14 | T-CCR-03 (debug log assertion) | tools.rs test module |
| FM-01 | T-CCR-04 (error propagation) | tools.rs test module |
