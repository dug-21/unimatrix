# col-031: background.rs Tick Integration — Pseudocode

File: `crates/unimatrix-server/src/background.rs`
Status: MODIFIED

---

## Purpose

Thread `PhaseFreqTableHandle` through the full tick call chain and call
`PhaseFreqTable::rebuild` after `TypedGraphState::rebuild` in `run_single_tick`.
The pattern is structurally identical to how `TypedGraphStateHandle` was threaded
in crt-021 — read that code as the direct template.

---

## Import Addition

Add to imports at top of `background.rs`:

```
use crate::services::phase_freq_table::{PhaseFreqTable, PhaseFreqTableHandle};
```

---

## Change 1: `spawn_background_tick` Signature

Add `phase_freq_table: PhaseFreqTableHandle` as the last parameter.
Keep all existing parameters unchanged — add only this one:

```
pub fn spawn_background_tick(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    entry_store: Arc<Store>,
    pending_entries: Arc<Mutex<PendingEntriesAnalysis>>,
    tick_metadata: Arc<Mutex<TickMetadata>>,
    training_service: Option<Arc<TrainingService>>,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,
    typed_graph_state: TypedGraphStateHandle,
    contradiction_cache: ContradictionScanCacheHandle,
    audit_log: Arc<AuditLog>,
    auto_quarantine_cycles: u32,
    confidence_params: Arc<ConfidenceParams>,
    ml_inference_pool: Arc<RayonPool>,
    nli_enabled: bool,
    nli_auto_quarantine_threshold: f32,
    nli_handle: Arc<NliServiceHandle>,
    inference_config: Arc<InferenceConfig>,
    phase_freq_table: PhaseFreqTableHandle,  // col-031: required non-optional (ADR-005)
) -> tokio::task::JoinHandle<()>
```

### Inner `tokio::spawn(background_tick_loop(...))` call

The supervisor loop clones all Arc params for each inner spawn. Add the clone:

```
let inner_handle = tokio::spawn(background_tick_loop(
    Arc::clone(&store),
    Arc::clone(&vector_index),
    Arc::clone(&embed_service),
    Arc::clone(&adapt_service),
    Arc::clone(&session_registry),
    Arc::clone(&entry_store),
    Arc::clone(&pending_entries),
    Arc::clone(&tick_metadata),
    training_service.clone(),
    confidence_state.clone(),
    effectiveness_state.clone(),
    typed_graph_state.clone(),
    Arc::clone(&contradiction_cache),
    Arc::clone(&audit_log),
    auto_quarantine_cycles,
    Arc::clone(&confidence_params),
    Arc::clone(&ml_inference_pool),
    nli_enabled,
    nli_auto_quarantine_threshold,
    Arc::clone(&nli_handle),
    Arc::clone(&inference_config),
    phase_freq_table.clone(),  // col-031: Arc::clone via .clone() (same as typed_graph_state)
));
```

---

## Change 2: `background_tick_loop` Signature

Add `phase_freq_table: PhaseFreqTableHandle` as the last parameter.
Mirror the pattern of the existing `typed_graph_state` parameter with its comment:

```
async fn background_tick_loop(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    entry_store: Arc<Store>,
    pending_entries: Arc<Mutex<PendingEntriesAnalysis>>,
    tick_metadata: Arc<Mutex<TickMetadata>>,
    _training_service: Option<Arc<TrainingService>>,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,
    typed_graph_state: TypedGraphStateHandle,
    contradiction_cache: ContradictionScanCacheHandle,
    audit_log: Arc<AuditLog>,
    auto_quarantine_cycles: u32,
    confidence_params: Arc<ConfidenceParams>,
    ml_inference_pool: Arc<RayonPool>,
    nli_enabled: bool,
    nli_auto_quarantine_threshold: f32,
    nli_handle: Arc<NliServiceHandle>,
    inference_config: Arc<InferenceConfig>,
    phase_freq_table: PhaseFreqTableHandle,  // col-031: threaded to run_single_tick
)
```

### `run_single_tick` call in `background_tick_loop`

Pass the handle through:

```
let tick_result = run_single_tick(
    &store,
    &vector_index,
    &embed_service,
    &adapt_service,
    &session_registry,
    &entry_store,
    &pending_entries,
    &tick_metadata,
    &mut extraction_ctx,
    neural_enhancer.as_ref(),
    shadow_evaluator.as_mut(),
    &confidence_state,
    &effectiveness_state,
    &typed_graph_state,
    &contradiction_cache,
    current_tick,
    &audit_log,
    auto_quarantine_cycles,
    tick_interval_secs,
    &ml_inference_pool,
    nli_enabled,
    nli_auto_quarantine_threshold,
    &nli_handle,
    &inference_config,
    &confidence_params,
    &phase_freq_table,  // col-031: passed by reference (mirrors typed_graph_state pattern)
)
.await;
```

---

## Change 3: `run_single_tick` Signature

Add `phase_freq_table: &PhaseFreqTableHandle` as the last parameter:

```
async fn run_single_tick(
    store: &Arc<Store>,
    vector_index: &Arc<VectorIndex>,
    embed_service: &Arc<EmbedServiceHandle>,
    adapt_service: &Arc<AdaptationService>,
    session_registry: &SessionRegistry,
    entry_store: &Arc<Store>,
    pending_entries: &Arc<Mutex<PendingEntriesAnalysis>>,
    tick_metadata: &Arc<Mutex<TickMetadata>>,
    extraction_ctx: &mut ExtractionContext,
    neural_enhancer: Option<&NeuralEnhancer>,
    shadow_evaluator: Option<&mut ShadowEvaluator>,
    confidence_state: &ConfidenceStateHandle,
    effectiveness_state: &EffectivenessStateHandle,
    typed_graph_state: &TypedGraphStateHandle,
    contradiction_cache: &ContradictionScanCacheHandle,
    current_tick: u32,
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
    tick_interval_secs: u64,
    ml_inference_pool: &Arc<RayonPool>,
    nli_enabled: bool,
    nli_auto_quarantine_threshold: f32,
    nli_handle: &Arc<NliServiceHandle>,
    inference_config: &Arc<InferenceConfig>,
    confidence_params: &Arc<ConfidenceParams>,
    phase_freq_table: &PhaseFreqTableHandle,  // col-031: required (ADR-005)
) -> Result<(), String>
```

---

## Change 4: PhaseFreqTable Rebuild in `run_single_tick`

Insert the rebuild block immediately AFTER the TypedGraphState rebuild block
(which ends at approximately line 567 in the existing file — the closing `}` of
the `{ let store_clone = ... }` block).

Before writing the block, add the lock ordering comment:

```
// col-031: PhaseFreqTable rebuild.
//
// LOCK ACQUISITION ORDER in run_single_tick (SR-07, NFR-03):
//   1. EffectivenessStateHandle  -- acquired and released during maintenance_tick above
//   2. TypedGraphStateHandle     -- acquired and released in the block above this one
//   3. PhaseFreqTableHandle      -- acquired here (write, swap only)
//
// Each handle is acquired, data extracted or swapped, and released before the next
// is acquired. No two locks are held simultaneously. No lock is held across an
// await point.
//
// Retain-on-error semantics (R-09, AC-04):
//   On rebuild success  -> write lock acquired; *guard = new_table; lock released.
//   On rebuild failure  -> NO write to the handle. Existing state retained.
//                          tracing::error! emitted. Tick continues.
//   On rebuild timeout  -> Same as failure: existing state retained; warning emitted.
//
// Cold-start: if this is the first tick, the existing state has use_fallback=true.
// After a successful rebuild, use_fallback=false (assuming non-empty result).
// The search path sees use_fallback=false on the next query after this tick.
{
    let store_clone = Arc::clone(store);
    let lookback_days = inference_config.query_log_lookback_days;

    match tokio::time::timeout(
        TICK_TIMEOUT,
        tokio::spawn(async move {
            PhaseFreqTable::rebuild(&store_clone, lookback_days).await
        }),
    )
    .await
    {
        Ok(Ok(Ok(new_table))) => {
            // Success: swap under write lock.
            let mut guard = phase_freq_table
                .write()
                .unwrap_or_else(|e| e.into_inner());
            *guard = new_table;
            tracing::debug!("PhaseFreqTable rebuilt successfully");
            // guard drops here — write lock released
        }
        Ok(Ok(Err(e))) => {
            // Store error: log; retain existing state (R-09).
            tracing::error!(
                error = %e,
                "PhaseFreqTable rebuild failed: store error; retaining existing state"
            );
            // No write to phase_freq_table handle.
        }
        Ok(Err(join_err)) => {
            // Spawned task panicked: log; retain existing state.
            tracing::error!(
                error = %join_err,
                "PhaseFreqTable rebuild task panicked; retaining existing state"
            );
        }
        Err(_timeout) => {
            // Timeout: log warning; retain existing state.
            tracing::warn!(
                timeout_secs = TICK_TIMEOUT.as_secs(),
                "PhaseFreqTable rebuild timed out; retaining existing state"
            );
        }
    }
}
```

---

## Change 5: `main.rs` — Thread Handle from ServiceLayer to spawn_background_tick

In `main.rs` (or wherever `spawn_background_tick` is called), add the handle argument.
This is NOT a `background.rs` change — it is a call-site change in the binary crate.
Document for the implementation agent:

```
// In main.rs, after ServiceLayer construction:
let phase_freq_table_handle = service_layer.phase_freq_table_handle();

// Pass to spawn_background_tick as the last argument:
spawn_background_tick(
    // ... all existing arguments ...
    phase_freq_table_handle,   // col-031
)
```

The accessor `service_layer.phase_freq_table_handle()` is defined in service_layer.md.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `PhaseFreqTable::rebuild` returns store error | Log `tracing::error!`; retain existing handle state |
| Spawned task panics | JoinError caught; log `tracing::error!`; retain existing handle state |
| Timeout | Log `tracing::warn!`; retain existing handle state |
| Write lock poisoned on swap | `.unwrap_or_else(|e| e.into_inner())` recovers; swap proceeds |

The error branch MUST NOT write to the handle — omitting the write IS the retain-on-error
behavior. A common mistake is writing `*guard = PhaseFreqTable::new()` in the error branch,
which resets an active table to cold-start. Do not do this (R-09).

---

## Key Test Scenarios

### AC-04: Retain-on-error semantics (R-09 guard)

```
// Create a SearchService with a populated PhaseFreqTableHandle (use_fallback=false,
// table has one entry). Inject a mock store that returns Err from query_phase_freq_table.
// Call run_single_tick (or equivalent).
// Assert:
//   - PhaseFreqTableHandle still has use_fallback=false after the tick.
//   - The pre-tick table entries are still present.
//   - tracing::error! was emitted.
```

### AC-04: Success swap semantics

```
// Create a SearchService with cold-start handle (use_fallback=true).
// Seed query_log with rows that will produce a non-empty rebuild.
// Call run_single_tick (or equivalent, using test DB).
// Assert:
//   - PhaseFreqTableHandle now has use_fallback=false.
//   - table is non-empty.
```

### R-14: Compile-time wiring check

```
// Removing the phase_freq_table parameter from any of the three function signatures
// (spawn_background_tick, background_tick_loop, run_single_tick) must produce
// a compile error. The non-optional parameter type enforces this (ADR-005).
```

### R-12: Lock order comment present

```
// Code review: the block containing PhaseFreqTable rebuild must have a comment
// naming the lock acquisition order:
//   1. EffectivenessStateHandle
//   2. TypedGraphStateHandle
//   3. PhaseFreqTableHandle
// This comment is the only enforcement mechanism for lock ordering (NFR-03).
```
