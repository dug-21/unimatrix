## ADR-004: Feature-Cycle-Keyed Accumulator with Mutex and TTL Eviction

### Context

The scope document (OQ-05) resolves the `pending_entries_analysis` refactor:
"`pending_entries_analysis` becomes a `HashMap<feature_cycle, Vec<EntryRecord>>`."
The risk assessment (SR-05) identifies the eviction policy as undefined: "stale buckets
for completed features accumulate indefinitely."

The current `PendingEntriesAnalysis` is a flat `HashMap<u64, EntryAnalysis>` (keyed by
entry ID), protected by `Arc<Mutex<PendingEntriesAnalysis>>`. Its 1000-entry cap
evicts by lowest `rework_flag_count`. There is no per-feature-cycle concept.

In daemon mode, multiple sessions working on different features accumulate entries
concurrently. `context_retrospective` specifies a `topic` (feature cycle string) and
should drain only that bucket, leaving others intact.

Two structural questions:

1. **Inner representation**: `Vec<EntryRecord>` (per scope) or `HashMap<u64, EntryAnalysis>` (current)?
   The current flat map uses entry ID as key to support `upsert` (merge rework counts
   across sessions for the same entry). This semantics is richer and should be
   preserved. The outer structure gains a `feature_cycle` key; the inner structure
   stays `HashMap<u64, EntryAnalysis>`.

2. **Mutex vs RwLock**: The prior pattern in this codebase for shared state is:
   - `Arc<RwLock<T>>` when the background tick is the sole writer and reads are frequent
     (pattern #1560).
   - `Arc<Mutex<T>>` when both reads and writes are short-duration and write frequency
     is comparable to read frequency.
   `PendingEntriesAnalysis` is written on every hook stop event and read (drain) once per
   `context_retrospective` call. Write frequency is high relative to reads. Both
   operations are short-duration (HashMap insert/remove). The `RwLock` read/write
   asymmetry provides no benefit here — writers cannot starve because there is no
   long-held read lock. Using `Mutex` is consistent with the existing type (already
   `Arc<Mutex<PendingEntriesAnalysis>>`) and avoids introducing an upgrade pattern.

3. **Eviction policy**: SR-05 asks when and how stale buckets are evicted.

Three eviction triggers considered:
- **On `context_cycle` call**: `context_cycle` is the "feature is done" signal
  (col-022). When called, its bucket is drained and removed.
- **On `context_retrospective` drain**: The drain call removes all entries for the
  given feature cycle. If retrospective is called before cycle close, the bucket is
  emptied but the feature may accumulate more entries. This is acceptable — a
  subsequent retrospective call on the same feature will see only post-drain entries.
- **TTL sweep by background tick**: Buckets not touched in 72 hours are evicted by the
  background tick. This prevents permanent accumulation from features that never call
  retrospective or cycle.

### Decision

Restructure `PendingEntriesAnalysis` as follows:

```rust
pub struct PendingEntriesAnalysis {
    /// Outer key: feature_cycle string (e.g., "vnc-005").
    /// Inner key: entry_id u64.
    pub buckets: HashMap<String, FeatureBucket>,
    pub created_at: u64,
}

pub struct FeatureBucket {
    pub entries: HashMap<u64, EntryAnalysis>,
    pub last_updated: u64,  // unix seconds — for TTL eviction
}
```

API changes:
- `upsert(feature_cycle: &str, analysis: EntryAnalysis)` — creates bucket if absent,
  merges entry, updates `last_updated`.
- `drain_for(feature_cycle: &str) -> Vec<EntryAnalysis>` — removes and returns all
  entries for the given feature cycle bucket.
- `evict_stale(now: u64, ttl_secs: u64)` — removes buckets where
  `now - last_updated > ttl_secs`. Called by the background tick.

Eviction policy: **all three triggers**:
1. `drain_for` in `context_retrospective` empties the bucket (entries cleared, bucket
   removed).
2. `context_cycle` calls `drain_for` as well (cycle close implicitly finalizes the
   retrospective accumulator).
3. Background tick calls `evict_stale(now, 72 * 3600)` each tick. This is the safety
   net for features that complete without calling retrospective.

The 1000-entry cap is now applied per-bucket, not globally, to preserve per-feature
eviction semantics. Each bucket caps at 1000 entries; excess entries are evicted by
lowest `rework_flag_count` (same policy as today).

The synchronization primitive stays `Arc<Mutex<PendingEntriesAnalysis>>`. No change
to the protection model — just the internal structure.

### Consequences

Easier:
- `context_retrospective` now spans all sessions that worked on a feature cycle, not
  just the current session's accumulated data. This is strictly richer.
- Stale bucket accumulation is bounded by the 72-hour TTL.
- The existing `Arc<Mutex<PendingEntriesAnalysis>>` sharing between UDS listener and
  MCP server requires no structural change — just the inner HashMap key hierarchy.

Harder:
- The UDS listener's `PendingEntriesAnalysis::upsert` calls must be updated to pass
  the `feature_cycle` string. The listener already has access to the active
  `feature_cycle` via the `SessionRegistry` (the session's `feature_cycle` field set
  by `context_cycle`). Implementation must extract this correctly.
- `context_retrospective` must pass the `topic` parameter as the `feature_cycle` key
  to `drain_for`. This is a minor API change to the retrospective handler.
- The `evict_stale` call in the background tick is new work. The tick already runs
  every 15 minutes; adding a HashMap sweep over typically 1-3 buckets is negligible.
