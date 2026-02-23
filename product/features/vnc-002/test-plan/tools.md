# Test Plan: tools (C5)

## Integration Tests (require full server setup with store + vector + registry)

### Capability Enforcement (R-05)

1. `test_search_restricted_agent_allowed` -- Restricted agent (Read+Search) calls context_search -> success
2. `test_store_restricted_agent_denied` -- Restricted agent calls context_store -> -32003 with "Write"
3. `test_lookup_restricted_agent_allowed` -- Restricted agent calls context_lookup -> success
4. `test_get_restricted_agent_allowed` -- Restricted agent calls context_get -> success
5. `test_store_privileged_agent_allowed` -- "human" agent calls context_store -> success
6. `test_capability_before_validation` -- Restricted agent with invalid params gets -32003 (not -32602)

### vnc-001 Regression (R-16)

7. All 72 existing tests pass -- verified by `cargo test -p unimatrix-server`

### context_search Integration

8. `test_search_returns_results` -- store entries, search with related query, get results
9. `test_search_respects_k` -- store 10 entries, search with k=3, get 3 results
10. `test_search_with_topic_filter` -- store in 2 topics, filter by one, only matching topic returned (R-10)
11. `test_search_with_category_filter` -- filter by category, only matching returned
12. `test_search_empty_results` -- search for unrelated query, get empty results
13. `test_search_embed_not_ready` -- embed in Loading state, get -32004 (R-06)

### context_lookup Integration

14. `test_lookup_by_id` -- store entry, lookup by id, get that entry
15. `test_lookup_by_id_ignores_filters` -- lookup with id + unrelated topic, returns entry (R-13)
16. `test_lookup_by_topic` -- store in 2 topics, lookup by one topic
17. `test_lookup_default_status_active` -- store active + deprecated, lookup without status -> only active (R-13)
18. `test_lookup_with_status_deprecated` -- explicitly lookup deprecated entries
19. `test_lookup_respects_limit` -- store 15 entries, limit=5, get 5

### context_store Integration

20. `test_store_creates_entry` -- store and verify entry exists with correct fields
21. `test_store_security_fields` -- verify created_by, trust_source, content_hash, version (AC-07)
22. `test_store_embeds_entry` -- verify vector_store contains the entry after store (AC-08)
23. `test_store_invalid_category_rejected` -- unknown category -> -32007 (AC-13)
24. `test_store_content_scan_rejected` -- injection content -> -32006 (AC-12)
25. `test_store_near_duplicate_detected` -- store identical entry twice -> duplicate response (R-02, AC-18)
26. `test_store_distinct_entries_no_duplicate` -- store different entries -> both created
27. `test_store_combined_audit` -- entry and audit in same transaction (AC-20)

### context_get Integration

28. `test_get_existing_entry` -- get by id returns full entry
29. `test_get_nonexistent_entry` -- get invalid id returns -32001 (AC-09)
30. `test_get_negative_id_rejected` -- id=-1 returns -32602 (R-11)

### Format Parameter (R-09)

31. `test_search_format_summary` -- default format returns compact lines
32. `test_search_format_markdown` -- markdown format has framing markers
33. `test_search_format_json` -- json format is valid JSON with similarity
34. `test_store_format_json` -- json store response has "stored": true
35. `test_get_format_all_three` -- get returns content in all three formats
36. `test_invalid_format_rejected` -- format="invalid" returns error

### Audit Logging

37. `test_get_creates_audit_event` -- context_get logs audit (standalone) (AC-21)
38. `test_store_audit_sequential` -- store then get -> sequential audit IDs (AC-22)

### Param Struct Deserialization

39. `test_search_params_with_format` -- format field deserializes
40. `test_store_params_with_format` -- format field deserializes

## Expected: ~40 new tests in tools.rs, cumulative with 8 existing = ~48 total
