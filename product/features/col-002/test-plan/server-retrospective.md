# Test Plan: server-retrospective

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-09 (Concurrent retrospective calls) | Sequential cached result |

## Unit Tests: `crates/unimatrix-server/src/tools.rs` and related

### Validation

1. **test_validate_retrospective_params_empty** -- Empty feature_cycle -> InvalidInput error
2. **test_validate_retrospective_params_valid** -- "col-002" -> Ok

### Error Mapping

3. **test_observation_error_maps_to_error_code** -- ObservationError -> ERROR_NO_OBSERVATION_DATA code
4. **test_observation_error_display** -- Readable message, no Rust types leaked

### Response Formatting

5. **test_format_retrospective_report** -- Report -> CallToolResult with JSON content

## Integration Tests (infra-001, Stage 3c)

### context_retrospective (AC-19, AC-20, AC-25, AC-26)

6. **test_retrospective_accepts_feature_cycle** -- Tool callable with valid params (AC-19)
7. **test_retrospective_no_data_error** -- No observation files, no stored MV -> error (AC-25)
8. **test_retrospective_report_structure** -- Verify report JSON has expected fields (AC-21)
9. **test_retrospective_cached_result** -- Store MV, call again -> is_cached=true (AC-26)
10. **test_retrospective_stores_metrics** -- After successful call, OBSERVATION_METRICS has entry (AC-23)

### Tool Discovery

11. **test_tool_list_includes_retrospective** -- context_retrospective appears in tool list
