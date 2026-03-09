# col-018: Pseudocode Overview

## Components

| Component | Description |
|-----------|-------------|
| context-search-observation | Observation write side-effect in the ContextSearch dispatch arm |

## Data Flow

```
ContextSearch dispatch arm (listener.rs)
  |
  +-- extract topic_signal = unimatrix_observe::extract_topic_signal(&query)
  |
  +-- if session_id.is_some() && !query.is_empty():
  |     |
  |     +-- session_registry.record_topic_signal(sid, signal, timestamp)
  |     |
  |     +-- build ObservationRow {
  |     |     session_id, ts_millis, hook="UserPromptSubmit",
  |     |     tool=None, input=truncated_query, response_size=None,
  |     |     response_snippet=None, topic_signal
  |     |   }
  |     |
  |     +-- spawn_blocking_fire_and_forget: insert_observation(store, obs)
  |
  +-- handle_context_search(...).await  (unchanged, always runs)
```

## Shared Types

No new types introduced. Reuses existing:
- `ObservationRow` (listener.rs:1797)
- `insert_observation()` (listener.rs:1866)
- `spawn_blocking_fire_and_forget()` (listener.rs:73)

## Sequencing

Single component -- no sequencing constraints. The observation write is a side-effect injected before the existing `handle_context_search()` call.

## Integration Harness

Existing integration suites (`smoke`, `tools`) cover ContextSearch response behavior. No new integration tests needed -- the observation write is an internal side-effect not visible through the MCP protocol. Verification is via unit tests querying the SQLite observations table directly.
