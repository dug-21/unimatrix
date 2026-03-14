## ADR-001: Generation Counter in EffectivenessState to Avoid Per-Search HashMap Clone

### Context

`SearchService::search()` needs a snapshot of `EffectivenessState.categories` (a
`HashMap<u64, EffectivenessCategory>`) for every search call. The naive implementation
acquires a read lock and clones the HashMap on every call.

SR-02 identifies this as a scalability concern: at 500 entries the clone is ~32KB and expected
to stay under 1ms, but the cost grows linearly with entry count. At 5,000 entries the clone is
~320KB, and at 50,000 it becomes a per-search bottleneck.

`EffectivenessState` is written at most once every 15 minutes (one background tick). In the
vast majority of `search()` calls, the HashMap content is unchanged since the previous call.

The `ConfidenceState` pattern from crt-019 does not have this problem because `SearchService`
only snapshots a single `f64` field (`confidence_weight`), not an entire `HashMap`.

Two alternatives were considered:

1. **Always clone** — simple, correct, but linear cost growth.
2. **Generation counter + cached local copy** — skip the clone when the generation has not
   changed since the last snapshot. Clone only when the generation increments (once per tick).
3. **Snapshot version + `Arc<HashMap>`** — replace the owned `HashMap` in `EffectivenessState`
   with `Arc<HashMap<...>>`, so readers clone the `Arc` (pointer copy) rather than the map.
   Writers replace the entire `Arc` under the write lock.

### Decision

Store a `generation: u64` counter in `EffectivenessState` that is incremented on every write.
`SearchService` and `BriefingService` cache the last-seen generation alongside a local
`HashMap<u64, EffectivenessCategory>` copy. On each `search()` or `assemble()` call, they
acquire a read lock, compare generations, and only re-clone when the generation has changed.

Because `SearchService` and `BriefingService` are both `Clone` (required by rmcp), the cache
fields are wrapped in `Arc<Mutex<_>>` so the cached copy is shared across clones of the same
service instance. This prevents each clone from maintaining an independent stale copy.

Concrete example:
```rust
struct EffectivenessSnapshot {
    generation: u64,
    categories: HashMap<u64, EffectivenessCategory>,
}

// In SearchService:
cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>

// In search():
let categories = {
    let guard = self.effectiveness_state.read().unwrap_or_else(|e| e.into_inner());
    let mut cache = self.cached_snapshot.lock().unwrap_or_else(|e| e.into_inner());
    if cache.generation != guard.generation {
        cache.generation = guard.generation;
        cache.categories = guard.categories.clone();
    }
    cache.categories.clone() // <-- this clone is the only one per 15-min window
};
```

The `Arc<HashMap>` alternative (option 3) was rejected because it requires changing the
`EffectivenessState` write pattern (full map rebuild on every tick instead of incremental
update), and it does not eliminate the need for a per-call `Arc` clone. The generation counter
approach reuses the existing `HashMap` mutation model and eliminates the clone on the hot path.

### Consequences

Easier:
- Search and briefing paths avoid HashMap clone on the common case (no state change since
  previous call).
- Clone cost is amortized to once per background tick, not per query.
- The approach generalizes to any other per-search stateful snapshot.

Harder:
- `SearchService` and `BriefingService` each carry an additional `Arc<Mutex<EffectivenessSnapshot>>`
  field, adding mild complexity.
- Tests must account for the generation-counter invalidation when exercising snapshot freshness.
- A double lock (`effectiveness_state.read()` then `cached_snapshot.lock()`) introduces a
  potential deadlock surface — mitigated by ensuring the read lock is released before acquiring
  the cache lock (never hold both simultaneously).
