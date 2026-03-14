# Component Test Plan: effectiveness-state

**Source**: `crates/unimatrix-server/src/services/effectiveness.rs` (new file)
**Risk coverage**: R-01 (Critical), R-06 (High), R-07 (High), R-10 (Medium)

---

## Unit Test Expectations

All tests in `#[cfg(test)] mod tests` within `services/effectiveness.rs`, mirroring the pattern established in `services/confidence.rs`.

### AC-03 / R-10 — Utility Constants Are Correct (in `unimatrix-engine::effectiveness`)

These tests belong in `crates/unimatrix-engine/src/effectiveness/tests_classify.rs` (cumulative extension).

**Test**: `test_utility_boost_constant_value`
- Assert `UTILITY_BOOST == 0.05_f64`

**Test**: `test_settled_boost_constant_value`
- Assert `SETTLED_BOOST == 0.01_f64`

**Test**: `test_utility_penalty_constant_value`
- Assert `UTILITY_PENALTY == 0.05_f64`

**Test**: `test_settled_boost_less_than_co_access_max`
- Assert `SETTLED_BOOST < 0.03`, verifying Constraint 5 / AC-03
- Inline comment: "co-access boost maximum is 0.03; SETTLED_BOOST must not exceed it"

### AC-06 / R-07 — Cold-Start (Empty State) Correctness

**Test**: `test_effectiveness_state_new_returns_empty`
- Call `EffectivenessState::new()`
- Assert `categories.is_empty()`
- Assert `consecutive_bad_cycles.is_empty()`
- Assert `generation == 0`

**Test**: `test_effectiveness_state_handle_type_alias`
- Construct `let handle: EffectivenessStateHandle = Arc::new(RwLock::new(EffectivenessState::new()))`
- Acquire read lock, assert `categories.len() == 0`
- This confirms the type alias compiles and is usable

### ADR-001 / R-01 — Generation Counter Semantics

**Test**: `test_generation_starts_at_zero`
- `EffectivenessState::new()` — assert `state.generation == 0`

**Test**: `test_generation_increments_on_write`
- Create handle, acquire write lock, insert one category entry, increment generation to 1, release
- Acquire read lock, assert `generation == 1`
- Repeat once more, assert `generation == 2`

**Test**: `test_generation_read_write_no_simultaneous_locks`
- Acquire read lock, copy `generation` field, **drop** the read guard
- Then acquire write lock and assert no deadlock
- Drop write guard, acquire read again — assert consistent state
- This is the structural test for R-01 lock ordering: read guard must be dropped before acquiring a second lock

### R-06 — EffectivenessSnapshot Shared via Arc<Mutex<_>>

**Test**: `test_effectiveness_snapshot_generation_match`
- Create an `EffectivenessSnapshot { generation: 0, categories: HashMap::new() }`
- Wrap in `Arc<Mutex<_>>`
- Clone the Arc (simulating rmcp clone)
- Update via original Arc (generation = 1)
- Assert clone sees generation = 1 via the same Arc
- Confirms that `Arc<Mutex<EffectivenessSnapshot>>` shares state across clones (R-06 structural check)

### Poison Recovery

**Test**: `test_effectiveness_state_handle_poison_recovery`
- Create handle
- Use `std::panic::catch_unwind` to acquire write lock and panic inside
- After catch, attempt `handle.read().unwrap_or_else(|e| e.into_inner())`
- Assert read succeeds (poisoned lock recovered via `into_inner`)
- Assert `categories` is in whatever state was written before the panic (no data loss from prior writes)

---

## Integration Test Expectations

This component has no standalone MCP-visible surface — it is purely internal state. Integration coverage is via background-tick-writer (which writes this state) and search-utility-delta (which reads it). See those component test plans.

---

## Edge Cases

| Scenario | Expected | Test Type |
|----------|----------|-----------|
| Empty state, read lock acquired while no entries | Returns empty HashMap, no panic | Unit |
| Generation u64 overflow | Wraps to 0 (u64 semantics) — document that 2^64 writes is not a practical concern | Doc comment |
| Concurrent reads from multiple clones | All see consistent snapshot | Unit (concurrent reads test) |
| Write lock acquired after read guard dropped | No deadlock | Unit (R-01 structural) |
