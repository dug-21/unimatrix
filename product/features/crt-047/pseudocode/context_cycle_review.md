# crt-047: Pseudocode — context_cycle_review handler extension

## Purpose

Extend Step 8a of `context_cycle_review` in `tools.rs` to:
1. Compute `CurationSnapshot` from ENTRIES before writing to `cycle_review_index`.
2. Pass snapshot fields into `store_cycle_review()` via the updated `CycleReviewRecord`.
3. Compute `CurationBaselineComparison` after store.
4. Attach `curation_health: Option<CurationHealthBlock>` to `RetrospectiveReport`.

Also update `build_cycle_review_record()` to populate the seven new `CycleReviewRecord`
fields from the snapshot.

File: `crates/unimatrix-server/src/mcp/tools.rs`
Related: `crates/unimatrix-server/src/mcp/response/cycle_review.rs`

---

## Context: Existing Step 8a

The current Step 8a (around line 2288-2315 in tools.rs) is:

```
// Step 8a (crt-033): Serialize and store the computed review.
match build_cycle_review_record(&feature_cycle, &report) {
    Ok(record) => {
        if let Err(e) = store.store_cycle_review(&record).await {
            tracing::warn!("crt-033: store_cycle_review failed for {}: {} — continuing", ...);
        }
    }
    Err(e) => {
        tracing::warn!("crt-033: build_cycle_review_record serialization failed for {}: {} — continuing", ...);
    }
}
```

The crt-047 extension inserts before this block and updates `build_cycle_review_record`.

---

## Step 8a Extension (new pseudocode)

The following block is inserted BEFORE the existing `build_cycle_review_record` call.
All of this is inside the `full pipeline block (memo_hit.is_none())` guard.

```
// -------------------------------------------------------------------
// Step 8a-crt-047: Compute curation snapshot BEFORE store_cycle_review.
// Read must complete before write (I-01: read → compute → write order).
// Uses read_pool() only; write happens inside store_cycle_review.
// -------------------------------------------------------------------

// Derive cycle_start_ts from the cycle_events data already read in the handler.
// The handler reads cycle_events via get_cycle_start_goal earlier in the pipeline.
// extract_cycle_start_ts() is a local helper that reads the minimum timestamp
// from the cycle_events vec for event_type = 'cycle_start'.
cycle_start_ts: i64 = extract_cycle_start_ts(cycle_events_vec.as_deref())
    // Returns 0 if no cycle_start event found (EC-02: open window, over-counts orphans).
    // Caller must log a warning when this happens (FM-05).

if cycle_start_ts == 0:
    tracing::warn!(
        "crt-047: no cycle_start event found for {} — orphan_deprecations window is [0, now], \
         over-counting risk (EC-02)",
        feature_cycle
    )

review_ts: i64 = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs() as i64

// Compute snapshot via ENTRIES queries. Non-fatal: if this fails, curation_health
// is absent from the response but the rest of context_cycle_review continues.
curation_snapshot: Option<CurationSnapshot> =
    match services::curation_health::compute_curation_snapshot(
        &store,
        &feature_cycle,
        cycle_start_ts,
        review_ts,
    ).await {
        Ok(snapshot) => Some(snapshot),
        Err(e) => {
            tracing::warn!(
                "crt-047: compute_curation_snapshot failed for {}: {} — \
                 curation_health will be absent from response",
                feature_cycle, e
            );
            None
        }
    }
```

### extract_cycle_start_ts (local helper, private to this module)

```
fn extract_cycle_start_ts(cycle_events: Option<&[CycleEventRow]>) -> i64
    // CycleEventRow is whatever type the handler uses to hold cycle_events rows.
    // (Look up the actual type from the existing get_cycle_start_goal query result.)

ALGORITHM:
  events = match cycle_events:
    None => return 0
    Some(e) => e

  cycle_start_ts = events.iter()
    .filter(|e| e.event_type == "cycle_start")
    .map(|e| e.timestamp)
    .min()
    .unwrap_or(0)

  cycle_start_ts

NOTE: The handler already queries cycle_events. Reuse the existing vec rather than
issuing another DB query. If the handler does not preserve the raw cycle_events rows,
add a single query here:
  "SELECT MIN(timestamp) FROM cycle_events WHERE cycle_id = ?1 AND event_type = 'cycle_start'"
This is acceptable as a single scalar query using read_pool().
```

---

## Updated build_cycle_review_record

The existing `build_cycle_review_record` helper at lines 2624-2638 populates
`CycleReviewRecord`. Extend it to accept and populate the seven new fields.

```
fn build_cycle_review_record(
    feature_cycle: &str,
    report: &unimatrix_observe::RetrospectiveReport,
    snapshot: Option<&CurationSnapshot>,
    first_computed_at: i64,   // passed by caller: cycle_start_ts on first write
) -> Result<unimatrix_store::CycleReviewRecord, serde_json::Error>

ALGORITHM:
  summary_json = serde_json::to_string(report)?

  computed_at = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs() as i64

  // Default all seven new fields to 0 when snapshot is unavailable.
  // store_cycle_review() handles the two-step upsert for first_computed_at.
  (ct, ca, ch, cs, dt, od) = match snapshot:
    None => (0, 0, 0, 0, 0, 0)
    Some(s) => (
      s.corrections_total as i64,
      s.corrections_agent as i64,
      s.corrections_human as i64,
      s.corrections_system as i64,
      s.deprecations_total as i64,
      s.orphan_deprecations as i64,
    )

  Ok(unimatrix_store::CycleReviewRecord {
    feature_cycle: feature_cycle.to_string(),
    schema_version: unimatrix_store::SUMMARY_SCHEMA_VERSION,  // now = 2
    computed_at,
    raw_signals_available: 1,
    summary_json,
    corrections_total:    ct,
    corrections_agent:    ca,
    corrections_human:    ch,
    corrections_system:   cs,
    deprecations_total:   dt,
    orphan_deprecations:  od,
    first_computed_at,    // set by caller; store_cycle_review preserves it on update
  })
```

---

## Updated Store + Baseline Call Site

Replace the existing Step 8a `build_cycle_review_record` call with the following:

```
// Step 8a (crt-033 + crt-047): Build record including snapshot columns, then store.
// first_computed_at = cycle_start_ts for new rows (fallback: computed_at / now).
// store_cycle_review() two-step upsert preserves it on subsequent writes (ADR-001).
first_computed_at: i64 = if cycle_start_ts > 0 { cycle_start_ts } else { review_ts }
    // NOTE: When cycle_start_ts = 0 (no event), fall back to review_ts.
    // This is the only write that sets first_computed_at; store_cycle_review UPDATE
    // does not touch first_computed_at (ADR-001).

match build_cycle_review_record(
    &feature_cycle,
    &report,
    curation_snapshot.as_ref(),
    first_computed_at,
) {
    Ok(record) => {
        if let Err(e) = store.store_cycle_review(&record).await {
            tracing::warn!("crt-033: store_cycle_review failed for {}: {} — continuing", feature_cycle, e);
        }
    }
    Err(e) => {
        tracing::warn!("crt-033: build_cycle_review_record serialization failed for {}: {} — continuing", feature_cycle, e);
    }
}

// Step 8a-post (crt-047): Compute baseline comparison AFTER store.
// Reads the updated window from cycle_review_index (including the just-written row,
// but that row has first_computed_at = cycle_start_ts, not 0, so it appears in the window).
// This is intentional: the current cycle is excluded from its own baseline comparison
// because the window reads BEFORE the store only if snapshot = None (no write occurred).
// When snapshot is Some and store succeeded, the current cycle IS in the window —
// callers should interpret history_cycles including the current row.
//
// Alternative (stricter): read baseline BEFORE store (pre-store window).
// The architecture diagram shows baseline read AFTER store. Accept this behavior.
curation_health_block: Option<CurationHealthBlock> = match &curation_snapshot {
    None => None,
    Some(snapshot) => {
        let baseline_rows = store.get_curation_baseline_window(
            services::curation_health::CURATION_MIN_HISTORY + // enough to check threshold
            // Use a small window here; status uses the full CURATION_BASELINE_WINDOW.
            // For cycle_review, use the same CURATION_BASELINE_WINDOW constant.
            // Import from services::status or define locally; match status.rs constant.
            CURATION_BASELINE_WINDOW_FOR_REVIEW  // = 10; defined as constant near call site
        ).await.unwrap_or_default();

        let baseline_opt = services::curation_health::compute_curation_baseline(
            &baseline_rows,
            CURATION_BASELINE_WINDOW_FOR_REVIEW,
        );

        let comparison_opt = baseline_opt.map(|baseline| {
            services::curation_health::compare_to_baseline(
                snapshot,
                &baseline,
                baseline.history_cycles,
            )
        });

        Some(CurationHealthBlock {
            snapshot: snapshot.clone(),
            baseline: comparison_opt,
        })
    }
}
```

---

## CurationHealthBlock Constant

Define near the call site in tools.rs (or import from curation_health.rs):

```
// Window size for context_cycle_review baseline.
// Matches CURATION_BASELINE_WINDOW in status.rs (both = 10).
const CURATION_BASELINE_WINDOW_FOR_REVIEW: usize = 10;
```

---

## RetrospectiveReport Extension

Add `curation_health: Option<CurationHealthBlock>` to `RetrospectiveReport` in
`mcp/response/cycle_review.rs`:

```
// In RetrospectiveReport struct (or wherever it is defined):
pub curation_health: Option<CurationHealthBlock>,
    // None when:
    //   - compute_curation_snapshot() failed (ServiceError)
    //   - force=false and memoization hit returns cached report without recomputing
    // Some(block) on every full-pipeline call.
    // block.baseline = None when < CURATION_MIN_HISTORY qualifying prior cycles.
```

Attach the block to the report before step 8b:

```
// After computing curation_health_block (above), attach to report.
report.curation_health = curation_health_block;
```

---

## Memoization Hit Path

On `force=false` cache hit (memo_hit is Some), the cached `RetrospectiveReport`
is returned. The `curation_health` field will be whatever was stored in `summary_json`
at the time of the original compute. This is correct:
- If the original compute succeeded, `curation_health` is Some in the cached report.
- If the original compute failed, `curation_health` is None in the cached report.

The stale-record advisory (schema_version = 1) applies: a cached record with
schema_version = 1 will have `curation_health = None` (it predates crt-047).
The advisory text already informs the caller to use `force=true`.

No change to the memoization path logic is needed.

---

## Data Flow

```
Input:
  feature_cycle: &str       (from handler params)
  cycle_events_vec: ...     (already read by handler)

Derived:
  cycle_start_ts: i64       (from extract_cycle_start_ts)
  review_ts: i64            (= now())
  first_computed_at: i64    (= cycle_start_ts if > 0, else review_ts)

Async reads (read_pool):
  compute_curation_snapshot(store, feature_cycle, cycle_start_ts, review_ts)
    → CurationSnapshot

Async write (write_pool_server):
  store.store_cycle_review(&record)
    -- record includes all 7 snapshot fields

Async read (read_pool, post-store):
  store.get_curation_baseline_window(CURATION_BASELINE_WINDOW_FOR_REVIEW)
    → Vec<CurationBaselineRow>

Pure:
  compute_curation_baseline(rows, n)  → Option<CurationBaseline>
  compare_to_baseline(snapshot, baseline, count)  → CurationBaselineComparison

Output:
  report.curation_health = Some(CurationHealthBlock { snapshot, baseline: Option<...> })
```

---

## Error Handling

| Step | Failure | Behavior |
|------|---------|----------|
| `compute_curation_snapshot` fails | `ServiceError` | Log warning; `curation_snapshot = None`; `curation_health = None` |
| `store_cycle_review` fails | `StoreError` | Log warning; continue (existing behavior) |
| `get_curation_baseline_window` fails | `StoreError` | `.unwrap_or_default()` → empty vec; `baseline = None` |
| `compute_curation_baseline` returns None | — | `baseline = None`; cold-start behavior |
| `build_cycle_review_record` fails | `serde_json::Error` | Log warning; continue (existing behavior) |

No failure path returns an error to the caller of `context_cycle_review`.

---

## Key Test Scenarios

**T-CCR-01 (AC-06)**: Fresh database — `curation_health` is Some with raw snapshot, no sigma.
- Call `context_cycle_review` on a fresh DB (no prior cycle_review_index rows).
- Assert: `curation_health` is present; `baseline` is None (< MIN_HISTORY cycles).

**T-CCR-02 (AC-07, R-11)**: 3 prior qualifying rows — sigma comparison present.
- Seed 3 prior rows with `first_computed_at > 0` and `schema_version = 2`.
- Call `context_cycle_review`.
- Assert: `curation_health.baseline` is Some; `history_cycles = 3`.

**T-CCR-03 (AC-08, R-11)**: 2 prior qualifying rows — sigma absent.
- Seed 2 prior rows.
- Assert: `curation_health.baseline` is None.

**T-CCR-04 (AC-11, AC-12)**: Stale schema_version = 1 → advisory on force=false, no recompute.
- Store a cached record with schema_version = 1.
- Call `context_cycle_review force=false`.
- Assert: advisory string present; snapshot columns unchanged in DB.

**T-CCR-05 (AC-R01)**: force=true on historical cycle preserves `first_computed_at`.
- Store cycle with known `first_computed_at`.
- Call `context_cycle_review force=true` for same cycle.
- Assert: `first_computed_at` in DB is unchanged.

**T-CCR-06 (EC-02)**: No `cycle_start` event — fallback to cycle_start_ts = 0.
- Cycle with no cycle_start event in cycle_events.
- Assert: no panic; warning logged; `curation_health` populated (possibly over-counted).

**T-CCR-07 (I-01)**: Correct ordering — snapshot read before write.
- Verified via code inspection: `compute_curation_snapshot` call precedes `store_cycle_review` call.

**T-CCR-08 (AC-05)**: All snapshot columns written atomically with the review record.
- Store a cycle review; retrieve the row.
- Assert: all seven new columns match the CurationSnapshot values.
