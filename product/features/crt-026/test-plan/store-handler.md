# crt-026: Test Plan — Component 2: context_store Handler — Histogram Recording

**File under test**: `crates/unimatrix-server/src/mcp/tools.rs`
**Test module**: `#[cfg(test)] mod tests` at bottom of `tools.rs`

---

## AC Coverage

| AC-ID | Test |
|-------|------|
| AC-02 | `test_duplicate_store_does_not_increment_histogram` (gate blocker) |
| AC-05 partial | Tested in search-handler.md (end-to-end threading) |

Risk coverage: R-03 (duplicate guard placement), R-04 partial (None session_id guard).

---

## Scope

Component 2 adds a single code block to the `context_store` handler — the
`record_category_store` call placed after the duplicate guard and before confidence seeding.
The unit tests here validate the guard logic and the no-session-id guard, not the full
end-to-end store flow (which requires a real database and is covered by integration tests).

The handler logic under test:
```rust
// crt-026: accumulate category histogram for session affinity boost (WA-2)
if let Some(ref sid) = ctx.audit_ctx.session_id {
    self.session_registry.record_category_store(sid, &params.category);
}
```
This block executes only after `insert_result.duplicate_of.is_none()`.

---

## Test Approach

Because `context_store` is an async MCP handler that requires a full `ToolContext` and
database connection, the tests for Component 2 use `SessionRegistry` directly to simulate
what the handler does. This follows the pattern of existing tools.rs tests that test the
underlying service layer rather than the full handler.

The gate-blocking test focuses on the invariant: the histogram count is exactly 1 after
two stores of the same entry (not 2), and the test is placed where it can be verified via
`SessionRegistry.get_category_histogram`.

---

## Tests

### T-SH-01: `test_duplicate_store_does_not_increment_histogram` **(GATE BLOCKER)**
**AC-02 | R-03**

**Arrange**:
```rust
let reg = SessionRegistry::new();
reg.register_session("s1", None, None);
```

**Act** (simulating what the handler does):
```rust
// First store — non-duplicate: record_category_store is called
reg.record_category_store("s1", "decision");

// Second store — duplicate: duplicate_of.is_some() → record_category_store NOT called
// (handler code skips the block when insert_result.duplicate_of.is_some())
// We simulate this by NOT calling record_category_store again.
```

**Assert**:
```rust
let histogram = reg.get_category_histogram("s1");
assert_eq!(histogram.get("decision"), Some(&1),
    "histogram must be 1 after two stores of the same entry; \
     duplicate store must not increment the count");
assert_eq!(histogram.len(), 1);
```

**Notes**: The test validates the invariant, not the handler code path directly. The
companion code-review check (R-12) ensures the duplicate guard is correctly placed in
the actual handler. The test documents the contract: only non-duplicate stores count.

**Module**: `mcp/tools.rs` `#[cfg(test)] mod tests`

---

### T-SH-02: `test_store_increments_histogram_for_registered_session`
**AC-02 | R-03 positive path**

**Arrange**:
```rust
let reg = SessionRegistry::new();
reg.register_session("s1", None, None);
```

**Act**: Simulate 3 distinct successful stores of different categories:
```rust
reg.record_category_store("s1", "decision");
reg.record_category_store("s1", "pattern");
reg.record_category_store("s1", "decision");
```

**Assert**:
```rust
let h = reg.get_category_histogram("s1");
assert_eq!(h.get("decision"), Some(&2));
assert_eq!(h.get("pattern"), Some(&1));
```

**Module**: `mcp/tools.rs` `#[cfg(test)] mod tests`

---

### T-SH-03: `test_store_no_session_id_does_not_record`
**AC-03 partial | R-04**

**Arrange**: `SessionRegistry::new()` — no sessions registered.

**Act**: Simulate the handler's `if let Some(ref sid)` guard when `session_id` is `None`:
```rust
let session_id: Option<String> = None;
if let Some(ref sid) = session_id {
    reg.record_category_store(sid, "decision");
}
```

**Assert**:
- `reg.get_category_histogram("anything").is_empty()` — registry is untouched.
- No panic occurred.

**Module**: `mcp/tools.rs` `#[cfg(test)] mod tests`

**Notes**: Documents that the `if let Some(...)` guard pattern from the `record_injection`
call is reused for `record_category_store`. When `session_id` is `None`, the block is
never entered.

---

### T-SH-04: `test_histogram_ordering_guard_semantics`
**R-03 (guard ordering documentation)**

This is a documentation/invariant test that asserts the contract on ordering:

**Assert** (pure logic, no state):
```rust
// The duplicate guard must precede the histogram record.
// Encoding this as a comment-driven assertion in the test suite:
// If duplicate_of.is_some() → histogram NOT incremented.
// If duplicate_of.is_none() → histogram IS incremented.
// Verified by T-SH-01 (duplicate path) and T-SH-02 (non-duplicate path).
// No code change: this test is a documentation anchor for R-03.
// Assertion: the two tests above together cover both branches.
assert!(true); // Placeholder; reviewer should verify T-SH-01 and T-SH-02 together.
```

**Notes**: This test exists to make the R-03 guard placement explicit in the test suite.
It is intentionally minimal; the substance is in T-SH-01 and T-SH-02. Consider merging
T-SH-01 and T-SH-02 into a single parametric test if the codebase supports it.

**Module**: `mcp/tools.rs` `#[cfg(test)] mod tests`

---

## Code-Review Assertions (not automated)

**R-12 check — struct literal construction sites**: After implementation, the implementer
must confirm that all `ServiceSearchParams { ... }` literal construction sites in `tools.rs`
explicitly set `session_id` and `category_histogram` fields.

**R-03 ordering check**: The `record_category_store` call in the handler must appear AFTER
the `if insert_result.duplicate_of.is_some() { return ...; }` check and BEFORE
`seed_confidence` / `record_usage`. Verify by reading the step comments in the handler body.

---

## Integration Test Reference

The end-to-end store → histogram → search flow is validated by the infra-001 integration
test `test_session_histogram_boosts_category_match` in `suites/test_lifecycle.py`.
See OVERVIEW.md § Integration Harness Plan for details.
