# Pseudocode: observe-types

## Purpose

Define all shared types for the unimatrix-observe crate. All other modules import from here.

## File: `crates/unimatrix-observe/src/types.rs`

### HookType Enum

```
enum HookType {
    PreToolUse,
    PostToolUse,
    SubagentStart,
    SubagentStop,
}
```

Derive: Debug, Clone, PartialEq, Eq, Serialize, Deserialize. Serde rename_all = "PascalCase".

### ObservationRecord

```
struct ObservationRecord {
    ts: u64,                              // Unix epoch milliseconds
    hook: HookType,
    session_id: String,
    tool: Option<String>,                 // tool_name or agent_type
    input: Option<serde_json::Value>,     // tool_input or prompt_snippet
    response_size: Option<u64>,           // PostToolUse only
    response_snippet: Option<String>,     // First 500 chars, PostToolUse only
}
```

Derive: Debug, Clone, Serialize, Deserialize.

### SessionFile

```
struct SessionFile {
    path: PathBuf,
    session_id: String,
    size_bytes: u64,
    modified_at: u64,     // Unix epoch seconds
}
```

Derive: Debug, Clone.

### ParsedSession

```
struct ParsedSession {
    session_id: String,
    records: Vec<ObservationRecord>,
}
```

Derive: Debug, Clone.

### ObservationStats

```
struct ObservationStats {
    file_count: u64,
    total_size_bytes: u64,
    oldest_file_age_days: u64,
    approaching_cleanup: Vec<String>,  // session_ids approaching 60-day threshold
}
```

Derive: Debug, Clone.

### HotspotCategory Enum

```
enum HotspotCategory {
    Agent,
    Friction,
    Session,
    Scope,
}
```

Derive: Debug, Clone, PartialEq, Eq, Serialize, Deserialize.

### Severity Enum

```
enum Severity {
    Info,
    Warning,
    Critical,
}
```

Derive: Debug, Clone, PartialEq, Eq, Serialize, Deserialize.

### EvidenceRecord

```
struct EvidenceRecord {
    description: String,
    ts: u64,
    tool: Option<String>,
    detail: String,
}
```

Derive: Debug, Clone, Serialize, Deserialize.

### HotspotFinding

```
struct HotspotFinding {
    category: HotspotCategory,
    severity: Severity,
    rule_name: String,
    claim: String,
    measured: f64,
    threshold: f64,
    evidence: Vec<EvidenceRecord>,
}
```

Derive: Debug, Clone, Serialize, Deserialize.

### UniversalMetrics

```
struct UniversalMetrics {
    total_tool_calls: u64,
    total_duration_secs: u64,
    session_count: u64,
    search_miss_rate: f64,
    edit_bloat_total_kb: f64,
    edit_bloat_ratio: f64,
    permission_friction_events: u64,
    bash_for_search_count: u64,
    cold_restart_events: u64,
    coordinator_respawn_count: u64,
    parallel_call_rate: f64,
    context_load_before_first_write_kb: f64,
    total_context_loaded_kb: f64,
    post_completion_work_pct: f64,
    follow_up_issues_created: u64,
    knowledge_entries_stored: u64,
    sleep_workaround_count: u64,
    agent_hotspot_count: u64,
    friction_hotspot_count: u64,
    session_hotspot_count: u64,
    scope_hotspot_count: u64,
}
```

Derive: Debug, Clone, Serialize, Deserialize, Default. All fields `#[serde(default)]` for forward compatibility (R-04).

### PhaseMetrics

```
struct PhaseMetrics {
    duration_secs: u64,
    tool_call_count: u64,
}
```

Derive: Debug, Clone, Serialize, Deserialize, Default. `#[serde(default)]`.

### MetricVector

```
struct MetricVector {
    computed_at: u64,
    universal: UniversalMetrics,
    phases: BTreeMap<String, PhaseMetrics>,
}
```

Derive: Debug, Clone, Serialize, Deserialize. `#[serde(default)]` on all fields.

### RetrospectiveReport

```
struct RetrospectiveReport {
    feature_cycle: String,
    session_count: usize,
    total_records: usize,
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
    is_cached: bool,
}
```

Derive: Debug, Clone, Serialize, Deserialize.

### Serialization Helpers (ADR-002)

```
fn serialize_metric_vector(mv: &MetricVector) -> Result<Vec<u8>> {
    bincode::serde::encode_to_vec(mv, bincode::config::standard())
}

fn deserialize_metric_vector(bytes: &[u8]) -> Result<MetricVector> {
    let (mv, _) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
    return mv
}
```

Follow workspace pattern from `unimatrix-store/src/schema.rs` serialize_entry.

### Error Handling

Define `ObserveError` enum:
```
enum ObserveError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Serialization(String),
    TimestampParse(String),
}
```

Implement Display, Error, From<io::Error>, From<serde_json::Error>.

Define `type Result<T> = std::result::Result<T, ObserveError>;`

## Key Test Scenarios

- MetricVector roundtrip serialization (R-04)
- MetricVector with all-default fields deserializes correctly
- MetricVector with populated phases roundtrips
- Verify #[serde(default)] on all MetricVector fields
- HookType serde roundtrip for all 4 variants
- ObservationRecord serde roundtrip
