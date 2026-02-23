# Test Plan: validation (C1)

## Unit Tests

### String Length Validation (R-04)

1. `test_title_at_max_length` -- 200 chars accepted
2. `test_title_over_max_length` -- 201 chars rejected with field="title"
3. `test_content_at_max_length` -- 50000 chars accepted
4. `test_content_over_max_length` -- 50001 chars rejected with field="content"
5. `test_query_at_max_length` -- 1000 chars accepted
6. `test_query_over_max_length` -- 1001 chars rejected with field="query"
7. `test_topic_at_max_length` -- 100 chars accepted
8. `test_topic_over_max_length` -- 101 chars rejected with field="topic"
9. `test_source_at_max_length` -- 200 chars accepted
10. `test_source_over_max_length` -- 201 chars rejected

### Control Character Validation (R-04)

11. `test_content_allows_newline` -- \n in content passes
12. `test_content_allows_tab` -- \t in content passes
13. `test_topic_rejects_newline` -- \n in topic rejected
14. `test_topic_rejects_null` -- \0 in topic rejected
15. `test_topic_rejects_control_char` -- U+0001 in topic rejected

### Tag Validation (R-04)

16. `test_tags_at_max_count` -- 20 tags accepted
17. `test_tags_over_max_count` -- 21 tags rejected with field="tags"
18. `test_individual_tag_at_max_length` -- 50 char tag accepted
19. `test_individual_tag_over_max_length` -- 51 char tag rejected

### ID Validation (R-11)

20. `test_validated_id_positive` -- id=1 returns Ok(1)
21. `test_validated_id_negative` -- id=-1 returns Err with field="id"
22. `test_validated_id_zero` -- id=0 returns Ok(0) (valid, though entry won't exist)
23. `test_validated_id_max` -- id=i64::MAX returns Ok

### K and Limit Validation

24. `test_validated_k_none_defaults_to_5` -- None returns 5
25. `test_validated_k_positive` -- k=10 returns 10
26. `test_validated_k_zero_rejected` -- k=0 rejected
27. `test_validated_k_negative_rejected` -- k=-1 rejected
28. `test_validated_k_exceeds_max` -- k=101 rejected
29. `test_validated_limit_none_defaults_to_10` -- None returns 10
30. `test_validated_limit_zero_rejected` -- limit=0 rejected

### Status Parsing

31. `test_parse_status_active` -- "active" -> Status::Active
32. `test_parse_status_deprecated` -- "deprecated" -> Status::Deprecated
33. `test_parse_status_proposed` -- "proposed" -> Status::Proposed
34. `test_parse_status_case_insensitive` -- "Active" -> Status::Active
35. `test_parse_status_invalid` -- "invalid" -> Err

### Full Param Validation

36. `test_validate_search_params_minimal` -- just query, passes
37. `test_validate_store_params_minimal` -- content+topic+category, passes
38. `test_validate_store_params_all_fields` -- all fields populated, passes
39. `test_validate_get_params_negative_id` -- id=-1, fails
