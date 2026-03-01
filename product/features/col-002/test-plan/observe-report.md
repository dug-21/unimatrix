# Test Plan: observe-report

## Risk Coverage

No high-priority risks. Report assembly is straightforward.

## Unit Tests: `crates/unimatrix-observe/src/report.rs`

### build_report (AC-22)

1. **test_build_report_session_count** -- 3 records from 2 sessions -> session_count=2
2. **test_build_report_total_records** -- 10 records -> total_records=10
3. **test_build_report_is_cached_false** -- Fresh report -> is_cached=false
4. **test_build_report_includes_all_hotspots** -- 3 hotspots provided -> all 3 in report
5. **test_build_report_self_contained** -- Report contains metrics + hotspots + counts (AC-22)
6. **test_build_report_feature_cycle** -- "col-002" -> report.feature_cycle == "col-002"
