# C2: Types (unimatrix-observe/src/types.rs)

## Purpose

Define new structs for multi-session retrospective data and extend `RetrospectiveReport` with optional fields. All new types use Serialize/Deserialize for JSON transport via MCP.

## New Structs

### SessionSummary

```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub started_at: u64,           // epoch millis (earliest observation ts)
    pub duration_secs: u64,        // (max_ts - min_ts) / 1000
    pub tool_distribution: HashMap<String, u64>,  // category -> PreToolUse count
    pub top_file_zones: Vec<(String, u64)>,       // (directory_zone, count), max 5
    pub agents_spawned: Vec<String>,              // from SubagentStart tool names
    pub knowledge_in: u64,         // context_search + context_lookup + context_get count
    pub knowledge_out: u64,        // context_store count
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,   // from SessionRecord, populated by handler
}
```

### KnowledgeReuse

```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeReuse {
    pub tier1_reuse_count: u64,                   // distinct entry IDs reused cross-session
    pub by_category: HashMap<String, u64>,         // category -> reuse count
    pub category_gaps: Vec<String>,                // categories with active entries but zero reuse
}
```

### AttributionMetadata

```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionMetadata {
    pub attributed_session_count: usize,  // sessions with direct feature_cycle match
    pub total_session_count: usize,       // all discovered sessions including fallback
}
```

## Modified Struct: RetrospectiveReport

Add five new optional fields after the existing `recommendations` field:

```
// Add to RetrospectiveReport:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub session_summaries: Option<Vec<SessionSummary>>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub knowledge_reuse: Option<KnowledgeReuse>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub rework_session_count: Option<u64>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub context_reload_pct: Option<f64>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub attribution: Option<AttributionMetadata>,
```

## Module Export (unimatrix-observe/src/lib.rs)

Add to the `pub use types::` block:

```
pub use types::{
    // existing exports unchanged...
    SessionSummary, KnowledgeReuse, AttributionMetadata,
};
```

## Update build_report() Return

The `build_report()` function in report.rs constructs `RetrospectiveReport`. Add the five new fields initialized to `None`:

```
// In build_report(), add to the RetrospectiveReport struct literal:
session_summaries: None,
knowledge_reuse: None,
rework_session_count: None,
context_reload_pct: None,
attribution: None,
```

## Update Cached Report Construction

In the handler's cached path (tools.rs ~line 1100), the `RetrospectiveReport` struct literal must also include the five new fields set to `None`.

## Error Handling

No error handling needed -- these are plain data types.

## Key Test Scenarios

1. **Backward-compatible deserialization (R-09)**: Deserialize pre-col-020 JSON (without new fields) into updated `RetrospectiveReport`. All new fields must be `None`. This validates `serde(default)`.
2. **Skip-serializing when None (R-09)**: Serialize a report with all new fields as `None`. JSON output must not contain `session_summaries`, `knowledge_reuse`, `rework_session_count`, `context_reload_pct`, or `attribution` keys.
3. **Round-trip with data present**: Serialize a report with all new fields populated, deserialize back, verify all data preserved.
4. **SessionSummary serde round-trip**: Create a SessionSummary with all fields populated, serialize/deserialize, verify equality.
5. **KnowledgeReuse serde round-trip**: Same as above for KnowledgeReuse.
6. **AttributionMetadata serde round-trip**: Same as above for AttributionMetadata.
