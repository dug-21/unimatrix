# Test Plan: PhaseFreqTable Alignment Guard

**Component:** `crates/unimatrix-server/src/services/status.rs` (step 4 preamble)
**Risks Covered:** R-11, R-16
**ACs Covered:** AC-17

---

## Overview

ADR-003 specifies a tick-time `tracing::warn!` emitted when the
`query_log_lookback_days` window (from `InferenceConfig`) extends beyond the age of
the oldest retained cycle's `computed_at` timestamp. This guard is advisory — it
does not block GC or alter config. The test plan must verify both the
warning-fires and warning-suppressed conditions to distinguish a correct predicate
from an inverted one (R-11), and must verify the K-boundary accuracy (R-16).

---

## Unit Test Expectations

### `test_gc_phase_freq_table_mismatch_warning` (AC-17)

Uses `tracing-test` (`#[traced_test]` or equivalent subscriber capture).

**Sub-case 1: Warning fires (mismatch present)**

**Arrange:**
- Insert K = 5 reviewed cycles into `cycle_review_index`, each with `computed_at`
  set to `now - N days` where N < 7 (all reviews within the past 7 days).
- Configure `query_log_lookback_days = 365` (much larger than 7-day coverage).
- Configure `activity_detail_retention_cycles = 5`.
- Insert additional cycles beyond K so some are purgeable (needed to trigger step 4).

**Act:** Execute the step 4 preamble (PhaseFreqTable alignment check portion of
the GC block).

**Assert:**
- A `WARN`-level log event is emitted.
- The warning message contains `"query_log_lookback_days"`.
- The warning message contains `"retention window"` (per AC-17 specification).

---

**Sub-case 2: Warning suppressed (sufficient coverage)**

**Arrange:**
- Same 5 cycles reviewed within the past 7 days.
- Configure `query_log_lookback_days = 3` (narrower than 7-day coverage).

**Act:** Execute the step 4 preamble.

**Assert:**
- No `WARN`-level event emitted containing `"query_log_lookback_days"`.
- No errors.

---

**Sub-case 3: Check skipped when fewer than K cycles exist (R-16)**

**Arrange:**
- Insert 3 reviewed cycles into `cycle_review_index`.
- Configure `activity_detail_retention_cycles = 10` (K = 10, more than 3 reviews).

**Act:** Execute the step 4 preamble.

**Assert:**
- No `WARN`-level event emitted for PhaseFreqTable mismatch.
- Rationale: fewer than K reviews means no pruning has occurred; no data gap
  is possible; the check is skipped per ADR-003.

---

## K-Boundary Accuracy (R-16)

The alignment check retrieves the `computed_at` of the K-th most recent cycle (the
oldest retained cycle). This must not be the K+1th or the most recent.

**Arrange:**
- Insert exactly K = 3 cycles with distinct `computed_at` values:
  - cycle_a: `computed_at = T - 1 day` (newest)
  - cycle_b: `computed_at = T - 5 days`
  - cycle_c: `computed_at = T - 30 days` (oldest; the K=3rd retained)
- Configure `query_log_lookback_days = 20` (covers cycle_b at 5 days but not
  cycle_c at 30 days).

**Act:** Execute the alignment check with `activity_detail_retention_cycles = 3`.

**Assert:**
- Warning fires (cycle_c is 30 days old, lookback is 20 days → mismatch).

**Negation (off-by-one check):**
- If the check mistakenly used cycle_b (5 days) as the boundary, it would NOT fire
  (5 days < 20 days). The warning firing confirms the K-th cycle (cycle_c at 30 days)
  is used, not the K-1th.

---

## Predicate Correctness (R-11)

ADR-003 specifies the condition:
```
if oldest_retained_computed_at < now - query_log_lookback_days * 86400 {
    tracing::warn!(...)
}
```

The inverted condition (`>`) would warn on sufficient coverage and suppress on
insufficient coverage. The two sub-cases in `test_gc_phase_freq_table_mismatch_warning`
are specifically designed to distinguish between these: sub-case 1 must warn and
sub-case 2 must not. Both failing (both warn or neither warn) indicates a predicate bug.

---

## Structural Assertions

- The alignment check runs at the start of step 4, after `list_purgeable_cycles`
  resolves the retain set (oldest retained cycle is a by-product of that query).
- The check is skipped (no warning, no error) when `cycle_review_index` has fewer
  than K rows.
- The `tracing::warn!` includes both `query_log_lookback_days` and
  `activity_detail_retention_cycles` as structured fields, per ADR-003.
