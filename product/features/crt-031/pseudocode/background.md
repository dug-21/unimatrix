# Component Pseudocode: background (maintenance tick lifecycle guard stub)

## Purpose

Add `Arc<CategoryAllowlist>` as a new parameter to `maintenance_tick`, `background_tick_loop`,
`spawn_background_tick`, and `run_single_tick`. Insert the Step 10b lifecycle guard stub between
Step 10 (`run_maintenance`) and Step 11 (`run_dead_knowledge_migration_v1`). Update the
`StatusService::new()` call in `run_single_tick` to pass the operator-loaded Arc (not a freshly
constructed default).

---

## Context: Current Signatures

### `spawn_background_tick` (22 params, public)
```
pub fn spawn_background_tick(
    store: Arc<Store>,                              // 1
    vector_index: Arc<VectorIndex>,                 // 2
    embed_service: Arc<EmbedServiceHandle>,         // 3
    adapt_service: Arc<AdaptationService>,          // 4
    session_registry: Arc<SessionRegistry>,         // 5
    entry_store: Arc<Store>,                        // 6
    pending_entries: Arc<Mutex<PendingEntriesAnalysis>>, // 7
    tick_metadata: Arc<Mutex<TickMetadata>>,        // 8
    training_service: Option<Arc<TrainingService>>, // 9
    confidence_state: ConfidenceStateHandle,        // 10
    effectiveness_state: EffectivenessStateHandle,  // 11
    typed_graph_state: TypedGraphStateHandle,       // 12
    contradiction_cache: ContradictionScanCacheHandle, // 13
    audit_log: Arc<AuditLog>,                       // 14
    auto_quarantine_cycles: u32,                    // 15
    confidence_params: Arc<ConfidenceParams>,       // 16
    ml_inference_pool: Arc<RayonPool>,              // 17
    nli_enabled: bool,                              // 18
    nli_auto_quarantine_threshold: f32,             // 19
    nli_handle: Arc<NliServiceHandle>,              // 20
    inference_config: Arc<InferenceConfig>,         // 21
    phase_freq_table: PhaseFreqTableHandle,         // 22
) -> tokio::task::JoinHandle<()>
```

### `background_tick_loop` (22 params, private async fn)
Same params as `spawn_background_tick` minus the return type change.

### `run_single_tick` (26 params including reference params, private async fn)
Existing params include `confidence_params`, `phase_freq_table` as refs.

### `maintenance_tick` (11 params, private async fn)
```
async fn maintenance_tick(
    status_svc: &StatusService,                     // 1
    session_registry: &SessionRegistry,             // 2
    entry_store: &Arc<Store>,                       // 3
    pending_entries: &Arc<Mutex<PendingEntriesAnalysis>>, // 4
    effectiveness_state: &EffectivenessStateHandle, // 5
    audit_log: &Arc<AuditLog>,                      // 6
    auto_quarantine_cycles: u32,                    // 7
    store: &Arc<Store>,                             // 8
    nli_enabled: bool,                              // 9
    nli_auto_quarantine_threshold: f32,             // 10
    inference_config: &Arc<InferenceConfig>,        // 11
) -> Result<(), ServiceError>
```

---

## Modified: `spawn_background_tick` (22 → 23 params)

### New parameter (appended as param 23)
```
category_allowlist: Arc<CategoryAllowlist>,    // crt-031: lifecycle policy for Step 10b stub
```

### Body change (one new line in the `tokio::spawn` inner call)
The body spawns `background_tick_loop` by cloning all Arcs. Add:
```
Arc::clone(&category_allowlist),    // crt-031: pass category_allowlist to tick loop
```
As the final argument to `background_tick_loop(...)`.

### Import addition (if not already present)
```
use crate::infra::categories::CategoryAllowlist;
```

### Full new signature
```
#[allow(clippy::too_many_arguments)]  // already present
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
    phase_freq_table: PhaseFreqTableHandle,
    category_allowlist: Arc<CategoryAllowlist>,     // NEW param 23
) -> tokio::task::JoinHandle<()>
```

---

## Modified: `background_tick_loop` (22 → 23 params)

### New parameter (appended as param 23)
```
category_allowlist: Arc<CategoryAllowlist>,    // crt-031: lifecycle policy for run_single_tick
```

### Body change
In the `run_single_tick(...)` call, add as final argument:
```
&category_allowlist,    // crt-031
```

---

## Modified: `run_single_tick` (gains one new reference param)

### New parameter (appended as final param)
```
category_allowlist: &Arc<CategoryAllowlist>,    // crt-031: lifecycle policy
```

### Body change 1: `StatusService::new()` call (~line 446)

#### Current
```
let status_svc = StatusService::new(
    Arc::clone(store),
    Arc::clone(vector_index),
    Arc::clone(embed_service),
    Arc::clone(adapt_service),
    Arc::clone(confidence_state),
    Arc::clone(confidence_params),
    Arc::clone(contradiction_cache),
    Arc::clone(ml_inference_pool),
    tick_observation_registry,
);
```

#### New
```
let status_svc = StatusService::new(
    Arc::clone(store),
    Arc::clone(vector_index),
    Arc::clone(embed_service),
    Arc::clone(adapt_service),
    Arc::clone(confidence_state),
    Arc::clone(confidence_params),
    Arc::clone(contradiction_cache),
    Arc::clone(ml_inference_pool),
    tick_observation_registry,
    Arc::clone(category_allowlist),    // NEW: operator-configured policy (crt-031, R-02 critical)
);
```

CRITICAL: This must be `Arc::clone(category_allowlist)` — the operator-loaded Arc threaded
from startup. Must NOT be `Arc::new(CategoryAllowlist::new())`. See R-02 / I-04 / FM-06.

### Body change 2: `maintenance_tick(...)` call

In the `tokio::time::timeout(TICK_TIMEOUT, maintenance_tick(...))` call, add the final argument:
```
category_allowlist,    // crt-031: &Arc<CategoryAllowlist>
```

---

## Modified: `maintenance_tick` (11 → 12 params)

### New parameter (appended as param 12)
```
category_allowlist: &Arc<CategoryAllowlist>,    // crt-031: lifecycle policy for Step 10b
```

### Body change: Step 10b inserted between Step 10 and Step 11

#### Existing Step 10 (unchanged)
```
// Step 10: Run existing maintenance logic (unchanged).
status_svc
    .run_maintenance(
        &active_entries,
        &mut report,
        session_registry,
        entry_store,
        pending_entries,
        inference_config,
    )
    .await?;
```

#### New Step 10b (inserted after Step 10, before Step 11)
```
// Step 10b: Lifecycle guard stub (crt-031) — #409 insertion point.
//
// Lists adaptive categories once per tick using a single lock acquisition.
// Only fires debug log when at least one adaptive category is configured (AC-10).
// This block is a no-op — no entries are modified.
//
// #409: insert auto-deprecation dispatch logic inside this block.
// Call `category_allowlist.is_adaptive(entry.category)` for each candidate entry.
// If is_adaptive returns false, skip unconditionally.
{
    let adaptive = category_allowlist.list_adaptive();
    if !adaptive.is_empty() {
        tracing::debug!(
            categories = ?adaptive,
            "lifecycle guard: adaptive categories eligible for auto-deprecation (stub, #409)"
        );
        // TODO(#409): for each candidate entry in these categories, call
        // category_allowlist.is_adaptive(category) before any deprecation action.
        // If is_adaptive returns false, skip. The outer guard is in place; #409 fills the body.
    }
}
```

#### Existing Step 11 (unchanged)
```
// Step 11: One-shot migration — bulk-deprecate existing noisy lesson-learned entries
// that were created by the old DeadKnowledgeRule extraction loop (GH #351).
// Gated by a COUNTERS marker so it runs exactly once per database.
run_dead_knowledge_migration_v1(store).await;
```

---

## Lock Safety Note (R-06)

The Step 10b stub calls `list_adaptive()` once (single lock acquisition for the adaptive set).
It does NOT call `is_adaptive()` per-category in a loop. The `list_adaptive()` lock guard is
released before the `tracing::debug!` call (the guard is scoped to `list_adaptive()`'s internal
body; it returns a `Vec<String>` and drops the guard before returning).

No lock guard is held across any `.await` point in `maintenance_tick`. The stub block is
synchronous — no async call inside it. The function returns `Result<(), ServiceError>` and
all existing await points are before (Steps 1–10) and after (Step 11, which is `async fn`
but the lock is already released).

---

## SR-02 / OQ-05 Deferral Note

`BackgroundTickConfig` composite struct is explicitly deferred out of scope. The 22→23
parameter growth is accepted. `#[allow(clippy::too_many_arguments)]` is already present on
all three functions (`spawn_background_tick`, `background_tick_loop`, `maintenance_tick`).
Confirm it is also present on `run_single_tick` before adding the new parameter there.
If not present, add it.

The PR description must reference this deferral and link to the crt-031 architecture document
(R-05 mitigation, AC from architecture constraints §7).

---

## Error Handling

`maintenance_tick` returns `Result<(), ServiceError>`. Step 10b introduces no new error paths:
`list_adaptive()` is infallible, `tracing::debug!` cannot fail. The outer `?` on `run_maintenance`
at Step 10 is unchanged.

If `maintenance_tick` is called with a poisoned `category_allowlist.adaptive` lock:
`list_adaptive()` recovers via `.unwrap_or_else(|e| e.into_inner())` (FM-02). No panic.

---

## Key Test Scenarios

### AC-10: debug log fires when adaptive is non-empty
```
test_lifecycle_stub_logs_when_adaptive_non_empty:
  // Use tracing_test or equivalent subscriber capture
  category_allowlist = Arc::new(CategoryAllowlist::from_categories_with_policy(
      INITIAL_CATEGORIES...,
      vec!["lesson-learned"],
  ))
  // Run maintenance_tick with this allowlist
  // Assert: tracing::debug! event fired with "lifecycle guard" message
  // The exact subscriber setup follows the existing tracing_test pattern in the test suite
```

### AC-10 complement: debug log does NOT fire when adaptive is empty
```
test_lifecycle_stub_silent_when_adaptive_empty:
  category_allowlist = Arc::new(CategoryAllowlist::from_categories_with_policy(
      INITIAL_CATEGORIES...,
      vec![],   // empty adaptive list
  ))
  // Run maintenance_tick with this allowlist
  // Assert: NO "lifecycle guard" debug event fired
```

### AC-11: stub is no-op
```
test_lifecycle_stub_is_noop:
  // Run maintenance_tick with a known CategoryAllowlist
  // Assert: no entries are modified in the store
  // Assert: the stub block exists with the #409 comment (code review)
  // Assert: step 10 and step 11 still execute (no early return from stub)
```

### R-02: run_single_tick passes operator Arc, not fresh default
```
test_run_single_tick_uses_operator_category_allowlist:
  // Construct a CategoryAllowlist with empty adaptive (not the default ["lesson-learned"])
  category_allowlist = Arc::new(CategoryAllowlist::from_categories_with_policy(
      INITIAL_CATEGORIES...,
      vec![],
  ))
  // Run a tick cycle that calls run_single_tick with this allowlist
  // Retrieve the StatusService's category_allowlist and assert list_adaptive() is empty
  // (not ["lesson-learned"] from a freshly constructed CategoryAllowlist::new())
```

### R-05: allow attribute present
```
// Pre-implementation check:
// grep -n "allow(clippy::too_many_arguments)" background.rs
// Confirm present on spawn_background_tick, background_tick_loop, maintenance_tick, run_single_tick
```
