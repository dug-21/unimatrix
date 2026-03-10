# col-020: Multi-Session Retrospective — Architecture

## System Overview

col-020 extends the existing retrospective pipeline to decompose per-session activity profiles, measure cross-session knowledge reuse, count rework sessions, compute context reload rates, and update topic_deliveries aggregate counters. It builds on top of the data infrastructure from col-017 (topic attribution) and nxs-010 (topic_deliveries + query_log tables).

The feature is additive: all existing retrospective behavior (hotspot detection, metric computation, baseline comparison, entry analysis, narratives, recommendations, lesson-learned write) continues unchanged. Four new optional fields are appended to `RetrospectiveReport`. The `context_retrospective` tool handler gains five additional computation steps after the existing pipeline.

### Position in the System

```
ObservationSource (unimatrix-observe)       Store (unimatrix-store)
       |                                         |
       | load_feature_observations()              | scan_query_log_by_sessions()
       | discover_sessions_for_feature()          | scan_injection_log_by_sessions()
       v                                         | scan_sessions_by_feature()
+------------------+                             | count_active_entries_by_category()
| unimatrix-observe|                             |
| session_metrics  |<-- ObservationRecord[] --+  |
| (NEW module)     |                          |  |
+------------------+                          |  |
       |                                      |  |
       | SessionSummary[]                     |  |
       | context_reload_pct                   |  |
       v                                      |  |
+----------------------------------------------+ |
| context_retrospective handler                 | |
| (unimatrix-server/src/mcp/tools.rs)          | |
|                                               | |
| Existing pipeline:                            | |
|   load obs -> detect hotspots -> metrics ->   | |
|   store metrics -> baselines -> entries ->    | |
|   narratives -> recommendations -> lesson     | |
|                                               | |
| NEW steps (after step 10e, before audit):     | |
|   11. compute_session_summaries()      [observe]
|   12. compute_context_reload_pct()     [observe]
|   13. compute_knowledge_reuse()        [server]
|   14. count rework sessions            [server]
|   15. set_topic_delivery_counters()    [store] |
|   16. attach all to report                    | |
+-----------------------------------------------+
```

## Component Breakdown

### C1: Session Metrics Module (`unimatrix-observe/src/session_metrics.rs`)

**Responsibility**: Compute per-session activity profiles and cross-session context reload rate from ObservationRecord arrays. Pure computation — no database access.

**Contains**:
- `compute_session_summaries(records: &[ObservationRecord]) -> Vec<SessionSummary>`
- `compute_context_reload_pct(summaries: &[SessionSummary], records: &[ObservationRecord]) -> f64`
- `extract_file_path(tool: &str, input: &serde_json::Value) -> Option<String>` (internal helper)
- `classify_tool(tool: &str) -> &'static str` (internal helper)

**Rationale**: Session summaries derive entirely from ObservationRecord data. Keeping computation in unimatrix-observe follows the existing pattern where all retrospective computation on observation data lives in this crate (ADR-002 col-012, Unimatrix #383).

### C2: New Types (`unimatrix-observe/src/types.rs`)

**Responsibility**: Define `SessionSummary` and `KnowledgeReuse` structs, extend `RetrospectiveReport` with four new optional fields.

**New structs**:
- `SessionSummary` — per-session activity profile
- `KnowledgeReuse` — cross-session knowledge flow measurement

**Extended struct**:
- `RetrospectiveReport` — four new `Option` fields with `skip_serializing_if`

### C3: Knowledge Reuse Computation (inline in `context_retrospective` handler)

**Responsibility**: Compute Tier 1 cross-session knowledge reuse by joining query_log, injection_log, and entries data. Lives in the server handler, not in unimatrix-observe.

**Rationale**: This computation requires cross-table joins against query_log, injection_log, and entries — all accessed through Store. Extending ObservationSource with query_log and injection_log methods would bloat the trait for a single consumer and violate the trait's purpose (observation data abstraction). The server handler already has Store access. See ADR-001 for full rationale.

### C4: Store API Extensions (`unimatrix-store`)

**Responsibility**: Provide batch query methods for multi-session data loading and category counting.

**New methods on Store**:
- `scan_query_log_by_sessions(session_ids: &[&str]) -> Vec<QueryLogRecord>`
- `scan_injection_log_by_sessions(session_ids: &[&str]) -> Vec<InjectionLogRecord>`
- `count_active_entries_by_category() -> HashMap<String, u64>`
- `set_topic_delivery_counters(topic, sessions, tool_calls, duration_secs) -> Result<()>`

### C5: Report Builder Extension (`unimatrix-observe/src/report.rs`)

**Responsibility**: Accept new session-level data in `build_report()` signature and attach to the report.

**Change**: The `build_report()` function gains additional parameters for the new fields. Alternatively, since the handler already mutates the report after `build_report()` returns (for narratives and recommendations), the new fields can be assigned directly on the returned `RetrospectiveReport`. The latter approach is simpler and consistent with the existing pattern (see how `narratives` and `recommendations` are set at lines 1216-1219 of tools.rs).

**Decision**: Follow the existing post-build mutation pattern. Do NOT change `build_report()` signature. Assign new fields directly on the returned report in the handler.

### C6: Handler Integration (`unimatrix-server/src/mcp/tools.rs`)

**Responsibility**: Wire the new computation steps into `context_retrospective` after the existing pipeline. Orchestrate data loading, computation, and report assembly.

## Component Interactions

### Data Flow

1. Handler loads observations via ObservationSource (existing, unchanged)
2. Handler runs existing pipeline: detect hotspots, compute metrics, store, baselines, entries analysis, narratives, recommendations (unchanged)
3. **NEW** Handler calls `compute_session_summaries(&attributed)` (C1) -> `Vec<SessionSummary>`
4. **NEW** Handler calls `compute_context_reload_pct(&summaries, &attributed)` (C1) -> `f64`
5. **NEW** Handler loads session records via `store.scan_sessions_by_feature(&feature_cycle)` (existing API) -> `Vec<SessionRecord>`
6. **NEW** Handler counts rework sessions from SessionRecord outcomes
7. **NEW** Handler loads query_log + injection_log for the topic's sessions via new batch Store methods (C4)
8. **NEW** Handler computes knowledge reuse in-line (C3) -> `KnowledgeReuse`
9. **NEW** Handler calls `store.set_topic_delivery_counters(...)` (C4) for idempotent counter update
10. **NEW** Handler assigns all new fields to the report before serialization

### Error Propagation

All new computation steps are fallible. Failures in session metrics or knowledge reuse should NOT abort the retrospective. The existing pipeline output is valuable on its own. New steps use a best-effort pattern: compute and attach if successful, log warning and leave field as `None` if not.

```
new_computation() -> Result<T>
  Ok(value)  -> report.field = Some(value)
  Err(e)     -> tracing::warn!("col-020: {step} failed: {e}"); report.field = None
```

## Technology Decisions

| Decision | ADR |
|----------|-----|
| Knowledge reuse computed server-side, not in unimatrix-observe | ADR-001 |
| Idempotent counter updates via absolute-set, not additive increment | ADR-002 |
| Attribution metadata on report for consumer trust assessment | ADR-003 |
| File path extraction via explicit tool-to-field mapping | ADR-004 |

## Integration Points

### Existing Dependencies (Read-Only)

- `ObservationSource::load_feature_observations()` — loads observation records (col-012)
- `ObservationSource::discover_sessions_for_feature()` — discovers session IDs (col-012)
- `Store::scan_sessions_by_feature()` — loads SessionRecord for outcome inspection
- `Store::scan_query_log_by_session()` — existing per-session scan (nxs-010)
- `Store::scan_injection_log_by_session()` — existing per-session scan (col-010)
- `Store::get_topic_delivery()` — read current counters (nxs-010)

### New APIs Introduced (C4)

See Integration Surface below.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `SessionSummary` | `pub struct SessionSummary { session_id: String, started_at: u64, duration_secs: u64, tool_distribution: HashMap<String, u64>, top_file_zones: Vec<(String, u64)>, agents_spawned: Vec<String>, knowledge_in: u64, knowledge_out: u64, outcome: Option<String> }` | `unimatrix-observe/src/types.rs` |
| `KnowledgeReuse` | `pub struct KnowledgeReuse { tier1_reuse_count: u64, by_category: HashMap<String, u64>, category_gaps: Vec<String> }` | `unimatrix-observe/src/types.rs` |
| `RetrospectiveReport` new fields | `session_summaries: Option<Vec<SessionSummary>>`, `knowledge_reuse: Option<KnowledgeReuse>`, `rework_session_count: Option<u64>`, `context_reload_pct: Option<f64>` | `unimatrix-observe/src/types.rs` |
| `compute_session_summaries` | `pub fn compute_session_summaries(records: &[ObservationRecord]) -> Vec<SessionSummary>` | `unimatrix-observe/src/session_metrics.rs` |
| `compute_context_reload_pct` | `pub fn compute_context_reload_pct(summaries: &[SessionSummary], records: &[ObservationRecord]) -> f64` | `unimatrix-observe/src/session_metrics.rs` |
| `Store::scan_query_log_by_sessions` | `pub fn scan_query_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<QueryLogRecord>>` | `unimatrix-store/src/query_log.rs` |
| `Store::scan_injection_log_by_sessions` | `pub fn scan_injection_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<InjectionLogRecord>>` | `unimatrix-store/src/injection_log.rs` |
| `Store::count_active_entries_by_category` | `pub fn count_active_entries_by_category(&self) -> Result<HashMap<String, u64>>` | `unimatrix-store/src/read.rs` |
| `Store::set_topic_delivery_counters` | `pub fn set_topic_delivery_counters(&self, topic: &str, total_sessions: i64, total_tool_calls: i64, total_duration_secs: i64) -> Result<()>` | `unimatrix-store/src/topic_deliveries.rs` |

### Tool Category Classification (C1 internal)

| Tool Name | Category |
|-----------|----------|
| `Read`, `Glob`, `Grep` | `read` |
| `Edit`, `Write` | `write` |
| `Bash` | `execute` |
| `context_search`, `context_lookup`, `context_get` | `search` |
| `context_store` | `store` |
| SubagentStart | `spawn` |
| Everything else | `other` |

### File Path Extraction Mapping (C1 internal)

| Tool | JSON Field | Example |
|------|-----------|---------|
| `Read` | `.file_path` | `{"file_path": "/foo/bar.rs"}` |
| `Edit` | `.file_path` | `{"file_path": "/foo/bar.rs", ...}` |
| `Write` | `.file_path` | `{"file_path": "/foo/bar.rs", ...}` |
| `Glob` | `.path` (optional, fallback to `.pattern`) | `{"path": "/foo", "pattern": "*.rs"}` |
| `Grep` | `.path` (optional) | `{"path": "/foo", "pattern": "test"}` |
| Unknown tool | Skip silently | N/A |

Directory prefix extraction: take the path up to the last `/`, or the first 3 path components, whichever is shorter. This produces zone names like `crates/unimatrix-store/src` rather than full file paths.

### Rework Outcome Detection

Substring match on `SessionRecord.outcome`:
- Match `result:rework` OR `result:failed` (case-insensitive substring)
- A session counts as rework if either pattern appears anywhere in the outcome string
- Sessions with `None` outcome are NOT counted as rework

### Overlapping Session Handling (SR-06)

Sessions are ordered chronologically by `started_at` from `SessionRecord`. For context reload computation, "prior sessions" means all sessions with a strictly earlier `started_at`. Concurrent sessions (identical `started_at`) are treated as independent — neither is "prior" to the other. This avoids ambiguity without needing to detect or merge overlapping sessions.

## Scope Risk Mitigations

### SR-07: Attribution Quality Bounding Metric Accuracy

The report includes attribution metadata so consumers can assess trustworthiness:

```rust
pub struct AttributionMetadata {
    pub attributed_session_count: usize,
    pub total_session_count: usize,
}
```

Added as `attribution: Option<AttributionMetadata>` on `RetrospectiveReport`. `attributed_session_count` is the number of sessions with non-NULL `feature_cycle` matching the requested topic. `total_session_count` is the total discovered via `discover_sessions_for_feature`. When attribution coverage is low (< 50%), consumers should treat derived metrics as approximate. See ADR-003.

### SR-08: Knowledge Reuse Crossing observe/server Boundary

Knowledge reuse computation lives in the server handler, not in unimatrix-observe. This is a deliberate, documented exception to the "all retrospective computation in unimatrix-observe" pattern. The exception is scoped: only computations requiring multi-table Store joins live server-side. Pure observation-derived computation (session summaries, reload rate) remains in unimatrix-observe. See ADR-001.

### SR-09: Idempotent Counter Updates on Re-Runs

`topic_deliveries` counters are updated via absolute-set, not additive increment. The handler computes the correct totals from source data (session count from `scan_sessions_by_feature`, tool calls from observation records, duration from timestamps) and writes them as absolute values. Repeated retrospective runs on the same topic produce the same counter values. See ADR-002.

### SR-01: JSON Parsing Robustness

`result_entry_ids` is parsed via `serde_json::from_str::<Vec<u64>>()`. On parse failure (malformed JSON, empty string, nulls), the query_log row contributes zero entry IDs to reuse computation. Logged at `tracing::debug!` level. No panic, no error propagation.

### SR-04: File Path Extraction

An explicit tool-to-field mapping covers known tools. Unknown tools are silently skipped (no data loss logging — the tool may legitimately have no file path). See ADR-004 and the mapping table above.

## Open Questions

1. **query_log batch scan performance**: The new `scan_query_log_by_sessions` uses `WHERE session_id IN (...)` with a parameter list. For topics with >100 sessions, this may need chunking. Current scale (< 100 sessions/topic) does not require it, but the implementation should handle the degenerate case gracefully (chunk into batches of 50).

2. **Helpful signal attribution**: AC-06 mentions "explicit helpful signals on cross-session entries" as part of Tier 1. The entries table has `helpful_count` but no per-session attribution of who marked it helpful. For v1, Tier 1 counts only search-return and injection-log signals, not helpful signals. Helpful-signal-based reuse requires session-attributed feedback (not available today).
