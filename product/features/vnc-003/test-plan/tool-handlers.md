# Test Plan: C1 Tool Handler Implementations

## File: `crates/unimatrix-server/src/tools.rs`

### New Tests: Param Deserialization

1. **test_correct_params_required_fields** (NEW)
   - JSON: {"original_id": 42, "content": "corrected"} -> Ok
2. **test_correct_params_all_fields** (NEW)
   - All optional fields populated -> Ok
3. **test_correct_params_missing_required** (NEW)
   - Missing content -> deserialization Err
4. **test_deprecate_params_required_fields** (NEW)
   - JSON: {"id": 42} -> Ok
5. **test_deprecate_params_with_reason** (NEW)
   - {"id": 42, "reason": "outdated"} -> Ok
6. **test_status_params_all_optional** (NEW)
   - JSON: {} -> Ok
7. **test_briefing_params_required_fields** (NEW)
   - {"role": "architect", "task": "design auth"} -> Ok
8. **test_briefing_params_missing_role** (NEW)
   - Missing role -> deserialization Err
9. **test_briefing_params_all_fields** (NEW)
   - All fields populated -> Ok

### Integration Tests (in integration test file or server tests module)

NOTE: These tests use `make_server()` and require tokio async runtime.
Many also require the embedding model to NOT be loaded (for EmbedNotReady scenarios)
or to be loaded (for full flow). Tests are organized by tool.

#### context_correct integration

10. **test_context_correct_capability_denied** (NEW, async)
    - Use restricted agent, verify -32003 error
    - Covers AC-10, R-06

11. **test_context_correct_embed_not_ready** (NEW, async)
    - Do not start embed loading, attempt correct
    - Verify EmbedNotReady error
    - Verify original entry unchanged
    - Covers R-08 scenarios 1-2

12. **test_context_correct_content_scan_rejection** (NEW, async)
    - Correct with injection pattern in content
    - Verify scan rejection error
    - Covers AC-04, R-05 scenario 1

13. **test_context_correct_title_scan_rejection** (NEW, async)
    - Correct with injection pattern in title
    - Verify scan rejection error
    - Covers R-05 scenario 2

14. **test_context_correct_category_validation** (NEW, async)
    - Correct with invalid new category -> error
    - Correct without category (inherit) -> no validation error
    - Covers AC-05, R-09 scenarios 1-2

15. **test_context_correct_nonexistent_id** (NEW, async)
    - Correct with non-existent original_id
    - Verify EntryNotFound error
    - Covers AC-08

16. **test_context_correct_deprecated_entry** (NEW, async)
    - Deprecate entry, then attempt correct
    - Verify error
    - Covers AC-09, R-04

#### context_deprecate integration

17. **test_context_deprecate_capability_denied** (NEW, async)
    - Use restricted agent, verify -32003
    - Covers AC-15

18. **test_context_deprecate_idempotent** (NEW, async)
    - Deprecate twice, both succeed
    - Verify only one audit event
    - Covers AC-13, R-11

19. **test_context_deprecate_nonexistent** (NEW, async)
    - Deprecate non-existent ID
    - Verify EntryNotFound
    - Covers AC-14

#### context_status integration

20. **test_context_status_capability_denied** (NEW, async)
    - Use restricted agent (no Admin), verify -32003
    - Covers AC-22, R-06

21. **test_context_status_empty_db** (NEW, async)
    - Status on empty database
    - Verify all zeros, empty distributions

22. **test_context_status_with_entries** (NEW, async)
    - Insert entries with various categories/topics
    - Verify counts and distributions
    - Covers AC-18, AC-19

23. **test_context_status_after_corrections** (NEW, async)
    - Insert + correct entries
    - Verify correction chain metrics
    - Covers AC-20

24. **test_context_status_security_metrics** (NEW, async)
    - Insert entries with/without created_by, with trust_source
    - Verify trust_source distribution + attribution count
    - Covers AC-21

25. **test_context_status_with_filters** (NEW, async)
    - Status with topic filter, verify only that topic's distribution
    - Covers AC-19

#### context_briefing integration

26. **test_context_briefing_capability_denied** (NEW, async)
    - Use agent without Read, verify -32003
    - Covers AC-29

27. **test_context_briefing_embed_not_ready_fallback** (NEW, async)
    - Do not start embed loading
    - Store conventions and duties, request briefing
    - Verify conventions + duties returned, search_available=false
    - Covers AC-28

28. **test_context_briefing_empty_results** (NEW, async)
    - Briefing for role with no stored entries
    - Verify empty sections, no error

29. **test_context_briefing_token_budget** (NEW, async)
    - Store many entries, request with max_tokens=500
    - Verify output is within budget
    - Covers AC-27, R-07

### Format Tests (all tools)

30. **test_all_v02_tools_accept_format_param** (NEW)
    - Verify CorrectParams, DeprecateParams, StatusParams, BriefingParams
      all have format field
    - Covers AC-35

### AC Coverage Summary

| AC | Test |
|----|------|
| AC-01 | tests 1-3 (param deserialization) |
| AC-04 | test 12 (content scanning) |
| AC-05 | test 14 (category validation) |
| AC-08 | test 15 (EntryNotFound) |
| AC-09 | test 16 (deprecated rejection) |
| AC-10 | test 10 (capability) |
| AC-13 | test 18 (idempotent) |
| AC-14 | test 19 (EntryNotFound) |
| AC-15 | test 17 (capability) |
| AC-18 | test 22 (status counts) |
| AC-19 | test 25 (distributions) |
| AC-20 | test 23 (correction metrics) |
| AC-21 | test 24 (security metrics) |
| AC-22 | test 20 (Admin required) |
| AC-27 | test 29 (token budget) |
| AC-28 | test 27 (embed fallback) |
| AC-29 | test 26 (Read required) |
| AC-35 | test 30 (format param) |
| AC-43 | tests 14, 18, 24 (audit events) |
