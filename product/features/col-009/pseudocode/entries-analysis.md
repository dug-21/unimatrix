# Pseudocode: entries-analysis

## Purpose

Add `EntryAnalysis` type to `unimatrix-observe`, extend `RetrospectiveReport` with `entries_analysis` field, and update `build_report()` with a new 6th parameter. This component is the observe-layer side of the PendingEntriesAnalysis pipeline.

## Files

- MODIFY `crates/unimatrix-observe/src/types.rs` — add `EntryAnalysis`, add field to `RetrospectiveReport`
- MODIFY `crates/unimatrix-observe/src/report.rs` — add `entries_analysis` param to `build_report`, update callers
- MODIFY `crates/unimatrix-observe/src/lib.rs` — re-export `EntryAnalysis`

## Modification: `types.rs`

### New type: `EntryAnalysis`

Add after existing types:

```rust
/// Aggregated entry-level performance data for the retrospective.
///
/// Accumulated across sessions from Flagged signal drains.
/// `injection_count` is populated as 0 in col-009; col-010 provides INJECTION_LOG data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntryAnalysis {
    pub entry_id: u64,
    pub title: String,
    pub category: String,
    pub rework_flag_count: u32,
    pub injection_count: u32,       // reserved for col-010
    pub success_session_count: u32,
    pub rework_session_count: u32,
}
```

### Modified: `RetrospectiveReport`

Add new field (backward-compatible):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrospectiveReport {
    // Existing fields (unchanged)...

    // New col-009 field
    /// Entry-level analysis from Flagged signals, if any were accumulated since last call.
    /// Absent (not null) in JSON when None — use skip_serializing_if.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries_analysis: Option<Vec<EntryAnalysis>>,
}
```

The `entries_analysis` field MUST be absent from JSON (not `"entries_analysis": null`) when None.
Verified by: `#[serde(default, skip_serializing_if = "Option::is_none")]`.

## Modification: `report.rs`

### Updated `build_report` signature

```rust
pub fn build_report(
    feature_cycle: &str,
    records: &[ObservationRecord],
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
    baseline: Option<Vec<BaselineComparison>>,
    entries_analysis: Option<Vec<EntryAnalysis>>,  // NEW 6th parameter
) -> RetrospectiveReport {
    let session_count = records
        .iter()
        .map(|r| r.session_id.as_str())
        .collect::<HashSet<_>>()
        .len();

    RetrospectiveReport {
        feature_cycle: feature_cycle.to_string(),
        session_count,
        total_records: records.len(),
        metrics,
        hotspots,
        is_cached: false,
        baseline_comparison: baseline,
        entries_analysis,  // pass through
    }
}
```

### Update all existing callers of `build_report`

Find all call sites in the codebase:

```bash
# Find callers
grep -rn "build_report(" crates/
```

All existing callers must be updated to pass `None` as the 6th argument:

```rust
// Before:
build_report(feature_cycle, &records, metrics, hotspots, baseline)

// After:
build_report(feature_cycle, &records, metrics, hotspots, baseline, None)
```

Likely call sites:
- `crates/unimatrix-server/src/tools.rs` — `context_retrospective` handler (this one gets `entries_analysis` from `PendingEntriesAnalysis`)
- Any test files in `crates/unimatrix-observe/src/report.rs`
- Any integration test harness files

For the `context_retrospective` handler in `tools.rs` or `server.rs`, pass the drained `entries_analysis` instead of `None`.

## Modification: `lib.rs` (observe crate)

```rust
pub use types::EntryAnalysis;
```

Ensure `EntryAnalysis` is publicly accessible from `unimatrix_observe`.

## Error Handling

- `EntryAnalysis` is a plain data struct — no error handling needed
- `build_report` is infallible — returns `RetrospectiveReport` directly
- The `entries_analysis: Option<Vec<EntryAnalysis>>` allows callers to pass `None` without any impact on existing functionality

## Backward Compatibility

- `RetrospectiveReport` JSON: `entries_analysis` absent (not null) when None — confirmed by `#[serde(skip_serializing_if = "Option::is_none")]`
- Existing callers of `build_report` that pass `None` produce identical JSON output to pre-col-009
- New callers that pass `Some(vec![...])` add `entries_analysis` array to the JSON

## Key Test Scenarios

1. `test_entries_analysis_absent_when_none` — serialize `RetrospectiveReport` with `entries_analysis = None` → JSON does NOT contain `"entries_analysis"` key
2. `test_entries_analysis_present_when_some` — serialize with `entries_analysis = Some(vec![...])` → JSON contains `"entries_analysis"` key
3. `test_build_report_with_entries_analysis` — `build_report(..., Some(vec![analysis]))` → report has matching entries_analysis
4. `test_build_report_without_entries_analysis` — `build_report(..., None)` → report has `entries_analysis = None`
5. `test_entry_analysis_roundtrip` — serialize + deserialize `EntryAnalysis` preserves all fields
6. `test_entry_analysis_default` — `EntryAnalysis::default()` has zero/empty values
7. `test_retrospective_report_existing_callers_pass_none` — existing callers compile and produce same output as before col-009
