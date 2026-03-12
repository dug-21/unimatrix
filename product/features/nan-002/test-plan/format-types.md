# nan-002: Test Plan -- format-types

## Component Scope

Shared typed deserialization structs in `crates/unimatrix-server/src/format.rs`: `ExportHeader`, `ExportRow` (tagged enum), and all per-table row structs (`CounterRow`, `EntryRow`, `EntryTagRow`, `CoAccessRow`, `FeatureEntryRow`, `OutcomeIndexRow`, `AgentRegistryRow`, `AuditLogRow`).

## Unit Tests

### ExportHeader Deserialization

#### test_header_deserialize_valid
- Input: `{"_header":true,"schema_version":11,"exported_at":1741234567,"entry_count":245,"format_version":1}`
- Assert all fields match expected values

#### test_header_deserialize_missing_field_errors
- Input: header JSON missing `format_version`
- Assert serde error

### ExportRow Tagged Enum

#### test_export_row_counter_dispatch
- Input: `{"_table":"counters","name":"next_entry_id","value":100}`
- Assert `ExportRow::Counter(CounterRow { name: "next_entry_id", value: 100 })`

#### test_export_row_entry_dispatch
- Input: JSON with `_table: "entries"` and all 26 fields
- Assert `ExportRow::Entry(EntryRow { ... })` with correct values

#### test_export_row_unknown_table_errors (R-11)
- Input: `{"_table":"unknown_table","foo":"bar"}`
- Assert deserialization error message contains "unknown_table"

### EntryRow Edge Cases (R-02, AC-23)

#### test_entry_row_null_optionals
- Input: entry JSON with `supersedes: null`, `superseded_by: null`, `pre_quarantine_status: null`
- Assert all three fields are `None`

#### test_entry_row_empty_strings
- Input: entry JSON with `previous_hash: ""`, `feature_cycle: ""`, `trust_source: ""`
- Assert fields are empty strings, not errors

#### test_entry_row_unicode_content
- Input: entry with title containing CJK characters, content containing emoji and combining marks
- Assert deserialized strings match input exactly (byte-equal)

#### test_entry_row_max_integers
- Input: entry with `access_count: 9223372036854775807` (i64::MAX), `helpful_count: 9223372036854775807`
- Assert values deserialize without overflow

#### test_entry_row_all_26_fields_present
- Input: entry JSON with exactly 26 non-`_table` fields
- Assert each field maps to the correct struct field
- Risks: R-01

### CounterRow

#### test_counter_row_deserialize
- Input: `{"_table":"counters","name":"schema_version","value":11}`
- Assert correct deserialization

### EntryTagRow

#### test_entry_tag_row_deserialize
- Input: `{"_table":"entry_tags","entry_id":1,"tag":"architecture"}`
- Assert correct deserialization

#### test_entry_tag_row_unicode_tag
- Input: tag with unicode characters
- Assert correct deserialization

### CoAccessRow

#### test_co_access_row_deserialize
- Input: `{"_table":"co_access","entry_id_a":1,"entry_id_b":2,"count":5,"last_updated":1741234567}`
- Assert correct deserialization

### FeatureEntryRow

#### test_feature_entry_row_deserialize
- Input: `{"_table":"feature_entries","feature_id":"crt-005","entry_id":42}`
- Assert `feature_id == "crt-005"`, `entry_id == 42`
- Note: field is `feature_id` per DDL, not `feature_cycle`

### OutcomeIndexRow

#### test_outcome_index_row_deserialize
- Input: `{"_table":"outcome_index","feature_cycle":"col-001","entry_id":10}`
- Assert correct deserialization

### AgentRegistryRow (R-02, JSON-in-TEXT)

#### test_agent_registry_row_deserialize
- Input: with `capabilities: "[\"admin\",\"read\"]"`, `allowed_topics: null`, `allowed_categories: null`
- Assert `capabilities` is the raw JSON string `["admin","read"]`, not parsed
- Assert `allowed_topics` is `None`

#### test_agent_registry_row_with_topics
- Input: with `allowed_topics: "[\"testing\"]"`, `allowed_categories: "[\"pattern\"]"`
- Assert raw JSON strings preserved

### AuditLogRow

#### test_audit_log_row_deserialize
- Input: with `target_ids: "[]"`, all other fields populated
- Assert correct deserialization, `target_ids` is raw string `[]`

### Floating-Point Fidelity (R-10)

#### test_entry_row_confidence_precision
- Input: entry with `confidence: 0.8723456789012345`
- Serialize back to JSON, compare string representation
- Assert at least 15 significant digits preserved

#### test_entry_row_confidence_boundaries
- Input: entries with `confidence: 0.0` and `confidence: 1.0`
- Assert exact values after round-trip

### Column Count Guard (R-01)

#### test_entry_row_field_count_matches_ddl
- Query `PRAGMA table_info(entries)` on a freshly opened Store
- Assert column count == 26
- Assert column names match EntryRow field names (accounting for serde rename if any)

## Risk Coverage

| Risk | Tests | Coverage |
|------|-------|----------|
| R-01 (SQL/schema divergence) | test_entry_row_all_26_fields_present, test_entry_row_field_count_matches_ddl | Full |
| R-02 (deserialization edge cases) | All edge-case tests (null, empty, unicode, max int, JSON-in-TEXT) | Full |
| R-10 (f64 precision) | test_entry_row_confidence_precision, test_entry_row_confidence_boundaries | Full |
| R-11 (unknown _table) | test_export_row_unknown_table_errors | Full |
