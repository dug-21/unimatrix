# Test Plan: store-analytics (unimatrix-store/src/analytics.rs)

Covers: `AnalyticsWrite::GraphEdge` variant; `variant_name()` method; drain task arm
with `INSERT OR IGNORE INTO graph_edges`; `weight.is_finite()` guard; idempotent insert.

Risks addressed: R-07, R-13 (indirect), AC-09, AC-17

---

## Unit Tests

### `test_analytics_write_graph_edge_variant_name` (AC-09)
- Arrange: construct `AnalyticsWrite::GraphEdge { source_id: 1, target_id: 2,
  relation_type: "Supersedes".to_string(), weight: 1.0, created_by: "test".to_string(),
  source: "test".to_string(), bootstrap_only: false }`
- Act: call `variant_name()` on the constructed variant
- Assert: returns `"GraphEdge"`

### `test_analytics_write_non_exhaustive_contract_preserved`
- Arrange: match on an `AnalyticsWrite` value with a wildcard arm `_ => {}`
- Assert: compiles without `non_exhaustive_omitted_patterns` warning in catch-all position
  (validates the `#[non_exhaustive]` contract is not broken by adding `GraphEdge`)

---

## Weight Validation Unit Tests (R-07, AC-17, AC-03)

These tests target the weight validation guard — the function or inline check that calls
`weight.is_finite()` before enqueuing a `GraphEdge` event.

### `test_weight_guard_rejects_nan`
- Act: pass `f32::NAN` to the weight validation function
- Assert: returns `Err` (or the equivalent rejection path) — the event is not enqueued

### `test_weight_guard_rejects_positive_infinity`
- Act: pass `f32::INFINITY`
- Assert: rejected with error log

### `test_weight_guard_rejects_negative_infinity`
- Act: pass `f32::NEG_INFINITY`
- Assert: rejected with error log

### `test_weight_guard_accepts_zero`
- Act: pass `0.0_f32`
- Assert: passes validation (0.0 is finite)

### `test_weight_guard_accepts_half`
- Act: pass `0.5_f32`
- Assert: passes validation

### `test_weight_guard_accepts_one`
- Act: pass `1.0_f32`
- Assert: passes validation

### `test_weight_guard_accepts_f32_max`
- Act: pass `f32::MAX`
- Assert: passes validation (f32::MAX is finite)

---

## Integration Tests (Drain Task)

All drain tests require an async tokio runtime. Use `#[tokio::test]`.
Use `SqlxStore::open(path, PoolConfig::test_default())` with `open_test_store` helper.

### `test_analytics_graph_edge_drain_inserts_row` (AC-09)
- Arrange: open fresh store; enqueue `AnalyticsWrite::GraphEdge` with valid weight=1.0
- Act: wait for drain task to flush (or call drain directly if exposed for testing)
- Act: query `SELECT * FROM graph_edges WHERE source_id=? AND target_id=?`
- Assert: exactly one row present
- Assert: all fields match the enqueued values

### `test_analytics_graph_edge_drain_rejects_nan_weight` (R-07, AC-17)
- Arrange: open fresh store; attempt to enqueue `AnalyticsWrite::GraphEdge` with `weight=f32::NAN`
- Act: drain
- Assert: zero rows in `graph_edges`
- Assert: an ERROR-level log message was produced (tracing assertion or verified via test output)

### `test_analytics_graph_edge_drain_idempotent_insert_or_ignore` (R-08, AC-09)
- Arrange: enqueue the same `GraphEdge` event twice (same `source_id`, `target_id`, `relation_type`)
- Act: drain both
- Assert: exactly one row in `graph_edges` (second INSERT OR IGNORE is a no-op)
- Assert: no error from the drain task

### `test_analytics_graph_edge_bootstrap_only_field_persisted`
- Arrange: enqueue `GraphEdge` with `bootstrap_only=true`
- Act: drain
- Assert: row in `graph_edges` has `bootstrap_only=1`

### `test_analytics_graph_edge_bootstrap_only_false_persisted`
- Arrange: enqueue `GraphEdge` with `bootstrap_only=false`
- Act: drain
- Assert: row in `graph_edges` has `bootstrap_only=0`

### `test_analytics_graph_edge_metadata_column_is_null`
- Arrange: enqueue any valid `GraphEdge` event (no metadata field in variant)
- Act: drain
- Assert: `metadata IS NULL` for the inserted row (crt-021 writes no metadata)

---

## Code Inspection Gate (R-13)

### `inspect_migration_bootstrap_bypasses_analytics_queue`
- Grep assertion: confirm that `migration.rs` contains zero calls to any analytics write
  enqueue function (e.g., `send`, `try_send`, `AnalyticsWrite::GraphEdge`)
- Assert: bootstrap migration inserts use raw `sqlx::query` calls, not the analytics queue
- This eliminates the R-13 shed risk for the migration bootstrap path

---

## Test Module Location

Unit tests for `variant_name()` and weight guard: `crates/unimatrix-store/src/analytics.rs`
`#[cfg(test)]` module.

Drain integration tests: `crates/unimatrix-store/src/analytics.rs` `#[cfg(test)]` module
or a dedicated `analytics_integration_test.rs` file if the drain task is not directly
accessible from the module's test scope.

If the drain task requires spawning the analytics write loop, use a test-only exposed
`drain_pending_for_test` function or call the drain arm directly via a test helper that
provides a `&SqlitePool` write handle.
