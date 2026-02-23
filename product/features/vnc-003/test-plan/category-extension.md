# Test Plan: C4 Category Extension

## File: `crates/unimatrix-server/src/categories.rs`

### Updated Tests

1. **test_validate_unknown_rejected** (MODIFY)
   - Change `assert_eq!(valid_categories.len(), 6)` to `8`

2. **test_list_categories_sorted** (MODIFY)
   - Change `assert_eq!(list.len(), 6)` to `8`

3. **test_error_lists_all_valid_categories** (MODIFY)
   - Add: `assert!(valid_categories.contains(&"duties".to_string()))`
   - Add: `assert!(valid_categories.contains(&"reference".to_string()))`

### New Tests

4. **test_validate_duties** (NEW)
   - `CategoryAllowlist::new().validate("duties")` -> Ok

5. **test_validate_reference** (NEW)
   - `CategoryAllowlist::new().validate("reference")` -> Ok

### AC Coverage

| AC | Test |
|----|------|
| AC-34 | test_validate_duties + test_validate_reference |
