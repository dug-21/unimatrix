# Test Plan: response (C3)

## Unit Tests

### Format Parsing (R-09)

1. `test_parse_format_none_defaults_to_summary` -- None returns Summary
2. `test_parse_format_summary` -- "summary" returns Summary
3. `test_parse_format_markdown` -- "markdown" returns Markdown
4. `test_parse_format_json` -- "json" returns Json
5. `test_parse_format_invalid` -- "invalid" returns error with field="format"
6. `test_parse_format_case_insensitive` -- "JSON" returns Json

### Single Entry Formatting

7. `test_format_single_entry_summary` -- compact line with id, title, category, tags
8. `test_format_single_entry_markdown` -- has [KNOWLEDGE DATA] markers around content
9. `test_format_single_entry_json` -- valid JSON with all fields

### Search Results Formatting

10. `test_format_search_results_summary` -- one line per result with similarity
11. `test_format_search_results_markdown` -- multiple framed sections with similarity
12. `test_format_search_results_json` -- JSON array with similarity field per entry

### Lookup Results Formatting

13. `test_format_lookup_results_summary` -- one line per result, no similarity
14. `test_format_lookup_results_json` -- JSON array without similarity

### Store Success Formatting

15. `test_format_store_success_summary` -- "Stored #N | title | category"
16. `test_format_store_success_json` -- JSON with "stored": true

### Duplicate Found Formatting (R-02)

17. `test_format_duplicate_summary` -- includes "duplicate: true" and similarity
18. `test_format_duplicate_markdown` -- includes "Near-Duplicate Detected"
19. `test_format_duplicate_json` -- JSON with "duplicate": true, "similarity", "existing_entry"

### Empty Results

20. `test_format_empty_results_summary` -- helpful message
21. `test_format_empty_results_json` -- "[]"

### Output Framing (R-07)

22. `test_markdown_has_knowledge_data_markers` -- markdown contains [KNOWLEDGE DATA] and [/KNOWLEDGE DATA]
23. `test_summary_has_no_markers` -- summary does NOT contain [KNOWLEDGE DATA]
24. `test_json_has_no_markers` -- json does NOT contain [KNOWLEDGE DATA]
25. `test_content_with_marker_in_body` -- entry content containing "[/KNOWLEDGE DATA]" still formatted correctly
