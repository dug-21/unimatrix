# Component: EffectivenessState Cache

**File**: `crates/unimatrix-server/src/services/effectiveness.rs` (new file)

**Purpose**: Define the in-memory state container and its thread-safe handle type for per-entry
effectiveness classifications. Provides the cold-start constructor and a factory method that mirrors
`ConfidenceState::new_handle()`. This file is purely type and constructor definitions — no business
logic resides here.

---

## Imports Required

```
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use unimatrix_engine::effectiveness::EffectivenessCategory;
```

---

## Structs

### `EffectivenessState`

```
pub struct EffectivenessState {
    /// entry_id -> last-known EffectivenessCategory, populated by background tick.
    /// Absent key means: not yet classified. utility_delta = 0.0 for absent keys.
    pub categories: HashMap<u64, EffectivenessCategory>,

    /// entry_id -> count of consecutive background ticks where the entry was
    /// classified Ineffective or Noisy. Absent key means counter = 0.
    /// In-memory only; resets to empty on server restart (intentional, Constraint 6).
    pub consecutive_bad_cycles: HashMap<u64, u32>,

    /// Incremented on every write to EffectivenessState.
    /// Readers compare against their cached generation to decide whether to re-clone
    /// the categories HashMap (ADR-001). Only the background tick writer increments this.
    pub generation: u64,
}
```

### `EffectivenessSnapshot`

```
pub struct EffectivenessSnapshot {
    /// The generation value at the time this snapshot was taken.
    pub generation: u64,
    /// Clone of EffectivenessState.categories at snapshot time.
    pub categories: HashMap<u64, EffectivenessCategory>,
}
```

`EffectivenessSnapshot` is held as `Arc<Mutex<EffectivenessSnapshot>>` in `SearchService` and
`BriefingService`. This wrapping ensures all rmcp-cloned instances of a service share the same
cached copy (R-06 mitigation).

---

## Type Alias

```
pub type EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>;
```

---

## `impl EffectivenessState`

### `new() -> EffectivenessState`

```
function new() -> EffectivenessState:
    return EffectivenessState {
        categories: HashMap::new(),
        consecutive_bad_cycles: HashMap::new(),
        generation: 0,
    }
```

Cold-start semantics: empty maps produce `utility_delta = 0.0` for all entries. No fallback or
guard logic is required — absence of a key is the correct zero-delta sentinel (NFR-06, AC-06).

### `new_handle() -> EffectivenessStateHandle`

```
function new_handle() -> EffectivenessStateHandle:
    return Arc::new(RwLock::new(EffectivenessState::new()))
```

Mirrors `ConfidenceState::new_handle()`. Called once by `ServiceLayer::with_rate_config()` to
create the shared handle, which is then `Arc::clone`-d into `SearchService`, `BriefingService`,
and `spawn_background_tick`.

---

## `impl EffectivenessSnapshot`

### `new_shared() -> Arc<Mutex<EffectivenessSnapshot>>`

```
function new_shared() -> Arc<Mutex<EffectivenessSnapshot>>:
    return Arc::new(Mutex::new(EffectivenessSnapshot {
        generation: 0,
        categories: HashMap::new(),
    }))
```

Used by `SearchService::new()` and `BriefingService::new()` to initialize the per-service
snapshot cache. The `Arc` wrapper ensures clones of the same service share one cache instance.

---

## Error Handling

All lock acquisitions on `EffectivenessStateHandle` and `Arc<Mutex<EffectivenessSnapshot>>`
throughout the codebase must use poison recovery:

```
// Read lock on EffectivenessStateHandle:
let guard = handle.read().unwrap_or_else(|e| e.into_inner());

// Write lock on EffectivenessStateHandle:
let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());

// Mutex on cached_snapshot:
let mut cache = cached_snapshot.lock().unwrap_or_else(|e| e.into_inner());
```

Never use `.unwrap()` or `.expect()` on these locks. A poisoned lock is recovered (the stale
state is used) rather than causing a panic that would cascade to all subsequent search calls
(Security Risk 3 from RISK-TEST-STRATEGY).

---

## Exports from `services/mod.rs`

Add to `services/mod.rs`:

```
pub(crate) mod effectiveness;
pub use effectiveness::{EffectivenessState, EffectivenessStateHandle};
// EffectivenessSnapshot is pub(crate) only — internal to services
```

---

## Key Test Scenarios

**Scenario 1 — Cold-start state is empty and produces zero deltas (AC-06, NFR-06)**
- Create `EffectivenessState::new()`
- Assert `categories.is_empty() == true`
- Assert `consecutive_bad_cycles.is_empty() == true`
- Assert `generation == 0`
- Call `utility_delta(state.categories.get(&999).copied())` and assert result is `0.0`

**Scenario 2 — `new_handle()` produces an independent Arc (not shared state)**
- Create two handles via `new_handle()`
- Write to handle1: insert a category
- Read from handle2: assert categories is still empty
- (Confirms each call produces a distinct `Arc<RwLock<_>>`)

**Scenario 3 — Poison recovery: poisoned read lock does not panic (Security Risk 3)**
- Create a handle
- In a separate thread, acquire write lock, panic while holding it (to poison the lock)
- In the main thread, call `handle.read().unwrap_or_else(|e| e.into_inner())`
- Assert no panic; assert the stale state is accessible

**Scenario 4 — Snapshot shared across clones (R-06)**
- Create `Arc<Mutex<EffectivenessSnapshot>>` via `new_shared()`
- Clone the Arc
- Via clone 1: lock and update `generation` to 5, insert a category
- Via clone 2: lock and read `generation`
- Assert clone 2 sees `generation == 5` (confirming shared backing object)
