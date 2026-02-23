# Test Plan: C3 Response Formatting Extensions

## File: `crates/unimatrix-server/src/response.rs`

### New Tests: format_correct_success

1. **test_format_correct_success_summary** (NEW)
   - Summary format: contains "Corrected #", original id, correction id

2. **test_format_correct_success_markdown** (NEW)
   - Markdown format: contains "Correction Applied", both entry titles

3. **test_format_correct_success_json** (NEW)
   - JSON format: parse, verify "corrected": true, both entries present

4. **test_format_correct_success_original_shows_deprecated** (NEW)
   - JSON format: verify original entry shows status "deprecated"

### New Tests: format_deprecate_success

5. **test_format_deprecate_success_summary** (NEW)
   - Summary: contains "Deprecated #" and entry title

6. **test_format_deprecate_success_markdown_with_reason** (NEW)
   - Markdown with reason: contains "Reason:" section

7. **test_format_deprecate_success_markdown_no_reason** (NEW)
   - Markdown without reason: no "Reason:" section

8. **test_format_deprecate_success_json** (NEW)
   - JSON: parse, verify "deprecated": true, entry present

9. **test_format_deprecate_success_json_with_reason** (NEW)
   - JSON: verify reason field present

10. **test_format_deprecate_success_json_no_reason** (NEW)
    - JSON: verify reason is null

### New Tests: format_status_report

11. **test_format_status_report_summary** (NEW)
    - Summary: contains "Active:", "Deprecated:", "Proposed:", "Corrections:"

12. **test_format_status_report_markdown** (NEW)
    - Markdown: contains "Entry Counts", "Category Distribution", "Correction Chains", "Security Metrics"

13. **test_format_status_report_json** (NEW)
    - JSON: parse, verify all fields present (total_active, category_distribution, etc.)

14. **test_format_status_report_empty** (NEW)
    - All zeros / empty distributions: formats without error

### New Tests: format_briefing

15. **test_format_briefing_summary** (NEW)
    - Summary: contains role and task info

16. **test_format_briefing_markdown_all_sections** (NEW)
    - Markdown: contains "Conventions", "Duties", "Relevant Context"

17. **test_format_briefing_markdown_search_unavailable** (NEW)
    - search_available=false: contains "search unavailable" note

18. **test_format_briefing_json** (NEW)
    - JSON: parse, verify role, task, conventions, duties, relevant_context arrays

19. **test_format_briefing_empty_sections** (NEW)
    - Empty conventions/duties/context: formats without error

### New Tests: entry_to_json corrections

20. **test_entry_to_json_includes_correction_fields** (NEW)
    - Entry with supersedes/superseded_by/correction_count set
    - Verify these fields appear in the JSON output

### AC Coverage

| AC | Test |
|----|------|
| AC-35 | tests 1-3, 5-10, 11-13, 15-18 (all formats for all tools) |
| AC-36 | tests 1-4 (correct response structure) |
| AC-37 | tests 5-10 (deprecate response structure) |
| AC-38 | tests 11-14 (status report structure) |
| AC-39 | tests 15-19 (briefing sections) |
