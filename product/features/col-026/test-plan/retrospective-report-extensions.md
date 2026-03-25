# Test Plan: retrospective-report-extensions

**Crate**: `unimatrix-observe/src/types.rs`
**Risks covered**: R-05, R-12, R-13
**ACs covered**: AC-05, AC-16, AC-17, AC-18

---

## Component Scope

This component adds five new optional fields to `RetrospectiveReport` and four new structs
(`PhaseStats`, `ToolDistribution`, `GateResult`, `EntryRef`) and four new fields to
`FeatureKnowledgeReuse`. All new fields use `#[serde(default, skip_serializing_if = "Option::is_none")]`
or `#[serde(default)]`. No breaking changes to existing fields.

---

## Unit Test Expectations

All tests live in `unimatrix-observe/src/types.rs` as a `#[cfg(test)] mod tests` block,
or in `unimatrix-server/src/mcp/response/retrospective.rs` (formatter tests for rendering).

### Test: `test_is_in_progress_none_when_no_events` (R-05, AC-05)

**Scenario**: No `cycle_events` rows exist for this cycle.
**Input**: `is_in_progress = None` on `RetrospectiveReport`.
**Assert**:
- Serialized JSON does not contain `"is_in_progress"` key at all (key-absent, not null).
- Formatter renders no `Status:` line in the header block.

```rust
let mut report = make_report();
report.is_in_progress = None;
let json = serde_json::to_string(&report).unwrap();
assert!(!json.contains("is_in_progress"));
let text = extract_text(&format_retrospective_markdown(&report));
assert!(!text.contains("Status:"));
```

### Test: `test_is_in_progress_some_true_renders_in_progress` (R-05, AC-05)

**Scenario**: `cycle_start` present but no `cycle_stop`.
**Input**: `is_in_progress = Some(true)`.
**Assert**: Formatter header contains `Status: IN PROGRESS`.

```rust
report.is_in_progress = Some(true);
let text = extract_text(&format_retrospective_markdown(&report));
assert!(text.contains("Status: IN PROGRESS"));
```

### Test: `test_is_in_progress_some_false_omits_status_line` (R-05, AC-05)

**Scenario**: `cycle_stop` row confirmed present.
**Input**: `is_in_progress = Some(false)`.
**Assert**: Formatter header does NOT contain `Status:` (spec FR-05: `Some(false)` omits status).

```rust
report.is_in_progress = Some(false);
let text = extract_text(&format_retrospective_markdown(&report));
assert!(!text.contains("Status:"));
```

### Test: `test_is_in_progress_serde_roundtrip_none` (R-05, AC-16)

**Scenario**: Serde roundtrip for the `None` case.
**Assert**:
- Serialized JSON lacks `is_in_progress` key.
- Deserializing a payload that lacks `is_in_progress` produces `None`, not `Some(false)`.

```rust
let json = r#"{"feature_cycle":"col-026","session_count":1,"total_records":0,...}"#;
let report: RetrospectiveReport = serde_json::from_str(json).unwrap();
assert!(report.is_in_progress.is_none());
```

### Test: `test_phase_stats_none_absent_from_json` (R-12, AC-16)

**Scenario**: `phase_stats = None`.
**Assert**: JSON does not contain `"phase_stats"` key.

### Test: `test_phase_stats_some_empty_present_in_json` (R-12, AC-16)

**Scenario**: `phase_stats = Some(vec![])`.
**Assert**: JSON contains `"phase_stats":[]` (key present, empty array).
This distinguishes the `None` (no cycle_events) from `Some([])` (events present but computed
to empty, which should not occur — see R-12 handler canonicalization).

### Test: `test_new_report_fields_absent_when_none` (AC-16, AC-17)

**Scenario**: All new fields (`goal`, `cycle_type`, `attribution_path`, `is_in_progress`,
`phase_stats`) are `None`.
**Assert**: Serialized JSON contains none of these keys (all `skip_serializing_if`).

### Test: `test_new_report_fields_present_when_some` (AC-16)

**Scenario**: All new fields are `Some(...)`.
**Assert**: All five keys present in JSON with correct values.

```rust
report.goal = Some("Design the API surface".to_string());
report.cycle_type = Some("Design".to_string());
report.attribution_path = Some("cycle_events-first (primary)".to_string());
report.is_in_progress = Some(false);
report.phase_stats = Some(vec![/* PhaseStats fixture */]);
let json = serde_json::to_string(&report).unwrap();
assert!(json.contains("\"goal\""));
assert!(json.contains("\"cycle_type\""));
assert!(json.contains("\"attribution_path\""));
assert!(json.contains("\"is_in_progress\""));
assert!(json.contains("\"phase_stats\""));
```

### Test: `test_knowledge_reuse_serde_backward_compat` (AC-18, R-13)

**Scenario**: Deserialize a `FeatureKnowledgeReuse` JSON payload that lacks all four new fields
(`total_served`, `total_stored`, `cross_feature_reuse`, `intra_cycle_reuse`,
`top_cross_feature_entries`).
**Assert**: Deserialization succeeds; new fields default to `0` / empty vec.

```rust
let json = r#"{"delivery_count":5,"cross_session_count":2,"by_category":{},"category_gaps":[]}"#;
let reuse: FeatureKnowledgeReuse = serde_json::from_str(json).unwrap();
assert_eq!(reuse.cross_feature_reuse, 0);
assert_eq!(reuse.intra_cycle_reuse, 0);
assert_eq!(reuse.total_stored, 0);
assert!(reuse.top_cross_feature_entries.is_empty());
```

### Test: `test_gate_result_serde` (AC-16)

**Scenario**: `GateResult` serializes and deserializes correctly.
**Assert**: Each variant round-trips via JSON.

### Test: `test_tool_distribution_default` (AC-16)

**Scenario**: `ToolDistribution::default()` has all zero counts.
**Assert**: `read == 0`, `execute == 0`, `write == 0`, `search == 0`.

### Test: `test_phase_stats_all_required_fields_present`

**Scenario**: Construct a `PhaseStats` with all fields. Serialize and deserialize.
**Assert**: All fields survive roundtrip. `hotspot_ids` defaults to empty vec.

### Test: `test_entry_ref_serde` (AC-16)

**Scenario**: `EntryRef { id: 42, title: "...", feature_cycle: "col-024", category: "decision", serve_count: 3 }`.
**Assert**: Serializes and deserializes correctly with all fields.

---

## Integration Test Expectations

**Through the MCP interface** (infra-001 `test_tools.py`):

- `context_cycle_review(format="json")` response must parse as JSON containing `is_in_progress`
  when cycle_events data is present.
- When no cycle_events exist, `is_in_progress` key must be absent from JSON (not null).

See OVERVIEW.md Integration Test 2 for the full scenario.

---

## Edge Cases

- `is_in_progress = Some(true)` on a cached report: formatter must still render `IN PROGRESS`
  from the stored field value — the caching path must preserve `is_in_progress`.
- `goal` containing markdown-special characters (backticks, pipes): verify they do not cause
  JSON serialization failure.

---

## Compile-Time Gate (R-13)

After adding new fields to `FeatureKnowledgeReuse`, `cargo build` must succeed with zero
errors. The three construction sites in `types.rs` tests, `knowledge_reuse.rs` production
code, and `retrospective.rs` test fixtures must all be updated. This is verified by the CI
build, not a runtime test.
