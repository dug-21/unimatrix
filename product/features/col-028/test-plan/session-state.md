# Test Plan: SessionState (Component 1)
# File: crates/unimatrix-server/src/infra/session.rs

## Risks Addressed

| Risk | AC | Priority |
|------|-----|----------|
| R-09: confirmed_entries missing from test helpers | AC-20 | High |
| R-13: confirmed_entries cardinality error | AC-10 | Medium |

## Unit Test Expectations

Location: `crates/unimatrix-server/src/infra/session.rs` test module, or a
dedicated test submodule `#[cfg(test)]`.

Pattern to follow: `signaled_entries` and `record_category_store` — these are
the existing analogues for `confirmed_entries` and `record_confirmed_entry`.

---

### AC-08: confirmed_entries initialised empty on register_session

```rust
#[test]
fn test_register_session_confirmed_entries_starts_empty() {
    // Arrange
    let registry = SessionRegistry::new();
    // Act
    registry.register_session("sess-001", None, None);
    let state = registry.get_state("sess-001").expect("state");
    // Assert
    assert!(
        state.confirmed_entries.is_empty(),
        "confirmed_entries must be empty after register_session"
    );
}
```

Expected: `confirmed_entries.is_empty()` is true immediately after `register_session`.
No entries from prior sessions bleed through (isolation guaranteed by `register_session`
reinitialising the HashSet to `HashSet::new()`).

---

### AC-08 variant: re-registration resets confirmed_entries

```rust
#[test]
fn test_re_register_session_resets_confirmed_entries() {
    // Arrange: register and populate confirmed_entries.
    let registry = SessionRegistry::new();
    registry.register_session("sess-002", None, None);
    registry.record_confirmed_entry("sess-002", 42_u64);
    // Sanity check: entry is present.
    let state = registry.get_state("sess-002").expect("state");
    assert!(state.confirmed_entries.contains(&42_u64));

    // Act: re-register same session_id.
    registry.register_session("sess-002", None, None);

    // Assert: confirmed_entries is reset to empty.
    let state = registry.get_state("sess-002").expect("state");
    assert!(
        state.confirmed_entries.is_empty(),
        "confirmed_entries must reset on re-registration"
    );
}
```

---

### AC-10 (positive arm): single-target lookup populates confirmed_entries

```rust
#[test]
fn test_record_confirmed_entry_single_id_is_stored() {
    // Arrange
    let registry = SessionRegistry::new();
    registry.register_session("sess-003", None, None);

    // Act: record_confirmed_entry is called when context_lookup target_ids.len() == 1
    registry.record_confirmed_entry("sess-003", 100_u64);

    // Assert
    let state = registry.get_state("sess-003").expect("state");
    assert!(
        state.confirmed_entries.contains(&100_u64),
        "confirmed_entries must contain entry 100 after record_confirmed_entry"
    );
}
```

---

### AC-10 (negative arm — REQUIRED): multi-target lookup must NOT populate confirmed_entries

This test does not call `record_confirmed_entry` — it validates that the
context_lookup handler only calls the method when `target_ids.len() == 1`.
The negative arm is tested via the handler-level test in `tools-read-side.md`.

At the SessionState layer, the contract is: `record_confirmed_entry` is an explicit
call; if the caller does not call it, `confirmed_entries` remains untouched.

```rust
#[test]
fn test_confirmed_entries_not_modified_without_record_call() {
    let registry = SessionRegistry::new();
    registry.register_session("sess-004", None, None);
    // No calls to record_confirmed_entry

    let state = registry.get_state("sess-004").expect("state");
    assert!(state.confirmed_entries.is_empty());
    // The multi-target guard is tested in tools-read-side.md AC-10 negative arm
}
```

---

### AC-10: multiple calls accumulate (idempotent for same entry)

```rust
#[test]
fn test_record_confirmed_entry_multiple_entries_accumulate() {
    let registry = SessionRegistry::new();
    registry.register_session("sess-005", None, None);
    registry.record_confirmed_entry("sess-005", 10_u64);
    registry.record_confirmed_entry("sess-005", 20_u64);
    registry.record_confirmed_entry("sess-005", 10_u64); // duplicate

    let state = registry.get_state("sess-005").expect("state");
    assert!(state.confirmed_entries.contains(&10_u64));
    assert!(state.confirmed_entries.contains(&20_u64));
    // HashSet deduplicates: len is 2, not 3
    assert_eq!(state.confirmed_entries.len(), 2);
}
```

---

### AC-20: test helper update (make_state_with_rework)

This AC has no standalone test; it is validated by `cargo test --workspace` compiling
and passing without errors. The specific requirement:

- `make_state_with_rework(...)` function in the test helpers must include
  `confirmed_entries: HashSet::new()` in its `SessionState` struct literal.
- Any other place in test code that constructs `SessionState { ... }` with named fields
  must also include `confirmed_entries: HashSet::new()`.

**Verification**: `cargo test --workspace` passes with no compile error from a missing
struct field. A compile error `missing field confirmed_entries in initializer of SessionState`
is the unmistakable failure mode (pattern #3180).

---

## Edge Cases

### EC-03: record_confirmed_entry for non-existent session is a no-op

```rust
#[test]
fn test_record_confirmed_entry_unknown_session_is_noop() {
    let registry = SessionRegistry::new();
    // No registration for "unknown-sess"
    // Must not panic
    registry.record_confirmed_entry("unknown-sess", 99_u64);
    // No assertion needed — the test passes if no panic occurs
}
```

---

### EC-05: confirmed_entries not polluted by failed context_get

Spec FR-08: record_confirmed_entry is called "after a successful retrieval". If
context_get returns not-found, the handler must NOT call record_confirmed_entry.
Test at the handler level in `tools-read-side.md` AC-09.

---

## Integration Test Expectations

SessionState itself has no MCP-visible interface — it is purely internal in-memory
state. The integration tests for confirmed_entries behavior are at the handler level
(see `tools-read-side.md` for AC-09 and AC-10 handler-level tests).

The infra-001 harness validates confirmed_entries indirectly via the AC-07 D-01 guard
test in `test_lifecycle.py` — if confirmed_entries were incorrectly populated or cleared,
the briefing→get sequence would produce wrong access_count values.

---

## Assertions Summary

| AC | Test Function Name | Expected Result |
|----|-------------------|-----------------|
| AC-08 | `test_register_session_confirmed_entries_starts_empty` | `confirmed_entries.is_empty()` |
| AC-08 variant | `test_re_register_session_resets_confirmed_entries` | HashSet reset to empty |
| AC-10 positive | `test_record_confirmed_entry_single_id_is_stored` | Contains entry_id |
| AC-10 negative | Validated in tools-read-side.md handler test | Handler does not call record_confirmed_entry for multi-target |
| AC-20 | `cargo test --workspace` compile success | No `missing field confirmed_entries` compile error |
| EC-03 | `test_record_confirmed_entry_unknown_session_is_noop` | No panic |
