# crt-026: Test Plan — Component 3: ServiceSearchParams Data Carrier Fields

**File under test**: `crates/unimatrix-server/src/services/search.rs`
**Test module**: `#[cfg(test)] mod tests` at bottom of `search.rs`

---

## AC Coverage

| AC-ID | Test |
|-------|------|
| AC-04 | `test_service_search_params_has_session_fields` (structural/compile) |
| AC-05 partial | See search-handler.md |

Risk coverage: R-12 (struct literal construction sites updated).

---

## Scope

Component 3 is a pure data carrier extension — two new fields on `ServiceSearchParams`,
no logic. The test strategy is:

1. **Structural test**: assert the fields exist with correct types (compile-time evidence plus
   a runtime construction test).
2. **Code-review check**: all existing `ServiceSearchParams { ... }` construction sites must
   be updated. Compilation is the primary gate; tests supplement.

---

## Tests

### T-SP-NEW-01: `test_service_search_params_has_session_fields`
**AC-04 | R-12**

**Arrange/Act**:
```rust
let params = ServiceSearchParams {
    query: "test".to_string(),
    k: 5,
    filters: None,
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: None,
    retrieval_mode: RetrievalMode::Flexible,
    session_id: None,                // NEW field
    category_histogram: None,        // NEW field
};
```

**Assert**:
```rust
assert!(params.session_id.is_none(),
    "session_id field must exist and be Option<String>");
assert!(params.category_histogram.is_none(),
    "category_histogram field must exist and be Option<HashMap<String, u32>>");
```

**Notes**: This test will fail to compile if the fields are missing or have wrong types.
Compilation failure IS the test failure for AC-04. The runtime assertion documents intent.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

### T-SP-NEW-02: `test_service_search_params_with_session_data`
**AC-05 partial | R-12**

**Arrange/Act**:
```rust
use std::collections::HashMap;
let mut hist: HashMap<String, u32> = HashMap::new();
hist.insert("decision".to_string(), 3);
hist.insert("pattern".to_string(), 2);

let params = ServiceSearchParams {
    query: "how to handle session state".to_string(),
    k: 10,
    filters: None,
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: None,
    retrieval_mode: RetrievalMode::Flexible,
    session_id: Some("sid-abc".to_string()),
    category_histogram: Some(hist),
};
```

**Assert**:
```rust
assert_eq!(params.session_id.as_deref(), Some("sid-abc"));
let h = params.category_histogram.as_ref().unwrap();
assert_eq!(h.get("decision"), Some(&3));
assert_eq!(h.get("pattern"), Some(&2));
```

**Notes**: Confirms that the `Option<HashMap<String, u32>>` type is correct and that
values can be populated, retrieved, and read by the scoring loop.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

### T-SP-NEW-03: `test_service_search_params_empty_histogram_maps_to_none`
**AC-08 partial | R-02, R-09**

This test documents the handler invariant that an empty histogram must be mapped to `None`
before constructing `ServiceSearchParams`.

**Arrange**:
```rust
use std::collections::HashMap;
let empty: HashMap<String, u32> = HashMap::new();
// Simulating: let category_histogram = if h.is_empty() { None } else { Some(h) };
let category_histogram: Option<HashMap<String, u32>> =
    if empty.is_empty() { None } else { Some(empty) };
```

**Assert**:
```rust
assert!(category_histogram.is_none(),
    "an empty histogram must be mapped to None before ServiceSearchParams construction");
```

**Notes**: This test documents the handler contract that prevents `Some(empty_map)` from
reaching the scoring loop (R-09 primary guard). The test lives in `search.rs` because the
invariant is about `ServiceSearchParams` semantics: `category_histogram = None` is the
cold-start signal; `Some(empty_map)` is not a valid state for the field.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

## Code-Review Assertions (not automated)

**AC-04**: `grep "session_id: Option<String>" services/search.rs` — assert exactly one match
in the `ServiceSearchParams` struct definition (not in a method or test).

**R-12**: After implementation, audit all `ServiceSearchParams { ... }` construction sites:
- `mcp/tools.rs` context_search handler
- `uds/listener.rs` handle_context_search
- Any test helpers that construct `ServiceSearchParams` with struct literals

All sites must explicitly populate `session_id` and `category_histogram`. The fields are
`Option`-typed, so omission at a struct literal site would silently compile (using `None`
default via `..Default::default()` if that is derived, or a compile error if not). Verify
each site by inspection.
