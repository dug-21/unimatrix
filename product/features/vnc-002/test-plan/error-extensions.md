# Test Plan: error-extensions (C7)

## Unit Tests

### ErrorData Mapping

1. `test_invalid_input_maps_to_32602` -- InvalidInput variant maps to ERROR_INVALID_PARAMS (-32602), message contains field name and reason
2. `test_content_scan_rejected_maps_to_32006` -- ContentScanRejected maps to ERROR_CONTENT_SCAN_REJECTED (-32006), message contains category and description, does NOT contain matched text
3. `test_invalid_category_maps_to_32007` -- InvalidCategory maps to ERROR_INVALID_CATEGORY (-32007), message lists valid categories

### Display Format

4. `test_display_invalid_input` -- Display output includes field and reason, no Rust type names
5. `test_display_content_scan_rejected` -- Display output includes category and description
6. `test_display_invalid_category` -- Display output includes category and valid list

### Existing Tests Unchanged

All 10 existing error.rs tests must continue to pass without modification.
