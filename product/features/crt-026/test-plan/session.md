# crt-026: Test Plan — Component 1: SessionState + SessionRegistry

**File under test**: `crates/unimatrix-server/src/infra/session.rs`
**Test module**: `#[cfg(test)] mod tests` at bottom of `session.rs`

---

## AC Coverage

| AC-ID | Test |
|-------|------|
| AC-01 | `test_register_session_category_counts_empty` |
| AC-03 | `test_record_category_store_unregistered_session_is_noop` (gate blocker) |

Risk coverage: R-04 (unregistered session safety), R-03 partial (guard placement verification).

---

## Test Fixtures and Helpers

All existing session tests use `make_registry()` which returns `SessionRegistry::new()`.
New tests follow the same pattern. No additional fixtures needed.

```rust
fn make_registry() -> SessionRegistry { SessionRegistry::new() }
```

---

## Tests

### T-SS-01: `test_register_session_category_counts_empty`
**AC-01 | R-04 (baseline)**

**Arrange**: Create a fresh `SessionRegistry`. Call `register_session("s1", None, None)`.

**Act**: `reg.get_state("s1").unwrap()`

**Assert**:
- `state.category_counts.is_empty()` is `true`
- `state.category_counts.len() == 0`

**Module**: `infra/session.rs` `#[cfg(test)] mod tests`

**Notes**: Mirrors `register_and_get_state` pattern. Verifies that `register_session` initializes
`category_counts` to `HashMap::new()` (FR-01). Extending the existing `register_and_get_state`
test to include the new field is acceptable if it keeps all checks in one place.

---

### T-SS-02: `test_record_category_store_increments_count` *(GATE BLOCKER precondition)*
**AC-02 partial | R-03**

**Arrange**: `make_registry()`; `reg.register_session("s1", None, None)`.

**Act**: `reg.record_category_store("s1", "decision")`

**Assert**:
- `reg.get_category_histogram("s1")` returns `{"decision": 1}`
- `histogram.get("decision") == Some(&1)`
- `histogram.len() == 1`

**Module**: `infra/session.rs` `#[cfg(test)] mod tests`

---

### T-SS-03: `test_record_category_store_multiple_categories`
**AC-02 | R-01 fixture baseline**

**Arrange**: `make_registry()`; `reg.register_session("s1", None, None)`.

**Act**:
```rust
reg.record_category_store("s1", "decision");
reg.record_category_store("s1", "decision");
reg.record_category_store("s1", "decision");
reg.record_category_store("s1", "pattern");
reg.record_category_store("s1", "pattern");
```

**Assert**:
- `histogram.get("decision") == Some(&3)`
- `histogram.get("pattern") == Some(&2)`
- `histogram.len() == 2`
- total = 5 (sum of all values)

**Module**: `infra/session.rs` `#[cfg(test)] mod tests`

**Notes**: This histogram state is the fixture for R-01 scenario 2 (60% concentration).
The math: `p("decision") = 3/5 = 0.6`.

---

### T-SS-04: `test_record_category_store_unregistered_session_is_noop` **(GATE BLOCKER)**
**AC-03 | R-04**

**Arrange**: `make_registry()` — no `register_session` called.

**Act**: `reg.record_category_store("nonexistent-session", "decision")` — must not panic.

**Assert**:
- Method returns without panicking.
- `reg.get_state("nonexistent-session")` returns `None` (state unchanged).
- `reg.get_category_histogram("nonexistent-session")` returns an empty `HashMap`.
- `empty_map.is_empty()` is `true`.

**Module**: `infra/session.rs` `#[cfg(test)] mod tests`

**Notes**: Follows the `record_injection` silent no-op contract. The test MUST use an
unregistered session ID — not just an empty histogram. Also verifies `get_category_histogram`
returns empty (not panic) for an unregistered session, covering R-04 scenario 2.

---

### T-SS-05: `test_get_category_histogram_unregistered_returns_empty`
**AC-03 partial | R-04**

**Arrange**: `make_registry()` — no sessions registered.

**Act**: `let h = reg.get_category_histogram("no-such-session")`

**Assert**:
- `h.is_empty()` is `true`
- Method does not panic

**Module**: `infra/session.rs` `#[cfg(test)] mod tests`

**Notes**: Explicit test for the `get_category_histogram` read path on a missing session.
The duplicate guard in the handler relies on this returning empty (not `None`), so the
`is_empty()` → `None` mapping in the handler is tested separately.

---

### T-SS-06: `test_record_category_store_isolated_between_sessions`
**AC-02 | R-04**

**Arrange**: `make_registry()`; register "s1" and "s2".

**Act**:
```rust
reg.record_category_store("s1", "decision");
reg.record_category_store("s1", "pattern");
```

**Assert**:
- `reg.get_category_histogram("s1")` contains `decision:1, pattern:1`
- `reg.get_category_histogram("s2")` is empty
- Stores for "s1" do not leak into "s2"

**Module**: `infra/session.rs` `#[cfg(test)] mod tests`

---

### T-SS-07: `test_register_session_resets_category_counts`
**AC-01 | R-03 (re-registration)**

**Arrange**: `make_registry()`; register "s1"; call `record_category_store("s1", "decision")`.

**Act**: `reg.register_session("s1", None, None)` — overwrite the session.

**Assert**:
- `reg.get_category_histogram("s1").is_empty()` is `true`
- The re-registration discards accumulated histogram state (mirrors `register_overwrites_existing`)

**Module**: `infra/session.rs` `#[cfg(test)] mod tests`

---

## Edge Cases

- **EC-01** (single store): `record_category_store` once → histogram = `{cat: 1}`. `p(cat) = 1.0`.
  Covered by T-SS-02.
- **EC-03** (empty category string): not tested here — `category_validate` in `context_store`
  handler rejects empty strings before `record_category_store` is reached. Document as a
  code-review checklist item; do not add a unit test here since the guard is upstream.
- **EC-06** (server restart): histogram is in-memory; after `register_session`, histogram is
  always empty. Covered by T-SS-07 (re-registration resets state).

---

## What Is NOT Tested Here

- The duplicate-store guard (`duplicate_of.is_some()` check) — that belongs in
  `store-handler.md`, where the handler logic is tested.
- The `is_empty() → None` mapping — that belongs in `search-handler.md`.
- Concurrency stress (FM-01) — optional, not required for Gate 3c.
