# Component: confidence-state

**File**: `crates/unimatrix-server/src/services/confidence.rs` (MODIFIED)

## Purpose

Holds the runtime-variable quad `{alpha0, beta0, observed_spread,
confidence_weight}` that the maintenance tick updates and query paths read.
This is the server-side state that keeps the formula's engine layer stateless.
`ConfidenceState` is a pure data struct; `ConfidenceStateHandle` is its
thread-safe wrapper.

The existing `ConfidenceService` struct (which does fire-and-forget batch
recomputation) is extended: it now holds a `ConfidenceStateHandle` and exposes
it for wiring. `ConfidenceService::recompute()` is also updated to read
`alpha0`/`beta0` from the handle before spawning.

## New Structs and Type Alias

```
pub(crate) struct ConfidenceState {
    pub alpha0:            f64,  // Bayesian prior positive pseudo-votes
    pub beta0:             f64,  // Bayesian prior negative pseudo-votes
    pub observed_spread:   f64,  // p95-p5 confidence spread of active population
    pub confidence_weight: f64,  // clamp(observed_spread * 1.25, 0.15, 0.25)
}

pub(crate) type ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>
```

## Initialization

```
fn ConfidenceState::new() -> ConfidenceState:
    ConfidenceState {
        alpha0:            3.0,    // COLD_START_ALPHA
        beta0:             3.0,    // COLD_START_BETA
        observed_spread:   0.1471, // pre-crt-019 measured value (R-06)
        confidence_weight: 0.184,  // clamp(0.1471 * 1.25, 0.15, 0.25) = 0.184
    }
```

The `0.1471` initial value is critical (R-06). Using `0.0` gives
`confidence_weight = 0.15` (floor) from server start until the first
maintenance tick — a regression. Using the measured pre-crt-019 value gives
`confidence_weight = 0.184` immediately.

`confidence_weight` is derived from `observed_spread`, not independently
settable. But it must be stored so readers can clone a single f64 cheaply
without re-computing the clamp each time.

## Modified ConfidenceService

The existing `ConfidenceService` gains a `state: ConfidenceStateHandle` field.

```
pub(crate) struct ConfidenceService {
    store: Arc<Store>,
    state: ConfidenceStateHandle,   // NEW
}

fn ConfidenceService::new(store: Arc<Store>) -> ConfidenceService:
    ConfidenceService {
        store,
        state: Arc::new(RwLock::new(ConfidenceState::new())),
    }

// NEW: accessor for wiring into SearchService and StatusService
fn ConfidenceService::state_handle(&self) -> ConfidenceStateHandle:
    Arc::clone(&self.state)
```

## Modified ConfidenceService::recompute

The existing `recompute(&self, entry_ids: &[u64])` must be updated to capture
`alpha0`/`beta0` from `ConfidenceState` before spawning.

```
fn ConfidenceService::recompute(&self, entry_ids: &[u64]):
    if entry_ids.is_empty():
        return

    let store = Arc::clone(&self.store)
    let ids = entry_ids.to_vec()

    // Snapshot the prior BEFORE spawn_blocking (on async thread — no lock issues)
    let (alpha0, beta0) = {
        let guard = self.state.read().unwrap_or_else(|e| e.into_inner())
        (guard.alpha0, guard.beta0)
    }

    let _ = tokio::task::spawn_blocking(move || {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()

        for id in ids:
            match store.get(id):
                Ok(entry) =>
                    // CHANGED: pass captured alpha0/beta0
                    let conf = compute_confidence(&entry, now, alpha0, beta0)
                    if let Err(e) = store.update_confidence(id, conf):
                        tracing::warn!("confidence recompute failed for {id}: {e}")
                Err(e) =>
                    tracing::warn!("confidence recompute: entry {id} not found: {e}")
    })
```

## ServiceLayer Wiring

`ServiceLayer::with_rate_config` must thread `ConfidenceStateHandle` to both
`SearchService` (reader) and `StatusService` (writer). The handle is cloned
(cheap Arc clone) and passed to each service constructor.

```
// In services/mod.rs ServiceLayer::with_rate_config:

let confidence = ConfidenceService::new(Arc::clone(&store))
let confidence_state_handle = confidence.state_handle()   // NEW

// Pass handle to SearchService constructor:
let search = SearchService::new(
    Arc::clone(&store),
    Arc::clone(&vector_store),
    Arc::clone(&entry_store),
    Arc::clone(&embed_service),
    Arc::clone(&adapt_service),
    Arc::clone(&gateway),
    Arc::clone(&confidence_state_handle),  // NEW parameter
)

// Pass handle to StatusService constructor:
let status = StatusService::new(
    Arc::clone(&store),
    Arc::clone(&vector_index),
    Arc::clone(&embed_service),
    Arc::clone(&adapt_service),
    Arc::clone(&confidence_state_handle),  // NEW parameter
)
```

Both `SearchService::new` and `StatusService::new` signatures gain a
`ConfidenceStateHandle` parameter. The handle field is stored as
`Arc<RwLock<ConfidenceState>>` (not cloned on each call — stored once).

## Read Pattern (SearchService, readers)

```
// In search.rs, before the re-ranking step, read the weight once:
let confidence_weight = {
    let guard = self.confidence_state
        .read()
        .unwrap_or_else(|e| e.into_inner())
    guard.confidence_weight  // clone f64 — cheap
}
// Then pass to all four rerank_score call sites in the search pipeline
```

The read lock is released immediately after cloning the f64. Do NOT hold the
read lock across the entire search loop (R-09: read-starvation risk is
negligible for a 4-field struct write, but the pattern should be clean).

## Write Pattern (StatusService, writer)

```
// In status.rs Step 2b, after computing new values:
{
    let mut guard = confidence_state_handle
        .write()
        .unwrap_or_else(|e| e.into_inner())
    guard.alpha0            = new_alpha0
    guard.beta0             = new_beta0
    guard.observed_spread   = new_observed_spread
    guard.confidence_weight = adaptive_confidence_weight(new_observed_spread)
}
// Write lock released at end of block
```

A single write covers all four fields atomically (ADR-002). The write lock is
held for the duration of the struct update only (microseconds), not for the
entire maintenance tick.

## re-export in services/mod.rs

```
// Add to services/mod.rs:
pub(crate) use confidence::{ConfidenceService, ConfidenceState, ConfidenceStateHandle}
```

## Error Handling

`RwLock::read()` and `RwLock::write()` return `PoisonError` if a writer panicked
while holding the lock. All acquisitions MUST use:
```
.unwrap_or_else(|e| e.into_inner())
```
This matches the `CategoryAllowlist` convention (FM-03). Do not use `.unwrap()`
or `.expect()` — poison propagation would crash query paths.

## Key Test Scenarios

```
// ConfidenceState initialization (R-06):
confidence_state_initial_spread:
    let state = ConfidenceState::new()
    assert_eq!(state.observed_spread, 0.1471)
    assert!((state.confidence_weight - 0.184).abs() < 0.001)
    assert_eq!(state.alpha0, 3.0)
    assert_eq!(state.beta0, 3.0)

// confidence_weight derived correctly:
confidence_state_weight_derived_from_spread:
    let state = ConfidenceState::new()
    let expected = adaptive_confidence_weight(state.observed_spread)
    assert!((state.confidence_weight - expected).abs() < f64::EPSILON)

// ConfidenceStateHandle: write then read (basic roundtrip):
confidence_state_handle_write_read:
    let handle: ConfidenceStateHandle = Arc::new(RwLock::new(ConfidenceState::new()))
    {
        let mut guard = handle.write().unwrap_or_else(|e| e.into_inner())
        guard.alpha0 = 5.0
        guard.beta0  = 2.0
        guard.observed_spread = 0.20
        guard.confidence_weight = adaptive_confidence_weight(0.20)
    }
    let read_guard = handle.read().unwrap_or_else(|e| e.into_inner())
    assert_eq!(read_guard.alpha0, 5.0)
    assert_eq!(read_guard.confidence_weight, 0.25)

// ServiceLayer wires handle (IR-01):
service_layer_confidence_state_wired:
    // Construct a ServiceLayer, trigger a maintenance tick that updates ConfidenceState,
    // then call context_search and verify the confidence_weight used > initial default
    // (integration test — see test-infrastructure.md)
```
