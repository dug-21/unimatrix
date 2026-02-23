# Pseudocode: error-extensions (C7)

## File: `crates/unimatrix-server/src/error.rs`

### New Constants

```
const ERROR_CONTENT_SCAN_REJECTED: ErrorCode = ErrorCode(-32006)
const ERROR_INVALID_CATEGORY: ErrorCode = ErrorCode(-32007)
```

### New Variants on ServerError

```
enum ServerError {
    // ... existing 8 variants unchanged ...

    InvalidInput {
        field: String,
        reason: String,
    },

    ContentScanRejected {
        category: String,
        description: String,
    },

    InvalidCategory {
        category: String,
        valid_categories: Vec<String>,
    },
}
```

### Display Implementation Extensions

```
fn fmt(self, f):
    match self:
        InvalidInput { field, reason } =>
            write "invalid parameter '{field}': {reason}"
        ContentScanRejected { category, description } =>
            write "content rejected: {description} ({category} detected)"
        InvalidCategory { category, valid_categories } =>
            write "unknown category '{category}'. Valid: {valid_categories joined by comma}"
```

### ErrorData From Implementation Extensions

```
fn from(err: ServerError) -> ErrorData:
    match err:
        InvalidInput { field, reason } =>
            ErrorData(ERROR_INVALID_PARAMS, "Invalid parameter '{field}': {reason}")
        ContentScanRejected { category, description } =>
            ErrorData(ERROR_CONTENT_SCAN_REJECTED,
                "Content rejected: {description} ({category} detected). Remove the flagged content and retry.")
        InvalidCategory { category, valid_categories } =>
            ErrorData(ERROR_INVALID_CATEGORY,
                "Unknown category '{category}'. Valid categories: {sorted list}.")
        // ... all existing arms unchanged ...
```

### Key Constraints
- Existing match arms in Display and From<ServerError> for ErrorData are NOT modified
- Error messages are actionable (tell the agent what to do)
- Content scan error does NOT include matched text (avoid leaking sensitive content)
- Invalid category error lists all valid categories (agent guidance)
