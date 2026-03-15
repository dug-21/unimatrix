# crt-018b: Effectiveness-Driven Retrieval — Architecture

## System Overview

crt-018b activates the effectiveness classification system (delivered in crt-018) as a live
retrieval signal. Prior to this feature, the five-category classification (Effective, Settled,
Unmatched, Ineffective, Noisy) is computed on every `context_status` call but is never fed back
into search re-ranking or briefing assembly. Entries with demonstrably bad outcomes rank identically
to entries with proven utility.

This feature adds three runtime behaviors on top of the existing stack:

1. A shared in-memory cache (`EffectivenessState`) written by the background tick and read by the
   search and briefing paths — following the `ConfidenceState` pattern established in crt-019.
2. An additive utility delta in the search re-ranking formula for the Flexible (MCP) retrieval path.
3. An effectiveness-weighted tiebreaker in briefing assembly for injection history and convention
   lookup paths.
4. An N-cycle consecutive-bad-classification auto-quarantine guard triggered from the background
   maintenance tick.

No new MCP tools are added. No new database tables or columns are added. All new state is in-memory.

## Component Breakdown

### Component 1: `EffectivenessState` and `EffectivenessStateHandle`

**Location**: `crates/unimatrix-server/src/services/effectiveness.rs` (new file)

**Responsibility**: In-memory cache of per-entry effectiveness classifications and consecutive-bad-
cycle counters. This is the single source of truth for query-time effectiveness data.

**Fields**:
```rust
pub struct EffectivenessState {
    /// entry_id -> EffectivenessCategory, populated by the background tick.
    pub categories: HashMap<u64, EffectivenessCategory>,
    /// entry_id -> consecutive cycle count where the entry was Ineffective or Noisy.
    /// Reset to 0 when an entry moves to any other category.
    pub consecutive_bad_cycles: HashMap<u64, u32>,
}
pub type EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>;
```

**Lifecycle**:
- Created once at server startup (empty), held by `ServiceLayer` alongside `ConfidenceStateHandle`.
- Populated by the background tick after each `compute_report()` call.
- Read by `SearchService` and `BriefingService` under short read locks.
- The handle is cloned (cheap `Arc::clone`) to wire into `SearchService`, `BriefingService`, and
  the background tick path.

**Cold-start**: Empty on startup. All utility deltas are `0.0` until the first background tick
(approximately 15 minutes after server start). This is safe — the system degrades to pre-crt-018b
behavior, not to broken behavior.

**Snapshot version counter** (ADR-001): `EffectivenessState` includes a `generation: u64` field
incremented atomically on each write. `SearchService` and `BriefingService` cache the last-seen
generation and skip the `HashMap` clone when the generation is unchanged since their previous call.

---

### Component 2: Background Tick Writer (modification to `background.rs`)

**Location**: `crates/unimatrix-server/src/background.rs`

**Responsibility**: After `compute_report()` succeeds in `maintenance_tick()`, extract the
`EffectivenessReport` from `StatusReport.effectiveness` and write the classification map and
consecutive-bad-cycles counters to `EffectivenessState` under a write lock.

**Error semantics** (ADR-002 — critical): If `compute_report()` returns an error, the write is
skipped entirely. `consecutive_bad_cycles` values are held at their existing levels — they do NOT
increment on a failed tick. A structured audit event is emitted for every skipped tick with
`operation = "tick_skipped"` and the error reason, enabling operators to detect tick failures and
understand that auto-quarantine logic was paused.

**Write logic**:
1. On success: for each entry in `report.effectiveness`, update `categories[entry_id]`.
   - If category is `Ineffective` or `Noisy`: increment `consecutive_bad_cycles[entry_id]` by 1.
   - Otherwise: set `consecutive_bad_cycles[entry_id] = 0` (or remove the key).
   - Entries absent from the report (no longer active) have their counter entries removed.
2. Increment `generation` before releasing the write lock.
3. Check auto-quarantine (Component 4) while holding the write lock (counters are already updated).

**What does NOT write `EffectivenessState`**: `context_status` MCP calls. Phase 8 runs inside
every `compute_report()` call but its output flows only to `StatusReport.effectiveness` for display.
Only the background tick writes `EffectivenessState`.

---

### Component 3: Search Utility Delta (modification to `services/search.rs`)

**Location**: `crates/unimatrix-server/src/services/search.rs`

**Responsibility**: Apply a per-entry additive delta based on effectiveness category to the
re-ranking formula at all four `rerank_score` call sites.

**Snapshot pattern**: At the top of `SearchService::search()`, immediately after snapshotting
`confidence_weight` from `ConfidenceStateHandle`, snapshot `EffectivenessState` using the generation
cache:
```rust
let (categories_snapshot, cached_generation) = {
    let guard = self.effectiveness_state.read().unwrap_or_else(|e| e.into_inner());
    if guard.generation != self.cached_generation {
        self.cached_generation = guard.generation;
        self.cached_categories = guard.categories.clone();
    }
    (self.cached_categories.clone(), self.cached_generation)
};
```

NOTE: `SearchService` is `Clone`, so the cache fields must be `Arc<Mutex<_>>` or a separate
inner struct. See ADR-001 for the chosen approach.

**Delta function**:
```rust
fn utility_delta(category: Option<EffectivenessCategory>) -> f64 {
    match category {
        Some(Effective)   =>  UTILITY_BOOST,   // +0.05
        Some(Settled)     =>  SETTLED_BOOST,   // +0.01
        Some(Ineffective) => -UTILITY_PENALTY, // -0.05
        Some(Noisy)       => -UTILITY_PENALTY, // -0.05
        Some(Unmatched) | None => 0.0,
    }
}
```

Constants (`UTILITY_BOOST`, `SETTLED_BOOST`, `UTILITY_PENALTY`) are defined in
`unimatrix-engine::effectiveness` module.

**Integration with existing formula**: The delta is applied at Steps 7 and 8 (the two sort passes).
In Step 7, the additive delta is included in `base_a`/`base_b` before the penalty multiplier. In
Step 8 (co-access re-sort), the delta is included alongside `boost_a`, `prov_a`, and `penalty_a`.
This preserves the multiplicative penalty semantics — the utility delta is inside the penalty
multiplication, not outside it (ADR-003 for rationale).

**Scope**: Applies only to `RetrievalMode::Flexible` (the MCP path). `RetrievalMode::Strict` (UDS
hard-filter path) already excludes all non-Active entries; no delta is needed.

**Full combined formula** (spread = 0.20, confidence_weight = 0.25):
```
final_score = (
    (1 - confidence_weight) * similarity
    + confidence_weight * confidence
    + utility_delta
    + provenance_boost
    + co_access_boost
) * status_penalty
```
Where:
- `confidence_weight` in [0.15, 0.25] (adaptive from crt-019)
- `utility_delta` in {-0.05, 0.0, +0.01, +0.05}
- `provenance_boost` = 0.02 for lesson-learned, else 0.0
- `co_access_boost` in [0.0, 0.03]
- `status_penalty` = 0.5 (superseded), 0.7 (deprecated), 1.0 (active)

---

### Component 4: Briefing Effectiveness Tiebreaker (modification to `services/briefing.rs`)

**Location**: `crates/unimatrix-server/src/services/briefing.rs`

**Responsibility**: Incorporate the effectiveness category as a secondary sort key in the injection
history and convention lookup sort paths.

**Constructor change** (SR-06 — required): `BriefingService::new()` takes
`EffectivenessStateHandle` as a required parameter. This is non-optional; missing wiring is a
compile error. The handle is stored in `BriefingService` alongside `SearchService`.

**Sort key**: A pure function maps category to a sort priority integer:
```rust
fn effectiveness_priority(category: Option<EffectivenessCategory>) -> i32 {
    match category {
        Some(Effective)   =>  2,
        Some(Settled)     =>  1,
        None | Some(Unmatched) => 0,
        Some(Ineffective) => -1,
        Some(Noisy)       => -2,
    }
}
```

**Injection history sort** (`process_injection_history`): The three group sorts (decisions,
injections, conventions) currently sort by confidence descending. They become composite sorts:
primary = confidence descending, secondary = `effectiveness_priority` descending.

**Convention lookup sort**: The convention entries currently sort by feature tag first, then
confidence descending. The effectiveness tiebreaker is added after confidence: feature tag first,
then confidence descending, then `effectiveness_priority` descending.

**Semantic search path**: No change needed. This path delegates to `SearchService::search()`, which
already applies the utility delta from Component 3.

**Snapshot**: `assemble()` takes a read lock on `EffectivenessState` once at the start and passes
the cloned `HashMap` to the sort comparators. No per-entry locking.

---

### Component 5: Auto-Quarantine Guard (modification to `background.rs`)

**Location**: `crates/unimatrix-server/src/background.rs`

**Responsibility**: After the `EffectivenessState` write in the background tick, scan
`consecutive_bad_cycles` for entries that have reached the threshold and trigger quarantine for
each one via the existing `store.quarantine_entry()` path.

**Configuration**: `AUTO_QUARANTINE_CYCLES: u32` read from `UNIMATRIX_AUTO_QUARANTINE_CYCLES`
env var (default: 3). Value 0 disables auto-quarantine entirely.

**Trigger condition**: `consecutive_bad_cycles[entry_id] >= AUTO_QUARANTINE_CYCLES && AUTO_QUARANTINE_CYCLES > 0`

**Quarantine execution**:
- Called from within the `spawn_blocking` block already used for maintenance operations.
- Calls the synchronous store quarantine path: `store.quarantine_entry(entry_id, pre_quarantine_status, reason)`.
- After each successful quarantine: reset `consecutive_bad_cycles[entry_id] = 0` (idempotent —
  the entry will no longer appear in active classification on the next tick).
- Fire-and-forget confidence recompute via `confidence_service.recompute(&[entry_id])`.
- Write a structured audit event (Component 6).

**Constraint**: Auto-quarantine only applies to `Ineffective` and `Noisy` entries. An entry with
`Settled`, `Unmatched`, or `Effective` classification that happens to have a nonzero
`consecutive_bad_cycles` counter (stale from a prior bad run) will have its counter reset to 0 on
the next write, but will not be quarantined (AC-14).

---

### Component 6: Auto-Quarantine Audit Event

**Location**: written from `background.rs` using the `AuditLog` infrastructure.

**Responsibility**: Provide operators with enough information to understand why an entry was
automatically quarantined and to restore it if the classification was a false positive.

**Audit event fields** (SR-03 — required):
```
operation: "auto_quarantine"
agent_id:  "system"
target_ids: [entry_id]
detail: "auto-quarantine: entry '{title}' (id={entry_id}, category={category},
         consecutive_bad_cycles={n}, topic={topic}) quarantined after {n} consecutive
         background maintenance ticks classified as {category}"
outcome: Success
```

The `title`, `topic`, and final `category` are captured from the `EntryEffectiveness` data
available in the `EffectivenessReport` that was computed in the same tick.

**Tick-skipped audit event** (SR-07 — required):
```
operation: "tick_skipped"
agent_id:  "system"
detail:    "background tick compute_report failed: {error_reason}"
outcome:   Failure
```
This event is emitted whenever `compute_report()` returns an error. No state changes occur when a
tick is skipped.

---

## Component Interactions

```
background.rs (maintenance_tick)
  |
  |-- compute_report() [StatusService]
  |     |-- Phase 8: classify all active entries -> EffectivenessReport
  |
  |-- [on success] acquire write lock on EffectivenessState
  |     |-- update categories HashMap
  |     |-- update consecutive_bad_cycles HashMap
  |     |-- increment generation counter
  |
  |-- [on success, if AUTO_QUARANTINE_CYCLES > 0]
  |     |-- scan consecutive_bad_cycles for threshold crossings
  |     |-- call store.quarantine_entry() for each
  |     |-- emit audit event per quarantine
  |
  |-- [on compute_report() error]
        |-- hold consecutive_bad_cycles (no increment)
        |-- emit tick_skipped audit event

SearchService::search()
  |-- snapshot confidence_weight from ConfidenceStateHandle
  |-- snapshot categories from EffectivenessStateHandle (generation-cached)
  |-- [steps 7 & 8] apply utility_delta(categories[entry_id]) in re-rank formula

BriefingService::assemble()
  |-- [injection history path] sort with effectiveness tiebreaker
  |-- [convention lookup path] sort with effectiveness tiebreaker
  |-- [semantic search path] delegates to SearchService (already benefits from delta)
```

## Technology Decisions

See individual ADR files:

- ADR-001: Generation counter for HashMap clone avoidance in hot path
- ADR-002: Hold (not increment) consecutive_bad_cycles on tick error
- ADR-003: Utility delta position inside vs. outside the status_penalty multiplication
- ADR-004: EffectivenessStateHandle as non-optional BriefingService constructor parameter

## Integration Points

### Existing Patterns Reused

| Pattern | Source | Usage in crt-018b |
|---------|--------|--------------------|
| `Arc<RwLock<_>>` state cache | `ConfidenceState` (crt-019) | `EffectivenessState` follows same structure |
| Background tick as sole writer | `ConfidenceState` (crt-019) | `EffectivenessState` written only in `maintenance_tick()` |
| Short read-lock snapshot at search top | `SearchService` (crt-019) | Same pattern for categories snapshot |
| Required constructor param | `ConfidenceStateHandle` in `SearchService` | `EffectivenessStateHandle` in `BriefingService` |
| Additive query-time delta | Co-access boost, provenance boost | `utility_delta` follows same pattern |
| spawn_blocking quarantine | `quarantine_with_audit` (server.rs) | Auto-quarantine calls synchronous store path |
| Poison recovery | `CategoryAllowlist` convention | `.unwrap_or_else(|e| e.into_inner())` on all lock ops |

### Modified Components

| Component | File | Modification |
|-----------|------|-------------|
| `EffectivenessState` (new) | `services/effectiveness.rs` | New file |
| `ServiceLayer` | `services/mod.rs` | Add `EffectivenessStateHandle` field; wire to search + briefing |
| `SearchService` | `services/search.rs` | Add `effectiveness_state` field; apply utility delta in Steps 7+8 |
| `BriefingService` | `services/briefing.rs` | Add `effectiveness_state` field via constructor; sort tiebreaker |
| `background.rs` | `background.rs` | Write `EffectivenessState` after `compute_report()`; auto-quarantine |
| `spawn_background_tick` | `background.rs` | Add `EffectivenessStateHandle` parameter |
| `unimatrix-engine::effectiveness` | `effectiveness/mod.rs` | Add `UTILITY_BOOST`, `SETTLED_BOOST`, `UTILITY_PENALTY` constants |

### Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `EffectivenessState` | `struct { categories: HashMap<u64, EffectivenessCategory>, consecutive_bad_cycles: HashMap<u64, u32>, generation: u64 }` | new: `services/effectiveness.rs` |
| `EffectivenessStateHandle` | `Arc<RwLock<EffectivenessState>>` | new: `services/effectiveness.rs` |
| `UTILITY_BOOST` | `pub const f64 = 0.05` | `unimatrix-engine::effectiveness` |
| `UTILITY_PENALTY` | `pub const f64 = 0.05` | `unimatrix-engine::effectiveness` |
| `SETTLED_BOOST` | `pub const f64 = 0.01` | `unimatrix-engine::effectiveness` |
| `AUTO_QUARANTINE_CYCLES` | `u32`, default 3, from `UNIMATRIX_AUTO_QUARANTINE_CYCLES` env var | new: `background.rs` |
| `utility_delta(category: Option<EffectivenessCategory>) -> f64` | pure fn | new: `services/search.rs` or `engine::effectiveness` |
| `effectiveness_priority(category: Option<EffectivenessCategory>) -> i32` | pure fn | new: `services/briefing.rs` |
| `BriefingService::new()` signature | adds `effectiveness_state: EffectivenessStateHandle` | modified: `services/briefing.rs` |
| `spawn_background_tick()` signature | adds `effectiveness_state: EffectivenessStateHandle` | modified: `background.rs` |
| `EffectivenessReport.auto_quarantined_this_cycle` | `Vec<u64>` field added | modified: `unimatrix-engine::effectiveness` |
| audit event `operation = "auto_quarantine"` | `AuditEvent` with `agent_id = "system"` | new: written from `background.rs` |
| audit event `operation = "tick_skipped"` | `AuditEvent` with `agent_id = "system"` | new: written from `background.rs` |

## Data Flow

### Search Path (Flexible mode)

```
Agent calls context_search
-> SearchService::search()
   -> snapshot confidence_weight (ConfidenceStateHandle, read lock)
   -> snapshot categories (EffectivenessStateHandle, read lock + generation check)
   -> embed query, HNSW search, quarantine filter, status filter/penalty
   -> Step 7: sort by rerank_score(sim, conf, cw) + utility_delta(categories[id]) + prov_boost
              then * status_penalty
   -> Step 8: co-access boost + re-sort with same formula + boost
   -> truncate to k, apply floors, emit audit
```

### Briefing Path

```
Agent calls context_briefing
-> BriefingService::assemble()
   -> snapshot categories (EffectivenessStateHandle, read lock)
   -> injection history: sort by (confidence DESC, effectiveness_priority DESC)
   -> convention lookup: sort by (feature_tag DESC, confidence DESC, effectiveness_priority DESC)
   -> semantic search: delegates to SearchService (already benefits from utility delta)
```

### Background Tick Write Path

```
Tick fires every 15 minutes
-> maintenance_tick()
   -> compute_report(None, None, false) [StatusService]
      -> Phase 8: classify all active entries
      -> return StatusReport with effectiveness: Some(EffectivenessReport)
   -> [on Ok]: acquire write lock on EffectivenessState
      -> update categories from report.effectiveness
      -> update consecutive_bad_cycles (increment for bad, reset for recovered)
      -> increment generation
      -> scan for auto-quarantine threshold
      -> for each threshold hit: quarantine, emit audit, reset counter
      -> release write lock
   -> [on Err]: emit tick_skipped audit event; do NOT modify EffectivenessState
   -> run_maintenance() [ConfidenceState write, graph compaction, etc.]
```

## Error Boundaries

| Boundary | Error Handling |
|----------|---------------|
| `compute_report()` error | Hold `consecutive_bad_cycles`; emit `tick_skipped` audit event; continue to next tick |
| `store.quarantine_entry()` error | Log warning; skip that entry; continue to next candidate |
| `EffectivenessState` lock poison | `.unwrap_or_else(|e| e.into_inner())` on all read and write ops |
| `SearchService` with empty state | `categories.get(id)` returns `None`; `utility_delta(None) = 0.0` |
| `BriefingService` with empty state | Same: `None` maps to priority `0` — sort degrades to confidence-only |

## Constraints Honored

1. No stored confidence formula change — `W_BASE + ... + W_TRUST = 0.92` invariant unchanged.
2. No new database tables or columns — all new state is in-memory.
3. Performance budget — generation-cached snapshot avoids HashMap clone on successive calls when
   state has not changed. Clone only occurs after each background tick (once per 15 minutes).
4. Cold-start is safe — empty state produces zero utility delta, not incorrect behavior.
5. Server restart resets consecutive counters — in-memory only, intentional.
6. Auto-quarantine is in `spawn_blocking` — synchronous SQLite compatible.
7. Test infrastructure extends existing `TestDb`, effectiveness tests, and search pipeline tests.
8. `RetrievalMode::Strict` path is unmodified — utility delta only applies to Flexible mode.
