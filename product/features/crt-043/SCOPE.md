# crt-043: Behavioral Signal Infrastructure — Goal Embedding + Phase Capture

## Problem Statement

ASS-040 validated three behavioral signal sources (S6, S7, H1, H3) that require infrastructure
plumbing before they can emit or be consumed. Group 5 of the ASS-040 roadmap identifies these
items as prerequisites for Group 6 (behavioral edge emission) and Group 7 (goal-conditioned
briefing). They produce no user-visible retrieval changes on their own — they are signal plumbing.

**Item A (dropped — already exists): observations feature attribution**
The original scope proposed adding `feature_id TEXT` to `observations`. This is unnecessary.
The `topic_signal TEXT` column (added col-017, enriched col-024) already serves this purpose.
`topic_signal` is populated at every observation write site from two sources: `extract_topic_signal(input)`
pattern-matches recognizable feature IDs in hook event text; when that returns `None`,
`enrich_topic_signal()` falls back to `session_registry.get_state(session_id)?.feature`
(the active feature cycle from the session registry). The col-024 `enrich_topic_signal`
helper centralises this logic across all four write sites in `listener.rs`. **`topic_signal`
is the feature ID for S6/S7 query purposes.** No new column is needed.

**Item B — Goal embedding at cycle start**: The `cycle_events` table stores a `goal TEXT`
column (added col-025 / schema v16). When a `context_cycle(type=start)` call carries a
`goal` parameter, that text is already present in the MCP handler. H1 (goal clustering with
cosine similarity) requires an embedding of the feature goal, not just the text. The work is:
when `goal` text is non-empty at cycle start, embed it via the existing `EmbedServiceHandle`
pipeline and store the embedding blob on the `cycle_events` start row. No external fetch.
No new dependencies.

**Item C — Phase capture on observations**: The `observations` table records every behavioral
event but carries no phase signal. Phase stratification (H3) requires knowing which cycle phase
was active when each observation was written. The active phase is already tracked on
`SessionState.current_phase` (added crt-025), set via `set_current_phase()` and available
via `get_state(session_id)?.current_phase` — the same O(1) in-memory lookup used by
`enrich_topic_signal`. It just needs to flow into the observation row at write time. This is
more reliable than agent_role: phase is a structured system value updated by `context_cycle`
events, not a free-text field dependent on naming conventions.

## Goals

1. Add `goal_embedding BLOB` to `cycle_events` (schema v21). At `context_cycle(type=start)`,
   when the `goal` parameter is non-empty, embed the goal text via the existing
   `EmbedServiceHandle` pipeline (rayon pool) and store the embedding on the start row.
   Fire-and-forget from the MCP handler — cycle start response is not blocked.
   No external fetch. No new crate dependencies.

2. Add `phase TEXT` to `observations` (schema v21, same migration step). At every observation
   row insertion, capture `SessionState.current_phase` from the session registry before entering
   `spawn_blocking` and bind it to the new column. NULL when no active cycle or no phase set.

## Non-Goals

- **No retrieval changes**: crt-043 does not modify search ranking, briefing selection, or
  any entry retrieval path. Signal plumbing only.
- **No behavioral edge emission**: Group 6 (S6/S7 Informs edges at cycle close) is a separate
  feature, conditional on crt-043.
- **No goal-conditioned briefing**: Group 7 briefing changes are not in scope.
- **No goal_cluster table**: The `goal_clusters` schema (Group 6 item 2) is out of scope.
- **No observations.feature_id column**: `topic_signal` already is the feature ID.
  No new attribution column needed on `observations`.
- **No audit_log changes**: `audit_log` is not touched in this feature.
- **No backfill**: Rows written before v21 get NULL `phase`. Pre-crt-043 `cycle_events` rows
  get no `goal_embedding`. Cold-start degradation is acceptable.
- **No external fetch for goal text**: The goal text comes exclusively from the `goal` parameter
  passed to `context_cycle(type=start)`. No GitHub API, no `gh` CLI, no HTTP client of any kind.
  No new crate dependencies for Item B.
- **No hard failure on embed unavailability**: If the embed service is not ready, goal embedding
  is skipped with a `tracing::warn!`. Cycle start is not blocked.
- **No agent_role changes**: agent_role instrumentation is a separate issue; not in scope here.
- **No change to `context_cycle` MCP tool response format**: Response text is unchanged.

## Background Research

### topic_signal IS the feature ID (why Item A was dropped)

`topic_signal TEXT` was added to `observations` in col-017. col-024 added `enrich_topic_signal`,
a module-private helper in `listener.rs` that fills `topic_signal` from the session registry
when text pattern extraction returns `None`:

```rust
fn enrich_topic_signal(
    extracted: Option<String>,
    session_id: &str,
    session_registry: &SessionRegistry,
) -> Option<String> {
    if extracted.is_some() { return extracted; }
    session_registry.get_state(session_id).and_then(|s| s.feature)
}
```

This runs at all four write sites: RecordEvent path, rework candidate path, RecordEvents batch
path, ContextSearch path. S6/S7 signal queries scope to a feature via `WHERE topic_signal = ?`.

### Item B — Goal embedding

- `cycle_events` schema (v16): columns include `goal TEXT`. No `goal_embedding` column.
- `insert_cycle_event()` (`db.rs:308`): does not accept an embedding.
- `context_cycle` MCP handler (`tools.rs:2127`): for `CycleType::Start`, fires the UDS hook
  path (`handle_cycle_event`) fire-and-forget. The MCP handler has the `goal` text in hand
  before the fire-and-forget call.
- **Goal text source**: the `goal` parameter on the MCP tool call. Already stored on
  `cycle_events` by `handle_cycle_event`. No fetch needed.
- **Embedding pipeline**: `EmbedServiceHandle::get_adapter()` → `adapter.embed_entry("", text)`
  → `Vec<f32>`. Rayon pool (`ml_inference_pool`) required for CPU embedding.
- **Trigger**: after the fire-and-forget UDS call, the MCP handler `tokio::spawn`s a task that
  embeds the goal and calls a new `update_cycle_start_goal_embedding(cycle_id, bytes)` store
  method to UPDATE the `cycle_events` row. The UDS hook path is unchanged.
- **Serialization**: open question (see below).

### Item C — Phase capture on observations

- `observations` schema (v10): columns include `topic_signal TEXT`. No `phase` column.
- `SessionState.current_phase: Option<String>` exists (crt-025, `session.rs:129`).
  Set via `set_current_phase()`, called synchronously before any `spawn_blocking` DB write.
  `get_state(session_id)?.current_phase` is available at all observation write sites — same
  O(1) Mutex read used by `enrich_topic_signal`.
- Write sites (all `listener.rs`): `insert_observation()`, `insert_observations_batch()`,
  plus the RecordEvent and rework candidate paths.  `feature_id` and `phase` are captured
  from session state in the same pre-`spawn_blocking` lookup.
- Phase values: free-text string as stored by `context_cycle` events (e.g., "scope", "design",
  "delivery"). NULL when no active cycle or phase not yet set. No allowlist enforcement.
- Index consideration: a composite index on `(topic_signal, phase)` may accelerate Group 6
  signal queries. Delivery agent to evaluate alongside the migration.

### Schema and migration conventions

- `ALTER TABLE ADD COLUMN` with `pragma_table_info` pre-check for idempotency (established v15/v16).
- `CURRENT_SCHEMA_VERSION = 20`. One combined migration step → v21.
- Two ADD COLUMN statements in one migration: `observations.phase` + `cycle_events.goal_embedding`.

## Proposed Approach

### Item B — Goal embedding at cycle start

1. Add `goal_embedding BLOB` to `cycle_events` (schema v21 migration).
2. In the `context_cycle` MCP handler for `CycleType::Start`: after the existing fire-and-forget
   UDS call, if `goal` text is non-empty, `tokio::spawn` a task that:
   a. Embeds the goal text via the rayon pool using `EmbedServiceHandle`.
   b. Calls new store method `update_cycle_start_goal_embedding(cycle_id, embedding_bytes)` to
      UPDATE the `cycle_events` start row with the blob.
3. The `context_cycle` tool response is unchanged. The embedding write is fire-and-forget.
4. The UDS hook path (`handle_cycle_event`) continues unchanged — `insert_cycle_event` signature
   does not change (the embedding is written via a subsequent UPDATE, not the INSERT).
5. If the embed service is unavailable or `goal` is None/empty, skip with `tracing::warn!`.
   Cycle start is never blocked.

### Item C — Phase on observations

1. Add `phase TEXT` column to `observations` (schema v21, same migration as Item B).
2. Add `phase: Option<String>` to `ObservationRow` struct in `listener.rs`.
3. Update `insert_observation()` and `insert_observations_batch()` to bind `phase`.
4. At each observation write site, capture `current_phase` from session state before
   `spawn_blocking` — same pre-capture pattern as `topic_signal` enrichment.
5. Evaluate adding a composite index on `(topic_signal, phase)` for Group 6 queries.

## Acceptance Criteria

- AC-01: `cycle_events` table has a `goal_embedding BLOB` column after schema migration (v21).
- AC-02: At `context_cycle type=start`, when `goal` text is non-empty, a background task embeds
  the goal text via the rayon pool and stores the result.
- AC-03: On successful embed, `goal_embedding` is written to the `cycle_events` start row via
  `update_cycle_start_goal_embedding()`.
- AC-04: If the embed service is not ready, or `goal` is absent/empty, the goal embedding step
  is skipped with `tracing::warn!`. Cycle start is not blocked.
- AC-05: No GitHub API call, no `gh` CLI, no HTTP client, no new crate dependencies introduced.
- AC-06: `context_cycle` MCP tool response text is unchanged from pre-crt-043 behavior.
- AC-07: `observations` table has a `phase TEXT` column after schema migration (v21).
- AC-08: `ObservationRow` has `phase: Option<String>`.
- AC-09: `insert_observation()` and `insert_observations_batch()` bind `phase`.
- AC-10: At observation write sites, `phase` is captured from `SessionState.current_phase`
  before `spawn_blocking`; NULL when no active cycle or phase not set.
- AC-11: Schema migration is idempotent: running migrate on a database already at v21 is a
  no-op (`pragma_table_info` pre-checks on all ADD COLUMN statements).
- AC-12: All existing tests pass. New unit tests cover: goal embedding fire-and-forget spawn
  (success + skip-on-no-goal + skip-on-no-embed), observation with phase populated, observation
  with NULL phase (no active cycle).

## Constraints

- **No external dependencies**: existing `EmbedServiceHandle` pipeline only. No new crates.
- **Embedding is async/rayon**: goal embedding MUST go through `ml_inference_pool` to avoid
  blocking the tokio executor.
- **Fire-and-forget**: `context_cycle` MCP response must not wait for embedding. Must be
  inside `tokio::spawn`.
- **UDS budget**: goal embedding cannot happen in `handle_cycle_event` (50ms hook budget).
  It happens in the MCP handler where there is no latency constraint on the spawned task.
- **INSERT vs UPDATE for embedding**: `insert_cycle_event` is called from the UDS spawn and
  cannot be modified to accept an embedding (different call path, different timing). The
  embedding write is a separate UPDATE from the MCP handler spawn.
- **Phase capture timing**: `current_phase` must be read from session state before entering
  `spawn_blocking`, passed by value into the closure — same pattern as `topic_signal`.

## Serialization Format Decision (Resolved)

**`goal_embedding BLOB` uses `bincode::serde::encode_to_vec(vec: Vec<f32>, config::standard())`.**

This is the first SQLite embedding blob in the codebase. The architect must document this as
an ADR — not just for crt-043, but because Group 6 will need more embedding blobs (goal_cluster
embeddings) and whatever is decided here becomes the codebase pattern.

Rationale (human decision, to be captured in ADR):
1. **Self-describing** — bincode length-prefixes `Vec<f32>`, so read sites do not need to know
   the dimension out-of-band. Raw bytes require every read site to hard-code 384 dimensions.
2. **Already in workspace** — no new dependency.
3. **Model upgrade path** — if dimension changes (384 → 768), bincode blobs are distinguishable
   by length. Raw bytes would silently deserialize into the wrong number of floats.
4. **Precedent value** — Group 6 needs a `goal_cluster` table with embeddings. This ADR gives
   them a pattern to cite rather than making a second independent decision.

Note: the VECTOR_MAP does NOT store embedding bytes — it stores only `entry_id → hnsw_data_id`
(two integers). Primary entry embeddings live in the HNSW in-memory index and on-disk binary
files. There is no existing SQLite embedding blob pattern to match; this ADR creates it.

**Primary embedding follow-up (out of scope for crt-043, ADR should note it):**
If primary entry embeddings were also stored as blobs in SQLite (keyed by `entry_id`),
`get_embedding()` becomes O(1) instead of the current O(N) HNSW scan — directly addressing
SR-01 in crt-042 (PPR expander latency). Storage cost is trivial (~7,000 × 1,536 bytes ≈ 10MB).
HNSW stays authoritative for ANN search; the SQLite blob is a parallel random-access path.
Evaluate alongside crt-042 delivery. The ADR should note this scope explicitly so it is not
written too narrowly.

**Rayon pattern:** Yes. Goal embedding goes through `ml_inference_pool.spawn_with_timeout`,
same as all existing embedding calls. Already noted in Constraints.

## Architecture Constraint: INSERT/UPDATE Race (Architect Must Resolve)

The scope proposes writing `goal_embedding` via a separate UPDATE from the MCP handler after
the INSERT fires through the UDS path. This has a race the architect must explicitly address:

- The MCP handler fires the UDS cycle event fire-and-forget (which spawns `insert_cycle_event`
  in the UDS listener).
- The MCP handler then `tokio::spawn`s the embedding task.
- These two spawns are unordered. The UPDATE (needing the row to exist) may execute before the
  INSERT completes.

`cycle_id` (the `topic` string, e.g., "crt-043") IS available in the MCP handler — no ID
generation race. The race is purely INSERT-before-UPDATE ordering.

The architect must choose one of:
1. **Fire embedding from UDS listener** — after `insert_cycle_event` completes, spawn the
   embedding task from within `handle_cycle_event`. INSERT is guaranteed to precede UPDATE.
   Requires the UDS listener to have access to the embed service handle (verify).
2. **Inline embedding before UDS dispatch** — embed on the rayon pool in the MCP handler
   (awaited), then pass the embedding bytes in the UDS message so `insert_cycle_event` writes
   both in the INSERT. No UPDATE needed. Changes the UDS message schema and `insert_cycle_event`
   signature, but eliminates the race entirely.
3. **Fire from MCP handler with retry** — the spawn polls or retries the UPDATE until the row
   exists. More complexity in the embed task; no changes to UDS path.

Options 1 and 2 eliminate the race. Option 2 is the cleanest if latency of embedding before
returning the MCP response is acceptable (fire-and-forget guarantee is lost — the MCP handler
awaits the embed). Option 1 is cleaner if the embed service is accessible in the UDS listener.
This is not a delivery-time question.

## Tracking

https://github.com/dug-21/unimatrix/issues/494
