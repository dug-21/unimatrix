# Specification: col-002 Retrospective Pipeline

## Objective

Build the observation pipeline for Unimatrix: hook-based telemetry collection from Claude Code sessions, a pure-Rust analysis engine that parses telemetry, attributes sessions to features, runs rule-based hotspot detection, and computes structured metric vectors. Expose analysis via a `context_retrospective` MCP tool that returns self-contained reports. Store per-feature metric vectors in a dedicated redb table. Ship 3 detection rules to validate the pipeline end-to-end.

## Functional Requirements

### FR-01: Hook Script Collection

- FR-01.1: Four shell scripts capture PreToolUse, PostToolUse, SubagentStart, SubagentStop events
- FR-01.2: Each script reads JSON from stdin, extracts session_id, and appends a normalized observation record to `~/.unimatrix/observation/{session_id}.jsonl`
- FR-01.3: Scripts create the observation directory if it does not exist
- FR-01.4: Scripts exit 0 unconditionally, regardless of errors
- FR-01.5: Response snippet in PostToolUse records is truncated to 500 characters maximum
- FR-01.6: Observation record schema varies by hook type:
  - **PreToolUse/PostToolUse**: `{ ts, hook, session_id, tool, input, response_size?, response_snippet? }` — `tool` is the tool name (e.g., `"Read"`, `"Bash"`, `"mcp__unimatrix__context_store"`), `input` is the tool input object
  - **SubagentStart**: `{ ts, hook, session_id, agent_type, prompt_snippet }` — `agent_type` is the spawned agent type (e.g., `"Explore"`, `"uni-scrum-master"`), `prompt_snippet` is the task prompt (may be empty)
  - **SubagentStop**: `{ ts, hook, session_id, agent_type }` — `agent_type` is empty string in practice (platform constraint: Claude Code does not populate this field for SubagentStop events)

### FR-02: JSONL Parsing

- FR-02.1: Parse JSONL files line-by-line into typed `ObservationRecord` structs
- FR-02.2: Skip malformed lines without failing the entire file parse
- FR-02.3: Parse ISO-8601 timestamps in the format `YYYY-MM-DDTHH:MM:SS.mmmZ` (UTC, with milliseconds, the format emitted by hook scripts)
- FR-02.4: Convert parsed timestamps to Unix epoch milliseconds (u64) for comparison and ordering
- FR-02.5: SubagentStart/Stop field mapping: the parser normalizes `agent_type` → `tool` field and `prompt_snippet` → `input` field (wrapped as `serde_json::Value::String`) so all hook types produce a uniform `ObservationRecord`. SubagentStop records will have `tool` as `None` (empty `agent_type` from platform) and `input` as `None`.

### FR-03: Session File Management

- FR-03.1: Discover all `.jsonl` files in the observation directory
- FR-03.2: Compute file age from filesystem metadata (modified time)
- FR-03.3: Identify files older than a configurable threshold (default 60 days) for cleanup
- FR-03.4: Compute aggregate statistics: file count, total size in bytes, oldest file age in days

### FR-04: Feature Attribution

- FR-04.1: Walk records in timestamp order within each session
- FR-04.2: Track "current feature" -- when a new feature ID appears, switch attribution
- FR-04.3: Attribution signals (priority order): (a) file paths matching `product/features/{id}/`, (b) task subjects containing feature IDs, (c) git checkout commands with `feature/{id}`
- FR-04.4: Records before any feature ID appears are attributed to the first feature found in that session
- FR-04.5: Multi-feature sessions are partitioned sequentially at switch points
- FR-04.6: Sessions with no feature signals are unattributable and excluded from retrospective
- FR-04.7: All sessions containing records attributed to a target feature are included in that feature's retrospective

### FR-05: Hotspot Detection Framework

- FR-05.1: Define a `DetectionRule` trait with methods: `name() -> &str`, `category() -> HotspotCategory`, `detect(records) -> Vec<HotspotFinding>`
- FR-05.2: Support four hotspot categories: Agent, Friction, Session, Scope
- FR-05.3: Each finding includes: category, severity (Info/Warning/Critical), claim text, measured value, threshold, and evidence records
- FR-05.4: The engine iterates all registered rules and collects findings

### FR-06: Detection Rules (3 shipped)

- FR-06.1: **Permission Retries** (Friction): Count PreToolUse events minus PostToolUse events per tool name. Threshold: >2 net retries for any tool. Evidence includes specific tool names and retry counts.
- FR-06.2: **Session Timeout** (Session): Detect timestamp gaps >2 hours between consecutive records within a session. Threshold: any occurrence. Evidence includes gap start/end timestamps and duration.
- FR-06.3: **Sleep Workarounds** (Friction): Match `sleep` command in Bash tool input records. Threshold: any occurrence. Evidence includes the specific sleep commands found.

### FR-07: Metric Computation

- FR-07.1: Compute all universal metric fields from analyzed records (not just fields with matching hotspot rules)
- FR-07.2: Universal metrics include: total_tool_calls, total_duration_secs, session_count, permission_friction_events, sleep_workaround_count, agent_hotspot_count, friction_hotspot_count, session_hotspot_count, scope_hotspot_count, and additional fields as specified in SCOPE.md
- FR-07.3: Phase metrics keyed by phase name extracted from task subject prefixes -- split on first `:`, trim prefix
- FR-07.4: Per-phase metrics include: duration_secs, tool_call_count
- FR-07.5: MetricVector includes `computed_at` timestamp (Unix epoch seconds)
- FR-07.6: MetricVector is serializable/deserializable via bincode with `#[serde(default)]` for forward compatibility

### FR-08: Retrospective Report

- FR-08.1: Report includes feature_cycle, session_count, total_records, MetricVector, and hotspot findings
- FR-08.2: Report is self-contained -- all data needed for LLM reasoning is included
- FR-08.3: Report includes `is_cached` flag indicating whether this is from a previous run

### FR-09: context_retrospective MCP Tool

- FR-09.1: Accept required `feature_cycle` parameter (string)
- FR-09.2: Accept optional `agent_id` parameter for identity/capability checks
- FR-09.3: Scan observation directory, attribute sessions, run analysis
- FR-09.4: Store computed MetricVector in OBSERVATION_METRICS table
- FR-09.5: Return complete RetrospectiveReport as tool response
- FR-09.6: When no new session data exists but a stored MetricVector is found, return cached result with `is_cached: true` and the stored `computed_at` timestamp
- FR-09.7: When no session data and no stored MetricVector exist, return descriptive error
- FR-09.8: Clean up session files older than 60 days during execution

### FR-10: OBSERVATION_METRICS Table

- FR-10.1: Table definition: `TableDefinition<&str, &[u8]>` named `"observation_metrics"`
- FR-10.2: Created during `Store::open` alongside existing 13 tables (becomes 14th table)
- FR-10.3: Store provides `store_metrics(feature_cycle, data)` method
- FR-10.4: Store provides `get_metrics(feature_cycle)` method returning `Option<Vec<u8>>`
- FR-10.5: Store provides `list_all_metrics()` method returning all stored feature-metric pairs
- FR-10.6: Schema version remains 3 (no EntryRecord changes)

### FR-11: context_status Extension

- FR-11.1: StatusReport extended with: `observation_file_count: u64`, `observation_total_size_bytes: u64`, `observation_oldest_file_days: u64`, `observation_approaching_cleanup: Vec<String>`, `retrospected_feature_count: u64`
- FR-11.2: Status formatting includes observation section in all formats (summary, markdown, json)
- FR-11.3: When `maintain=true`: execute 60-day file cleanup as part of maintenance

## Non-Functional Requirements

### NFR-01: Performance
- JSONL parsing should handle files with 10,000+ records without excessive memory usage (stream line-by-line, do not load entire file into memory)
- Retrospective analysis for a typical feature (5 sessions, ~5,000 total records) should complete in under 5 seconds

### NFR-02: Reliability
- Hook scripts must never block tool execution (exit 0 always)
- Malformed JSONL lines are skipped, not fatal
- Missing observation directory is handled gracefully (empty results, not errors)

### NFR-03: Compatibility
- `#![forbid(unsafe_code)]` on `unimatrix-observe`
- Edition 2024, MSRV 1.89
- No new external crate dependencies beyond workspace (serde, bincode)
- All existing tests pass without regression

### NFR-04: Extensibility
- Adding a new detection rule requires implementing the `DetectionRule` trait only, not modifying engine internals
- MetricVector uses `#[serde(default)]` for forward-compatible field addition

## Acceptance Criteria

| AC-ID | Description | Verification Method |
|-------|-------------|---------------------|
| AC-01 | Hook scripts capture PreToolUse, PostToolUse, SubagentStart, SubagentStop | test: synthetic hook input piped to scripts, verify JSONL output |
| AC-02 | Records route to `~/.unimatrix/observation/{session_id}.jsonl` based on session_id | test: verify file naming from synthetic input |
| AC-03 | Hook scripts exit 0 always | test: pipe invalid JSON, verify exit code |
| AC-04 | Record schema includes ts, hook, session_id, tool, input, response_size, response_snippet | test: parse output JSONL line, verify all fields |
| AC-05 | Response snippet truncated to bound per-record size | test: large response input, verify snippet length |
| AC-06 | `unimatrix-observe` parses JSONL session files into typed record structs | test: unit test with sample JSONL |
| AC-07 | Sequential feature attribution walks records in timestamp order | test: multi-feature JSONL, verify partition boundaries |
| AC-08 | Multi-session features: all attributed sessions included | test: 3 sessions touching same feature, all records present |
| AC-09 | Multi-feature sessions: records partitioned by feature switch points | test: session with 2 features, verify split |
| AC-10 | Hotspot framework supports registering rules by category | test: register custom rule, verify it runs |
| AC-11 | Permission retries rule: Pre-Post differential per tool, threshold >2 | test: synthetic records with known retry counts |
| AC-12 | Session timeout rule: gap >2 hours, any occurrence | test: records with 3-hour gap, verify detection |
| AC-13 | Sleep workarounds rule: regex match in Bash input | test: Bash records with sleep commands |
| AC-14 | Each hotspot includes category, severity, claim, measured, threshold, evidence | test: verify HotspotFinding struct fields |
| AC-15 | MetricVector contains universal metrics and phase metrics | test: compute from synthetic records, verify both sections |
| AC-16 | Phase names extracted from task subject prefix (split on first `:`) | test: "3a: Pseudocode" -> phase "3a" |
| AC-17 | `unimatrix-observe` has no dependency on store or server | grep: Cargo.toml dependency check |
| AC-18 | Hotspot framework extensible without engine modification | test: add rule implementing trait, engine runs it |
| AC-19 | `context_retrospective` accepts `feature_cycle` parameter | test: integration test with valid/invalid params |
| AC-20 | Tool scans, attributes, analyzes, returns report | test: e2e with synthetic JSONL |
| AC-21 | Report includes metrics and hotspot findings with evidence | test: verify report structure |
| AC-22 | Report is self-contained | test: verify no round-trip data needed |
| AC-23 | Tool stores MetricVector in OBSERVATION_METRICS | test: call tool, verify table has entry |
| AC-24 | Tool cleans up files older than 60 days | test: create old files, call tool, verify deleted |
| AC-25 | Error when no data and no stored MetricVector | test: empty dir, verify error response |
| AC-26 | Cached result when no new data but stored MetricVector exists | test: store MV, call tool again, verify is_cached |
| AC-27 | MetricVector includes computed_at timestamp | test: verify field after computation |
| AC-28 | OBSERVATION_METRICS table exists with correct schema | test: Store::open, verify table accessible |
| AC-29 | Table created during Store::open (14th table) | test: open store, count tables |
| AC-30 | Store provides store_metrics, get_metrics, list_all_metrics | test: CRUD operations |
| AC-31 | Schema version remains 3 | test: verify no migration triggered |
| AC-32 | MetricVector serializable/deserializable via bincode | test: roundtrip |
| AC-33 | Files >60 days auto-removed during retrospective or maintain | test: create aged files, trigger cleanup |
| AC-34 | context_status reports observation file count, size, oldest age | test: create files, check status output |
| AC-35 | context_status warns when files approach 60-day threshold | test: files at 45+ days, check warning |
| AC-36 | `#![forbid(unsafe_code)]` on unimatrix-observe | grep: crate root |
| AC-37 | No new crate dependencies beyond workspace | grep: Cargo.toml |
| AC-38 | All existing tests pass | test: cargo test --workspace |
| AC-39 | Unit tests cover parsing, attribution, detection rules, serialization, file age | test: specific test modules |
| AC-40 | Integration tests cover e2e retrospective, status fields, OBSERVATION_METRICS CRUD | test: integration test suite |
| AC-41 | Tests build on existing workspace fixtures | test: reuse test_helpers patterns |

## Domain Models

### ObservationRecord
The fundamental data unit -- a single normalized event from a Claude Code hook.

| Field | Type | Description |
|-------|------|-------------|
| `ts` | `u64` | Unix epoch milliseconds, parsed from ISO-8601 `YYYY-MM-DDTHH:MM:SS.mmmZ` |
| `hook` | `HookType` | PreToolUse, PostToolUse, SubagentStart, SubagentStop |
| `session_id` | `String` | Claude Code session identifier |
| `tool` | `Option<String>` | Tool name for PreToolUse/PostToolUse (e.g., `"Read"`, `"Bash"`); mapped from `agent_type` for SubagentStart (e.g., `"Explore"`); `None` for SubagentStop (platform does not populate `agent_type`) |
| `input` | `Option<serde_json::Value>` | Tool input object for PreToolUse/PostToolUse; mapped from `prompt_snippet` (as String value) for SubagentStart; `None` for SubagentStop |
| `response_size` | `Option<u64>` | PostToolUse response byte count; `None` for other hook types |
| `response_snippet` | `Option<String>` | First 500 chars of PostToolUse response (JSON string, not plain text); `None` for other hook types |

### MetricVector
Structured numeric telemetry for one retrospected feature.

| Section | Type | Description |
|---------|------|-------------|
| `computed_at` | `u64` | When this vector was computed (Unix seconds) |
| `universal` | `UniversalMetrics` | Fixed fields applicable to any agentic workflow |
| `phases` | `BTreeMap<String, PhaseMetrics>` | Dynamic map keyed by discovered phase names |

### HotspotFinding
A single detection rule finding with supporting evidence.

| Field | Type | Description |
|-------|------|-------------|
| `category` | `HotspotCategory` | Agent, Friction, Session, Scope |
| `severity` | `Severity` | Info, Warning, Critical |
| `rule_name` | `String` | The rule that produced this finding |
| `claim` | `String` | Human-readable statement of the finding |
| `measured` | `f64` | The measured value |
| `threshold` | `f64` | The threshold that was exceeded |
| `evidence` | `Vec<EvidenceRecord>` | Concrete tool call data supporting the claim |

### RetrospectiveReport
Complete analysis output returned by context_retrospective.

| Field | Type | Description |
|-------|------|-------------|
| `feature_cycle` | `String` | The feature analyzed |
| `session_count` | `usize` | Number of sessions analyzed |
| `total_records` | `usize` | Total observation records processed |
| `metrics` | `MetricVector` | Computed metric vector |
| `hotspots` | `Vec<HotspotFinding>` | All hotspot findings |
| `is_cached` | `bool` | Whether this is from a previous computation |

### HookType (enum)
```
PreToolUse | PostToolUse | SubagentStart | SubagentStop
```

### HotspotCategory (enum)
```
Agent | Friction | Session | Scope
```

### Severity (enum)
```
Info | Warning | Critical
```

## User Workflows

### Workflow 1: Running a Retrospective

1. Agent (or human) completes a feature delivery
2. Agent calls `context_retrospective(feature_cycle: "col-002")`
3. Unimatrix scans observation files, finds sessions attributed to col-002
4. Analysis runs: hotspot detection + metric computation
5. Report returned with findings and metrics
6. Agent/human discusses findings and takes action (store conventions, edit protocols)

### Workflow 2: Checking Observation Health

1. Agent calls `context_status()`
2. StatusReport includes observation section: file count, size, oldest file age
3. If files approach 60-day threshold, warnings are shown
4. Agent calls `context_status(maintain: true)` to trigger cleanup

### Workflow 3: Setting Up Hooks

1. User reads documentation for hook script installation
2. User copies hook scripts to a known location
3. User adds hook entries to `.claude/settings.json`
4. Claude Code begins invoking hooks on tool calls
5. JSONL files accumulate in `~/.unimatrix/observation/`

## Constraints

- No new external crate dependencies beyond workspace (serde, bincode, std)
- `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89
- `unimatrix-observe` has zero dependency on `unimatrix-store` or `unimatrix-server`
- Schema version remains 3 (new table, no EntryRecord changes)
- Backward compatible -- no changes to existing tools, entries, or schema
- Hook scripts are shell scripts, not Rust -- tested via integration tests with synthetic JSONL
- Hook installation is manual (documented, not automated)
- Observation directory is a constant (`~/.unimatrix/observation/`), not configurable
- Platform constraint: SubagentStop events have empty `agent_type` — Claude Code does not populate this field. SubagentStop records are useful only for timestamp bracketing (pairing with SubagentStart for lifespan measurement), not for agent identification

## Dependencies

- `serde` (workspace) -- serialization/deserialization
- `bincode` (workspace) -- binary encoding for MetricVector
- `redb` (workspace, via unimatrix-store) -- OBSERVATION_METRICS table
- `rmcp` (existing server dep) -- MCP tool registration
- `schemars` (existing server dep) -- JSON schema for tool params
- `serde_json` (existing server dep) -- JSON parsing for observation records
- `std::fs`, `std::time`, `std::io` -- file operations and timestamps

## NOT in Scope

- Full detection library (21 rules) -- col-002b
- Historical baseline comparison across features -- col-002b
- Threshold convergence from dismissed hotspot feedback -- future
- Compound signal detection -- future
- Auto-knowledge extraction from telemetry -- col-005
- `/retrospective` Claude Code skill -- separate prompt file
- Per-agent attribution within sessions -- platform limitation
- LLM in the analysis pipeline -- all detection is rule-based
- Streaming or background analysis -- batch only
- Changes to existing entry pipeline -- metric vectors use dedicated table
- Multi-project support -- single project assumed
