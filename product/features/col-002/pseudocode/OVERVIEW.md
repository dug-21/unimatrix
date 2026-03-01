# Pseudocode Overview: col-002 Retrospective Pipeline

## Components

| Component | Crate | Purpose |
|-----------|-------|---------|
| observe-types | unimatrix-observe | Shared types: ObservationRecord, MetricVector, HotspotFinding, etc. |
| observe-parser | unimatrix-observe | JSONL line parsing, ISO-8601 timestamp conversion |
| observe-attribution | unimatrix-observe | Feature attribution: session-to-feature mapping |
| observe-detection | unimatrix-observe | DetectionRule trait + 3 shipped rules |
| observe-metrics | unimatrix-observe | MetricVector computation from records + hotspots |
| observe-report | unimatrix-observe | RetrospectiveReport assembly |
| observe-files | unimatrix-observe | Session file discovery, age, cleanup |
| store-observation | unimatrix-store | OBSERVATION_METRICS table + CRUD methods |
| server-retrospective | unimatrix-server | context_retrospective MCP tool handler |
| server-status-ext | unimatrix-server | StatusReport extension + format updates |
| hooks | repo root | 4 shell scripts for Claude Code hook events |

## Data Flow

```
Hook scripts -> JSONL files -> parser -> ObservationRecord[]
  -> attribution (filter by feature) -> ObservationRecord[]
  -> detection (3 rules) -> HotspotFinding[]
  -> metrics (compute) -> MetricVector
  -> report (assemble) -> RetrospectiveReport
  -> server (store MetricVector, return report)
```

## Shared Types (observe-types)

All types live in `crates/unimatrix-observe/src/types.rs`. Key structs:

- `ObservationRecord` -- normalized hook event
- `HookType` -- enum: PreToolUse, PostToolUse, SubagentStart, SubagentStop
- `MetricVector` -- computed_at + UniversalMetrics + BTreeMap phases
- `UniversalMetrics` -- 20 fixed numeric fields
- `PhaseMetrics` -- duration_secs + tool_call_count
- `HotspotFinding` -- category, severity, rule_name, claim, measured, threshold, evidence
- `HotspotCategory` -- enum: Agent, Friction, Session, Scope
- `Severity` -- enum: Info, Warning, Critical
- `EvidenceRecord` -- description, ts, tool, detail
- `RetrospectiveReport` -- feature_cycle, session_count, total_records, metrics, hotspots, is_cached
- `SessionFile` -- path, session_id, size_bytes, modified_at
- `ObservationStats` -- file_count, total_size_bytes, oldest_file_age_days, approaching_cleanup
- `ParsedSession` -- session_id, records vec

## Serialization Boundary (ADR-002)

`unimatrix-observe` owns `serialize_metric_vector` and `deserialize_metric_vector` using `bincode::serde::encode_to_vec` / `decode_from_slice` with `bincode::config::standard()`. The store handles MetricVector as opaque `&[u8]`.

## Sequencing Constraints

1. observe-types must be defined first (all other modules import from it)
2. observe-parser has no internal dependencies beyond types
3. observe-files has no internal dependencies beyond types
4. observe-attribution depends on types only
5. observe-detection depends on types only
6. observe-metrics depends on types only
7. observe-report depends on types only
8. store-observation is independent of unimatrix-observe
9. server-retrospective depends on all observe modules + store-observation
10. server-status-ext depends on observe-files + store-observation
11. hooks are independent shell scripts

## Crate Root (lib.rs)

```
#![forbid(unsafe_code)]
pub mod types;
pub mod parser;
pub mod attribution;
pub mod detection;
pub mod metrics;
pub mod report;
pub mod files;

// Re-exports for public API
pub use types::*;
pub use parser::{parse_session_file, parse_timestamp};
pub use files::{discover_sessions, identify_expired, scan_observation_stats};
pub use attribution::attribute_sessions;
pub use detection::{detect_hotspots, default_rules, DetectionRule};
pub use metrics::compute_metric_vector;
pub use report::build_report;
```
