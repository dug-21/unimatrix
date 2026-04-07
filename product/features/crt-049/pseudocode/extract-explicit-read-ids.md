# Component 2: extract_explicit_read_ids — `unimatrix-server/src/mcp/knowledge_reuse.rs`

## Purpose

New pure function that filters an in-memory `ObservationRecord` slice and extracts the set
of distinct entry IDs explicitly read by agents via `context_get` or single-ID
`context_lookup` during the cycle. No DB access; no async. Directly unit-testable without
a store fixture (ADR-001).

---

## Imports Required

The existing `knowledge_reuse.rs` already imports `HashSet` from `std::collections`.

New imports needed:
```
use unimatrix_core::observation::ObservationRecord;
// (or the path where ObservationRecord is defined)
use unimatrix_observe::normalize_tool_name;
// (already re-exported from unimatrix_observe; verify exact re-export path in lib.rs)
```

`EventType` comparison: `event_type` is a `String` field on `ObservationRecord`,
not an enum in this codebase. Compare as `record.event_type == "PreToolUse"`.

---

## Function Signature

```
pub(crate) fn extract_explicit_read_ids(
    attributed: &[ObservationRecord],
) -> HashSet<u64>
```

Visibility: `pub(crate)` — accessible from `tools.rs` in the same crate, not exported.
No generics; no async; no error return.

---

## Algorithm

```
FUNCTION extract_explicit_read_ids(attributed: &[ObservationRecord]) -> HashSet<u64>:

    let mut result: HashSet<u64> = HashSet::new()

    FOR each record IN attributed:

        // Condition 1: must be a PreToolUse event
        IF record.event_type != "PreToolUse":
            CONTINUE

        // Condition 2: tool name must normalize to "context_get" or "context_lookup"
        // normalize_tool_name strips "mcp__unimatrix__" prefix
        // tool field is Option<String>; handle None as empty string
        let raw_tool = record.tool.as_deref().unwrap_or("")
        let normalized = normalize_tool_name(raw_tool)
        IF normalized != "context_get" AND normalized != "context_lookup":
            CONTINUE

        // Condition 3 + 4: parse input into a JSON object (two-branch parse)
        // Hook-listener path: input = Some(Value::String(raw_json))
        //   → must call serde_json::from_str to get the object
        // Direct MCP path: input = Some(Value::Object(_))
        //   → use as-is (clone)
        // Any other form (None, Value::Array, etc.) → skip record
        let obj: Option<serde_json::Value> = MATCH record.input:
            Some(Value::Object(_)) => Some(record.input.clone())
            Some(Value::String(s)) => serde_json::from_str(s).ok()
                // from_str failure (malformed JSON) silently yields None → record skipped
            _ => None

        IF obj IS None:
            CONTINUE

        let obj = obj.unwrap()

        // Condition 5: extract id as u64
        // Try integer form first: {"id": 42}
        // Fall back to string form: {"id": "42"}
        // Both forms must be accepted (AC-16 GATE)
        let id_val = &obj["id"]
        let id: Option<u64> = id_val.as_u64()
            .or_else(|| id_val.as_str().and_then(|s| s.parse::<u64>().ok()))

        // If id is None (field absent, null, float non-integer, non-numeric string):
        // → record silently skipped (natural exclusion of filter-path context_lookup)
        IF let Some(n) = id:
            result.insert(n)

    RETURN result

END FUNCTION
```

### Key Notes for Implementation

1. `record.tool` is `Option<String>`. Use `as_deref().unwrap_or("")` to get a `&str`
   before passing to `normalize_tool_name`. Do not unwrap or panic.

2. The two-branch parse is the critical correctness constraint (ADR-001 Correction).
   Hook-listener sourced `ObservationRecord.input` is `Some(Value::String(raw_json))`.
   Indexing a `Value::String` with `["id"]` always returns `Value::Null` — silent zero
   if the branch is not implemented. Evidence: `extract_topic_signal` at `listener.rs:1911`
   uses the identical two-branch pattern.

3. `serde_json::from_str(s).ok()` on malformed JSON silently yields `None` — this is
   correct behavior. Do not log at warn level for malformed input (too noisy).

4. `obj["id"].as_u64()` returns `None` for: null, string, boolean, array, object, and
   non-integer floats. For integer JSON numbers (including `42.0`), it returns `Some(n)`.
   The string fallback handles `{"id": "42"}` (string-form IDs from `GetParams`).

5. The returned `HashSet<u64>` is already deduplicated — an agent calling `context_get`
   for entry 42 ten times produces `{42}`, not a count of 10.

6. Filter-based `context_lookup` calls (no `id` field, or `id` is null) are excluded
   by Condition 5 without any special-casing. `obj["id"]` returns `Value::Null` when
   the field is absent; `as_u64()` returns `None`; string parse also returns `None`.

---

## Error Handling

This function has no error return. All failure modes are handled silently:
- `record.tool == None` → treated as empty string, fails Condition 2, skipped
- `record.input == None` → obj is None, CONTINUE
- Malformed JSON in `Value::String` → `from_str(...).ok()` returns None, CONTINUE
- No `id` field in object → `obj["id"]` is `Value::Null`, both paths return None, skipped
- `id` is null → same as above
- `id` is a float like `42.5` → `as_u64()` returns None, string parse returns None, skipped

No `tracing::warn` or `tracing::debug` inside this function (too noisy for a hot filter path).

---

## Key Test Scenarios

All tests in the `#[cfg(test)]` module in `knowledge_reuse.rs`. Extend the existing module.

### Helper for tests

```
fn make_obs(event_type: &str, tool: Option<&str>, input: Option<serde_json::Value>)
    -> ObservationRecord:
    ObservationRecord {
        event_type: event_type.to_string(),
        tool: tool.map(str::to_string),
        input,
        session_id: "test-session".to_string(),
        // other fields: defaults/zero
    }
```

### AC-12(a) — context_get with integer id

```
Test: test_extract_explicit_reads_context_get
    obs = make_obs("PreToolUse", Some("context_get"), Some(json!({"id": 42})))
    result = extract_explicit_read_ids(&[obs])
    assert result.contains(42)
    assert result.len() == 1
```

### AC-12(b) — filter context_lookup (no id field) excluded

```
Test: test_extract_explicit_reads_filter_lookup_excluded
    obs = make_obs("PreToolUse", Some("context_lookup"), Some(json!({"query": "some text"})))
    result = extract_explicit_read_ids(&[obs])
    assert result.is_empty()
```

### AC-12(c) — single-ID context_lookup (with id) included

```
Test: test_extract_explicit_reads_single_id_lookup
    obs = make_obs("PreToolUse", Some("context_lookup"), Some(json!({"id": 99})))
    result = extract_explicit_read_ids(&[obs])
    assert result.contains(99)
```

### AC-12(d) GATE — prefixed tool name normalized correctly

```
Test: test_extract_explicit_reads_prefixed_tool_name
    obs = make_obs("PreToolUse", Some("mcp__unimatrix__context_get"), Some(json!({"id": 7})))
    result = extract_explicit_read_ids(&[obs])
    assert result.contains(7)
    // This test is non-negotiable: production hook events carry the prefix
```

### AC-12(e) — empty slice produces empty set

```
Test: test_extract_explicit_reads_empty_slice
    result = extract_explicit_read_ids(&[])
    assert result.is_empty()
```

### AC-16 GATE — string-form ID handled

```
Test: test_extract_explicit_reads_string_form_id
    obs_int    = make_obs("PreToolUse", Some("context_get"), Some(json!({"id": 42})))
    obs_string = make_obs("PreToolUse", Some("context_get"), Some(json!({"id": "42"})))
    result = extract_explicit_read_ids(&[obs_int, obs_string])
    assert result.contains(42)
    assert result.len() == 1   // deduplicated — both forms produce the same u64
```

### Additional: Value::String input (hook path) correctly parsed

```
Test: test_extract_explicit_reads_hook_path_string_input
    // Hook listener wraps raw JSON in Value::String, not Value::Object
    raw_json = "{\"id\": 55}"
    obs = make_obs("PreToolUse", Some("mcp__unimatrix__context_get"),
                   Some(Value::String(raw_json.to_string())))
    result = extract_explicit_read_ids(&[obs])
    assert result.contains(55)
```

### Additional: non-PreToolUse events excluded

```
Test: test_extract_explicit_reads_wrong_event_type
    obs = make_obs("PostToolUse", Some("context_get"), Some(json!({"id": 1})))
    result = extract_explicit_read_ids(&[obs])
    assert result.is_empty()
```

### Additional: context_search excluded (not a read tool)

```
Test: test_extract_explicit_reads_search_excluded
    obs = make_obs("PreToolUse", Some("mcp__unimatrix__context_search"),
                   Some(json!({"query": "foo", "id": 42})))
    result = extract_explicit_read_ids(&[obs])
    assert result.is_empty()
    // Even if input has "id", context_search is not a read tool
```

### AC-04 — null id excluded

```
Test: test_extract_explicit_reads_null_id_excluded
    obs = make_obs("PreToolUse", Some("context_lookup"), Some(json!({"id": null})))
    result = extract_explicit_read_ids(&[obs])
    assert result.is_empty()
```

### Edge: None tool field skipped

```
Test: test_extract_explicit_reads_none_tool_skipped
    obs = make_obs("PreToolUse", None, Some(json!({"id": 10})))
    result = extract_explicit_read_ids(&[obs])
    assert result.is_empty()
    // normalize_tool_name("") does not match context_get or context_lookup
```

---

## Integration Surface

| Name | Signature | Caller |
|------|-----------|--------|
| `extract_explicit_read_ids` | `fn(&[ObservationRecord]) -> HashSet<u64>` | `compute_knowledge_reuse_for_sessions` in `tools.rs` |
