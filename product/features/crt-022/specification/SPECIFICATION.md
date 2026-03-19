# SPECIFICATION: crt-022 — Rayon Thread Pool + Embedding Migration (W1-2)

**Feature ID**: crt-022
**Phase**: Cortical
**Upstream scope**: `product/features/crt-022/SCOPE.md`
**Architect consultation**: `product/features/crt-022/agents/crt-022a-architect-consult.md`

---

## Objective

Establish a dedicated `rayon::ThreadPool` in `unimatrix-server` for all CPU-bound ML
inference, bridged to tokio via oneshot channel. Migrate all ONNX embedding inference
call sites in `unimatrix-server` off `spawn_blocking` and onto this pool as the first
consumer and validation of the pattern. Remove the dead `AsyncEmbedService` wrapper
from `unimatrix-core` to enforce the crate boundary established by ADR-001.

This pool is the shared infrastructure that W1-4 (NLI), W2-4 (GGUF uses a separate
pool), and W3-1 (GNN) all depend on. Establishing it here — at the lowest-stakes
migration point — validates the bridge before higher-stakes models depend on it.

---

## Functional Requirements

**FR-01**: The system shall provide a `RayonPool` struct in
`unimatrix-server/src/infra/rayon_pool.rs` that wraps `Arc<rayon::ThreadPool>` and
exposes an async `spawn` method. The method shall accept an `FnOnce() -> T + Send +
'static` closure, dispatch it to a rayon worker thread, and return
`Result<T, RayonError>` to the awaiting tokio task via a `tokio::sync::oneshot`
channel.

**FR-02**: The system shall guarantee that a panic inside a `RayonPool::spawn` closure
does not propagate to the tokio runtime or the calling async task. When the closure
panics, `tx` is dropped, `rx.await` returns `Err(RecvError)`, and the bridge maps
this to `Err(RayonError::Cancelled)`. No `std::panic::catch_unwind` is required; the
channel drop is the complete panic containment mechanism.

**FR-03**: The server shall initialise exactly one `Arc<RayonPool>` (named
`ml_inference_pool`) at startup from `[inference] rayon_pool_size` config. This pool
shall be distributed to all inference consumers through the server's `ServiceLayer` /
`AppState`. No subsystem shall construct a second rayon pool for ML inference.

**FR-04**: If `rayon::ThreadPoolBuilder` fails to construct the pool at startup, the
error shall be propagated through the server's structured startup error chain and
abort startup with a clear diagnostic message. Silent fallback to `spawn_blocking` is
not permitted.

**FR-05**: The `[inference]` config section shall be added to `UnimatrixConfig` using
`#[serde(default)]`. An absent `[inference]` section shall use compiled defaults. The
single field in scope for W1-2 is `rayon_pool_size: usize`. Validation: value must be
in `[1, 64]`; an out-of-range value shall abort startup with a structured error naming
the offending field and its acceptable range.

**FR-06**: The default value for `rayon_pool_size` when absent from config shall be
`(num_cpus::get() / 2).max(4).min(8)`. On single-core machines `num_cpus::get()` is
1; integer division yields 0; `.max(4)` ensures the pool is never smaller than 4
threads, leaving at least 3 threads available while the contradiction scan occupies one
(SR-04 monopolisation analysis, ADR-003).

**FR-07**: The `AsyncEmbedService` struct and its `embed_entry` and `embed_entries`
methods shall be removed from `unimatrix-core/src/async_wrappers.rs`. The module
shall retain `AsyncVectorStore` and all its associated methods unchanged.
`unimatrix-core` shall gain no new dependencies as a result of this feature.

**FR-08**: All seven ONNX embedding inference call sites in `unimatrix-server` that
currently use `spawn_blocking` or `spawn_blocking_with_timeout` shall be migrated to
`ml_inference_pool.spawn(move || ...).await`. The sites are enumerated in §Call Site
Inventory. At each migrated site, `RayonError::Cancelled` shall be mapped to the
existing `ServiceError::EmbeddingFailed` (or the nearest equivalent error variant in
that service's error type) — no new error variants are introduced.

**FR-09**: The `OnnxProvider::new(config)` call in
`unimatrix-server/src/infra/embed_handle.rs` (line ~76) shall remain on
`spawn_blocking`. This is model file I/O and ONNX session initialisation — not
steady-state CPU inference — and is correctly placed on the tokio blocking pool.

**FR-10**: After the migration, a grep for `spawn_blocking` in all `unimatrix-server`
source files shall return only the non-inference sites enumerated in §Call Site
Inventory. The embedding inference sites shall not appear in grep results. This
invariant shall be enforced by a CI step (grep-based) rather than a post-ship manual
audit. See AC-07 and the CI enforcement note in §Constraints.

**FR-11**: The `[inference]` section naming shall be designed to accommodate future
W1-4 NLI parameters (`model_path`, `model_sha256`, thresholds) and W2-4 GGUF
parameters under the same section hierarchy without renaming.

---

## Non-Functional Requirements

**NFR-01 (Performance)**: After migration, tokio's blocking thread pool shall not be
consumed during ONNX embedding inference. Zero tokio blocking threads shall be held
during the duration of a `ml_inference_pool.spawn(...)` call. The calling tokio task
suspends on `rx.await`; no tokio thread is blocked.

**NFR-02 (Performance)**: Embedding inference throughput shall not regress relative
to the `spawn_blocking` baseline. The rayon work-stealing scheduler is expected to
perform equivalently or better under concurrent load because it is sized for compute
rather than I/O-bound work. No specific latency target is set for W1-2; regression
will be detectable through the W1-3 eval harness.

**NFR-03 (Reliability)**: The pool initialisation failure path (SR-02) shall produce
a structured `ServerStartupError::InferencePoolInit(rayon::ThreadPoolBuildError)`
(or equivalent) rather than an unwrapped panic. The startup abort message shall
include the configured `rayon_pool_size` value.

**NFR-04 (Resource)**: The `[inference] rayon_pool_size` default formula caps the
pool at 8 threads to prevent rayon from consuming all available CPU on heavily
scheduled deployments. The floor of 4 (raised from 2 per ADR-003, SR-04
monopolisation analysis) ensures that when the contradiction scan occupies one rayon
thread, at least 3 threads remain available for concurrent MCP inference calls. The
default formula of `(num_cpus / 2).max(4).min(8)` is the specified baseline;
operators can raise it via config up to 64.

**NFR-05 (Compatibility)**: `ort = "=2.0.0-rc.9"` pinned in
`unimatrix-embed/Cargo.toml` must not be changed. `OrtSession` / `EmbedAdapter`
thread-safety across rayon worker threads must be verified by the architect before
implementation begins (SR-01). The `Send + 'static` bound on the `RayonPool::spawn`
closure signature is the compile-time enforcement of this requirement.

**NFR-06 (Maintainability)**: The `// TODO(W3-1): extract to unimatrix-onnx` comment
pattern (from architect consultation) shall be placed at the `OnnxProvider` import
site in `embed_handle.rs` to signal the future extraction work. No other
documentation artefacts are required for this deferred decision.

**NFR-07 (Build)**: `cargo check --workspace` must pass with zero errors after
`AsyncEmbedService` removal. Any transitive consumer in a test binary or integration
test harness must be identified and updated as part of this feature.

---

## Acceptance Criteria

**AC-01** (from SCOPE.md): `rayon = "1"` is present in `unimatrix-server/Cargo.toml`.
No other workspace crate (`unimatrix-core`, `unimatrix-embed`, `unimatrix-vector`,
`unimatrix-store`, `unimatrix-engine`) gains a rayon dependency.
*Verification*: `cargo tree -p unimatrix-core | grep rayon` returns empty;
`cargo tree -p unimatrix-server | grep rayon` returns the entry.

**AC-02** (from SCOPE.md): `unimatrix-server/src/infra/rayon_pool.rs` exists and
exports `RayonPool` and `RayonError`. `RayonPool` has a field named
`ml_inference_pool` of type `Arc<rayon::ThreadPool>` (or the `RayonPool` struct
itself is the named item; see §Domain Models). `RayonPool::spawn` is an async
method accepting `FnOnce() -> T + Send + 'static` and returning
`Result<T, RayonError>`.
*Verification*: File exists; public API compiles; unit test dispatches a closure and
receives the return value.

**AC-03** (from SCOPE.md): A panic inside a `RayonPool::spawn` closure returns
`Err(RayonError::Cancelled)` to the awaiting tokio task. The panic does not propagate
to the tokio runtime or the MCP handler thread. The test runtime does not abort.
*Verification*: Unit test — spawn a closure containing `panic!("test")`, await
the result, assert `Err(RayonError::Cancelled)`.

**AC-04** (from SCOPE.md): The rayon pool is initialised at server startup. Its size
is read from `[inference] rayon_pool_size` in config (default
`(num_cpus::get() / 2).max(4).min(8)`, max 8). The pool is wrapped in `Arc<RayonPool>`
and is reachable from all inference consumers via `ServiceLayer`.
*Verification*: Integration smoke test — server starts with a minimal config;
a search request that triggers embedding inference completes successfully; pool is
confirmed single-instance via `Arc::strong_count` or structural inspection.

**AC-05** (from SCOPE.md): `AsyncEmbedService` does not exist anywhere in
`unimatrix-core/src/async_wrappers.rs` or any file reachable from
`unimatrix-core/src/lib.rs`. `AsyncVectorStore` is retained and unchanged.
`unimatrix-core` gains no new dependencies. `cargo check --workspace` passes.
*Verification*: `grep -r "AsyncEmbedService" crates/` returns zero results.
`cargo check --workspace` exits 0.

**AC-06** (from SCOPE.md): All ONNX embedding inference call sites in
`unimatrix-server` use `ml_inference_pool.spawn(...)` (or equivalent
`RayonPool::spawn` call through a field named `ml_inference_pool`). The seven sites
are: `search.rs` query embedding, `store_ops.rs` store-path embedding,
`store_correct.rs` correction-path embedding, `background.rs` contradiction scan,
`background.rs` quality-gate embedding loop, `uds/listener.rs` warmup,
`services/status.rs` embedding consistency check.
*Verification*: At each of the seven file locations, the old `spawn_blocking` /
`spawn_blocking_with_timeout` call is absent and a `ml_inference_pool.spawn`
(or `rayon_pool.spawn`) call is present.

**AC-07** (from SCOPE.md): `spawn_blocking` is not used for ONNX inference anywhere
in the codebase after this feature ships. A CI grep step (not a post-ship manual
audit) enforces this. The grep pattern targets `spawn_blocking` in
`unimatrix-server/src/services/` and `unimatrix-server/src/background.rs` and
cross-references against the permitted non-inference list in §Call Site Inventory.
*Verification*: The CI step passes on the merged branch. See §Constraints for the
CI enforcement requirement.

**AC-08** (from SCOPE.md): `OnnxProvider::new(config)` in
`unimatrix-server/src/infra/embed_handle.rs` remains on `spawn_blocking`.
*Verification*: `grep -n "spawn_blocking" crates/unimatrix-server/src/infra/embed_handle.rs`
returns the `OnnxProvider::new` call and no other call.

**AC-09** (from SCOPE.md): An `InferenceConfig` struct (or equivalent named
`InferenceConfig`) with field `rayon_pool_size: usize` exists in
`unimatrix-server/src/infra/config.rs` and is included as a field in
`UnimatrixConfig` as `inference: InferenceConfig`. The struct derives `serde::Deserialize`
with `#[serde(default)]`. The `rayon_pool_size` field validates `[1, 64]` at server
startup; an out-of-range value aborts startup with a structured error.
*Verification*: Unit test for `InferenceConfig::validate()` covering values 0, 1, 8,
64, 65. Integration test confirming server refuses to start with `rayon_pool_size = 0`.

**AC-10** (from SCOPE.md): The server starts and handles MCP requests successfully
with the rayon pool active. Embedding inference produces valid embeddings (same
dimensionality and L2-normalised output as before migration) after the migration.
*Verification*: Existing integration test suite passes without modification.
Embedding output for a known input produces the same vector (within f32 tolerance)
before and after migration.

**AC-11** (from SCOPE.md): At minimum 8 new unit tests cover: (1) `RayonPool::spawn`
successful dispatch, (2) `RayonPool::spawn` panic safety (AC-03), (3) pool
initialisation with `num_threads = 1`, (4) pool initialisation with `num_threads = 8`,
(5) `InferenceConfig` validation — valid lower bound (`rayon_pool_size = 1`),
(6) `InferenceConfig` validation — valid upper bound (`rayon_pool_size = 64`),
(7) `InferenceConfig` validation — rejects 0, (8) `InferenceConfig` validation —
rejects 65. Compilation failure for `AsyncEmbedService` import is verified by
`AC-05`'s `grep` check (compilation is the enforcement; no explicit test needed beyond
`cargo check --workspace`).
*Verification*: `cargo test -p unimatrix-server` passes including all 8 new tests.

---

## Domain Models

### Key Entities

**RayonPool**
The dedicated ML inference thread pool. Wraps `Arc<rayon::ThreadPool>` from the
`rayon` crate. Created once at server startup; held as `Arc<RayonPool>` distributed
through `ServiceLayer`. The field name used for the pool in all contexts (struct
fields, variable names, config) is `ml_inference_pool`. This name was approved by
the human after researcher OQ-1 evaluation.

```
RayonPool {
    ml_inference_pool: Arc<rayon::ThreadPool>
}
```

**RayonError**
The error type returned by `RayonPool::spawn` and `RayonPool::spawn_with_timeout`
when the rayon closure panics, times out, or the pool shuts down before the closure
completes. Has two variants:

- `Cancelled` — the oneshot receiver was dropped (closure panic or pool shutdown).
  Mapped to `ServiceError::EmbeddingFailed` at each call site.
- `TimedOut(Duration)` — the timeout supplied to `spawn_with_timeout` elapsed before
  the closure result was received. Mapped to `ServiceError::EmbeddingFailed` (same
  target variant) at MCP handler call sites.

`RayonPool` exposes two methods:
- `spawn(f)` — background dispatch with no timeout (used for contradiction scan and
  quality-gate loop, where indefinite occupancy of one rayon thread is acceptable).
- `spawn_with_timeout(duration, f)` — wraps `rx.await` with `tokio::time::timeout`;
  used at MCP handler call sites (search, store, correct, status) to enforce
  `MCP_HANDLER_TIMEOUT` without a per-call-site `tokio::time::timeout` wrapper.

This design resolves OQ-2 (ADR-003). The architect chose option (b): a dedicated
`spawn_with_timeout` variant on `RayonPool`.

**InferenceConfig**
The `[inference]` section of `UnimatrixConfig`. Holds `rayon_pool_size: usize`
for W1-2. Will gain NLI and GGUF fields in W1-4 and W2-4 respectively. Follows the
`#[serde(default)]` pattern established by all other config section structs.

**EmbedAdapter**
Existing type from `unimatrix-embed`. Returned by `EmbedServiceHandle::get_adapter()`
as `Arc<EmbedAdapter>`. The rayon migration does not change this type; it changes
where the adapter is called from (rayon worker thread instead of a tokio blocking
thread). Must be `Send + 'static` to satisfy the `RayonPool::spawn` closure bound.
The architect (SR-01) must confirm this before implementation.

**AsyncVectorStore** (retained, unchanged)
Lives in `unimatrix-core/src/async_wrappers.rs`. Wraps HNSW operations
(`insert`, `search`, `search_filtered`, `point_count`) in `spawn_blocking`.
HNSW search is short-duration memory-mapped index traversal — not the problem
workload. Not migrated. The `async` feature flag and tokio dependency in
`unimatrix-core` are retained because `AsyncVectorStore` requires them.

**AsyncEmbedService** (removed)
Was in `unimatrix-core/src/async_wrappers.rs`. Wrapped `EmbedService::embed_entry`
and `EmbedService::embed_entries` in `spawn_blocking`. Had zero consumers in
`unimatrix-server` (server used `EmbedAdapter` directly). Removed as dead code and
a crate boundary violation (scheduler concern in a domain crate).

### Ubiquitous Language

| Term | Meaning in this feature |
|------|------------------------|
| rayon bridge | The `RayonPool::spawn` async method that dispatches work to a rayon thread and returns the result to the calling tokio task via oneshot channel |
| ML inference pool | The single dedicated `RayonPool` for all ONNX CPU-bound inference; field/variable name: `ml_inference_pool` |
| inference consumer | Any server subsystem that calls `ml_inference_pool.spawn(...)` to run embedding inference |
| call site migration | Replacing a `spawn_blocking` / `spawn_blocking_with_timeout` ONNX embedding call with `ml_inference_pool.spawn(...)` |
| dead code removal | The `AsyncEmbedService` deletion — not a migration, since it has no active consumers |
| steady-state inference | CPU-bound ONNX session execution: tokenise input, run forward pass, return tensor. Runs on rayon. |
| model loading | `OnnxProvider::new`: file I/O + ONNX session creation. Not steady-state inference. Stays on `spawn_blocking`. |
| panic containment | The property that a panic inside a rayon closure drops `tx`, causing `rx.await` to return `Err`, without unwinding into the tokio runtime |

---

## User Workflows

### Workflow 1: MCP search request triggers embedding inference

1. MCP client sends `context_search` with a natural-language query.
2. Server's search handler calls `embed_service_handle.get_adapter().await` to
   retrieve `Arc<EmbedAdapter>`.
3. Handler calls `ml_inference_pool.spawn(move || adapter.embed_entry(...)).await`.
4. Rayon worker thread executes the ONNX forward pass (10–50ms CPU).
5. Result is sent on `tx`; the awaiting `rx.await` completes in the search handler.
6. Search proceeds with the embedding vector. The tokio thread was not blocked
   during step 4.

### Workflow 2: Background contradiction scan

1. Background tick triggers contradiction scan (`background.rs:~543`).
2. The entire `scan_contradictions` closure (which iterates entries calling
   `adapter.embed_entry` in a loop) is dispatched as a single
   `ml_inference_pool.spawn(...)` call.
3. One rayon thread is occupied for the full scan duration (longest-running inference
   workload; see SR-04 in §Constraints).
4. Other MCP inference calls (search, store) share the remaining pool threads during
   the scan.
5. On completion, the result is returned via the oneshot channel to the background
   task coordinator.

### Workflow 3: Server startup with [inference] config

1. `main.rs` reads `UnimatrixConfig` from `config.toml`.
2. `InferenceConfig::validate()` checks `rayon_pool_size` in `[1, 64]`.
3. `RayonPool::new(config.inference.rayon_pool_size)` builds the rayon pool.
4. On `ThreadPoolBuildError`, server aborts with structured startup error.
5. On success, `Arc::new(rayon_pool)` is placed into `ServiceLayer` as
   `ml_inference_pool`.
6. All inference consumers receive a clone of `Arc<RayonPool>` at construction time.

### Workflow 4: Model loading (unchanged — stays on spawn_blocking)

1. `EmbedServiceHandle` initialises in `Loading` state on server startup.
2. `OnnxProvider::new(config)` runs inside `spawn_blocking` — file I/O + ONNX
   session creation.
3. On success, handle transitions to `Ready`; `get_adapter()` becomes available.
4. This workflow is not changed by this feature.

---

## Call Site Inventory

### Sites migrating to rayon (7 server call sites + 1 dead code removal)

| # | File | Location | Current primitive | Migration |
|---|------|----------|-------------------|-----------|
| 1 | `services/search.rs` | ~line 228 | `spawn_blocking_with_timeout` | `ml_inference_pool.spawn(...)` |
| 2 | `services/store_ops.rs` | ~line 113 | `spawn_blocking_with_timeout` | `ml_inference_pool.spawn(...)` |
| 3 | `services/store_correct.rs` | ~line 50 | `spawn_blocking_with_timeout` | `ml_inference_pool.spawn(...)` |
| 4 | `background.rs` | ~line 543 | `spawn_blocking` | `ml_inference_pool.spawn(...)` — whole scan closure |
| 5 | `background.rs` | ~line 1162 | `spawn_blocking` | `ml_inference_pool.spawn(...)` — whole loop closure |
| 6 | `uds/listener.rs` | ~line 1383 | `spawn_blocking` | `ml_inference_pool.spawn(...)` |
| 7 | `services/status.rs` | ~line 542 | `spawn_blocking_with_timeout` | `ml_inference_pool.spawn(...)` |
| 8 | `unimatrix-core/async_wrappers.rs` | lines 100, 110 | `spawn_blocking` in `AsyncEmbedService` | Remove (dead code) |

### Sites remaining on spawn_blocking (non-inference, must NOT move)

| File | Description | Why it stays |
|------|-------------|--------------|
| `infra/embed_handle.rs:~76` | `OnnxProvider::new(config)` | File I/O + ONNX session init — not steady-state inference |
| `background.rs:~1088` | `run_extraction_rules` | Pure in-memory rule evaluation, no ONNX |
| `background.rs:~1144` | `persist_shadow_evaluations` | DB write |
| `server.rs`, `gateway.rs`, `usage.rs` | Registry reads, audit writes, rate-limit checks | I/O-bound DB or short-duration CPU |
| `uds/listener.rs` (various non-warmup) | Session lifecycle DB writes, signal dispatch | I/O-bound |

---

## Constraints

**C-01**: `rayon` is added only to `unimatrix-server/Cargo.toml`. `unimatrix-core`
stays lean. Rayon is a deployment scheduling concern, not a domain abstraction.
Authority: architect consultation (crt-022a), ADR-001 (Core Crate as Trait Host,
entry #71).

**C-02**: `ort = "=2.0.0-rc.9"` is pinned in `unimatrix-embed/Cargo.toml` and must
not be changed. All ONNX inference (W1-4 NLI, W3-1 GNN) must use the same pinned
version.

**C-03**: `OnnxProvider::new` must remain on `spawn_blocking`. This is not a migration
target.

**C-04**: `AsyncVectorStore` must remain in `unimatrix-core`. HNSW operations are not
the problem workload and are explicitly out of scope.

**C-05**: Exactly one rayon pool for W1-2. A second rayon pool for GGUF long-duration
inference is a W2-4 concern and must not be introduced here.

**C-06**: No schema changes. This feature adds no tables, no columns, no migrations.

**C-07**: No `unimatrix-onnx` crate. Extraction is deferred to before W3-1 ships.
Authority: architect consultation (crt-022a). A `// TODO(W3-1): extract to
unimatrix-onnx` comment documents the deferral at the relevant code site.

**C-08**: The `[inference]` config section is named `[inference]` (not `[nli]` as in
earlier drafts and not `[rayon]`). This name accommodates W1-4 NLI and W2-4 GGUF
parameters without renaming.

**C-09 (CI enforcement of AC-07)**: Per SR-05, the AC-07 "no spawn_blocking for ONNX
inference" requirement must be enforced by a CI grep step, not a post-ship manual
audit. The implementer shall add a CI check (e.g., a `cargo xtask` step or a shell
script in the CI pipeline) that greps `spawn_blocking` in
`crates/unimatrix-server/src/services/` and
`crates/unimatrix-server/src/background.rs` and fails if any result appears that is
not in the permitted non-inference list. The check must run on every PR against main.

**C-10 (pool distribution via ServiceLayer)**: Per SR-06, `Arc<RayonPool>` (named
`ml_inference_pool`) must be placed in `ServiceLayer` (the server's top-level
application state struct) and distributed from there to all inference consumers at
startup. Ad-hoc per-consumer instantiation is forbidden. This ensures W1-4 and W3-1
do not inadvertently create a second pool.

**C-11 (timeout semantics)**: The `spawn_blocking_with_timeout` wrapper at the four
MCP handler call sites (search, store, correct, status) enforces `MCP_HANDLER_TIMEOUT`
on embedding calls. After migration to rayon, these sites use
`ml_inference_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, move || ...)` instead of
a per-call-site `tokio::time::timeout` wrapper. The three background call sites
(contradiction scan, quality-gate loop, warmup) use `ml_inference_pool.spawn(...)` with
no timeout, because indefinite occupancy of one rayon thread is acceptable for
background work.

This resolves OQ-2. The architect selected option (b): a `spawn_with_timeout` variant
on `RayonPool` (ADR-003). See `ARCHITECTURE.md §timeout-semantics` for the full
rationale. `RayonError::TimedOut(Duration)` is the error variant returned when the
timeout elapses; it maps to `ServiceError::EmbeddingFailed` at the call site.

---

## Dependencies

### New dependencies

| Crate | Version | Added to | Justification |
|-------|---------|----------|---------------|
| `rayon` | `"1"` | `unimatrix-server/Cargo.toml` | ML inference thread pool |

### Existing dependencies (relied upon, must not change)

| Crate | Version | Location | Role in this feature |
|-------|---------|----------|---------------------|
| `tokio` | existing | `unimatrix-server` | `oneshot` channel for rayon bridge; runtime for awaiting |
| `ort` | `"=2.0.0-rc.9"` | `unimatrix-embed` | ONNX runtime; pinned, must not change |
| `toml` | `"0.8"` | `unimatrix-server` | Already present; used by InferenceConfig deserialization |
| `thiserror` | existing | `unimatrix-server` | `RayonError` derive |
| `num_cpus` | check | `unimatrix-server` | Default pool size formula — verify presence or add |

### Existing components consumed

| Component | Location | How used |
|-----------|----------|----------|
| `EmbedServiceHandle` | `infra/embed_handle.rs` | State machine providing `get_adapter()`; not changed |
| `EmbedAdapter` | `unimatrix-embed` | The `Send + 'static` type passed into rayon closures |
| `ServiceLayer` | `unimatrix-server` | Container for `Arc<RayonPool>`; pattern from entry #316 |
| `UnimatrixConfig` | `infra/config.rs` | Extended with `inference: InferenceConfig` field |
| `ServiceError::EmbeddingFailed` | server service layer | Target error variant for `RayonError::Cancelled` mapping |
| `AsyncVectorStore` | `unimatrix-core/async_wrappers.rs` | Retained; not touched |
| `spawn_blocking_with_timeout` | server infra | Removed from 4 call sites; retained at non-inference sites |

---

## NOT in Scope

The following are explicitly excluded to prevent scope creep:

- **NLI model integration**: No `NliProvider`, `NliServiceHandle`, NLI ONNX session,
  NLI tokenizer, or any NLI-related type. NLI is W1-4.
- **Bootstrap edge promotion**: No `DELETE+INSERT` promotion of `bootstrap_only=1`
  `GRAPH_EDGES` rows. W1-4 concern.
- **NLI post-store pipeline**: No fire-and-forget NLI task, no `Contradicts`/`Supports`
  edge writes, no circuit breaker. W1-4.
- **SHA-256 model hash pinning**: For the existing embedding model download path
  (`download.rs`) or any model. W1-4.
- **Second rayon pool (GGUF)**: W2-4 creates a separate pool sized for long-duration
  inference. Not here.
- **`unimatrix-onnx` crate**: Extraction deferred to before W3-1. A TODO comment
  is the only artefact.
- **`AsyncVectorStore` removal or migration**: HNSW stays on `spawn_blocking`. The
  `async` feature flag and tokio dep in `unimatrix-core` are retained.
- **Schema changes**: No new tables, columns, or migrations.
- **Contradiction scan logic changes**: The internal logic of `scan_contradictions`
  and `conflict_heuristic` is unchanged. Only the execution context changes
  (`spawn_blocking` → rayon).
- **`context_cycle_review` tool rename**: Agile vocabulary issue; a separate scope item.
- **Any W1-3, W1-4, W1-5, W2-x, or W3-x feature content**: This feature is
  infrastructure only — pool establishment and embedding migration.

---

## Open Questions

*Note*: OQ-1 (pool naming) was resolved by human approval: use `ml_inference_pool`.

*Note*: OQ-2 (timeout semantics) was resolved by ADR-003 (architect). Option (b)
selected: `RayonPool::spawn_with_timeout(Duration, f)` for MCP handler call sites;
`RayonPool::spawn(f)` (no timeout) for background call sites. `RayonError` gains the
`TimedOut(Duration)` variant. See C-11 and §Domain Models for the full resolved design.

*Note*: All other open questions from the researcher and architect reports (OQ-5 NLI
provider home, `AsyncVectorStore` migration question, feature flag question) have been
resolved and their resolutions are reflected in this specification.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "rayon thread pool tokio bridge ML inference
  spawn_blocking" — found entry #2491 (Rayon-Tokio bridge pattern, tagged crt-022)
  and #2535 (shared rayon pool monopolisation by long-running background scans,
  tagged crt-022). Both informed the call site inventory, the contradiction scan
  single-closure design, and the pool-size floor reasoning in NFR-04.
- Queried: `/uni-query-patterns` for "OrtSession EmbedAdapter thread safety Send
  async wrappers" — found entries #2524, #68 (ADR-002), #76 (ADR-006 Send+Sync
  traits). ADR-006 confirms the established pattern of `Send + Sync` trait bounds;
  SR-01 thread-safety validation is a prerequisite gate, not a spec resolution.
- Queried: `/uni-query-patterns` for "AppState ServiceLayer startup wiring Arc pool" —
  found entries #316 and #1560 (ServiceLayer extraction pattern, background-tick
  Arc<RwLock<T>> cache pattern). C-10 and the pool distribution workflow are grounded
  in these established patterns.
- No new knowledge stored — patterns are feature-specific and already exist in
  entries #2491, #2524, #2535 from researcher/architect work.
