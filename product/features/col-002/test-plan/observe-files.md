# Test Plan: observe-files

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-08 (File cleanup deletes active files) | Boundary age testing at 59/60/61 days |
| R-12 (Directory permissions) | Missing dir handling |

## Unit Tests: `crates/unimatrix-observe/src/files.rs`

### discover_sessions

1. **test_discover_sessions_multiple_files** -- Dir with 3 .jsonl files -> 3 SessionFiles
2. **test_discover_sessions_empty_dir** -- Empty dir -> empty vec
3. **test_discover_sessions_nonexistent_dir** -- Missing dir -> empty vec (not error)
4. **test_discover_sessions_ignores_non_jsonl** -- Dir with .txt and .jsonl -> only .jsonl returned
5. **test_discover_sessions_session_id_from_filename** -- "abc-123.jsonl" -> session_id="abc-123"
6. **test_discover_sessions_sorted_by_modified** -- Files with different ages -> sorted oldest first

### identify_expired (R-08)

7. **test_identify_expired_at_threshold** -- File at exactly 60 days old -> included (R-08 scenario 1)
8. **test_identify_expired_below_threshold** -- File at 59 days -> excluded (R-08 scenario 2)
9. **test_identify_expired_above_threshold** -- File at 61 days -> included (R-08 scenario 3)
10. **test_identify_expired_empty_dir** -- No files -> empty vec

### scan_observation_stats (AC-34, AC-35)

11. **test_scan_stats_correct_counts** -- 3 files totaling 1500 bytes -> file_count=3, total_size=1500
12. **test_scan_stats_oldest_file_age** -- Oldest file 30 days old -> oldest_file_age_days=30
13. **test_scan_stats_approaching_cleanup** -- File at 50 days old -> appears in approaching_cleanup (AC-35)
14. **test_scan_stats_empty_dir** -- All zeros
15. **test_scan_stats_no_approaching** -- All files < 45 days -> approaching_cleanup empty
