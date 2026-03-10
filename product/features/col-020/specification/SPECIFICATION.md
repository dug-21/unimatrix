# col-020: Multi-Session Retrospective -- Specification

## Objective

Add per-session decomposition, cross-session knowledge reuse measurement, rework session counting, and context reload rate to the retrospective pipeline. These four capabilities transform the retrospective from a flat aggregate into a multi-session analysis that answers: what did each session do, is knowledge flowing across sessions, and how much rework occurred. The feature also connects retrospective computation to topic_deliveries aggregate counters.

## Functional Requirements

### FR-01: Session Summary Computation

FR-01.1: Group observation records by `session_id` and produce one `SessionSummary` per distinct session.

FR-01.2: Each `SessionSummary` contains `tool_distribution` -- a map from tool category to PreToolUse event count. Categories are: `read` (Read, Glob, Grep), `write` (Edit, Write), `execute` (Bash), `search` (context_search, context_lookup, context_get), `store` (context_store), `spawn` (SubagentStart), `other` (all remaining tools).

FR-01.3: Each `SessionSummary` contains `top_file_zones` -- the top 5 directory prefixes by frequency, extracted from file path fields in Read/Edit/Write/Glob tool inputs.

FR-01.4: File path extraction uses an explicit tool-to-field mapping: Read -> `file_path`, Edit -> `file_path`, Write -> `file_path`, Glob -> `path`. Tools not in this mapping are skipped. The `input` JSON field is parsed; missing or non-string path values are silently skipped (logged at debug level, not errors).

FR-01.5: Each `SessionSummary` contains `agents_spawned` -- the list of agent names from SubagentStart observation records (the `tool` field of SubagentStart events).

FR-01.6: Each `SessionSummary` contains `knowledge_in` (count of context_search + context_lookup + context_get PreToolUse calls) and `knowledge_out` (count of context_store PreToolUse calls).

FR-01.7: Each `SessionSummary` contains `started_at` (earliest observation timestamp in the session) and `duration_secs` (difference between latest and earliest observation timestamps, in seconds).

FR-01.8: Each `SessionSummary` contains `outcome` (the session outcome from the sessions table, if available; None otherwise).

FR-01.9: Session summaries are ordered by `started_at` ascending (chronological).

### FR-02: Knowledge Reuse Computation (Tier 1)

FR-02.1: Compute `tier1_reuse_count` -- the count of distinct entry IDs that satisfy both conditions: (a) the entry was stored or created in session A within the topic, and (b) the entry was retrieved in session B (different session, same topic) via either query_log (entry ID appears in `result_entry_ids`) or injection_log (entry ID appears with session B's session_id).

FR-02.2: Compute `by_category` -- a breakdown of Tier 1 reused entries by their knowledge category (convention, pattern, decision, procedure, lesson-learned). Entries with categories not in this set are grouped under their actual category string.

FR-02.3: Compute `category_gaps` -- categories that have at least one active entry in the knowledge base but received zero reuse across all of the topic's sessions. This is scoped to all active entries globally, not just entries created within the topic.

FR-02.4: Knowledge reuse computation is performed in the server layer (unimatrix-server), not in unimatrix-observe. The assembled `KnowledgeReuse` struct is passed to the report builder. This follows Option B from SCOPE.md to avoid bloating the ObservationSource trait.

FR-02.5: When query_log data is absent (pre-nxs-010 sessions or sessions with no search activity), knowledge reuse computes from available data sources (injection_log, helpful signals). Search-based reuse reports 0 for those sessions without producing errors.

FR-02.6: Parse `result_entry_ids` from query_log as a JSON array of integers. Malformed JSON, empty strings, and null values produce an empty entry ID set for that row (logged at warn level, not a fatal error).

### FR-03: Rework Session Count

FR-03.1: Compute `rework_session_count` -- the count of sessions within the topic whose `outcome` field contains the substring `result:rework` OR the substring `result:failed` (case-sensitive match).

FR-03.2: Sessions with NULL or empty outcome are not counted as rework sessions.

### FR-04: Context Reload Percentage

FR-04.1: Compute `context_reload_pct` -- the percentage of files read in sessions after the first session that were also read in at least one chronologically prior session within the same topic.

FR-04.2: Sessions are ordered chronologically by their earliest observation timestamp. For sessions with identical earliest timestamps (concurrent sessions), order by session_id lexicographically as a tiebreaker.

FR-04.3: The value is a float in the range [0.0, 1.0]. A topic with only one session has `context_reload_pct` of 0.0 (no prior session to reload from).

FR-04.4: The value is reported raw without interpretation labels or qualitative assessment.

### FR-05: Report Extension

FR-05.1: Add `session_summaries: Option<Vec<SessionSummary>>` to `RetrospectiveReport`. Absent (not null) when not computed.

FR-05.2: Add `knowledge_reuse: Option<KnowledgeReuse>` to `RetrospectiveReport`. Absent when not computed.

FR-05.3: Add `rework_session_count: Option<u64>` to `RetrospectiveReport`. Absent when not computed.

FR-05.4: Add `context_reload_pct: Option<f64>` to `RetrospectiveReport`. Absent when not computed.

FR-05.5: All new fields use `#[serde(default, skip_serializing_if = "Option::is_none")]` for backward compatibility.

FR-05.6: Add `attribution_coverage: Option<AttributionCoverage>` to `RetrospectiveReport`. This struct reports `attributed_sessions` (sessions with non-NULL feature_cycle matching the topic) and `total_sessions` (all sessions discovered for the topic, including those attributed via content-based fallback). Absent when not computed. See NFR-05 for rationale.

### FR-06: Topic Deliveries Counter Update

FR-06.1: After computing the retrospective, update the `topic_deliveries` record for the topic with current values for `total_sessions`, `total_tool_calls`, and `total_duration_secs`.

FR-06.2: Counter updates are idempotent. Use absolute replacement (set to computed values), not additive increment. Repeated retrospective runs on the same topic produce the same counter values, not accumulated totals.

### FR-07: Tool Handler Integration

FR-07.1: The `context_retrospective` handler computes session summaries, knowledge reuse, rework count, reload percentage, and attribution coverage after loading observation records and before returning the report.

FR-07.2: Knowledge reuse computation loads query_log and injection_log data for the topic's session IDs using batch queries.

FR-07.3: The handler updates topic_deliveries counters after successful computation (FR-06).

FR-07.4: Empty topic (zero sessions, zero observation records) returns existing cached/empty behavior unchanged. New fields are absent (None) in the cached report.

### FR-08: Store API Extensions

FR-08.1: Add `scan_injection_log_by_sessions(session_ids: &[String]) -> Vec<InjectionLogRecord>` -- batch load injection_log rows for multiple session IDs.

FR-08.2: Add `scan_query_log_by_sessions(session_ids: &[String]) -> Vec<QueryLogRecord>` -- batch variant that loads query_log rows for multiple session IDs.

FR-08.3: Add `count_active_entries_by_category() -> HashMap<String, u64>` -- counts active (non-deprecated, non-quarantined) entries grouped by category. Used for category gap detection.

FR-08.4: Add or modify `set_topic_delivery_counters(topic, total_sessions, total_tool_calls, total_duration_secs)` -- absolute setter for topic_deliveries counters (replacing additive increment for retrospective use).

## Non-Functional Requirements

NFR-01: **Performance** -- Retrospective computation (including new cross-session metrics) completes within existing spawn_blocking budget. Added database reads (query_log, injection_log batch scans) are indexed queries on bounded result sets (< 100 sessions per topic, < 1000 injection_log entries typical). No new background tasks or async coordination required.

NFR-02: **Backward compatibility** -- Existing retrospective consumers that deserialize `RetrospectiveReport` JSON without the new fields must not break. All new fields are optional with serde defaults. Pre-col-020 JSON round-trips through the updated struct without data loss.

NFR-03: **No regression** -- All existing retrospective tests (21 detection rules, report builder, baseline comparison, entries analysis, narrative synthesis) continue to pass without modification.

NFR-04: **Graceful degradation** -- Missing data sources (absent query_log for pre-nxs-010 sessions, missing injection_log entries, NULL session outcomes) produce conservative results (lower counts, 0.0 rates), not errors. The retrospective never fails due to incomplete cross-session data.

NFR-05: **Attribution transparency** -- Sessions attributed via content-based fallback (rather than direct `feature_cycle` match) are counted in `total_sessions` but distinguished via `AttributionCoverage`. This addresses SR-07: consumers can assess metric trustworthiness by comparing `attributed_sessions` to `total_sessions`. When `attributed_sessions < total_sessions`, some sessions were attributed heuristically and cross-session metrics may undercount.

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `RetrospectiveReport` includes `session_summaries` field containing one `SessionSummary` per distinct session_id in the observation data | Unit test: build report from records with N distinct session_ids, assert N summaries returned |
| AC-02 | Each `SessionSummary` contains tool distribution grouped by category (read, write, execute, search, store, spawn, other) | Unit test: construct records with known tool names, verify category counts |
| AC-03 | Each `SessionSummary` contains top 5 file path zones (directory prefixes) by frequency | Unit test: construct records with file paths across multiple directories, verify top 5 ordering and count |
| AC-04 | Each `SessionSummary` contains the list of agents spawned (SubagentStart event tool names) | Unit test: include SubagentStart records, verify agents_spawned list |
| AC-05 | Each `SessionSummary` contains `knowledge_in` count and `knowledge_out` count | Unit test: mix search/store tool calls, verify counts |
| AC-06 | `knowledge_reuse.tier1_reuse_count` counts distinct entries stored in one session and retrieved in a different session within the same topic | Integration test: seed query_log and injection_log with cross-session patterns, verify count |
| AC-07 | `knowledge_reuse.by_category` breaks down Tier 1 reuse by entry category | Integration test: seed entries with known categories, verify breakdown matches |
| AC-08 | `knowledge_reuse.category_gaps` lists categories with active entries but zero reuse | Integration test: seed active entries in categories not referenced by any session, verify gap list |
| AC-09 | `rework_session_count` counts sessions whose outcome contains `result:rework` or `result:failed` | Unit test: construct sessions with various outcome strings, verify count |
| AC-10 | `context_reload_pct` reports percentage of files read in session N+1 that were also read in a prior session | Unit test: construct multi-session records with overlapping file reads, verify percentage |
| AC-11 | All new report fields use `#[serde(default, skip_serializing_if)]` -- old JSON without new fields deserializes successfully | Unit test: deserialize pre-col-020 JSON into updated RetrospectiveReport struct |
| AC-12 | `context_retrospective` updates `topic_deliveries` aggregate counters after computation | Integration test: run retrospective, verify topic_deliveries record updated with absolute values |
| AC-13 | `context_reload_pct` is reported as a raw float (0.0-1.0) without interpretation labels | Unit test: verify return type is f64 in range, no string labels attached |
| AC-14 | Empty topic (zero sessions) returns existing cached/empty behavior unchanged -- new fields are None | Unit test: call with empty feature_cycle, verify new fields absent |
| AC-15 | All existing retrospective tests continue to pass | CI: full test suite passes with no modifications to existing tests |
| AC-16 | Session summaries are ordered by `started_at` ascending (chronological) | Unit test: construct out-of-order records, verify summaries sorted by started_at |

## Domain Models

### SessionSummary

A per-session activity profile derived from observation records. Represents one agent session's contribution to a topic, including what tools it used, what files it touched, what agents it spawned, and how much knowledge flowed in and out. Does not classify session type (design/delivery/bugfix) -- the consumer interprets session character from the reported data.

**Fields**: `session_id` (String), `started_at` (u64, epoch millis), `duration_secs` (u64), `tool_distribution` (HashMap<String, u64> -- category to count), `top_file_zones` (Vec<(String, u64)> -- directory to count, max 5), `agents_spawned` (Vec<String>), `knowledge_in` (u64), `knowledge_out` (u64), `outcome` (Option<String>).

### KnowledgeReuse

Cross-session knowledge flow measurement for a topic. Answers: "Is knowledge stored in earlier sessions being retrieved and used in later sessions?" Tier 1 only -- conservative signal from explicit search/injection sequences.

**Fields**: `tier1_reuse_count` (u64 -- distinct entry count), `by_category` (HashMap<String, u64> -- category to reuse count), `category_gaps` (Vec<String> -- categories with active entries but zero reuse).

### AttributionCoverage

Metadata about how sessions were attributed to the topic. Enables consumers to assess trustworthiness of cross-session metrics. When `attributed_sessions < total_sessions`, some sessions were matched via content-based heuristic rather than direct `feature_cycle` tag.

**Fields**: `attributed_sessions` (u64 -- direct feature_cycle match), `total_sessions` (u64 -- all sessions including fallback-attributed).

### Tool Categories

Fixed categorization of tool names for session profiling:
- **read**: Read, Glob, Grep
- **write**: Edit, Write
- **execute**: Bash
- **search**: context_search, context_lookup, context_get
- **store**: context_store
- **spawn**: SubagentStart
- **other**: everything else

### Rework Outcome Patterns

Substring patterns matched against session `outcome` text (case-sensitive):
- `result:rework`
- `result:failed`

These are the structured outcome tags produced by the existing outcome tracking system (col-001). False positives from free-form text are accepted as a known trade-off (SR-03).

### File Path Extraction Mapping

Tool-to-field mapping for extracting file paths from observation record `input` JSON:
- Read -> `file_path`
- Edit -> `file_path`
- Write -> `file_path`
- Glob -> `path`

Directory prefix is extracted by taking the parent directory of the resolved path. Tools not in this mapping are ignored for file zone computation.

## User Workflows

### Retrospective Consumer (Agent or Human)

1. Agent calls `context_retrospective` with a `feature_cycle` (topic) parameter
2. System loads observation records for all sessions attributed to that topic
3. System computes existing metrics (MetricVector, hotspots, baselines, narratives)
4. System computes new multi-session metrics: session summaries, knowledge reuse, rework count, reload rate, attribution coverage
5. System assembles complete `RetrospectiveReport` with all fields
6. System updates topic_deliveries counters (idempotent)
7. Agent receives JSON report containing both aggregate and per-session data
8. Agent interprets session character from tool distribution and file zones (Unimatrix does not classify sessions)
9. Agent uses `knowledge_reuse` to assess whether Unimatrix is delivering value across sessions
10. Agent uses `attribution_coverage` to assess metric trustworthiness

### Cached/Empty Path

1. Agent calls `context_retrospective` for a topic with no new observation data
2. System checks for cached MetricVector
3. If cached: returns cached report with `is_cached: true`, new multi-session fields absent (None)
4. If no cache: returns error (existing behavior, unchanged)

## Constraints

### Hard Dependencies

- **col-017** (Hook-Side Topic Attribution): Sessions must have `feature_cycle` attribution for direct-path observation loading. Without col-017, the content-based fallback is used, producing lower attribution coverage. col-017 is Wave 1; must land first.
- **nxs-010** (Activity Schema Evolution): `topic_deliveries` and `query_log` tables must exist (schema v11). Required for knowledge reuse computation and counter updates. nxs-010 has landed.

### Data Constraints

- **ObservationRecord lacks session sequence number**: Cross-session ordering uses earliest observation timestamp per session. Concurrent sessions (overlapping timestamps) use session_id lexicographic tiebreaker.
- **injection_log has no topic column**: Cross-session injection queries require first discovering session IDs for the topic, then querying injection_log by those IDs.
- **query_log stores entry IDs as JSON strings**: `result_entry_ids` is `"[1,2,3]"`. Robust JSON parsing required with fallback to empty set on malformed data.
- **File path locations vary by tool**: Each tool stores file paths under different JSON keys. An explicit mapping (FR-01.4) handles this; unrecognized tools are silently skipped.

### Attribution Quality (SR-07)

Sessions with incomplete or heuristic attribution affect all cross-session metrics. The specification addresses this through `AttributionCoverage` (FR-05.6), which reports the ratio of directly-attributed sessions to total sessions. Consumers must treat metrics from topics with low attribution coverage as approximate.

Specifically for sessions with incomplete attribution:
- **Session summaries** are produced for all sessions (attributed and fallback-attributed). Each summary is self-contained and accurate for that session's data regardless of attribution method.
- **Knowledge reuse** may undercount when fallback-attributed sessions have incomplete query_log or injection_log data. The conservative Tier 1 signal tolerates this -- undercounting is acceptable, overcounting is not.
- **Rework session count** depends on session outcome population, which is independent of topic attribution. Attribution quality does not affect this metric.
- **Context reload percentage** is computed across all sessions in the topic. Fallback-attributed sessions that should not belong to the topic would inflate this metric. Attribution coverage lets consumers discount reload rates when coverage is low.

### Architectural Constraint

- **ObservationSource trait stability**: No changes to the ObservationSource trait. New data loading methods are added to Store/SqlObservationSource directly.
- **Server-side knowledge reuse**: Knowledge reuse computation lives in unimatrix-server (Option B), not unimatrix-observe. This is a deliberate exception to the "all computation in observe" pattern, justified by the cross-table join requirement. An ADR should document this decision (SR-08).

### Idempotency Constraint (SR-09)

- **topic_deliveries updates must be idempotent**: Use absolute counter replacement, not additive increment. Running `context_retrospective` twice for the same topic produces identical counter values.

## Dependencies

### Crate Dependencies (no new external crates)

- `unimatrix-observe` -- new types (SessionSummary, KnowledgeReuse, AttributionCoverage), new computation module (session_metrics.rs), report builder extension
- `unimatrix-server` -- knowledge reuse computation, Store API extensions, tool handler integration, topic_deliveries counter update
- `unimatrix-store` -- new query methods (batch injection_log, batch query_log, active entries by category, absolute counter setter)
- `unimatrix-core` -- ObservationRecord (existing, unchanged)

### Existing Components Used

- `SqlObservationSource::load_feature_observations` -- loads observation records (unchanged)
- `SqlObservationSource::discover_sessions_for_feature` -- returns session IDs for a topic (unchanged)
- `Store::get_topic_delivery` -- reads topic_deliveries record (unchanged)
- `Store::scan_query_log_by_session` -- existing single-session query_log scan (extended to batch variant)
- `build_report()` -- report assembly function (extended with new parameters)
- `RetrospectiveReport` -- report struct (extended with new optional fields)

## NOT in Scope

- **Session-type classification** (design vs delivery vs bugfix). Session character emerges from the data; the consumer interprets.
- **Session efficiency trend** (`session_efficiency_trend`). Dropped explicitly -- efficiency comparisons require session-type awareness.
- **Tier 2 or Tier 3 knowledge reuse**. Only Tier 1 (search/injection sequences). Briefing-injected entries are opaque.
- **Changes to existing aggregate metrics** (UniversalMetrics, PhaseMetrics). Those continue unchanged.
- **Changes to detection rules or hotspot finding logic**. Existing 21 rules remain untouched.
- **Changes to ObservationSource trait**. New queries are direct Store methods.
- **Per-session hotspot findings**. Hotspots remain topic-level aggregates.
- **Retrospective output format changes beyond new additive fields**. vnc-011 (ReportFormatter) handles markdown formatting separately.
- **New MetricVector columns for cross-session metrics**. Session-level data lives in the report struct, not in the 22-column observation_metrics table.
