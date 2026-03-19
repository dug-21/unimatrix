# Component Pseudocode: Call-Site Migration (7 Sites)

**Files to modify**: see Call-Site Inventory below.

---

## Purpose

Replace `spawn_blocking` and `spawn_blocking_with_timeout` at all 7 ONNX embedding
inference call sites in `unimatrix-server` with the equivalent `RayonPool` method.

- MCP handler paths: `spawn_blocking_with_timeout` → `rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`
- Background task paths: `spawn_blocking` → `rayon_pool.spawn(...)`

No changes to embedding logic, error types at the service boundary, or call order.

---

## How `rayon_pool` Reaches Each Call Site

### MCP handler paths (search, store_ops, store_correct, status, warmup)

Each service struct (`SearchService`, `StoreService`, `StatusService`) needs a
`rayon_pool: Arc<RayonPool>` field added. These structs are constructed in
`ServiceLayer::with_rate_config`.

Required changes to `ServiceLayer`:

```
// services/mod.rs — ServiceLayer struct
pub struct ServiceLayer {
    // existing fields unchanged
    pub(crate) ml_inference_pool: Arc<RayonPool>,  // new field (ADR-004)
    // TODO(W2-4): add gguf_rayon_pool: Arc<RayonPool> here
}
```

`ServiceLayer::new` and `ServiceLayer::with_rate_config` gain a new parameter:

```
pub fn new(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    audit: Arc<AuditLog>,
    usage_dedup: Arc<UsageDedup>,
    boosted_categories: HashSet<String>,
    ml_inference_pool: Arc<RayonPool>,   // new parameter
) -> Self
```

Inside `with_rate_config`, each service that calls embedding inference receives
`Arc::clone(&ml_inference_pool)` as a constructor argument:

```
let search = SearchService::new(
    ...,
    Arc::clone(&ml_inference_pool),  // new argument
);
let store_ops = StoreService::new(
    ...,
    Arc::clone(&ml_inference_pool),  // new argument
);
let status = StatusService::new(
    ...,
    Arc::clone(&ml_inference_pool),  // new argument
);
// BriefingService, ConfidenceService, UsageService do not embed — no change
```

Each of `SearchService`, `StoreService`, `StatusService` gains:

```
struct SearchService {
    // existing fields unchanged
    rayon_pool: Arc<RayonPool>,   // new field
}
```

### Background task paths (contradiction scan, quality-gate loop)

`spawn_background_tick` in `background.rs` already receives `Arc<EmbedServiceHandle>`.
It must also receive `Arc<RayonPool>`:

```
pub fn spawn_background_tick(
    // existing params unchanged
    ...
    ml_inference_pool: Arc<RayonPool>,   // new parameter
) -> JoinHandle<()>
```

The background tick's internal tasks access `rayon_pool` via a local clone.

### `uds/listener.rs` warmup path

`start_uds_listener` already receives `Arc<EmbedServiceHandle>`. It must also receive
`Arc<RayonPool>` to pass to the warmup path:

```
pub async fn start_uds_listener(
    // existing params unchanged
    ...
    ml_inference_pool: Arc<RayonPool>,   // new parameter
) -> ...
```

### `main.rs` wiring

In `tokio_main_daemon` and `tokio_main_stdio`, after constructing `arc_pool`:

```
let pool = RayonPool::new(config.inference.rayon_pool_size, "ml_inference_pool")
    .map_err(|e| ServerError::InferencePoolInit(e.to_string()))?;
    // or equivalent structured error wrapper

let arc_pool = Arc::new(pool);

// Pass to ServiceLayer
let services = ServiceLayer::new(
    ...,
    Arc::clone(&arc_pool),   // new argument
);

// Pass to background tick
let tick_handle = spawn_background_tick(
    ...,
    Arc::clone(&arc_pool),   // new argument
);

// Pass to UDS listener (for warmup)
let (uds_handle, socket_guard) = start_uds_listener(
    ...,
    Arc::clone(&arc_pool),   // new argument
).await?;

// TODO(W1-4): ml_inference_pool will also be accessed via AppState for NLI
// TODO(W2-4): add gguf_rayon_pool: Arc<RayonPool> here
```

---

## Migration Pattern

### Pattern A — MCP handler paths (`spawn_blocking_with_timeout` → `spawn_with_timeout`)

Before:
```
let raw_embedding: Vec<f32> = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, {
    let adapter = Arc::clone(&adapter);
    move || adapter.embed_entry(&title, &content)
})
.await
.map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
.map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;
```

After:
```
let raw_embedding: Vec<f32> = self.rayon_pool
    .spawn_with_timeout(MCP_HANDLER_TIMEOUT, {
        let adapter = Arc::clone(&adapter);
        move || adapter.embed_entry(&title, &content)
    })
    .await
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;
```

The double `.map_err` collapses to a single `.map_err` because `RayonPool::spawn_with_timeout`
returns `Result<T, RayonError>` where `T = Result<Vec<f32>, CoreError>`. The outer
`.map_err` maps `RayonError` to `ServiceError::EmbeddingFailed`; then the inner `?`
propagates the `CoreError` (embed failure) to `ServiceError::Core`.

Wait — the architecture diagram shows double `.map_err`. Read the current code carefully.

Looking at `search.rs` lines 228–234: `spawn_blocking_with_timeout` returns
`Result<T, ServerError>`. The first `.map_err` maps the outer `ServerError` (join/timeout
error) to `ServiceError::EmbeddingFailed`. The second `.map_err` maps `CoreError`
(the `T` from `adapter.embed_entry`) to `ServiceError::EmbeddingFailed`.

With `RayonPool::spawn_with_timeout` returning `Result<Result<Vec<f32>, CoreError>, RayonError>`:
- Outer `?` maps `RayonError` → `ServiceError::EmbeddingFailed`
- Inner `?` maps `CoreError` → `ServiceError::EmbeddingFailed` (or `ServiceError::Core`)

The double `.map_err` structure is preserved:
```
let raw_embedding: Vec<f32> = self.rayon_pool
    .spawn_with_timeout(MCP_HANDLER_TIMEOUT, {
        let adapter = Arc::clone(&adapter);
        move || adapter.embed_entry(&title, &content)
    })
    .await
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?  // maps RayonError
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?; // maps CoreError
```

This is the exact pattern shown in ARCHITECTURE.md §Call-Site Migration Pattern and
IMPLEMENTATION-BRIEF.md. Implementers must use it verbatim.

The `MCP_HANDLER_TIMEOUT` constant is imported from `crate::infra::timeout`:
```
use crate::infra::timeout::MCP_HANDLER_TIMEOUT;
// spawn_blocking_with_timeout import can be removed once all sites in the module migrate
```

### Pattern B — Background task paths (`spawn_blocking` → `spawn` with error logging)

Before (contradiction scan, `background.rs ~543`):
```
let result = spawn_blocking(move || {
    scan_contradictions(&adapter, &entries, ...)
})
.await
.unwrap_or_else(|e| { ... });
```

After:
```
let result = rayon_pool
    .spawn(move || {
        scan_contradictions(&adapter, &entries, ...)
    })
    .await;

match result {
    Ok(scan_output) => {
        // process scan_output
    }
    Err(e) => {
        error!(error = %e, "contradiction scan rayon task cancelled");
        // background tick continues; next tick will reattempt
    }
}
```

`RayonError::Cancelled` must emit an `error!()` tracing event. Silent discard (`.ok()`)
is prohibited (RISK-TEST-STRATEGY.md background task coordinator risk).

Background task paths do NOT use `spawn_with_timeout` (ADR-002, C-11). They are
fire-and-forget tasks that must run to completion. Applying `MCP_HANDLER_TIMEOUT` to
a contradiction scan would incorrectly kill a multi-minute scan.

---

## Call-Site Inventory

### Site 1: `services/search.rs` ~line 228

Context: query embedding in `SearchService::search`.

```
// Before
let raw_embedding: Vec<f32> = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, {
    let adapter = Arc::clone(&adapter);
    move || adapter.embed_entry("", &query)
})
.await
.map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
.map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;

// After (Pattern A)
let raw_embedding: Vec<f32> = self.rayon_pool
    .spawn_with_timeout(MCP_HANDLER_TIMEOUT, {
        let adapter = Arc::clone(&adapter);
        move || adapter.embed_entry("", &query)
    })
    .await
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;
```

Note: `self.rayon_pool` refers to the new `rayon_pool: Arc<RayonPool>` field on
`SearchService`.

Note: there is also a `spawn_blocking_with_timeout` call at ~line 455 for the
co-access boost computation (`compute_search_boost`). That call is NOT an ONNX
inference site — it is a DB read (`Store` operation). Per the "sites remaining on
`spawn_blocking`" table, it must NOT be migrated. The implementer must migrate only
the query embedding call at ~line 228.

### Site 2: `services/store_ops.rs` ~line 113

Context: content embedding in `StoreService` (the store / correct path).

```
// Before
spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, {
    let adapter = Arc::clone(&adapter);
    move || adapter.embed_entry(&title, &content)
})
.await
.map_err(...)?
.map_err(...)?;

// After (Pattern A — self.rayon_pool.spawn_with_timeout)
```

### Site 3: `services/store_correct.rs` ~line 50

Context: correction-path embedding in `StoreCorrectService` (or wherever store_correct
lives).

```
// Before
spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, {
    let adapter = Arc::clone(&adapter);
    move || adapter.embed_entry(&title, &content)
})
.await
.map_err(...)?
.map_err(...)?;

// After (Pattern A)
```

### Site 4: `background.rs` ~line 543

Context: the entire `scan_contradictions` closure for the contradiction scan.

```
// Before
let result = spawn_blocking({
    let adapter = Arc::clone(&adapter);
    move || scan_contradictions(&adapter, &all_entries, ...)
})
.await;

// After (Pattern B — no timeout, error!)
let result = rayon_pool
    .spawn({
        let adapter = Arc::clone(&adapter);
        move || scan_contradictions(&adapter, &all_entries, ...)
    })
    .await;
match result {
    Ok(scan_result) => { /* process */ }
    Err(e) => {
        error!(error = %e, "contradiction scan rayon task cancelled");
    }
}
```

The `scan_contradictions` logic itself is unchanged. Only the execution context changes.

### Site 5: `background.rs` ~line 1162

Context: quality-gate embedding loop. The entire loop closure is dispatched as a
single rayon task.

```
// Before
let result = spawn_blocking({
    let adapter = Arc::clone(&adapter);
    move || {
        for entry in &entries_to_embed {
            // ... embed_entry loop
        }
    }
})
.await;

// After (Pattern B — no timeout, error!)
let result = rayon_pool
    .spawn({
        let adapter = Arc::clone(&adapter);
        move || {
            for entry in &entries_to_embed {
                // ... embed_entry loop (unchanged)
            }
        }
    })
    .await;
match result {
    Ok(_) => { /* continue */ }
    Err(e) => {
        error!(error = %e, "quality-gate embedding rayon task cancelled");
    }
}
```

### Site 6: `uds/listener.rs` ~line 1383

Context: warmup embedding. The warmup is triggered when the UDS listener starts.
This is on the MCP handler path (warmup is user-visible startup step).

```
// Before
spawn_blocking({
    let adapter = Arc::clone(&adapter);
    move || adapter.embed_entry("warmup", "warmup")
})
.await
.ok();   // warmup failure is non-fatal

// After (Pattern A — spawn_with_timeout, warmup failure still non-fatal)
let _ = rayon_pool
    .spawn_with_timeout(MCP_HANDLER_TIMEOUT, {
        let adapter = Arc::clone(&adapter);
        move || adapter.embed_entry("warmup", "warmup")
    })
    .await;
// Result is discarded — warmup failure is non-fatal (same as before)
```

Note: the warmup uses `spawn_with_timeout` because it runs during server startup
on a user-visible path, and an indefinitely hung warmup would block the listener
startup. `MCP_HANDLER_TIMEOUT` (30s) is a reasonable bound for warmup.

### Site 7: `services/status.rs` ~line 542

Context: embedding consistency check in `StatusService`. This runs on the MCP
handler path (part of `context_status` response).

```
// Before
spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, {
    let adapter = Arc::clone(&adapter);
    move || adapter.embed_entry(&title, &content)
})
.await
.map_err(...)?
.map_err(...)?;

// After (Pattern A)
self.rayon_pool
    .spawn_with_timeout(MCP_HANDLER_TIMEOUT, {
        let adapter = Arc::clone(&adapter);
        move || adapter.embed_entry(&title, &content)
    })
    .await
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;
```

---

## Sites That Must NOT Be Migrated

Implementers must leave the following `spawn_blocking` calls unchanged:

| File | Location | Description | Why it stays |
|------|----------|-------------|--------------|
| `infra/embed_handle.rs` | ~line 76 | `OnnxProvider::new(config)` | File I/O + ONNX session init — not steady-state inference |
| `background.rs` | ~line 1088 | `run_extraction_rules` | Pure in-memory rule evaluation, no ONNX |
| `background.rs` | ~line 1144 | `persist_shadow_evaluations` | DB write |
| `server.rs`, `gateway.rs`, `usage.rs` | various | Registry reads, audit writes, rate-limit checks | I/O-bound DB or short-duration CPU |
| `uds/listener.rs` (non-warmup) | various | Session lifecycle DB writes, signal dispatch | I/O-bound |
| `services/search.rs` | ~line 455 | `compute_search_boost` (co-access) | DB read, not ONNX inference |

The CI grep step (see `ci_enforcement.md`) will verify that no `spawn_blocking`
remains in `services/` or `background.rs` at inference sites.

---

## Import Changes

At each migrated file, update imports:

Remove (if the file no longer uses `spawn_blocking_with_timeout`):
```
use crate::infra::timeout::{MCP_HANDLER_TIMEOUT, spawn_blocking_with_timeout};
```

Add (if not already present for `MCP_HANDLER_TIMEOUT`):
```
use crate::infra::timeout::MCP_HANDLER_TIMEOUT;
use crate::infra::rayon_pool::RayonPool;  // for type annotation if needed
```

Note: `Arc<RayonPool>` is accessed via `self.rayon_pool` (struct field) or via
a local `rayon_pool` variable (background tasks). Direct imports of `RayonPool`
are only needed if the type appears explicitly in a function signature.

---

## Error Handling Summary

| Call site | Error path | Handling |
|-----------|-----------|---------|
| Sites 1–3, 7 (MCP handlers) | `RayonError::Cancelled` or `TimedOut` | `.map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?` |
| Site 6 (warmup) | `RayonError::*` | Discarded (`let _ = ...`) — warmup is non-fatal |
| Sites 4–5 (background) | `RayonError::Cancelled` | `error!()` tracing event emitted; tick continues |

Background task `Cancelled` must not be silently ignored (no `.ok()` or `let _ =`).
The `error!()` event provides operator visibility into ONNX failures in the background.

---

## Key Test Scenarios (AC-06, AC-07, R-04, R-06)

1. **All 7 sites use rayon, not spawn_blocking** (AC-06): at each of the 7 file/line
   locations, assert absence of `spawn_blocking` / `spawn_blocking_with_timeout` and
   presence of `rayon_pool.spawn` or `rayon_pool.spawn_with_timeout`.

2. **MCP handler sites use `spawn_with_timeout`, not `spawn`** (R-04): grep all 5 MCP
   handler sites (sites 1, 2, 3, 6, 7) and assert they use `spawn_with_timeout`.

3. **Background sites use `spawn`, not `spawn_with_timeout`** (R-04 scenario 2): grep
   sites 4 and 5 and assert they do NOT use `spawn_with_timeout`.

4. **Timeout constant is `MCP_HANDLER_TIMEOUT`, not a literal** (integration risk from
   RISK-TEST-STRATEGY.md): grep all `spawn_with_timeout` calls; assert each passes
   `MCP_HANDLER_TIMEOUT` by name, not a hard-coded `Duration::from_secs(30)`.

5. **Non-inference `spawn_blocking` sites are unchanged** (AC-08): grep
   `infra/embed_handle.rs` and assert exactly one `spawn_blocking` call (the `OnnxProvider::new`
   call); assert no rayon call is present in that file.

6. **`search.rs` co-access boost remains on `spawn_blocking`**: grep `search.rs` for
   `spawn_blocking`; assert it appears for the co-access path but NOT for the embedding path.

7. **Integration smoke test** (AC-10): server starts with rayon pool active; a
   `context_search` request that triggers ONNX inference completes successfully with a
   valid embedding vector.

8. **Background `Cancelled` emits `error!`** (integration risk): in a test that mocks a
   panicking `EmbedAdapter`, assert the background tick emits a tracing `error!` event
   for `Cancelled` and does not abort the tick.
