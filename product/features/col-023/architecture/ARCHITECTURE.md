# col-023: Observation Pipeline Generalization — Architecture

## System Overview

The observation pipeline ingests Claude Code hook events, stores them in the `observations`
table, groups them into sessions, and runs a retrospective analysis pipeline: detection
rules emit hotspot findings, a metric vector is computed, and a `RetrospectiveReport` is
returned via `context_cycle_review`.

The pipeline is currently hardwired to Claude Code's hook model at three coupling points:
1. `ObservationRecord.hook: HookType` — closed 4-variant enum in `unimatrix-core`
2. `UniversalMetrics` — 21 hardcoded Claude Code-specific metric fields
3. 21 detection rules — match on `HookType` variants and Claude Code tool names

col-023 breaks all three couplings by replacing the typed enum with string fields, making
the metric extension point configurable, and rewriting rules to be domain-aware. The wire
protocol, observations table schema (for core columns), and MCP interface are unchanged.

This feature directly enables W3-1 (GNN training signal pipeline) by providing a
domain-neutral event substrate for implicit training labels.

## Component Breakdown

### unimatrix-core/src/observation.rs (modified)

Owns the `ObservationRecord` struct and previously owned `HookType`. After this feature:
- `ObservationRecord`: replaces `hook: HookType` with `event_type: String` and
  `source_domain: String`. All other fields unchanged.
- `HookType` enum: deprecated in-place as a `pub mod hook_type` constants module with
  `const PRETOOLUSE: &str = "PreToolUse"` etc. — retained for documentation only, not
  used in hot paths.

Responsibility: shared event record type used by both `unimatrix-observe` and
`unimatrix-server`. Must compile before either crate.

### unimatrix-observe/src/ (heavily modified)

Owns the retrospective analysis pipeline. After this feature:
- **types.rs**: removes `HookType` re-export; `ObservationRecord` imported from
  `unimatrix-core` with new fields
- **detection/{agent,friction,session,scope}.rs**: all 21 rules rewritten; each applies
  `source_domain == "claude-code"` filter as first operation (ADR-005)
- **metrics.rs**: `compute_universal()` rewrites `HookType` comparisons to string
  comparisons with `source_domain` guards; `compute_metric_vector()` signature unchanged
- **extraction/{recurring_friction,...}.rs**: extraction rules use `event_type` and
  `source_domain` string comparisons
- **detection/mod.rs**: `default_rules()` returns the same 21 rules, now with string-based
  matching; new `domain_rules(pack: &DomainPack) -> Vec<Box<dyn DetectionRule>>` returns
  `RuleEvaluator` instances for data-driven rules

Responsibility: retrospective analysis, domain-agnostic rule evaluation framework.
Independent of `unimatrix-store` (preserved from ADR-002 col-012).

### unimatrix-observe/src/domain/ (new module)

New module containing:
- `DomainPack` struct: `source_domain: String`, `event_types: Vec<String>`,
  `categories: Vec<String>`, `rules: Vec<RuleDescriptor>`
- `DomainPackRegistry`: `Arc<RwLock<HashMap<String, DomainPack>>>` initialized at startup
- `RuleDescriptor` enum: `Threshold(ThresholdRule)` | `TemporalWindow(TemporalWindowRule)`
- `RuleEvaluator` struct: implements `DetectionRule` for data-driven rules
- Default `"claude-code"` pack definition (const)

Responsibility: registry of domain packs; produces `Vec<Box<dyn DetectionRule>>` for any
registered domain; validates rule descriptors at load time.

### unimatrix-store/src/metrics.rs (modified)

Owns `UniversalMetrics`, `MetricVector`, `UNIVERSAL_METRICS_FIELDS`. After this feature:
- `MetricVector` gains `domain_metrics: HashMap<String, f64>` field (empty for claude-code)
- `UNIVERSAL_METRICS_FIELDS` const unchanged (21 entries, still in declaration order)
- `store_metrics()` / `get_metrics()`: writes/reads `domain_metrics_json` column

Responsibility: typed metric storage and retrieval; canonical representation for
claude-code metrics (ADR-006).

### unimatrix-store/src/migration.rs (modified)

Schema v13 → v14 migration:
```sql
ALTER TABLE OBSERVATION_METRICS ADD COLUMN domain_metrics_json TEXT NULL;
```
Single `ALTER TABLE ADD COLUMN` with NULL default. This is the only migration required.
`CURRENT_SCHEMA_VERSION` increments to 14.

### unimatrix-server/src/infra/config.rs (modified)

Adds `ObservationConfig` section and `DomainPackConfig` struct to `UnimatrixConfig`:
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
Follows exact `#[serde(default)]` pattern of `KnowledgeConfig`. Absent `[observation]`
section defaults to empty `domain_packs` vec — the built-in claude-code pack is always
loaded regardless.

### unimatrix-server/src/services/observation.rs (modified)

`parse_observation_rows()`: removes the `HookType` match arm and `_ => continue` filter.
Now constructs `ObservationRecord` with:
- `event_type` = the raw `hook_str` value from DB
- `source_domain` = `"claude-code"` (all hook-path records)
- All other fields unchanged

The `source_domain` assignment for records with event types not in any registered domain
pack is `"unknown"` — handled by the `DomainPackRegistry` lookup: if the event type
matches a known domain, that domain is used; otherwise `"unknown"`.

Input deserialization logic (SubagentStart → `Value::String`, others → JSON parse) is
preserved — it is `event_type`-conditional, not `source_domain`-conditional.

Security bounds (payload size, depth) are applied here per ADR-007.

## Component Interactions

```
[hook ingress: uds/hook.rs]
        │ build_request() → RecordEvent / RecordEvents
        │
[observation service: services/observation.rs]
        │ parse_observation_rows()
        │   source_domain = "claude-code" (inferred from ingress path)
        │   security bounds: payload size, depth, source_domain format
        │
[ObservationRecord: unimatrix-core]
        │ event_type: String, source_domain: String, ...
        │
[DomainPackRegistry: unimatrix-observe/src/domain/]
        │ lookup(event_type) → source_domain confirmation
        │ domain_rules(pack) → Vec<Box<dyn DetectionRule>>
        │
[detect_hotspots: unimatrix-observe/src/detection/]
        │ 21 built-in rules (claude-code pack) +
        │ N data-driven RuleEvaluator rules (external packs)
        │ each rule: source_domain guard first (ADR-005)
        │
[compute_metric_vector: unimatrix-observe/src/metrics.rs]
        │ compute_universal() for claude-code metrics
        │ domain_metrics HashMap for non-claude-code
        │
[MetricVector: unimatrix-store]
        │ universal: UniversalMetrics (typed, unchanged)
        │ domain_metrics: HashMap<String, f64> (new)
        │
[store_metrics: unimatrix-store]
        │ 21 typed columns (unchanged) + domain_metrics_json (new)
        │
[RetrospectiveReport: unimatrix-observe/src/report.rs]
        │ unchanged external shape
        ▼
[context_cycle_review: MCP tool]
```

## Technology Decisions

| Decision | Choice | ADR |
|----------|--------|-----|
| Replace HookType enum | String fields event_type + source_domain | ADR-001 |
| Domain registration mechanism | TOML config at startup, no new MCP tool | ADR-002 |
| External rule DSL | Two-kind bounded evaluator (threshold, temporal_window) | ADR-003 |
| HookType blast radius management | Four-wave compilation-gated refactor | ADR-004 |
| Cross-domain false findings | Mandatory source_domain guard in all rules | ADR-005 |
| Metric canonical representation | UniversalMetrics typed struct (Option A) | ADR-006 |
| Ingest security bounds | 64 KB / depth-10 / domain regex at parse_observation_rows | ADR-007 |
| Schema migration | v13 → v14: single ALTER TABLE ADD COLUMN | ADR-006 |
| Runtime Admin re-registration | Removed from scope (config-only) | ADR-002 |

## Integration Points

### Unchanged (explicitly out of scope)

- `ImplantEvent` / `HookRequest` / `HookResponse` in `unimatrix-engine` — wire protocol
  is already generic; no changes
- `observations` table core columns — `hook` column is TEXT already; no migration needed
  for core columns
- `context_cycle_review` MCP tool — `RetrospectiveReport` shape is unchanged
- `CategoryAllowlist` behavior — domain packs contribute categories at startup via the
  existing `from_categories()` path

### Changed

- `ObservationRecord` in `unimatrix-core` — all 25 callsite files updated atomically
- `parse_observation_rows` in `unimatrix-server` — no longer drops unknown event types
- `OBSERVATION_METRICS` table — gains `domain_metrics_json TEXT NULL` column in v14
- `MetricVector` in `unimatrix-store` — gains `domain_metrics: HashMap<String, f64>`
- `UnimatrixConfig` in `unimatrix-server` — gains `observation: ObservationConfig`

## Integration Surface

| Integration Point | Type / Signature | Source | Notes |
|-------------------|-----------------|--------|-------|
| `ObservationRecord` | `struct { ts: u64, event_type: String, source_domain: String, session_id: String, tool: Option<String>, input: Option<Value>, response_size: Option<u64>, response_snippet: Option<String> }` | `unimatrix-core/src/observation.rs` | Replaces `hook: HookType` |
| `DetectionRule::detect` | `fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>` | `unimatrix-observe/src/detection/mod.rs` | Signature unchanged; records now heterogeneous |
| `DomainPackRegistry::new` | `fn new(packs: Vec<DomainPack>) -> Self` | `unimatrix-observe/src/domain/` (new) | Initialized at startup with config packs |
| `DomainPackRegistry::rules_for_domain` | `fn rules_for_domain(&self, source_domain: &str) -> Vec<Box<dyn DetectionRule>>` | same | Returns data-driven rules for a domain |
| `MetricVector` | `struct { computed_at: u64, universal: UniversalMetrics, phases: BTreeMap<String, PhaseMetrics>, domain_metrics: HashMap<String, f64> }` | `unimatrix-store/src/metrics.rs` | Adds `domain_metrics` field |
| `ObservationConfig` | `struct { domain_packs: Vec<DomainPackConfig> }` | `unimatrix-server/src/infra/config.rs` | New section; `#[serde(default)]` |
| `DomainPackConfig` | `struct { source_domain: String, event_types: Vec<String>, categories: Vec<String>, rule_file: Option<PathBuf> }` | same | Deserialized from `[[observation.domain_packs]]` |
| `ObserveError::PayloadTooLarge` | `{ session_id: String, event_type: String, size: usize }` | `unimatrix-observe/src/error.rs` | New variant |
| `ObserveError::PayloadNestingTooDeep` | `{ session_id: String, event_type: String, depth: usize }` | same | New variant |
| `ObserveError::InvalidSourceDomain` | `{ domain: String }` | same | New variant |
| `UNIVERSAL_METRICS_FIELDS` | `&[&str]` (21 entries, unchanged) | `unimatrix-store/src/metrics.rs` | Unchanged const; structural test updated |
| Schema v14 migration | `ALTER TABLE OBSERVATION_METRICS ADD COLUMN domain_metrics_json TEXT NULL` | `unimatrix-store/src/migration.rs` | Only OBSERVATION_METRICS changes |

## Data Flow: RecordEvent Path (unchanged externally, generalized internally)

```
1. hook CLI: event_type = "PostToolUse", payload = {...}
2. build_request() → HookRequest::RecordEvent { ... }
3. UDS handler → ObservationService::record_event()
4. parse_observation_rows():
   a. source_domain = "claude-code"  ← inferred from ingress, not payload
   b. payload size check (≤ 64 KB)
   c. payload depth check (≤ 10 levels)
   d. ObservationRecord { event_type: "PostToolUse", source_domain: "claude-code", ... }
5. INSERT INTO observations (hook = "PostToolUse", ...)  ← DB column name unchanged
6. On context_cycle_review:
   a. load_feature_observations() → Vec<ObservationRecord>
   b. detect_hotspots(records, all_rules)
      - each rule: filter to source_domain == "claude-code" first
      - 21 claude-code rules run unchanged semantics
   c. compute_metric_vector() → MetricVector { universal, phases, domain_metrics: {} }
   d. store_metrics() → OBSERVATION_METRICS (21 columns + domain_metrics_json NULL)
   e. RetrospectiveReport { ... }  ← unchanged shape
```

## Known Limitations and Open Questions for Spec Writer

**OQ-1: Spec scope of AC-08 (Admin runtime re-registration)**
The spawn prompt resolves SR-05 by removing Admin runtime re-registration from scope.
SCOPE.md Goal #5 still mentions it. The spec writer must explicitly mark AC-08 as
out-of-scope for W1-5 or remove it from the acceptance criteria. If AC-08 stays, the
spec must name the target existing tool and define its schema delta before implementation.

**OQ-2: W3-1 "fully functional" gate definition**
PRODUCT-VISION.md states W3-1 requires detection rules "fully functional for the
generalized event schema." The scope risk assessment (SR-08) notes this is ambiguous:
does W3-1 need multi-domain detection rules, or only that the pipeline accepts
multi-domain events? The spec writer should document the W3-1 unblocking condition
precisely: "pipeline accepts multi-domain events and non-claude-code sessions pass
through without panicking" is the narrower (achievable) reading; "domain-specific
detection rules must exist for W3-1 training domains" is the broader reading.

**OQ-3: source_domain in OUTCOME_INDEX vs. observations table**
The `observations.hook` column continues to store the raw `event_type` string (e.g.,
`"PostToolUse"`) — it does not store `source_domain`. If a future query needs to filter
observations by `source_domain`, an index or additional column on the `observations` table
would be needed. This is not required for W1-5 since source_domain is always inferred
server-side. The spec writer should confirm this is acceptable for the W3-1 training
signal requirements.

**OQ-4: rule_file path validation at startup**
When a domain pack specifies `rule_file = "/etc/unimatrix/sre-rules.toml"`, the file is
read and its rule descriptors are validated at startup. The spec must define: what error
is returned if the file is absent or malformed — startup failure (current ADR-007 decision)
or a warning with that pack's rules disabled? The current decision (startup failure) is
strictest but may be too disruptive for operators adding optional packs.

**OQ-5: HookType constants module retention**
ADR-001 retains `HookType` as a constants module for documentation. The spec writer must
decide: is this a public re-export from `unimatrix-core`, or is it private to the
claude-code domain pack? If public, it creates a stable API surface for tools that
want to reference claude-code event type names by constant rather than string literal.
