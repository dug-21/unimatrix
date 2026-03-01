# Architecture: col-002b Detection Library + Baseline Comparison

## System Overview

col-002b extends the `unimatrix-observe` crate with 18 additional detection rules and a baseline comparison module. All additions are internal to the observe crate — no changes to the store schema, MCP tool interface, or hook infrastructure. The server crate's `context_retrospective` handler gains baseline comparison by passing historical MetricVectors (already retrievable via `Store::list_all_metrics`) to a new baseline computation function in the observe crate.

```
unimatrix-observe (modified)                     unimatrix-server (modified)
┌─────────────────────────────────┐              ┌──────────────────────────┐
│ detection.rs                    │              │ tools.rs                 │
│   existing: 3 rules            │              │   context_retrospective  │
│   NEW: 18 rules (4 modules)    │              │     │                    │
│                                 │              │     ├─ call observe API  │
│ NEW: baseline.rs                │              │     ├─ load all metrics  │
│   compute_baselines()           │◄─────────────│     ├─ call baselines    │
│   compare_to_baseline()         │              │     └─ include in report │
│                                 │              └──────────────────────────┘
│ report.rs (modified)            │
│   RetrospectiveReport           │
│     + baseline_comparison field │
│                                 │
│ types.rs (modified)             │
│   + BaselineSet, BaselineEntry  │
│   + BaselineComparison          │
└─────────────────────────────────┘
```

## Component Breakdown

### 1. Detection Rules (4 category modules)

18 new rules organized by hotspot category. Each rule implements the existing `DetectionRule` trait from col-002. No trait modifications.

**Module organization within `detection.rs` or as submodules:**

| Module | Rules | Count |
|--------|-------|-------|
| `detection/agent.rs` | context_load, lifespan, file_breadth, reread_rate, mutation_spread, compile_cycles, edit_bloat | 7 |
| `detection/friction.rs` | search_via_bash, output_parsing_struggle | 2 |
| `detection/session.rs` | cold_restart, coordinator_respawns, post_completion_work, rework_events | 4 |
| `detection/scope.rs` | source_file_count, design_artifact_count, adr_count, post_delivery_issues, phase_duration_outlier | 5 |

Each rule is a struct implementing `DetectionRule`:
```rust
pub trait DetectionRule {
    fn name(&self) -> &str;
    fn category(&self) -> HotspotCategory;
    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>;
}
```

**Phase duration outlier** is the only rule that requires baseline data. It receives historical MetricVectors at construction time (not through the trait). See ADR-001.

### 2. Baseline Comparison Module

New `baseline.rs` module in `unimatrix-observe`. Pure computation — takes `&[MetricVector]` as input, produces comparison results.

**Responsibilities:**
- Compute per-metric mean and standard deviation across historical MetricVectors
- Group by phase name for phase-specific baselines
- Compare current feature metrics against baselines
- Flag outliers exceeding mean + 1.5 sigma
- Enforce minimum 3 data points requirement

**Key types:**
- `BaselineSet` — computed baselines for all metrics
- `BaselineEntry` — mean + stddev for one metric
- `BaselineComparison` — one metric's current value vs baseline with outlier flag

### 3. Report Extension

`RetrospectiveReport` gains an optional `baseline_comparison` field. This is a struct extension in `types.rs` — not a MetricVector change.

### 4. Server Integration (Minimal)

The `context_retrospective` handler in `unimatrix-server` adds two steps after existing analysis:
1. Call `Store::list_all_metrics()` to load historical MetricVectors
2. Deserialize each using `observe::deserialize_metric_vector()`
3. Call `observe::compute_baselines()` and `observe::compare_to_baseline()`
4. Pass the comparison result to `observe::build_report()`

### 5. Rule Registration

col-002 provides `default_rules() -> Vec<Box<dyn DetectionRule>>`. col-002b extends this to include all 21 rules. The function remains the single entry point for rule enumeration — no registry pattern, no configuration.

## Component Interactions

### Data Flow: Detection Rules

```
ObservationRecord[] ──► detect_hotspots(records, rules) ──► HotspotFinding[]
                              │
                              ├── PermissionRetriesRule.detect(records)    (col-002)
                              ├── SessionTimeoutRule.detect(records)       (col-002)
                              ├── SleepWorkaroundsRule.detect(records)     (col-002)
                              ├── ContextLoadRule.detect(records)          (col-002b NEW)
                              ├── LifespanRule.detect(records)             (col-002b NEW)
                              ├── ... 16 more rules ...                   (col-002b NEW)
                              └── collect all findings
```

### Data Flow: Baseline Comparison

```
Store::list_all_metrics() ──► Vec<(String, Vec<u8>)>
                                    │
                    deserialize_metric_vector() each
                                    │
                                    ▼
                          Vec<MetricVector> (historical)
                                    │
                    compute_baselines(history)
                                    │
                                    ▼
                              BaselineSet
                                    │
                    compare_to_baseline(current, baselines)
                                    │
                                    ▼
                          Vec<BaselineComparison>
                                    │
                    included in RetrospectiveReport
```

### Data Flow: Phase Duration Outlier Rule

```
Vec<MetricVector> (historical) ──► PhaseDurationOutlierRule::new(history)
                                          │
ObservationRecord[] ──────────────────────┤
                                          ▼
                              PhaseDurationOutlierRule.detect(records)
                                          │
                              compare each phase duration against
                              historical mean for that phase name
                                          │
                              ≥3 data points: use 2x mean threshold
                              <3 data points: use absolute threshold
                                          │
                                          ▼
                                    HotspotFinding[]
```

## Technology Decisions

### ADR-001: Baseline Data Injection via Constructor (Not Trait Extension)

The phase duration outlier rule needs historical MetricVector data. Rather than extending the `DetectionRule` trait (breaking col-002's interface), this data is injected through the rule's constructor. See `architecture/ADR-001-baseline-injection-via-constructor.md`.

### ADR-002: Detection Rules as Submodules

18 rules in a single `detection.rs` file would exceed 1500 lines. Rules are organized into category submodules. See `architecture/ADR-002-detection-submodules.md`.

### ADR-003: Baseline Arithmetic Edge Cases

Zero-stddev, NaN, and insufficient data are handled with explicit guards rather than relying on floating-point behavior. See `architecture/ADR-003-baseline-arithmetic-guards.md`.

## Integration Points

### Existing: unimatrix-observe (col-002)

- **DetectionRule trait** — unchanged. All 18 new rules implement this exact trait.
- **`default_rules()`** — extended to return all 21 rules. Phase duration outlier rule requires a `Vec<MetricVector>` parameter, so the function signature changes to `default_rules(history: Option<&[MetricVector]>) -> Vec<Box<dyn DetectionRule>>`. When `history` is None, the phase duration outlier rule uses absolute thresholds only.
- **`detect_hotspots()`** — unchanged. Takes `&[ObservationRecord]` and `&[Box<dyn DetectionRule>]`.
- **`build_report()`** — extended to accept optional `Vec<BaselineComparison>`.
- **RetrospectiveReport** — extended with `baseline_comparison: Option<Vec<BaselineComparison>>`.
- **types.rs** — new types: `BaselineSet`, `BaselineEntry`, `BaselineComparison`.

### Existing: unimatrix-server

- **`context_retrospective` handler** — modified to load historical metrics, compute baselines, pass to report builder.
- **No new tools, no new parameters, no new error codes.**

### Existing: unimatrix-store

- **`list_all_metrics()`** — already defined in col-002. Called by server to get historical data.
- **No store changes.**

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `DetectionRule` trait | `fn name(&self) -> &str; fn category(&self) -> HotspotCategory; fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>` | `unimatrix-observe/src/detection.rs` (col-002, unchanged) |
| `default_rules` | `fn default_rules(history: Option<&[MetricVector]>) -> Vec<Box<dyn DetectionRule>>` | `unimatrix-observe/src/detection.rs` (signature change) |
| `compute_baselines` | `fn compute_baselines(history: &[MetricVector]) -> Option<BaselineSet>` | `unimatrix-observe/src/baseline.rs` (new) |
| `compare_to_baseline` | `fn compare_to_baseline(current: &MetricVector, baselines: &BaselineSet) -> Vec<BaselineComparison>` | `unimatrix-observe/src/baseline.rs` (new) |
| `build_report` | `fn build_report(feature_cycle: &str, records: &[ObservationRecord], metrics: MetricVector, hotspots: Vec<HotspotFinding>, baseline: Option<Vec<BaselineComparison>>) -> RetrospectiveReport` | `unimatrix-observe/src/report.rs` (signature change) |
| `RetrospectiveReport` | `struct { ..., baseline_comparison: Option<Vec<BaselineComparison>> }` | `unimatrix-observe/src/types.rs` (field addition) |
| `BaselineSet` | `struct { universal: HashMap<String, BaselineEntry>, phases: HashMap<String, HashMap<String, BaselineEntry>> }` | `unimatrix-observe/src/types.rs` (new) |
| `BaselineEntry` | `struct { mean: f64, stddev: f64, sample_count: usize }` | `unimatrix-observe/src/types.rs` (new) |
| `BaselineComparison` | `struct { metric_name: String, current_value: f64, mean: f64, stddev: f64, is_outlier: bool, phase: Option<String> }` | `unimatrix-observe/src/types.rs` (new) |
| `Store::list_all_metrics` | `fn(&self) -> Result<Vec<(String, Vec<u8>)>>` | `unimatrix-store/src/read.rs` (col-002, unchanged) |
| `serialize_metric_vector` | `fn(mv: &MetricVector) -> Result<Vec<u8>>` | `unimatrix-observe` (col-002, unchanged) |
| `deserialize_metric_vector` | `fn(bytes: &[u8]) -> Result<MetricVector>` | `unimatrix-observe` (col-002, unchanged) |

## Files to Create/Modify

### New Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-observe/src/detection/agent.rs` | 7 agent hotspot rules |
| `crates/unimatrix-observe/src/detection/friction.rs` | 2 friction hotspot rules (extend existing) |
| `crates/unimatrix-observe/src/detection/session.rs` | 4 session hotspot rules (extend existing) |
| `crates/unimatrix-observe/src/detection/scope.rs` | 5 scope hotspot rules |
| `crates/unimatrix-observe/src/detection/mod.rs` | Module root: re-exports, default_rules() |
| `crates/unimatrix-observe/src/baseline.rs` | Baseline computation and comparison |

### Modified Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-observe/src/lib.rs` | Add `pub mod baseline;`, restructure detection module |
| `crates/unimatrix-observe/src/types.rs` | Add BaselineSet, BaselineEntry, BaselineComparison; extend RetrospectiveReport |
| `crates/unimatrix-observe/src/report.rs` | Accept and include baseline comparison in report |
| `crates/unimatrix-observe/src/detection.rs` | Refactor into detection/ module directory with submodules |
| `crates/unimatrix-server/src/tools.rs` | Extend context_retrospective handler to compute and include baselines |
