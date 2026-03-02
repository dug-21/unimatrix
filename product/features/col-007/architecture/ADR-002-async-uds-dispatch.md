## ADR-002: Async UDS Dispatch

### Context

The `dispatch_request()` function in `uds_listener.rs` is currently synchronous (`fn dispatch_request(...) -> HookResponse`). col-007's ContextSearch handler needs async operations: `embed_service.get_adapter().await`, `vector_store.search().await`, and `entry_store.get().await`. The SessionStart pre-warming handler also needs `get_adapter().await`.

Two approaches were considered:

**Option A: Hybrid dispatch.** Keep `dispatch_request()` synchronous for existing handlers (Ping, SessionRegister, etc.) and add a separate async dispatch path for ContextSearch. This preserves the sync-simplicity of existing handlers but adds branching complexity.

**Option B: Fully async dispatch.** Make `dispatch_request()` async for all handlers. Existing handlers are trivially async (they just log and return Ack). The async keyword is added to their signature but the implementation doesn't change.

### Decision

Use Option B: fully async dispatch.

`dispatch_request()` becomes `async fn dispatch_request(...) -> HookResponse`. All handler arms are async. This is a mechanical change: existing handlers add `.await` to nothing (they're synchronous operations in an async function, which is valid Rust). The ContextSearch and SessionRegister handlers use actual async operations.

The `handle_connection()` function already runs in an async context (spawned via `tokio::spawn`), so the caller is already async-ready.

### Consequences

**Easier:**
- Single dispatch function, no branching between sync and async paths.
- Future hook handlers (col-008 CompactPayload, col-011 routing) can use async operations without refactoring.
- Consistent code style across all handlers.

**Harder:**
- Trivially async handlers (Ping, RecordEvent) carry the `async` overhead. In practice this is negligible since they're already running inside a tokio task.
