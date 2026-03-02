# Architecture: col-007 Automatic Context Injection

## System Overview

col-007 adds the first knowledge-delivery hook to the cortical implant architecture. It connects the UserPromptSubmit lifecycle event to Unimatrix's search pipeline, enabling automatic context injection into every Claude Code prompt. The feature touches three subsystems: the hook process (client-side), the UDS listener (server-side dispatcher), and the search pipeline (server-side business logic).

col-007 also extends the SessionStart handler to pre-warm the ONNX embedding model, ensuring the first UserPromptSubmit search completes within the 50ms latency budget.

## Component Breakdown

### Component 1: UserPromptSubmit Hook Handler (hook process)

**Location**: `crates/unimatrix-server/src/hook.rs`
**Responsibility**: Extract prompt text from Claude Code's stdin JSON, send ContextSearch request via UDS, format response as plain text stdout.

Changes to existing code:
- Add `"UserPromptSubmit"` arm to `build_request()` that constructs `HookRequest::ContextSearch`
- Add `format_injection()` function for stdout formatting
- Add `is_injection_request()` helper to classify requests as synchronous
- Extend `write_stdout()` to handle `HookResponse::Entries`

### Component 2: UDS ContextSearch Dispatcher (server-side)

**Location**: `crates/unimatrix-server/src/uds_listener.rs`
**Responsibility**: Route `HookRequest::ContextSearch` to the search pipeline, return `HookResponse::Entries`.

Changes to existing code:
- `dispatch_request()` becomes async (signature change)
- Add `HookRequest::ContextSearch` arm that runs the search pipeline
- `start_uds_listener()` receives additional shared state (see ADR-001)
- `handle_connection()` uses async dispatch

### Component 3: Injection Formatter (hook process)

**Location**: `crates/unimatrix-server/src/hook.rs` (same file as Component 1)
**Responsibility**: Format `Vec<EntryPayload>` as structured plain text within token budget.

New code:
- `format_injection()`: iterate entries in rank order, append to output until byte budget exhausted
- `MAX_INJECTION_BYTES` constant (1400 bytes, ~350 tokens at 4 bytes/token)
- `SIMILARITY_FLOOR` constant (0.5)
- `CONFIDENCE_FLOOR` constant (0.3)

### Component 4: SessionStart Pre-Warming (server-side)

**Location**: `crates/unimatrix-server/src/uds_listener.rs`
**Responsibility**: On SessionStart, block until embedding model is warm, then run a no-op embedding to ensure ONNX runtime is fully initialized.

Changes to existing code:
- `SessionRegister` handler becomes async (awaits `embed_service.get_adapter()`)
- Calls `adapter.embed_entry("", "warmup")` synchronously via `spawn_blocking`
- Returns `Ack` only after warming completes

## Component Interactions

```
Claude Code                Hook Process              UDS Listener            Search Pipeline
    |                          |                          |                         |
    |--UserPromptSubmit------->|                          |                         |
    |  stdin: {prompt: "..."}  |                          |                         |
    |                          |--ContextSearch----------->|                         |
    |                          |  via LocalTransport       |                         |
    |                          |                          |--embed query----------->|
    |                          |                          |  spawn_blocking          |
    |                          |                          |<--embedding vector------|
    |                          |                          |--HNSW search----------->|
    |                          |                          |<--candidate IDs---------|
    |                          |                          |--fetch entries---------->|
    |                          |                          |<--full entries-----------|
    |                          |                          |--re-rank + boost-------->|
    |                          |                          |<--ranked results---------|
    |                          |                          |--co-access pairs-------->|
    |                          |                          |  (session-scoped dedup)  |
    |                          |<--Entries response--------|                         |
    |                          |                          |                         |
    |<--stdout: formatted------|                          |                         |
    |   knowledge entries      |                          |                         |
```

### SessionStart Warming Flow

```
Claude Code                Hook Process              UDS Listener            EmbedServiceHandle
    |                          |                          |                         |
    |--SessionStart----------->|                          |                         |
    |  stdin: {session_id}     |                          |                         |
    |                          |--SessionRegister-------->|                         |
    |                          |  (fire-and-forget)       |                         |
    |                          |                          |--get_adapter().await---->|
    |                          |                          |  (blocks until Ready)    |
    |                          |                          |<--adapter----------------|
    |                          |                          |--embed_entry("","warmup")|
    |                          |                          |  (spawn_blocking)        |
    |                          |                          |<--done-------------------|
    |                          |                          |  (ONNX runtime warm)     |
```

Note: The hook process fires SessionRegister as fire-and-forget (`transport.fire_and_forget()`), so it exits immediately. The server-side warming is asynchronous from the hook's perspective but synchronous within the server (the Ack is sent only after warming completes, but the hook process has already disconnected).

## Technology Decisions

### ADR-001: UDS Listener Shared State via Parameter Expansion

See `architecture/ADR-001-uds-shared-state.md`.

The UDS listener receives the additional services it needs as individual Arc parameters in `start_uds_listener()`. No shared extraction of the search pipeline into `unimatrix-engine`. The search pipeline logic is implemented directly in the UDS dispatcher module, calling the same underlying services (embed, HNSW, re-rank, boost) that the MCP tool uses. This duplicates the orchestration code (~40 lines of pipeline wiring) but avoids the coupling risk of a shared function that would need to import types from both `unimatrix-server` (EmbedServiceHandle, AdaptationService) and `unimatrix-core` (AsyncEntryStore, AsyncVectorStore).

### ADR-002: Async Dispatch for UDS Listener

See `architecture/ADR-002-async-uds-dispatch.md`.

The `dispatch_request()` function becomes async. All existing handlers are trivially async (log + return Ack). The ContextSearch handler uses async for embedding (`spawn_blocking`) and entry fetching. This is a mechanical change that simplifies the code path.

### ADR-003: Session-Scoped Co-Access Dedup via In-Memory Set

See `architecture/ADR-003-session-coaccess-dedup.md`.

The UDS listener maintains a lightweight in-memory `HashMap<String, HashSet<Vec<u64>>>` keyed by session_id to track which entry-set combinations have already had co-access pairs recorded. This prevents redundant co-access writes when the same entries are injected across multiple prompts in a session.

## Integration Points

### Existing Components Used (Read-Only Integration)

| Component | Crate | What col-007 Uses |
|-----------|-------|-------------------|
| `EmbedServiceHandle` | unimatrix-server | `get_adapter()` for query embedding, `is_ready()` for readiness check |
| `AdaptationService` | unimatrix-adapt | `adapt_embedding()` for MicroLoRA adaptation |
| `AsyncVectorStore` | unimatrix-core | `search()` and `search_filtered()` for HNSW similarity search |
| `AsyncEntryStore` | unimatrix-core | `get()` for fetching full entry records |
| `Store` | unimatrix-store | `record_co_access()` for co-access pair recording |
| `confidence` module | unimatrix-engine | `rerank_score()` for blended similarity+confidence scoring |
| `coaccess` module | unimatrix-engine | `generate_pairs()`, `compute_search_boost()` for co-access |

### Existing Components Modified

| Component | Change | Risk |
|-----------|--------|------|
| `hook.rs` | Add UserPromptSubmit arm, injection formatting | Low -- additive |
| `uds_listener.rs` | Async dispatch, additional parameters, ContextSearch handler, SessionStart warming | Medium -- signature changes |
| `wire.rs` | Remove `#[allow(dead_code)]` from ContextSearch, Entries, EntryPayload | Low -- activating existing stubs |
| `HookInput` (wire.rs) | Add `prompt: Option<String>` field | Low -- additive with `#[serde(default)]` |
| `main.rs` | Pass additional Arcs to `start_uds_listener()` | Low -- mechanical |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `start_uds_listener()` | `async fn(socket_path: &Path, store: Arc<Store>, embed_service: Arc<EmbedServiceHandle>, vector_store: Arc<AsyncVectorStore<VectorAdapter>>, entry_store: Arc<AsyncEntryStore<StoreAdapter>>, adapt_service: Arc<AdaptationService>, server_uid: u32, server_version: String) -> io::Result<(JoinHandle<()>, SocketGuard)>` | uds_listener.rs (modified) |
| `dispatch_request()` | `async fn(request: HookRequest, store: &Arc<Store>, embed_service: &Arc<EmbedServiceHandle>, vector_store: &Arc<AsyncVectorStore<VectorAdapter>>, entry_store: &Arc<AsyncEntryStore<StoreAdapter>>, adapt_service: &Arc<AdaptationService>, server_version: &str, coaccess_dedup: &CoAccessDedup) -> HookResponse` | uds_listener.rs (modified) |
| `format_injection()` | `fn(entries: &[EntryPayload], max_bytes: usize) -> Option<String>` | hook.rs (new) |
| `build_request()` | `fn(event: &str, input: &HookInput) -> HookRequest` | hook.rs (modified -- add UserPromptSubmit arm) |
| `HookInput.prompt` | `pub prompt: Option<String>` | wire.rs (modified) |
| `CoAccessDedup` | `struct { sessions: Mutex<HashMap<String, HashSet<Vec<u64>>>> }` | uds_listener.rs (new) |
| `MAX_INJECTION_BYTES` | `const: usize = 1400` | hook.rs (new) |
| `SIMILARITY_FLOOR` | `const: f64 = 0.5` | uds_listener.rs (new) |
| `CONFIDENCE_FLOOR` | `const: f64 = 0.3` | uds_listener.rs (new) |
| `INJECTION_K` | `const: usize = 5` | uds_listener.rs (new) |
| `EF_SEARCH` | `const: usize = 32` | uds_listener.rs (new, mirrors tools.rs constant) |

## Files to Create/Modify

### New Files

| File | Summary |
|------|---------|
| `product/features/col-007/architecture/ADR-001-uds-shared-state.md` | Parameter expansion for UDS shared state |
| `product/features/col-007/architecture/ADR-002-async-uds-dispatch.md` | Async dispatch decision |
| `product/features/col-007/architecture/ADR-003-session-coaccess-dedup.md` | In-memory co-access dedup |

### Modified Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/uds_listener.rs` | Async dispatch, ContextSearch handler, SessionStart warming, CoAccessDedup, additional parameters |
| `crates/unimatrix-server/src/hook.rs` | UserPromptSubmit arm, injection formatting, constants |
| `crates/unimatrix-engine/src/wire.rs` | Remove dead_code attrs, add prompt field to HookInput |
| `crates/unimatrix-server/src/main.rs` | Pass additional Arcs to start_uds_listener() |

## Architectural Constraints

1. **No shared search function in unimatrix-engine** (ADR-001). The search pipeline orchestration lives in the UDS dispatcher alongside the MCP tool's version. The underlying service calls are identical; the wiring is duplicated.

2. **Dispatcher is fully async** (ADR-002). All handler arms are async, even those that don't need it.

3. **Co-access dedup is session-scoped and in-memory** (ADR-003). No persistence. Session entries are cleaned up when `SessionClose` is dispatched.

4. **Hook process remains synchronous** (inherited from col-006 ADR-002). No tokio runtime in the hook path. All async operations happen server-side.

5. **Pre-warming blocks the SessionRegister response** but not the hook process. The hook fires SessionRegister as fire-and-forget and exits immediately. The server completes warming before processing subsequent requests.
