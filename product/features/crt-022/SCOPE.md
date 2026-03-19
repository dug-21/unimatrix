# crt-022: Rayon Thread Pool + Embedding Migration (W1-2)

## Problem Statement

All CPU-bound ML inference in Unimatrix currently runs through
`tokio::task::spawn_blocking`. That routes ONNX work through the tokio blocking
pool — a pool sized for short-lived I/O-bound work, not sustained CPU inference.
Three documented incidents (#735, #1628, #1688) show blocking-pool saturation
under concurrent inference load. As Wave 1–3 adds NLI (W1-4), GGUF (W2-4), and
GNN (W3-1), each new model adds more CPU pressure to the same shared pool. If the
pattern is not corrected now, saturation compounds with every new model.

The structural fix is a dedicated `rayon::ThreadPool` for all CPU-bound ML
inference, bridged to tokio via `oneshot` channel. Rayon is the right primitive:
work-stealing scheduler, sized for compute, no tokio thread budget impact. The
oneshot bridge keeps the async caller suspended while rayon does the CPU work —
zero tokio threads consumed during inference.

A second problem is a crate boundary violation: `AsyncEmbedService`, which wraps
ONNX embedding inference in `spawn_blocking`, lives in `unimatrix-core`. That
crate is a domain aggregation layer — it hosts `EmbedService` and `VectorStore`
traits and re-exports types for the server's convenience. Execution scheduling
(how many threads, which pool) is a deployment concern, not a domain concern.
Rayon belongs in `unimatrix-server`, not in `unimatrix-core`. The architect
consultation (crt-022a) resolved this definitively: move the rayon bridge and
async embed wrappers to the server; `unimatrix-core` stays lean.

**Who is affected**: Every MCP session that triggers embedding inference (search,
store, correct, contradiction scan, warmup). Every Wave 1–3 feature that adds
ML inference (W1-4 NLI, W2-4 GGUF, W3-1 GNN) depends on this infrastructure
being in place before they arrive.

**Why now**: W1-4 (NLI) and W2-4 (GGUF) both require this pool. Establishing the
pattern once at the lowest-stakes point — embedding migration — validates the
bridge before higher-stakes models depend on it.

---

## Goals

1. Add `rayon = "1"` to `unimatrix-server/Cargo.toml` and initialize a dedicated
   `rayon::ThreadPool` at server startup, sized from config (`[inference]
   rayon_pool_size`), shared as `Arc<rayon::ThreadPool>`.
2. Implement a reusable rayon-tokio oneshot bridge (`rayon_spawn<F,T>`) in
   `unimatrix-server/src/infra/rayon_pool.rs`. A panic in the closure must not
   propagate — the oneshot channel drop is the only failure signal.
3. Move `AsyncEmbedService` out of `unimatrix-core/src/async_wrappers.rs` and
   into `unimatrix-server`, replacing its `spawn_blocking` implementation with
   the rayon bridge.
4. Migrate all ONNX embedding inference call sites in `unimatrix-server` from
   `spawn_blocking` to the rayon pool (search path, store path, correction path,
   contradiction scan, warmup).
5. After migration, `spawn_blocking` must no longer be used for ONNX inference
   anywhere in the codebase. Remaining `spawn_blocking` calls cover only
   I/O-bound or short-duration work: model loading (`OnnxProvider::new`), DB
   registry reads, rule execution, shadow evaluation persistence.
6. Add an `[inference]` config section to `UnimatrixConfig` for pool size and
   future inference parameters. Absent section uses compiled defaults.

---

## Non-Goals

- **NLI model integration** — no `NliProvider`, no `NliServiceHandle`, no NLI
  ONNX session. The NLI model is W1-4 (crt-023 or later). It uses the rayon pool
  established here but is not part of this feature.
- **Bootstrap edge promotion** — the DELETE+INSERT promotion path for
  `bootstrap_only=1` GRAPH_EDGES rows is a W1-4 concern. This feature writes no
  GRAPH_EDGES rows.
- **NLI post-store pipeline** — no fire-and-forget NLI task, no Contradicts/
  Supports edge writes, no circuit breaker. All of that is W1-4.
- **SHA-256 model hash pinning** — the NLI model security requirement (Critical,
  product vision W1-4). The embedding model download path (`download.rs`) also
  lacks hash verification; plugging that gap is W1-4 scope, not here.
- **GNN or GGUF integration** — both are later waves. The rayon pool established
  here is the infrastructure they use; building the pool is all that is in scope.
- **A second rayon pool** — W2-4 (GGUF) gets a separate pool sized for long-
  duration inference. That pool is created in W2-4, not here.
- **New `unimatrix-onnx` crate** — the architect consultation (crt-022a)
  deferred this extraction to before W3-1 ships. Not in scope here.
- **Removing `AsyncVectorStore` from `unimatrix-core`** — HNSW search stays on
  `spawn_blocking` (short-duration, memory-mapped index, not the problem
  workload). `AsyncVectorStore` remains in `unimatrix-core`.
- **Schema changes** — no new tables, no migration.
- **Changes to contradiction scan logic or `conflict_heuristic`** — those are
  internal to the contradiction scan; this feature only changes its execution
  context from `spawn_blocking` to rayon.

---

## Background Research

### spawn_blocking Call Sites for ONNX Inference

The following sites wrap CPU-bound ONNX embedding inference and **must move to
rayon**:

| File | Line(s) | Description |
|------|---------|-------------|
| `unimatrix-core/src/async_wrappers.rs` | 100, 110 | `AsyncEmbedService::embed_entry` and `embed_entries` — generic `spawn_blocking` wrappers. Confirmed unused by the server's direct call sites (see below). |
| `unimatrix-server/src/services/search.rs` | ~228 | Query embedding via `spawn_blocking_with_timeout`. CPU-bound ONNX call. Must move to rayon. |
| `unimatrix-server/src/services/store_ops.rs` | ~113 | Store-path embedding via `spawn_blocking_with_timeout`. CPU-bound ONNX call. Must move to rayon. |
| `unimatrix-server/src/services/store_correct.rs` | ~50 | Correction-path embedding via `spawn_blocking_with_timeout`. CPU-bound ONNX call. Must move to rayon. |
| `unimatrix-server/src/background.rs` | ~543 | Contradiction scan: `spawn_blocking` wrapping `scan_contradictions` which calls `adapter.embed_entry` in a loop. Longest-running ONNX call. Primary saturation risk. Must move to rayon. |
| `unimatrix-server/src/background.rs` | ~1162 | Quality-gate embedding check: `spawn_blocking` wrapping per-entry `adapter.embed_entry` calls. CPU-bound ONNX. Must move to rayon. |
| `unimatrix-server/src/uds/listener.rs` | ~1383 | `warm_embedding_model`: one-shot warmup `spawn_blocking`. Must move to rayon. |
| `unimatrix-server/src/services/status.rs` | ~542 | `check_embedding_consistency` via `spawn_blocking_with_timeout`. CPU-bound ONNX. Must move to rayon. |

The following `spawn_blocking` call sites wrap **I/O-bound or short-duration work
and must NOT move to rayon**:

| File | Description |
|------|-------------|
| `embed_handle.rs:76` | `OnnxProvider::new(config)` — model file I/O + session initialization. Not steady-state inference. Stays on `spawn_blocking`. |
| `background.rs:1088` | `run_extraction_rules` — pure in-memory rule evaluation. Not ONNX. Stays on `spawn_blocking`. |
| `background.rs:1144` | `persist_shadow_evaluations` — DB write. Stays on `spawn_blocking` (or becomes async-native under sqlx). |
| `server.rs`, `gateway.rs`, `usage.rs` | Registry reads, audit writes, rate-limit checks — all I/O-bound DB or short-duration CPU. Stays on `spawn_blocking`. |
| `uds/listener.rs` (various) | Session lifecycle DB writes, signal dispatch. Stays on `spawn_blocking`. |

### AsyncEmbedService Is Not the Server's Primary Embedding Path

`AsyncEmbedService` (in `unimatrix-core/src/async_wrappers.rs`) is defined but
not used by the server's critical call sites. Confirmed by grep: no import of
`AsyncEmbedService` in any `unimatrix-server` source file. The server instead
calls `embed_service.get_adapter().await` (returns `Arc<EmbedAdapter>`) and then
passes the adapter into `spawn_blocking` directly. This means the rayon migration
targets those direct call sites, and `AsyncEmbedService` is removed from
`unimatrix-core` as part of the crate cleanup (it has no consumers).

### AsyncVectorStore Is the Server's HNSW Search Path

`AsyncVectorStore` (also in `unimatrix-core/src/async_wrappers.rs`) is heavily
used across the server (confirmed by grep: ~20 import sites). It wraps HNSW
operations (`insert`, `search`, `search_filtered`, `point_count`, etc.) in
`spawn_blocking`. HNSW search is short-duration memory-mapped index traversal —
not the problem workload — and stays on `spawn_blocking` per Non-Goals. The
`async` feature and tokio dependency in `unimatrix-core` remain for this reason.

### Current Crate Dependencies (no rayon)

Confirmed by grepping all workspace `Cargo.toml` files:
- `rayon` is referenced exactly once, in a comment in
  `unimatrix-engine/Cargo.toml` noting it is excluded from the `petgraph`
  feature set. No crate has a live rayon dependency.
- `unimatrix-server/Cargo.toml` is the correct and only target for `rayon = "1"`.

### ONNX Runtime Version

`ort = "=2.0.0-rc.9"` pinned in `unimatrix-embed/Cargo.toml`.
`ort-sys = "=2.0.0-rc.9"` also pinned. `unimatrix-server` inherits this via its
`unimatrix-embed` dependency. Any future ONNX provider (W1-4 NLI) must use the
same pinned version to avoid runtime conflicts.

### EmbedServiceHandle Architecture

`unimatrix-server/src/infra/embed_handle.rs` implements the established state
machine pattern: `Loading → Ready | Failed → Retrying` with exponential backoff
(10s, 20s, 40s, max 3 retries). `get_adapter()` returns `Arc<EmbedAdapter>`.
The rayon migration does not change this state machine or the lazy-loading
behavior — it changes only what happens after `get_adapter()` succeeds: instead
of passing the adapter into `spawn_blocking`, it passes it into `rayon_spawn`.

### Config Infrastructure (W0-3/dsn-001)

`UnimatrixConfig` in `unimatrix-server/src/infra/config.rs` has sections:
`ProfileConfig`, `KnowledgeConfig`, `ServerConfig`, `AgentsConfig`,
`ConfidenceConfig`. All use `#[serde(default)]`; absent sections use compiled
defaults. `toml = "0.8"` is already present. W1-2 adds an `[inference]` section
following the same pattern — add a struct, add a field to `UnimatrixConfig`,
validate at load, distribute in startup wiring.

### W1-1 Handshake Context

crt-021 shipped `GRAPH_EDGES` with `bootstrap_only` flag, the SR-02 constraint
(NLI-confirmed edge writes must use direct `write_pool`, not the analytics queue),
and the `metadata TEXT DEFAULT NULL` column for W3-1 GNN edge features. All of
this is W1-4 territory. W1-2 (this feature) has no GRAPH_EDGES interaction. The
rayon pool established here is the infrastructure W1-4 needs — that is the full
extent of the W1-1 → W1-2 → W1-4 chain relevant here.

---

## Proposed Approach

### Phase 1: Pool Infrastructure

Add `rayon = "1"` to `unimatrix-server/Cargo.toml`. Create
`unimatrix-server/src/infra/rayon_pool.rs` with:

```rust
pub struct RayonPool {
    inner: Arc<rayon::ThreadPool>,
}

#[derive(Debug, thiserror::Error)]
pub enum RayonError {
    #[error("rayon worker cancelled (panic or pool shutdown)")]
    Cancelled,
}

impl RayonPool {
    pub fn new(num_threads: usize) -> Result<Self, rayon::ThreadPoolBuildError> {
        let inner = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()?;
        Ok(RayonPool { inner: Arc::new(inner) })
    }

    pub async fn spawn<F, T>(&self, f: F) -> Result<T, RayonError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.inner.spawn(move || { tx.send(f()).ok(); });
        rx.await.map_err(|_| RayonError::Cancelled)
    }
}
```

A panic in `f` drops `tx`, which makes `rx.await` return `Err` — no panic
propagation across thread boundaries. This is the documented safety guarantee.

Pool is initialized in `main.rs` startup wiring from `[inference]
rayon_pool_size` config. Default: `(num_cpus::get() / 2).max(2).min(8)`. Passed
to all subsystems that need it as `Arc<RayonPool>`.

### Phase 2: Crate Boundary Cleanup

Remove `AsyncEmbedService` from `unimatrix-core/src/async_wrappers.rs`. It has
no consumers in `unimatrix-server` (all server call sites use `EmbedAdapter`
directly). Its removal simplifies the module; `AsyncVectorStore` remains.

The `unimatrix-core` `async` feature and tokio dependency are retained because
`AsyncVectorStore` still needs them.

### Phase 3: Migrate Embedding Call Sites

For each call site currently using `spawn_blocking` or
`spawn_blocking_with_timeout` for ONNX embedding inference:

- Replace with `rayon_pool.spawn(move || adapter.embed_entry(...)).await`
- Map `RayonError::Cancelled` to the existing `ServiceError::EmbeddingFailed`
  error variant at each call site

The `spawn_blocking_with_timeout` wrapper at embedding call sites already handles
the timeout concern. After migration, the rayon pool is unbounded in duration
(rayon threads are not subject to the tokio blocking pool timeout). The timeout
wrapper should be replaced with a direct `rayon_pool.spawn(...)` call, and
timeout semantics handled at the service level if still needed.

The contradiction scan (`background.rs:543`) is the most complex migration: the
closure calls `scan_contradictions` which iterates over many entries calling
`adapter.embed_entry` in a loop. The entire scan closure moves into a single
`rayon_pool.spawn(...)` call — same as before, but on rayon's pool.

Similarly, the quality-gate embedding block (`background.rs:1162`) is a loop
over entries. The entire loop moves into a single rayon closure.

### Phase 4: Config Extension

Add `[inference]` section to `UnimatrixConfig`:

```toml
[inference]
# Number of threads for the dedicated ML inference rayon pool.
# Default: max(num_cpus / 2, 2), capped at 8.
rayon_pool_size = 4
```

Validation: `rayon_pool_size` must be in `[1, 64]`. Invalid value aborts startup
with a clear error.

The `[inference]` section is named to accommodate W1-4 NLI parameters
(`model_path`, `model_sha256`, thresholds) and W2-4 GGUF parameters under the
same config section hierarchy, without renaming.

---

## Acceptance Criteria

- AC-01: `rayon = "1"` is added to `unimatrix-server/Cargo.toml`. No other crate
  gains a rayon dependency.
- AC-02: `unimatrix-server/src/infra/rayon_pool.rs` exists and exports `RayonPool`
  and `RayonError`. `RayonPool::spawn` is an async method that bridges a closure
  to a tokio oneshot channel.
- AC-03: A panic inside a `RayonPool::spawn` closure results in
  `Err(RayonError::Cancelled)` returned to the awaiting async task. The panic
  does not propagate to the tokio runtime or the MCP handler thread.
- AC-04: The rayon pool is initialized at server startup from `[inference]
  rayon_pool_size` config (default `max(num_cpus / 2, 2)`, max 8). Pool is an
  `Arc<RayonPool>` distributed to all inference consumers.
- AC-05: `AsyncEmbedService` is removed from
  `unimatrix-core/src/async_wrappers.rs`. `AsyncVectorStore` is retained.
  `unimatrix-core` gains no new dependencies.
- AC-06: All ONNX embedding inference call sites in `unimatrix-server` use
  `rayon_pool.spawn(...)` instead of `spawn_blocking` or
  `spawn_blocking_with_timeout`. Specifically: `search.rs` query embedding,
  `store_ops.rs` store-path embedding, `store_correct.rs` correction-path
  embedding, `background.rs` contradiction scan, `background.rs` quality-gate
  embedding, `uds/listener.rs` warmup, `services/status.rs` embedding
  consistency check.
- AC-07: `spawn_blocking` is not used for ONNX inference anywhere in the
  codebase after this feature ships. A code reviewer must be able to grep for
  `spawn_blocking` in the server and find only I/O-bound or short-duration
  non-inference call sites.
- AC-08: `OnnxProvider::new(config)` in `embed_handle.rs` remains on
  `spawn_blocking` (model I/O + session initialization, not steady-state
  inference).
- AC-09: An `[inference]` section is added to `UnimatrixConfig`. It is
  deserialized from `config.toml` with `#[serde(default)]`. Absent section uses
  compiled defaults. `rayon_pool_size` is validated: must be in `[1, 64]`;
  out-of-range value aborts startup with a structured error.
- AC-10: The server starts and handles MCP requests successfully with the rayon
  pool active. Embedding inference continues to function correctly (produce valid
  embeddings) after the migration.
- AC-11: All existing tests pass. At minimum 8 new unit tests covering: rayon
  bridge successful dispatch, rayon bridge panic safety (AC-03), pool
  initialization with various sizes, config validation (valid and invalid
  `rayon_pool_size`), `AsyncEmbedService` removal (compilation test — it must not
  exist).

---

## Constraints

1. **`rayon` is added only to `unimatrix-server`**. `unimatrix-core` stays lean;
   rayon is a deployment scheduling concern, not a domain abstraction.
   Established by architect consultation (crt-022a).
2. **`ort = "2.0.0-rc.9"` is pinned and must not be changed**. The existing
   embedding model depends on this version. W1-4 NLI must use the same version.
3. **`OnnxProvider::new` stays on `spawn_blocking`**. Model loading is I/O +
   initialization, not steady-state CPU inference. This is correct behavior.
4. **`AsyncVectorStore` stays in `unimatrix-core`**. HNSW operations are not the
   problem workload and are not migrated.
5. **Single rayon pool for W1-2**. One shared pool serves all embedding inference.
   W2-4 (GGUF) introduces a separate pool for long-duration inference; that is not
   in scope here. W1-4 (NLI) shares the pool established by this feature.
6. **No schema migration**. This feature adds no tables, no columns, and does not
   touch the database schema.
7. **No `unimatrix-onnx` crate**. Deferred to before W3-1. Documented by
   architect consultation (crt-022a) as a `// TODO(W3-1)` concern.
8. **`[inference]` config section naming**. The section is named `[inference]`
   (not `[nli]` as in the old SCOPE.md) to avoid a rename when W1-4 adds NLI
   parameters under the same section.

---

## Open Questions

**OQ-1 (NAMING): Should the single shared pool be named `ml_inference_pool` or
`onnx_pool` or just `rayon_pool`?**
The pool serves ONNX inference now (W1-2, W1-4 NLI, W3-1 GNN). It will not serve
GGUF (separate pool, W2-4). Naming it `onnx_pool` is more precise but would need
renaming if a non-ONNX CPU-bound task needs it in the future. `rayon_pool` is
generic but accurate — it is the rayon pool. This is a naming convention question
for the spec writer.

**OQ-2 (DESIGN): Timeout semantics after migration**
The current `spawn_blocking_with_timeout` wrappers enforce `MCP_HANDLER_TIMEOUT`
on embedding calls. After migrating to rayon, that timeout is removed (rayon
closures do not have a built-in timeout mechanism analogous to
`tokio::time::timeout`). Should the rayon bridge include optional timeout support
(a secondary tokio timeout around `rx.await`)? Or is the existing timeout
superseded by the fact that rayon work-stealing prevents indefinite queuing?
The architect should decide whether to wrap `rayon_pool.spawn(...).await` with
`tokio::time::timeout` at call sites.

---

## Tracking

https://github.com/dug-21/unimatrix/issues/317
