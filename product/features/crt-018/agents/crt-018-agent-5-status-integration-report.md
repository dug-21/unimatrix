# Agent Report: crt-018-agent-5-status-integration

## Files Modified

- `crates/unimatrix-server/src/mcp/response/status.rs` -- Extended StatusReport with `effectiveness: Option<EffectivenessReport>`, added EffectivenessReportJson and 7 supporting JSON structs, added effectiveness mapping in From impl, added summary/markdown/JSON formatting
- `crates/unimatrix-server/src/services/status.rs` -- Added Phase 8 to compute_report: spawn_blocking that calls store.compute_effectiveness_aggregates() + load_entry_classification_meta(), classifies entries via classify_entry(), builds report via build_report()
- `crates/unimatrix-server/src/mcp/response/mod.rs` -- Added `effectiveness: None` to all existing StatusReport test constructions (8 instances)

## Test Results

- 1009 unit tests passed, 0 failed (unimatrix-server)
- 7 integration tests passed, 0 failed (pipeline_e2e)
- Full workspace build: success (zero errors)

## Implementation Details

### Phase 8 (services/status.rs)
- spawn_blocking wraps both store calls (compute_effectiveness_aggregates + load_entry_classification_meta)
- Builds HashMap<u64, &EntryInjectionStats> for O(1) lookup
- Iterates entry_meta, classifies each via classify_entry with NOISY_TRUST_SOURCES
- Constructs DataWindow from flat aggregates fields (session_count, earliest_session_at, latest_session_at)
- Graceful degradation: store errors and task panics both set effectiveness = None (R-11)
- No .unwrap() on spawn_blocking result

### Summary Format
- One-line: "Effectiveness: N effective, N settled, N unmatched, N ineffective, N noisy (N sessions analyzed)"
- "Effectiveness: no injection data" when effectiveness is None

### Markdown Format
- "### Effectiveness Analysis" section with data window indicator
- Category table with percentages, per-source table, calibration table
- Top ineffective/noisy/unmatched entry lists with pipe-character sanitization (R-12)
- "Insufficient injection data for analysis" when None

### JSON Format
- EffectivenessReportJson with skip_serializing_if = "Option::is_none"
- 7 nested structs: CategoryCount, SourceEffectivenessJson, CalibrationBucketJson, IneffectiveEntryJson, NoisyEntryJson, UnmatchedEntryJson, DataWindowJson
- span_days computed from earliest/latest timestamps

## Deviations from Pseudocode

1. **DataWindow construction**: Pseudocode shows `aggregates.session_count` / `aggregates.earliest_session_at` / `aggregates.latest_session_at` as direct fields. The store agent implemented EffectivenessAggregates with flat fields (not nested DataWindow), so the code constructs DataWindow manually from those fields. This matches the actual implementation.

2. **Summary format HashMap**: Pseudocode uses HashMap for category lookup, but EffectivenessCategory does not implement Hash. Replaced with explicit match loop over by_category vec.

## Issues

- Both status.rs files exceed the 500-line limit (1000 and 823 lines respectively). These files were already over the limit before crt-018 changes (704 and 751 lines). Splitting would be a refactor outside scope.
