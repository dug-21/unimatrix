# vnc-011: Retrospective ReportFormatter -- Test Strategy

## Test Approach

Three test levels, all within `crates/unimatrix-server/`:

1. **Unit tests** -- Pure function testing of each render helper and the top-level `format_retrospective_markdown()`. Build `RetrospectiveReport` structs in-memory, call the formatter, assert on output strings. No I/O, no server. These live in `retrospective.rs` as `#[cfg(test)] mod tests`.
2. **Component integration tests** -- Verify handler dispatch in `tools.rs` routes correctly between markdown and JSON formatters based on the `format` parameter. These extend the existing `#[cfg(test)]` section in `tools.rs`.
3. **Integration harness tests** -- MCP-level validation through the compiled binary. Existing `tools` suite covers `context_retrospective`; new tests validate the `format` parameter routing and markdown output shape.

## Risk-to-Test Mapping

| Risk ID | Priority | Component | Test Level | Test Name(s) |
|---------|----------|-----------|------------|---------------|
| R-01 | High | retrospective-formatter | Unit | `test_collapse_mixed_severity_picks_highest`, `test_collapse_same_severity_preserved`, `test_finding_ordering_by_severity_then_count` |
| R-02 | High | handler-dispatch | Unit | `test_json_evidence_limit_default_3`, `test_json_evidence_limit_explicit_5`, `test_markdown_ignores_evidence_limit` |
| R-03 | High | retrospective-formatter | Unit | `test_all_none_optional_fields_valid_markdown`, `test_single_optional_some_others_none` (x8) |
| R-04 | High | retrospective-formatter | Unit | `test_narrative_matches_by_hotspot_type`, `test_narrative_no_match_falls_back`, `test_narrative_sequence_pattern_inline` |
| R-05 | Med | retrospective-formatter | Unit | `test_evidence_empty_pool`, `test_evidence_one_record`, `test_evidence_three_records`, `test_evidence_ten_records_earliest_three`, `test_evidence_same_timestamp` |
| R-06 | Med | retrospective-formatter | Unit | `test_session_empty_tool_dist`, `test_session_zero_duration`, `test_session_missing_outcome`, `test_session_two_rows` |
| R-07 | Med | retrospective-formatter | Unit | `test_baseline_all_normal_omits_section`, `test_baseline_mixed_filters_correctly`, `test_baseline_empty_vec_omits_section`, `test_baseline_single_outlier` |
| R-08 | Med | retrospective-formatter | Unit | `test_recommendation_dedup_same_type`, `test_recommendation_distinct_types`, `test_recommendation_empty` |
| R-09 | Low | retrospective-formatter | Unit | `test_zero_activity_phase_suppressed` |
| R-10 | Low | retrospective-formatter | Unit | `test_duration_zero`, `test_duration_over_24h`, `test_duration_standard` |
| R-11 | Low | retrospective-formatter | Unit | `test_pipe_in_metric_name` |
| R-12 | Low | retrospective-formatter | Unit | `test_float_sum_no_artifacts` |
| R-13 | Med | handler-dispatch | Unit | `test_format_markdown_explicit`, `test_format_json_explicit`, `test_format_none_defaults_markdown`, `test_format_invalid_error` |
| R-14 | Low | retrospective-formatter | Unit | `test_phase_outlier_suppresses_zero_activity` |
| IR-01 | -- | retrospective-formatter | Compile | Type mismatches caught at build time |
| IR-02 | -- | handler-dispatch | Unit+Integration | `test_dispatch_routes_correctly` + harness `test_retrospective_format_markdown` |
| IR-03 | -- | handler-dispatch | Unit | `test_markdown_receives_full_report_ignores_evidence_limit` |
| IR-04 | -- | params-extension | Compile | `cargo test` without `mcp-briefing` feature (CI gate) |

## Cross-Component Test Dependencies

- **params-extension -> handler-dispatch**: The `format` field on `RetrospectiveParams` must deserialize correctly before the handler can dispatch. Param deserialization tests must pass first.
- **handler-dispatch -> retrospective-formatter**: Dispatch routes to `format_retrospective_markdown`. The formatter must be importable and callable. Module registration in `response/mod.rs` is a prerequisite.
- **retrospective-formatter -> unimatrix-observe types**: The formatter reads `RetrospectiveReport` fields. Type compatibility is compile-time verified; semantic correctness (field meaning) is covered by unit tests with known inputs.

## Integration Harness Plan

### Existing Suite Coverage

| Suite | Relevant Tests | What They Cover |
|-------|---------------|-----------------|
| `tools` | `test_retrospective_*` (if any exist) | Existing JSON retrospective call -- validates the handler path works end-to-end |
| `protocol` | handshake, tool discovery | Confirms `context_retrospective` appears in tool list with updated schema (new `format` param) |
| `smoke` | minimum gate | Baseline regression check |

### Suites to Run (Stage 3c)

Per the suite selection table -- this feature touches server tool logic and tool parameters:

1. **`smoke`** -- mandatory gate
2. **`tools`** -- validates `context_retrospective` through MCP interface
3. **`protocol`** -- confirms tool schema discovery includes `format` parameter

### New Integration Tests Needed

The `format` parameter changes MCP-visible behavior (different response content type). This warrants new integration tests in `suites/test_tools.py`:

| Test Name | Fixture | What It Validates |
|-----------|---------|-------------------|
| `test_retrospective_markdown_default` | `server` | Call `context_retrospective` with no `format` param, assert response text starts with `# Retrospective:` |
| `test_retrospective_json_explicit` | `server` | Call with `format: "json"`, assert response is valid JSON object |
| `test_retrospective_format_invalid` | `server` | Call with `format: "xml"`, assert error or graceful fallback |

These tests require observation data to exist for a feature cycle. If no existing fixture populates observation data, the tests should store sufficient data first or be scoped to validate format routing (checking error messages or empty-report formatting).

### Tests NOT Needed at Integration Level

- Finding collapse logic -- purely internal, fully testable via unit tests
- Render helper output -- string formatting, no MCP-visible effect beyond final content
- Evidence selection -- deterministic algorithm, unit tests are definitive
