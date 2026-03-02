# Test Plan: entries-analysis

## Component Scope

`crates/unimatrix-observe/src/types.rs`, `crates/unimatrix-observe/src/report.rs`
New `EntryAnalysis` type, `RetrospectiveReport.entries_analysis` field, `build_report` extension.

## Unit Tests

### EntryAnalysis type

**`test_entry_analysis_default`**
- `EntryAnalysis::default()` → all numeric fields == 0, strings are empty
- Verifies `#[derive(Default)]` works correctly

**`test_entry_analysis_roundtrip`**
- Construct `EntryAnalysis` with non-default values
- Serialize to JSON, deserialize back
- Assert: all fields preserved

**`test_entry_analysis_all_fields`**
- EntryAnalysis { entry_id: 42, title: "foo", category: "decision", rework_flag_count: 5, injection_count: 0, success_session_count: 3, rework_session_count: 2 }
- Roundtrip JSON: all fields correct

### RetrospectiveReport entries_analysis field (R-12, AC-13)

**`test_entries_analysis_absent_when_none`** (AC-13, R-12 scenario 1)
- Build RetrospectiveReport with entries_analysis = None
- Serialize to JSON string
- Assert: JSON string does NOT contain "entries_analysis" substring
- This verifies `#[serde(skip_serializing_if = "Option::is_none")]`

**`test_entries_analysis_present_when_some`** (R-12 scenario 2)
- Build RetrospectiveReport with entries_analysis = Some(vec![EntryAnalysis { entry_id: 1, ... }])
- Serialize to JSON
- Assert: JSON contains "entries_analysis" key with array value

**`test_entries_analysis_is_not_null_when_none`**
- Serialize with None → JSON does not contain `"entries_analysis": null`
- (Not just absent — the serialized form must not be null)

**`test_entries_analysis_deserialize_missing_field`** (backward compatibility)
- Deserialize a JSON string that does NOT have "entries_analysis" key
- Assert: entries_analysis == None (no error — `#[serde(default)]`)

**`test_entries_analysis_empty_array_vs_none`**
- Some(vec![]) → serializes as `"entries_analysis": []` (present but empty)
- None → key absent (not present at all)
- These are semantically different: None = no signals, Some([]) = drained but empty

### build_report function (FR-10.3, FR-10.4)

**`test_build_report_with_entries_analysis`**
- Call build_report with entries_analysis = Some(vec![entry_analysis_1])
- Assert: result.entries_analysis == Some(vec![entry_analysis_1])

**`test_build_report_without_entries_analysis`**
- Call build_report with entries_analysis = None
- Assert: result.entries_analysis == None

**`test_build_report_entries_analysis_passthrough`**
- Provide 3 EntryAnalysis items
- build_report returns them unchanged in entries_analysis

**`test_build_report_session_count_unchanged`**
- Adding entries_analysis parameter does not change session_count computation
- Assert: result.session_count == distinct session_ids in records

**`test_existing_callers_compile_with_none`**
- Any existing test that calls build_report must pass None as 6th arg
- Verified implicitly by `cargo test --workspace` (compilation test)

### RetrospectiveReport existing fields unchanged

**`test_retrospective_report_existing_fields_intact`**
- Serialize a RetrospectiveReport with entries_analysis = None
- JSON output matches pre-col-009 format exactly (no new required fields, no field name changes)

## Integration Tests (via MCP harness)

**`test_retrospective_returns_entries_analysis_when_flagged_signals`**
- Suite: `test_lifecycle.py`
- Simulate flagged signals accumulating
- Call context_retrospective via MCP
- Assert: response has entries_analysis with correct structure

**`test_retrospective_entries_analysis_absent_when_fresh_server`** (AC-13)
- Suite: `test_lifecycle.py`
- Fresh server, no signals
- Call context_retrospective
- Assert: JSON response does NOT have "entries_analysis" key

## Edge Cases

- Large entries_analysis (1000 items) serializes correctly
- entries_analysis with unicode in title/category: preserved in JSON
- injection_count is always 0 in col-009 (col-010 populates this): assert field present but zero
