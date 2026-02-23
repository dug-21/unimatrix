# Pseudocode: C2 Validation Extensions

## File: `crates/unimatrix-server/src/validation.rs`

### New Import

```
use crate::tools::{CorrectParams, DeprecateParams, StatusParams, BriefingParams};
```

### New Constants

```
const MAX_REASON_LEN: usize = 1_000;
const MAX_FEATURE_LEN: usize = 100;
const MAX_ROLE_LEN: usize = 100;
const MAX_TASK_LEN: usize = 1_000;
const DEFAULT_MAX_TOKENS: usize = 3_000;
const MIN_MAX_TOKENS: usize = 500;
const MAX_MAX_TOKENS: usize = 10_000;
```

### New Function: `validate_correct_params`

```
pub fn validate_correct_params(params: &CorrectParams) -> Result<(), ServerError>:
    // original_id: validated separately via validated_id (non-negative)
    validated_id(params.original_id)?

    // content: required, max 50000, allow newlines/tabs
    validate_string_field("content", &params.content, MAX_CONTENT_LEN, true)?

    // reason: optional, max 1000, allow newlines/tabs
    if let Some(reason) = &params.reason:
        validate_string_field("reason", reason, MAX_REASON_LEN, true)?

    // topic: optional override, max 100, no control chars
    if let Some(topic) = &params.topic:
        validate_string_field("topic", topic, MAX_TOPIC_LEN, false)?

    // category: optional override, max 50, no control chars
    if let Some(category) = &params.category:
        validate_string_field("category", category, MAX_CATEGORY_LEN, false)?

    // tags: optional override, max 20 tags, each max 50
    validate_optional_tags(&params.tags)?

    // title: optional, max 200, allow newlines/tabs
    if let Some(title) = &params.title:
        validate_string_field("title", title, MAX_TITLE_LEN, true)?

    return Ok(())
```

### New Function: `validate_deprecate_params`

```
pub fn validate_deprecate_params(params: &DeprecateParams) -> Result<(), ServerError>:
    // id: validated separately via validated_id
    validated_id(params.id)?

    // reason: optional, max 1000, allow newlines/tabs
    if let Some(reason) = &params.reason:
        validate_string_field("reason", reason, MAX_REASON_LEN, true)?

    return Ok(())
```

### New Function: `validate_status_params`

```
pub fn validate_status_params(params: &StatusParams) -> Result<(), ServerError>:
    // topic: optional filter, max 100, no control chars
    if let Some(topic) = &params.topic:
        validate_string_field("topic", topic, MAX_TOPIC_LEN, false)?

    // category: optional filter, max 50, no control chars
    if let Some(category) = &params.category:
        validate_string_field("category", category, MAX_CATEGORY_LEN, false)?

    return Ok(())
```

### New Function: `validate_briefing_params`

```
pub fn validate_briefing_params(params: &BriefingParams) -> Result<(), ServerError>:
    // role: required, max 100, no control chars
    validate_string_field("role", &params.role, MAX_ROLE_LEN, false)?

    // task: required, max 1000, allow newlines/tabs
    validate_string_field("task", &params.task, MAX_TASK_LEN, true)?

    // feature: optional, max 100, no control chars
    if let Some(feature) = &params.feature:
        validate_string_field("feature", feature, MAX_FEATURE_LEN, false)?

    return Ok(())
```

### New Function: `validated_max_tokens`

```
pub fn validated_max_tokens(max_tokens: Option<i64>) -> Result<usize, ServerError>:
    match max_tokens:
        None => return Ok(DEFAULT_MAX_TOKENS)  // 3000
        Some(v) if v < MIN_MAX_TOKENS as i64 =>
            return Err(InvalidInput { field: "max_tokens", reason: "minimum is 500" })
        Some(v) if v > MAX_MAX_TOKENS as i64 =>
            return Err(InvalidInput { field: "max_tokens", reason: "maximum is 10000" })
        Some(v) => return Ok(v as usize)
```
