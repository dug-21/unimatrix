# Test Plan: server-status-ext

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-14 (StatusReport test churn) | Compilation verification, test helper updates |

## Unit Tests: `crates/unimatrix-server/src/response.rs`

### StatusReport Fields

1. **test_status_report_has_observation_fields** -- Construct StatusReport, verify 5 new fields accessible
2. **test_format_status_summary_includes_observation** -- Summary format includes observation section
3. **test_format_status_markdown_includes_observation** -- Markdown format includes observation section
4. **test_format_status_json_includes_observation** -- JSON format includes observation fields

### format_status_report with Observation Data

5. **test_format_status_approaching_cleanup** -- Non-empty approaching_cleanup -> warning in output (AC-35)
6. **test_format_status_zero_observations** -- All observation fields at 0 -> section still present but shows zeros

## Integration Tests (infra-001, Stage 3c)

### context_status Extension (AC-34)

7. **test_status_includes_observation_fields** -- Call context_status, verify observation fields in response (AC-34)
8. **test_status_observation_defaults_when_no_dir** -- Missing observation dir -> zeros in response

## Compilation Verification (R-14)

9. **All existing StatusReport construction sites updated** -- cargo test --workspace compiles (R-14)
