# Test Plan: C4 — Assertion Helpers

## Scope

The assertion layer (ADR-001) is tested implicitly by every test that uses it. A bug in assertions.py would cause widespread, obvious failures across all suites.

## Validation Through Usage

| Function | Used By | Failure Visibility |
|----------|---------|-------------------|
| assert_tool_success | Every successful tool call in every suite | All tests fail if broken |
| assert_tool_error | Error path tests in Tools, Security, Protocol | Error tests fail if broken |
| parse_entry | Tools (get, store), Lifecycle, Edge Cases | Entry data tests fail |
| parse_entries | Tools (search, lookup), Volume, Confidence | List tests fail |
| assert_search_contains | Tools, Lifecycle, Volume, Confidence | Search tests fail |
| assert_search_not_contains | Quarantine, Deprecate, Security | Exclusion tests fail |
| extract_entry_id | Every test that stores then references by ID | All lifecycle tests fail |
| parse_status_report | Status tool tests, Volume | Status tests fail |

## Format Coverage

| Format | Exercises |
|--------|-----------|
| json | Primary format for structured assertions. Used by ~80% of tests. |
| summary | Default format tested in P-15, selected tool tests |
| markdown | Tested in P-15, selected tool tests |

## Explicit Format Tests

- P-15: test_json_format_responses_parseable — all tools with format=json
- T-16: test_search_all_formats — search in summary, markdown, json
- T-30: test_get_all_formats — get in all formats

## Risk Coverage

| Risk | Assertion Responsibility | Test |
|------|------------------------|------|
| R-03 | Centralized parsing absorbs format changes | Implicit: all tests pass through assertions.py |
| SR-07 | Single point of change | Architecture decision, not testable per se |
