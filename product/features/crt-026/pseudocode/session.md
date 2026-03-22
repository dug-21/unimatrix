# crt-026: Component — `infra/session.rs`

File: `crates/unimatrix-server/src/infra/session.rs`
Wave: 1

---

## Purpose

Extend `SessionState` with a `category_counts` histogram and add two new public
methods on `SessionRegistry` to record and read it. These are the only entry points
for accumulating and reading session category signal in crt-026.

---

## Current State (Relevant Context)

`SessionState` currently ends with:
```
// crt-025 fields
pub current_phase: Option<String>,
```

`register_session` initializes `current_phase: None` in the `sessions.insert(...)` literal.

`record_injection` is the reference pattern for all new registry methods:
- Lock via `sessions.lock().unwrap_or_else(|e| e.into_inner())`
- Check `sessions.get_mut(session_id)` — silent no-op for `None`
- Perform mutation inside the `if let Some(state)` block
- No `await`, no I/O, no `spawn_blocking`

---

## New / Modified: `SessionState`

### Modification to struct definition

Add one field at the end of `SessionState`, after the `current_phase` field:

```
// crt-026 fields
pub category_counts: HashMap<String, u32>,
// Per-session category histogram for WA-2 histogram affinity boost.
// Incremented by record_category_store on each successful non-duplicate context_store.
// Read by get_category_histogram before context_search scoring.
// In-memory only: never persisted, reset on register_session (reconnection).
```

### Modification to `register_session`

In the `SessionState { ... }` struct literal inside `sessions.insert(...)`, add the
new field after `current_phase: None`:

```
category_counts: HashMap::new(),   // crt-026: empty histogram on session start
```

No other changes to `register_session`.

---

## New Methods on `SessionRegistry`

### `record_category_store`

```
pub fn record_category_store(&self, session_id: &str, category: &str)

Purpose:
  Increment category_counts[category] by 1 for the registered session.
  Silent no-op for unregistered sessions.

Preconditions enforced by caller (context_store handler):
  - Called ONLY when insert_result.duplicate_of.is_none()
  - Called ONLY when session_id is Some (the caller guards with if let Some(ref sid))
  - category has already been validated by category_validate before this point

Algorithm:
  1. Acquire lock:
       sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner())

  2. Lookup session:
       if let Some(state) = sessions.get_mut(session_id) {
           // 3. Increment the count for this category.
           //    entry().or_insert(0) handles first-time categories.
           let count = state.category_counts.entry(category.to_string()).or_insert(0);
           *count += 1;
       }
       // Unregistered session: fall through, no action (silent no-op matches record_injection contract)

Error handling:
  - Mutex poison: recovered via unwrap_or_else(|e| e.into_inner())
    (same recovery pattern as all existing SessionRegistry methods — see FR-02)
  - No return value, no Result — callers do not need to check success

Lock behavior:
  - Held for: one HashMap::entry().or_insert() + one integer increment = microseconds
  - No I/O, no spawn_blocking, no await
  - Same lock contract as record_injection (NFR-01)
```

### `get_category_histogram`

```
pub fn get_category_histogram(&self, session_id: &str) -> HashMap<String, u32>

Purpose:
  Return a clone of category_counts for the registered session,
  or an empty HashMap if the session is not registered.
  This is the sole read path for the histogram.

Preconditions:
  - Called after sanitize_session_id has already validated the session_id string
    (in the UDS path this is done at listener.rs lines 796-803; in the MCP path
    the session_id comes from ctx.audit_ctx.session_id which was validated during
    build_context)

Algorithm:
  1. Acquire read lock (same lock as write path — Mutex, not RwLock):
       sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner())

  2. Lookup and clone:
       match sessions.get(session_id) {
           Some(state) => state.category_counts.clone(),
           None        => HashMap::new(),
       }

  3. Return the HashMap<String, u32>.
     The caller is responsible for mapping an empty return value to None
     before storing in ServiceSearchParams.category_histogram.

Error handling:
  - Mutex poison: recovered via unwrap_or_else(|e| e.into_inner())
  - Unregistered session: returns empty HashMap (not an error; cold-start safe)
  - Never panics

Lock behavior:
  - Held for: one HashMap lookup + one clone = O(categories), microseconds in practice
    (<20 categories typical; EC-02 analysis confirms negligible latency)
  - No I/O, no spawn_blocking, no await (NFR-01)
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `session_id` not in registry (`record_category_store`) | Silent no-op; no panic, no error |
| `session_id` not in registry (`get_category_histogram`) | Returns `HashMap::new()` |
| Mutex poison | Recovered via `unwrap_or_else(|e| e.into_inner())` |
| Empty `category` string | Never reaches this layer; `category_validate` in `context_store` handler rejects it |

---

## Key Test Scenarios

See `test-plan/session.md` for the full test plan. Key scenarios:

1. **AC-01 / FR-01**: After `register_session`, `get_category_histogram` returns empty map.

2. **AC-02 / FR-02 + R-03 (gate blocker)**: Call `record_category_store` once with
   category `"decision"`. Assert histogram = `{"decision": 1}`. Call again with same
   session/category. Assert histogram = `{"decision": 2}` (two separate stores, different
   entries — this is distinct from the duplicate guard test in the tools.rs layer).

3. **AC-03 / R-04 (gate blocker)**: Call `record_category_store("nonexistent-session", "decision")`
   on a fresh `SessionRegistry`. Assert no panic. Assert `get_category_histogram("nonexistent-session")`
   returns empty map.

4. **Multi-category accumulation**: Register session. Call `record_category_store` with
   categories `"decision"` × 3, `"pattern"` × 2. Assert `get_category_histogram` returns
   `{"decision": 3, "pattern": 2}`, total = 5.

5. **Reconnection reset**: Register session, accumulate histogram, call `register_session`
   again with same `session_id`. Assert `get_category_histogram` returns empty map (field
   reinitialized to `HashMap::new()` on re-registration).

6. **EC-01 (session with exactly one store)**: `p(category) = 1.0` after one store.
   Assert total = 1, count = 1, no panic.
