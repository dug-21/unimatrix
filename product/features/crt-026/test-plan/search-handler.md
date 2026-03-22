# crt-026: Test Plan — Component 4: context_search Handler Pre-Resolution

**File under test**: `crates/unimatrix-server/src/mcp/tools.rs`
**Test module**: `#[cfg(test)] mod tests` at bottom of `tools.rs`

---

## AC Coverage

| AC-ID | Test |
|-------|------|
| AC-05 | `test_context_search_handler_populates_service_search_params` |
| AC-08 partial | `test_category_histogram_none_when_session_empty` |
| AC-06 | Transitively via AC-12 (fused-score.md) |

Risk coverage: R-02 (cold-start), R-05 (pre-resolution before await), R-13 (snapshot ordering).

---

## Scope

Component 4 adds the pre-resolution block to the `context_search` MCP handler:
```rust
// crt-026: pre-resolve session histogram (WA-2, SR-07 snapshot pattern)
let category_histogram: Option<HashMap<String, u32>> =
    ctx.audit_ctx.session_id.as_deref().and_then(|sid| {
        let h = self.session_registry.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

The handler is async with many dependencies (store, embed, NLI), making full handler
integration tests impractical at the unit level. Tests here focus on the pre-resolution
logic in isolation using `SessionRegistry` directly.

---

## Tests

### T-SCH-01: `test_pre_resolve_histogram_some_when_session_has_stores`
**AC-05 | R-02**

**Arrange**:
```rust
let reg = SessionRegistry::new();
reg.register_session("s1", None, None);
reg.record_category_store("s1", "decision");
reg.record_category_store("s1", "decision");
reg.record_category_store("s1", "decision");
```

**Act** (simulating the handler's pre-resolution):
```rust
let session_id = Some("s1".to_string());
let category_histogram: Option<HashMap<String, u32>> =
    session_id.as_deref().and_then(|sid| {
        let h = reg.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

**Assert**:
```rust
assert!(category_histogram.is_some(),
    "pre-resolved histogram must be Some when session has stores");
let h = category_histogram.unwrap();
assert_eq!(h.get("decision"), Some(&3));
```

**Notes**: Tests the `and_then` + `is_empty` guard pattern from the architecture spec.
Validates AC-05 — the handler passes the pre-resolved histogram into `ServiceSearchParams`.

**Module**: `mcp/tools.rs` `#[cfg(test)] mod tests`

---

### T-SCH-02: `test_category_histogram_none_when_session_empty`
**AC-08 partial | R-02 cold-start**

**Arrange**:
```rust
let reg = SessionRegistry::new();
reg.register_session("s1", None, None);
// No stores — histogram is empty
```

**Act** (simulating pre-resolution):
```rust
let category_histogram: Option<HashMap<String, u32>> =
    Some("s1").and_then(|sid| {
        let h = reg.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

**Assert**:
```rust
assert!(category_histogram.is_none(),
    "pre-resolved histogram must be None when session has no stores (cold start)");
```

**Module**: `mcp/tools.rs` `#[cfg(test)] mod tests`

---

### T-SCH-03: `test_category_histogram_none_when_no_session_id`
**AC-08 partial | R-02 no-session path**

**Arrange/Act** (simulating `ctx.audit_ctx.session_id = None`):
```rust
let session_id: Option<String> = None;
let reg = SessionRegistry::new();
let category_histogram: Option<HashMap<String, u32>> =
    session_id.as_deref().and_then(|sid| {
        let h = reg.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

**Assert**:
```rust
assert!(category_histogram.is_none(),
    "category_histogram must be None when session_id is None (no session path)");
```

**Notes**: Covers the workflow where an agent calls `context_search` without a session_id
(Workflow 3 in SPECIFICATION.md).

**Module**: `mcp/tools.rs` `#[cfg(test)] mod tests`

---

### T-SCH-04: `test_context_search_handler_populates_service_search_params`
**AC-05 | R-12**

This test documents the expected construction of `ServiceSearchParams` with both new fields.
It is a structural/logic test, not a full handler invocation.

**Arrange**:
```rust
let reg = SessionRegistry::new();
reg.register_session("s1", None, None);
reg.record_category_store("s1", "decision");

let session_id_ctx = Some("s1".to_string()); // simulates ctx.audit_ctx.session_id

let category_histogram: Option<HashMap<String, u32>> =
    session_id_ctx.as_deref().and_then(|sid| {
        let h = reg.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

**Act**:
```rust
let params = ServiceSearchParams {
    query: "session registry pattern".to_string(),
    k: 5,
    filters: None,
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: None,
    retrieval_mode: RetrievalMode::Flexible,
    session_id: session_id_ctx.clone(),
    category_histogram,
};
```

**Assert**:
```rust
assert_eq!(params.session_id.as_deref(), Some("s1"));
let h = params.category_histogram.as_ref().unwrap();
assert_eq!(h.get("decision"), Some(&1));
```

**Module**: `mcp/tools.rs` `#[cfg(test)] mod tests`

---

## Code-Review Assertions (not automated, R-13)

**Pre-resolution before `await`**: The `get_category_histogram` call must appear before the
first `await` point in `context_search`. Review the handler function body:

1. Locate the `// crt-026: pre-resolve session histogram` comment.
2. Confirm it appears in the handler before any `.await` expression.
3. This satisfies the SR-07 snapshot pattern (crt-025) — session state read once synchronously.

No automated test can reliably enforce the `await` ordering. This is a code review checkpoint
for Stage 3b (implementer) and Gate 3a (reviewer). The risk (R-13) is documented; the test
plan acknowledges it as a code-review item per RISK-TEST-STRATEGY.md.

---

## Integration Test Reference

The full MCP handler path (context_store → session accumulation → context_search → boosted
ranking) is validated by `test_session_histogram_boosts_category_match` in
`suites/test_lifecycle.py`. See OVERVIEW.md § Integration Harness Plan.
