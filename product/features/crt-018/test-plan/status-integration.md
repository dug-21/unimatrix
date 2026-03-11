# Test Plan: status-integration

Component: `crates/unimatrix-server/src/mcp/response/status.rs` (StatusReport extension, formatting) and `crates/unimatrix-server/src/services/status.rs` (Phase 8 integration)
Test location: Existing test modules in those files, using TestDb and the server's test infrastructure

## End-to-End Integration Tests

These tests exercise the full pipeline: Store SQL -> Engine classification -> StatusReport -> format output.

**I-01: Full pipeline with all five categories** (AC-15)
- Setup via TestDb:
  - Entry A: trust_source="auto", helpful_count=0, injected into 2 success sessions -> Noisy (auto + 0 helpful + injections)
  - Entry B: trust_source="agent", helpful_count=1, injected into 4 abandoned sessions -> Ineffective (>= 3 injections, 0% success)
  - Entry C: trust_source="human", zero injections, topic "active-topic" has sessions -> Unmatched
  - Entry D: trust_source="human", injected into 1 success session, topic "dead-topic" has no sessions -> Settled
  - Entry E: trust_source="agent", injected into 3 success sessions -> Effective
- Call: `StatusService::compute_report()` (or the relevant test entry point)
- Assert: `report.effectiveness.is_some()`, by_category contains all five categories with correct counts.

**I-02: Effectiveness absent when no injection_log data** (AC-10, R-08)
- Setup: 5 entries, no sessions, no injection_log records.
- Call: compute_report
- Assert: `report.effectiveness` is None OR report shows all entries as Unmatched/Settled with zero injection data.
- Format as JSON: verify `effectiveness` key is absent (skip_serializing_if).

**I-03: Identical results on repeated calls** (AC-13)
- Setup: Fixed test data.
- Call: compute_report twice.
- Assert: Both EffectivenessReport values are identical (classifications are deterministic).
- Assert: No table row count changes (SELECT COUNT(*) from entries, sessions, injection_log before and after).

## Format Output Tests

**I-04: Summary format one-liner** (AC-08, R-08)
- Setup: Test data producing known category counts.
- Call: format_status_report with Summary format.
- Assert: Output contains line matching `Effectiveness: N effective, N settled, N unmatched, N ineffective, N noisy (N sessions analyzed)` with correct numbers.

**I-05: Summary format "no injection data"** (AC-08, NFR-06)
- Setup: Entries but no injection_log data.
- Assert: Output contains `Effectiveness: no injection data`.

**I-06: Markdown format section** (AC-09)
- Setup: Test data with effectiveness results.
- Call: format_status_report with Markdown format.
- Assert: Output contains `### Effectiveness Analysis` section header.
- Assert: Contains category table with all five categories.
- Assert: Contains per-source table.
- Assert: Contains calibration table with 10 rows.
- Assert: Contains data window indicator line.

**I-07: JSON format structure** (AC-10, R-08)
- Setup: Test data with effectiveness results.
- Call: format_status_report with Json format.
- Assert: Deserialize full JSON output; `effectiveness` object present.
- Assert: `effectiveness.by_category` is array with 5 entries.
- Assert: `effectiveness.calibration_buckets` is array with 10 entries.
- Assert: `effectiveness.data_window.session_count` matches expected.
- Assert: Existing JSON fields (entries, confidence, etc.) still present and unchanged.

## Graceful Degradation Tests

**I-08: Store error produces effectiveness = None** (R-11)
- Approach: If testable, simulate a store error (e.g., corrupt database or locked connection). Otherwise, verify by code review that the spawn_blocking result is matched with Ok/Err and Err sets effectiveness = None.
- Assert: Rest of StatusReport (phases 1-7) unaffected.

**I-09: No unwrap() on spawn_blocking result** (R-11)
- Verification: Code review / grep. Confirm Phase 8 uses `match` or `unwrap_or` on the JoinHandle result, not `.unwrap()`.

## Markdown Rendering Edge Cases

**I-10: Entry title with pipe character** (R-12)
- Setup: Entry with title containing `|` character, classified as Ineffective (appears in top_ineffective list).
- Call: format_status_report with Markdown format.
- Assert: Markdown table is not broken (pipe escaped or sanitized in title).

## StatusReport Field Compatibility

**I-11: Existing fields unchanged** (R-08)
- Setup: Same test data as used by existing status tests.
- Call: compute_report.
- Assert: All existing fields (entries_total, active_entries, confidence stats, contradiction data, etc.) have identical values whether or not effectiveness data is present.

## Acceptance Criteria Coverage

| AC-ID | Test ID(s) |
|-------|-----------|
| AC-01 | I-01 (all five categories present) |
| AC-08 | I-04, I-05 |
| AC-09 | I-06 |
| AC-10 | I-02, I-07 |
| AC-11 | I-09 (code review) |
| AC-12 | Covered by engine E-25, E-26, E-27; verified end-to-end in I-01 |
| AC-13 | I-03 |
| AC-15 | I-01 |
| AC-17 | Code review (grep for OUTCOME_WEIGHT_ constants) |

## spawn_blocking Verification (AC-11)

- Grep: `spawn_blocking` in `crates/unimatrix-server/src/services/status.rs`
- Verify Phase 8 effectiveness computation is wrapped in `tokio::task::spawn_blocking`
- Verify no `.await` on synchronous Store methods outside spawn_blocking
