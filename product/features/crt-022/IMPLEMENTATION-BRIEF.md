# crt-022 Implementation Brief: Rayon Thread Pool + Embedding Migration (W1-2)

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-022/SCOPE.md |
| Scope Risk Assessment | product/features/crt-022/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-022/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-022/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/crt-022/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-022/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `RayonPool` + `RayonError` | pseudocode/rayon_pool.md | test-plan/rayon_pool.md |
| `InferenceConfig` | pseudocode/inference_config.md | test-plan/inference_config.md |
| Call-site migration (7 sites) | pseudocode/call_site_migration.md | test-plan/call_site_migration.md |
| `AsyncEmbedService` removal | pseudocode/async_embed_removal.md | test-plan/async_embed_removal.md |
| CI grep enforcement | pseudocode/ci_enforcement.md | test-plan/ci_enforcement.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Establish a dedicated `rayon::ThreadPool` in `unimatrix-server` for all CPU-bound ML inference, bridged to tokio via oneshot channel, and migrate all 7 ONNX embedding call sites off `spawn_blocking` onto this pool. Simultaneously remove the dead `AsyncEmbedService` wrapper from `unimatrix-core` to enforce the crate boundary rule that execution scheduling is a server-layer concern. This pool (`ml_inference_pool`) is the shared infrastructure on which W1-4 (NLI) and W3-1 (GNN) both depend.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Which crate owns the rayon dependency | `rayon = "1"` added to `unimatrix-server` only; no other crate gains the dep; `AsyncEmbedService` removed from `unimatrix-core` as crate-boundary cleanup | SCOPE.md + crt-022a architect | architecture/ADR-001-rayon-in-server-only.md |
| Timeout semantics after migration | `RayonPool` exposes two methods: `spawn` (no timeout, for background tasks) and `spawn_with_timeout(Duration, f)` (for MCP handler paths); `RayonError` gains `TimedOut(Duration)` variant | ADR-002 / §timeout-semantics in ARCHITECTURE.md | architecture/ADR-002-timeout-at-bridge-not-call-site.md |
| Contradiction scan threading model and pool floor | Scan stays as a single rayon task (not per-entry decomposed); pool floor raised from 2 to 4: `(num_cpus / 2).max(4).min(8)` | ADR-003 / §pool-sizing in ARCHITECTURE.md | architecture/ADR-003-scan-single-task-pool-floor-four.md |
| Pool distribution pattern | `Arc<RayonPool>` placed on `AppState` / `ServiceLayer`; initialised once in `main.rs`; all consumers receive it via shared state, never constructed ad-hoc | ADR-004 | architecture/ADR-004-appstate-pool-distribution.md |
| Pool naming (OQ-1) | Named `ml_inference_pool` (human-approved); generic enough for NLI + GNN, without ONNX specificity | SPECIFICATION.md §Domain Models | — (human-resolved) |
| `OnnxProvider` thread safety (SR-01) | Confirmed `Send + Sync` via `test_send_sync` in `onnx.rs`; `Mutex<Session>` serialises concurrent callers; no synchronisation change needed | ARCHITECTURE.md §thread-safety | — (resolved in architecture) |

---

## Files to Create / Modify

### New Files

| Path | Summary |
|------|---------|
| `crates/unimatrix-server/src/infra/rayon_pool.rs` | `RayonPool` struct wrapping `Arc<rayon::ThreadPool>`, two async methods (`spawn` and `spawn_with_timeout`), `RayonError` enum with `Cancelled` and `TimedOut(Duration)` variants, module-level rustdoc documenting the MCP vs background convention |

### Modified Files

| Path | Change Summary |
|------|---------------|
| `crates/unimatrix-server/Cargo.toml` | Add `rayon = "1"` dependency |
| `crates/unimatrix-server/src/infra/config.rs` | Add `InferenceConfig` struct (`rayon_pool_size: usize`, `#[serde(default)]`, `validate()` method checking `[1, 64]`); add `inference: InferenceConfig` field to `UnimatrixConfig` |
| `crates/unimatrix-server/src/infra/mod.rs` | Re-export `RayonPool` and `RayonError` from the `infra` module |
| `crates/unimatrix-server/src/main.rs` (or startup wiring) | Initialise `RayonPool::new(config.inference.rayon_pool_size, "ml_inference_pool")`; wrap in `Arc`; place on `AppState`; propagate `ThreadPoolBuildError` as structured `ServerStartupError::InferencePoolInit`; add `// TODO(W2-4): add gguf_rayon_pool: Arc<RayonPool> here` comment |
| `crates/unimatrix-server/src/services/search.rs` | Migrate query embedding (~line 228) from `spawn_blocking_with_timeout` to `ml_inference_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| `crates/unimatrix-server/src/services/store_ops.rs` | Migrate store-path embedding (~line 113) from `spawn_blocking_with_timeout` to `ml_inference_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| `crates/unimatrix-server/src/services/store_correct.rs` | Migrate correction-path embedding (~line 50) from `spawn_blocking_with_timeout` to `ml_inference_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| `crates/unimatrix-server/src/services/status.rs` | Migrate embedding consistency check (~line 542) from `spawn_blocking_with_timeout` to `ml_inference_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| `crates/unimatrix-server/src/background.rs` | Migrate contradiction scan (~line 543) from `spawn_blocking` to `ml_inference_pool.spawn(...)` (no timeout — background task); migrate quality-gate embedding loop (~line 1162) from `spawn_blocking` to `ml_inference_pool.spawn(...)`; ensure `Cancelled` emits `error!` tracing event |
| `crates/unimatrix-server/src/uds/listener.rs` | Migrate warmup embedding (~line 1383) from `spawn_blocking` to `ml_inference_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| `crates/unimatrix-core/src/async_wrappers.rs` | Remove `AsyncEmbedService` struct and its `embed_entry` / `embed_entries` methods; retain `AsyncVectorStore` and all its methods unchanged |
| CI pipeline (e.g., `.github/workflows/` or `xtask`) | Add grep-based step enforcing AC-07: `spawn_blocking` must not appear in `crates/unimatrix-server/src/services/` or `crates/unimatrix-server/src/background.rs` at inference call sites; step runs on every PR against main |

---

## Data Structures

### `RayonPool`

```rust
pub struct RayonPool {
    ml_inference_pool: Arc<rayon::ThreadPool>,
}
```

Constructed via `RayonPool::new(num_threads: usize, name: &str) -> Result<Self, rayon::ThreadPoolBuildError>`. Held as `Arc<RayonPool>` on `AppState`.

### `RayonError`

```rust
#[derive(Debug, thiserror::Error)]
pub enum RayonError {
    #[error("rayon worker cancelled (panic or pool shutdown)")]
    Cancelled,
    #[error("rayon inference timed out after {0:?}")]
    TimedOut(std::time::Duration),
}
```

Both variants map to `ServiceError::EmbeddingFailed` at call sites.

### `InferenceConfig`

```rust
#[derive(Debug, serde::Deserialize)]
#[serde(default)]
pub struct InferenceConfig {
    pub rayon_pool_size: usize,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            rayon_pool_size: (num_cpus::get() / 2).max(4).min(8),
        }
    }
}
```

`validate()` returns a structured `ConfigError` when `rayon_pool_size` is outside `[1, 64]`.

Added to `UnimatrixConfig`:

```rust
pub inference: InferenceConfig,
```

---

## Function Signatures

```rust
// rayon_pool.rs — new file
impl RayonPool {
    pub fn new(num_threads: usize, name: &str) -> Result<Self, rayon::ThreadPoolBuildError>;
    pub async fn spawn<F, T>(&self, f: F) -> Result<T, RayonError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;
    pub async fn spawn_with_timeout<F, T>(&self, timeout: Duration, f: F) -> Result<T, RayonError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;
    pub fn pool_size(&self) -> usize;
    pub fn name(&self) -> &str;
}

// config.rs — added
impl InferenceConfig {
    pub fn validate(&self) -> Result<(), ConfigError>;
}
```

### Call-Site Migration Pattern

Before (at each of the 7 MCP handler sites):
```rust
spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, { let adapter = ...; move || adapter.embed_entry(...) })
    .await
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
```

After:
```rust
self.rayon_pool
    .spawn_with_timeout(MCP_HANDLER_TIMEOUT, { let adapter = ...; move || adapter.embed_entry(...) })
    .await
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
```

Background paths (contradiction scan, quality-gate loop) use `spawn(...)` with no timeout.

---

## Call Site Inventory

### Sites migrating to rayon

| # | File | Current primitive | Migration target |
|---|------|-------------------|-----------------|
| 1 | `services/search.rs` ~228 | `spawn_blocking_with_timeout` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| 2 | `services/store_ops.rs` ~113 | `spawn_blocking_with_timeout` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| 3 | `services/store_correct.rs` ~50 | `spawn_blocking_with_timeout` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| 4 | `background.rs` ~543 | `spawn_blocking` | `spawn(...)` — whole scan closure, no timeout |
| 5 | `background.rs` ~1162 | `spawn_blocking` | `spawn(...)` — whole loop closure, no timeout |
| 6 | `uds/listener.rs` ~1383 | `spawn_blocking` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| 7 | `services/status.rs` ~542 | `spawn_blocking_with_timeout` | `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` |
| 8 | `unimatrix-core/async_wrappers.rs` lines 100, 110 | `spawn_blocking` in `AsyncEmbedService` | Remove (dead code) |

### Sites remaining on `spawn_blocking` (must NOT move)

| File | Description |
|------|-------------|
| `infra/embed_handle.rs` ~76 | `OnnxProvider::new(config)` — model file I/O + ONNX session init |
| `background.rs` ~1088 | `run_extraction_rules` — pure in-memory rule evaluation |
| `background.rs` ~1144 | `persist_shadow_evaluations` — DB write |
| `server.rs`, `gateway.rs`, `usage.rs` | Registry reads, audit writes, rate-limit checks |
| `uds/listener.rs` (non-warmup paths) | Session lifecycle DB writes, signal dispatch |

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | `rayon` added only to `unimatrix-server/Cargo.toml`; no other workspace crate gains it |
| C-02 | `ort = "=2.0.0-rc.9"` pinned in `unimatrix-embed/Cargo.toml` — must not change |
| C-03 | `OnnxProvider::new` remains on `spawn_blocking`; it is I/O-bound initialisation, not steady-state inference |
| C-04 | `AsyncVectorStore` remains in `unimatrix-core`; HNSW stays on `spawn_blocking` |
| C-05 | Exactly one rayon pool for W1-2; second GGUF pool is W2-4 scope |
| C-06 | No schema changes — no new tables, columns, or migrations |
| C-07 | No `unimatrix-onnx` crate; deferred to before W3-1; a `// TODO(W3-1)` comment at the `OnnxProvider` import site is the only artefact |
| C-08 | Config section named `[inference]` (not `[nli]` or `[rayon]`) to accommodate W1-4 and W2-4 without renaming |
| C-09 | AC-07 enforced by a CI grep step on every PR against main, not a post-ship manual audit |
| C-10 | `Arc<RayonPool>` placed on `AppState` / `ServiceLayer`; single construction site in `main.rs` |
| C-11 | MCP handler paths use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`; background tasks use `spawn(...)` with no timeout; convention documented in `RayonPool` module-level rustdoc |

---

## Dependencies

### New

| Crate | Version | Added to | Justification |
|-------|---------|----------|---------------|
| `rayon` | `"1"` | `unimatrix-server/Cargo.toml` | ML inference thread pool |
| `num_cpus` | verify or add | `unimatrix-server/Cargo.toml` | Default pool size formula |

### Existing (relied upon, must not change)

| Crate | Version | Role |
|-------|---------|------|
| `tokio` | existing | `oneshot` channel for rayon bridge |
| `ort` | `"=2.0.0-rc.9"` | ONNX runtime; pinned |
| `toml` | `"0.8"` | `InferenceConfig` deserialisation |
| `thiserror` | existing | `RayonError` derive |

### Existing Components Consumed

| Component | Location | How used |
|-----------|----------|----------|
| `EmbedServiceHandle` | `infra/embed_handle.rs` | State machine providing `get_adapter()`; unchanged |
| `EmbedAdapter` | `unimatrix-embed` | `Send + 'static` type passed into rayon closures |
| `ServiceLayer` / `AppState` | `unimatrix-server` | Container for `Arc<RayonPool>` |
| `UnimatrixConfig` | `infra/config.rs` | Extended with `inference: InferenceConfig` |
| `ServiceError::EmbeddingFailed` | server service layer | Target for `RayonError` mapping at all call sites |
| `AsyncVectorStore` | `unimatrix-core/async_wrappers.rs` | Retained; not touched |
| `MCP_HANDLER_TIMEOUT` | server infra | Duration passed to `spawn_with_timeout` at all 7 MCP handler sites |

---

## NOT in Scope

- NLI model integration (`NliProvider`, `NliServiceHandle`, NLI ONNX session) — W1-4
- Bootstrap edge promotion (DELETE+INSERT for `bootstrap_only=1` GRAPH_EDGES rows) — W1-4
- NLI post-store pipeline (fire-and-forget NLI task, Contradicts/Supports edge writes, circuit breaker) — W1-4
- SHA-256 model hash pinning for any model — W1-4
- Second rayon pool for GGUF long-duration inference — W2-4
- `unimatrix-onnx` crate extraction — deferred to before W3-1
- `AsyncVectorStore` removal or migration — explicitly retained
- Schema changes — no new tables, columns, or migrations
- Contradiction scan logic changes — only the execution context changes
- Any W1-3, W1-4, W1-5, W2-x, or W3-x feature content

---

## Alignment Status

**Overall: PASS with 2 WARNs. No FAILs. No blocking issues for implementation.**

| Variance | Severity | Status | Resolution |
|----------|----------|--------|-----------|
| WARN 1: `RayonError::TimedOut` and `spawn_with_timeout` not in original SCOPE.md | WARN | Accepted | These are the direct, necessary resolution of SCOPE.md's own OQ-2, which was explicitly flagged as load-bearing. ADR-002 documents the rationale. The spawn prompt confirms human acceptance. Implementation proceeds with the two-method API. |
| WARN 2: SPECIFICATION.md FR-06 and NFR-04 carry old pool floor formula `max(2)` conflicting with ARCHITECTURE.md `max(4)` | WARN | Resolved by spec update note | ARCHITECTURE.md and ADR-003 are authoritative. The floor is **4**, not 2. Implementers must use `(num_cpus / 2).max(4).min(8)` as the default. The SPECIFICATION.md document carries an internal copy-paste regression; this brief reflects the correct resolved value. |

Source: `product/features/crt-022/ALIGNMENT-REPORT.md`

---

## Minimum New Tests (AC-11)

At minimum 8 unit tests required, covering:

1. `RayonPool::spawn` successful dispatch — closure executes and return value is received
2. `RayonPool::spawn` panic safety — closure containing `panic!` returns `Err(RayonError::Cancelled)`; test runtime does not abort
3. Pool initialisation with `num_threads = 1`
4. Pool initialisation with `num_threads = 8`
5. `InferenceConfig::validate()` — valid lower bound (`rayon_pool_size = 1`)
6. `InferenceConfig::validate()` — valid upper bound (`rayon_pool_size = 64`)
7. `InferenceConfig::validate()` — rejects 0 with structured error
8. `InferenceConfig::validate()` — rejects 65 with structured error

Additional required scenarios (from RISK-TEST-STRATEGY.md, covering Critical/High risks):

- R-02: `spawn_with_timeout` returns `Err(RayonError::TimedOut)` when closure exceeds timeout; pool remains functional after timeout
- R-04: CI grep step confirms all 7 MCP sites use `spawn_with_timeout`, not `spawn`; background paths use `spawn`, not `spawn_with_timeout`
- R-05: `cargo check --workspace` exits 0 after `AsyncEmbedService` removal; `grep -r "AsyncEmbedService" crates/` returns zero results
- R-06: CI grep step passes — no `spawn_blocking` in `services/` or `background.rs` at inference sites
- R-07: Server integration test — startup fails with structured error when `rayon_pool_size = 0`
- R-08: Pool of 4 threads does not deadlock when all 4 are occupied; 5th submission enqueues and completes when a thread frees
