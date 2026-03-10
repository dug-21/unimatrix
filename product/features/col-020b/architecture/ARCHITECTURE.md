# col-020b Architecture: Retrospective Knowledge Metric Fixes

## System Overview

col-020b fixes two bugs in the col-020 retrospective pipeline and refines the knowledge metric semantics. The retrospective pipeline spans three crates:

- **unimatrix-observe** -- Pure computation. `session_metrics.rs` computes per-session profiles (tool distribution, knowledge flow counters). `types.rs` defines the data structures (`SessionSummary`, `KnowledgeReuse`, `RetrospectiveReport`). No database dependency.
- **unimatrix-server** -- Orchestration. `tools.rs` loads data from Store, calls observe's computation functions, assembles the `RetrospectiveReport`. `knowledge_reuse.rs` contains the pure computation for cross-session knowledge reuse (lives server-side per col-020 ADR-001 because it requires multi-table Store joins).
- **unimatrix-store** -- Data access. `query_log.rs` and `injection_log.rs` provide `scan_*_by_sessions` methods used by the knowledge reuse data flow.

The bugs are localized: #192 is entirely in `session_metrics.rs` (tool name matching). #193 spans `tools.rs` (data flow) and `knowledge_reuse.rs` (semantics). The fix does not change any Store queries, schema, or the ObservationSource trait.

## Component Breakdown

### C1: Tool Name Normalizer (`session_metrics.rs`)

**Responsibility:** Strip `mcp__unimatrix__` prefix from tool names before classification and counting.

**Location:** `crates/unimatrix-observe/src/session_metrics.rs`

**Interface:**
```rust
/// Strip MCP server prefix from tool names.
/// Returns the bare tool name for Unimatrix MCP tools,
/// or the input unchanged for Claude-native tools.
fn normalize_tool_name(tool: &str) -> &str {
    tool.strip_prefix("mcp__unimatrix__").unwrap_or(tool)
}
```

This is a private helper function, not a public API. It is applied in exactly three places:
1. `classify_tool` -- before matching tool names to categories
2. Knowledge served counter (lines 157-166) -- before checking for `context_search`/`context_lookup`/`context_get`
3. Knowledge stored counter (lines 168-171) -- before checking for `context_store`

It is NOT applied in `extract_file_path` (Claude-native tools are never MCP-prefixed; see SCOPE Background Research).

### C2: Tool Classification Extension (`session_metrics.rs`)

**Responsibility:** Add `curate` category to `classify_tool` for `context_correct`, `context_deprecate`, `context_quarantine`.

**Interface change to `classify_tool`:**
```rust
fn classify_tool(tool: &str) -> &'static str {
    let normalized = normalize_tool_name(tool);
    match normalized {
        "Read" | "Glob" | "Grep" => "read",
        "Edit" | "Write" => "write",
        "Bash" => "execute",
        "context_search" | "context_lookup" | "context_get" => "search",
        "context_store" => "store",
        "context_correct" | "context_deprecate" | "context_quarantine" => "curate",
        "SubagentStart" => "spawn",
        _ => "other",
    }
}
```

### C3: Knowledge Curated Counter (`session_metrics.rs`)

**Responsibility:** Count `context_correct`, `context_deprecate`, `context_quarantine` PreToolUse events as `knowledge_curated`.

Added alongside `knowledge_served` and `knowledge_stored` in `build_session_summary`. Uses `normalize_tool_name` for prefix handling.

### C4: Type Renames (`types.rs`, `lib.rs`)

**Responsibility:** Rename fields and types with serde backward compatibility.

Changes:

| Old | New | Serde annotation |
|-----|-----|-----------------|
| `SessionSummary.knowledge_in` | `SessionSummary.knowledge_served` | `#[serde(alias = "knowledge_in")]` |
| `SessionSummary.knowledge_out` | `SessionSummary.knowledge_stored` | `#[serde(alias = "knowledge_out")]` |
| (new field) | `SessionSummary.knowledge_curated` | `#[serde(default)]` |
| `KnowledgeReuse` (type) | `FeatureKnowledgeReuse` (type) | N/A (type rename, not serde) |
| `KnowledgeReuse.tier1_reuse_count` | `FeatureKnowledgeReuse.delivery_count` | `#[serde(alias = "tier1_reuse_count")]` |
| (new field) | `FeatureKnowledgeReuse.cross_session_count` | `#[serde(default)]` |
| `RetrospectiveReport.knowledge_reuse` | `RetrospectiveReport.feature_knowledge_reuse` | `#[serde(alias = "knowledge_reuse")]` |

Re-export in `lib.rs`: `KnowledgeReuse` renamed to `FeatureKnowledgeReuse`.

### C5: Knowledge Reuse Semantics Revision (`knowledge_reuse.rs`)

**Responsibility:** Change primary count from "entries in 2+ sessions" to "all distinct entries delivered". Keep cross-session as a sub-metric.

**Interface change:**
```rust
pub fn compute_knowledge_reuse<F>(
    query_log_records: &[QueryLogRecord],
    injection_log_records: &[InjectionLogRecord],
    active_category_counts: &HashMap<String, u64>,
    entry_category_lookup: F,
) -> FeatureKnowledgeReuse
```

Return type changes from `KnowledgeReuse` to `FeatureKnowledgeReuse`. The function signature (parameters) is unchanged.

**Semantic change:** Step 5 currently filters to entries in 2+ sessions. The revised logic:
- `delivery_count`: ALL distinct entry IDs across all sessions (union of query_log + injection_log)
- `cross_session_count`: entries appearing in 2+ distinct sessions (the old primary metric, now a sub-metric)
- `by_category`: counts ALL delivered entries by category (not just cross-session)
- `category_gaps`: categories with active entries but zero delivery (unchanged semantics)

### C6: Data Flow Debugging (`tools.rs`)

**Responsibility:** Add `tracing::debug!` at data flow boundaries in `compute_knowledge_reuse_for_sessions` to make future debugging possible.

Log points:
1. After loading session_id_list: log count
2. After loading query_logs: log record count
3. After loading injection_logs: log record count
4. After computing result: log delivery_count and cross_session_count

No behavioral change. The function continues to use `tracing::warn` on error (existing behavior).

### C7: Re-export Update (`lib.rs`)

**Responsibility:** Update the re-export path when `KnowledgeReuse` is renamed to `FeatureKnowledgeReuse`.

Change in `crates/unimatrix-observe/src/lib.rs`:
```rust
// Before
pub use types::KnowledgeReuse;
// After
pub use types::FeatureKnowledgeReuse;
```

All crate-external consumers (unimatrix-server) must update their imports.

## Component Interactions

```
                    tools.rs (orchestration)
                   /          |            \
                  /           |             \
    session_metrics.rs   knowledge_reuse.rs   Store
    (C1,C2,C3)          (C5)                  (data)
         |                    |                 |
         v                    v                 |
    types.rs (C4)        types.rs (C4)    query_log.rs
    SessionSummary       FeatureKnowledgeReuse  injection_log.rs
         \                   /
          \                 /
           v               v
        RetrospectiveReport (C4)
              |
              v
         MCP JSON response
```

Data flow for a `context_retrospective` call:
1. `tools.rs` loads session records from Store
2. `tools.rs` calls `compute_session_summaries` (observe) with observation records
3. `session_metrics.rs` normalizes tool names (C1), classifies tools (C2), counts knowledge flow (C3)
4. `tools.rs` calls `compute_knowledge_reuse_for_sessions` which loads query_log + injection_log from Store
5. `knowledge_reuse.rs` computes delivery_count and cross_session_count (C5)
6. `tools.rs` assembles RetrospectiveReport with all fields (C4)
7. Report serialized to JSON for MCP response

## Technology Decisions

| Decision | ADR | Rationale |
|----------|-----|-----------|
| normalize_tool_name as private fn in session_metrics | ADR-001 | Normalization is a session_metrics concern, not a shared utility |
| Rust-only tests for col-020b; infra-001 as follow-up | ADR-002 | Keeps scope bounded; computation paths are testable with synthetic data |
| Serde alias for read-old-with-new only (unidirectional) | ADR-003 | Matches the actual compat requirement; bidirectional not needed |
| FeatureKnowledgeReuse stays in unimatrix-server | ADR-004 | Upholds col-020 ADR-001; no architectural reason to move |
| Time-boxed #193 investigation with scope boundary | ADR-005 | Prevents unbounded scope if root cause is in Store layer |

## Integration Points

### Store -> knowledge_reuse.rs (existing, unchanged)
- `Store::scan_query_log_by_sessions(&[&str]) -> Result<Vec<QueryLogRecord>>`
- `Store::scan_injection_log_by_sessions(&[&str]) -> Result<Vec<InjectionLogRecord>>`
- `Store::count_active_entries_by_category() -> Result<HashMap<String, u64>>`
- `Store::get(u64) -> Result<EntryRecord>` (for category lookup)

### session_metrics.rs -> types.rs (modified)
- `SessionSummary` struct gains `knowledge_curated: u64` field
- `knowledge_in` renamed to `knowledge_served`
- `knowledge_out` renamed to `knowledge_stored`

### knowledge_reuse.rs -> types.rs (modified)
- Return type changes from `KnowledgeReuse` to `FeatureKnowledgeReuse`
- `FeatureKnowledgeReuse` gains `cross_session_count: u64` field
- `tier1_reuse_count` renamed to `delivery_count`

### tools.rs -> RetrospectiveReport (modified)
- `report.knowledge_reuse` renamed to `report.feature_knowledge_reuse`
- Type changes from `Option<KnowledgeReuse>` to `Option<FeatureKnowledgeReuse>`

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `normalize_tool_name` | `fn(&str) -> &str` | `session_metrics.rs` (private) |
| `classify_tool` | `fn(&str) -> &'static str` | `session_metrics.rs` (private) |
| `SessionSummary.knowledge_served` | `u64` | `types.rs` |
| `SessionSummary.knowledge_stored` | `u64` | `types.rs` |
| `SessionSummary.knowledge_curated` | `u64` | `types.rs` (new) |
| `FeatureKnowledgeReuse` | struct (renamed from `KnowledgeReuse`) | `types.rs` |
| `FeatureKnowledgeReuse.delivery_count` | `u64` (renamed from `tier1_reuse_count`) | `types.rs` |
| `FeatureKnowledgeReuse.cross_session_count` | `u64` (new) | `types.rs` |
| `RetrospectiveReport.feature_knowledge_reuse` | `Option<FeatureKnowledgeReuse>` (renamed) | `types.rs` |
| `compute_knowledge_reuse` | `fn(&[QueryLogRecord], &[InjectionLogRecord], &HashMap<String, u64>, F) -> FeatureKnowledgeReuse` | `knowledge_reuse.rs` |
| `compute_knowledge_reuse_for_sessions` | `async fn(&Arc<Store>, &[SessionRecord]) -> Result<FeatureKnowledgeReuse>` | `tools.rs` (private) |

## Testing Architecture

### Rust Unit Tests (in-scope for col-020b)

**session_metrics.rs tests:**
- Update existing tests to use renamed fields (`knowledge_served`, `knowledge_stored`)
- Add tests with `mcp__unimatrix__` prefixed tool names for `classify_tool`, knowledge flow counters, and `tool_distribution`
- Add test for `knowledge_curated` counter
- Add test for `curate` category in `tool_distribution`

**knowledge_reuse.rs tests:**
- Update existing tests to use `FeatureKnowledgeReuse` type and renamed fields
- Add test: single-session data produces `delivery_count > 0` (regression for the "2+ sessions" filter)
- Add test: `cross_session_count` correctly counts entries in 2+ sessions while `delivery_count` counts all

**types.rs tests:**
- Add serde backward compat test: old JSON with `knowledge_in`/`knowledge_out` deserializes into new type
- Add serde backward compat test: old JSON with `knowledge_reuse`/`tier1_reuse_count` deserializes into new type
- Add serde test: new field `knowledge_curated` defaults to 0 when absent from JSON

### Integration Tests (deferred -- see ADR-002)

The infra-001 harness already has `context_retrospective` client support and a `_seed_observation_sql` helper for injecting test data. A follow-up could add tests that seed MCP-prefixed tool names into observations and verify the retrospective report shows non-zero `knowledge_served`/`knowledge_stored`. This validates the full stack (Store -> observe -> server -> MCP response) but is a separate effort from the bug fixes.

## Open Questions

1. **#193 root cause**: The `compute_knowledge_reuse_for_sessions` data flow loads query_log and injection_log by session IDs extracted from session records. If those session records have no corresponding query_log/injection_log rows (because no MCP searches happened during those sessions, or because session_id formats differ between the sessions table and the query_log table), the computation will correctly return zero. The debug tracing added in C6 will make this diagnosable. If the root cause turns out to be in Store SQL queries or session_id format mismatch, that fix should be a separate issue per ADR-005.

2. **`tool_distribution` extensibility**: Adding the `curate` category means downstream consumers parsing the `tool_distribution` HashMap may see a new key. The HashMap is already extensible by design (String keys, not an enum), so this is informational rather than a breaking change (SR-07 acknowledged).
