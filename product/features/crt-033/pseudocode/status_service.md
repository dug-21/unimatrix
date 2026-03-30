# Pseudocode: services/status.rs Phase 7b

## Purpose

Add Phase 7b to `compute_report()` in
`crates/unimatrix-server/src/services/status.rs`.

Phase 7b queries `pending_cycle_reviews(k_window_cutoff)` and populates
`report.pending_cycle_reviews`. It executes unconditionally — no opt-in
parameter (C-07, FR-09).

---

## New Constant

Add near the existing `MINIMUM_VOTED_POPULATION` and `PRE_CRT019_SPREAD_BASELINE`
constants at the top of the file:

```
/// K-window for pending cycle review detection (crt-033, ADR-004).
///
/// Cycles with a cycle_start event older than this window are excluded from
/// pending_cycle_reviews. Default: 90 days = 7_776_000 seconds.
///
/// Must be reconciled with GH #409's retention window constant when that feature
/// merges. If #409 exposes a pub const, import it; otherwise update this value
/// and add a comment referencing the #409 constant.
///
/// Not inlined at the call site (C-11, NFR-05).
pub(crate) const PENDING_REVIEWS_K_WINDOW_SECS: i64 = 90 * 24 * 3600; // 7_776_000
```

---

## Modified: compute_report()

### Placement

Phase 7b inserts AFTER Phase 7 (retrospected feature count) and BEFORE Phase 8
(effectiveness analysis). In the existing code this is after:

```
// Phase 7: Retrospected feature count
let retrospected = self.store.list_all_metrics().await.unwrap_or_else(|e| { ... });
report.retrospected_feature_count = retrospected.len() as u64;
```

### Phase 7b Block

```
// Phase 7b: Pending cycle reviews (crt-033).
//
// Set-difference query: cycles with cycle_start events in K-window
// but no cycle_review_index row.
// Uses read_pool() — never write_pool_server() (ADR-004, entry #3619).
// Always computed — no opt-in parameter (C-07, FR-09).
// On query failure: degrade gracefully with empty vec; do NOT fail compute_report().
{
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64

    let k_window_cutoff = now_secs - PENDING_REVIEWS_K_WINDOW_SECS

    match self.store.pending_cycle_reviews(k_window_cutoff).await {
        Ok(pending) => {
            report.pending_cycle_reviews = pending
        }
        Err(e) => {
            // Graceful degradation: log and leave pending_cycle_reviews as the
            // default empty vec. context_status must not fail because of Phase 7b.
            tracing::error!(
                "crt-033: pending_cycle_reviews query failed: {} — \
                 pending_cycle_reviews will be empty in this response",
                e
            )
            // report.pending_cycle_reviews remains Vec::new() (set by StatusReport initializer)
        }
    }
}
```

### Import Required

The store method `pending_cycle_reviews` is defined on `SqlxStore` in
`cycle_review_index.rs`. The `self.store` field in `StatusService` is an
`Arc<SqlxStore>` (or `Arc<dyn Store>` — check the actual type). If `SqlxStore`
is used directly (most likely, given the pattern in status.rs), the method is
available without additional imports.

The constant `PENDING_REVIEWS_K_WINDOW_SECS` is defined in this file, so no import.

`SystemTime` and `UNIX_EPOCH` are already imported at the top of `services/status.rs`
(they are used in Phase 7 and elsewhere). Verify before adding a duplicate import.

---

## State Machine / Initialization

`compute_report()` is the sole call site for Phase 7b. There is no persistent
state involved — the K-window cutoff is computed from `now()` on each call.
`PENDING_REVIEWS_K_WINDOW_SECS` is a compile-time constant, not configurable
at runtime.

---

## Data Flow

```
services/status.rs compute_report()
    │
    ├─ Phase 7: retrospected_feature_count (UNCHANGED)
    │
    ├─ Phase 7b (NEW):
    │   now_secs = SystemTime::now().as_secs() as i64
    │   k_window_cutoff = now_secs - PENDING_REVIEWS_K_WINDOW_SECS
    │   │
    │   ├─ self.store.pending_cycle_reviews(k_window_cutoff)
    │   │       → calls cycle_review_index.rs pending_cycle_reviews(k_window_cutoff)
    │   │       → SQL: DISTINCT cycle_id WHERE cycle_start >= cutoff
    │   │              AND NOT IN cycle_review_index
    │   │       → read_pool() (never write_pool_server)
    │   │
    │   ├─ Ok(pending) → report.pending_cycle_reviews = pending
    │   └─ Err(e) → log error, report.pending_cycle_reviews stays []
    │
    └─ Phase 8: effectiveness analysis (UNCHANGED)
```

---

## Error Handling

| Scenario | Response |
|----------|----------|
| `pending_cycle_reviews` SQL success | `report.pending_cycle_reviews` populated |
| `pending_cycle_reviews` SQL error | Log at `error!`, field stays `vec![]` (graceful degradation) |
| `pending_cycle_reviews` returns empty vec | Field = `vec![]` (FR-10 empty case) |
| Clock skew (now() returns unexpected value) | K-window cutoff may be wrong; no panic — graceful |

---

## Key Test Scenarios

1. Two cycles in `cycle_events` (both within K-window); one has `cycle_review_index` row.
   `compute_report()` → `pending_cycle_reviews` contains exactly the un-reviewed cycle (AC-09).

2. Both cycles have `cycle_review_index` rows.
   `compute_report()` → `pending_cycle_reviews` is empty (AC-10, FR-10).

3. A cycle's `cycle_start` timestamp is before the K-window cutoff.
   It does NOT appear in `pending_cycle_reviews` (R-07 scenario 4).

4. A cycle has only `cycle_end` events, no `cycle_start`.
   It does NOT appear in `pending_cycle_reviews` (R-07 scenario 3).

5. Pre-cycle_events cycle (row in `observation_metrics` only, no `cycle_events` row).
   Does NOT appear in `pending_cycle_reviews` (FR-09 pre-cycle_events exclusion).

6. `pending_cycle_reviews` query fails (injected DB error).
   `compute_report()` completes successfully; `pending_cycle_reviews` = `[]` (failure mode table).

7. `PENDING_REVIEWS_K_WINDOW_SECS` is a named constant, not a magic number (C-11, NFR-05).
   Static verification: the literal `7_776_000` (or `90 * 24 * 3600`) appears only in the
   constant definition, not as an inline value in Phase 7b.

8. Phase 7b uses `read_pool()` (via `self.store.pending_cycle_reviews`).
   Static grep check: no `write_pool_server` call within `pending_cycle_reviews` impl (R-12).
