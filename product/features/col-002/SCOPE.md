# col-002: Retrospective Pipeline

## Problem Statement

Unimatrix accumulates knowledge and tracks outcomes, but has no way to observe *how agents actually work*. Without telemetry, anti-patterns are invisible — a monolithic agent consuming 190KB of context before its first write, 10 permission retries on a single tool, a 3-hour session timeout followed by 123KB of cold-restart overhead. These problems only surface when a human manually inspects tool call logs, which nobody does.

The ASS-013 research spike analyzed 1,180 tool call records from one feature cycle (crt-006) and discovered 15+ detectable anti-patterns — all rule-based, no model required. The highest-value application is a structured retrospective capability: Unimatrix observes agent sessions via Claude Code hooks, runs rule-based hotspot detection against the telemetry, and presents opinionated findings to the LLM via an MCP tool. The LLM and human discuss the findings and take action through existing channels (store conventions, edit protocols, create issues).

Today, observation data exists only as a research prototype: a single `observe.sh` hook capturing selected tools to a monolithic file. There is no production collection infrastructure, no analysis engine, no way to trigger a retrospective, and no persistent storage for the metrics that would allow Unimatrix to learn what "normal" looks like for a project.

col-002 builds the observation pipeline: collection, attribution, analysis framework, reporting, and metric storage. It ships with a minimal set of detection rules to validate the pipeline end-to-end. col-002b fills in the full detection library and baseline comparison capabilities.

## Goals

### 1. Production Hook Infrastructure

Ship Claude Code hook scripts that capture all tool call and subagent lifecycle events to per-session JSONL files.

- **Events captured**: PreToolUse, PostToolUse, SubagentStart, SubagentStop
- **File routing**: Each record appends to `~/.unimatrix/observation/{session_id}.jsonl` based on the `session_id` field in the hook input
- **Record schema**: `{ ts, hook, session_id, tool, input, response_size, response_snippet }` — no enrichment, no feature attribution at collection time
- **Passive observation**: Hooks cannot block or modify tool execution. Exit 0 always.

### 2. Analysis Engine (`unimatrix-observe` crate)

A new Rust crate that parses JSONL telemetry, attributes sessions to features, runs multi-pass hotspot detection, and produces structured retrospective reports.

**Session-to-feature attribution**: Content-based, computed at analysis time by walking records in timestamp order. Track the "current feature" — when a new feature ID appears in a record (file paths referencing `product/features/{id}/`, task subjects containing feature IDs, git checkout commands), that becomes the current feature for all subsequent records until the next switch. Records before any feature ID appears are attributed to the first feature (orientation work). No git branch stamping (branch is global, breaks with concurrent sessions).

**Hotspot detection framework**: Extensible rule engine with four hotspot categories (agent, friction, session, scope). Each rule implements a common interface: threshold configuration, record scanning, metric computation, and evidence collection. col-002 ships with 3 pipeline-proof rules; col-002b fills in the remaining ~18 rules.

**col-002 detection rules** (representative rules to validate the framework across friction and session categories; col-002b covers all 4 categories completely):
- **Friction: Permission retries** — PreToolUse count minus PostToolUse count per tool. Threshold: >2 retries. Simplest rule, highest signal.
- **Session: Session timeout** — Gap >2 hours in timestamp sequence within a session. Threshold: any occurrence. Simple timestamp analysis.
- **Friction: Sleep workarounds** — Any `sleep` command in Bash tool input. Threshold: any occurrence. Simple regex match.

**Bootstrapped thresholds**: Each detection rule ships with a starting threshold derived from context engineering principles and the ASS-013 crt-006 baseline. These represent "good agentic development practice" defaults — universal starting points that a future convergence phase will adapt to project-specific norms.

**Hotspot output**: Each hotspot is a claim with evidence — not "permission retries exceeded threshold" but "permission retries: 10 across context_store (Pre: 18, Post: 8). Evidence: 'Adaptive Embedding Pipeline' — 4 attempts before success, 'Session activity capture hooks' — 6 attempts before success." Concrete examples that give the LLM enough context to reason about root causes and propose improvements.

### 3. `context_retrospective` MCP Tool

A new tool on the Unimatrix MCP server that triggers retrospective analysis for a feature and returns a self-contained report.

**Input**: `feature_cycle` (required) — the feature ID to retrospect (e.g., `crt-006`, `col-002`).

**Behavior**:
1. Scan `~/.unimatrix/observation/` for session files
2. Attribute sessions to the requested feature via sequential content-based scanning
3. If attributable session data exists:
   a. Load and merge records from attributed sessions, ordered by timestamp
   b. Run hotspot detection with current thresholds
   c. Compute per-feature metric vector (universal metrics + phase metrics)
   d. Store metric vector (with `computed_at` timestamp) in OBSERVATION_METRICS table
   e. Return the complete retrospective report
4. If no new session data exists but a previous MetricVector is stored:
   a. Return the stored MetricVector with `computed_at` timestamp and a note indicating this was a previous run
   b. Do not overwrite or re-analyze
5. If no session data and no stored MetricVector: return an error
6. Clean up session files older than 60 days

**Output**: A self-contained report including:
- Feature-level metrics (universal + per-phase)
- Hotspot findings with severity, claim, measured value, threshold, and supporting evidence records
- The report gives the LLM everything it needs — no round-trip back to Unimatrix required
- Historical baseline comparison (metric table of previous features) is deferred to col-002b

### 4. Per-Feature Metric Storage (OBSERVATION_METRICS table)

A dedicated redb table for storing structured metric vectors — one row per retrospected feature. This is data, not knowledge. It does not enter the entry pipeline (no embedding, no HNSW, no confidence, no contradiction detection).

**Table schema**: `&str -> Vec<u8>` — feature cycle string to bincode-serialized `MetricVector`.

**MetricVector structure — two layers**:

**Universal metrics** (fixed fields, apply to any agentic workflow regardless of project-specific phases):
- Total tool calls, total duration, session count
- Search miss rate, edit bloat (total KB, ratio to all response data)
- Permission friction events, Bash-for-search count
- Cold restart events, coordinator respawn count
- Parallel call rate, thinking time distribution
- Context load before first write (KB), total context loaded (KB)
- Post-completion work (%), follow-up issues created
- Knowledge entries stored, sleep workaround count
- Agent hotspot count, friction hotspot count, session hotspot count, scope hotspot count

**Phase metrics** (dynamic map, adapts to whatever workflow the project defines):
- Keyed by phase name extracted from task subject prefixes using `{phase-id}: {description}` convention (e.g., `3a`, `3b`, `gate-3c` — or `design`, `review`, `deploy` in another project)
- Per-phase: duration (minutes), tool call count, compile cycles, activity profile (explore/produce/execute ratios)
- Phase names are locally consistent within a project — baselines accumulate naturally by phase name
- No NLP or LLM required — the parser splits on the first `:` in task subjects

**Important**: The analysis engine computes ALL universal metric fields during retrospective analysis, not just the fields that have matching hotspot rules. col-002 ships 3 hotspot rules but the MetricVector captures the full picture — search miss rate, edit bloat, cold restart count, parallel call rate, etc. are all computed and stored. Hotspot rules and MetricVector computation are separate concerns: rules trigger findings in the report, MetricVector fields accumulate data for col-002b's baseline comparison and future threshold convergence.

**Baseline computation** (col-002b): At retrospective time, scan all stored MetricVectors to compute per-metric mean and standard deviation. Present as a comparison table alongside the current feature's metrics. col-002 stores the MetricVectors; col-002b adds the comparison logic.

### 5. Observation File Lifecycle

**Collection**: Hook scripts append to `~/.unimatrix/observation/{session_id}.jsonl` during active sessions.

**Retention**: Session files remain in the observation directory. No archival, no moving to feature directories. Files are re-scannable for re-analysis at any time.

**Automatic cleanup**: Files older than 60 days are deleted automatically. The metric vector in Unimatrix is the permanent analytical record — it survives file cleanup.

**Safety valve**: `context_status` reports:
- Total observation file count and storage size
- Age of oldest session file
- Warning when files approach the 60-day cleanup threshold

**Cleanup trigger**: During `context_retrospective` execution (check for and remove files older than 60 days). Alternatively during `context_status` with `maintain=true`, consistent with crt-005 coherence maintenance patterns.

### 6. `context_status` Extension

Extend `StatusReport` with observation infrastructure status:
- Observation file count, total size, oldest file age
- Retrospected feature count (rows in OBSERVATION_METRICS)
- Warning flags for files approaching 60-day cleanup

## Non-Goals

- **No full detection library.** col-002 ships 3 detection rules to validate the pipeline. The remaining ~18 rules (agent context load, lifespan, file breadth, re-read rate, mutation spread, compile cycles, edit bloat, search-via-Bash, output parsing struggle, cold restart, coordinator respawns, post-completion work, rework events, source file count, design artifact count, ADR count, post-delivery issues, phase duration outlier) are col-002b scope.
- **No baseline comparison.** Historical mean/stddev computation across accumulated MetricVectors and the comparison table in the retrospective report are col-002b. col-002 stores MetricVectors but does not compare them.
- **No threshold convergence.** col-002 ships with bootstrapped thresholds only. Adapting thresholds to project-specific norms (dismissed hotspot feedback, empirical adjustment toward mean+1.5σ) is a future follow-on. The data model supports it — metric vectors store the raw measurements, and the MetricVector struct is extensible for dismissed-hotspot annotations.
- **No compound signal detection.** Identifying correlated metrics requires accumulated data and LLM reasoning. col-002 provides the metric table that makes this possible. Compound signal promotion is future work.
- **No auto-knowledge extraction.** That is col-005. col-002 provides the observation data and metric accumulation that col-005 depends on.
- **No `/retrospective` Claude Code skill.** The MCP tool (`context_retrospective`) is the interface. A skill that coaches the LLM on interpreting the report and using it effectively can be added separately — it's a prompt file, not infrastructure.
- **No per-agent attribution.** Platform constraint: all subagent tool calls share the parent session_id. Hotspots operate at session/feature-cycle granularity. Per-agent attribution via timestamp bracketing or tool-pattern inference is heuristic only and not a requirement for col-002.
- **No LLM in the analysis pipeline.** All hotspot detection is rule-based. The LLM participates in the *conversation* after receiving the report — interpreting findings, discussing with the human, taking action. The analysis engine itself requires no model inference.
- **No streaming or background analysis.** Retrospective is batch, on-demand. No background processing, no continuous monitoring. Analysis runs when `context_retrospective` is called.
- **No changes to existing entry pipeline.** Metric vectors are stored in a dedicated table, not as entries. No new categories, no embedding bypass logic, no pipeline branching.
- **No multi-project support.** v1 assumes a single project. The data model (per-feature metrics, phase names from telemetry) does not preclude multi-project, but there is no project-scoping mechanism.

## Background Research

### ASS-013: Tool Call Observation Analysis

Full research in `product/research/ass-013/`. Key artifacts:

- **initial-findings.md**: First-pass analysis of 1,180 records across 4 sessions. Established tool distribution, file access patterns, workflow choreography. Discovered monolithic agent anti-pattern, design-to-delivery handoff cost.
- **deep-findings.md**: 15+ specific detection rules with metrics, methods, and starting thresholds. Edit bloat (44% of context load), permission friction, Bash compliance, cold restart cost, search miss rate (32%), thinking time distribution (97% reasoning), activity profiles by phase, agent warmup patterns.
- **retrospective-design.md**: Full retrospective architecture — 4 layers, 4 hotspot categories, threshold convergence model, data lifecycle, detection tier summary. 6/8 detection categories need zero model.
- **data-pipeline.md**: Pipeline design — per-session JSONL, content-based feature attribution, batch retrospective flow, platform constraints.
- **compound-signals.md**: Metric table design, LLM-driven correlation reasoning, promoted compound signal lifecycle. The feature metric table concept that drives the MetricVector design.
- **auto-knowledge.md**: Three extraction tiers (structural/procedural/dependency). Deferred to col-005.

### Existing Infrastructure

**Claude Code hooks**: The hook API provides PreToolUse, PostToolUse, SubagentStart, SubagentStop events. Each carries `session_id`, `tool_name`/`agent_type`, `tool_input`, and (for PostToolUse) `tool_response` with response_size. The research prototype (`product/research/ass-011/hooks/observe.sh`) captures selected tools to a spool directory — col-002 replaces this with production hooks capturing all events.

**col-001 Outcome Tracking**: Provides structured outcome entries with OUTCOME_INDEX. col-002's retrospective report can reference outcome data when analyzing feature results, but does not depend on or modify col-001 infrastructure.

**crt-005 Coherence Gate**: Established the `maintain=true` pattern on `context_status` for opt-in maintenance operations. col-002 uses this same pattern for observation file cleanup.

### Platform Constraints

These are limitations of the Claude Code hook API that the analysis engine must work within:

| Constraint | Impact | col-002 Handling |
|------------|--------|------------------|
| All subagent tool calls share parent `session_id` | Cannot attribute tool calls to specific agents within a session | Hotspots operate at session/feature-cycle granularity |
| Nested subagent types invisible (26/31 SubagentStop have empty `agent_type`) | Worker agents are anonymous | Infer role from tool patterns where useful, but don't depend on it |
| No SubagentStart for nested children | Only top-level spawns emit start events | Use SubagentStop timestamps for bracketing where possible |
| Git branch is global, not per-session | Branch field would mis-attribute in multi-session workflows | Use content-based attribution from tool inputs |
| Edit responses echo entire file | 44% of response data is platform echo-back | Discount edit responses in context load calculations |

### Detection Rules and Starting Thresholds

Each rule is derived from ASS-013 findings. Starting thresholds represent "good agentic development practice" — universal defaults that a future convergence phase will adapt to project norms.

**col-002 ships 3 rules** to validate the pipeline framework. The full rule set (21 rules across 4 categories) is documented here as reference for both col-002 and col-002b.

#### col-002 Rules (pipeline proof)

| Category | Signal | Metric | Starting Threshold | Detection |
|----------|--------|--------|--------------------|-----------|
| Friction | Permission retries | PreToolUse count - PostToolUse count per tool | >2 retries same tool | Count differential by tool name |
| Session | Session timeout | Gap >2 hours within a session | Any occurrence | Timestamp gap analysis |
| Friction | Sleep workarounds | Any `sleep` command in Bash | Any occurrence | Regex match |

#### col-002b Rules (full detection library)

**Agent Hotspots**

| Signal | Metric | Starting Threshold | Detection |
|--------|--------|--------------------|-----------|
| Context load | KB read before first Write/Edit | >100 KB | Sum Read response_size until first Write/Edit |
| Lifespan | SubagentStart → SubagentStop duration | >45 min | Timestamp diff |
| File breadth | Distinct files touched (read + write) | >20 files | Unique file paths in tool inputs |
| Re-read rate | Files read 2+ times within agent window | >3 re-reads | File path frequency count |
| Mutation spread | Distinct files written/edited | >10 files | Unique Write/Edit target paths |
| Compile cycles | cargo check/test invocations per phase | >6 per phase | Regex match Bash commands |
| Edit bloat | Average edit response size | >50 KB avg | PostToolUse response_size for Edit tool |

**Friction Hotspots**

| Signal | Metric | Starting Threshold | Detection |
|--------|--------|--------------------|-----------|
| Search-via-Bash | Bash commands matching find/grep/rg patterns | >5% of Bash calls | Regex on Bash command input |
| Output parsing struggle | Same cargo command with different pipe filters within 3 min | >2 filter variations | Command similarity + timestamp proximity |

**Session Hotspots**

| Signal | Metric | Starting Threshold | Detection |
|--------|--------|--------------------|-----------|
| Cold restart | Gap >30 min + burst of reads to already-read files | Any occurrence | Timestamp gap + file path intersection |
| Coordinator respawns | SubagentStart count for coordinator types | >3 per feature | Count by agent_type |
| Post-completion work | Tool calls after final task completion / total | >8% | TaskUpdate completion timestamp as boundary |
| Rework events | Task status completed → in_progress | Any occurrence | TaskUpdate state transition |

**Scope Hotspots**

| Signal | Metric | Starting Threshold | Detection |
|--------|--------|--------------------|-----------|
| Source file count | New *.rs files created via Write | >6 files | Write tool path filter |
| Design artifact count | Files in feature directory | >25 files | Write/Edit paths under product/features/ |
| ADR count | ADR-* files created | >3 ADRs | Write path pattern match |
| Post-delivery issues | GH issues created after final task completion | >0 | Bash commands matching `gh issue create` |
| Phase duration outlier | Any phase >2x its evolving baseline duration | 2x baseline | Compare against stored MetricVector history |

### Feature Attribution

Attribution maps sessions to features at retrospective time by scanning record content.

**Attribution signals** (priority order):
1. File paths in tool inputs: Any reference to `product/features/{id}/` definitively identifies the feature
2. Task subjects: TaskCreate/TaskUpdate subjects containing feature IDs
3. Git checkout commands: Bash commands with `git checkout -b feature/{id}`

**Attribution logic**: Walk records in timestamp order, tracking the current feature. When a new feature ID appears, switch attribution. All records between switches belong to the current feature.

**Edge cases**:

| Scenario | Resolution |
|----------|------------|
| Session works on two features sequentially | Sequential attribution — records before the switch belong to feature A, records after belong to feature B. Both retrospectives see their respective partitions. |
| Session has no feature paths (triage, dialogue) | Unattributed. Ignored by retrospective. Flagged by safety valve if approaching 60-day cleanup. |
| Feature spans 5 sessions over 3 days | All sessions contain the same feature's file paths — attributed correctly. |
| Concurrent features in separate sessions | Each session's file paths reference different features — clean separation. |
| Design session before feature files exist | TaskCreate subjects carry feature ID — attributable. |
| Records before any feature ID appears | Attributed to the first feature that appears in the session (orientation/setup work). |

## Proposed Approach

### 1. `unimatrix-observe` Crate

New workspace crate at `crates/unimatrix-observe/`. Contains:

- **JSONL parser**: Read and deserialize observation records from session files
- **Attribution engine**: Content-based session-to-feature mapping
- **Hotspot detector**: Multi-pass rule engine with configurable thresholds
- **Metric computation**: Universal metrics and phase-adaptive metrics from analyzed records
- **Report builder**: Structured `RetrospectiveReport` with hotspots, evidence, and metrics
- **File manager**: Session file discovery, age checking, cleanup of files older than 60 days

No dependency on `unimatrix-store` or `unimatrix-server`. Pure computation library — takes file paths and historical metrics as input, produces structured reports as output. The server crate calls it.

### 2. OBSERVATION_METRICS Table

Add to `unimatrix-store` schema:
```rust
pub const OBSERVATION_METRICS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("observation_metrics");
```

14th table, created during `Store::open`. Key is feature cycle string, value is bincode-serialized `MetricVector`. Store methods: `store_metrics`, `get_metrics`, `list_all_metrics`.

Schema version remains 3 (no EntryRecord changes, new table only).

### 3. `context_retrospective` MCP Tool

New tool in the server crate. Handler:
1. Read observation directory from constant (`~/.unimatrix/observation/`)
2. Call `unimatrix-observe` to scan, attribute, and analyze
3. Store the new MetricVector in OBSERVATION_METRICS (or return cached if no new data)
4. Clean up session files older than 60 days
5. Return the complete `RetrospectiveReport` as the tool response

### 4. Production Hook Scripts

Ship hook scripts for Claude Code integration. Scripts handle:
- Reading hook input from stdin (JSON)
- Routing to `~/.unimatrix/observation/{session_id}.jsonl`
- Ensuring the observation directory exists
- Capturing all event types (not filtered by tool name)
- Truncating response_snippet to bound file size

Hook registration is documented but managed by the user via `.claude/settings.json` or `.claude/settings.local.json`.

### 5. `context_status` Extension

Extend `StatusReport` with observation fields:
- `observation_file_count: u64`
- `observation_total_size_bytes: u64`
- `observation_oldest_file_days: u64`
- `observation_approaching_cleanup: Vec<String>` (session IDs within 15 days of 60-day cleanup)
- `retrospected_feature_count: u64`

When `maintain=true`: run 60-day file cleanup as part of maintenance.

## Acceptance Criteria

### Hook Infrastructure
- AC-01: Hook scripts capture PreToolUse, PostToolUse, SubagentStart, and SubagentStop events
- AC-02: Records route to `~/.unimatrix/observation/{session_id}.jsonl` based on session_id field
- AC-03: Hook scripts are passive — exit 0 always, cannot block tool execution
- AC-04: Record schema includes: ts, hook, session_id, tool, input, response_size, response_snippet
- AC-05: Response snippet is truncated to bound per-record size

### Analysis Engine
- AC-06: `unimatrix-observe` crate parses JSONL session files into typed record structs
- AC-07: Sequential session-to-feature attribution walks records in timestamp order, switching current feature when a new feature ID appears in tool inputs (file paths, task subjects, git checkout commands)
- AC-08: Multi-session features: all sessions containing records attributed to the same feature are included
- AC-09: Multi-feature sessions: records are partitioned sequentially by feature switch points, each feature's retrospective sees only its partition
- AC-10: Hotspot detection framework supports registering rules by category (agent, friction, session, scope) with configurable thresholds
- AC-11: Permission retries rule implemented: Pre-Post count differential per tool, threshold >2
- AC-12: Session timeout rule implemented: timestamp gap >2 hours, threshold any occurrence
- AC-13: Sleep workarounds rule implemented: regex match `sleep` in Bash input, threshold any occurrence
- AC-14: Each hotspot includes: category, severity, claim text, measured value, threshold used, and evidence records (concrete tool call data that triggered the detection)
- AC-15: MetricVector contains universal metrics (fixed fields) and phase metrics (dynamic map keyed by discovered phase names)
- AC-16: Phase names are extracted from task subject prefixes using `{phase-id}: {description}` convention — parser splits on first `:`
- AC-17: `unimatrix-observe` has no dependency on `unimatrix-store` or `unimatrix-server`
- AC-18: The hotspot framework is extensible — adding a new rule requires implementing a trait/interface, not modifying the engine core

### MCP Tool
- AC-19: `context_retrospective` accepts `feature_cycle` parameter (required)
- AC-20: Tool scans observation directory, attributes sessions, runs analysis, returns complete report
- AC-21: Report includes: feature metrics (universal + per-phase) and hotspot findings with evidence
- AC-22: Report is self-contained — LLM does not need to call Unimatrix again for additional data
- AC-23: Tool stores the computed MetricVector in OBSERVATION_METRICS
- AC-24: Tool cleans up session files older than 60 days
- AC-25: Tool returns meaningful error if no session data and no stored MetricVector exist for the requested feature
- AC-26: When no new session data exists but a previous MetricVector is stored, tool returns the cached result with `computed_at` timestamp and a previous-run indicator
- AC-27: MetricVector includes a `computed_at` timestamp field

### Metric Storage
- AC-28: `OBSERVATION_METRICS` redb table exists with `&str -> &[u8]` schema (feature_cycle → serialized MetricVector)
- AC-29: Table is created during `Store::open` alongside existing tables (14th table)
- AC-30: Store provides methods: `store_metrics`, `get_metrics`, `list_all_metrics`
- AC-31: Schema version remains 3 (no EntryRecord changes)
- AC-32: MetricVector is serializable/deserializable via bincode (serde)

### File Lifecycle
- AC-33: Session files older than 60 days are automatically removed during retrospective or `context_status` maintenance
- AC-34: `context_status` reports observation file count, total size, and oldest file age
- AC-35: `context_status` warns when files approach the 60-day cleanup threshold

### General
- AC-36: `#![forbid(unsafe_code)]` on `unimatrix-observe`, edition 2024
- AC-37: No new crate dependencies beyond existing workspace (serde, bincode) and `std::time`
- AC-38: All existing tests pass with no regressions
- AC-39: Unit tests cover: JSONL parsing, feature attribution logic, the 3 shipped detection rules, MetricVector serialization, file age calculation
- AC-40: Integration tests cover: `context_retrospective` end-to-end (write test JSONL → call tool → verify report structure and metric storage), `context_status` observation fields, OBSERVATION_METRICS CRUD
- AC-41: Test infrastructure builds on existing workspace fixtures and patterns

## Constraints

- **No new external crate dependencies.** All functionality uses existing workspace crates (redb, bincode, serde, tokio) and `std::time`. ISO-8601 timestamp parsing uses manual parsing of the known hook-emitted format — no chrono or time crate.
- **`#![forbid(unsafe_code)]`**, edition 2024, MSRV 1.89.
- **`unimatrix-observe` is a pure computation library.** No database access, no MCP protocol awareness. Takes file paths and historical data as input, produces structured results as output. The server crate orchestrates.
- **Store crate remains domain-agnostic.** OBSERVATION_METRICS is a structural table (key-value storage). MetricVector serialization/deserialization logic can live in `unimatrix-observe` or `unimatrix-server`, not in the store crate.
- **Backward compatible.** New table and MCP tool. No changes to existing tools, entries, or schema. Existing tool calls unaffected.
- **Test infrastructure is cumulative.** Build on existing test fixtures in unimatrix-store and unimatrix-server.
- **Observation directory is a constant.** `~/.unimatrix/observation/` is defined as a constant, not configurable. Future configurability is a one-line change (constant → config lookup).
- **Hook scripts are not unit-testable in the Rust test suite.** They are shell scripts tested via integration tests that write synthetic JSONL and verify file routing.
- **Hook installation is manual.** col-002 documents the `.claude/settings.json` configuration required. The user or their LLM performs the setup.

## Resolved Decisions

1. **Metric vectors stored in a dedicated table, not as entries.** Metric data is structured numeric telemetry, not textual knowledge. Storing it in the entry pipeline would pollute the embedding space (HNSW), trigger irrelevant contradiction detection and near-duplicate checks, and waste ONNX inference cycles. A dedicated table provides clean, purpose-built storage without pipeline side effects.

2. **Phase metrics use a dynamic map, not hardcoded fields.** This project's workflow phases (3a, 3b, gate-3c) are project-specific. Another project might use (design, review, deploy). Phase names are discovered from telemetry and accumulated by name — locally consistent within a project, adaptable across projects.

3. **Content-based feature attribution, not git branch.** Git branch is global to the working directory and mis-attributes in multi-session workflows. Content-based attribution (scanning tool inputs for feature file paths and task subjects) is accurate regardless of branch state and handles concurrent sessions cleanly.

4. **Session files stay in observation directory, 60-day auto-cleanup.** No archival to feature directories, no file moving. Session files are re-scannable for re-analysis at any time. 60-day retention prevents unbounded growth while giving ample time to run retrospectives. The metric vector in Unimatrix is the permanent record.

5. **Hotspots include concrete evidence, not just metrics.** The LLM needs specific tool call examples to reason about root causes and propose actionable improvements. "Search miss rate: 32%" is not actionable. "Search miss rate: 32% — `Grep('store_observation_metrics')` → 0 results, `Glob('**/observe.rs')` → 0 results" is.

6. **MCP tool, not Claude Code skill.** `context_retrospective` is a server-side tool. A skill (prompt coaching the LLM on interpretation) can be added independently — it's a prompt file, not infrastructure. The tool response is self-contained.

7. **Bootstrapped thresholds only in v1.** Thresholds encode "good agentic development practice" as universal starting defaults. Future convergence adapts these to project-specific norms using accumulated MetricVectors and dismissed-hotspot feedback. The data model (raw measurements, not just pass/fail flags) supports this without re-processing raw JSONL.

8. **Phase names extracted from task subject prefix convention.** Task subjects use `{phase-id}: {description}` format (e.g., `3a: Pseudocode + test plans`). The parser splits on the first `:` and uses the trimmed prefix as the phase key. Phase IDs are opaque strings — could be `3a`, `design`, `sprint-2`, whatever the project uses. This is a protocol-level convention enforced in agent definitions — no NLP or LLM required.

9. **Sequential feature attribution, not majority voting.** Records in a session are walked in timestamp order. The "current feature" is tracked — when a new feature ID appears in a record (file path, task subject), that becomes the current feature for all subsequent records until the next switch. This cleanly partitions multi-feature sessions without ambiguity. Records before any feature ID appears are attributed to the first feature that appears (orientation work).

10. **Retrospective re-run returns cached result if no new data.** First call: analyze, store MetricVector with `computed_at` timestamp, return report. Subsequent call with new session data available: re-analyze, overwrite MetricVector, return fresh report. Subsequent call with no new data: return the stored MetricVector with previous `computed_at` timestamp and a note indicating this was a previous run. Never overwrite good data with an empty analysis.

11. **Hook installation is documented, not automated.** col-002 ships hook scripts and documents the `.claude/settings.json` configuration. The user (or their LLM) copies the config. No setup command in v1.

12. **Observation directory path is a constant.** `~/.unimatrix/observation/` is defined as a constant in the crate, not configurable. This allows easy future configurability (change constant to config lookup) without requiring config infrastructure now.

13. **No new crate dependencies for timestamp parsing.** The workspace uses `std::time` throughout — no chrono or time crate. JSONL timestamps are ISO-8601 strings in a format we control (emitted by our own hook scripts). Manual parsing of the known format avoids adding a dependency.

## Tracking

https://github.com/dug-21/unimatrix/issues/56

## Open Questions

_All original open questions resolved during scoping. None remaining._

