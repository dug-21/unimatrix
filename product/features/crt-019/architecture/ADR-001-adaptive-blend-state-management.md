## ADR-001: Adaptive Blend State Management

### Context

`rerank_score(similarity, confidence)` currently reads `SEARCH_SIMILARITY_WEIGHT = 0.85` as a
compiled constant. The adaptive blend (Change 4) requires this weight to be runtime-variable:
`confidence_weight = clamp(observed_spread * 1.25, 0.15, 0.25)`, updated each maintenance tick.

SR-03 identified two options:

**Option A — Parameter passing**: Add `confidence_weight: f64` as a parameter to `rerank_score`
and all call sites. The engine remains stateless. The server layer reads the runtime weight from
shared state and passes it in at each call.

**Option B — Shared atomic**: Introduce `Arc<AtomicU64>` (f64 bits via `f64::to_bits`) in the
engine crate, shared between the refresh tick (writer) and the query path (reader). The engine
gains a state dependency.

Two additional sub-questions arose:

1. Where does `{ alpha0, beta0, observed_spread, confidence_weight }` live — all four values
   are computed together in the same tick and must be consistent with each other.
2. `AtomicU64` supports atomic reads/writes of a single f64 (via bit reinterpretation), but
   four f64 values cannot be updated atomically via `AtomicU64` without a lock.

### Decision

**Option A (parameter passing) with a server-side `RwLock<ConfidenceState>`.**

`rerank_score` gains a `confidence_weight: f64` parameter. The compiled constant
`SEARCH_SIMILARITY_WEIGHT` is removed. The engine crate remains stateless.

On the server side, a new `ConfidenceState` struct holds all four runtime values:

```rust
pub(crate) struct ConfidenceState {
    pub alpha0: f64,
    pub beta0: f64,
    pub observed_spread: f64,
    pub confidence_weight: f64,
}
```

`ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>` is shared between:
- The background tick (`StatusService::run_maintenance`) — **write** at the end of each tick.
- `SearchService::search` — **read** only long enough to clone `confidence_weight` as an f64.
- The confidence refresh loop — **read** to snapshot `alpha0`/`beta0` before iterating.

The `RwLock` write hold is a brief critical section: update four f64 fields and release.
This ensures all four values are consistent within any given read — no torn reads where a search
call sees the new `confidence_weight` with the old `alpha0`.

**`AtomicU64` was rejected** for the composite state case because atomic updates of four
independent values cannot be composed without a lock anyway, so `RwLock` is simpler and clearer.
Single-value atomics (e.g., for `confidence_weight` alone) were also rejected because
`confidence_weight` and `alpha0`/`beta0` must be consistent with each other — stale-reading
`confidence_weight` is acceptable (at most 15 minutes stale), but splitting the four values
across independent atomics risks confusion. `RwLock` makes consistency explicit.

**Option B was rejected** because introducing shared mutable state into `unimatrix-engine`
breaks its design invariant: all functions are pure and deterministic given inputs. Tests rely on
this property (direct unit tests with no mocking). Keeping state in the server layer preserves
testability and the module boundary.

The 4 `rerank_score` call sites in `search.rs` are close together. The mechanical update is
low-risk. Tests `search_similarity_weight_is_f64` and `rerank_score_f64_precision` must be
updated to use the parameter form.

### Consequences

**Easier:**
- `unimatrix-engine` remains fully stateless; all engine tests remain pure unit tests.
- The adaptive blend behavior is observable from server-side state alone.
- Four computed values are always consistent with each other within a search call.

**Harder:**
- All 4 `rerank_score` call sites in `search.rs` must pass `confidence_weight`.
- `SearchService` must hold a `ConfidenceStateHandle` (thread-safe — `Arc<RwLock<_>>`).
- `ServiceLayer::new` must wire the handle to both `SearchService` and `StatusService`.
- `compute_confidence` calls in the refresh loop must pass the snapshotted `alpha0`/`beta0`
  before the loop begins (not re-read per entry, to avoid lock contention per iteration).
