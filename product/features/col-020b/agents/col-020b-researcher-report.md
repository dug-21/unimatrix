# col-020b Researcher Report

## Agent ID: col-020b-researcher

## Task
Explore problem space for col-020b (Retrospective Knowledge Metric Fixes) and produce SCOPE.md.

## Key Findings

1. **MCP prefix mismatch is pervasive in session_metrics.rs**: `classify_tool`, `knowledge_in` counter, and `knowledge_out` counter all match on bare names. The `extract_file_path` function is NOT affected because it only handles Claude-native tools (Read, Edit, Write, Glob, Grep) which are never MCP-prefixed.

2. **knowledge_reuse.rs computation logic is correct for its specification**: The algorithm correctly groups entries by session and filters to 2+ sessions. The bug is either upstream (data not loaded from Store) or definitional (2+ sessions is too restrictive for the "knowledge delivery" metric the human wants).

3. **No integration tests exist anywhere**: Both unimatrix-observe and unimatrix-server have zero `tests/` directories. All testing is inline `#[cfg(test)] mod tests`. The session_metrics tests use bare tool names, which matched the buggy implementation.

4. **Serde backward compat is well-established**: The codebase uses `serde(alias)` for renames and `serde(default)` for new fields consistently across col-020 and earlier features.

5. **The data flow in tools.rs has three spawn_blocking calls**: Each has `??` error propagation. Silent failures would produce a `tracing::warn` and set `knowledge_reuse: None` on the report. Need tracing at data flow boundaries to diagnose.

## Scope Boundaries

- **In scope**: Tool name normalization, field renames with serde compat, classify_tool curate category, FeatureKnowledgeReuse semantics revision, data flow debugging, tests
- **Out of scope**: ObservationSource trait changes, detection rules, aggregate metrics, briefing counting, schema changes

## Risks

- The #193 root cause may be in the Store layer (SQL queries) rather than the computation module, requiring changes in unimatrix-store
- Renaming struct fields across two crates requires coordinated changes to re-exports in lib.rs

## Output
- `/workspaces/unimatrix/product/features/col-020b/SCOPE.md`
