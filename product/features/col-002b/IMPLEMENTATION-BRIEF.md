# Implementation Brief: col-002b Detection Library + Baseline Comparison

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/col-002b/SCOPE.md |
| Scope Risk Assessment | product/features/col-002b/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-002b/architecture/ARCHITECTURE.md |
| ADR-001 | product/features/col-002b/architecture/ADR-001-baseline-injection-via-constructor.md |
| ADR-002 | product/features/col-002b/architecture/ADR-002-detection-submodules.md |
| ADR-003 | product/features/col-002b/architecture/ADR-003-baseline-arithmetic-guards.md |
| Specification | product/features/col-002b/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-002b/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-002b/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| detection-agent | pseudocode/detection-agent.md | test-plan/detection-agent.md |
| detection-friction | pseudocode/detection-friction.md | test-plan/detection-friction.md |
| detection-session | pseudocode/detection-session.md | test-plan/detection-session.md |
| detection-scope | pseudocode/detection-scope.md | test-plan/detection-scope.md |
| baseline | pseudocode/baseline.md | test-plan/baseline.md |
| server-integration | pseudocode/server-integration.md | test-plan/server-integration.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Implement 18 additional hotspot detection rules across all 4 categories (agent, friction, session, scope) into col-002's existing `DetectionRule` framework, and add historical baseline comparison to the `context_retrospective` report. This completes the detection library and enables per-feature metric comparison against project norms.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Baseline data for phase duration outlier | Constructor injection, not trait extension | ADR-001 | architecture/ADR-001-baseline-injection-via-constructor.md |
| Detection rule file organization | Submodule directory per category | ADR-002 | architecture/ADR-002-detection-submodules.md |
| Baseline arithmetic edge cases | Explicit guards: zero-stddev, NaN prevention, four status modes | ADR-003 | architecture/ADR-003-baseline-arithmetic-guards.md |
| DetectionRule trait | Unchanged from col-002 | SCOPE.md constraint | — |
| MetricVector struct | Unchanged from col-002 | SCOPE.md constraint (AC-14) | — |
| default_rules() signature | Changed to accept `Option<&[MetricVector]>` for phase duration outlier | ADR-001 | architecture/ADR-001-baseline-injection-via-constructor.md |
| Outlier threshold | mean + 1.5 * stddev, display only | SCOPE.md Resolved Decision 3 | — |
| Minimum baseline history | 3 MetricVectors required | SCOPE.md Goals section 2 | — |

## Files to Create/Modify

### New Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-observe/src/detection/mod.rs` | Detection module root: trait, engine, default_rules(), re-exports |
| `crates/unimatrix-observe/src/detection/agent.rs` | 7 agent hotspot rules: context_load, lifespan, file_breadth, reread_rate, mutation_spread, compile_cycles, edit_bloat |
| `crates/unimatrix-observe/src/detection/friction.rs` | 2 new friction rules + 2 existing from col-002: search_via_bash, output_parsing_struggle, permission_retries, sleep_workarounds |
| `crates/unimatrix-observe/src/detection/session.rs` | 4 new session rules + 1 existing from col-002: cold_restart, coordinator_respawns, post_completion_work, rework_events, session_timeout |
| `crates/unimatrix-observe/src/detection/scope.rs` | 5 scope hotspot rules: source_file_count, design_artifact_count, adr_count, post_delivery_issues, phase_duration_outlier |
| `crates/unimatrix-observe/src/baseline.rs` | Baseline computation: compute_baselines(), compare_to_baseline() |

### Modified Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-observe/src/lib.rs` | Add `pub mod baseline;`, change `pub mod detection;` to module directory |
| `crates/unimatrix-observe/src/types.rs` | Add BaselineSet, BaselineEntry, BaselineComparison, BaselineStatus; add `baseline_comparison` field to RetrospectiveReport |
| `crates/unimatrix-observe/src/report.rs` | Extend build_report() to accept and include baseline comparison |
| `crates/unimatrix-observe/src/detection.rs` | Refactored into detection/ module directory (file becomes directory) |
| `crates/unimatrix-server/src/tools.rs` | Extend context_retrospective handler: load history, compute baselines, pass to default_rules() and build_report() |

## Data Structures

### BaselineSet (unimatrix-observe)
```rust
pub struct BaselineSet {
    pub universal: HashMap<String, BaselineEntry>,
    pub phases: HashMap<String, HashMap<String, BaselineEntry>>,
}
```

### BaselineEntry (unimatrix-observe)
```rust
pub struct BaselineEntry {
    pub mean: f64,
    pub stddev: f64,
    pub sample_count: usize,
}
```

### BaselineComparison (unimatrix-observe)
```rust
pub struct BaselineComparison {
    pub metric_name: String,
    pub current_value: f64,
    pub mean: f64,
    pub stddev: f64,
    pub is_outlier: bool,
    pub status: BaselineStatus,
    pub phase: Option<String>,
}
```

### BaselineStatus (unimatrix-observe)
```rust
pub enum BaselineStatus {
    Normal,
    Outlier,
    NoVariance,
    NewSignal,
}
```

### RetrospectiveReport (unimatrix-observe, extended)
```rust
pub struct RetrospectiveReport {
    pub feature_cycle: String,
    pub session_count: usize,
    pub total_records: usize,
    pub metrics: MetricVector,
    pub hotspots: Vec<HotspotFinding>,
    pub is_cached: bool,
    #[serde(default)]
    pub baseline_comparison: Option<Vec<BaselineComparison>>,  // NEW
}
```

### Detection Rule Structs (18 new, all in unimatrix-observe)

Each rule is a unit struct (or struct with constructor data for PhaseDurationOutlierRule):

```rust
// Agent rules
pub struct ContextLoadRule;
pub struct LifespanRule;
pub struct FileBreadthRule;
pub struct RereadRateRule;
pub struct MutationSpreadRule;
pub struct CompileCyclesRule;
pub struct EditBloatRule;

// Friction rules
pub struct SearchViaBashRule;
pub struct OutputParsingStruggleRule;

// Session rules
pub struct ColdRestartRule;
pub struct CoordinatorRespawnsRule;
pub struct PostCompletionWorkRule;
pub struct ReworkEventsRule;

// Scope rules
pub struct SourceFileCountRule;
pub struct DesignArtifactCountRule;
pub struct AdrCountRule;
pub struct PostDeliveryIssuesRule;
pub struct PhaseDurationOutlierRule {
    phase_baselines: Option<HashMap<String, f64>>,  // phase_name -> mean_duration
}
```

## Function Signatures

### unimatrix-observe public API (new)

```rust
// baseline.rs
pub fn compute_baselines(history: &[MetricVector]) -> Option<BaselineSet>;
pub fn compare_to_baseline(current: &MetricVector, baselines: &BaselineSet) -> Vec<BaselineComparison>;

// detection/mod.rs (signature change)
pub fn default_rules(history: Option<&[MetricVector]>) -> Vec<Box<dyn DetectionRule>>;
```

### unimatrix-observe public API (signature changes)

```rust
// report.rs (extended)
pub fn build_report(
    feature_cycle: &str,
    records: &[ObservationRecord],
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
    baseline: Option<Vec<BaselineComparison>>,
) -> RetrospectiveReport;
```

### Detection Rule Implementations (18 new)

Each implements:
```rust
impl DetectionRule for {RuleName} {
    fn name(&self) -> &str;
    fn category(&self) -> HotspotCategory;
    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>;
}
```

## Constraints

- `#![forbid(unsafe_code)]` maintained on `unimatrix-observe`
- No new external crate dependencies
- `DetectionRule` trait interface unchanged from col-002
- `MetricVector` struct unchanged from col-002
- `OBSERVATION_METRICS` table schema unchanged
- No new MCP tools or tool parameters
- No changes to hook scripts or JSONL format
- `unimatrix-observe` remains independent of `unimatrix-store` and `unimatrix-server`
- All existing col-002 tests pass without regression

## Dependencies

| Crate | Version | Used By | Purpose |
|-------|---------|---------|---------|
| serde | workspace | unimatrix-observe | Serialize/Deserialize on new types |
| serde_json | existing | unimatrix-observe | Parse tool input fields in detection rules |
| regex patterns | std only | unimatrix-observe | Detection rule matching (no regex crate — use str methods and manual parsing) |

Note: No `regex` crate. Detection rules use `str::contains()`, `str::starts_with()`, and manual pattern matching. This avoids adding a dependency and keeps the attack surface minimal per RISK-TEST-STRATEGY.md security assessment.

## NOT in Scope

- Threshold convergence (adapting thresholds to project norms) — future
- Compound signal detection (correlated outliers) — future
- New MCP tools or parameters
- MetricVector structural changes
- Hook or collection infrastructure changes
- Auto-knowledge extraction — col-005
- Changes to OBSERVATION_METRICS table
- Per-agent attribution — platform limitation

## Alignment Status

All checks PASS. No variances requiring approval. See ALIGNMENT-REPORT.md for details.
