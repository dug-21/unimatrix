# crt-026: Component — `context_store` histogram recording (`mcp/tools.rs`)

File: `crates/unimatrix-server/src/mcp/tools.rs`
Wave: 2 (depends on Wave 1: `session.rs` must have `record_category_store`)

---

## Purpose

After the duplicate check in the `context_store` handler, call
`session_registry.record_category_store` to increment the category histogram for
non-duplicate stores. Duplicate stores must NOT increment the histogram.

---

## Current State (Relevant Context)

The `context_store` handler currently has this step ordering (lines 484-610):

```
// 1. Identity + format + audit context
// 2. Capability check
// 3. Input validation (validate_store_params, validate_feature, validate_helpful)
// 4. Category validation (category_validate)
// 5. Scanning (security scan)
// 6. Delegate to StoreService (insert)
//    → insert_result = self.services.store_ops.insert(...)
// 7. Handle duplicate result:
//    if insert_result.duplicate_of.is_some() { return duplicate response }
// 8. Seed initial confidence (fire-and-forget)
// 9. Usage recording with phase snapshot (crt-025 SR-07)
// 10. Format response
```

The new histogram recording sits between the end of step 7 and the beginning of step 8.
It is reached ONLY when execution has passed the `if insert_result.duplicate_of.is_some() { return ... }`
guard, meaning `duplicate_of.is_none()` is guaranteed at this point.

The `session_registry` is available on `self` (as `self.session_registry`).
The pattern for session_id access is `ctx.audit_ctx.session_id` (type `Option<String>`).
The reference pattern is `record_injection`, which also uses `if let Some(ref sid) = ctx.audit_ctx.session_id`.

---

## Modification to `context_store` handler

Insert after step 7 (`if insert_result.duplicate_of.is_some() { return ... }`), immediately
before step 8 (`self.services.confidence.recompute`):

```
// crt-026: Accumulate category histogram for session affinity boost (WA-2).
// Called ONLY after the duplicate guard — duplicate stores must not count (C-09, R-03).
// Pattern mirrors record_injection: if let Some(ref sid) guards the session_id None case.
if let Some(ref sid) = ctx.audit_ctx.session_id {
    self.session_registry.record_category_store(sid, &params.category);
}
```

### Exact insertion point

After this block (step 7, end):
```rust
if insert_result.duplicate_of.is_some() {
    let similarity = insert_result.duplicate_similarity.unwrap_or(1.0);
    return Ok(format_duplicate_found(
        &insert_result.entry,
        similarity,
        ctx.format,
    ));
}
```

Before this block (step 8, begin):
```rust
// 8. Seed initial confidence (fire-and-forget, via ConfidenceService)
self.services
    .confidence
    .recompute(&[insert_result.entry.id]);
```

---

## Ordering Invariant

The ordering `duplicate_guard → histogram_recording → confidence_seeding → usage_recording`
is a handler contract, not just an implementation choice (see RISK-TEST-STRATEGY Integration Risks).

If the handler is ever refactored to move steps into spawned tasks, the duplicate guard
and histogram recording must remain synchronous and ordered before any fire-and-forget.

---

## Access to `session_registry`

`self.session_registry` is a field on `UnimatrixServer`. It is already used in other tools
in this file. No new imports or field additions are required.

Type: `Arc<SessionRegistry>` or direct `SessionRegistry` depending on how the server holds it.
The method call `self.session_registry.record_category_store(sid, &params.category)` is
synchronous — no `.await`.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `ctx.audit_ctx.session_id` is `None` | `if let Some(ref sid)` guard: no call made |
| `session_id` is unregistered | `record_category_store` is a silent no-op (handled in session.rs) |
| `params.category` is empty | Cannot occur here; `category_validate` in step 4 already rejected it |
| `duplicate_of.is_some()` | Handler returns early at step 7; never reaches this block |

---

## Key Test Scenarios

See `test-plan/store-handler.md` for the full test plan. Key scenarios:

1. **AC-02 / R-03 (gate blocker)**: Register a session. Call `context_store` twice with
   identical content (same hash). After first call: histogram `{"decision": 1}`. After
   second call (duplicate): histogram still `{"decision": 1}`, not `{"decision": 2}`.
   The second call must trigger the duplicate return path before reaching the histogram
   recording block.

2. **AC-03**: Call `context_store` without a `session_id` (MCP params field absent).
   Assert histogram is empty / unchanged (no call to `record_category_store`).

3. **Ordering**: Assert the guard placement in code — duplicate check precedes histogram
   recording (static code review / grep assertion).

4. **Multiple categories**: Call `context_store` with category `"decision"` once and
   `"pattern"` once (different entries). Assert histogram = `{"decision": 1, "pattern": 1}`.
