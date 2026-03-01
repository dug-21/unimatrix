# Test Plan: observe-detection

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-05 (DetectionRule extensibility) | Custom rule via trait, engine collects all |
| R-10 (Permission retries false positives) | No retries, threshold, multiple tools |

## Unit Tests: `crates/unimatrix-observe/src/detection.rs`

### Detection Engine

1. **test_detect_hotspots_collects_from_all_rules** -- 3 rules each returning 1 finding -> 3 total (FR-05.4)
2. **test_detect_hotspots_empty_records** -- No records -> no findings
3. **test_detect_hotspots_custom_rule** -- Implement DetectionRule on test struct, pass to engine, verify runs (AC-18, R-05)

### PermissionRetriesRule (AC-11, R-10)

4. **test_permission_retries_no_retries** -- 3 Pre + 3 Post for tool X -> no finding (R-10 scenario 1)
5. **test_permission_retries_above_threshold** -- 5 Pre + 2 Post for tool X -> finding with measured=3 (AC-11)
6. **test_permission_retries_multiple_tools** -- Tool A (5 Pre + 2 Post), Tool B (3 Pre + 3 Post) -> finding only for A (R-10 scenario 3)
7. **test_permission_retries_evidence_included** -- Finding has non-empty evidence vec (AC-14)
8. **test_permission_retries_exactly_threshold** -- 4 Pre + 2 Post (retries=2, threshold=2) -> no finding (boundary)

### SessionTimeoutRule (AC-12)

9. **test_session_timeout_3_hour_gap** -- Records with 3h gap -> finding (AC-12)
10. **test_session_timeout_1_hour_gap** -- Records with 1h gap -> no finding
11. **test_session_timeout_exactly_2_hours** -- 2h gap -> no finding (boundary, must exceed)
12. **test_session_timeout_evidence_has_gap_timestamps** -- Finding evidence includes gap start/end

### SleepWorkaroundsRule (AC-13)

13. **test_sleep_workarounds_detected** -- Bash record with "sleep 5" -> finding (AC-13)
14. **test_sleep_workarounds_no_bash** -- Only Read/Write records -> no finding
15. **test_sleep_workarounds_sleep_in_compound** -- "echo hello && sleep 10" -> detected
16. **test_sleep_workarounds_not_substring** -- "sleeping" is not a sleep command -> no finding

### Finding Structure (AC-14)

17. **test_finding_has_all_fields** -- Any detection -> category, severity, rule_name, claim, measured, threshold, evidence all populated
