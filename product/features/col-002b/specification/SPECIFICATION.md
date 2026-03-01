# Specification: col-002b Detection Library + Baseline Comparison

## Objective

Implement the remaining 18 hotspot detection rules across all 4 hotspot categories into col-002's existing framework, and add historical baseline comparison to the `context_retrospective` report so that each feature's metrics can be evaluated against project norms.

## Functional Requirements

### FR-01: Agent Hotspot Rules (7 rules)

- FR-01.1: **Context Load** — Sum `response_size` from Read tool PostToolUse records until the first Write or Edit PostToolUse record. Threshold: >100 KB. Evidence: the specific Read calls and their sizes.
- FR-01.2: **Lifespan** — Compute duration between SubagentStart and SubagentStop timestamp pairs. Threshold: >45 minutes. Evidence: agent type, start/stop timestamps, duration.
- FR-01.3: **File Breadth** — Count distinct file paths appearing in Read, Write, or Edit tool input fields. Threshold: >20 files. Evidence: file path list with access counts.
- FR-01.4: **Re-read Rate** — Count files appearing in Read tool input more than once. Threshold: >3 re-read files. Evidence: file paths with read counts.
- FR-01.5: **Mutation Spread** — Count distinct file paths in Write or Edit tool input fields. Threshold: >10 files. Evidence: file path list.
- FR-01.6: **Compile Cycles** — Count Bash tool calls where input matches `cargo (check|test|build|clippy)` regex. Threshold: >6 per phase. Evidence: specific commands and timestamps. Phase-scoped: uses task subject phase attribution from col-002 to scope per phase.
- FR-01.7: **Edit Bloat** — Compute average `response_size` for Edit tool PostToolUse records. Threshold: >50 KB average. Evidence: individual Edit calls exceeding 50 KB with their response sizes.

### FR-02: Friction Hotspot Rules (2 rules)

- FR-02.1: **Search-via-Bash** — Count Bash tool calls where input matches `(find |grep |rg |ag )` regex patterns, compute as percentage of total Bash calls. Threshold: >5% of Bash calls. Evidence: specific Bash commands matching the pattern.
- FR-02.2: **Output Parsing Struggle** — Detect sequences where the same base cargo command appears with different pipe filters (different text after `|`) within a 3-minute window. Threshold: >2 filter variations for any command. Evidence: the command variations and their timestamps.

### FR-03: Session Hotspot Rules (4 rules)

- FR-03.1: **Cold Restart** — Detect timestamp gaps >30 minutes followed by a burst of Read calls to files that were already read earlier in the session (file path intersection with prior reads). Threshold: any occurrence. Evidence: gap duration, re-read file list, overlap count.
- FR-03.2: **Coordinator Respawns** — Count SubagentStart records where `tool` field (mapped from `agent_type`) matches coordinator patterns (contains "scrum-master", "coordinator", or "lead"). Threshold: >3 per feature. Evidence: agent types and spawn timestamps.
- FR-03.3: **Post-Completion Work** — Identify the last TaskUpdate record with a "completed" status transition as the completion boundary. Count tool calls after this boundary as a percentage of total. Threshold: >8% of total calls. Evidence: completion timestamp, post-completion call count, total count.
- FR-03.4: **Rework Events** — Detect TaskUpdate records where a task transitions from completed back to in_progress (status regression). Threshold: any occurrence. Evidence: task subjects and transition timestamps.

### FR-04: Scope Hotspot Rules (5 rules)

- FR-04.1: **Source File Count** — Count distinct `*.rs` file paths in Write tool input that create new files (first Write to that path). Threshold: >6 files. Evidence: file path list.
- FR-04.2: **Design Artifact Count** — Count distinct file paths under `product/features/` in Write or Edit tool input. Threshold: >25 files. Evidence: file path list.
- FR-04.3: **ADR Count** — Count distinct file paths matching `ADR-*` pattern in Write tool input. Threshold: >3 ADRs. Evidence: ADR file paths.
- FR-04.4: **Post-Delivery Issues** — Count Bash tool calls where input matches `gh issue create` after the final task completion boundary (same as FR-03.3). Threshold: >0. Evidence: issue creation commands and timestamps.
- FR-04.5: **Phase Duration Outlier** — Compare each phase's duration (from PhaseMetrics in current MetricVector) against the historical mean duration for that phase name. Threshold: >2x historical mean when 3+ data points exist for that phase; falls back to absolute threshold when insufficient history. Evidence: phase name, current duration, historical mean, threshold used.

### FR-05: Rule Registration

- FR-05.1: All 18 new rules register into `default_rules()` alongside col-002's 3 existing rules
- FR-05.2: Each rule implements the existing `DetectionRule` trait without modification to the trait
- FR-05.3: Rules are independent — no ordering dependencies between rules (except phase duration outlier depends on baseline data availability)
- FR-05.4: Each rule defines its bootstrapped threshold as a constant within its implementation

### FR-06: Baseline Computation

- FR-06.1: `compute_baselines(history: &[MetricVector]) -> Option<BaselineSet>` — returns None if fewer than 3 MetricVectors
- FR-06.2: For each universal metric field, compute mean and standard deviation across all historical MetricVectors
- FR-06.3: For phase metrics, group by phase name and compute mean/stddev per phase per metric
- FR-06.4: BaselineSet contains both universal and phase-specific baseline entries

### FR-07: Baseline Comparison

- FR-07.1: `compare_to_baseline(current: &MetricVector, baselines: &BaselineSet) -> Vec<BaselineComparison>`
- FR-07.2: Each comparison includes: metric name, current value, historical mean, stddev, outlier flag, optional phase name
- FR-07.3: A metric is flagged as outlier when `current > mean + 1.5 * stddev` AND stddev > 0.0
- FR-07.4: When stddev is 0.0 (no variance), the metric is not flagged as outlier
- FR-07.5: When mean is 0.0 and stddev is 0.0, a non-zero current value is labeled "new signal"
- FR-07.6: Phase-specific comparisons use only historical data for the matching phase name

### FR-08: Report Extension

- FR-08.1: `RetrospectiveReport` extended with `baseline_comparison: Option<Vec<BaselineComparison>>`
- FR-08.2: When baseline data is available (3+ historical vectors), the comparison is included
- FR-08.3: When insufficient history, `baseline_comparison` is None and the report includes a note
- FR-08.4: `build_report()` signature extended to accept optional baseline comparison

### FR-09: Server Integration

- FR-09.1: `context_retrospective` handler loads all historical MetricVectors via `Store::list_all_metrics()`
- FR-09.2: Handler deserializes each MetricVector using `deserialize_metric_vector()`
- FR-09.3: Handler excludes the current feature's own MetricVector from baseline history (avoids self-comparison)
- FR-09.4: Handler passes history to `default_rules()` for phase duration outlier construction
- FR-09.5: Handler passes history to `compute_baselines()` and then `compare_to_baseline()`
- FR-09.6: Handler passes baseline comparison to `build_report()`

## Non-Functional Requirements

### NFR-01: Performance

- 18 rules scanning a typical feature (5,000 records) should add less than 2 seconds to retrospective analysis time
- Baseline computation across 20 historical MetricVectors should complete in under 100ms

### NFR-02: Extensibility

- Adding a future detection rule requires only: implementing `DetectionRule` trait, adding to `default_rules()` list
- No changes to the detection engine, report builder, or server handler needed for additional rules

### NFR-03: Compatibility

- `#![forbid(unsafe_code)]` maintained on `unimatrix-observe`
- No new crate dependencies
- All existing col-002 tests pass without regression
- MetricVector serialization backward-compatible (no struct changes, serde(default) on RetrospectiveReport)

## Acceptance Criteria

| AC-ID | Description | Verification Method |
|-------|-------------|---------------------|
| AC-01 | All 7 agent hotspot rules implemented with bootstrapped thresholds | test: unit test per rule with synthetic records |
| AC-02 | Both friction hotspot rules implemented with bootstrapped thresholds | test: unit test per rule with synthetic records |
| AC-03 | All 4 session hotspot rules implemented with bootstrapped thresholds | test: unit test per rule with synthetic records |
| AC-04 | All 5 scope hotspot rules implemented with bootstrapped thresholds | test: unit test per rule with synthetic records |
| AC-05 | Each rule includes evidence records (concrete tool call data) | test: verify evidence fields on findings |
| AC-06 | Each rule is independently testable with unit tests | test: each rule has its own test module |
| AC-07 | All 18 new rules register into framework without modifying engine core | test: default_rules() returns 21 rules; detect_hotspots runs all |
| AC-08 | Baseline computation produces per-metric mean and stddev | test: known input, verify mean/stddev values |
| AC-09 | Phase-specific baselines computed per phase name | test: history with phases "3a" and "3b", verify separate baselines |
| AC-10 | Metrics exceeding mean + 1.5 stddev flagged as outliers | test: known outlier value, verify flag |
| AC-11 | Baseline requires minimum 3 MetricVectors; "insufficient history" with fewer | test: pass 2 vectors, verify None result |
| AC-12 | Comparison table included in retrospective report when baseline available | test: provide 3+ vectors, verify report field populated |
| AC-13 | Phase duration outlier uses baseline when available, falls back to absolute | test: with and without sufficient history |
| AC-14 | No changes to MetricVector structure, OBSERVATION_METRICS schema, or hooks | grep: no MetricVector field additions |
| AC-15 | No new MCP tools or parameters | grep: no new tool registration |
| AC-16 | All existing tests pass with no regressions | test: cargo test --workspace |
| AC-17 | Unit tests cover each of 18 rules, baseline computation, phase grouping, minimum history, outlier flagging | test: specific test counts |
| AC-18 | Integration tests cover full retrospective with all rules and baseline comparison | test: e2e with synthetic data |
| AC-19 | `#![forbid(unsafe_code)]` maintained | grep: crate root |
| AC-20 | No new crate dependencies | grep: Cargo.toml diff |

## Domain Models

### BaselineSet
Computed statistical baselines for all metrics across historical feature retrospectives.

| Field | Type | Description |
|-------|------|-------------|
| `universal` | `HashMap<String, BaselineEntry>` | Per-metric baselines for universal metrics (key = metric name) |
| `phases` | `HashMap<String, HashMap<String, BaselineEntry>>` | Phase-specific baselines (outer key = phase name, inner key = metric name) |

### BaselineEntry
Statistical summary for one metric across historical data.

| Field | Type | Description |
|-------|------|-------------|
| `mean` | `f64` | Arithmetic mean |
| `stddev` | `f64` | Population standard deviation |
| `sample_count` | `usize` | Number of data points used |

### BaselineComparison
One metric's current value compared to its historical baseline.

| Field | Type | Description |
|-------|------|-------------|
| `metric_name` | `String` | Name of the metric |
| `current_value` | `f64` | Current feature's value |
| `mean` | `f64` | Historical mean |
| `stddev` | `f64` | Historical stddev |
| `is_outlier` | `bool` | Whether current exceeds mean + 1.5 * stddev |
| `status` | `BaselineStatus` | normal, outlier, no_variance, new_signal |
| `phase` | `Option<String>` | Phase name if this is a phase-specific metric |

### BaselineStatus (enum)
```
Normal | Outlier | NoVariance | NewSignal
```

### Detection Rule Record Access Patterns

Each rule accesses specific fields from `ObservationRecord`:

| Rule | Hook Types | Fields Accessed |
|------|-----------|-----------------|
| Context Load | PostToolUse (Read, Write, Edit) | tool, response_size |
| Lifespan | SubagentStart, SubagentStop | ts, tool (agent_type) |
| File Breadth | PreToolUse/PostToolUse (Read, Write, Edit) | tool, input (file_path) |
| Re-read Rate | PreToolUse/PostToolUse (Read) | tool, input (file_path) |
| Mutation Spread | PreToolUse/PostToolUse (Write, Edit) | tool, input (file_path) |
| Compile Cycles | PreToolUse (Bash) | tool, input (command) |
| Edit Bloat | PostToolUse (Edit) | tool, response_size |
| Search-via-Bash | PreToolUse (Bash) | tool, input (command) |
| Output Parsing Struggle | PreToolUse (Bash) | tool, input (command), ts |
| Cold Restart | All | ts, tool, input (file_path) |
| Coordinator Respawns | SubagentStart | tool (agent_type) |
| Post-Completion Work | All + TaskUpdate | ts, tool, input |
| Rework Events | PreToolUse/PostToolUse (TaskUpdate) | tool, input (status) |
| Source File Count | PostToolUse (Write) | tool, input (file_path) |
| Design Artifact Count | PostToolUse (Write, Edit) | tool, input (file_path) |
| ADR Count | PostToolUse (Write) | tool, input (file_path) |
| Post-Delivery Issues | PreToolUse (Bash) | tool, input (command), ts |
| Phase Duration Outlier | N/A (uses MetricVector) | phases map from MetricVector |

## User Workflows

### Workflow 1: Running Retrospective with Full Detection

1. Agent calls `context_retrospective(feature_cycle: "col-002b")`
2. Server loads observation files, runs attribution, runs all 21 detection rules
3. Server loads historical MetricVectors, computes baselines
4. Server returns report with hotspot findings across all 4 categories and baseline comparison table
5. Agent/human reviews findings — richer hotspot coverage reveals previously invisible patterns

### Workflow 2: Early Project (Insufficient Baseline)

1. Agent calls `context_retrospective(feature_cycle: "nxs-001")`
2. Only 2 historical MetricVectors exist (insufficient for baseline)
3. Report includes all 21 detection rules with bootstrapped thresholds
4. Report notes "insufficient history for baseline comparison" — no comparison table
5. Phase duration outlier rule uses absolute threshold fallback

## Constraints

- No modifications to `DetectionRule` trait interface
- No modifications to `MetricVector` struct fields
- No modifications to `OBSERVATION_METRICS` table schema
- No new MCP tools or tool parameters
- No changes to hook scripts or JSONL format
- No new external crate dependencies
- `unimatrix-observe` remains independent of `unimatrix-store` and `unimatrix-server`
- Test infrastructure builds on col-002's test fixtures (synthetic JSONL generators, test MetricVectors)

## Dependencies

- `unimatrix-observe` (col-002) — DetectionRule trait, ObservationRecord, MetricVector, RetrospectiveReport, detect_hotspots(), build_report()
- `unimatrix-store` (col-002) — list_all_metrics() for historical data retrieval
- `unimatrix-server` (col-002) — context_retrospective handler as integration point

## NOT in Scope

- Threshold convergence (adapting thresholds to project norms) — future
- Compound signal detection (correlated outliers) — future
- New MCP tools or parameters
- MetricVector structural changes
- Hook or collection infrastructure changes
- Auto-knowledge extraction — col-005
