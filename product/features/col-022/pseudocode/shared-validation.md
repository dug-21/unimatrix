# col-022: shared-validation -- Pseudocode

## Purpose

Single validation function for cycle parameters, called by both the MCP tool handler (C1) and the hook handler (C2). Prevents validation divergence (ADR-004, SR-07). Also defines shared event-type constants used by hook and listener to avoid magic-string coupling (R-04).

## File: `crates/unimatrix-server/src/infra/validation.rs`

### New Constants

```
// Shared event type constants (ADR-001, R-04 mitigation)
pub const CYCLE_START_EVENT: &str = "cycle_start";
pub const CYCLE_STOP_EVENT: &str = "cycle_stop";

// Validation limits
const MAX_CYCLE_TOPIC_LEN: usize = 128;
const MAX_KEYWORD_LEN: usize = 64;
const MAX_KEYWORDS_COUNT: usize = 5;
```

### New Types

```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleType {
    Start,
    Stop,
}

#[derive(Debug, Clone)]
pub struct ValidatedCycleParams {
    pub cycle_type: CycleType,
    pub topic: String,
    pub keywords: Vec<String>,
}
```

### New Function: `validate_cycle_params`

Signature (from architecture Integration Surface):
```
pub fn validate_cycle_params(
    type_str: &str,
    topic: &str,
    keywords: Option<&[String]>,
) -> Result<ValidatedCycleParams, String>
```

Note: returns `Result<_, String>` not `Result<_, ServerError>`. The hook handler needs a plain string error (it does not use ServerError). The MCP tool wraps the String error into a tool error response.

#### Pseudocode

```
fn validate_cycle_params(type_str, topic, keywords):
    // Step 1: Validate type
    cycle_type = match type_str.to_lowercase():
        "start" => CycleType::Start
        "stop"  => CycleType::Stop
        other   => return Err("invalid type '{other}': must be 'start' or 'stop'")

    // Step 2: Validate topic
    if topic.is_empty():
        return Err("topic must not be empty")

    // Sanitize: strip control chars, truncate to 128
    // Reuse the same logic as sanitize_metadata_field in listener.rs
    clean_topic = topic.chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .take(MAX_CYCLE_TOPIC_LEN)
        .collect::<String>()

    if clean_topic.is_empty():
        return Err("topic contains only invalid characters")

    if clean_topic.len() > MAX_CYCLE_TOPIC_LEN:
        return Err("topic exceeds 128 characters")

    // Structural check: must look like a feature ID (contains hyphen, safe chars)
    if !is_valid_feature_id(&clean_topic):
        return Err("topic is not a valid feature cycle identifier")

    // Step 3: Validate keywords
    validated_keywords = Vec::new()
    if let Some(kw_slice) = keywords:
        for kw in kw_slice.iter().take(MAX_KEYWORDS_COUNT):
            // Truncate individual keywords to 64 chars (FR-06: truncate, not reject)
            truncated = if kw.len() > MAX_KEYWORD_LEN:
                kw[..MAX_KEYWORD_LEN].to_string()  // Note: must handle UTF-8 boundary
            else:
                kw.clone()

            // Skip empty strings after truncation
            if !truncated.is_empty():
                validated_keywords.push(truncated)
        // Silently ignore keywords beyond index 4 (FR-05)

    return Ok(ValidatedCycleParams {
        cycle_type,
        topic: clean_topic,
        keywords: validated_keywords,
    })
```

**UTF-8 truncation note**: Rust's `&str[..64]` panics on non-char-boundary. Use `.chars().take(64).collect::<String>()` or `.char_indices()` to find the safe boundary at or before byte 64.

### New Function: `is_valid_feature_id` (duplicated from unimatrix-observe)

```
fn is_valid_feature_id(s: &str) -> bool:
    // Duplicated from unimatrix-observe::attribution (private fn).
    // Structural check: non-empty, max 128 chars, contains hyphen,
    // no leading/trailing hyphens, only [a-zA-Z0-9\-_.]
    !s.is_empty()
        && s.len() <= 128
        && s.contains('-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
```

This is a private helper within `validation.rs`, not pub. Only `validate_cycle_params` calls it.

### New Import in `validation.rs`

The existing `use crate::mcp::tools::{...}` import block must add `CycleParams` once that type exists.

## Error Handling

- All errors are returned as `String` (not `ServerError`), because the hook handler does not use `ServerError`.
- The MCP tool handler wraps the `String` into a tool error response.
- The hook handler logs the error at `warn` and falls through to generic `RecordEvent`.

## Key Test Scenarios

1. **Valid start**: `validate_cycle_params("start", "col-022", Some(&["kw1", "kw2"]))` returns `Ok` with `CycleType::Start`, topic "col-022", keywords ["kw1", "kw2"]
2. **Valid stop**: `validate_cycle_params("stop", "col-022", None)` returns `Ok` with `CycleType::Stop`, empty keywords
3. **Invalid type**: `validate_cycle_params("pause", "col-022", None)` returns `Err` containing "must be 'start' or 'stop'"
4. **Empty type**: `validate_cycle_params("", ...)` returns `Err`
5. **Empty topic**: `validate_cycle_params("start", "", None)` returns `Err` containing "must not be empty"
6. **Topic at 128 chars**: accepted
7. **Topic at 129 chars**: rejected after sanitize (sanitize truncates to 128, then passes)
8. **Topic with control chars**: sanitized, then checked for emptiness
9. **Topic without hyphen** (e.g., "foobar"): rejected by `is_valid_feature_id`
10. **6 keywords truncated to 5**: only first 5 kept
11. **Keyword at 64 chars**: accepted as-is
12. **Keyword at 65 chars**: truncated to 64
13. **Empty keyword string**: filtered out
14. **Keywords with unicode**: truncation respects char boundaries
15. **Case-insensitive type**: "Start", "START", "start" all accepted
16. **Null/None keywords**: returns empty vec
