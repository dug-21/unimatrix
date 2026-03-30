# Component Test Plan: status_service

## Component Scope

`crates/unimatrix-server/src/services/status.rs` — modifications:
- `PENDING_REVIEWS_K_WINDOW_SECS` named constant (90 days = 7_776_000 seconds)
- Phase 7b in `compute_report()`: call `store.pending_cycle_reviews(k_window_cutoff)`
  and populate `report.pending_cycle_reviews`
- K-window cutoff computation: `now() - PENDING_REVIEWS_K_WINDOW_SECS`

**AC coverage**: AC-09, AC-10.
**Risk coverage**: R-07 (partial — Phase 7b wiring), R-12 (partial — pool selection).

---

## Unit Tests (in `#[cfg(test)]` inside `services/status.rs`)

### SS-U-01: PENDING_REVIEWS_K_WINDOW_SECS constant value (NFR-05, C-11)

```rust
#[test]
fn test_pending_reviews_k_window_secs_is_90_days() {
    // 90 days * 24 * 60 * 60 = 7_776_000 seconds
    assert_eq!(PENDING_REVIEWS_K_WINDOW_SECS, 7_776_000u64,
        "K-window must default to 90 days (7_776_000 seconds)");
}
```

### SS-U-02: K-window cutoff is computed as now - PENDING_REVIEWS_K_WINDOW_SECS

This is a behavioral assertion rather than a direct unit test. It is verified
indirectly by CRS-I-06 (cycles outside K-window excluded) and SS-I-02 below.
A direct unit test is not needed if the constant is verified by SS-U-01 and the
SQL behaviour is verified by the store integration tests.

---

## Integration Tests (store-backed compute_report tests)

These tests call `compute_report()` (or the `context_status` handler) with a
real SqlxStore seeded with relevant data.

### SS-I-01: Phase 7b populates pending_cycle_reviews for un-reviewed cycles (AC-09, R-07)

```
#[tokio::test]
async fn test_compute_report_includes_pending_cycle_reviews()

Arrange:
  Open fresh SqlxStore.
  Insert cycle_events rows (event_type='cycle_start') within K-window:
    cycle_id = "pending-A", timestamp = now - 1_day
    cycle_id = "pending-B", timestamp = now - 5_days
  Insert cycle_review_index row for "pending-B" only.

Act: call compute_report() (or context_status handler).

Assert:
  StatusReport.pending_cycle_reviews == vec!["pending-A"]
  "pending-B" is not in the list (it has a review row).
```

### SS-I-02: Phase 7b returns empty list when all cycles are reviewed (AC-10, R-07)

```
#[tokio::test]
async fn test_compute_report_pending_cycle_reviews_empty_when_all_reviewed()

Arrange:
  Insert cycle_events rows for "done-X" and "done-Y" within K-window.
  Insert cycle_review_index rows for both.

Act: call compute_report().

Assert:
  StatusReport.pending_cycle_reviews is empty.
```

### SS-I-03: Phase 7b excludes cycles outside K-window (R-07)

```
#[tokio::test]
async fn test_compute_report_excludes_old_cycles_from_pending()

Arrange:
  Insert cycle_events row for "old-cycle" with timestamp = now - 91_days.
  No review row for "old-cycle".
  Insert cycle_events row for "recent-cycle" with timestamp = now - 1_day.
  No review row for "recent-cycle".

Act: call compute_report().

Assert:
  StatusReport.pending_cycle_reviews == vec!["recent-cycle"]
  "old-cycle" is absent.
```

### SS-I-04: Phase 7b failure is non-blocking — context_status still returns (failure mode)

```
This tests the graceful degradation path: if pending_cycle_reviews() returns an Err,
context_status must not fail entirely.

Note: This requires either a store error injection or a separate unit test on the
Phase 7b error-handling path. If store error injection is not feasible, this scenario
is documented as a code review gate: the Phase 7b caller must have a
.unwrap_or_else(|e| { tracing::warn!(...); vec![] }) or equivalent pattern.

Assert (code review): Phase 7b wraps the pending_cycle_reviews() call in error
handling that defaults to an empty vec on Err, with a tracing::warn! log.
```

### SS-I-05: pending_cycle_reviews always computed — no opt-in parameter required (C-07)

```
Verify by reading compute_report() implementation:
Assert: there is no conditional guard (no if flag, no check_pending parameter) around
the Phase 7b call. It runs unconditionally.
This is a code review assertion, not a runtime test.
```

---

## Static / Grep Checks

### SS-G-01: PENDING_REVIEWS_K_WINDOW_SECS is a named constant, not an inline literal (C-11)

```bash
grep -n 'PENDING_REVIEWS_K_WINDOW_SECS' crates/unimatrix-server/src/services/status.rs
```
Assert: at least one match (the const definition).

```bash
grep -n '7_776_000\|7776000' crates/unimatrix-server/src/services/status.rs
```
Assert: the magic number appears only as the value assigned to the const, not as an
inline literal at the call site.

### SS-G-02: pending_cycle_reviews query uses read_pool (R-12)

This is verified by CRS-G-02 in `cycle_review_index.md`. The `status_service`
component calls `store.pending_cycle_reviews()`, which delegates to `cycle_review_index.rs`.
The pool selection is enforced at the store layer; no additional check needed here.

---

## Context_status MCP-Level Verification

After infra-001 `tools` suite runs, verify:

1. `context_status` response JSON includes `pending_cycle_reviews` key.
2. When no un-reviewed cycles exist (freshly seeded DB), the field is `[]`.
3. The field is always present (not absent) regardless of its content.

Specific infra-001 test: `test_status_pending_cycle_reviews_field_present` planned in
OVERVIEW.md Integration Harness Plan section.

---

## Assertions and Expected Behaviors

| Behavior | Assertion |
|----------|-----------|
| `PENDING_REVIEWS_K_WINDOW_SECS` value | `== 7_776_000` |
| `compute_report()` with unreviewed K-window cycles | `pending_cycle_reviews` contains those cycle IDs |
| `compute_report()` with all cycles reviewed | `pending_cycle_reviews` is `vec![]` |
| `compute_report()` with only out-of-window cycles | `pending_cycle_reviews` is `vec![]` |
| `compute_report()` with `pending_cycle_reviews()` returning `Err` | `compute_report()` succeeds; field defaults to `vec![]` with warning log |
| Phase 7b runs unconditionally | No opt-in flag in code |

---

## Edge Cases

| Edge Case | Test | Expected |
|-----------|------|---------|
| `pending_cycle_reviews()` returns cycles in non-alphabetical order | SS-I-01 with multi-cycle result | Result is ordered by cycle_id (SQL `ORDER BY ce.cycle_id`) |
| K-window boundary at exactly now - 7_776_000 seconds | SS-I-03 variant | Boundary cycle included (`>=` comparison) |
| No cycle_events rows at all (empty DB) | SS-I-02 variant with empty DB | Returns `vec![]` without error |
| `pending_cycle_reviews()` returns a very large list (100+ cycles) | Not required for Stage 3c — deferred to volume suite | Response still returns OK |
