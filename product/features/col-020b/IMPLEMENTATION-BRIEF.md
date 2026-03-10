# col-020b Implementation Brief: Retrospective Knowledge Metric Fixes

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/col-020b/SCOPE.md |
| Scope Risk Assessment | product/features/col-020b/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-020b/architecture/ARCHITECTURE.md |
| Specification | product/features/col-020b/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/col-020b/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-020b/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| tool-name-normalizer | pseudocode/tool-name-normalizer.md | test-plan/tool-name-normalizer.md |
| tool-classification | pseudocode/tool-classification.md | test-plan/tool-classification.md |
| knowledge-curated-counter | pseudocode/knowledge-curated-counter.md | test-plan/knowledge-curated-counter.md |
| type-renames | pseudocode/type-renames.md | test-plan/type-renames.md |
| knowledge-reuse-semantics | pseudocode/knowledge-reuse-semantics.md | test-plan/knowledge-reuse-semantics.md |
| data-flow-debugging | pseudocode/data-flow-debugging.md | test-plan/data-flow-debugging.md |
| re-export-update | pseudocode/re-export-update.md | test-plan/re-export-update.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Fix two bugs shipped with col-020 (Multi-Session Retrospective) that render knowledge metrics in `RetrospectiveReport` non-functional: MCP tool name mismatch (#192) causing knowledge counters to always be 0, and `FeatureKnowledgeReuse` computation (#193) returning empty results due to an overly restrictive 2+ session filter. Additionally, rename fields for semantic clarity, add a `knowledge_curated` counter and `curate` tool category, revise knowledge reuse semantics to count all delivery (not just cross-session), and add test coverage with realistic MCP-prefixed tool names.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| normalize_tool_name placement | Private fn in session_metrics.rs; single consumer file, no cross-crate need | ADR-001 | architecture/ADR-001-normalize-tool-name-placement.md |
| Integration testing scope | Rust-only unit tests for col-020b; infra-001 integration tests deferred to follow-up | ADR-002 | architecture/ADR-002-integration-testing-scope.md |
| Serde backward compat strategy | Unidirectional via serde(alias) for deserialization only; reports are ephemeral, no cross-version consumers | ADR-003 | architecture/ADR-003-serde-backward-compat-strategy.md |
| FeatureKnowledgeReuse location | Stays in unimatrix-server/src/mcp/knowledge_reuse.rs; upholds col-020 ADR-001 | ADR-004 | architecture/ADR-004-knowledge-reuse-stays-server-side.md |
| #193 investigation boundary | Time-boxed investigation; if root cause is Store SQL, split to separate issue | ADR-005 | architecture/ADR-005-issue-193-investigation-boundary.md |

## Files to Create/Modify

### Modified Files

| File | Change Summary |
|------|---------------|
| `crates/unimatrix-observe/src/session_metrics.rs` | Add `normalize_tool_name`; update `classify_tool` with normalization + `curate` category; add `knowledge_curated` counter; apply normalization to knowledge_served/stored counters |
| `crates/unimatrix-observe/src/types.rs` | Rename `knowledge_in`->`knowledge_served`, `knowledge_out`->`knowledge_stored` on SessionSummary; add `knowledge_curated`; rename `KnowledgeReuse`->`FeatureKnowledgeReuse`; rename `tier1_reuse_count`->`delivery_count`; add `cross_session_count`; rename `knowledge_reuse`->`feature_knowledge_reuse` on RetrospectiveReport; add serde aliases and defaults |
| `crates/unimatrix-observe/src/lib.rs` | Update re-export from `KnowledgeReuse` to `FeatureKnowledgeReuse` |
| `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` | Change return type to `FeatureKnowledgeReuse`; revise semantics: `delivery_count` = all entries, `cross_session_count` = 2+ sessions; update `by_category` and `category_gaps` to use all deliveries |
| `crates/unimatrix-server/src/mcp/tools.rs` | Update `compute_knowledge_reuse_for_sessions` return type; add `tracing::debug!` at data flow boundaries; update field name references |

### No New Files

All changes are modifications to existing source files. Tests are added within existing `#[cfg(test)] mod tests` blocks.

## Data Structures

### SessionSummary (modified)

```rust
pub struct SessionSummary {
    // ... existing fields unchanged ...
    #[serde(alias = "knowledge_in")]
    pub knowledge_served: u64,
    #[serde(alias = "knowledge_out")]
    pub knowledge_stored: u64,
    #[serde(default)]
    pub knowledge_curated: u64,
    // tool_distribution: HashMap<String, u64> -- gains "curate" key
}
```

### FeatureKnowledgeReuse (renamed from KnowledgeReuse)

```rust
pub struct FeatureKnowledgeReuse {
    #[serde(alias = "tier1_reuse_count")]
    pub delivery_count: u64,
    #[serde(default)]
    pub cross_session_count: u64,
    pub by_category: HashMap<String, u64>,
    pub category_gaps: Vec<String>,
}
```

### RetrospectiveReport (modified field)

```rust
pub struct RetrospectiveReport {
    // ... existing fields unchanged ...
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "knowledge_reuse")]
    pub feature_knowledge_reuse: Option<FeatureKnowledgeReuse>,
}
```

## Function Signatures

### New: normalize_tool_name (private, session_metrics.rs)

```rust
fn normalize_tool_name(tool: &str) -> &str
```

Strips `mcp__unimatrix__` prefix. Returns bare name or input unchanged.

### Modified: classify_tool (private, session_metrics.rs)

```rust
fn classify_tool(tool: &str) -> &'static str
```

Now calls `normalize_tool_name` before matching. Adds `curate` category for `context_correct`, `context_deprecate`, `context_quarantine`.

### Modified: compute_knowledge_reuse (public, knowledge_reuse.rs)

```rust
pub fn compute_knowledge_reuse<F>(
    query_log_records: &[QueryLogRecord],
    injection_log_records: &[InjectionLogRecord],
    active_category_counts: &HashMap<String, u64>,
    entry_category_lookup: F,
) -> FeatureKnowledgeReuse
```

Return type changes from `KnowledgeReuse` to `FeatureKnowledgeReuse`. `delivery_count` = all unique entries; `cross_session_count` = entries in 2+ sessions.

### Modified: compute_knowledge_reuse_for_sessions (private async, tools.rs)

```rust
async fn compute_knowledge_reuse_for_sessions(
    store: &Arc<Store>,
    sessions: &[SessionRecord],
) -> Result<FeatureKnowledgeReuse>
```

Return type changes. Adds `tracing::debug!` at data flow boundaries.

## Constraints

- **Serde backward compatibility required**: All renames use `serde(alias)`, all new fields use `serde(default)`. Unidirectional only (read-old-with-new). ADR-003 rationale: reports are ephemeral MCP output, never persisted.
- **2-crate scope**: Changes confined to `unimatrix-observe` and `unimatrix-server`. If #193 root cause is in `unimatrix-store`, that fix is a separate issue (ADR-005).
- **extract_file_path must NOT be changed**: Claude-native tools are never MCP-prefixed; normalization there is unnecessary.
- **Rust-only tests**: No infra-001 integration tests in this feature (ADR-002). Unit tests use MCP-prefixed inputs for realism.
- **No new crate dependencies**: All changes use existing serde, tracing, and std library features.
- **#193 investigation time-boxed**: Add debug tracing, validate with live data. If Store SQL is the root cause, split to separate issue (ADR-005).

## Dependencies

### Crate Dependencies (no new additions)

- `serde` / `serde_json` -- serde alias and default attributes (existing)
- `tracing` -- debug-level logging at data flow boundaries (existing)

### Internal Crate Dependencies

- `unimatrix-observe` (modified) -- session_metrics.rs, types.rs, lib.rs
- `unimatrix-server` (modified) -- mcp/knowledge_reuse.rs, mcp/tools.rs
- `unimatrix-store` (read-only) -- QueryLogRecord, InjectionLogRecord, Store query methods
- `unimatrix-core` (unchanged) -- HookType, ObservationRecord

## NOT in Scope

- No changes to observation recording pipeline (tool names stored as-is from hooks)
- No changes to detection rules or hotspot logic (21 existing rules unaffected)
- No changes to UniversalMetrics, PhaseMetrics, or MetricVector
- No changes to ObservationSource trait
- No changes to context_reload_pct or rework_session_count
- No query_log or injection_log schema changes
- No infra-001 integration tests (deferred per ADR-002)
- No new cross-crate test infrastructure
- No Tier 2/3 knowledge reuse (semantic similarity, feedback-based measurement)
- No serde(rename) for bidirectional compat (unidirectional only per ADR-003)
- No changes to context_briefing counting (explicitly excluded from knowledge_served)

## Alignment Status

**Clean alignment. No variances.**

The vision guardian confirmed all 6 checks pass: vision alignment, milestone fit, scope gaps, scope additions, architecture consistency, and risk completeness. All 16 acceptance criteria from SCOPE.md are addressed across source documents. All 8 scope risks (SR-01 through SR-08) are traced to architecture risks with ADR-backed resolutions.

Key vision alignment points:
- Fixes knowledge flow counters that support "auditable knowledge lifecycle"
- Revised FeatureKnowledgeReuse semantics provide more accurate picture of knowledge utilization
- Addition of `knowledge_curated` tracks curation activity as first-class metric, supporting "correctable and auditable" principle

## Open Questions

1. **#193 root cause (OQ-01)**: Is the data flow bug caused by session_id format mismatch, empty query_log/injection_log, or computation logic? Implementation should add debug tracing first, validate with live data, and split Store-layer fixes to a separate issue if needed (ADR-005).

2. **Persisted RetrospectiveReport instances (OQ-02)**: The specification assumes reports are ephemeral (MCP tool output only). Current evidence supports this -- `is_cached` is in-memory memoization, not disk persistence. If reports are discovered to be persisted, the serde alias strategy needs review.
