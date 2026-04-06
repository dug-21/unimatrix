# Test Plan: `context_status` Phase 7c

Component: `crates/unimatrix-server/src/services/status.rs` (Phase 7c, ~15-20 lines)
Supporting: `services/curation_health.rs` (`compute_curation_summary()`), `mcp/response/status.rs`
Risk coverage: R-02, I-01, I-04, AC-09, AC-10, AC-13

---

## What Is Under Test

Phase 7c is the new block added to `compute_report()` in `status.rs`:

```
1. Call get_curation_baseline_window(CURATION_BASELINE_WINDOW=10) via read_pool()
2. Call compute_curation_summary(&rows) → Option<CurationHealthSummary>
3. Attach result to StatusReport.curation_health
```

`CURATION_BASELINE_WINDOW: usize = 10` is a named constant in `status.rs`.
All curation logic is delegated to `curation_health.rs` — Phase 7c itself is ~15-20 lines.

---

## Unit Tests

These tests exercise the integration between Phase 7c and the store via the existing
`status.rs` test infrastructure (`compute_report()` with a seeded `SqlxStore`).

### CS7C-U-01: `curation_health` block present when rows exist (AC-09)

```
test_status_curation_health_present_when_rows_exist
```

- Arrange: insert 3 `cycle_review_index` rows with `first_computed_at > 0`,
  `schema_version = 2`, non-zero snapshot values.
- Act: call `compute_report()` or the Phase 7c sub-routine.
- Assert: `report.curation_health.is_some()`.
- Assert: `curation_health.correction_rate_mean >= 0.0`.
- Assert: `!curation_health.correction_rate_mean.is_nan()`.
- Assert: `!curation_health.orphan_ratio_mean.is_nan()`.

### CS7C-U-02: `curation_health` block absent when no qualifying rows (EC-06)

```
test_status_curation_health_absent_when_no_qualifying_rows
```

- Arrange: fresh DB, or insert only rows with `first_computed_at = 0`.
- Act: call `compute_report()`.
- Assert: `report.curation_health.is_none()`.
- Assert: no error in response.

### CS7C-U-03: Trend absent when fewer than 6 cycles (AC-10 sub-test a)

```
test_status_curation_health_trend_absent_with_five_cycles
```

- Arrange: insert 5 `cycle_review_index` rows with `first_computed_at > 0`,
  `schema_version = 2`, non-zero `corrections_total`.
- Assert: `curation_health.is_some()`.
- Assert: `curation_health.trend.is_none()`.
- Assert: `curation_health.correction_rate_mean > 0.0`.

### CS7C-U-04: Trend present when 7 cycles available (AC-10 sub-test b)

```
test_status_curation_health_trend_present_with_seven_cycles
```

- Arrange: insert 7 `cycle_review_index` rows with `first_computed_at > 0`,
  `schema_version = 2`, non-zero snapshot values.
- Assert: `curation_health.trend.is_some()`.

### CS7C-U-05: Agent% and human% in expected range (AC-10 breakdown)

```
test_status_curation_health_source_breakdown_percentages
```

- Arrange: 5 rows each with `corrections_total = 6`, `corrections_agent = 4`,
  `corrections_human = 2`.
- Assert: `curation_health.agent_pct ≈ 66.7` (within 0.1 tolerance).
- Assert: `curation_health.human_pct ≈ 33.3`.
- Assert: `curation_health.agent_pct + curation_health.human_pct ≈ 100.0`.

### CS7C-U-06: Window capped at `CURATION_BASELINE_WINDOW = 10` (AC-09 boundary)

```
test_status_curation_health_window_capped_at_ten
```

- Arrange: insert 15 `cycle_review_index` rows with distinct `first_computed_at > 0`,
  `schema_version = 2`.
- Act: call `compute_report()`.
- Assert: `curation_health.cycles_in_window == 10`.
- Assert: the 10 most-recent-by-`first_computed_at` rows contribute.

### CS7C-U-07: `read_pool()` used for baseline window query (AC-13, I-01)

Pool usage is verified statically:

```bash
grep -n "read_pool\|write_pool" crates/unimatrix-server/src/services/status.rs \
    | grep -A2 -B2 "curation"
```

Must show `read_pool()` at the Phase 7c call site. No `write_pool_server()` or
`spawn_blocking` calls in Phase 7c.

```bash
grep -n "curation_baseline_window\|get_curation_baseline" \
    crates/unimatrix-server/src/services/status.rs
```

Must show the call using `self.store.get_curation_baseline_window(CURATION_BASELINE_WINDOW)`.

### CS7C-U-08: Phase 7c does not invoke the full retrospective pipeline (NFR-04)

```bash
grep -n "retrospective\|compute_retrospective\|unimatrix_observe" \
    crates/unimatrix-server/src/services/status.rs
```

The Phase 7c code block must NOT contain references to the retrospective pipeline.
The curation_health block reads ONLY from `cycle_review_index` snapshot columns.

---

## `StatusReport` Structure (AC-09)

The new `curation_health: Option<CurationHealthSummary>` field on `StatusReport`:

```bash
grep -n "curation_health" crates/unimatrix-server/src/mcp/response/status.rs
```

Must show:
1. Field declaration with `Option<CurationHealthSummary>`.
2. `#[serde(skip_serializing_if = "Option::is_none")]` — omit from JSON when absent.

---

## `CURATION_BASELINE_WINDOW` constant (FR-10, FR-18)

```bash
grep -n "CURATION_BASELINE_WINDOW" crates/unimatrix-server/src/services/status.rs
```

Must return the constant definition `const CURATION_BASELINE_WINDOW: usize = 10`.
The value `10` must not appear inlined in the `get_curation_baseline_window(...)` call.

---

## Integration Tests (MCP-Level)

### CS7C-I-01: `context_status` response includes `curation_health` (AC-09)

```python
def test_status_curation_health_block_present(shared_server):
    # Arrange: complete 3 cycles via context_cycle + context_cycle_review
    # Act: call context_status
    # Assert: response contains curation_health with correction_rate_mean
    # Assert: curation_health.orphan_ratio_mean present (may be 0.0)
    # Assert: no NaN in numeric fields
```

In `suites/test_lifecycle.py` using `shared_server` fixture.

### CS7C-I-02: `context_status` curation block absent on empty DB (EC-06)

```python
def test_status_curation_health_absent_on_fresh_db(server):
    # Act: call context_status on a fresh DB with no cycle_review_index rows
    # Assert: curation_health field absent or None in response
    # Assert: no error returned
```

In `suites/test_lifecycle.py` using `server` fixture (fresh DB per test).

---

## Coverage Summary for Phase 7c

| AC-ID | Test | Verified By |
|-------|------|-------------|
| AC-09 | CS7C-U-01, CS7C-I-01 | Block present, window ordered by first_computed_at |
| AC-10 (5-cycle) | CS7C-U-03 | Trend absent |
| AC-10 (7-cycle) | CS7C-U-04 | Trend present |
| AC-10 (breakdown) | CS7C-U-05 | agent%/human% |
| AC-13 | CS7C-U-07 (grep) | read_pool() at Phase 7c call site |
| NFR-04 | CS7C-U-08 (grep) | No retrospective pipeline invocation |
