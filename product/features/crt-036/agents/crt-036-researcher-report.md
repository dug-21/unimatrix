# crt-036 Researcher Report

## Summary

Explored the full codebase to understand current retention state across all activity/observation tables and produced SCOPE.md for the Intelligence-Driven Retention Framework.

## Key Findings

### Current State — What Exists Today

**observations** (152 MB, 83% of DB):
- Single 60-day hard DELETE in `run_maintenance()` step 4, `status.rs` line 1380:
  `DELETE FROM observations WHERE ts_millis < ?1`
- Redundant path in `tools.rs` line 1638 (same DELETE, FR-07 label)
- No feature_cycle column. Cycle resolution requires join through `sessions`
- Schema: `(id, session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal)`
- Indexes: `idx_observations_session`, `idx_observations_ts`

**query_log**:
- No retention policy at all
- No feature_cycle column — must join through `sessions`
- Schema: `(query_id, session_id, query_text, ts, result_count, result_entry_ids, similarity_scores, retrieval_mode, source, phase)`
- Indexes: `idx_query_log_session`, `idx_query_log_ts`, `idx_query_log_phase`
- ADR-002 col-031 (entry #3686) explicitly deferred cycle-aligned GC to GH #409

**sessions / injection_log**:
- `gc_sessions()` in `sessions.rs` line 294: 30-day time-based cascade delete
- `sessions` has `feature_cycle TEXT` column — the only table with direct cycle linkage
- `injection_log` cascades via `session_id` (injection_log → sessions)
- `gc_sessions()` runs at step 6 of `run_maintenance()`

**audit_log**:
- No existing retention policy
- Schema: `(event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail)`
- Indexes: `idx_audit_log_agent`, `idx_audit_log_timestamp` — timestamp index exists for GC query

### cycle_review_index Gate

- Schema: `(feature_cycle PK, schema_version, computed_at, raw_signals_available, summary_json)`
- `raw_signals_available` field (i32, not bool) explicitly exists for the post-GC signal purge flag
- `get_cycle_review()` returns `Ok(None)` for absent cycles — the gate check
- `store_cycle_review()` uses INSERT OR REPLACE — can overwrite to set `raw_signals_available = 0`

### cycle_events Table

- Schema: `(id, cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal)`
- `event_type` column holds hook type strings: `cycle_start`, `cycle_stop`, etc.
- K-cycle resolution query via `cycle_review_index ORDER BY computed_at DESC LIMIT K`

### Config Shape

`UnimatrixConfig` has 8 sections using `#[serde(default)]` pattern. New `[retention]` block
adds a 9th section (`RetentionConfig`) with two fields: `activity_detail_retention_cycles`
(default 50) and `audit_log_retention_days` (default 180).

### Maintenance Tick Integration (entry #3911)

- `run_maintenance()` signature already receives `&InferenceConfig`
- New GC is a prune-style pass — independent of prune/heal/compact ordering constraint
- Step 4 (60-day DELETE) is replaced; audit_log DELETE becomes step 4b
- Config field must have `validate()` range check

## Files Read

- `crates/unimatrix-server/src/services/status.rs` (lines 976–1459) — `run_maintenance()` body
- `crates/unimatrix-store/src/sessions.rs` — `gc_sessions()` pattern, table schema, constants
- `crates/unimatrix-store/src/cycle_review_index.rs` — full file, gate mechanism
- `crates/unimatrix-store/src/observations.rs` — table schema, `load_sessions_for_feature()`
- `crates/unimatrix-store/src/query_log.rs` — table schema, no retention logic
- `crates/unimatrix-store/src/audit.rs` — table schema, no retention logic
- `crates/unimatrix-store/src/db.rs` (lines 290–560, 661–810) — DDL for all tables
- `crates/unimatrix-store/src/migration.rs` (lines 1–100, 460–560) — schema version 19
- `crates/unimatrix-store/src/injection_log.rs` — cascade schema
- `crates/unimatrix-store/src/schema.rs` — EntryRecord, Status enum
- `crates/unimatrix-server/src/infra/config.rs` — UnimatrixConfig, InferenceConfig shape

## Unimatrix Knowledge Consulted

- Entry #3686: ADR-002 col-031 — time-based query_log window, deferred cycle GC to #409
- Entry #3793: ADR-001 crt-033 — synchronous write for cycle_review_index
- Entry #3802: ADR-004 crt-033 — K-window scoping via cycle_events
- Entry #3911: Procedure — how to add a new maintenance tick pass

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- found entries #3686, #3793, #3802, #3911 relevant
- Stored: entry #3914 "Cycle-based GC for observations/query_log requires two-hop join through sessions" via /uni-store-pattern

## Output

SCOPE.md: `product/features/crt-036/SCOPE.md`
