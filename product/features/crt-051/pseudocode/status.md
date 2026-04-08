# Component: services/status.rs
# crt-051 Pseudocode

## Purpose

`status.rs` contains `compute_report()`, which orchestrates all phases of
`context_status` data collection and assembles `StatusReport`. This component has one
load-bearing change: the call site at Phase 5 that passes an argument to
`coherence::contradiction_density_score()`. The argument must change from
`report.total_quarantined` to `report.contradiction_count`.

A phase-ordering comment must also be added at the Phase 5 call site to document the
dependency on Phase 2.

No other code in `status.rs` changes.

---

## Modified Code: Phase 5 Call Site (~line 747)

### Old Code (exact text to be replaced)

```rust
report.contradiction_density_score =
    coherence::contradiction_density_score(report.total_quarantined, report.total_active);
```

### New Code (exact replacement)

```rust
// report.contradiction_count is populated in Phase 2 (contradiction cache read);
// Phase 5 must not be reordered above Phase 2. See crt-051 ADR-001.
report.contradiction_density_score =
    coherence::contradiction_density_score(report.contradiction_count, report.total_active);
```

### What Changed

- First argument: `report.total_quarantined` (type `u64`) -> `report.contradiction_count`
  (type `usize`)
- Added two-line comment above the assignment
- `report.total_active` (second argument) is unchanged

### Why No Cast is Needed

`StatusReport.contradiction_count` is already `usize`. The new function parameter is
`usize`. The types match directly. No `as usize` or `as u64` cast is required or
permitted.

---

## Unchanged Code: generate_recommendations() Call Site (~line 784)

This block passes `report.total_quarantined` to `generate_recommendations()`. It must
not change. Delivery must read this call site after making the Phase 5 change to confirm
it was not accidentally modified.

Expected state after fix:

```rust
// somewhere around line 784-790 — must remain exactly as-is:
coherence::generate_recommendations(
    report.coherence,
    coherence::DEFAULT_LAMBDA_THRESHOLD,
    report.graph_stale_ratio,
    report.embedding_inconsistencies.len(),
    report.total_quarantined,   // <-- MUST remain total_quarantined
)
```

---

## Phase Ordering Reference (do not change; confirm by reading)

Phase 2 (contradiction cache read, ~line 576):

```rust
// Phase 2: Contradiction scan — read from cache populated by background tick.
{
    let cached = self
        .contradiction_cache
        .read()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(ref result) = *cached {
        report.contradiction_count = result.pairs.len();   // <-- sets contradiction_count
        report.contradictions = result.pairs.clone();
        report.contradiction_scan_performed = true;
    }
    // If None (cold-start): contradiction_scan_performed stays false (default).
}
```

Phase 5 (Lambda computation, ~line 747) comes after Phase 2 in the sequential flow.
The comment added at Phase 5 makes this dependency explicit. No structural change is
needed to enforce ordering — the sequential code already guarantees it.

---

## Data Flow

```
ContradictionScanCacheHandle (Arc<RwLock<Option<ContradictionScanResult>>>)
    |
    | Phase 2 read (~line 583)
    | result.pairs.len() -> report.contradiction_count: usize
    |
    v
Phase 5 (~line 747)
    coherence::contradiction_density_score(
        report.contradiction_count,   // usize — first arg
        report.total_active,          // u64 — second arg (unchanged)
    )
    -> report.contradiction_density_score: f64
    |
    v
coherence::compute_lambda(
    report.graph_quality_score,
    embed_dim,
    report.contradiction_density_score,   // contradiction dimension
    &coherence::DEFAULT_WEIGHTS,
)
    -> report.coherence: f64

SEPARATE PATH (must not change):
Phase 5 (~line 784)
    coherence::generate_recommendations(
        report.coherence,
        coherence::DEFAULT_LAMBDA_THRESHOLD,
        report.graph_stale_ratio,
        report.embedding_inconsistencies.len(),
        report.total_quarantined,   // u64 — quarantine recs, not Lambda
    )
    -> report.maintenance_recommendations: Vec<String>
```

---

## Error Handling

`compute_report()` is an async method. The Phase 5 change is synchronous arithmetic —
it cannot fail. No error handling changes are required for this component.

The Phase 2 read uses `.unwrap_or_else(|e| e.into_inner())` for poisoned-lock recovery.
This is existing behavior and must not be altered.

---

## Key Test Scenarios

This component has no unit tests that directly assert `contradiction_density_score`
values. The integration surface is verified by:

1. Static read: confirm the Phase 5 call site passes `report.contradiction_count` (not
   `report.total_quarantined`) — AC-06, R-01.
2. Static read: confirm `report.total_quarantined` still appears at the
   `generate_recommendations()` call site (~line 784) — AC-08, R-06.
3. Static grep: `contradiction_density_score.*total_quarantined` returns zero matches
   anywhere in the workspace — AC-09, R-01.
4. Static read: phase-ordering comment is present at the Phase 5 call site — AC-16, R-04.
5. `cargo test --workspace` passes — AC-11. The type change (`u64` -> `usize`) is caught
   by the compiler if status.rs still passes `report.total_quarantined` (which is `u64`)
   to the new `usize` parameter. This serves as an automatic correctness guard.

| Scenario | Verification Method | AC/Risk |
|---|---|---|
| Phase 5 passes contradiction_count | Read lines ~747–748 | AC-06, R-01 |
| Phase 5 does not pass total_quarantined | Grep | AC-09, R-01 |
| generate_recommendations still has total_quarantined | Read lines ~784–790 | AC-08, R-06 |
| Phase ordering comment present | Read line ~747 | AC-16, R-04 |
| No compile error | cargo build | FM-01 |
