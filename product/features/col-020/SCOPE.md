# col-020: Multi-Session Retrospective

## Problem Statement

The retrospective pipeline treats all sessions within a topic as a flat aggregated bag. A 10-session feature delivery produces one `RetrospectiveReport` with aggregate metrics (total tool calls, total duration, hotspot counts) but no visibility into how those sessions relate to each other. Three questions are unanswerable:

1. **What did each session actually do?** Without per-session decomposition, a design session and a delivery session are indistinguishable in the retrospective output. Session character (tool distribution, file zones, agents spawned, knowledge flow) is invisible.

2. **Is knowledge flowing across session boundaries?** Unimatrix's value proposition is that knowledge stored in one session helps future sessions. But there is no measurement of whether entries stored in session N are actually retrieved and used in session N+1. The `knowledge_reuse_rate` is the headline proof of Unimatrix's value and currently does not exist.

3. **How much rework happened?** Sessions with rework outcomes (bug fixes, retries) are counted in aggregate but not surfaced as a distinct count per topic.

With col-017 (topic attribution) and nxs-010 (topic_deliveries + query_log tables) now landed, the data infrastructure exists to answer these questions.

## Goals

1. Add `topic_session_summary` to the retrospective report: per-session activity profiles showing tool distribution by category, file path zones, agents spawned, and knowledge flow in/out
2. Add `knowledge_reuse` to the retrospective report: cross-session knowledge flow measurement with Tier 1 reuse count, category breakdown, and category gap detection
3. Add `rework_session_count` to the retrospective report: count of sessions with rework outcomes for the topic
4. Add `context_reload_pct` to the retrospective report: cross-session file re-read rate reported raw without interpretation
5. Update `topic_deliveries` aggregate counters after retrospective computation

## Non-Goals

- **No session-type classification** (design vs delivery vs bugfix). Session character emerges from the reported data; the consumer interprets, not Unimatrix. This avoids baking one workflow as a product assumption.
- **No session efficiency trend** (`session_efficiency_trend` was explicitly dropped). Efficiency comparisons are meaningless without session-type awareness -- a design session reading 50 files is not "less efficient" than a delivery session reading 5.
- **No Tier 2 or Tier 3 knowledge reuse measurement.** Only Tier 1 (search-then-lookup/get sequences across session boundaries + explicit helpful signals on cross-session entries). Briefing-injected entries are opaque -- there is no way to measure whether they "helped" without session-type classification.
- **No changes to the existing per-topic aggregate metrics** (UniversalMetrics, PhaseMetrics). Those continue to work as-is.
- **No changes to detection rules or hotspot finding logic.** Existing 21 detection rules remain untouched.
- **No retrospective output format changes** beyond new fields. The JSON structure is additive (serde `skip_serializing_if` for backward compatibility). vnc-011 (ReportFormatter) handles markdown formatting separately.
- **No changes to the ObservationSource trait.** New queries are added to Store/SqlObservationSource as needed, but the trait interface stays stable.

## Background Research

### Current Retrospective Architecture

The retrospective pipeline has three layers:

1. **Data loading** (`ObservationSource` trait in unimatrix-observe, implemented by `SqlObservationSource` in unimatrix-server): Loads `ObservationRecord` vectors from the `observations` and `sessions` tables. Two paths: direct `feature_cycle` query (fast path) and content-based attribution fallback.

2. **Computation** (unimatrix-observe crate): `detect_hotspots()` runs 21 detection rules. `compute_metric_vector()` produces `MetricVector` (22 universal metrics + per-phase metrics). `build_report()` assembles the `RetrospectiveReport`.

3. **Orchestration** (`context_retrospective` MCP tool handler in unimatrix-server): Loads data, runs computation, stores metrics, computes baselines, drains accumulated entry analysis, synthesizes narratives, fires lesson-learned write, and returns the report.

### Data Available for Per-Session Decomposition

Each `ObservationRecord` carries: `ts`, `hook` (PreToolUse/PostToolUse/SubagentStart/SubagentStop), `session_id`, `tool`, `input` (JSON), `response_size`, `response_snippet`.

From these fields, per-session profiles can be derived:
- **Tool distribution by category**: Group tools into categories (read: Read/Glob/Grep, write: Edit/Write, execute: Bash, search: context_search/context_lookup/context_get, store: context_store, spawn: SubagentStart). Count PreToolUse events per category.
- **File path zones**: Extract directory prefixes from Read/Edit/Write/Glob tool inputs. Report top directories touched.
- **Agents spawned**: SubagentStart events with agent name in the `tool` field.
- **Knowledge flow in**: Count of context_search/context_lookup/context_get calls (knowledge retrieval). Also injection_log entries for this session.
- **Knowledge flow out**: Count of context_store calls (knowledge creation).

### Data Available for Knowledge Reuse

Two data sources enable cross-session reuse measurement:

1. **query_log** (nxs-010): Records every search query with `session_id`, `result_entry_ids` (JSON array of entry IDs returned), and `ts`. By joining query_log across sessions attributed to the same topic, we can detect search-then-lookup/get sequences where an entry created/stored in session A is retrieved by search in session B.

2. **injection_log**: Records `(session_id, entry_id, confidence, timestamp)` for every briefing injection. Cross-session injection reuse: entry stored in session A, injected in session B.

3. **entries table**: Has `feature_cycle` (topic of origin), `category`, `helpful_count`, `unhelpful_count`. Entries with `helpful_count > 0` that originated in a different session than where they were rated helpful represent confirmed cross-session value.

**Tier 1 reuse signal** (conservative, trustworthy):
- query_log shows entry X was returned to a search in session B
- injection_log shows entry X was injected into session B
- Entry X was stored/created in session A (different session, same topic)
- This is intentional retrieval = strong signal

**Category gap detection**: For each knowledge category (convention, pattern, decision, procedure, lesson-learned), check if active entries exist but got zero reuse across the topic's sessions. Categories with active but unused entries are "populated but unused" -- possibly stale or poorly categorized.

### Data Available for Context Reload Rate

Read tool PostToolUse records carry the file path in the input field. Cross-session reload rate: files read in session N that are also read in session N+1. Reported as a raw percentage without interpretation -- high reload might be expected (delivery reading design artifacts) or wasteful (re-reading the same source files because context was lost).

### Data Available for Rework Sessions

The `sessions` table has an `outcome TEXT` column. Sessions with `outcome` containing rework indicators (e.g., "rework", "failed", rework-tagged outcomes from `context_retrospective` tool calls) can be counted per topic.

### Existing Store APIs Available

- `Store::get_topic_delivery(topic)` -- returns `TopicDeliveryRecord` with aggregate counters
- `Store::update_topic_delivery_counters(topic, sessions_delta, tool_calls_delta, duration_delta)` -- atomic counter increment
- `Store::scan_query_log_by_session(session_id)` -- returns all query_log rows for a session
- `SqlObservationSource::load_feature_observations(feature_cycle)` -- loads all observation records for a feature
- `SqlObservationSource::discover_sessions_for_feature(feature_cycle)` -- returns session IDs

### Codebase Patterns

- All retrospective computation lives in unimatrix-observe. Data loading lives in unimatrix-server's `SqlObservationSource`. New metrics should follow this split.
- `RetrospectiveReport` uses `#[serde(default, skip_serializing_if)]` for optional/additive fields. New fields follow the same pattern.
- MetricVector is stored via `Store::store_metrics()` and loaded via `Store::get_metrics()`. It uses the `observation_metrics` table with 22 typed columns (nxs-009 ADR-001). New cross-session metrics may need additional columns or a separate storage mechanism.
- The report builder pattern: `build_report()` takes pre-computed components. New session-level data follows the same assembly pattern.

## Proposed Approach

### 1. New Types (unimatrix-observe/src/types.rs)

Add four new structs to the retrospective report:

```rust
/// Per-session activity profile.
pub struct SessionSummary {
    pub session_id: String,
    pub started_at: u64,      // earliest observation ts
    pub duration_secs: u64,
    pub tool_distribution: HashMap<String, u64>,  // category -> count
    pub top_file_zones: Vec<(String, u64)>,       // directory -> count, top 5
    pub agents_spawned: Vec<String>,
    pub knowledge_in: u64,    // search + lookup + get calls
    pub knowledge_out: u64,   // store calls
    pub outcome: Option<String>,
}

/// Cross-session knowledge reuse report.
pub struct KnowledgeReuse {
    pub tier1_reuse_count: u64,
    pub by_category: HashMap<String, u64>,  // category -> reuse count
    pub category_gaps: Vec<String>,         // categories with entries but zero reuse
}
```

### 2. New Computation Module (unimatrix-observe/src/session_metrics.rs)

New module with functions:
- `compute_session_summaries(records: &[ObservationRecord]) -> Vec<SessionSummary>` -- groups records by session_id, computes per-session profiles
- `compute_context_reload_pct(summaries: &[SessionSummary], records: &[ObservationRecord]) -> f64` -- cross-session file re-read rate

### 3. Knowledge Reuse Computation (unimatrix-observe or unimatrix-server)

Knowledge reuse requires data from multiple tables (query_log, injection_log, entries). This crosses the unimatrix-observe/unimatrix-server boundary. Two options:

**Option A**: Extend `ObservationSource` trait with methods for query_log and injection_log access, keeping computation in unimatrix-observe.

**Option B**: Compute knowledge reuse in unimatrix-server (where Store access is direct), pass the result to `build_report()`.

Recommendation: **Option B**. Knowledge reuse is inherently a cross-table join that requires Store access. Adding query_log and injection_log methods to ObservationSource would bloat the trait for a single consumer. Compute in the server, pass the assembled `KnowledgeReuse` struct to the report builder.

### 4. Report Extension

Add new optional fields to `RetrospectiveReport`:
- `session_summaries: Option<Vec<SessionSummary>>` (skip_serializing_if None)
- `knowledge_reuse: Option<KnowledgeReuse>` (skip_serializing_if None)
- `rework_session_count: Option<u64>` (skip_serializing_if None)
- `context_reload_pct: Option<f64>` (skip_serializing_if None)

### 5. Tool Handler Integration

In `context_retrospective`, after computing the existing metrics:
1. Compute session summaries from the already-loaded observation records
2. Load query_log + injection_log data for the topic's sessions
3. Compute knowledge reuse
4. Count rework sessions from sessions table
5. Compute context reload percentage
6. Attach all results to the report
7. Update topic_deliveries counters via `update_topic_delivery_counters()`

### 6. Store API Extensions

New Store methods needed:
- `scan_injection_log_by_sessions(session_ids: &[String]) -> Vec<InjectionLogRecord>` -- batch load injection_log for multiple sessions
- `scan_query_log_by_sessions(session_ids: &[String]) -> Vec<QueryLogRecord>` -- batch variant of existing scan_query_log_by_session
- `count_active_entries_by_category() -> HashMap<String, u64>` -- for category gap detection

## Acceptance Criteria

- AC-01: `RetrospectiveReport` includes `session_summaries` field containing one `SessionSummary` per distinct session_id in the observation data
- AC-02: Each `SessionSummary` contains tool distribution grouped by category (read, write, execute, search, store, spawn, other)
- AC-03: Each `SessionSummary` contains top 5 file path zones (directory prefixes) by frequency
- AC-04: Each `SessionSummary` contains the list of agents spawned (SubagentStart event tool names)
- AC-05: Each `SessionSummary` contains `knowledge_in` count (context_search + context_lookup + context_get PreToolUse calls) and `knowledge_out` count (context_store PreToolUse calls)
- AC-06: `RetrospectiveReport` includes `knowledge_reuse` field with `tier1_reuse_count`: count of distinct entries that were stored in one session and retrieved (search returning them, or injection_log containing them) in a different session within the same topic
- AC-07: `knowledge_reuse.by_category` breaks down Tier 1 reuse by entry category (convention, pattern, decision, procedure, lesson-learned)
- AC-08: `knowledge_reuse.category_gaps` lists categories that have active entries in the knowledge base but received zero reuse across the topic's sessions
- AC-09: `RetrospectiveReport` includes `rework_session_count` with the count of sessions whose outcome field indicates rework
- AC-10: `RetrospectiveReport` includes `context_reload_pct` as the percentage of files read in session N+1 that were also read in at least one prior session within the same topic
- AC-11: All new report fields use `#[serde(default, skip_serializing_if)]` for backward compatibility -- old consumers that deserialize the JSON without these fields do not break
- AC-12: `context_retrospective` updates `topic_deliveries` aggregate counters (total_sessions, total_tool_calls, total_duration_secs) after computation
- AC-13: `context_reload_pct` is reported as a raw float (0.0-1.0) without interpretation labels
- AC-14: Empty topic (zero sessions) returns the existing cached/empty behavior unchanged
- AC-15: All existing retrospective tests continue to pass (no regressions)
- AC-16: Session summaries are ordered by `started_at` ascending (chronological)

## Constraints

- **col-017 dependency**: Sessions must be attributed to topics via `sessions.feature_cycle`. Without col-017, `load_feature_observations` returns empty for most features. col-017 is in Wave 1 and must land first.
- **nxs-010 dependency**: `topic_deliveries` and `query_log` tables must exist (schema v11). Without them, knowledge reuse computation and counter updates fail. nxs-010 is the other Wave 2 feature and has now landed.
- **ObservationRecord lacks session ordering**: Records have `session_id` and `ts` but no session sequence number. Cross-session reload requires ordering sessions chronologically by their earliest observation timestamp.
- **injection_log has no topic column**: To find cross-session injections within a topic, we must first discover session IDs for the topic, then query injection_log by those session IDs. This is the same pattern used by `load_feature_observations`.
- **query_log stores entry IDs as JSON strings**: `result_entry_ids` is a JSON array string (e.g., `"[1,2,3]"`). Parsing required for reuse computation.
- **Backward compatibility**: `RetrospectiveReport` is serialized as JSON to MCP consumers. All new fields must be optional and absent when not computed, not null.
- **File path extraction from tool inputs**: The `input` field is a `serde_json::Value`. File paths appear in different locations depending on the tool (Read: `file_path`, Edit: `file_path`, Write: `file_path`, Glob: `path`). A unified extractor is needed.
- **Performance**: Retrospective computation is already blocking (spawn_blocking). Adding query_log and injection_log scans adds database reads but these are indexed queries on small result sets (typically < 100 sessions per topic, < 1000 injection_log entries).

## Resolved Questions

1. **Rework outcome detection**: OR-match on outcome text — match `result:rework` OR `result:failed` (substring). Avoids being overly opinionated about what constitutes rework. If the outcome contains either signal, it counts.

2. **knowledge_in excludes injection_log**: knowledge_in counts intentional retrieval only (context_search/lookup/get tool calls). Injection_log (ambient briefing) is opaque — no way to assess whether injected entries helped. This is an architectural decision to revisit when/if we gain capabilities to assess injection value or surface confidence metrics in briefing responses.

3. **Category gap detection scopes to all active entries**: Doesn't matter where the knowledge came from if it's valuable. The question "are there entire categories of knowledge your agents never touch?" is more actionable than topic-scoped reuse.

4. **Graceful degradation for missing query_log data**: Report what we have. When query_log data is absent (pre-nxs-010 sessions), knowledge reuse computes from available data (injection_log, helpful signals) and reports 0 for search-based reuse without failing.

5. **Per-session hotspot findings are out of scope for v1**: Hotspots remain topic-level. Session decomposition is additive, not a replacement.

## Tracking

https://github.com/dug-21/unimatrix/issues/190
