# col-020: Multi-Session Retrospective -- Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/col-020/SCOPE.md |
| Scope Risk Assessment | product/features/col-020/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-020/architecture/ARCHITECTURE.md |
| Specification | product/features/col-020/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/col-020/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-020/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/col-020/architecture/ADR-001-knowledge-reuse-server-side.md |
| ADR-002 | product/features/col-020/architecture/ADR-002-idempotent-counter-updates.md |
| ADR-003 | product/features/col-020/architecture/ADR-003-attribution-metadata.md |
| ADR-004 | product/features/col-020/architecture/ADR-004-file-path-extraction-mapping.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| session_metrics (C1) | pseudocode/session_metrics.md | test-plan/session_metrics.md |
| types (C2) | pseudocode/types.md | test-plan/types.md |
| knowledge_reuse (C3) | pseudocode/knowledge_reuse.md | test-plan/knowledge_reuse.md |
| store_api (C4) | pseudocode/store_api.md | test-plan/store_api.md |
| report_builder (C5) | pseudocode/report_builder.md | test-plan/report_builder.md |
| handler_integration (C6) | pseudocode/handler_integration.md | test-plan/handler_integration.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Add per-session decomposition, cross-session knowledge reuse measurement, rework session counting, context reload rate, and attribution coverage to the retrospective pipeline. This transforms the retrospective from a flat topic aggregate into a multi-session analysis that answers: what did each session do, is knowledge flowing across sessions, and how much rework occurred. The feature also connects retrospective computation to topic_deliveries aggregate counters via idempotent absolute-set updates.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Knowledge reuse computation location | Server-side (unimatrix-server handler), not unimatrix-observe. Only multi-table Store joins go server-side; pure observation computation stays in observe. | SR-08, SCOPE Option B | architecture/ADR-001-knowledge-reuse-server-side.md |
| Counter update idempotency | Absolute-set via `set_topic_delivery_counters()`, not additive increment. Repeated runs produce same values. | SR-09 | architecture/ADR-002-idempotent-counter-updates.md |
| Attribution metadata on report | `AttributionMetadata { attributed_session_count, total_session_count }` added to report for consumer trust assessment. | SR-07 | architecture/ADR-003-attribution-metadata.md |
| File path extraction strategy | Explicit tool-to-field match: Read/Edit/Write -> `file_path`, Glob/Grep -> `path`. Unknown tools return None silently. | SR-04 | architecture/ADR-004-file-path-extraction-mapping.md |
| Rework outcome case sensitivity | Case-insensitive substring match on `result:rework` and `result:failed`. Human override of spec FR-03.1 (which said case-sensitive). Architecture line 189 is authoritative. | Vision variance #1 (human-resolved) | N/A |
| query_log JSON parsing risk | Not a real risk. `QueryLogRecord::new()` serializes from typed `&[u64]` slices via serde_json. Defensive `unwrap_or_default` on read is sufficient. | Vision variance #2 (human-resolved) | N/A |
| build_report() signature | Unchanged. New fields assigned via post-build mutation on returned report (same pattern as narratives/recommendations). | Architecture C5 | N/A |
| Helpful signal in Tier 1 | Excluded from v1. Helpful signals lack per-session attribution. Tier 1 counts search-return and injection-log signals only. | Architecture open question #2 | N/A |

## Files to Create/Modify

### New Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-observe/src/session_metrics.rs` | Session summary computation, context reload rate, tool classification, file path extraction (C1) |

### Modified Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-observe/src/types.rs` | Add `SessionSummary`, `KnowledgeReuse`, `AttributionMetadata` structs; extend `RetrospectiveReport` with 5 new Option fields (C2) |
| `crates/unimatrix-observe/src/lib.rs` | Export new `session_metrics` module (C1) |
| `crates/unimatrix-store/src/query_log.rs` | Add `scan_query_log_by_sessions()` batch method (C4) |
| `crates/unimatrix-store/src/injection_log.rs` | Add `scan_injection_log_by_sessions()` batch method (C4) |
| `crates/unimatrix-store/src/read.rs` | Add `count_active_entries_by_category()` (C4) |
| `crates/unimatrix-store/src/topic_deliveries.rs` | Add `set_topic_delivery_counters()` absolute setter (C4) |
| `crates/unimatrix-server/src/mcp/tools.rs` | Extend `context_retrospective` handler with 6 new steps: session summaries, reload rate, knowledge reuse, rework count, attribution metadata, counter update (C3, C6) |

## Data Structures

### SessionSummary (unimatrix-observe/src/types.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub started_at: u64,
    pub duration_secs: u64,
    pub tool_distribution: HashMap<String, u64>,
    pub top_file_zones: Vec<(String, u64)>,
    pub agents_spawned: Vec<String>,
    pub knowledge_in: u64,
    pub knowledge_out: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}
```

### KnowledgeReuse (unimatrix-observe/src/types.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeReuse {
    pub tier1_reuse_count: u64,
    pub by_category: HashMap<String, u64>,
    pub category_gaps: Vec<String>,
}
```

### AttributionMetadata (unimatrix-observe/src/types.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionMetadata {
    pub attributed_session_count: usize,
    pub total_session_count: usize,
}
```

### RetrospectiveReport Extensions

```rust
// New optional fields on RetrospectiveReport:
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

## Function Signatures

### C1: Session Metrics (unimatrix-observe/src/session_metrics.rs)

```rust
pub fn compute_session_summaries(records: &[ObservationRecord]) -> Vec<SessionSummary>
pub fn compute_context_reload_pct(summaries: &[SessionSummary], records: &[ObservationRecord]) -> f64

// Internal helpers:
fn extract_file_path(tool: &str, input: &serde_json::Value) -> Option<String>
fn classify_tool(tool: &str) -> &'static str
fn extract_directory_zone(path: &str) -> String
```

### C4: Store API Extensions

```rust
// unimatrix-store query_log.rs
pub fn scan_query_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<QueryLogRecord>>

// unimatrix-store injection_log.rs
pub fn scan_injection_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<InjectionLogRecord>>

// unimatrix-store read.rs
pub fn count_active_entries_by_category(&self) -> Result<HashMap<String, u64>>

// unimatrix-store topic_deliveries.rs
pub fn set_topic_delivery_counters(
    &self,
    topic: &str,
    total_sessions: i64,
    total_tool_calls: i64,
    total_duration_secs: i64,
) -> Result<()>
```

## Constraints

- **col-017 dependency**: Sessions must be attributed to topics via `sessions.feature_cycle`. col-017 is Wave 1 and must land first.
- **nxs-010 dependency**: `topic_deliveries` and `query_log` tables must exist (schema v11). nxs-010 has landed.
- **ObservationSource trait stability**: No changes to the ObservationSource trait. New queries are direct Store/SqlObservationSource methods.
- **Server-side knowledge reuse**: Knowledge reuse is computed in unimatrix-server (ADR-001). Only multi-table Store joins go server-side.
- **Backward compatibility**: All new RetrospectiveReport fields are `Option` with `serde(default, skip_serializing_if)`. Pre-col-020 JSON round-trips without breakage.
- **Best-effort computation**: Failures in new steps produce None fields and log warnings. Existing pipeline output is never lost.
- **PreToolUse filtering**: Tool distribution counts PreToolUse events only (FR-01.2). The handler loads all observation records; C1 must filter by hook type.
- **Deduplication**: Tier 1 reuse counts distinct entry IDs, not retrieval events. An entry appearing in both query_log and injection_log for different sessions counts as 1.
- **Session ordering**: By `started_at` ascending; lexicographic `session_id` tiebreaker for identical timestamps.
- **Directory zone extraction**: First 3 path components from workspace root (e.g., `crates/unimatrix-store/src`).
- **Batch query chunking**: `IN (...)` clauses chunked to batches of 50 session IDs for large topics.

## Dependencies

### Internal Crates (no new external crates)

- `unimatrix-observe` -- new types, new session_metrics module, report builder extension
- `unimatrix-server` -- knowledge reuse computation, handler integration, Store access
- `unimatrix-store` -- new batch query methods, absolute counter setter
- `unimatrix-core` -- ObservationRecord (existing, unchanged)

### Existing APIs Consumed (unchanged)

- `SqlObservationSource::load_feature_observations(feature_cycle)`
- `SqlObservationSource::discover_sessions_for_feature(feature_cycle)`
- `Store::scan_sessions_by_feature(feature_cycle)`
- `Store::scan_query_log_by_session(session_id)` -- existing single-session variant
- `Store::get_topic_delivery(topic)`
- `Store::upsert_topic_delivery()` -- for creating record before setting counters

## NOT in Scope

- Session-type classification (design vs delivery vs bugfix)
- Session efficiency trend (`session_efficiency_trend` -- dropped)
- Tier 2 or Tier 3 knowledge reuse (only Tier 1)
- Changes to existing aggregate metrics (UniversalMetrics, PhaseMetrics)
- Changes to detection rules or hotspot finding logic (21 rules untouched)
- Changes to ObservationSource trait
- Per-session hotspot findings (hotspots remain topic-level)
- Retrospective output format changes beyond new additive fields
- New MetricVector columns for cross-session metrics
- Helpful-signal-based reuse (lacks per-session attribution)

## Alignment Status

**Overall: PASS with one human-resolved variance.**

The vision guardian found one substantive variance: rework outcome case sensitivity was contradictory between architecture (case-insensitive) and specification FR-03.1 (case-sensitive). Human resolved this to **case-insensitive substring match** -- matching semantic intent rather than brittle format assumptions. The architecture document is authoritative; spec FR-03.1 is overridden.

A second flagged item (JSON parsing risk for `query_log result_entry_ids`) was dismissed by the human as not a real risk, since both write and read paths are controlled.

Additional alignment notes from the report:
- `AttributionCoverage` was added beyond SCOPE.md to address SR-07 (High severity risk). Justified addition.
- Grep was added to the file path extraction mapping beyond SCOPE.md (architecture ADR-004). Improves data completeness.
- `session_efficiency_trend` was dropped from the vision roadmap description. Justified scope reduction documented in SCOPE.md non-goals.
- Minor log-level discrepancy (warn vs debug for JSON parse failures) -- functionally irrelevant.
- Risk strategy coverage summary has a typo (says 6 high-priority risks but lists 7).

## Tool Category Classification Reference

| Tool Name | Category |
|-----------|----------|
| Read, Glob, Grep | `read` |
| Edit, Write | `write` |
| Bash | `execute` |
| context_search, context_lookup, context_get | `search` |
| context_store | `store` |
| SubagentStart | `spawn` |
| Everything else | `other` |

## File Path Extraction Mapping Reference

| Tool | JSON Field |
|------|-----------|
| Read | `.file_path` |
| Edit | `.file_path` |
| Write | `.file_path` |
| Glob | `.path` |
| Grep | `.path` |
| Unknown | Skip silently |

## Rework Outcome Patterns Reference

Case-insensitive substring match on session `outcome` field:
- `result:rework`
- `result:failed`

Sessions with NULL or empty outcome are not counted.

## Handler Integration Sequence

After existing pipeline (hotspots, metrics, baselines, entries, narratives, recommendations):

1. `compute_session_summaries(&attributed)` -> `Vec<SessionSummary>` (C1, unimatrix-observe)
2. `compute_context_reload_pct(&summaries, &attributed)` -> `f64` (C1, unimatrix-observe)
3. Load `query_log` + `injection_log` for topic's session IDs via batch Store methods (C4)
4. Compute knowledge reuse inline: cross-table join in Rust, deduplicate entry IDs (C3, unimatrix-server)
5. Count rework sessions from `SessionRecord` outcomes (C6, unimatrix-server)
6. Compute `AttributionMetadata` from session discovery results (C6, unimatrix-server)
7. `set_topic_delivery_counters()` with absolute values from MetricVector (C4, unimatrix-store)
8. Assign all new fields to report before serialization

Each step is wrapped in best-effort error handling: `Ok(value)` -> `report.field = Some(value)`, `Err(e)` -> `tracing::warn!` + `report.field = None`.

## Error Propagation

All new computation steps are fallible but non-fatal. The existing pipeline output (hotspots, MetricVector, baselines, narratives, recommendations, lesson-learned) is preserved regardless of new step failures. New steps use:

```rust
match new_computation() {
    Ok(value) => report.field = Some(value),
    Err(e) => {
        tracing::warn!("col-020: {step} failed: {e}");
        // report.field remains None
    }
}
```
