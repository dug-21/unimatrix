# Test Plan: C2 Validation Extensions

## File: `crates/unimatrix-server/src/validation.rs`

### New Tests: validate_correct_params

1. **test_validate_correct_params_minimal** (NEW)
   - original_id=1, content="valid", all others None -> Ok

2. **test_validate_correct_params_all_fields** (NEW)
   - All optional fields populated with valid values -> Ok

3. **test_validate_correct_params_negative_id** (NEW)
   - original_id=-1 -> Err InvalidInput

4. **test_validate_correct_params_content_too_long** (NEW)
   - content = "a".repeat(50001) -> Err

5. **test_validate_correct_params_reason_too_long** (NEW)
   - reason = "a".repeat(1001) -> Err

6. **test_validate_correct_params_content_control_chars** (NEW)
   - content with \x01 -> Err (but \n and \t allowed)

### New Tests: validate_deprecate_params

7. **test_validate_deprecate_params_minimal** (NEW)
   - id=1, reason=None -> Ok

8. **test_validate_deprecate_params_negative_id** (NEW)
   - id=-1 -> Err

9. **test_validate_deprecate_params_reason_valid** (NEW)
   - id=1, reason=Some("outdated") -> Ok

10. **test_validate_deprecate_params_reason_too_long** (NEW)
    - reason = "a".repeat(1001) -> Err

### New Tests: validate_status_params

11. **test_validate_status_params_empty** (NEW)
    - All None -> Ok

12. **test_validate_status_params_topic_too_long** (NEW)
    - topic = "a".repeat(101) -> Err

13. **test_validate_status_params_category_control_chars** (NEW)
    - category with \x00 -> Err

### New Tests: validate_briefing_params

14. **test_validate_briefing_params_minimal** (NEW)
    - role="architect", task="design auth" -> Ok

15. **test_validate_briefing_params_role_too_long** (NEW)
    - role = "a".repeat(101) -> Err

16. **test_validate_briefing_params_task_too_long** (NEW)
    - task = "a".repeat(1001) -> Err

17. **test_validate_briefing_params_feature_valid** (NEW)
    - feature=Some("vnc-003") -> Ok

18. **test_validate_briefing_params_feature_too_long** (NEW)
    - feature = "a".repeat(101) -> Err

### New Tests: validated_max_tokens

19. **test_validated_max_tokens_none_default** (NEW)
    - None -> Ok(3000)

20. **test_validated_max_tokens_valid** (NEW)
    - Some(1000) -> Ok(1000)

21. **test_validated_max_tokens_min_boundary** (NEW)
    - Some(500) -> Ok(500)
    - Some(499) -> Err

22. **test_validated_max_tokens_max_boundary** (NEW)
    - Some(10000) -> Ok(10000)
    - Some(10001) -> Err

### AC Coverage

| AC | Test |
|----|------|
| AC-01 | test_validate_correct_params (param structure) |
| AC-11 | test_validate_deprecate_params |
| AC-17 | test_validate_status_params |
| AC-23 | test_validate_briefing_params |
| AC-27 | test_validated_max_tokens (budget validation) |
