# col-028: Component 1 — SessionState + SessionRegistry

**File**: `crates/unimatrix-server/src/infra/session.rs`

## Purpose

Add `confirmed_entries: HashSet<u64>` to `SessionState` and expose
`SessionRegistry::record_confirmed_entry` so the four read-side handlers can mark
entries the agent explicitly retrieved. The field has no consumer in this feature; it
is populated now so Thompson Sampling inherits populated data from day one (ADR-005).

## New / Modified Declarations

### 1. SessionState struct — new field

Add `confirmed_entries` as the final field in the struct, after `current_goal`.
Group it under a `// col-028 fields` comment to match the per-feature grouping
convention already present in the file (col-009, col-017, crt-025, crt-026, col-025).

```
pub struct SessionState {
    // ... existing fields unchanged (session_id through current_goal) ...

    // col-028 fields
    /// Entry IDs explicitly retrieved by the agent this session.
    ///
    /// Populated by `context_get` (always) and `context_lookup` (single-ID
    /// requests only — request-side cardinality, not result-set cardinality).
    /// Not populated by briefing, search, write, or mutation tools.
    /// In-memory only; reset on register_session; never persisted.
    /// First consumer: Thompson Sampling (future feature).
    pub confirmed_entries: HashSet<u64>,
}
```

The doc comment text is load-bearing (AC-24). Use verbatim.

### 2. register_session — initialiser update

In the `SessionState { ... }` struct literal inside `register_session`, append:

```
confirmed_entries: HashSet::new(),   // col-028: empty on session start
```

Position after `current_goal: None,` (the previous last field).

### 3. SessionRegistry::record_confirmed_entry — new method

Pattern: identical lock-and-mutate idiom as `record_category_store`. Lock, get_mut,
insert into the HashSet, silent no-op for unregistered sessions.

```
FUNCTION record_confirmed_entry(self, session_id: &str, entry_id: u64):
    LOCK self.sessions (unwrap_or_else poison recovery)
    IF sessions.get_mut(session_id) IS Some(state):
        state.confirmed_entries.insert(entry_id)
    // Unregistered session: silent no-op (matches record_injection contract)
```

Exact signature (from ARCHITECTURE.md Integration Surface):
```rust
pub fn record_confirmed_entry(&self, session_id: &str, entry_id: u64)
```

No return value. The method does not need to report whether the entry was already
present — callers do not use the boolean.

## make_state_with_rework — Test Helper Update (Pattern #3180)

The `make_state_with_rework` helper at line 1071 constructs a `SessionState` struct
literal directly. It must gain the new field or every test using it will fail to compile.

```
fn make_state_with_rework(events: Vec<(&str, Option<&str>, bool)>) -> SessionState {
    SessionState {
        // ... all existing fields ...
        current_goal: None,
        confirmed_entries: HashSet::new(),   // col-028: add this line
    }
}
```

Search for ALL occurrences of `SessionState {` struct literals in test code within
this file and any adjacent test modules. Each must gain `confirmed_entries: HashSet::new()`.
The compiler will catch every missed site.

## Error Handling

No new error conditions. `record_confirmed_entry` is infallible:
- Mutex poison is recovered with `unwrap_or_else(|e| e.into_inner())` (existing pattern).
- Unregistered session is a silent no-op (matching all other SessionRegistry methods).
- HashSet insert is infallible.

## Data Flow

- Caller (mcp/tools.rs): calls `record_confirmed_entry(session_id, entry_id)` after a
  successful retrieval in `context_get` or after a single-ID `context_lookup`.
- Reader (future Thompson Sampling feature): calls `get_state(session_id)` and reads
  `.confirmed_entries` from the returned clone.
- The field is NOT read anywhere in this feature (C-07).

## Key Test Scenarios

**AC-08** — `register_session` initialises `confirmed_entries` as empty.
  - Call `register_session("sid", None, None)`.
  - Call `get_state("sid")`.
  - Assert `state.confirmed_entries.is_empty()`.

**AC-09 (partial)** — `record_confirmed_entry` inserts the entry ID.
  - Register a session.
  - Call `record_confirmed_entry("sid", 42)`.
  - Call `get_state("sid")`.
  - Assert `state.confirmed_entries.contains(&42)`.

**Multiple inserts** — Inserting the same entry ID twice is idempotent (HashSet).
  - Call `record_confirmed_entry("sid", 42)` twice.
  - Assert `state.confirmed_entries.len() == 1`.

**Unregistered session no-op** — Calling `record_confirmed_entry` for a session that
  was never registered must not panic.
  - Do not call `register_session`.
  - Call `record_confirmed_entry("nosuchsid", 42)`.
  - No panic; no effect.

**AC-20 — make_state_with_rework compiles** — All existing rework threshold tests pass
  with `confirmed_entries: HashSet::new()` added to the helper. Verified by
  `cargo test --workspace`.

## Out of Scope for This Component

- Reading `confirmed_entries` (no consumer in this feature).
- Persisting `confirmed_entries` (in-memory only per spec).
- Any change to `current_phase`, `signaled_entries`, or other existing fields.
