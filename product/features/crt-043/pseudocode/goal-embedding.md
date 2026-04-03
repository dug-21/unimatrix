# crt-043: Goal Embedding Write Path — Pseudocode

## Purpose

Extend `handle_cycle_event` in `listener.rs` to spawn a fire-and-forget embedding task
(Step 6) after the existing INSERT spawn (Step 5) when a `CycleLifecycle::Start` event
carries a non-empty goal. The embedding task runs on the rayon `ml_inference_pool` and
writes the result to `cycle_events.goal_embedding` via `update_cycle_start_goal_embedding`.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/uds/listener.rs` | Extend `handle_cycle_event` signature; add Step 6 embed spawn; update three call sites in `dispatch_request` |

---

## Modified Function: `handle_cycle_event`

### Signature Change

```
// Before:
fn handle_cycle_event(
    event: &unimatrix_engine::wire::ImplantEvent,
    lifecycle: CycleLifecycle,
    session_registry: &SessionRegistry,
    store: &Arc<Store>,
)

// After (crt-043):
fn handle_cycle_event(
    event: &unimatrix_engine::wire::ImplantEvent,
    lifecycle: CycleLifecycle,
    session_registry: &SessionRegistry,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,  // crt-043: added for goal embedding
)
```

`embed_service` is already available at all three call sites in `dispatch_request` —
it is a parameter of `dispatch_request` itself (line 517 of listener.rs). Only the
`handle_cycle_event` signature and the three call sites need updating.

### New Step 6 — Fire-and-Forget Goal Embedding Spawn

Insert after the closing brace of the existing Step 5 `tokio::spawn` block (around line 2472),
still within `handle_cycle_event`, before the closing brace of the function.

```
// Step 6: Fire-and-forget goal embedding (crt-043).
//
// Spawned after the INSERT spawn (Step 5) so the tokio task queue provides a
// best-effort INSERT-before-UPDATE ordering. The embed task performs CPU-bound
// rayon work before issuing the UPDATE, so the INSERT virtually always commits
// first (ADR-002). The residual race is accepted: a missed UPDATE leaves
// goal_embedding = NULL, identical to the embed-service-unavailable path.
//
// Whitespace-only goal: trimmed before the Some check. A whitespace-only string
// produces a zero-information embedding; treat as absent (no spawn). The goal_for_event
// value used in Step 5 (insert_cycle_event) is NOT trimmed — UDS verbatim storage
// is preserved per col-025 ADR-005 FR-11.
//
// Empty goal (goal_for_event = None): no spawn, no warn (FR-B-09).
if lifecycle == CycleLifecycle::Start {
    // Trim whitespace from the in-memory copy before checking.
    // goal_for_event is the col-025 goal value; clone it to avoid moving.
    let trimmed_goal: Option<String> = goal_for_event
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if let Some(goal_text) = trimmed_goal {
        let embed_svc = Arc::clone(embed_service);
        let store_embed = Arc::clone(store);
        let cycle_id_embed = feature_cycle.clone();   // feature_cycle from Step 1

        let _ = tokio::spawn(async move {
            match embed_svc.get_adapter().await {
                Err(e) => {
                    // Embed service not ready — accepted degradation path (FR-B-10).
                    // goal_embedding remains NULL. Cycle start is not blocked.
                    tracing::warn!(
                        error = %e,
                        "crt-043: embed service not ready; goal_embedding skipped"
                    );
                }
                Ok(adapter) => {
                    match adapter.embed_entry("", &goal_text).await {
                        Err(e) => {
                            // ONNX inference error — accepted degradation (FR-B-10).
                            tracing::warn!(
                                error = %e,
                                "crt-043: goal embedding failed; goal_embedding skipped"
                            );
                        }
                        Ok(vec) => {
                            match unimatrix_store::embedding::encode_goal_embedding(vec) {
                                Err(e) => {
                                    // bincode encode error — unreachable for valid Vec<f32>
                                    // but propagated per FR-B-04.
                                    tracing::warn!(
                                        error = %e,
                                        "crt-043: encode_goal_embedding failed"
                                    );
                                }
                                Ok(bytes) => {
                                    if let Err(e) = store_embed
                                        .update_cycle_start_goal_embedding(
                                            &cycle_id_embed,
                                            bytes,
                                        )
                                        .await
                                    {
                                        tracing::warn!(
                                            error = %e,
                                            cycle_id = %cycle_id_embed,
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
    // If trimmed_goal is None (absent or whitespace-only), no spawn, no warn (FR-B-09).
}
// PhaseEnd and Stop: no embedding spawn (lifecycle != Start guard above).
```

### Call to `encode_goal_embedding`

The import path is `unimatrix_store::embedding::encode_goal_embedding`. The `embedding` module
is `pub(crate)` within `unimatrix-store`. However, `unimatrix-server` is a separate crate that
depends on `unimatrix-store`. `pub(crate)` functions are not accessible across crate boundaries.

Resolution: the implementation agent must expose `encode_goal_embedding` via a thin re-export
or use the function through a wrapper. Two options:

**Option A (preferred): Re-export from unimatrix-store lib.rs as `pub` with a doc note.**

In `unimatrix-store/src/lib.rs`:
```
pub use embedding::{encode_goal_embedding, decode_goal_embedding};
```
Then update `embedding.rs` functions from `pub(crate)` to `pub`.

**Option B: Add a crate-internal wrapper in unimatrix-store.**

Add a `pub fn encode_goal_embedding_pub(...)` wrapper in `embedding.rs` that is `pub`, and
export only the wrapper.

The ADR-001 decision says helpers are `pub(crate)` within `unimatrix-store` because Group 6
consumes them via store query methods. However, the write path in `unimatrix-server` needs
to call `encode_goal_embedding` directly. This is a delivery-time API surface decision.

**Decision for implementation agent:** Use Option A. Promote both helpers to `pub` and
re-export from `lib.rs`. The WARN-2 resolution in OVERVIEW.md stands for `decode_goal_embedding`
(Group 6 calls a store query method, not decode directly). But `encode_goal_embedding` must be
callable from `unimatrix-server`. Making both `pub` is consistent, safe, and follows the
Group 6 pattern doc that Group 6 agents need a "defined, tested encode/decode API to call."

---

## Modified: `dispatch_request` Call Sites

Three call sites in `dispatch_request` (around lines 733-737) must be updated to pass
`embed_service`:

```
// Before (all three):
handle_cycle_event(&event, CycleLifecycle::Start, session_registry, store);
handle_cycle_event(&event, CycleLifecycle::PhaseEnd, session_registry, store);
handle_cycle_event(&event, CycleLifecycle::Stop, session_registry, store);

// After (all three):
handle_cycle_event(&event, CycleLifecycle::Start, session_registry, store, embed_service);
handle_cycle_event(&event, CycleLifecycle::PhaseEnd, session_registry, store, embed_service);
handle_cycle_event(&event, CycleLifecycle::Stop, session_registry, store, embed_service);
```

`embed_service` is already in scope in `dispatch_request` as a parameter (line 517).
No other changes to `dispatch_request` are needed for this component.

---

## State Machine: Goal Embedding Lifecycle

```
CycleStart event received
    │
    ▼
goal_for_event = extract + set_current_goal (Step 3b, synchronous)
    │
    ▼
Step 5: tokio::spawn(insert_cycle_event(goal=goal_for_db))
    │
    ▼
Step 6 entry: lifecycle == Start?
    │
    ├─ No (PhaseEnd / Stop) → skip, function returns
    │
    └─ Yes:
         │
         ▼
         trimmed_goal = goal_for_event.trim().filter(non-empty)
         │
         ├─ None (absent or whitespace-only)
         │    → no spawn, no warn, function returns
         │
         └─ Some(goal_text):
              │
              ▼
              tokio::spawn(embed_goal_task):
                  get_adapter()
                  │
                  ├─ EmbedNotReady → warn!, exit task (goal_embedding = NULL)
                  │
                  └─ Ok(adapter):
                       embed_entry("", goal_text)  [rayon ml_inference_pool]
                       │
                       ├─ Err → warn!, exit task (goal_embedding = NULL)
                       │
                       └─ Ok(vec):
                            encode_goal_embedding(vec)
                            │
                            ├─ Err → warn!, exit task (goal_embedding = NULL)
                            │
                            └─ Ok(bytes):
                                 update_cycle_start_goal_embedding(cycle_id, bytes)
                                 │
                                 ├─ Err → warn!(cycle_id), exit task (goal_embedding = NULL)
                                 │
                                 └─ Ok(()) → goal_embedding written (happy path)
```

---

## NFR Compliance

| NFR | Mechanism |
|-----|-----------|
| NFR-01 (< 5ms added latency) | embed task is fire-and-forget; no await in `handle_cycle_event` |
| NFR-02 (rayon pool, not tokio) | `adapter.embed_entry()` routes through `ml_inference_pool` — no change to embed service internals |
| NFR-03 (Store mutex) | `update_cycle_start_goal_embedding` acquires one connection from write pool independently; it does not share a connection with insert_cycle_event. This is consistent with other fire-and-forget writes (feature_cycle persist, eager attribution). The constraint "not independently from other fire-and-forget work" is interpreted as: do not hold the write pool connection across both spawns. Both spawns are independent async tasks that each acquire and release a connection independently — this is the correct pattern. |

---

## Ordering Comment (Required in Code)

The ADR-002 residual race must be documented in a code comment at the Step 6 spawn site.
The minimum required comment is already included in the pseudocode above. The implementation
agent must not remove it.

---

## Error Handling Summary

| Step | Error Condition | Behavior |
|------|-----------------|---------|
| get_adapter() | EmbedNotReady | warn!, task exits, goal_embedding = NULL |
| embed_entry() | ONNX failure | warn!, task exits, goal_embedding = NULL |
| encode_goal_embedding() | EncodeError | warn!, task exits, goal_embedding = NULL |
| update_cycle_start_goal_embedding() | DB error | warn! with cycle_id, task exits, goal_embedding = NULL |
| INSERT-before-UPDATE race | UPDATE matches 0 rows | silent no-op (Ok(())), goal_embedding = NULL |
| goal absent or whitespace-only | None | no spawn, no warn, goal_embedding = NULL |

All error paths produce the same outcome: `goal_embedding = NULL`. No error is surfaced to
the `context_cycle` MCP caller. The cycle start is not blocked (C-03, FR-B-10).

---

## Key Test Scenarios

For full test scenarios see `test-plan/goal-embedding.md`. Required scenarios:

1. **Happy path** (AC-02, AC-03, R-01 scenario 1) — `handle_cycle_event(Start, goal="design pipeline")` with operational embed stub. Await both spawned tasks. Assert `goal_embedding` on the cycle_start row is non-NULL and decodes to a `Vec<f32>` of the expected dimension.

2. **Empty goal — no spawn** (AC-04b, R-09 scenario 1) — call with `goal = ""`. Verify embed stub receives zero calls. Verify no warn emitted. Verify `goal_embedding = NULL`.

3. **Absent goal — no spawn** (AC-04b, R-09 scenario 2) — call with no goal in payload. Same assertions as scenario 2.

4. **Whitespace-only goal — no spawn** (edge case from OVERVIEW.md) — call with `goal = "   "`. Verify embed stub receives zero calls. Verify `goal_embedding = NULL`.

5. **Embed service unavailable** (AC-04a, R-10 scenario 1) — stub returns EmbedNotReady. Assert handle_cycle_event returns without blocking. Assert warn emitted. Assert `goal_embedding = NULL`.

6. **Embed computation error** (R-10 scenario 2) — stub embed_entry returns Err. Same assertions as scenario 5.

7. **Ordering: INSERT before UPDATE** (R-01 scenario 1) — await both tasks, assert row exists before UPDATE executes. Observable via non-NULL blob on the start row.

8. **MCP response text unchanged** (AC-06, R-12) — call context_cycle through MCP layer with goal. Assert response string byte-for-byte identical to pre-crt-043.

9. **Concurrent CycleStart events** (R-01 scenario 2, slow test) — fire 20 concurrent Start events each with distinct cycle_id and non-empty goal. After all tasks settle, assert all 20 `goal_embedding` columns non-NULL.
