# crt-043: Behavioral Signal Infrastructure — Architecture

## System Overview

crt-043 is signal plumbing: it adds two new data fields to existing tables so that downstream
behavioral analytics (Group 6 — S6/S7 edge emission; Group 7 — goal-conditioned briefing) have
the columns they need. No retrieval path changes. No new crate dependencies. No user-visible
behavior changes.

The two items share one schema migration (v20 → v21) and touch disjoint code paths:

- **Item B** — `goal_embedding BLOB` on `cycle_events`: write path is the UDS listener's
  `handle_cycle_event` function, triggered by `CycleStart` hook events
- **Item C** — `phase TEXT` on `observations`: write path is the UDS listener's
  `insert_observation` / `insert_observations_batch` functions, triggered by all four
  observation write sites

## Component Breakdown

### 1. Schema Migration (unimatrix-store)

`crates/unimatrix-store/src/migration.rs` — `run_main_migrations()` block for `current_version < 21`

Responsibilities:
- Add `goal_embedding BLOB` to `cycle_events` with `pragma_table_info` idempotency pre-check
- Add `phase TEXT` to `observations` with `pragma_table_info` idempotency pre-check
- Both ADD COLUMN statements execute inside the existing outer transaction opened in
  `migrate_if_needed()` — no additional `BEGIN`/`COMMIT` needed (the caller's transaction
  is the atomicity boundary)
- Bump `schema_version` counter to 20
- Update `CURRENT_SCHEMA_VERSION` constant from 20 to 21

### 2. Goal Embedding Write Path (unimatrix-server, UDS listener)

`crates/unimatrix-server/src/uds/listener.rs`

**`handle_cycle_event`** — extended to accept `embed_service: &Arc<EmbedServiceHandle>`

After the existing Step 5 `tokio::spawn` (which fires `insert_cycle_event`), a second
`tokio::spawn` fires the embedding task when `lifecycle == CycleLifecycle::Start` and
`goal_for_event.is_some()`. The embedding task:

1. Calls `embed_service.get_adapter().await`
2. On `EmbedNotReady`: `tracing::warn!` and returns (no UPDATE)
3. On success: calls `adapter.embed_entry("", goal_text)` on `ml_inference_pool`
4. Encodes result via `encode_goal_embedding(vec)` helper (bincode)
5. Calls `store.update_cycle_start_goal_embedding(cycle_id, bytes)` — a new async store method
   that issues `UPDATE cycle_events SET goal_embedding = ?1 WHERE topic = ?2 AND event_type = 'cycle_start'`

**`dispatch_request`** — passes `embed_service` to `handle_cycle_event` at all three call sites
(Start, PhaseEnd, Stop). PhaseEnd and Stop callers receive it but `handle_cycle_event` ignores
it for those lifecycle values.

**New store method** (`unimatrix-store/src/db.rs`):
```
pub async fn update_cycle_start_goal_embedding(
    &self,
    cycle_id: &str,
    embedding_bytes: Vec<u8>,
) -> Result<()>
```

**New serialization helpers** (`unimatrix-store/src/embedding.rs` or inline in `db.rs`):
```
pub(crate) fn encode_goal_embedding(vec: Vec<f32>) -> Result<Vec<u8>, bincode::error::EncodeError>
pub(crate) fn decode_goal_embedding(bytes: &[u8]) -> Result<Vec<f32>, bincode::error::DecodeError>
```

### 3. Phase Capture Write Path (unimatrix-server, UDS listener)

`crates/unimatrix-server/src/uds/listener.rs`

**`ObservationRow`** — add `phase: Option<String>` field

**`extract_observation_fields`** — does NOT capture phase (it has no session_registry access).
Phase is captured at the call site, after `extract_observation_fields` returns, using the same
pre-`spawn_blocking` session registry read pattern as `topic_signal` enrichment.

**Four write sites** in `dispatch_request`:
1. `RecordEvent` path — `obs.phase = session_registry.get_state(&event.session_id).and_then(|s| s.current_phase.clone())`
2. `post_tool_use_rework_candidate` path — same pattern
3. `RecordEvents` batch path — same pattern per event
4. `ContextSearch` path — same pattern (if it writes observations)

**`insert_observation`** and **`insert_observations_batch`** — add `phase` to the INSERT column
list and bind `&obs.phase`.

## Component Interactions

```
Hook event (CycleStart)
  → UDS accept loop
  → dispatch_request(embed_service)
  → handle_cycle_event(embed_service)          [synchronous: set_current_phase, set_current_goal]
      → tokio::spawn: insert_cycle_event()     [fire-and-forget INSERT, no embedding]
      → tokio::spawn: embed_goal_task()        [fire-and-forget, runs after INSERT spawn]
          → embed_service.get_adapter()
          → adapter.embed_entry() on ml_inference_pool
          → encode_goal_embedding()
          → store.update_cycle_start_goal_embedding()  [UPDATE, after row exists]

Hook event (RecordEvent, RecordEvents, ContextSearch)
  → dispatch_request
  → extract_observation_fields()               [no session_registry access]
  → obs.phase = session_registry.get_state().current_phase  [pre-spawn_blocking capture]
  → spawn_blocking: insert_observation(obs)    [INSERT includes phase]
```

## INSERT/UPDATE Race Resolution

**Decision: Option 1 — embed task spawned from within `handle_cycle_event`, after the INSERT spawn.**

### Analysis of the Three Options

The race exists because the goal embedding UPDATE must target a row that the INSERT creates.
Both fire-and-forget spawns are unordered by default.

**Why Option 2 (inline embed before UDS dispatch) is not applicable:**

The MCP `context_cycle` handler does not dispatch into the UDS listener. These are independent
paths: the hook fires a UDS RecordEvent; the MCP tool returns an acknowledgment. There is no
point in the MCP handler where the UDS INSERT is triggered and an embedding task can be paired
with it. Option 2 as described in SCOPE.md is architecturally unavailable.

**Why Option 3 (MCP handler with retry) is not applicable:**

Same reason — the MCP handler has no view of when the UDS INSERT completes. A retry loop in
the MCP handler would poll blindly against a row whose INSERT timing it cannot observe.

**Why Option 1 is correct:**

`handle_cycle_event` is the only site that (a) knows the Start event just occurred, (b) has
the validated `goal_for_event` text, and (c) is called synchronously before the INSERT spawn
fires. By spawning the embedding task inside `handle_cycle_event` after the INSERT spawn, we
get the strongest available ordering: the INSERT spawn is registered with tokio before the
embed spawn. Under a single-threaded runtime this is a guarantee; under the multi-threaded
runtime it is a strong probabilistic ordering. The embed task is non-trivial CPU work (rayon
pool), so it will always take longer than the INSERT to complete — the INSERT will virtually
always be committed before the UPDATE executes.

**Residual race:** The ordering is not guaranteed under the multi-threaded tokio runtime. The
UPDATE may theoretically execute before the INSERT commits if tokio schedules the embed spawn
on a free thread before the INSERT spawn runs. Mitigation: `update_cycle_start_goal_embedding`
uses `UPDATE ... WHERE topic = ? AND event_type = 'cycle_start'` — a missed UPDATE is a
silent no-op (the column stays NULL), which is the same outcome as the embed-service-unavailable
path. No data corruption. The degradation is cold-start compatible with the accepted NULL
baseline for pre-v21 rows.

**If the residual race is unacceptable in future:** the embedding task can call
`get_next_cycle_seq` to retrieve the row's seq, retry the UPDATE once if zero rows affected,
then log a warn. This is an enhancement, not required for crt-043.

**Signature change to `handle_cycle_event`:**
```rust
fn handle_cycle_event(
    event: &ImplantEvent,
    lifecycle: CycleLifecycle,
    session_registry: &SessionRegistry,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,  // added
)
```

`embed_service` is already available in `dispatch_request` at all three call sites.

## Integration Points

### unimatrix-store

- `migration.rs`: new `current_version < 21` block; `CURRENT_SCHEMA_VERSION` → 20
- `db.rs`: new `update_cycle_start_goal_embedding(cycle_id, bytes)` method
- New `encode_goal_embedding` / `decode_goal_embedding` helpers (crate-internal, `pub(crate)`)
- `insert_observation` / `insert_observations_batch`: add `phase` column bind

### unimatrix-server / uds / listener.rs

- `handle_cycle_event`: add `embed_service` parameter; add embedding spawn after INSERT spawn
- `dispatch_request`: pass `embed_service` at all three `handle_cycle_event` call sites
- `ObservationRow`: add `phase: Option<String>`
- Four observation write sites: capture `current_phase` before `spawn_blocking`

### Bincode dependency

`bincode` is already in the Cargo workspace. No new crate dependencies.

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `handle_cycle_event` (extended) | `fn(event: &ImplantEvent, lifecycle: CycleLifecycle, session_registry: &SessionRegistry, store: &Arc<Store>, embed_service: &Arc<EmbedServiceHandle>)` | `uds/listener.rs` |
| `Store::update_cycle_start_goal_embedding` | `pub async fn(&self, cycle_id: &str, embedding_bytes: Vec<u8>) -> Result<()>` | `unimatrix-store/src/db.rs` |
| `encode_goal_embedding` | `pub(crate) fn(vec: Vec<f32>) -> Result<Vec<u8>, bincode::error::EncodeError>` | `unimatrix-store` |
| `decode_goal_embedding` | `pub(crate) fn(bytes: &[u8]) -> Result<Vec<f32>, bincode::error::DecodeError>` | `unimatrix-store` |
| `ObservationRow.phase` | `phase: Option<String>` | `uds/listener.rs` |
| `insert_observation` | adds `phase` bind at position ?9 | `uds/listener.rs` |
| `insert_observations_batch` | adds `phase` bind at position ?9 | `uds/listener.rs` |
| `cycle_events.goal_embedding` | `BLOB NULL` | schema v21 |
| `observations.phase` | `TEXT NULL` | schema v21 |

## Technology Decisions

- **Bincode for embedding blob** — see ADR-001
- **Option 1 for INSERT/UPDATE race** — see ADR-002
- **Single v20→v21 migration block for both columns** — see ADR-003

## Open Questions

None. All critical decisions resolved.

The `(topic_signal, phase)` composite index on `observations` is deferred to the delivery
agent per SCOPE.md. The delivery agent must evaluate and decide — it must not be deferred
further to Group 6.
