# col-023: W1-5 Observation Pipeline Generalization — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-023/SCOPE.md |
| Architecture | product/features/col-023/architecture/ARCHITECTURE.md |
| Specification | product/features/col-023/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-023/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-023/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| observation-record | pseudocode/observation-record.md | test-plan/observation-record.md |
| domain-pack-registry | pseudocode/domain-pack-registry.md | test-plan/domain-pack-registry.md |
| ingest-security | pseudocode/ingest-security.md | test-plan/ingest-security.md |
| detection-rules | pseudocode/detection-rules.md | test-plan/detection-rules.md |
| rule-dsl-evaluator | pseudocode/rule-dsl-evaluator.md | test-plan/rule-dsl-evaluator.md |
| metrics-extension | pseudocode/metrics-extension.md | test-plan/metrics-extension.md |
| schema-migration | pseudocode/schema-migration.md | test-plan/schema-migration.md |
| config-extension | pseudocode/config-extension.md | test-plan/config-extension.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map lists expected components from the architecture — actual file paths are filled during delivery. The Cross-Cutting Artifacts section tracks files that don't belong to a single component but are consumed by specific stages.

---

## Goal

Replace the Claude Code-hardwired observation pipeline with a domain-agnostic event processing framework by substituting the `HookType` closed enum with string-typed `event_type`/`source_domain` fields, introducing a TOML-configured `DomainPackRegistry`, rewriting all 21 detection rules to guard on `source_domain`, and extending `OBSERVATION_METRICS` with a nullable `domain_metrics_json` column (schema v14). The "claude-code" default domain pack must be bundled and always active so existing retrospective behavior is identical with zero config changes. This unblocks W3-1 (GNN training signal pipeline), which requires a domain-neutral observation substrate.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Replace `HookType` enum | Replace `ObservationRecord.hook: HookType` with `event_type: String` + `source_domain: String`; retain `HookType` as `pub mod hook_type` string constants for documentation only | Specification FR-01, Architecture §unimatrix-core | architecture/ADR-001-observation-event-type-generalization.md |
| Domain pack registration mechanism | TOML `[observation]` config at startup; no new MCP tool; no runtime re-registration in W1-5 | Specification FR-02, ADR-002, human confirmation | architecture/ADR-002-domain-pack-registry.md |
| External rule DSL | Bounded two-kind `RuleEvaluator` struct: `threshold` (count > N) and `temporal_window` (count > N within T secs); no `eval`, no dynamic loading; `serde_json::Value::pointer` for payload extraction | Specification FR-04.5, ADR-003 | architecture/ADR-003-rule-evaluator-dsl.md |
| HookType blast radius management | Four-wave compilation-gated refactor; `cargo check --workspace` gate after each wave; entire refactor lands in one PR | Specification NFR-09, ADR-004 | architecture/ADR-004-hooktype-refactor-wave-plan.md |
| Cross-domain false findings | Mandatory `source_domain` guard preamble as first operation in every domain-specific `DetectionRule::detect()` | Specification FR-04.3, ADR-005 | architecture/ADR-005-source-domain-guard-contract.md |
| `UniversalMetrics` canonical representation | Typed struct remains canonical (Option A); `MetricVector` gains `domain_metrics: HashMap<String, f64>` extension field; `OBSERVATION_METRICS` gains nullable `domain_metrics_json TEXT` column | Specification FR-05, ADR-006 | architecture/ADR-006-universal-metrics-canonical-representation.md |
| Ingest security bounds | 64 KB payload size check (raw bytes before parse); depth ≤ 10 via recursive `json_depth()`; `source_domain` regex `^[a-z0-9_-]{1,64}$` validated at registration and ingest; violations produce `ObserveError` variants and skip the event | Specification NFR-02/03/04, ADR-007 | architecture/ADR-007-ingest-security-bounds.md |
| Schema migration | v13 → v14: single `ALTER TABLE OBSERVATION_METRICS ADD COLUMN domain_metrics_json TEXT NULL` | Specification NFR-08, ADR-006 | architecture/ADR-006-universal-metrics-canonical-representation.md |
| FR-06 Admin runtime override | Removed from W1-5 scope; config-only is simpler, reproducible, and version-controllable | ADR-002, human-confirmed; AC-08 and Workflow 3 removed from spec | architecture/ADR-002-domain-pack-registry.md |

---

## Files to Create/Modify

### unimatrix-core

| File | Change |
|------|--------|
| `crates/unimatrix-core/src/observation.rs` | Replace `hook: HookType` with `event_type: String` + `source_domain: String` on `ObservationRecord`; convert `HookType` enum to `pub mod hook_type` string constants |

### unimatrix-observe

| File | Change |
|------|--------|
| `crates/unimatrix-observe/src/types.rs` | Remove `HookType` re-export; update `ObservationRecord` import for new fields |
| `crates/unimatrix-observe/src/domain/mod.rs` | **New module**: `DomainPack`, `DomainPackRegistry`, `RuleDescriptor`, `RuleEvaluator`, built-in claude-code pack const |
| `crates/unimatrix-observe/src/detection/mod.rs` | Add `domain_rules(pack: &DomainPack) -> Vec<Box<dyn DetectionRule>>`; `default_rules()` unchanged in count |
| `crates/unimatrix-observe/src/detection/agent.rs` | Rewrite rules to use `event_type: String` + `source_domain: String`; add `source_domain == "claude-code"` preamble to all rules |
| `crates/unimatrix-observe/src/detection/friction.rs` | Same as agent.rs |
| `crates/unimatrix-observe/src/detection/session.rs` | Same as agent.rs |
| `crates/unimatrix-observe/src/detection/scope.rs` | Same as agent.rs |
| `crates/unimatrix-observe/src/metrics.rs` | Replace `HookType` comparisons with string comparisons guarded by `source_domain == "claude-code"` |
| `crates/unimatrix-observe/src/extraction/recurring_friction.rs` | Replace `HookType` match arms with string comparisons |
| `crates/unimatrix-observe/src/extraction/knowledge_gap.rs` | Same |
| `crates/unimatrix-observe/src/extraction/implicit_convention.rs` | Same |
| `crates/unimatrix-observe/src/extraction/file_dependency.rs` | Same |
| `crates/unimatrix-observe/src/extraction/dead_knowledge.rs` | Same |
| `crates/unimatrix-observe/src/session_metrics.rs` | Update `ObservationRecord` field references |
| `crates/unimatrix-observe/src/report.rs` | Update field references if any |
| `crates/unimatrix-observe/src/lib.rs` | Export `domain` module |
| `crates/unimatrix-observe/src/error.rs` | Add `PayloadTooLarge`, `PayloadNestingTooDeep`, `InvalidSourceDomain`, `InvalidRuleDescriptor` variants |
| `crates/unimatrix-observe/tests/extraction_pipeline.rs` | Update all `ObservationRecord` construction sites to supply `event_type` and `source_domain` |

### unimatrix-store

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/metrics.rs` | Add `domain_metrics: HashMap<String, f64>` field to `MetricVector`; update `store_metrics()`/`get_metrics()` to read/write `domain_metrics_json`; update structural test |
| `crates/unimatrix-store/src/migration.rs` | Add v13→v14 migration: `ALTER TABLE OBSERVATION_METRICS ADD COLUMN domain_metrics_json TEXT NULL`; increment `CURRENT_SCHEMA_VERSION` to 14 |

### unimatrix-server

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/infra/config.rs` | Add `ObservationConfig` struct and `observation: ObservationConfig` field to `UnimatrixConfig`; add `DomainPackConfig` struct |
| `crates/unimatrix-server/src/services/observation.rs` | Remove `HookType` match + `_ => continue`; set `source_domain = "claude-code"` for all hook-path records; apply size/depth security bounds; use `DomainPackRegistry` for passthrough source_domain resolution |
| `crates/unimatrix-server/src/uds/listener.rs` | Update any direct `HookType` references (Wave 4) |
| `crates/unimatrix-server/src/background.rs` | Update any direct `HookType` references (Wave 4) |
| `crates/unimatrix-server/src/lib.rs` | Thread `DomainPackRegistry` as `Arc` into `SqlObservationSource` at startup |

---

## Data Structures

### ObservationRecord (unimatrix-core/src/observation.rs)

```rust
pub struct ObservationRecord {
    pub ts: u64,
    pub event_type: String,       // replaces hook: HookType
    pub source_domain: String,    // new; "claude-code", "sre", "unknown", etc.
    pub session_id: String,
    pub tool: Option<String>,
    pub input: Option<serde_json::Value>,
    pub response_size: Option<u64>,
    pub response_snippet: Option<String>,
}
```

### HookType constants (unimatrix-core/src/observation.rs)

```rust
pub mod hook_type {
    pub const PRETOOLUSE: &str = "PreToolUse";
    pub const POSTTOOLUSE: &str = "PostToolUse";
    pub const SUBAGENTSTART: &str = "SubagentStart";
    pub const SUBAGENTSTOPPED: &str = "SubagentStop";
}
```

### DomainPack and DomainPackRegistry (unimatrix-observe/src/domain/)

```rust
pub struct DomainPack {
    pub source_domain: String,
    pub event_types: Vec<String>,
    pub categories: Vec<String>,
    pub rules: Vec<RuleDescriptor>,
}

pub struct DomainPackRegistry {
    inner: Arc<RwLock<HashMap<String, DomainPack>>>,
}

impl DomainPackRegistry {
    pub fn new(packs: Vec<DomainPack>) -> Self;
    pub fn lookup(&self, source_domain: &str) -> Option<DomainPack>;
    pub fn rules_for_domain(&self, source_domain: &str) -> Vec<Box<dyn DetectionRule>>;
    pub fn resolve_source_domain(&self, event_type: &str) -> String;
}
```

### RuleDescriptor (unimatrix-observe/src/domain/)

```rust
pub enum RuleDescriptor {
    Threshold(ThresholdRule),
    TemporalWindow(TemporalWindowRule),
}

pub struct ThresholdRule {
    pub name: String,
    pub source_domain: String,        // REQUIRED — startup validation rejects absent
    pub event_type_filter: Vec<String>,
    pub field_path: String,           // json_pointer; empty = count events
    pub threshold: f64,
    pub severity: String,
    pub claim_template: String,
}

pub struct TemporalWindowRule {
    pub name: String,
    pub source_domain: String,
    pub event_type_filter: Vec<String>,
    pub window_secs: u64,             // must be > 0; startup validation rejects 0
    pub threshold: f64,
    pub severity: String,
    pub claim_template: String,
}
```

### RuleEvaluator (unimatrix-observe/src/domain/)

```rust
pub struct RuleEvaluator {
    descriptor: RuleDescriptor,
}

impl DetectionRule for RuleEvaluator {
    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>;
    // 1. Filter by source_domain (mandatory preamble)
    // 2. Filter by event_type_filter
    // 3. Threshold: count (or extract field_path numeric value)
    // 4. TemporalWindow: sort by ts, two-pointer sliding window max-count
    // 5. Emit HotspotFinding if threshold exceeded
}
```

### MetricVector (unimatrix-store/src/metrics.rs)

```rust
pub struct MetricVector {
    pub computed_at: u64,
    pub universal: UniversalMetrics,            // unchanged; claude-code canonical
    pub phases: BTreeMap<String, PhaseMetrics>, // unchanged
    pub domain_metrics: HashMap<String, f64>,   // NEW; empty for claude-code sessions
}
```

### ObservationConfig / DomainPackConfig (unimatrix-server/src/infra/config.rs)

```rust
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct ObservationConfig {
    pub domain_packs: Vec<DomainPackConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DomainPackConfig {
    pub source_domain: String,
    pub event_types: Vec<String>,
    pub categories: Vec<String>,
    #[serde(default)]
    pub rule_file: Option<PathBuf>,
}
```

### ObserveError variants (unimatrix-observe/src/error.rs)

```rust
PayloadTooLarge { session_id: String, event_type: String, size: usize }
PayloadNestingTooDeep { session_id: String, event_type: String, depth: usize }
InvalidSourceDomain { domain: String }
InvalidRuleDescriptor { rule_name: String, reason: String }
```

---

## Function Signatures

```rust
// unimatrix-observe/src/domain/mod.rs
impl DomainPackRegistry {
    pub fn new(packs: Vec<DomainPack>) -> Self;
    pub fn with_builtin_claude_code() -> Self;  // always loads built-in pack
    pub fn lookup(&self, source_domain: &str) -> Option<DomainPack>;
    pub fn rules_for_domain(&self, source_domain: &str) -> Vec<Box<dyn DetectionRule>>;
    pub fn resolve_source_domain(&self, event_type: &str) -> String;
    // Returns the source_domain for a known event_type, or "unknown" if unregistered
}

// unimatrix-observe/src/detection/mod.rs (extended)
pub fn default_rules() -> Vec<Box<dyn DetectionRule>>;  // 21 claude-code rules, unchanged count
pub fn domain_rules(pack: &DomainPack) -> Vec<Box<dyn DetectionRule>>;  // RuleEvaluator instances

// unimatrix-server/src/services/observation.rs (modified)
fn parse_observation_rows(
    rows: Vec<RawObservationRow>,
    registry: &DomainPackRegistry,
) -> Vec<ObservationRecord>;
// No longer drops unknown event_type; sets source_domain from registry or "unknown"
// Applies security bounds: size check, depth check, source_domain validation

// unimatrix-server/src/services/observation.rs (new helper)
fn json_depth(v: &serde_json::Value, current: usize, max: usize) -> bool;
// Returns false if nesting exceeds max; O(n) walk; short-circuits at max+1
```

---

## Constraints

1. **No wire protocol changes**: `ImplantEvent`, `HookRequest`, `HookResponse` in `unimatrix-engine/src/wire.rs` are frozen.
2. **No `observations` table migration**: `hook` column is already TEXT. Only `OBSERVATION_METRICS` changes.
3. **`UniversalMetrics` typed struct is the only canonical representation**: `MetricVector.universal` stays `UniversalMetrics`. `domain_metrics` is a separate extension field — not a second representation.
4. **No runtime domain pack registration**: domain pack changes require a server restart. No MCP write path to `DomainPackRegistry`.
5. **Rule DSL is bounded**: only `threshold` and `temporal_window` operators. No `eval`, no script files, no dynamic loading. If a rule cannot be expressed in two operators, it must be a Rust `DetectionRule`.
6. **All 21 rule implementations plus their tests must be updated in the same PR**: no external consumers outside the workspace.
7. **`source_domain` guard is mandatory** in every domain-specific `DetectionRule::detect()`; it is the first filter, not an optimization. Gate-3a checklist enforces this.
8. **No new crate dependencies**: `serde_json::Value::pointer` (already transitive) plus the `RuleEvaluator` host struct are sufficient.
9. **Compilation gates**: `cargo check --workspace` must pass after each of the four waves before proceeding to the next.
10. **`"unknown"` is a reserved `source_domain`**: domain packs attempting to register `source_domain = "unknown"` must be rejected at startup.
11. **`window_secs = 0` is invalid**: rejected at startup with `InvalidRuleDescriptor`.
12. **Temporal window rules**: `detect()` must sort (or verify sort order of) the input slice by `ts` before the two-pointer scan.

---

## Dependencies

### Internal Crates (modified)

| Crate | Role |
|-------|------|
| `unimatrix-core` | Owns `ObservationRecord` (field change) and `HookType` constants module |
| `unimatrix-observe` | Retrospective pipeline: detection rules, metrics, extraction rules, new `domain/` module |
| `unimatrix-store` | `MetricVector`, `UniversalMetrics`, migration v13→v14 |
| `unimatrix-server` | Config extension (`ObservationConfig`), ingest boundary (`parse_observation_rows`), startup wiring |

### External Crates (existing transitive dependencies — no new additions)

| Crate | Usage |
|-------|-------|
| `serde` / `serde_json` | TOML deserialization for domain pack config; `Value::pointer` for DSL payload extraction |
| `toml` | Already used by `config.rs`; extended for `ObservationConfig` |
| `tokio` | Async server runtime (unchanged) |
| `rayon` | Detection rule execution via `spawn_blocking` (unchanged) |

### External Services

None. This feature is entirely server-side.

---

## NOT in Scope

- Changing `ImplantEvent`, `HookRequest`, `HookResponse` in `unimatrix-engine`.
- Migrating or renaming the `observations.hook` column (it stays as TEXT, column name unchanged).
- Implementing domain packs for specific non-Claude-Code domains (SRE, environmental monitoring). Only the "claude-code" built-in pack ships.
- Adding a new 13th MCP tool for domain pack registration (FR-06 and AC-08 removed per ADR-002 and human confirmation).
- `RetrospectiveReport` wire shape changes; `context_cycle_review` MCP interface is unchanged.
- W3-1 GNN training label generation (enabled by this feature, not built here).
- Runtime domain pack hot-reload without restart.
- Changing session lifecycle paths (`SessionStart`, `Stop`, `TaskCompleted`) or synchronous injection paths (`UserPromptSubmit`, `PreCompact`).
- Database migrations for `OUTCOME_INDEX` or `BaselineSet`.
- Confidence system changes (lambda, weights, Wilson score).

---

## Alignment Status

**Overall: PASS** (one variance resolved prior to implementation)

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly addresses the `HookType` coupling gap and metric schema gap named in PRODUCT-VISION.md; W1-5 goals met verbatim |
| Milestone Fit | PASS | Wave 1 final item; W0-x dependencies all COMPLETE; effort estimate matches vision (5–7 days) |
| Architecture Consistency | PASS | All seven ADRs are internally consistent; all scope open questions resolved |
| Risk Completeness | PASS | 14 risks mapped; all scope risks traced; non-negotiable tests named |
| FR-06 Conflict | **RESOLVED** | SPECIFICATION.md retained FR-06 (Admin runtime domain pack override) in contradiction of ADR-002. Human confirmed config-only is correct. FR-06 (four sub-requirements), Workflow 3, AC-08, and OQ-01 are treated as removed from spec. `DomainPackRegistry` has no MCP write path in W1-5. |
| Spec additions beyond scope | WARN (benign) | EC-04 ("unknown" reserved domain), EC-07 (overlapping event_type across packs), EC-08/09 (rule validation edge cases) added in risk strategy — consistent with architecture, undeclared in SCOPE.md. All are constraints on implementation, not new functional capabilities. |

**Critical implementor obligations from alignment review**:
- Every `DetectionRule::detect()` must apply `source_domain` guard as its first filter (ADR-005; gate-3a checklist item).
- `DomainPackRegistry` must be threaded as `Arc` into `SqlObservationSource` at startup; failing to inject it causes all events to resolve to `source_domain = "unknown"` and all 21 rules to silently produce no findings (IR-01).
- `compute_universal()` in `metrics.rs` must guard on `source_domain == "claude-code"` before counting any events (IR-03).
- Four-wave compilation gate discipline is non-negotiable (ADR-004); each wave must compile before the next begins.
