# Agent Report: col-020b-agent-1-architect

## Task
Design architecture for col-020b (Retrospective Knowledge Metric Fixes).

## Status: COMPLETE

## Files Produced

### Architecture
- `/workspaces/unimatrix/product/features/col-020b/architecture/ARCHITECTURE.md`

### ADRs
- `/workspaces/unimatrix/product/features/col-020b/architecture/ADR-001-normalize-tool-name-placement.md`
- `/workspaces/unimatrix/product/features/col-020b/architecture/ADR-002-integration-testing-scope.md`
- `/workspaces/unimatrix/product/features/col-020b/architecture/ADR-003-serde-backward-compat-strategy.md`
- `/workspaces/unimatrix/product/features/col-020b/architecture/ADR-004-knowledge-reuse-stays-server-side.md`
- `/workspaces/unimatrix/product/features/col-020b/architecture/ADR-005-issue-193-investigation-boundary.md`

## Key Decisions

1. **ADR-001**: `normalize_tool_name` is a private function in `session_metrics.rs`. No shared utility needed -- only one file consumes it.

2. **ADR-002**: Rust-only tests for col-020b. infra-001 integration tests deferred to follow-up. The bugs are in pure computation functions testable with synthetic data. Mixing test types risks scope expansion (SR-04).

3. **ADR-003**: Serde alias for unidirectional backward compat only. `RetrospectiveReport` is not persisted, no cross-version consumer exists. First use of `serde(alias)` in the codebase.

4. **ADR-004**: `FeatureKnowledgeReuse` computation stays in `unimatrix-server`. Upholds col-020 ADR-001 (Unimatrix #864). No architectural reason to move.

5. **ADR-005**: Time-boxed #193 investigation. If root cause is Store-layer SQL, file separate issue. Semantic revision (all-delivery vs 2+ sessions) is the primary fix.

## Unimatrix Storage

All 5 ADRs attempted storage via `context_store` but agent `col-020b-agent-1-architect` lacks Write capability. ADR files exist on disk. A Write-capable agent (human or system) must store them in Unimatrix to complete the knowledge stewardship requirement.

## Prior Decisions Consulted

- col-020 ADR-001 (Unimatrix #864): Knowledge reuse computed server-side
- col-020 ADR-004 (Unimatrix #867): Explicit tool-to-field mapping for file path extraction
- No existing `serde(alias)` or `serde(rename)` on affected fields (confirmed via codebase grep)
- infra-001 harness already has `context_retrospective` client method and `_seed_observation_sql` helper

## Open Questions

1. **#193 root cause**: May be empty query_log/injection_log (no MCP searches in attributed sessions), session_id format mismatch, or Store SQL bug. Debug tracing (C6) makes it diagnosable. If Store-layer, needs separate issue.

2. **tool_distribution extensibility**: New `curate` category is a non-breaking addition to a HashMap<String, u64>. Informational only.
