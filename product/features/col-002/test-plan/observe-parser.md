# Test Plan: observe-parser

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-01 (JSONL parsing drops records) | Malformed lines, mixed valid/invalid, empty file, all-malformed |
| R-03 (Timestamp parsing edge cases) | Epoch boundaries, leap year, midnight, invalid format |
| R-13 (Large session files) | 10K record file parsing |

## Unit Tests: `crates/unimatrix-observe/src/parser.rs`

### parse_timestamp (R-03, AC-06)

1. **test_parse_timestamp_standard** -- "2024-06-15T14:30:45.123Z" -> correct epoch millis
2. **test_parse_timestamp_epoch_zero** -- "1970-01-01T00:00:00.000Z" -> 0
3. **test_parse_timestamp_2038_boundary** -- "2038-01-19T03:14:07.000Z" -> 2147483647000
4. **test_parse_timestamp_leap_year** -- "2024-02-29T12:00:00.000Z" -> valid
5. **test_parse_timestamp_midnight** -- "2024-01-01T00:00:00.000Z" -> correct
6. **test_parse_timestamp_end_of_day** -- "2024-12-31T23:59:59.999Z" -> correct
7. **test_parse_timestamp_invalid_format** -- "2024/01/01 12:00:00" -> Err
8. **test_parse_timestamp_no_z_suffix** -- "2024-01-01T12:00:00.000" -> Err
9. **test_parse_timestamp_invalid_month** -- month=13 -> Err
10. **test_parse_timestamp_feb_29_non_leap** -- "2023-02-29T00:00:00.000Z" -> Err

### parse_line

11. **test_parse_line_pre_tool_use** -- Valid PreToolUse JSON -> correct ObservationRecord
12. **test_parse_line_post_tool_use** -- PostToolUse with response_size and snippet
13. **test_parse_line_subagent_start** -- agent_type mapped to tool, prompt_snippet to input (FR-02.5)
14. **test_parse_line_subagent_stop** -- tool=None, input=None (platform constraint)
15. **test_parse_line_malformed_json** -- "{garbage}" -> None
16. **test_parse_line_unknown_hook** -- hook="Unknown" -> None
17. **test_parse_line_missing_session_id** -- No session_id field -> None

### parse_session_file (R-01)

18. **test_parse_session_file_valid** -- File with 3 valid lines -> 3 records
19. **test_parse_session_file_mixed** -- 5 lines: 3 valid, 2 malformed -> 3 records (R-01 scenario 1)
20. **test_parse_session_file_empty** -- Empty file -> empty vec (R-01 scenario 3)
21. **test_parse_session_file_all_malformed** -- All invalid -> empty vec (R-01 scenario 4)
22. **test_parse_session_file_sorted_by_timestamp** -- Out-of-order timestamps -> sorted result
23. **test_parse_session_file_large** -- 10K records -> parses correctly under 2s (R-13)
24. **test_parse_session_file_nonexistent** -- Missing file -> Io error
