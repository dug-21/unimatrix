# Test Plan: observe-metrics

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-11 (Phase name extraction) | Standard format, no colon, multiple colons, empty prefix |

## Unit Tests: `crates/unimatrix-observe/src/metrics.rs`

### compute_metric_vector (AC-15, AC-27)

1. **test_compute_metric_vector_basic** -- Synthetic records -> MetricVector with correct total_tool_calls, session_count, duration
2. **test_compute_metric_vector_with_hotspots** -- 2 friction + 1 session hotspot -> correct category counts
3. **test_compute_metric_vector_empty_records** -- Empty input -> all-zero MetricVector
4. **test_compute_metric_vector_includes_computed_at** -- computed_at matches provided timestamp (AC-27)

### Phase Extraction (R-11, AC-16)

5. **test_extract_phase_standard** -- Value::String("3a: Pseudocode") -> "3a" (AC-16)
6. **test_extract_phase_no_colon** -- "Implementation work" -> None (R-11 scenario 2)
7. **test_extract_phase_multiple_colons** -- "3b: Code: implement parser" -> "3b" (R-11 scenario 3)
8. **test_extract_phase_empty_prefix** -- ": Just a description" -> None (R-11 scenario 4)

### Phase Metrics

9. **test_compute_phases_single_phase** -- SubagentStart with "3a: Design" followed by records -> phase "3a" with correct counts
10. **test_compute_phases_multiple_phases** -- Two SubagentStart events -> two phase entries
11. **test_compute_phases_no_subagent_events** -- No SubagentStart records -> empty phases map

### Specific Metric Computations

12. **test_permission_friction_count** -- Pre/Post differential summed across tools
13. **test_sleep_workaround_count** -- Bash records with sleep commands
14. **test_total_context_loaded_kb** -- Sum of PostToolUse response_size in KB
15. **test_knowledge_entries_stored** -- context_store PreToolUse count
