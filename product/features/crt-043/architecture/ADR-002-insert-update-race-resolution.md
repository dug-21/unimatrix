## ADR-002: Goal Embedding INSERT/UPDATE Race Resolution — Option 1 (Embed Spawn in UDS Listener)

### Context

`goal_embedding` is written to `cycle_events` via a two-step sequence:

1. **INSERT** — `insert_cycle_event()` is called from within `handle_cycle_event` (Step 5),
   spawned fire-and-forget via `tokio::spawn`. This creates the `cycle_events` row.
2. **UPDATE** — `update_cycle_start_goal_embedding()` must target that row to write the blob.

These two operations are unordered by default. An UPDATE targeting a row that does not yet
exist is a silent no-op in SQLite — it returns zero rows affected, not an error. This is the
race described in SCOPE.md §Architecture Constraint and SR-01/SR-07.

SCOPE.md presents three options. SR-07 requires verifying whether `EmbedServiceHandle` is
accessible in the UDS listener before committing to Option 1.

**Verification result (from code inspection):**

- `EmbedServiceHandle` is imported in `listener.rs` at line 38.
- `dispatch_request()` receives `embed_service: &Arc<EmbedServiceHandle>` at line 517.
- `handle_cycle_event()` current signature does NOT include `embed_service` — it receives
  only `(event, lifecycle, session_registry, store)` — but `embed_service` IS in scope at
  all three call sites in `dispatch_request` (lines 733, 735, 737).
- `UnimatrixServer.embed_service: Arc<EmbedServiceHandle>` is confirmed at `server.rs:196`.

Option 1 is available. The embed service handle requires only a signature extension to
`handle_cycle_event` to pass through.

**Option 2 architectural analysis:**

Option 2 (inline embed in MCP handler, pass bytes in UDS message) is architecturally
unavailable. The MCP `context_cycle` handler does not call `handle_cycle_event` or dispatch
into the UDS listener at all. The hook fires a UDS `RecordEvent` independently; the MCP tool
returns an acknowledgment. There is no point in the MCP handler where the UDS INSERT is
triggered. Option 2 as stated in SCOPE.md does not correspond to the actual call graph.

**Option 3 analysis:**

Option 3 (retry loop in MCP handler spawn) is architecturally unavailable for the same reason
as Option 2 — the MCP handler has no view of UDS INSERT timing and cannot observe the row's
existence without a polling loop against the store. This adds complexity with no benefit over
Option 1.

### Decision

**Use Option 1: spawn the embedding task from within `handle_cycle_event`, after the INSERT spawn.**

Extend `handle_cycle_event`'s signature to accept `embed_service: &Arc<EmbedServiceHandle>`:

```rust
fn handle_cycle_event(
    event: &ImplantEvent,
    lifecycle: CycleLifecycle,
    session_registry: &SessionRegistry,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
)
```

After Step 5 (INSERT spawn), add Step 6 for `CycleLifecycle::Start` when `goal_for_event.is_some()`:

```rust
// Step 6: Fire-and-forget goal embedding (crt-043).
// Spawned after the INSERT spawn so the tokio queue ordering provides a best-effort
// INSERT-before-UPDATE guarantee. Embedding is CPU-bound (rayon pool); the INSERT
// will virtually always commit before the UPDATE executes.
if lifecycle == CycleLifecycle::Start {
    if let Some(goal_text) = goal_for_event {
        let embed_svc = Arc::clone(embed_service);
        let store_clone = Arc::clone(store);
        let cycle_id = feature_cycle.clone();
        let _ = tokio::spawn(async move {
            match embed_svc.get_adapter().await {
                Err(e) => {
                    tracing::warn!(error = %e, "crt-043: embed service not ready, goal_embedding skipped");
                }
                Ok(adapter) => {
                    match adapter.embed_entry("", &goal_text).await {
                        Err(e) => {
                            tracing::warn!(error = %e, "crt-043: goal embedding failed");
                        }
                        Ok(vec) => {
                            match unimatrix_store::encode_goal_embedding(vec) {
                                Err(e) => {
                                    tracing::warn!(error = %e, "crt-043: encode_goal_embedding failed");
                                }
                                Ok(bytes) => {
                                    if let Err(e) = store_clone
                                        .update_cycle_start_goal_embedding(&cycle_id, bytes)
                                        .await
                                    {
                                        tracing::warn!(
                                            error = %e,
                                            cycle_id = %cycle_id,
                                            "crt-043: update_cycle_start_goal_embedding failed"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}
```

**Ordering rationale:** tokio registers spawned tasks in FIFO order on the runtime's task queue.
Spawning the embed task after the INSERT task means the INSERT task is enqueued first. The
embed task also performs substantial CPU work on the rayon pool (`ml_inference_pool`) before
issuing the UPDATE — this adds at minimum tens of milliseconds of latency before the UPDATE
executes. In practice the INSERT will always be committed before the UPDATE runs.

**Residual race:** The multi-threaded tokio runtime may schedule the embed task on a free thread
before the INSERT task completes in pathological cases. If the UPDATE executes first, it matches
zero rows — the column stays NULL, identical to the embed-service-unavailable degradation.
No data corruption, no error surfaced to the caller. The outcome is cold-start compatible
(NULL `goal_embedding` is the accepted baseline for pre-v21 rows).

**Acceptable degradation:** A missed UPDATE due to the residual race is not retried. The goal
embedding for that cycle is permanently NULL unless the feature is re-run. This is consistent
with the fire-and-forget contract: `context_cycle` response is not blocked, and embedding
failure (for any reason) degrades to NULL rather than blocking cycle start.

**If the residual race must be eliminated in future:** implement the embedding task to verify
`rows_affected > 0` after the UPDATE and retry once after a short sleep. This is an enhancement;
it is not required for crt-043 and would complicate the fire-and-forget semantics.

### Consequences

Easier:
- No UDS message schema changes (Option 2 avoided)
- No new retry complexity (Option 3 avoided)
- `embed_service` is already accessible in `dispatch_request`; only a signature extension needed
- INSERT and embed task are co-located in `handle_cycle_event` — easy to audit and test together
- Degradation mode (NULL `goal_embedding`) is identical to embed-service-unavailable path

Harder:
- `handle_cycle_event` gains a fifth parameter; all three call sites in `dispatch_request`
  must be updated
- The residual race requires documentation in comments to prevent future agents from treating
  the NULL column as a bug
