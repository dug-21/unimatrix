# Pseudocode: validation (C1)

## File: `crates/unimatrix-server/src/validation.rs`

### Constants

```
const MAX_TITLE_LEN: usize = 200
const MAX_CONTENT_LEN: usize = 50_000
const MAX_TOPIC_LEN: usize = 100
const MAX_CATEGORY_LEN: usize = 50
const MAX_TAG_LEN: usize = 50
const MAX_TAGS_COUNT: usize = 20
const MAX_QUERY_LEN: usize = 1_000
const MAX_SOURCE_LEN: usize = 200
const MAX_K: usize = 100
const MAX_LIMIT: usize = 100
const DEFAULT_K: usize = 5
const DEFAULT_LIMIT: usize = 10
```

### Helper Functions

```
fn check_length(field_name: &str, value: &str, max: usize) -> Result<(), ServerError>:
    if value.len() > max:
        return Err(InvalidInput { field: field_name, reason: "exceeds {max} characters" })
    Ok(())

fn check_control_chars(field_name: &str, value: &str, allow_newline_tab: bool) -> Result<(), ServerError>:
    for ch in value.chars():
        if ch as u32 <= 0x1F:
            if allow_newline_tab and (ch == '\n' or ch == '\t'):
                continue
            return Err(InvalidInput { field: field_name, reason: "contains control character U+{hex}" })
    Ok(())

fn validate_string_field(field_name: &str, value: &str, max: usize, allow_newline_tab: bool) -> Result<(), ServerError>:
    check_length(field_name, value, max)?
    check_control_chars(field_name, value, allow_newline_tab)?
    Ok(())
```

### Public Functions

```
fn validated_id(id: i64) -> Result<u64, ServerError>:
    if id < 0:
        return Err(InvalidInput { field: "id", reason: "must be non-negative" })
    Ok(id as u64)

fn validated_k(k: Option<i64>) -> Result<usize, ServerError>:
    match k:
        None => Ok(DEFAULT_K)
        Some(v) if v <= 0 => Err(InvalidInput { field: "k", reason: "must be positive" })
        Some(v) if v > MAX_K as i64 => Err(InvalidInput { field: "k", reason: "exceeds maximum {MAX_K}" })
        Some(v) => Ok(v as usize)

fn validated_limit(limit: Option<i64>) -> Result<usize, ServerError>:
    match limit:
        None => Ok(DEFAULT_LIMIT)
        Some(v) if v <= 0 => Err(InvalidInput { field: "limit", reason: "must be positive" })
        Some(v) if v > MAX_LIMIT as i64 => Err(InvalidInput { field: "limit", reason: "exceeds maximum {MAX_LIMIT}" })
        Some(v) => Ok(v as usize)

fn parse_status(s: &str) -> Result<Status, ServerError>:
    match s.to_lowercase().as_str():
        "active" => Ok(Status::Active)
        "deprecated" => Ok(Status::Deprecated)
        "proposed" => Ok(Status::Proposed)
        _ => Err(InvalidInput { field: "status", reason: "must be active, deprecated, or proposed" })

fn validate_search_params(params: &SearchParams) -> Result<(), ServerError>:
    validate_string_field("query", &params.query, MAX_QUERY_LEN, false)?
    if let Some(topic) = &params.topic:
        validate_string_field("topic", topic, MAX_TOPIC_LEN, false)?
    if let Some(category) = &params.category:
        validate_string_field("category", category, MAX_CATEGORY_LEN, false)?
    if let Some(tags) = &params.tags:
        if tags.len() > MAX_TAGS_COUNT:
            return Err(InvalidInput { field: "tags", reason: "exceeds {MAX_TAGS_COUNT} tags" })
        for tag in tags:
            validate_string_field("tags", tag, MAX_TAG_LEN, false)?
    Ok(())

fn validate_lookup_params(params: &LookupParams) -> Result<(), ServerError>:
    if let Some(topic) = &params.topic:
        validate_string_field("topic", topic, MAX_TOPIC_LEN, false)?
    if let Some(category) = &params.category:
        validate_string_field("category", category, MAX_CATEGORY_LEN, false)?
    if let Some(tags) = &params.tags:
        if tags.len() > MAX_TAGS_COUNT:
            return Err(InvalidInput { field: "tags", reason: "exceeds {MAX_TAGS_COUNT} tags" })
        for tag in tags:
            validate_string_field("tags", tag, MAX_TAG_LEN, false)?
    if let Some(status) = &params.status:
        parse_status(status)?  // validates format
    if let Some(id) = params.id:
        validated_id(id)?  // validates non-negative
    Ok(())

fn validate_store_params(params: &StoreParams) -> Result<(), ServerError>:
    if let Some(title) = &params.title:
        validate_string_field("title", title, MAX_TITLE_LEN, true)?  // allow newline/tab
    validate_string_field("content", &params.content, MAX_CONTENT_LEN, true)?  // allow newline/tab
    validate_string_field("topic", &params.topic, MAX_TOPIC_LEN, false)?
    validate_string_field("category", &params.category, MAX_CATEGORY_LEN, false)?
    if let Some(tags) = &params.tags:
        if tags.len() > MAX_TAGS_COUNT:
            return Err(InvalidInput { field: "tags", reason: "exceeds {MAX_TAGS_COUNT} tags" })
        for tag in tags:
            validate_string_field("tags", tag, MAX_TAG_LEN, false)?
    if let Some(source) = &params.source:
        validate_string_field("source", source, MAX_SOURCE_LEN, false)?
    Ok(())

fn validate_get_params(params: &GetParams) -> Result<(), ServerError>:
    validated_id(params.id)?
    Ok(())
```

### Key Constraints
- All functions are pure (no I/O, no state)
- Control characters U+0000-U+001F rejected except \n and \t in content/title fields
- i64 -> u64 conversion rejects negative values (JSON numbers arrive as i64)
- k defaults to 5, limit defaults to 10, both max 100
- Status parsing is case-insensitive
