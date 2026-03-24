# col-024 Pseudocode Overview
# Cycle-Events-First Observation Lookup and Topic Signal Enrichment

## Components and Why They Are Affected

| Component | File | Reason |
|-----------|------|--------|
| ObservationSource trait | `unimatrix-observe/src/source.rs` | New method `load_cycle_observations` must be declared on the trait so callers can dispatch via `&dyn ObservationSource` |
| SqlObservationSource impl + helper | `unimatrix-server/src/services/observation.rs` | Implements the new trait method; three-step SQL algorithm; unit-conversion helper |
| enrich_topic_signal helper | `unimatrix-server/src/uds/listener.rs` | Centralised fallback for all four write sites; prevents per-site drift |
| context_cycle_review lookup | `unimatrix-server/src/mcp/tools.rs` | Restructures two-path to three-path with structured fallback log |

## Data Flow

```
WRITE PATH (listener.rs)
========================
ImplantEvent arrives at UDS handler
    |
    v
extract_observation_fields(&event)  -->  ObservationRow (raw, topic_signal may be None)
    |
    v
enrich_topic_signal(obs.topic_signal, event.session_id, session_registry)
    |  if None: reads session_registry.get_state(sid).feature
    |  if Some(x) and x != registry.feature: tracing::debug!(both values)
    |  if Some(x): returns x unchanged
    v
obs.topic_signal = enriched_value          -- may still be None if registry has no feature
    |
    v  (spawn_blocking fire-and-forget)
insert_observation / insert_observations_batch
    |
    v
observations table  (topic_signal column written)


READ PATH (tools.rs -> services/observation.rs)
===============================================
context_cycle_review handler
    |
    v  (spawn_blocking_with_timeout)
Step 1: load_cycle_observations(cycle_id)     [primary: cycle_events-based]
    |  Step 0: COUNT(*) cycle_events WHERE cycle_id = ?1
    |    -- if 0: return Ok(vec![])  ["no rows" case]
    |  Step 1: SELECT event_type, timestamp FROM cycle_events ORDER BY timestamp ASC, seq ASC
    |    -- pair cycle_start/cycle_stop into (start_ms, stop_ms) windows
    |    -- open-ended start: stop_ms = cycle_ts_to_obs_millis(unix_now_secs())
    |  Step 2: per-window DISTINCT session_id WHERE topic_signal = cycle_id AND ts BETWEEN
    |    -- union all windows, deduplicate session IDs
    |  Step 3: 7-col SELECT WHERE session_id IN (...) AND ts_millis BETWEEN min AND max
    |    -- Rust-filter per record: retain only records in at least one window
    |    -- parse via parse_observation_rows (security bounds apply)
    v
Vec<ObservationRecord>
    |  non-empty --> detection pipeline
    |  empty (either case) --> tracing::debug! "primary path empty"
    v
Step 2: load_feature_observations(cycle_id)   [legacy: sessions.feature_cycle]
    |  non-empty --> detection pipeline
    |  empty --> tracing::debug! "legacy sessions path empty"
    v
Step 3: load_unattributed_sessions() + attribute_sessions(...)  [content-based]
    --> result (possibly empty) --> check cached MetricVector if empty
```

## Shared Types (No New Types Introduced)

All types are existing. The only structural change is a new field in the
`ObservationSource` trait method list, and a new private function in two files.

| Type | Crate | Role |
|------|-------|------|
| `ObservationRecord` | `unimatrix-observe` | Output of all load methods; unchanged |
| `ParsedSession` | `unimatrix-observe` | Output of `load_unattributed_sessions`; unchanged |
| `ObservationRow` | `unimatrix-server/uds/listener.rs` | Internal write struct; `topic_signal` field patched after enrichment |
| `SessionState` | `unimatrix-server/infra/session.rs` | `feature: Option<String>` read by `enrich_topic_signal` |
| `SessionRegistry` | `unimatrix-server/infra/session.rs` | `get_state(sid) -> Option<SessionState>`; read-only in enrichment |
| `ObserveError::Database(String)` | `unimatrix-observe` | Error propagated from all SQL failures |

## Time Window Type (Logical — No New Struct)

Windows are represented as `Vec<(i64, i64)>` where each tuple is `(start_ms, stop_ms)`
in Unix epoch milliseconds. Constructed inline inside `load_cycle_observations`.

## Sequencing Constraints (Build Order)

1. **observation-source-trait** must be implemented first — it defines the interface
   that `load-cycle-observations` implements and `context-cycle-review` calls.
2. **load-cycle-observations** + `cycle_ts_to_obs_millis` can be developed in parallel
   with **enrich-topic-signal** — they share no direct dependencies.
3. **context-cycle-review** depends on `load_cycle_observations` being available on the
   trait; implement after step 1 and 2 compile.

## Key Invariants

- `cycle_ts_to_obs_millis` is the ONLY site that multiplies seconds by 1000. No raw
  `* 1000` literal appears anywhere in window-boundary construction (ADR-002).
- All three SQL steps in `load_cycle_observations` run inside ONE `block_sync` call.
  No nested `block_sync` or `block_in_place` (ADR-001).
- `enrich_topic_signal` returns `extracted` unchanged when `Some(_)` — explicit wins
  always (ADR-004, FR-14).
- Legacy fallback activates ONLY on `Ok(vec![])`, never on `Err(...)` (FM-01).
- `unimatrix-observe` crate does NOT import `tracing`. The trait method declaration
  must not add tracing-dependent code to that crate.
