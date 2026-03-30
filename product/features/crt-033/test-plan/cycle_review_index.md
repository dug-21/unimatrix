# Component Test Plan: cycle_review_index

## Component Scope

`crates/unimatrix-store/src/cycle_review_index.rs` — new module.

Owns: `CycleReviewRecord` struct, `SUMMARY_SCHEMA_VERSION` const, `get_cycle_review`,
`store_cycle_review`, and `pending_cycle_reviews` on `SqlxStore`.

**AC coverage**: AC-03 (partial), AC-08 (partial), AC-09 (partial), AC-10 (partial),
AC-11, AC-16, AC-17.
**Risk coverage**: R-03, R-06, R-07 (partial), R-10, R-11, R-12, R-13.

---

## Unit Tests (in `#[cfg(test)]` block inside `cycle_review_index.rs`)

### CRS-U-01: CycleReviewRecord round-trip (AC-16)

```
fn test_cycle_review_record_serde_round_trip()

Arrange: construct a fully-populated CycleReviewRecord with
  feature_cycle = "crt-033",
  schema_version = SUMMARY_SCHEMA_VERSION,
  computed_at = 1_700_000_000,
  raw_signals_available = 1,
  summary_json = valid JSON string.

Act: serialize to JSON via serde_json::to_string, deserialize back via
  serde_json::from_str::<CycleReviewRecord>.

Assert: all fields equal original values.
```

### CRS-U-02: SUMMARY_SCHEMA_VERSION is 1 (R-13, AC-17)

```
fn test_summary_schema_version_is_one()

Assert: SUMMARY_SCHEMA_VERSION == 1u32.
```

Note: AC-17 requires a grep gate in addition to this assertion — the grep confirms
no second definition exists in `tools.rs` or `unimatrix-observe`.

### CRS-U-03: 4MB ceiling — over limit returns Err (R-11, NFR-03)

```
fn test_store_cycle_review_4mb_ceiling_exceeded()

Arrange: construct CycleReviewRecord with summary_json = "x".repeat(4 * 1024 * 1024 + 1)
  (4MB + 1 byte string).
Act: call store_cycle_review(&record) on a real SqlxStore (in-memory or tempfile).
Assert: returns Err (not Ok, not panic).
```

### CRS-U-04: 4MB ceiling — exactly at limit returns Ok (R-11, NFR-03)

```
fn test_store_cycle_review_4mb_ceiling_boundary()

Arrange: construct CycleReviewRecord with summary_json = "x".repeat(4 * 1024 * 1024)
  (exactly 4MB).
Act: call store_cycle_review(&record) on a real SqlxStore.
Assert: returns Ok(()).
```

### CRS-U-05: RetrospectiveReport serde round-trip (AC-16, R-06)

```
#[tokio::test]
async fn test_retrospective_report_serde_round_trip()

Arrange: construct a minimal but non-trivial RetrospectiveReport instance containing
  at least one HotspotFinding with at least two EvidenceRecords, and a non-None
  phase_narrative.

Act:
  let json = serde_json::to_string(&report).expect("serialize");
  let recovered = serde_json::from_str::<RetrospectiveReport>(&json).expect("deserialize");

Assert:
  recovered.feature_cycle == report.feature_cycle (or equivalent top-level field)
  recovered.hotspot_findings.len() == report.hotspot_findings.len()
  recovered.phase_narrative is_some == report.phase_narrative.is_some()
```

### CRS-U-06: Backward-compat deserialization of JSON with missing optional fields (R-06)

```
fn test_retrospective_report_serde_missing_optional_fields()

Arrange: a JSON string representing a RetrospectiveReport that omits all
  #[serde(default)] optional fields (simulate older stored record).

Act: serde_json::from_str::<RetrospectiveReport>(&json)

Assert: returns Ok (not Err). Missing fields are populated with defaults.
```

---

## Store Integration Tests (in `tests/migration_v17_to_v18.rs` or `tests/sqlite_parity_specialized.rs`)

### CRS-I-01: get_cycle_review returns None for missing feature_cycle (AC-04)

```
#[tokio::test]
async fn test_get_cycle_review_missing_returns_none()

Arrange: open fresh SqlxStore.
Act: get_cycle_review("nonexistent-cycle").await
Assert: returns Ok(None).
```

### CRS-I-02: store_cycle_review then get_cycle_review returns stored record (AC-03, AC-11)

```
#[tokio::test]
async fn test_store_and_get_cycle_review_round_trip()

Arrange: open fresh SqlxStore; construct CycleReviewRecord with
  schema_version = SUMMARY_SCHEMA_VERSION,
  raw_signals_available = 1.
Act: store_cycle_review(&record).await; get_cycle_review(&record.feature_cycle).await
Assert:
  - store returns Ok(())
  - get returns Ok(Some(record))
  - returned record.schema_version == SUMMARY_SCHEMA_VERSION (AC-11)
  - returned record.raw_signals_available == 1
  - returned record.summary_json == original summary_json
```

### CRS-I-03: INSERT OR REPLACE overwrites prior record (AC-05)

```
#[tokio::test]
async fn test_store_cycle_review_overwrites_prior()

Arrange: store initial record with computed_at = T1.
  Wait 1 second (or artificially bump computed_at).
  Store new record with same feature_cycle and computed_at = T2 > T1.
Act: get_cycle_review(feature_cycle).await
Assert: returned record.computed_at == T2 (the newer value overwrote T1).
```

### CRS-I-04: pending_cycle_reviews set-difference — happy path (AC-09, R-07)

```
#[tokio::test]
async fn test_pending_cycle_reviews_returns_unreviewed_cycles()

Arrange:
  Insert two cycle_events rows (event_type='cycle_start', timestamp within K-window):
    cycle_id = "feat-A", timestamp = now - 1_day
    cycle_id = "feat-B", timestamp = now - 2_days
  Store a cycle_review_index row for "feat-A" only.

Act: pending_cycle_reviews(k_window_cutoff = now - 90_days).await

Assert: returns Ok(vec!["feat-B"]). "feat-A" is excluded because it has a review row.
```

### CRS-I-05: pending_cycle_reviews empty when all have review rows (AC-10)

```
#[tokio::test]
async fn test_pending_cycle_reviews_empty_when_all_reviewed()

Arrange: same as CRS-I-04 but store review rows for both "feat-A" and "feat-B".
Act: pending_cycle_reviews(k_window_cutoff).await
Assert: returns Ok(vec![]).
```

### CRS-I-06: pending_cycle_reviews excludes cycles outside K-window (R-07)

```
#[tokio::test]
async fn test_pending_cycle_reviews_excludes_outside_k_window()

Arrange:
  Insert cycle_events row for "old-cycle" with timestamp = now - 91_days (outside 90-day window).
  No review row for "old-cycle".

Act: pending_cycle_reviews(k_window_cutoff = now - 90_days).await
Assert: returns Ok(vec![]) — "old-cycle" not included.
```

### CRS-I-07: pending_cycle_reviews excludes cycles with only cycle_end events (R-07)

```
#[tokio::test]
async fn test_pending_cycle_reviews_excludes_cycle_end_only()

Arrange:
  Insert cycle_events row for "end-only-cycle" with event_type='cycle_end', within K-window.
  No review row.

Act: pending_cycle_reviews(k_window_cutoff).await
Assert: returns Ok(vec![]) — only cycle_start events qualify.
```

### CRS-I-08: pending_cycle_reviews K-window boundary is inclusive (R-07)

```
#[tokio::test]
async fn test_pending_cycle_reviews_boundary_is_inclusive()

Arrange:
  Insert cycle_events row for "boundary-cycle" with timestamp = exactly k_window_cutoff.
  No review row.

Act: pending_cycle_reviews(k_window_cutoff = boundary_cycle.timestamp).await
Assert: returns Ok(vec!["boundary-cycle"]) — timestamp >= cutoff is inclusive.
```

### CRS-I-09: pending_cycle_reviews DISTINCT — multiple cycle_start events for same cycle (R-07)

```
#[tokio::test]
async fn test_pending_cycle_reviews_distinct_on_cycle_id()

Arrange:
  Insert three cycle_events rows for "dup-cycle" all with event_type='cycle_start',
  timestamps within K-window.
  No review row.

Act: pending_cycle_reviews(k_window_cutoff).await
Assert: "dup-cycle" appears exactly once in the result (DISTINCT applies to cycle_id).
```

### CRS-I-10: concurrent store_cycle_review for same cycle — last writer wins (R-10)

```
#[tokio::test]
async fn test_concurrent_store_same_cycle_last_writer_wins()

Arrange: open fresh SqlxStore.
Act: tokio::join! two tasks each calling store_cycle_review with same feature_cycle
  but different computed_at values.
Assert:
  - both store calls complete Ok (no error, no panic)
  - get_cycle_review returns exactly one row (not zero, not two separate rows)
```

---

## Static / Grep Checks (CI gate — not Rust tests, run by Stage 3c tester)

### CRS-G-01: SUMMARY_SCHEMA_VERSION defined only in cycle_review_index.rs (AC-17, R-13)

```
grep -r 'SUMMARY_SCHEMA_VERSION\s*=\s*[0-9]' crates/
```
Assert: exactly one match, in `crates/unimatrix-store/src/cycle_review_index.rs`.

```
grep -r 'SUMMARY_SCHEMA_VERSION' crates/unimatrix-server/
```
Assert: zero matches containing a numeric literal (uses only definitions, not re-defines).

### CRS-G-02: pending_cycle_reviews and get_cycle_review use read_pool (R-12)

```
grep -n 'write_pool_server\|read_pool' crates/unimatrix-store/src/cycle_review_index.rs
```
Assert:
- `pending_cycle_reviews` implementation calls `read_pool()`.
- `get_cycle_review` implementation calls `read_pool()`.
- Only `store_cycle_review` calls `write_pool_server()`.

---

## Edge Cases

| Edge Case | Test | Expected |
|-----------|------|---------|
| `feature_cycle` = empty string | CRS-I-01 variant | `get_cycle_review("")` returns Ok(None) |
| `summary_json` = empty string (invalid JSON) | CRS-U-06 variant | `serde_json::from_str("")` returns Err; does not panic |
| `raw_signals_available` = 0 stored, read back | CRS-I-02 variant with flag=0 | Returns `raw_signals_available = 0` — sqlx INTEGER→i32 binding verified |
| pre-cycle_events cycle (observation_metrics only, no cycle_events row) | CRS-I-04 with such a cycle | Not returned by `pending_cycle_reviews` |
