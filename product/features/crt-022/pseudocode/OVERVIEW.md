# crt-022 Pseudocode Overview

## Purpose

This overview covers the five components of crt-022: rayon thread pool + embedding
migration. It documents component interactions, data flow across boundaries, shared
types introduced or modified, sequencing constraints, and wave grouping rationale.

---

## Components Involved

| Component | File | Nature |
|-----------|------|--------|
| `RayonPool` + `RayonError` | `rayon_pool.md` | New module — the tokio-rayon bridge |
| `InferenceConfig` | `inference_config.md` | Additive change to `config.rs` |
| Call-site migration (7 sites) | `call_site_migration.md` | Replace `spawn_blocking` at 7 locations |
| `AsyncEmbedService` removal | `async_embed_removal.md` | Subtractive change to `unimatrix-core` |
| CI grep enforcement | `ci_enforcement.md` | New CI step — no runtime code changes |

---

## Data Flow Between Components

```
Startup (main.rs)
  │
  │  read config.inference.rayon_pool_size
  │  → InferenceConfig::validate() → OK or ConfigError (abort startup)
  │
  │  RayonPool::new(size, "ml_inference_pool")
  │  → Result<RayonPool, rayon::ThreadPoolBuildError>
  │    on Err → ServerStartupError::InferencePoolInit(err) → process exit
  │    on Ok  → Arc::new(pool) stored on AppState / ServiceLayer
  │
MCP Handler (tokio task)
  │
  │  EmbedServiceHandle::get_adapter().await
  │  → Arc<EmbedAdapter>
  │
  │  rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, move || {
  │      adapter.embed_entry(title, content)    // OnnxProvider::Mutex<Session>.lock()
  │  }).await
  │  → Result<Vec<f32>, RayonError>
  │    RayonError::Cancelled → ServiceError::EmbeddingFailed
  │    RayonError::TimedOut  → ServiceError::EmbeddingFailed
  │    Ok(vec)               → continues into search / store pipeline
  │
Background Tick (tokio task)
  │
  │  rayon_pool.spawn(move || {
  │      scan_contradictions(adapter, entries)  // whole loop, no per-entry tasks
  │  }).await
  │  → Result<_, RayonError>
  │    RayonError::Cancelled → error!() tracing event, tick continues
  │    Ok(_)                 → contradiction results stored
```

The rayon bridge mechanism is identical for all consumers; only the method variant
(`spawn` vs `spawn_with_timeout`) and caller context differ.

---

## Shared Types Introduced or Modified

### New types (in `unimatrix-server/src/infra/rayon_pool.rs`)

```
RayonPool {
    ml_inference_pool: Arc<rayon::ThreadPool>,
}

RayonError {
    Cancelled,           -- closure panic or pool shutdown dropped tx
    TimedOut(Duration),  -- tokio::time::timeout fired on rx.await
}
```

Both variants map to `ServiceError::EmbeddingFailed(msg)` at every call site. The
error message must include the variant details for operator diagnostics.

### Modified types (in `unimatrix-server/src/infra/config.rs`)

```
InferenceConfig {
    rayon_pool_size: usize,   -- serde(default), range [1, 64]
}

UnimatrixConfig {
    // existing fields unchanged
    inference: InferenceConfig,   // new field, #[serde(default)]
}
```

`InferenceConfig` follows the same `#[serde(default)]` pattern as `ProfileConfig`,
`KnowledgeConfig`, `ServerConfig`, `AgentsConfig`, and `ConfidenceConfig`.

The `ConfigError` enum in `config.rs` gains one new variant:

```
ConfigError::InferencePoolSizeOutOfRange {
    path: PathBuf,
    value: usize,
}
```

Its `Display` impl names the field, the bad value, and the valid range `[1, 64]`.

### Modified startup wiring (in `crates/unimatrix-server/src/main.rs`)

`AppState` / `ServiceLayer` gains a field:

```
ml_inference_pool: Arc<RayonPool>,
// TODO(W2-4): add gguf_rayon_pool: Arc<RayonPool> here
```

`ServiceLayer::new` and `ServiceLayer::with_rate_config` gain one additional parameter:
`ml_inference_pool: Arc<RayonPool>`. All consumers of the pool receive it as a clone of
`Arc<RayonPool>` from this field.

### Removed types (from `unimatrix-core/src/async_wrappers.rs`)

```
AsyncEmbedService<T>    -- removed entirely; zero consumers in unimatrix-server
  embed_entry()         -- removed
  embed_entries()       -- removed
  dimension()           -- removed
```

`AsyncVectorStore<T>` and all its methods are retained unchanged.

---

## Sequencing Constraints (Build Order)

Wave 1 — parallel, no inter-component dependencies:
- `rayon_pool.rs`: new file; depends only on `tokio::sync::oneshot`, `rayon`, `thiserror`
- `InferenceConfig` additions to `config.rs`: additive; no dependency on `rayon_pool.rs`
- `AsyncEmbedService` removal from `async_wrappers.rs`: subtractive; no code deps

Wave 2 — depends on Wave 1 completion:
- `main.rs` wiring: constructs `RayonPool` using `InferenceConfig`; places on `AppState`
- `ServiceLayer::new`: gains `ml_inference_pool` parameter from `main.rs`
- All 7 call-site migrations: need `RayonPool` on `AppState`/`ServiceLayer` (from Wave 2 wiring)

Wave 3 — additive, no code dependencies:
- CI grep enforcement step: shell script or xtask; does not require Wave 1 or 2 to merge first,
  but only becomes meaningful once Wave 2 is merged

---

## Wave Grouping Rationale

### Wave 1 is parallelisable because:
- `rayon_pool.rs` is a new file with no dependents yet
- `InferenceConfig` addition is structurally isolated within `config.rs`; it adds a field and
  a `ConfigError` variant that do not touch existing validation paths
- `AsyncEmbedService` deletion compiles cleanly because `cargo check --workspace` confirms zero
  consumers (AC-05 prerequisite); the deletion is independent of all server-side changes

### Wave 2 serialises after Wave 1 because:
- `main.rs` must call `RayonPool::new` (Wave 1 type) and `InferenceConfig::validate` (Wave 1)
- `ServiceLayer::new` must accept `Arc<RayonPool>` (Wave 1 type)
- The 7 call-site migrations must reference `self.rayon_pool` which is only available after
  the `ServiceLayer` field is wired in Wave 2 `main.rs`

### Wave 3 is independent because:
- The CI step is a shell grep; it does not depend on compiled Rust types
- The step will trivially pass on the pre-migration codebase (no `spawn_blocking` in `services/`
  that isn't already there), and must pass on the post-migration codebase

---

## Integration Surface Summary

The architecture defines the following integration surface (traced verbatim from ARCHITECTURE.md):

| Point | Signature | File |
|-------|-----------|------|
| `RayonPool::new` | `fn new(num_threads: usize, name: &str) -> Result<Self, rayon::ThreadPoolBuildError>` | `infra/rayon_pool.rs` (new) |
| `RayonPool::spawn` | `pub async fn spawn<F,T>(&self, f: F) -> Result<T, RayonError>` | `infra/rayon_pool.rs` (new) |
| `RayonPool::spawn_with_timeout` | `pub async fn spawn_with_timeout<F,T>(&self, timeout: Duration, f: F) -> Result<T, RayonError>` | `infra/rayon_pool.rs` (new) |
| `RayonPool::pool_size` | `pub fn pool_size(&self) -> usize` | `infra/rayon_pool.rs` (new) |
| `RayonPool::name` | `pub fn name(&self) -> &str` | `infra/rayon_pool.rs` (new) |
| `RayonError` | `enum { Cancelled, TimedOut(Duration) }` | `infra/rayon_pool.rs` (new) |
| `InferenceConfig` | `struct { rayon_pool_size: usize }` + `Default` + `validate()` | `infra/config.rs` (modified) |
| `UnimatrixConfig::inference` | `pub inference: InferenceConfig` | `infra/config.rs` (modified) |
| `ConfigError::InferencePoolSizeOutOfRange` | new variant | `infra/config.rs` (modified) |
| `EmbedAdapter::embed_entry` | unchanged; called inside rayon closures | `unimatrix-core/src/adapters.rs` |
| `AsyncEmbedService` | REMOVED | `unimatrix-core/src/async_wrappers.rs` |
| `ServiceError::EmbeddingFailed` | unchanged; target for `RayonError` mapping | `services/mod.rs` |
| `MCP_HANDLER_TIMEOUT` | unchanged constant `Duration::from_secs(30)` | `infra/timeout.rs` |
