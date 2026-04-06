# crt-047: Pseudocode — context_status Phase 7c

## Purpose

Add Phase 7c to `compute_report()` in `status.rs`. This phase reads the curation
health baseline window from `cycle_review_index` and delegates all computation to
`services/curation_health.rs`. Total addition to `status.rs`: ~15-20 lines.

File: `crates/unimatrix-server/src/services/status.rs`

---

## Insertion Point

Phase 7c is inserted immediately after Phase 7b (pending cycle reviews, crt-033)
and before Phase 8 (effectiveness analysis). The existing structure is:

```
// Phase 7b: Pending cycle reviews (crt-033).
{
    ...
    report.pending_cycle_reviews = pending;
}

// Phase 8: Effectiveness analysis (crt-018)
```

Insert Phase 7c between these two blocks.

---

## Constant

Define near other window constants in `status.rs` (following the pattern of
`PENDING_REVIEWS_K_WINDOW_SECS` defined in the same file):

```
/// Number of prior cycles to include in the curation health baseline window.
/// Used by Phase 7c; must match the window size used by context_cycle_review
/// for consistency.
const CURATION_BASELINE_WINDOW: usize = 10;
```

---

## Phase 7c Pseudocode

```
// Phase 7c: Curation health aggregate (crt-047).
//
// Reads the last CURATION_BASELINE_WINDOW cycle_review_index rows ordered by
// first_computed_at DESC (excluding rows with first_computed_at = 0).
// Uses read_pool() — consistent with all other Phase 7 reads (ADR-001, crt-033).
// All computation is delegated to curation_health::compute_curation_summary().
// On failure: degrade gracefully to None; do NOT fail compute_report().
{
    let curation_window = self.store
        .get_curation_baseline_window(CURATION_BASELINE_WINDOW)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(
                "crt-047: get_curation_baseline_window failed: {} — \
                 curation_health will be absent from this response",
                e
            );
            vec![]
        });

    report.curation_health = curation_health::compute_curation_summary(&curation_window);
}
```

---

## StatusReport Extension

Add `curation_health: Option<CurationHealthSummary>` to `StatusReport` in
`mcp/response/status.rs`:

```
// In StatusReport struct:
pub curation_health: Option<CurationHealthSummary>,
    // None when:
    //   - get_curation_baseline_window() returns empty (no qualifying rows)
    //   - get_curation_baseline_window() fails (graceful degradation)
    //   - compute_curation_summary() receives empty slice
    // Some(summary) when at least one qualifying row exists.
    // trend field within summary is None when fewer than 6 qualifying rows.
```

Initialize in the `StatusReport` default/initializer to `None` (consistent with
other optional fields).

---

## StatusService Access to Store

Phase 7c calls `self.store.get_curation_baseline_window(...)`. Verify that
`StatusService` holds an `Arc<SqlxStore>` (or equivalent) accessible as `self.store`.
If `StatusService` accesses the store through a different field name, use that name.

The existing Phase 7b calls `self.store.pending_cycle_reviews(...)`, confirming
`self.store` is the correct access path.

---

## Data Flow

```
Input:
  (none — reads from persistent store)

Async read (read_pool):
  self.store.get_curation_baseline_window(CURATION_BASELINE_WINDOW)
    → Result<Vec<CurationBaselineRow>, StoreError>
    → .unwrap_or_else(|_| vec![])  → Vec<CurationBaselineRow>

Pure:
  curation_health::compute_curation_summary(&curation_window)
    → Option<CurationHealthSummary>

Output:
  report.curation_health = Option<CurationHealthSummary>
```

`CurationHealthSummary` fields (all computed by `compute_curation_summary`):
- `correction_rate_mean`, `correction_rate_stddev` — mean/stddev of `corrections_total` over window
- `agent_pct`, `human_pct` — source breakdown percentages (0.0 when total = 0)
- `orphan_ratio_mean`, `orphan_ratio_stddev` — orphan ratio mean/stddev
- `trend: Option<TrendDirection>` — None when < 6 qualifying rows
- `cycles_in_window` — total rows in the window (including legacy DEFAULT-0 rows)

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| `get_curation_baseline_window` SQL failure | Log error; `curation_window = vec![]`; `curation_health = None` |
| Empty window (no qualifying rows) | `compute_curation_summary([])` → None; `curation_health = None` |
| Phase 7c failure of any kind | `compute_report()` must NOT return Err; degrade to None |

Rationale: `compute_report()` is called by the `context_status` MCP tool. Any failure
in Phase 7c must not propagate as an error to the caller — consistent with Phase 7b
degradation (existing pattern in the codebase).

---

## NFR Compliance

**NFR-04**: Phase 7c reads ONLY from `cycle_review_index` snapshot columns. It does NOT
re-run the full retrospective pipeline (no ENTRIES traversal, no ONNX calls, no graph queries).

**FR-17**: Phase 7c uses `read_pool()` (via `get_curation_baseline_window`). No write
operations in this phase.

---

## Key Test Scenarios

**T-CS7C-01 (AC-09)**: Phase 7c populates `curation_health` from N rows with `first_computed_at > 0`.
- Seed 5 rows with `first_computed_at > 0` and schema_version = 2.
- Call `context_status`.
- Assert: `curation_health` is Some; `cycles_in_window = 5`.

**T-CS7C-02 (AC-10, 5-cycle case)**: Trend absent when fewer than 6 qualifying rows.
- Seed exactly 5 qualifying rows.
- Assert: `curation_health.trend` is None.

**T-CS7C-03 (AC-10, 7-cycle case)**: Trend present when 7 qualifying rows.
- Seed 7 rows; corrections_total increasing over time.
- Assert: `curation_health.trend` is Some(Increasing).

**T-CS7C-04 (EC-06)**: Empty window — `curation_health` is None, no error.
- Fresh database with no cycle_review_index rows (or all `first_computed_at = 0`).
- Call `context_status`.
- Assert: no error; `curation_health` is None.

**T-CS7C-05 (NFR-04)**: Phase 7c reads only from `cycle_review_index`.
- Code inspection: no ENTRIES query, no ONNX call, no graph traversal in Phase 7c.

**T-CS7C-06 (AC-13)**: Phase 7c uses `read_pool()`.
- Code inspection: `get_curation_baseline_window` uses `self.read_pool()`.

**T-CS7C-07 (R-10)**: Schema cascade — `CURRENT_SCHEMA_VERSION = 24` in all test assertions.
- After implementation: `grep -r 'schema_version.*== 23' crates/` returns zero matches.
