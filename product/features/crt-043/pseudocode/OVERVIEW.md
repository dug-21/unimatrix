# crt-043: Behavioral Signal Infrastructure — Pseudocode Overview

## Components Involved

| Component | Files Modified/Created | Why |
|-----------|----------------------|-----|
| schema-migration | `unimatrix-store/src/migration.rs`, `unimatrix-store/src/db.rs`, `unimatrix-store/src/embedding.rs` (new) | Add v20→v21 migration block, update store method, provide encode/decode helpers |
| goal-embedding | `unimatrix-server/src/uds/listener.rs` | Extend `handle_cycle_event` to spawn embedding task after INSERT spawn |
| phase-capture | `unimatrix-server/src/uds/listener.rs` | Add `phase` to `ObservationRow`; capture pre-spawn at all four write sites |

Components 2 (goal-embedding) and 3 (phase-capture) both modify `listener.rs`. Stage 3b must handle them in a single agent pass to avoid merge conflicts.

---

## WARN-2 Resolution: `encode_goal_embedding` / `decode_goal_embedding` Visibility

Both helpers are `pub` (not `pub(crate)`) and re-exported from `unimatrix-store/src/lib.rs`.

Rationale: `encode_goal_embedding` must be callable from `unimatrix-server/src/uds/listener.rs` (cross-crate call from the goal-embedding spawn). `pub(crate)` is inaccessible across crate boundaries. Both helpers are promoted to `pub` together for symmetry.

Group 6 context: Group 6 will consume decoded embeddings through a future store query method (e.g., `get_cycle_start_embedding`) that decodes internally. However, `decode_goal_embedding` is still promoted to `pub` now to avoid a breaking change when Group 6 ships, and because `encode_goal_embedding` requires `pub` regardless.

---

## FR-C-07 Resolution: Composite Index on (topic_signal, phase)

Decision: **Add the composite index in the v21 migration.**

Justification: Group 6 S6/S7 signal queries will filter `observations` by both `topic_signal` (feature attribution) and `phase` (phase stratification). Without an index, these queries become full-table scans. The `observations` table grows continuously during active development (every hook event writes a row). At the current rate, the table will have tens of thousands of rows within a few weeks of Group 6 deployment. Adding the index now costs ~microseconds at migration time and prevents a latency regression at Group 6 ship time.

Index definition: `CREATE INDEX IF NOT EXISTS idx_observations_topic_phase ON observations (topic_signal, phase)`.

Column order: `topic_signal` first (higher cardinality, often used as the primary filter by feature attribution), then `phase`. This order matches the expected query pattern: `WHERE topic_signal = ? AND phase = ?`.

---

## Edge Case Resolution: `goal = " "` (Whitespace-Only)

Decision: **Treat whitespace-only goal as absent — trim before check, no spawn if trimmed result is empty.**

Rationale: A whitespace-only goal produces a zero-information embedding (the model embeds the empty or near-empty token sequence). Storing such a blob wastes space and may introduce noise in H1 goal-clustering. Consistent with FR-B-09 ("empty string" is absent). The trim is applied to the in-memory `goal_for_event` copy before the `if let Some(goal_text) = ...` check in Step 6. The `insert_cycle_event` call in Step 5 still receives the unmodified goal value (preserving verbatim UDS payload storage per col-025 ADR-005 FR-11).

---

## Shared Types Introduced or Modified

### In `unimatrix-store/src/embedding.rs` (new file)

```
pub fn encode_goal_embedding(vec: Vec<f32>) -> Result<Vec<u8>, bincode::error::EncodeError>
pub fn decode_goal_embedding(bytes: &[u8]) -> Result<Vec<f32>, bincode::error::DecodeError>
```

These are the canonical SQLite embedding blob helpers for the codebase (ADR-001). Every future embedding BLOB column must have analogous paired helpers in the same PR as the write path.

### In `unimatrix-store/src/db.rs` (new method on `SqlxStore`)

```
pub async fn update_cycle_start_goal_embedding(
    &self,
    cycle_id: &str,
    embedding_bytes: Vec<u8>,
) -> Result<()>
```

### In `unimatrix-server/src/uds/listener.rs` (struct modification)

```
struct ObservationRow {
    // existing fields unchanged ...
    topic_signal: Option<String>,  // existing
    phase: Option<String>,          // NEW — crt-043
}
```

---

## Data Flow

### Item B — Goal Embedding

```
context_cycle MCP tool call (type=start, goal="...")
  → MCP handler (unchanged interface)
  → UDS hook fires RecordEvent with CYCLE_START_EVENT
  → dispatch_request receives RecordEvent
  → handle_cycle_event(event, Start, registry, store, embed_service)  [synchronous]
      synchronous section:
        → set_feature_force (Step 2)
        → set_current_phase (Step 3)
        → set_current_goal (Step 3b) with goal_for_event = Some(trimmed_goal)
      fire-and-forget section:
        → tokio::spawn: insert_cycle_event(goal=goal_for_db)    [Step 5 — INSERT]
        → tokio::spawn: embed_goal_task(goal_text, cycle_id)    [Step 6 — UPDATE]
            → embed_service.get_adapter()
            → on EmbedNotReady: warn!, return
            → adapter.embed_entry("", goal_text)  [rayon ml_inference_pool]
            → on Err: warn!, return
            → encode_goal_embedding(vec)
            → on EncodeError: warn!, return
            → store.update_cycle_start_goal_embedding(cycle_id, bytes)
            → on Err: warn!(cycle_id), return
```

Key invariant: Step 5 spawn is registered in tokio queue before Step 6 spawn. The rayon embedding task provides natural delay (tens of ms) before the UPDATE executes, making INSERT-before-UPDATE the overwhelmingly likely ordering. Residual race is accepted as NULL degradation (ADR-002).

### Item C — Phase Capture

```
Hook event (RecordEvent, RecordEvents, ContextSearch, post_tool_use_rework_candidate)
  → dispatch_request
  → extract_observation_fields(event)          [no session_registry access]
  → enrich_topic_signal(...)                   [existing pattern]
  → phase = session_registry
                .get_state(&session_id)
                .and_then(|s| s.current_phase.clone())   [pre-spawn capture]
  → obs.phase = phase
  → spawn_blocking / spawn_blocking_fire_and_forget:
      insert_observation(store, obs)           [obs.phase moved into closure]
      or insert_observations_batch(store, batch)
```

Key invariant: `phase` is captured from session registry before entering any spawn context. The captured `Option<String>` is moved into the closure by value, not captured by reference. This is identical to the `topic_signal` enrichment pattern (col-024 ADR-004, entry #3374).

---

## Sequencing Constraints

1. `schema-migration` must be implemented first: `embedding.rs` helpers are needed by `goal-embedding`; the v21 migration columns are needed by all tests.
2. `goal-embedding` and `phase-capture` can be implemented in parallel within the same agent pass (they modify disjoint regions of `listener.rs`, but the file is large and the two sets of changes must be coordinated).
3. Integration tests for both items require a real v20 fixture database (FR-M-04, entry #378 lesson).

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — attempted; Unimatrix MCP server was disconnected at agent spawn time. Fell back to reading ADR files directly from product/features/crt-043/architecture/. ADR entry IDs #4067, #4068, #4069 referenced by number from IMPLEMENTATION-BRIEF.md.
- Deviations from established patterns: none. The `pragma_table_info` pre-check pattern (entry #1264), the `enrich_topic_signal` pre-capture pattern (entry #3374), and the fire-and-forget spawn pattern (entry #735) are all followed as specified.
