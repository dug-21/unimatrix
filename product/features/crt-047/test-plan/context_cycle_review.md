# Test Plan: `context_cycle_review` Handler

Component: `crates/unimatrix-server/src/mcp/tools.rs` (Step 8a extension)
Supporting files: `mcp/response/cycle_review.rs`, `services/curation_health.rs`
Risk coverage: R-01, R-07, R-08, R-12, I-01, I-02, I-03, I-04, AC-02, AC-05–AC-08, AC-11, AC-12, AC-13

---

## What Is Under Test

The `context_cycle_review` MCP tool handler gains Step 8a:
1. Call `compute_curation_snapshot()` (read from ENTRIES via `read_pool()`).
2. Pass snapshot into the updated `store_cycle_review()` (write via `write_pool_server()`).
3. Read baseline window via `get_curation_baseline_window()`.
4. Call `compute_curation_baseline()` and `compare_to_baseline()` if `MIN_HISTORY` met.
5. Populate `RetrospectiveReport.curation_health` (`CurationHealthBlock`).
6. On `force=false` with `schema_version = 1`: return advisory, no recompute.

---

## Unit Tests (Handler-Level)

These tests call the handler indirectly through a test `SqlxStore` rather than through
the full MCP binary, using the same test infrastructure as `server.rs` tests.

### CCR-U-01: `curation_health` block present in cold-start (AC-06, EC-01)

```
test_context_cycle_review_curation_health_present_on_cold_start
```

- Arrange: fresh DB, one feature cycle with no corrections and no deprecations.
  No prior `cycle_review_index` rows.
- Act: call handler with this cycle (force=false, fresh call → computes new).
- Assert: `response.curation_health.is_some()`.
- Assert: `curation_health.snapshot.corrections_total == 0`.
- Assert: `curation_health.snapshot.deprecations_total == 0`.
- Assert: `curation_health.baseline.is_none()` (cold start — fewer than 3 prior rows).

### CCR-U-02: `curation_health.baseline` absent when 2 prior rows (AC-08, R-11)

```
test_context_cycle_review_baseline_absent_with_two_prior_rows
```

- Arrange: insert 2 `cycle_review_index` rows with `first_computed_at > 0` and
  `schema_version = 2` (real snapshot data).
- Act: call handler for a new cycle.
- Assert: `curation_health.baseline.is_none()`.
- Assert: no error returned.

### CCR-U-03: `curation_health.baseline` present when 3+ prior rows with σ annotation (AC-07)

```
test_context_cycle_review_baseline_present_with_three_prior_rows
```

- Arrange: insert 3 `cycle_review_index` rows with `first_computed_at > 0`,
  `schema_version = 2`, non-zero `corrections_total`.
- Act: call handler for a new cycle with non-zero corrections.
- Assert: `curation_health.baseline.is_some()`.
- Assert: `curation_health.baseline.history_cycles == 3`.
- Assert: σ values are finite (not NaN, not infinite).

### CCR-U-04: `force=false` with `schema_version = 1` returns advisory (AC-11, R-12)

```
test_context_cycle_review_advisory_on_stale_schema_version
```

- Arrange: store a `CycleReviewRecord` with `schema_version = 1` for the target cycle.
- Act: call handler with `force=false`.
- Assert: response text contains the advisory string:
  `"computed with schema_version 1, current is 2 — use force=true to recompute"`.
- Assert: response is otherwise valid (cached report returned alongside advisory).

### CCR-U-05: `force=false` with `schema_version = 1` does NOT recompute snapshot (AC-12, R-12)

```
test_context_cycle_review_force_false_no_silent_recompute
```

- Arrange: store record with `schema_version = 1`, `corrections_total = 0` (stale zeros).
- Act: call handler with `force=false`.
- After call: retrieve row from `cycle_review_index`.
- Assert: `row.schema_version == 1` (not updated to 2).
- Assert: `row.corrections_total == 0` (not recomputed).
- Negative assertion: no curation snapshot was written by the force=false path.

### CCR-U-06: `force=true` on stale record updates schema_version to 2 (AC-12 positive path)

```
test_context_cycle_review_force_true_updates_stale_record
```

- Arrange: store record with `schema_version = 1`.
- Act: call handler with `force=true`.
- Assert: retrieved row has `schema_version == 2`.
- Assert: no advisory in response (fresh computation).

### CCR-U-07: Step ordering — read before write (I-01)

```
test_context_cycle_review_snapshot_computed_before_store
```

This is verified structurally via code review (grep check) rather than runtime test:

```bash
# Verify compute_curation_snapshot() precedes store_cycle_review() in the handler
grep -n "compute_curation_snapshot\|store_cycle_review" \
    crates/unimatrix-server/src/mcp/tools.rs
```

The line number of `compute_curation_snapshot` must be LESS THAN the line number
of `store_cycle_review` in the `context_cycle_review` handler.

### CCR-U-08: Pool discipline verified (AC-13, I-02)

```bash
# AC-13 grep check:
grep -n "write_pool_server\|read_pool" \
    crates/unimatrix-server/src/services/curation_health.rs \
    crates/unimatrix-store/src/cycle_review_index.rs
```

Expected:
- `compute_curation_snapshot()` in `curation_health.rs`: uses `read_pool()` only.
- `store_cycle_review()` in `cycle_review_index.rs`: uses `write_pool_server()` only.
- No `spawn_blocking` wrapping either function at the `context_cycle_review` call site.

### CCR-U-09: Cycle with no `cycle_start` event does not panic (EC-02, I-03)

```
test_context_cycle_review_no_cycle_start_event_does_not_panic
```

- Arrange: create a cycle_id with no `cycle_start` event in `cycle_events`.
- Act: call handler.
- Assert: returns `Ok(response)` — no panic.
- Assert: snapshot is present in response (may be over-counted, which is documented).
- Assert: response contains a warning annotation about missing cycle start timestamp.

### CCR-U-10: `CycleReviewRecord` field addition does not break existing store path (I-04)

Verified structurally: confirm only one call site for `store_cycle_review()` in
`tools.rs`:

```bash
grep -n "store_cycle_review" crates/unimatrix-server/src/mcp/tools.rs
```

Must return exactly one match. If more than one call site exists, the additional call
sites must also be updated to pass the snapshot fields.

---

## Integration Tests (MCP-Level via `infra-001`)

These tests exercise the handler through the full MCP JSON-RPC binary. They belong
in `suites/test_lifecycle.py` using `server` fixture (fresh DB) or `shared_server`
(state accumulation).

### CCR-I-01: `curation_health.snapshot` present in response (AC-06)

```python
def test_cycle_review_curation_health_cold_start(server):
    # Arrange: call context_cycle with a fresh cycle
    # Act: call context_cycle_review for that cycle
    # Assert: response contains curation_health block
    # Assert: curation_health.snapshot present with corrections_total >= 0
    # Assert: curation_health.baseline absent (cold start)
```

### CCR-I-02: σ baseline present after 3+ cycles (AC-07)

```python
def test_cycle_review_curation_health_with_baseline(shared_server):
    # Arrange: complete 3 cycles via context_cycle + context_cycle_review
    # Act: call context_cycle_review for a 4th cycle
    # Assert: curation_health.baseline is present
    # Assert: history_cycles annotation >= 3
```

### CCR-I-03: Advisory returned for stale schema_version (AC-11)

```python
def test_cycle_review_advisory_on_schema_version_1(server):
    # Arrange: manually inject a cycle_review_index row with schema_version=1
    #          (requires admin fixture with direct DB access or a seeding tool)
    # Act: context_cycle_review force=false for that cycle
    # Assert: response text contains advisory substring
```

Note: if direct DB seeding is not feasible through MCP, this test uses an integration
fixture that pre-seeds the DB file before the server process starts.

---

## `RetrospectiveReport` Structure (AC-06)

The new `curation_health: Option<CurationHealthBlock>` field on `RetrospectiveReport`
must serialize cleanly. Verify via `context_cycle_review` response:

```
grep -n "curation_health" crates/unimatrix-server/src/mcp/response/cycle_review.rs
```

Must show:
1. Field declaration `curation_health: Option<CurationHealthBlock>`.
2. `#[serde(skip_serializing_if = "Option::is_none")]` attribute (omit from output
   when `None` to avoid sending `"curation_health": null` to callers).
