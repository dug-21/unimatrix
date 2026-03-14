# Test Plan: confidence-refresh-batch
## Component: `crates/unimatrix-server/src/services/status.rs` (refresh loop changes) + `crates/unimatrix-server/src/infra/coherence.rs` (batch size constant)

### Risk Coverage

| Risk | Severity | Test(s) |
|------|----------|---------|
| R-13 | Medium | Duration guard checked BEFORE `update_confidence` call (pre-iteration) |
| IR-02 | High | alpha0/beta0 snapshotted outside loop — not re-read per entry |
| FM-01 | Low | Maintenance tick panic after refresh loop but before ConfidenceState write — graceful |
| AC-07 | Acceptance | `MAX_CONFIDENCE_REFRESH_BATCH == 500`, duration guard present |

---

## Unit Tests

### AC-07: Batch Size Constant

The simplest test: verify the constant has the new value.

```rust
// In services/status.rs or infra/coherence.rs tests
#[test]
fn test_max_confidence_refresh_batch_is_500() {
    use crate::infra::coherence::MAX_CONFIDENCE_REFRESH_BATCH;
    assert_eq!(MAX_CONFIDENCE_REFRESH_BATCH, 500,
        "MAX_CONFIDENCE_REFRESH_BATCH must be 500 after crt-019");
}
```

---

### R-13: Duration Guard Pre-Iteration Placement

**Risk**: If the guard is checked AFTER `update_confidence()` instead of BEFORE, a slow entry
pushes the batch over the 200ms limit before breaking. FR-05 specifies pre-check.

The placement cannot be verified by a pure unit test without mocking `Instant`. The test plan
requires code review as primary coverage, with a secondary mock-based test as a stretch goal.

**Code review assertion** (documented in RISK-COVERAGE-REPORT.md at Stage 3c):
```
for each entry in candidate_entries {
    if start.elapsed() > Duration::from_millis(200) {
        log::warn!("confidence refresh budget exhausted after {} entries", count);
        break;  // ← break fires BEFORE update_confidence, not after
    }
    update_confidence(entry, alpha0, beta0, now, store);
    count += 1;
}
```

**Unit test (mock-based, stretch goal)**:

If the `run_maintenance` function accepts an injectable `Instant` supplier or a deadline
parameter (possible future refactor), this can be tested directly. For now, the implementation
must use `Instant::now()` inline and the guard placement is code-review-verified.

```rust
// Structural verification: the refresh loop must NOT look like:
// update_confidence(entry, ...);
// if start.elapsed() > budget { break; }
// The test plan asserts the above pattern is absent from the implementation diff.
```

---

### IR-02: Snapshot Pattern — alpha0/beta0 Outside Loop

**Risk**: If `ConfidenceState.read()` is called inside the 500-iteration loop, the write lock
is acquired/released 500 times, serializing concurrent search calls.

**Correct pattern**:
```rust
// Snapshot ONCE before loop begins
let (alpha0, beta0) = {
    let state = confidence_state.read().unwrap_or_else(|e| e.into_inner());
    (state.alpha0, state.beta0)
};

for entry in candidate_entries {
    if start.elapsed() > budget { break; }
    update_confidence(entry, alpha0, beta0, now, store);
}
```

**Test**: Code review verification. Document in RISK-COVERAGE-REPORT.md.

If a performance test harness is available (not currently in infra-001), a 500-entry refresh
with concurrent search calls should complete within 300ms total. This is a stretch goal.

---

### Partial Refresh Logging (FM-04 complement)

```rust
// Verify the implementation logs a warning when the duration guard fires early
// This is testable via captured log output in unit tests using a log capture fixture
#[test]
fn test_refresh_logs_partial_count_on_budget_exhaustion() {
    // Construct a minimal run_maintenance scenario with a mock time source
    // that immediately returns > 200ms on the second iteration.
    // Assert log output contains "budget" or "partial" with the entry count.
    // Implementation detail: depends on whether the function takes an injectable
    // time source. If not, this is a code-review-only check.
    //
    // If injectable: use a mock that returns 201ms on iteration 2, assert count == 1 in log.
    // If not injectable: document as code-review in RISK-COVERAGE-REPORT.md.
}
```

---

## Integration Expectations

The confidence refresh batch is exercised indirectly through the maintenance tick in the
integration harness. The `context_status` tool with `maintain: true` triggers `run_maintenance`.

**Integration test coverage**:
- `test_lifecycle.py::test_empirical_prior_flows_to_stored_confidence` — triggers maintenance
  tick, verifies entries are refreshed (indirectly validates the batch runs).
- No dedicated integration test for the 200ms guard is planned because the guard is a
  safety ceiling that only fires under load, not in typical integration test conditions.

**Acceptance verification at Stage 3c**:
```bash
grep -n "MAX_CONFIDENCE_REFRESH_BATCH" crates/unimatrix-server/src/infra/coherence.rs
# Must show: MAX_CONFIDENCE_REFRESH_BATCH: usize = 500

grep -n "Instant" crates/unimatrix-server/src/services/status.rs
# Must show Instant::now() used in run_maintenance, inside the refresh loop area
```
