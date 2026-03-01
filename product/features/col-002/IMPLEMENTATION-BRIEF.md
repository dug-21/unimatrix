# Implementation Brief: col-002 Retrospective Pipeline

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/col-002/SCOPE.md |
| Scope Risk Assessment | product/features/col-002/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-002/architecture/ARCHITECTURE.md |
| ADR-001 | product/features/col-002/architecture/ADR-001-observe-crate-independence.md |
| ADR-002 | product/features/col-002/architecture/ADR-002-metric-vector-serialization.md |
| ADR-003 | product/features/col-002/architecture/ADR-003-separate-hook-scripts.md |
| ADR-004 | product/features/col-002/architecture/ADR-004-observation-dir-constant.md |
| Specification | product/features/col-002/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-002/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-002/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| observe-types | pseudocode/observe-types.md | test-plan/observe-types.md |
| observe-parser | pseudocode/observe-parser.md | test-plan/observe-parser.md |
| observe-attribution | pseudocode/observe-attribution.md | test-plan/observe-attribution.md |
| observe-detection | pseudocode/observe-detection.md | test-plan/observe-detection.md |
| observe-metrics | pseudocode/observe-metrics.md | test-plan/observe-metrics.md |
| observe-report | pseudocode/observe-report.md | test-plan/observe-report.md |
| observe-files | pseudocode/observe-files.md | test-plan/observe-files.md |
| store-observation | pseudocode/store-observation.md | test-plan/store-observation.md |
| server-retrospective | pseudocode/server-retrospective.md | test-plan/server-retrospective.md |
| server-status-ext | pseudocode/server-status-ext.md | test-plan/server-status-ext.md |
| hooks | pseudocode/hooks.md | test-plan/hooks.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Build the end-to-end observation pipeline for Unimatrix: Claude Code hook scripts that collect per-session JSONL telemetry, a new `unimatrix-observe` crate that parses, attributes, detects hotspots, and computes metric vectors, a `context_retrospective` MCP tool that triggers analysis and returns self-contained reports, an `OBSERVATION_METRICS` redb table for metric storage, and `context_status` observation health fields. Ship 3 detection rules (permission retries, session timeout, sleep workarounds) to validate the pipeline framework end-to-end.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Observe crate dependency boundary | No dependency on unimatrix-store or unimatrix-server | SCOPE.md constraint, SR-03 | architecture/ADR-001-observe-crate-independence.md |
| MetricVector serialization ownership | Observe crate owns serialize/deserialize helpers; store handles opaque bytes | Workspace pattern (serialize_entry) | architecture/ADR-002-metric-vector-serialization.md |
| Hook script structure | Four separate scripts (one per event type) | Claude Code hook API model | architecture/ADR-003-separate-hook-scripts.md |
| Observation directory path | Compile-time constant, functions accept &Path for testability | SCOPE.md constraint | architecture/ADR-004-observation-dir-constant.md |
| Metric storage model | Dedicated OBSERVATION_METRICS table, not entry pipeline | SCOPE.md Resolved Decision 1 | SCOPE.md |
| Phase metric keys | Dynamic BTreeMap, discovered from task subject prefix | SCOPE.md Resolved Decision 2, 8 | SCOPE.md |
| Feature attribution | Content-based sequential scanning, not git branch | SCOPE.md Resolved Decision 3, 9 | SCOPE.md |
| File lifecycle | 60-day retention, no archival | SCOPE.md Resolved Decision 4 | SCOPE.md |
| Hotspot evidence | Concrete tool call data, not just metrics | SCOPE.md Resolved Decision 5 | SCOPE.md |
| Re-run behavior | Return cached MetricVector when no new data | SCOPE.md Resolved Decision 10 | SCOPE.md |
| Timestamp parsing | Manual ISO-8601 parsing of controlled format (no chrono) | SCOPE.md Resolved Decision 13 | SCOPE.md |
| Schema version | Remains 3 (new table, no EntryRecord changes) | SCOPE.md, FR-10.6 | -- |

## Files to Create/Modify

### New Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-observe/Cargo.toml` | New crate manifest: edition 2024, workspace deps (serde, bincode), serde_json |
| `crates/unimatrix-observe/src/lib.rs` | Crate root: `#![forbid(unsafe_code)]`, module declarations, re-exports |
| `crates/unimatrix-observe/src/types.rs` | ObservationRecord, MetricVector, UniversalMetrics, PhaseMetrics, HotspotFinding, HotspotCategory, Severity, HookType, EvidenceRecord, RetrospectiveReport, SessionFile, ObservationStats, ParsedSession |
| `crates/unimatrix-observe/src/parser.rs` | JSONL line parsing, timestamp parsing, session file reading |
| `crates/unimatrix-observe/src/attribution.rs` | Feature attribution logic: signal extraction, sequential tracking, session partitioning |
| `crates/unimatrix-observe/src/detection.rs` | DetectionRule trait, PermissionRetriesRule, SessionTimeoutRule, SleepWorkaroundsRule, detection engine |
| `crates/unimatrix-observe/src/metrics.rs` | MetricVector computation from analyzed records |
| `crates/unimatrix-observe/src/report.rs` | RetrospectiveReport assembly |
| `crates/unimatrix-observe/src/files.rs` | Session file discovery, age calculation, cleanup identification, stats |
| `hooks/observe-pre-tool.sh` | PreToolUse hook script |
| `hooks/observe-post-tool.sh` | PostToolUse hook script |
| `hooks/observe-subagent-start.sh` | SubagentStart hook script |
| `hooks/observe-subagent-stop.sh` | SubagentStop hook script |

### Modified Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-store/src/schema.rs` | Add OBSERVATION_METRICS table definition |
| `crates/unimatrix-store/src/db.rs` | Open OBSERVATION_METRICS in Store::open, update table count comments |
| `crates/unimatrix-store/src/write.rs` | Add store_metrics method |
| `crates/unimatrix-store/src/read.rs` | Add get_metrics and list_all_metrics methods |
| `crates/unimatrix-server/Cargo.toml` | Add unimatrix-observe dependency |
| `crates/unimatrix-server/src/tools.rs` | Add RetrospectiveParams, context_retrospective handler |
| `crates/unimatrix-server/src/response.rs` | Extend StatusReport with observation fields, update format_status_report |
| `crates/unimatrix-server/src/error.rs` | Add ObservationError variant, ERROR_NO_OBSERVATION_DATA code |
| `crates/unimatrix-server/src/server.rs` | Register context_retrospective in tool router |
| `crates/unimatrix-server/src/validation.rs` | Add validate_retrospective_params |

## Data Structures

### ObservationRecord (unimatrix-observe)
```rust
pub struct ObservationRecord {
    pub ts: u64,                              // Unix epoch seconds
    pub hook: HookType,                       // PreToolUse | PostToolUse | SubagentStart | SubagentStop
    pub session_id: String,
    pub tool: Option<String>,                 // tool_name or agent_type
    pub input: Option<serde_json::Value>,     // tool_input (truncated)
    pub response_size: Option<u64>,           // PostToolUse only
    pub response_snippet: Option<String>,     // First 500 chars
}
```

### MetricVector (unimatrix-observe)
```rust
pub struct MetricVector {
    pub computed_at: u64,
    pub universal: UniversalMetrics,
    pub phases: BTreeMap<String, PhaseMetrics>,
}
```

### UniversalMetrics (unimatrix-observe)
```rust
pub struct UniversalMetrics {
    pub total_tool_calls: u64,
    pub total_duration_secs: u64,
    pub session_count: u64,
    pub search_miss_rate: f64,
    pub edit_bloat_total_kb: f64,
    pub edit_bloat_ratio: f64,
    pub permission_friction_events: u64,
    pub bash_for_search_count: u64,
    pub cold_restart_events: u64,
    pub coordinator_respawn_count: u64,
    pub parallel_call_rate: f64,
    pub context_load_before_first_write_kb: f64,
    pub total_context_loaded_kb: f64,
    pub post_completion_work_pct: f64,
    pub follow_up_issues_created: u64,
    pub knowledge_entries_stored: u64,
    pub sleep_workaround_count: u64,
    pub agent_hotspot_count: u64,
    pub friction_hotspot_count: u64,
    pub session_hotspot_count: u64,
    pub scope_hotspot_count: u64,
}
```

### PhaseMetrics (unimatrix-observe)
```rust
pub struct PhaseMetrics {
    pub duration_secs: u64,
    pub tool_call_count: u64,
}
```

### HotspotFinding (unimatrix-observe)
```rust
pub struct HotspotFinding {
    pub category: HotspotCategory,
    pub severity: Severity,
    pub rule_name: String,
    pub claim: String,
    pub measured: f64,
    pub threshold: f64,
    pub evidence: Vec<EvidenceRecord>,
}
```

### EvidenceRecord (unimatrix-observe)
```rust
pub struct EvidenceRecord {
    pub description: String,
    pub ts: u64,
    pub tool: Option<String>,
    pub detail: String,
}
```

### RetrospectiveReport (unimatrix-observe)
```rust
pub struct RetrospectiveReport {
    pub feature_cycle: String,
    pub session_count: usize,
    pub total_records: usize,
    pub metrics: MetricVector,
    pub hotspots: Vec<HotspotFinding>,
    pub is_cached: bool,
}
```

### DetectionRule Trait (unimatrix-observe)
```rust
pub trait DetectionRule {
    fn name(&self) -> &str;
    fn category(&self) -> HotspotCategory;
    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>;
}
```

### RetrospectiveParams (unimatrix-server)
```rust
pub struct RetrospectiveParams {
    pub feature_cycle: String,
    pub agent_id: Option<String>,
}
```

### StatusReport Extension (unimatrix-server)
```rust
// New fields added to existing StatusReport struct:
pub observation_file_count: u64,
pub observation_total_size_bytes: u64,
pub observation_oldest_file_days: u64,
pub observation_approaching_cleanup: Vec<String>,
pub retrospected_feature_count: u64,
```

### OBSERVATION_METRICS Table (unimatrix-store)
```rust
pub const OBSERVATION_METRICS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("observation_metrics");
```

## Function Signatures

### unimatrix-observe public API

```rust
// parser
pub fn parse_session_file(path: &Path) -> Result<Vec<ObservationRecord>>;
pub fn parse_timestamp(ts: &str) -> Result<u64>;

// files
pub fn discover_sessions(dir: &Path) -> Result<Vec<SessionFile>>;
pub fn identify_expired(dir: &Path, max_age_secs: u64) -> Result<Vec<PathBuf>>;
pub fn scan_observation_stats(dir: &Path) -> Result<ObservationStats>;

// attribution
pub fn attribute_sessions(sessions: &[ParsedSession], target_feature: &str) -> Vec<ObservationRecord>;

// detection
pub fn detect_hotspots(records: &[ObservationRecord], rules: &[Box<dyn DetectionRule>]) -> Vec<HotspotFinding>;
pub fn default_rules() -> Vec<Box<dyn DetectionRule>>;

// metrics
pub fn compute_metric_vector(records: &[ObservationRecord], hotspots: &[HotspotFinding], computed_at: u64) -> MetricVector;

// report
pub fn build_report(feature_cycle: &str, records: &[ObservationRecord], metrics: MetricVector, hotspots: Vec<HotspotFinding>) -> RetrospectiveReport;

// serialization (ADR-002)
pub fn serialize_metric_vector(mv: &MetricVector) -> Result<Vec<u8>>;
pub fn deserialize_metric_vector(bytes: &[u8]) -> Result<MetricVector>;
```

### unimatrix-store additions

```rust
// write.rs
pub fn store_metrics(&self, feature_cycle: &str, data: &[u8]) -> Result<()>;

// read.rs
pub fn get_metrics(&self, feature_cycle: &str) -> Result<Option<Vec<u8>>>;
pub fn list_all_metrics(&self) -> Result<Vec<(String, Vec<u8>)>>;
```

## Constraints

- `#![forbid(unsafe_code)]` on unimatrix-observe, edition 2024, MSRV 1.89
- No new external crate dependencies beyond workspace (serde, bincode) plus serde_json (already a server dep, needed by observe for parsing tool input JSON)
- unimatrix-observe has zero dependency on unimatrix-store or unimatrix-server
- Schema version remains 3
- All existing tests pass without regression
- Hook scripts are not Rust-testable; shell integration tests validate them
- Hook installation is manual (documented)
- Observation directory path is a constant; functions accept &Path for testability

## Dependencies

| Crate | Version | Used By | Purpose |
|-------|---------|---------|---------|
| serde | workspace | unimatrix-observe | Derive Serialize/Deserialize on types |
| bincode | workspace | unimatrix-observe | MetricVector serialization |
| serde_json | existing server dep | unimatrix-observe | Parse JSONL records (tool input field) |
| redb | workspace | unimatrix-store | OBSERVATION_METRICS table |
| rmcp | existing | unimatrix-server | Tool registration |
| schemars | existing | unimatrix-server | JSON Schema for params |

## NOT in Scope

- Full detection library (21 rules) -- col-002b
- Baseline comparison across features -- col-002b
- Threshold convergence -- future
- Compound signal detection -- future
- Auto-knowledge extraction -- col-005
- `/retrospective` skill -- separate prompt file
- Per-agent attribution -- platform limitation
- LLM in analysis -- all rule-based
- Streaming/background analysis -- batch only
- Entry pipeline changes -- dedicated table
- Multi-project support -- single project

## Alignment Status

All checks PASS. No variances requiring approval. See ALIGNMENT-REPORT.md for details.
