# Test Plan: categories (C4)

## Unit Tests

### Initial Set Validation (R-08)

1. `test_validate_outcome` -- "outcome" passes
2. `test_validate_lesson_learned` -- "lesson-learned" passes
3. `test_validate_decision` -- "decision" passes
4. `test_validate_convention` -- "convention" passes
5. `test_validate_pattern` -- "pattern" passes
6. `test_validate_procedure` -- "procedure" passes

### Rejection

7. `test_validate_unknown_rejected` -- "unknown" rejected with InvalidCategory listing 6 valid categories
8. `test_validate_case_sensitive` -- "Convention" (uppercase) rejected
9. `test_validate_empty_string_rejected` -- "" rejected

### Runtime Extension

10. `test_add_category_then_validate` -- add "custom", then validate "custom" passes
11. `test_list_categories_sorted` -- list returns sorted alphabetical order

### Error Message

12. `test_error_lists_all_valid_categories` -- InvalidCategory error contains all 6 initial categories sorted
