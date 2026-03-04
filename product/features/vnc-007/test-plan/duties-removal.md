# Test Plan: Duties Removal (response.rs)

## Test Scenarios

### T-DR-01: Briefing struct has no duties field (AC-09, AC-12)
```
Method: grep -n "duties" response.rs
Assert: No "duties" field in Briefing struct
Assert: No duties references in format_briefing
```

### T-DR-02: format_briefing summary has no duties count (R-07)
```
Call: format_briefing(briefing_with_conventions_and_context, Summary)
Assert: Output contains "Conventions: N | Context: M"
Assert: Output does NOT contain "Duties"
```

### T-DR-03: format_briefing markdown has no duties section (R-07)
```
Call: format_briefing(briefing, Markdown)
Assert: Output does NOT contain "### Duties"
Assert: Output does NOT contain "No duties found"
Assert: Output contains "### Conventions"
Assert: Output contains "### Relevant Context"
```

### T-DR-04: format_briefing JSON has no duties array (R-07)
```
Call: format_briefing(briefing, Json)
Parse JSON output
Assert: No "duties" key in JSON object
Assert: "conventions" key present
Assert: "relevant_context" key present
```

### T-DR-05: Empty briefing — no duties mention in any format
```
Setup: Briefing with empty conventions, empty relevant_context
Call: format_briefing for each format
Assert: No "duties" or "Duties" in any output
```

## Updated Existing Tests

| Test Name | Change Required |
|-----------|----------------|
| `test_format_briefing_summary` | Remove assertion on `Duties: 1` |
| `test_format_briefing_markdown_all_sections` | Remove assertions on `### Duties`, `Duty 1` |
| `test_format_briefing_json` | Remove assertion on `parsed["duties"]` |
| `test_format_briefing_empty_sections` | Remove `duties: vec![]` from constructor, remove `No duties found` assertion |
| `make_briefing` helper | Remove `duties` field |

## Risk Coverage

| Risk | Test(s) | Status |
|------|---------|--------|
| R-07 (Duties removal breakage) | T-DR-02, T-DR-03, T-DR-04, T-DR-05 | Covered |
