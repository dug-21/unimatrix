# Test Plan: extract_explicit_read_ids (knowledge_reuse.rs)

**File**: `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`
**Function**: `pub(crate) fn extract_explicit_read_ids(attributed: &[ObservationRecord]) -> HashSet<u64>`
**Test module**: existing `#[cfg(test)] mod tests` block

---

## Risks Covered

| Risk | AC | Priority |
|------|----|----------|
| R-02: normalize_tool_name omission (silent zero) | AC-06 [GATE], AC-12(d) | High |
| R-10: Filter-based lookup included in explicit reads | AC-04, AC-12(b) | Medium |
| I-04: ObservationRecord.tool field optionality | — | Medium |

---

## Test Helper Required

All tests construct synthetic `ObservationRecord` values. A helper function is needed inside
the test module:

```rust
fn make_obs(event_type: &str, tool: Option<&str>, input: Option<serde_json::Value>)
    -> ObservationRecord
```

This helper populates the minimum required fields; all other fields use defaults. The helper
must NOT go in a new file — it lives inline in the `#[cfg(test)] mod tests` block.

For JSON input encoded as `Value::String` (simulating the hook listener path — confirmed at
`listener.rs:1911`), wrap the object JSON: `Value::String(json_obj.to_string())`.

---

## AC-12 Unit Tests (Five Cases, All Required)

### AC-12(a): context_get observation — explicit read extracted

**Test: `test_extract_explicit_read_ids_context_get_included`**
```
Arrange: attributed = [
    make_obs("PreToolUse", Some("context_get"), Some(Object({"id": 42})))
]
Act:     extract_explicit_read_ids(&attributed)
Assert:  result.contains(&42u64)
         result.len() == 1
```

### AC-12(b): Filter-based context_lookup (no id field) — excluded

**Test: `test_extract_explicit_read_ids_filter_lookup_excluded`**
```
Arrange: attributed = [
    make_obs("PreToolUse", Some("context_lookup"), Some(Object({"query": "some text"})))
]
Act:     extract_explicit_read_ids(&attributed)
Assert:  result.is_empty()
         (no id field → as_u64() returns None, string parse returns None → excluded)
```

Also test `input = {"id": null}`:
```
Arrange: attributed = [
    make_obs("PreToolUse", Some("context_lookup"), Some(Object({"id": null})))
]
Assert:  result.is_empty()
```

### AC-12(c): Single-ID context_lookup (with id field) — included

**Test: `test_extract_explicit_read_ids_single_id_lookup_included`**
```
Arrange: attributed = [
    make_obs("PreToolUse", Some("context_lookup"), Some(Object({"id": 99})))
]
Act:     extract_explicit_read_ids(&attributed)
Assert:  result.contains(&99u64)
         result.len() == 1
```

### AC-12(d) [GATE]: Prefixed tool name matched via normalize_tool_name

**Test: `test_extract_explicit_read_ids_prefixed_context_get_matched`**
```
Arrange: attributed = [
    make_obs("PreToolUse", Some("mcp__unimatrix__context_get"),
             Some(String(r#"{"id": 7}"#.to_string())))
]
Act:     extract_explicit_read_ids(&attributed)
Assert:  result.contains(&7u64)
         result.len() == 1
```

This test is non-negotiable (AC-06 [GATE]). The hook listener path always writes the
`mcp__unimatrix__` prefix. A bare string comparison would return an empty set here.

Also test prefixed context_lookup:
```
Arrange: attributed = [
    make_obs("PreToolUse", Some("mcp__unimatrix__context_lookup"),
             Some(String(r#"{"id": 8}"#.to_string())))
]
Assert:  result.contains(&8u64)
```

### AC-12(e): Empty slice produces empty set

**Test: `test_extract_explicit_read_ids_empty_slice_returns_empty`**
```
Arrange: attributed = []
Act:     extract_explicit_read_ids(&[])
Assert:  result.is_empty()
         (no panic, no error — edge case E-01)
```

---

## AC-16 [GATE]: String-Form ID Handling

**Test: `test_extract_explicit_read_ids_string_form_id_handled`**
```
Arrange: attributed = [
    make_obs("PreToolUse", Some("context_get"),
             Some(Object({"id": 42}))),           // integer form
    make_obs("PreToolUse", Some("context_get"),
             Some(Object({"id": "99"})))           // string form (GetParams compatible)
]
Act:     extract_explicit_read_ids(&attributed)
Assert:  result.contains(&42u64)
         result.contains(&99u64)
         result.len() == 2
```

Both integer form (`as_u64()`) and string form (`as_str().and_then(|s| s.parse().ok())`)
must succeed. Failure mode: if only `as_u64()` is applied, string-form IDs are silently
dropped with no diagnostic.

---

## AC-04: Non-PreToolUse Events Excluded

**Test: `test_extract_explicit_read_ids_non_pretooluse_excluded`**
```
Arrange: attributed = [
    make_obs("PostToolUse", Some("context_get"), Some(Object({"id": 5}))),
    make_obs("PreToolUse",  Some("context_get"), Some(Object({"id": 6})))
]
Act:     extract_explicit_read_ids(&attributed)
Assert:  result.contains(&6u64)
         result.len() == 1  (PostToolUse excluded)
```

---

## AC-03: Non-Read Tools Excluded (context_search)

**Test: `test_extract_explicit_read_ids_search_tool_excluded`**
```
Arrange: attributed = [
    make_obs("PreToolUse", Some("context_search"), Some(Object({"id": 5}))),
    make_obs("PreToolUse", Some("mcp__unimatrix__context_search"), Some(Object({"id": 6})))
]
Act:     extract_explicit_read_ids(&attributed)
Assert:  result.is_empty()
         (context_search excluded even after normalization)
```

---

## Edge Cases

**E-02: Duplicate context_get for same entry ID**
```
Arrange: attributed = [
    make_obs("PreToolUse", Some("context_get"), Some(Object({"id": 42}))),
    make_obs("PreToolUse", Some("context_get"), Some(Object({"id": 42})))
]
Assert:  result.len() == 1  (HashSet deduplication)
```

**E-03: Float ID in input**
```
Arrange: input = {"id": 42.0}
Assert:  result.contains(&42u64)  (if as_u64() succeeds for .0 floats in serde_json)
         input = {"id": 42.5} → result.is_empty()  (non-integer float fails both paths)
```

**I-04: None tool field**
```
Arrange: attributed = [
    make_obs("PreToolUse", None, Some(Object({"id": 5})))
]
Assert:  result.is_empty()
         (tool.as_deref().unwrap_or("") → "" after normalization → no match, no panic)
```

**Hook listener input path (Value::String)**
```
Arrange: input = Some(Value::String(r#"{"id": 42}"#.to_string()))
         (not Value::Object — the hook listener wraps raw JSON as a string)
Assert:  result.contains(&42u64)
         (two-branch parse handles this form correctly)
```

---

## Expected Test Count Delta

- 5 AC-12 tests (required)
- 1 AC-16 test (required, GATE)
- 4 additional edge/exclusion tests (AC-03, AC-04, E-02, I-04)
- Total: +10 unit tests in `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` test module
