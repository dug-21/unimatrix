# Architecture: col-002 Retrospective Pipeline

## System Overview

col-002 adds an observation and analysis subsystem to Unimatrix. The system collects agent telemetry via Claude Code hooks, stores it as per-session JSONL files, and exposes a `context_retrospective` MCP tool that triggers batch analysis. A new `unimatrix-observe` crate performs all analysis computation (parsing, attribution, hotspot detection, metric computation). The server crate orchestrates analysis and stores results in a new `OBSERVATION_METRICS` redb table. `context_status` is extended with observation health fields.

```
Hook Scripts                 unimatrix-observe              unimatrix-server
(shell, stdin)               (pure Rust lib)                (MCP tool handler)
      |                            |                              |
      |  JSONL append              |                              |
      +----> ~/.unimatrix/         |                              |
             observation/          |                              |
             {session}.jsonl       |                              |
                                   |                              |
                       parse <-----+------- context_retrospective |
                       attribute   |                              |
                       detect      |                              |
                       compute     |                              |
                       report -----+------>  store MetricVector    |
                                   |         OBSERVATION_METRICS  |
                                   |                              |
                       file_mgmt --+------>  cleanup >60d files   |
                                              status extension    |
```

## Component Breakdown

### 1. Hook Scripts (Collection Layer)

Shell scripts registered in `.claude/settings.json` that capture Claude Code lifecycle events to per-session JSONL files.

**Responsibilities:**
- Read JSON from stdin (hook input)
- Extract `session_id` for file routing
- Construct a normalized observation record
- Append to `~/.unimatrix/observation/{session_id}.jsonl`
- Exit 0 unconditionally (passive observation)

**Files:**
- `hooks/observe-pre-tool.sh` -- PreToolUse events
- `hooks/observe-post-tool.sh` -- PostToolUse events
- `hooks/observe-subagent-start.sh` -- SubagentStart events
- `hooks/observe-subagent-stop.sh` -- SubagentStop events

**Design note:** Four separate scripts (one per hook type) rather than a single script with type dispatch. Claude Code invokes hooks by type -- each `.claude/settings.json` hook entry maps to one script. A single script would require the hook type to be passed as an argument or inferred, adding fragility.

**Raw record shapes** (hook-type-specific fields before normalization):
- PreToolUse/PostToolUse: `{ ts, hook, session_id, tool, input, response_size?, response_snippet? }`
- SubagentStart: `{ ts, hook, session_id, agent_type, prompt_snippet }`
- SubagentStop: `{ ts, hook, session_id, agent_type }` — `agent_type` is always empty (platform constraint)

**Parser normalization**: The JSONL parser maps raw records into a uniform `ObservationRecord` struct. For SubagentStart/Stop, `agent_type` maps to the `tool` field and `prompt_snippet` maps to `input` (wrapped as `Value::String`). SubagentStop records have `tool: None` and `input: None` due to the empty `agent_type` platform constraint.

**Timestamp format**: All records use ISO-8601 with milliseconds: `YYYY-MM-DDTHH:MM:SS.mmmZ`. The parser converts to `u64` epoch milliseconds for ordering and gap analysis.

### 2. unimatrix-observe Crate (Analysis Engine)

New workspace crate at `crates/unimatrix-observe/`. Pure computation library with no dependencies on `unimatrix-store` or `unimatrix-server`.

**Responsibilities:**
- JSONL parsing into typed `ObservationRecord` structs
- Session file discovery and age computation
- Content-based feature attribution (session-to-feature mapping)
- Hotspot detection via extensible rule engine
- Universal and phase metric computation
- Report construction with evidence
- File lifecycle management (age checking, cleanup list)

**Internal modules:**
- `parser` -- JSONL line parsing, ISO-8601 millis timestamp conversion, SubagentStart/Stop field normalization (agent_type→tool, prompt_snippet→input)
- `attribution` -- Feature attribution logic with signal priority
- `detection` -- Hotspot rule trait + 3 initial implementations
- `metrics` -- MetricVector computation from analyzed records
- `report` -- RetrospectiveReport assembly
- `files` -- Session file discovery, age calculation, cleanup identification
- `types` -- Shared types (ObservationRecord, MetricVector, RetrospectiveReport, HotspotFinding, etc.)

### 3. OBSERVATION_METRICS Table (Storage)

14th redb table in `unimatrix-store`. Key-value: feature cycle string to bincode-serialized `MetricVector`.

**Responsibilities:**
- Persist per-feature metric vectors
- Support retrieval by feature cycle
- Support listing all stored metrics (for future col-002b baseline computation)

### 4. context_retrospective Tool Handler (Server Integration)

New MCP tool in `unimatrix-server`. Orchestrates the analysis pipeline.

**Responsibilities:**
- Accept `feature_cycle` parameter
- Invoke `unimatrix-observe` for scanning, attribution, analysis
- Store/retrieve MetricVector in OBSERVATION_METRICS
- Trigger file cleanup
- Return self-contained report

### 5. context_status Extension

Extends the existing `StatusReport` struct and formatting with observation health fields.

**Responsibilities:**
- Report observation file count, total size, oldest file age
- Report retrospected feature count
- Warn when files approach 60-day cleanup
- Execute cleanup during `maintain=true` calls

## Component Interactions

### Data Flow: Hook to Report

1. **Collection** (hook scripts): Claude Code invokes hook -> script reads stdin JSON -> extracts fields -> appends JSONL line to `~/.unimatrix/observation/{session_id}.jsonl`

2. **Trigger** (MCP tool): Agent calls `context_retrospective(feature_cycle: "col-002")` -> server handler receives request

3. **Analysis** (observe crate):
   - `files::discover_sessions(observation_dir)` -> list of session file paths with metadata
   - `parser::parse_session_file(path)` -> `Vec<ObservationRecord>` per file
   - `attribution::attribute_sessions(records, target_feature)` -> records attributed to target feature
   - `detection::detect_hotspots(records, rules)` -> `Vec<HotspotFinding>`
   - `metrics::compute_metric_vector(records, hotspots)` -> `MetricVector`
   - `report::build_report(metrics, hotspots, computed_at)` -> `RetrospectiveReport`

4. **Storage** (server): Store `MetricVector` in OBSERVATION_METRICS via `store.store_metrics(feature_cycle, metric_vector)`

5. **Cleanup** (observe crate + server): `files::identify_expired(observation_dir, 60_days)` -> server deletes listed files

6. **Response** (server): Format `RetrospectiveReport` as MCP tool response

### Data Flow: Status Extension

1. `context_status` handler calls `files::scan_observation_stats(observation_dir)` -> file count, total size, oldest age
2. Handler reads OBSERVATION_METRICS row count from store
3. Handler computes approaching-cleanup warnings
4. If `maintain=true`: call `files::identify_expired()` and delete

### Crate Dependency Graph

```
unimatrix-observe (new)
  depends on: serde, bincode (workspace deps only)
  NO dependency on: unimatrix-store, unimatrix-server, unimatrix-core

unimatrix-store
  depends on: redb, serde, bincode (unchanged)
  additions: OBSERVATION_METRICS table definition, store/get/list methods

unimatrix-server
  depends on: unimatrix-observe (new), unimatrix-store, unimatrix-core, ...
  additions: context_retrospective tool, status extension
```

## Technology Decisions

### ADR-001: Observe Crate Independence

The `unimatrix-observe` crate has no dependency on `unimatrix-store` or `unimatrix-server`. See `architecture/ADR-001-observe-crate-independence.md`.

### ADR-002: MetricVector Serialization Boundary

MetricVector serialization/deserialization uses bincode via serde, defined in `unimatrix-observe`. The store crate handles MetricVector as opaque `&[u8]`. See `architecture/ADR-002-metric-vector-serialization.md`.

### ADR-003: Four Separate Hook Scripts

One hook script per event type rather than a unified dispatcher. See `architecture/ADR-003-separate-hook-scripts.md`.

### ADR-004: Observation Directory as Constant

`~/.unimatrix/observation/` is a compile-time constant, not configurable. See `architecture/ADR-004-observation-dir-constant.md`.

## Integration Points

### Existing: unimatrix-store

- **Table addition**: `OBSERVATION_METRICS` added to `schema.rs` as 14th table. Follows OUTCOME_INDEX pattern.
- **Store::open**: Updated to open the new table in the initialization write transaction.
- **New methods**: `store_metrics(&self, feature_cycle: &str, data: &[u8])`, `get_metrics(&self, feature_cycle: &str) -> Option<Vec<u8>>`, `list_all_metrics(&self) -> Vec<(String, Vec<u8>)>` on a new `observation` module or added to existing `write.rs`/`read.rs`.

### Existing: unimatrix-server

- **New tool**: `context_retrospective` added to `tools.rs` tool router.
- **StatusReport extension**: New fields on `StatusReport` struct in `response.rs`.
- **Format extension**: `format_status_report` updated to render observation fields.
- **Error extension**: New `ServerError::ObservationError` variant for analysis failures.
- **Error code**: `ERROR_NO_OBSERVATION_DATA: ErrorCode = ErrorCode(-32010)`.
- **UnimatrixServer**: No new Arc fields needed -- observation directory is a constant, analysis is done via function calls to `unimatrix-observe`, store access uses existing `self.store`.

### Existing: Workspace Cargo.toml

- New `crates/unimatrix-observe` added to workspace members (automatic via `members = ["crates/*"]`).

### New: Hook Scripts

- Scripts live in project repo (e.g., `hooks/` directory at repo root or documented path).
- User registers them in `.claude/settings.json` manually.
- No runtime integration with the Rust codebase.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `OBSERVATION_METRICS` | `TableDefinition<&str, &[u8]>` | `unimatrix-store/src/schema.rs` |
| `Store::store_metrics` | `fn(&self, feature_cycle: &str, data: &[u8]) -> Result<()>` | `unimatrix-store` (new) |
| `Store::get_metrics` | `fn(&self, feature_cycle: &str) -> Result<Option<Vec<u8>>>` | `unimatrix-store` (new) |
| `Store::list_all_metrics` | `fn(&self) -> Result<Vec<(String, Vec<u8>)>>` | `unimatrix-store` (new) |
| `observe::parse_session_file` | `fn(path: &Path) -> Result<Vec<ObservationRecord>>` | `unimatrix-observe` (new) |
| `observe::discover_sessions` | `fn(dir: &Path) -> Result<Vec<SessionFile>>` | `unimatrix-observe` (new) |
| `observe::attribute_sessions` | `fn(sessions: &[ParsedSession], feature: &str) -> Vec<ObservationRecord>` | `unimatrix-observe` (new) |
| `observe::detect_hotspots` | `fn(records: &[ObservationRecord], rules: &[Box<dyn DetectionRule>]) -> Vec<HotspotFinding>` | `unimatrix-observe` (new) |
| `observe::compute_metric_vector` | `fn(records: &[ObservationRecord], hotspots: &[HotspotFinding], computed_at: u64) -> MetricVector` | `unimatrix-observe` (new) |
| `observe::build_report` | `fn(metrics: &MetricVector, hotspots: Vec<HotspotFinding>) -> RetrospectiveReport` | `unimatrix-observe` (new) |
| `observe::identify_expired` | `fn(dir: &Path, max_age_secs: u64) -> Result<Vec<PathBuf>>` | `unimatrix-observe` (new) |
| `observe::scan_observation_stats` | `fn(dir: &Path) -> Result<ObservationStats>` | `unimatrix-observe` (new) |
| `StatusReport` (extended) | 5 new fields: `observation_file_count`, `observation_total_size_bytes`, `observation_oldest_file_days`, `observation_approaching_cleanup`, `retrospected_feature_count` | `unimatrix-server/src/response.rs` |
| `ServerError::ObservationError` | `ObservationError(String)` | `unimatrix-server/src/error.rs` |
| `ERROR_NO_OBSERVATION_DATA` | `ErrorCode(-32010)` | `unimatrix-server/src/error.rs` |
| `RetrospectiveParams` | `struct { feature_cycle: String, agent_id: Option<String> }` | `unimatrix-server/src/tools.rs` |
| `ObservationRecord` | `struct { ts: u64, hook: HookType, session_id: String, tool: Option<String>, input: Option<serde_json::Value>, response_size: Option<u64>, response_snippet: Option<String> }` | `unimatrix-observe/src/types.rs` |
| `MetricVector` | `struct { computed_at: u64, universal: UniversalMetrics, phases: BTreeMap<String, PhaseMetrics> }` | `unimatrix-observe/src/types.rs` |
| `HotspotFinding` | `struct { category: HotspotCategory, severity: Severity, claim: String, measured: f64, threshold: f64, evidence: Vec<EvidenceRecord> }` | `unimatrix-observe/src/types.rs` |
| `RetrospectiveReport` | `struct { feature_cycle: String, metrics: MetricVector, hotspots: Vec<HotspotFinding>, session_count: usize, total_records: usize, is_cached: bool }` | `unimatrix-observe/src/types.rs` |
| `DetectionRule` trait | `trait { fn name(&self) -> &str; fn category(&self) -> HotspotCategory; fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>; }` | `unimatrix-observe/src/detection.rs` |
