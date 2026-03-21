# col-023 Pseudocode Overview

## Feature Summary

Replace the `HookType` closed enum with string-typed `event_type`/`source_domain` fields
across the observation pipeline. Introduce a TOML-configured `DomainPackRegistry`.
Rewrite all 21 detection rules with mandatory `source_domain` guards. Extend
`OBSERVATION_METRICS` with a nullable `domain_metrics_json` column (schema v14).

## Components and Wave Assignments

| Wave | Component | Crate | File(s) |
|------|-----------|-------|---------|
| 1 | observation-record | unimatrix-core | `src/observation.rs` |
| 2 | domain-pack-registry | unimatrix-observe | `src/domain/mod.rs` (new module) |
| 2 | rule-dsl-evaluator | unimatrix-observe | `src/domain/mod.rs` (same module) |
| 2 | config-extension | unimatrix-server | `src/infra/config.rs` |
| 3 | detection-rules | unimatrix-observe | `src/detection/{agent,friction,session,scope}.rs` + `src/metrics.rs` + `src/extraction/*.rs` |
| 3 | metrics-extension | unimatrix-store | `src/metrics.rs` |
| 3 | schema-migration | unimatrix-store | `src/migration.rs` |
| 4 | ingest-security | unimatrix-server | `src/services/observation.rs` + `src/lib.rs` |

Wave 2 components are independent of each other and all depend on Wave 1.
Wave 3 components are independent of each other and all depend on Wave 2.
Wave 4 depends on all of Wave 1-3.

Compilation gate: `cargo check --workspace` must pass after each wave before proceeding.

## Data Flow

```
[UDS hook ingress]
     |
     | event_type = raw string (e.g., "PostToolUse")
     | source_domain NOT declared by client
     v
[parse_observation_rows — ingest-security]
     | 1. size check (< 64 KB raw bytes)
     | 2. json_depth check (<= 10 levels) on parsed value
     | 3. source_domain = "claude-code" for all hook-path records
     | 4. source_domain validation against regex ^[a-z0-9_-]{1,64}$
     | 5. DomainPackRegistry.resolve_source_domain(event_type)
     |    → "claude-code" if in claude-code pack, else "unknown"
     v
[ObservationRecord { event_type: String, source_domain: String, ... }]
     |
     v
[detect_hotspots]
     | default_rules() — 21 claude-code Rust rules
     |   each: filter source_domain == "claude-code" FIRST
     | domain_rules(pack) — RuleEvaluator instances per registered pack
     |   each: filter source_domain == pack.source_domain FIRST
     v
[compute_metric_vector]
     | compute_universal() — guards source_domain == "claude-code"
     | domain_metrics: HashMap<String, f64> for non-claude-code sessions
     v
[MetricVector { universal: UniversalMetrics, domain_metrics: HashMap<String, f64> }]
     |
     v
[store_metrics / get_metrics — schema v14]
     | 21 typed columns (unchanged) + domain_metrics_json TEXT NULL (new)
     v
[RetrospectiveReport — unchanged external shape]
```

## Shared Types Introduced or Modified

### ObservationRecord (Wave 1 output, consumed everywhere)

```
struct ObservationRecord:
    ts: u64
    event_type: String           -- replaces hook: HookType
    source_domain: String        -- NEW; set server-side at ingest
    session_id: String
    tool: Option<String>
    input: Option<serde_json::Value>
    response_size: Option<u64>
    response_snippet: Option<String>
```

### hook_type constants (Wave 1 output, documentation only)

```
mod hook_type:
    PRETOOLUSE    = "PreToolUse"
    POSTTOOLUSE   = "PostToolUse"
    SUBAGENTSTART = "SubagentStart"
    SUBAGENTSTOPPED = "SubagentStop"
```

### DomainPack (Wave 2 output)

```
struct DomainPack:
    source_domain: String
    event_types: Vec<String>
    categories: Vec<String>
    rules: Vec<RuleDescriptor>
```

### DomainPackRegistry (Wave 2 output, threaded as Arc into server)

```
struct DomainPackRegistry:
    inner: Arc<RwLock<HashMap<String, DomainPack>>>

methods:
    new(packs: Vec<DomainPack>) -> Self
    with_builtin_claude_code() -> Self
    lookup(source_domain: &str) -> Option<DomainPack>
    rules_for_domain(source_domain: &str) -> Vec<Box<dyn DetectionRule>>
    resolve_source_domain(event_type: &str) -> String
```

### RuleDescriptor enum (Wave 2 output)

```
enum RuleDescriptor:
    Threshold(ThresholdRule)
    TemporalWindow(TemporalWindowRule)

struct ThresholdRule:
    name: String
    source_domain: String      -- REQUIRED; validated at startup
    event_type_filter: Vec<String>
    field_path: String         -- json_pointer; empty = count events
    threshold: f64
    severity: String
    claim_template: String

struct TemporalWindowRule:
    name: String
    source_domain: String      -- REQUIRED; validated at startup
    event_type_filter: Vec<String>
    window_secs: u64           -- must be > 0; validated at startup
    threshold: f64
    severity: String
    claim_template: String
```

### MetricVector (Wave 3 output, schema v14)

```
struct MetricVector:
    computed_at: u64
    universal: UniversalMetrics              -- unchanged; 21 fields
    phases: BTreeMap<String, PhaseMetrics>   -- unchanged
    domain_metrics: HashMap<String, f64>     -- NEW; empty for claude-code
```

### ObserveError new variants (Wave 2 output, consumed in Wave 4)

```
PayloadTooLarge { session_id: String, event_type: String, size: usize }
PayloadNestingTooDeep { session_id: String, event_type: String, depth: usize }
InvalidSourceDomain { domain: String }
InvalidRuleDescriptor { rule_name: String, reason: String }
```

### ObservationConfig / DomainPackConfig (Wave 2 output)

```
struct ObservationConfig:
    domain_packs: Vec<DomainPackConfig>    -- serde(default) = empty vec

struct DomainPackConfig:
    source_domain: String
    event_types: Vec<String>
    categories: Vec<String>
    rule_file: Option<PathBuf>             -- serde(default) = None
```

## Sequencing Constraints

1. Wave 1 must land before any other wave begins — `ObservationRecord` is the shared
   foundation. All downstream crate compilations break after Wave 1 until their files
   are also updated.

2. Wave 2 components can be developed in parallel (they are in separate crates / modules)
   but all three must be done before Wave 3 begins, because:
   - `detection-rules` imports `DomainPackRegistry` types for the `domain_rules()` function
   - `metrics-extension` and `schema-migration` do not depend on Wave 2, but the
     full `compute_metric_vector` signature must match the new `MetricVector` type

3. `domain-pack-registry` and `rule-dsl-evaluator` live in the same new module
   (`unimatrix-observe/src/domain/mod.rs`) and should be written together.

4. Wave 4 (`ingest-security`) must be last because it wires `DomainPackRegistry` into
   `SqlObservationSource` and updates all test fixtures.

5. `cargo check --workspace` gate must pass after Wave 1, Wave 2, Wave 3, and Wave 4.
   The final gate is `cargo test --workspace`.
