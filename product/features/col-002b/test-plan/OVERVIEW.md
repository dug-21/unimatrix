# Test Plan Overview: col-002b Detection Library + Baseline Comparison

## Test Strategy

### Unit Tests (per-component, in `#[cfg(test)] mod tests`)

Each component has comprehensive unit tests with synthetic `ObservationRecord` data. Tests follow Arrange/Act/Assert. Every detection rule has at minimum:
- `fires_above_threshold` -- synthetic records exceeding threshold
- `silent_below_threshold` -- records below threshold produce no findings
- `handles_empty_records` -- empty input returns empty findings
- `handles_missing_fields` -- records with None tool/input are skipped

### Integration Tests (via infra-001 harness)

The feature modifies `context_retrospective` behavior. Integration tests exercise the full pipeline: observation data -> detection -> baseline -> report.

## Risk-to-Test Mapping

| Risk ID | Risk | Priority | Test Coverage |
|---------|------|----------|---------------|
| R-01 | Rules silently produce no findings | High | Per-rule fires_above_threshold tests |
| R-02 | Baseline NaN/Inf | Medium | baseline::test_identical_values, test_all_zeros, test_no_nan_inf |
| R-03 | Phase duration outlier mismatched names | Medium | scope::test_phase_no_matching_history |
| R-04 | Regex patterns miss variations | Medium | Per-rule regex edge case tests |
| R-05 | Submodule refactor breaks col-002 | Medium | Existing col-002 tests pass unchanged + regression test |
| R-06 | RetrospectiveReport serde compat | Medium | types::test_report_serde_default_compat |
| R-07 | default_rules() signature change | Low | Compile-time verification + integration test |
| R-08 | Cold restart false positives | Low | session::test_cold_restart_new_files_only |
| R-09 | Post-completion boundary detection | Medium | session::test_post_completion_no_taskupdate |
| R-10 | Self-comparison in baseline | Medium | Integration test: verify current feature excluded |
| R-11 | Output parsing false positives | Low | friction::test_output_parsing_different_base_cmds |
| R-12 | Input field variations | High | Per-rule tests with realistic JSON input structures |

## Cross-Component Test Dependencies

- `detection/mod.rs` must compile first (trait + helpers)
- Category modules are test-independent (each has own test module)
- `baseline.rs` tests are independent of detection tests
- `report.rs` tests depend on types.rs changes (new baseline field)
- Server integration tests depend on all observe crate changes

## Integration Harness Plan

### Suites to Run

Per the suite selection table, col-002b touches server tool logic:
- **smoke** (mandatory gate) -- ~15 tests covering critical paths
- **tools** -- context_retrospective behavior changes
- **lifecycle** -- multi-step flows

### Existing Coverage

The existing `tools` suite already tests `context_retrospective` basic behavior (call with feature_cycle, verify report structure). These tests cover:
- Basic retrospective call/response
- Error when no observation data
- Report structure validation

### Gaps (New Tests Needed)

1. **test_retrospective_baseline_comparison** -- Store 3+ MetricVectors, write observation data, call context_retrospective, verify `baseline_comparison` is present in the response
2. **test_retrospective_insufficient_baseline** -- Store only 2 MetricVectors, verify `baseline_comparison` is null/absent
3. **test_retrospective_21_rules** -- Verify the report's hotspot findings can come from all 4 categories (agent, friction, session, scope)

These tests go in `suites/test_tools.py` or `suites/test_lifecycle.py`.

### Running

```bash
# Build binary
cargo build --release

# From product/test/infra-001/
cd product/test/infra-001

# Mandatory smoke
python -m pytest suites/ -v -m smoke --timeout=60

# Relevant suites
python -m pytest suites/test_tools.py -v --timeout=60
python -m pytest suites/test_lifecycle.py -v --timeout=60
```

## Test Infrastructure Reuse

Reuse existing test helpers from col-002:
- `make_pre(ts, tool)` -- create PreToolUse record
- `make_post(ts, tool)` -- create PostToolUse record
- `make_bash_with_input(ts, command)` -- create Bash record with input
- `make_record_in_session(ts, session)` -- create record in specific session

New shared helpers needed:
- `make_read_with_file(ts, file_path)` -- create Read record with file_path input
- `make_write_with_file(ts, file_path)` -- create Write record with file_path input
- `make_edit_with_file(ts, file_path)` -- create Edit record with file_path input
- `make_subagent_start(ts, agent_type)` -- create SubagentStart record
- `make_subagent_stop(ts)` -- create SubagentStop record
- `make_task_update(ts, task_id, status)` -- create TaskUpdate record with input
- `make_metric_vector(tool_calls, duration, phases)` -- create MetricVector for baseline tests
