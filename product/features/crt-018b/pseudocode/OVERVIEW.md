# crt-018b Pseudocode Overview

## Components Involved

| Component | File | Role |
|-----------|------|------|
| EffectivenessState cache | `services/effectiveness.rs` (new) | In-memory state, type definitions, handle factory |
| Background tick writer | `background.rs` (modified) | Sole writer; updates state after each `compute_report()` |
| Search utility delta | `services/search.rs` (modified) | Reads snapshot; applies `utility_delta` at 4 call sites |
| Briefing tiebreaker | `services/briefing.rs` (modified) | Reads snapshot; applies `effectiveness_priority` in 2 sort paths |
| Auto-quarantine guard | `background.rs` (modified, same file as tick writer) | Scans counters post-write; calls `quarantine_entry()` per threshold hit |
| Auto-quarantine audit | `background.rs` + `unimatrix-engine::effectiveness` (modified) | Audit events + `EffectivenessReport.auto_quarantined_this_cycle` |

Auto-quarantine guard and audit live in `background.rs` alongside the tick writer. Their pseudocode
is separated into distinct files for clarity but the implementation agent should understand these
reside in the same file and share local variables within `maintenance_tick`.

---

## Shared Types Introduced or Modified

### New: `EffectivenessState` (`services/effectiveness.rs`)

```
EffectivenessState {
    categories: HashMap<u64, EffectivenessCategory>   // entry_id -> last-known category
    consecutive_bad_cycles: HashMap<u64, u32>         // entry_id -> consecutive Ineffective/Noisy tick count
    generation: u64                                   // incremented on every write
}
```

### New: `EffectivenessStateHandle` (`services/effectiveness.rs`)

```
type EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>
```

### New: `EffectivenessSnapshot` (`services/effectiveness.rs`)

```
EffectivenessSnapshot {
    generation: u64,
    categories: HashMap<u64, EffectivenessCategory>,
}
// Held as Arc<Mutex<EffectivenessSnapshot>> in SearchService and BriefingService
// Shared across rmcp-cloned instances (ADR-001)
```

### Modified: `EffectivenessReport` (`unimatrix-engine/src/effectiveness/mod.rs`)

```
// Existing fields unchanged; add:
auto_quarantined_this_cycle: Vec<u64>
```

### New Constants (`unimatrix-engine/src/effectiveness/mod.rs`)

```
pub const UTILITY_BOOST:    f64 = 0.05;
pub const SETTLED_BOOST:    f64 = 0.01;
pub const UTILITY_PENALTY:  f64 = 0.05;
```

---

## Data Flow Between Components

```
[background.rs] maintenance_tick()
    |
    +--> compute_report() -> StatusReport { effectiveness: Some(EffectivenessReport) }
    |
    +--> [on Ok] write lock EffectivenessState
    |       update categories
    |       update consecutive_bad_cycles
    |       increment generation
    |       DROP write lock
    |
    +--> [write lock released] auto-quarantine scan
    |       collect entries where consecutive_bad_cycles >= threshold
    |       for each: quarantine_entry() inside spawn_blocking
    |       emit audit event per quarantine
    |       reset counter for quarantined entries
    |
    +--> [on compute_report() Err] emit tick_skipped audit event
         do NOT touch EffectivenessState

[services/search.rs] SearchService::search()
    |
    +--> read lock EffectivenessState -> get generation
    |    DROP read lock
    |    lock cached_snapshot.mutex -> compare generation, clone if changed
    |    DROP mutex
    |
    +--> Step 7 sort: base_score = rerank_score + utility_delta + prov_boost; final = base_score * penalty
    +--> Step 8 sort: final = (rerank_score + utility_delta + boost + prov_boost) * penalty

[services/briefing.rs] BriefingService::assemble()
    |
    +--> read lock EffectivenessState -> get generation
    |    DROP read lock
    |    lock cached_snapshot.mutex -> compare generation, clone if changed
    |    DROP mutex
    |
    +--> process_injection_history: sort by (confidence DESC, effectiveness_priority DESC)
    +--> convention lookup: sort by (feature_tag, confidence DESC, effectiveness_priority DESC)
    +--> semantic search: delegates to SearchService (already applies utility_delta)
```

---

## Lock Ordering Invariant (ADR-001, R-01, R-13)

Two strict rules govern all lock acquisition in this feature:

**Rule 1 — Double-lock ordering in search/briefing snapshot (R-01)**

```
ACQUIRE effectiveness_state.read()    -- RwLock read guard
READ guard.generation into local var
DROP read guard                       -- MUST drop before next acquisition
ACQUIRE cached_snapshot.lock()        -- Mutex guard (different lock type)
compare generations, clone if needed
DROP mutex guard
-- Never hold both guards simultaneously
```

**Rule 2 — Write lock released before SQL (NFR-02, R-13)**

```
[In maintenance_tick, background.rs]

ACQUIRE effectiveness_state.write()   -- RwLock write guard
  update categories
  update consecutive_bad_cycles
  increment generation
  collect to_quarantine: Vec<u64>     -- read counters while holding write lock
DROP write guard                      -- MUST drop before any quarantine_entry() call

-- Write guard is now out of scope (or explicitly drop()'d)
for entry_id in to_quarantine:
    quarantine_entry(entry_id, ...)   -- SQL write; no EffectivenessState lock held
```

---

## Sequencing Constraints

1. `unimatrix-engine::effectiveness` constants must be added before any consuming crate compiles.
2. `services/effectiveness.rs` (new file) must exist before `services/mod.rs`, `search.rs`,
   `briefing.rs`, and `background.rs` are modified.
3. `services/mod.rs` changes (adding `EffectivenessStateHandle` to `ServiceLayer` and wiring into
   constructors) must be applied after both `effectiveness.rs` and the modified `BriefingService::new()`
   signature are in place — otherwise the construction site fails to compile.
4. `background.rs` modifications depend on `EffectivenessStateHandle` being importable from `services`.
5. `AUTO_QUARANTINE_CYCLES` startup validation must happen before `spawn_background_tick` is called
   in `main.rs`.

---

## Pattern Source References

| Pattern | Where it lives | crt-018b usage |
|---------|---------------|----------------|
| `Arc<RwLock<_>>` state cache | `services/confidence.rs` `ConfidenceState` | `EffectivenessState` mirrors this struct |
| Short read-lock snapshot | `services/search.rs` line 125-131 | Same pattern, extended with generation cache |
| Background tick as sole writer | `background.rs` `maintenance_tick()` | `EffectivenessState` written only here |
| Poison recovery | `confidence.rs` all lock ops | `.unwrap_or_else(|e| e.into_inner())` on all new lock ops |
| Required constructor param | `SearchService::new()` `confidence_state` | `BriefingService::new()` `effectiveness_state` |
| Additive query-time delta | `coaccess.rs` `compute_search_boost` | `utility_delta` follows same additive model |
| spawn_blocking quarantine | `server.rs` quarantine path | Auto-quarantine reuses this path |
