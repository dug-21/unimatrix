# Pseudocode Overview: col-012 Data Path Unification

## Component Interaction

```
Hook Scripts (bash)
  [JSONL writes REMOVED]
  Only forward to UDS socket
       |
       v
UDS Listener (event-persistence)
  RecordEvent -> extract fields -> INSERT observations
  RecordEvents -> batch INSERT in single txn
       |
       v
SQLite (schema-migration created observations table at v7)
  observations table + sessions table
       |
       v
SqlObservationSource (sql-implementation)
  implements ObservationSource trait
  queries sessions -> observations JOIN
       |
       v
ObservationSource trait (observation-source)
  defined in unimatrix-observe
  3 methods: load_feature_observations, discover_sessions_for_feature, observation_stats
       |
       v
Retrospective Pipeline (retrospective-migration)
  context_retrospective uses SqlObservationSource
  context_status uses observation_stats()
  Retention: DELETE WHERE ts_millis < 60 days ago
       |
       v
Detection Rules (UNCHANGED)
  Still receive Vec<ObservationRecord>
```

## Data Flow

1. Hook fires -> shell script reads stdin -> forwards to `unimatrix-server hook` via UDS
2. UDS listener receives HookRequest::RecordEvent with ImplantEvent
3. Handler extracts fields from ImplantEvent.payload, maps to observations columns
4. spawn_blocking INSERT (fire-and-forget, ADR-003)
5. Later: context_retrospective called with feature_cycle
6. SqlObservationSource queries SESSIONS for matching session_ids
7. Queries observations WHERE session_id IN (...), maps rows to ObservationRecord
8. Detection rules, metrics, baseline, report -- all unchanged

## Shared Types

- `ObservationRecord` (existing, unchanged) - unimatrix-observe::types
- `HookType` enum (existing, unchanged) - unimatrix-observe::types
- `ObservationStats` (REVISED) - unimatrix-observe::types
  - `file_count` -> `record_count`
  - `total_size_bytes` REMOVED
  - `oldest_file_age_days` -> `oldest_record_age_days`
  - `approaching_cleanup` stays (Vec<String> of session_ids)
- `ObservationSource` trait (NEW) - unimatrix-observe::source
- `SqlObservationSource` struct (NEW) - unimatrix-server::services::observation

## Patterns Used

- **spawn_blocking fire-and-forget**: Same pattern as injection_log writes in UDS listener
- **Store::lock_conn()**: Direct SQL via rusqlite, same as StatusService
- **Trait in observe, impl in server**: Dependency inversion (ADR-002), same pattern as how server provides Store to observe functions
- **Schema migration step**: Same idempotent pattern as v5->v6 (CREATE TABLE IF NOT EXISTS)
- **StatusReport field update**: Same as existing observation stats fields

## Component List

| Component | Crate(s) | Wave |
|-----------|----------|------|
| schema-migration | unimatrix-store | 1 |
| event-persistence | unimatrix-server | 1 |
| observation-source | unimatrix-observe | 2 |
| sql-implementation | unimatrix-server | 2 |
| retrospective-migration | unimatrix-server | 3 |
| jsonl-removal | unimatrix-observe, hooks | 4 |
