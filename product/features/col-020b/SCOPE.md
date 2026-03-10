# col-020b: Retrospective Knowledge Metric Fixes

## Problem Statement

Two bugs shipped with col-020 (Multi-Session Retrospective) that render the knowledge metrics in `RetrospectiveReport` non-functional:

1. **MCP tool name mismatch (#192)**: `session_metrics.rs` matches tool names using bare names (`context_search`, `context_store`) but observation records store the full MCP-qualified prefix (`mcp__unimatrix__context_search`). This causes `knowledge_in` and `knowledge_out` to always be 0, and MCP tool calls to fall through to the `other` category in `tool_distribution`.

2. **FeatureKnowledgeReuse returns 0 (#193)**: The `compute_knowledge_reuse` function requires entries to appear in 2+ distinct sessions, but the data flow from `compute_knowledge_reuse_for_sessions` in `tools.rs` may not be loading data correctly. Additionally, the "2+ sessions" filter is too restrictive -- any entry delivered to agents for this feature counts as knowledge delivery, not just cross-session reuse.

Both bugs were confirmed by running `context_retrospective(feature_cycle: "col-020")` on live data: knowledge counts were 0 despite real MCP tool usage, and knowledge_reuse was empty despite 17 distinct entries being delivered across sessions.

The human also wants integration test coverage for the retrospective pipeline's computation path, which col-020 shipped without.

## Goals

1. Fix MCP tool name normalization so `classify_tool`, knowledge flow counters, and `extract_file_path` handle both bare and `mcp__unimatrix__`-prefixed tool names
2. Rename `knowledge_in`/`knowledge_out` to `knowledge_served`/`knowledge_stored` and add `knowledge_curated` (counts `context_correct` + `context_deprecate` + `context_quarantine`)
3. Add `curate` category to `classify_tool` for knowledge curation tools
4. Rename `KnowledgeReuse` to `FeatureKnowledgeReuse` with revised fields: `delivery_count` (all entries), `cross_session_count` (2+ sessions), `by_category`, `category_gaps`
5. Rename `knowledge_reuse` field on `RetrospectiveReport` to `feature_knowledge_reuse`
6. Fix the computation in `compute_knowledge_reuse` to count all delivered entries (primary) with cross-session as a sub-metric
7. Root-cause the data flow bug in `compute_knowledge_reuse_for_sessions` (whether query_log/injection_log loading is failing silently)
8. Add integration tests that exercise the full session_metrics and knowledge_reuse computation paths with realistic MCP-prefixed tool names

## Non-Goals

- **No changes to the observation recording pipeline.** Tool names are stored as-is from Claude's hook system; normalization happens at computation time.
- **No changes to detection rules or hotspot logic.** The 21 existing rules are not affected.
- **No changes to UniversalMetrics or PhaseMetrics.** Aggregate metrics remain untouched.
- **No changes to the ObservationSource trait.** The data loading interface stays stable.
- **No changes to context_reload_pct or rework_session_count.** Those metrics work correctly.
- **No query_log or injection_log schema changes.** The SQL queries may need debugging but not schema modifications.
- **No Tier 2/3 knowledge reuse.** The revised semantics count all delivery, but do not add semantic similarity or feedback-based measurement.

## Background Research

### Existing Code State

**`crates/unimatrix-observe/src/session_metrics.rs`**: Pure computation module. `classify_tool` (line 187-197) matches on bare names only. `knowledge_in` filter (line 157-166) and `knowledge_out` filter (line 168-171) both use bare `context_*` names. `extract_file_path` (line 200-206) only handles `Read`, `Edit`, `Write`, `Glob`, `Grep` -- these are Claude-native tools that do NOT get prefixed, so `extract_file_path` is actually correct as-is (no MCP prefix on those tools).

**`crates/unimatrix-observe/src/types.rs`**: `SessionSummary` struct (line 170-190) has `knowledge_in: u64` and `knowledge_out: u64`. `KnowledgeReuse` struct (line 193-201) has `tier1_reuse_count`, `by_category`, `category_gaps`. `RetrospectiveReport` (line 213-256) references both.

**`crates/unimatrix-server/src/mcp/knowledge_reuse.rs`**: Pure computation. The logic itself (grouping by session, filtering to 2+ sessions) is correct for its original specification. The bug is either upstream (data not loaded) or definitional (2+ sessions is too restrictive).

**`crates/unimatrix-server/src/mcp/tools.rs`**: `compute_knowledge_reuse_for_sessions` (line 1619-1667) loads query_log and injection_log via `spawn_blocking`. The data flow has three `spawn_blocking` calls and two `??` unwraps -- a failure in any would propagate as Err, which the caller handles with `tracing::warn` (line 1289). This means silent failure produces a warning log but the report still returns with `knowledge_reuse: None`.

### Test Infrastructure Gap

- **No integration tests exist** for `unimatrix-observe` or `unimatrix-server`. All tests are unit tests in `#[cfg(test)] mod tests` blocks within source files.
- The `session_metrics.rs` unit tests (line 237-712) use bare tool names (`context_search`, `context_store`), which is why the bug was not caught -- the tests matched the (incorrect) implementation.
- The `knowledge_reuse.rs` unit tests (line 146-676) test the computation logic in isolation with synthetic data. They verify the algorithm is correct for its inputs, but the data flow from Store to computation is untested.

### Key Finding: extract_file_path Does Not Need MCP Normalization

Claude-native tools (`Read`, `Edit`, `Write`, `Glob`, `Grep`, `Bash`) are NOT prefixed with `mcp__unimatrix__`. Only Unimatrix MCP tools get the prefix. Therefore `extract_file_path` works correctly as-is -- it only handles Claude-native tools. The normalization is needed only in `classify_tool` and the knowledge flow counters.

### Serde Backward Compatibility Pattern

The codebase uses `#[serde(default)]` and `#[serde(skip_serializing_if)]` extensively for additive changes (see `RetrospectiveReport` lines 228-255). For renames, `#[serde(alias = "old_name")]` is the established pattern.

## Proposed Approach

### A. Tool Name Normalization (session_metrics.rs)

Add a `normalize_tool_name` function that strips `mcp__unimatrix__` prefix:
```
fn normalize_tool_name(tool: &str) -> &str {
    tool.strip_prefix("mcp__unimatrix__").unwrap_or(tool)
}
```
Apply in `classify_tool` and the knowledge flow counter filters. Do NOT apply in `extract_file_path` (not needed -- see Background Research).

### B. Field Renames (types.rs)

- `SessionSummary`: `knowledge_in` -> `knowledge_served`, `knowledge_out` -> `knowledge_stored`, add `knowledge_curated: u64`
- `KnowledgeReuse` -> `FeatureKnowledgeReuse`: `tier1_reuse_count` -> `delivery_count`, add `cross_session_count: u64`
- `RetrospectiveReport`: `knowledge_reuse` -> `feature_knowledge_reuse`
- All renames get `#[serde(alias = "old_name")]` for deserialization backward compat
- New fields get `#[serde(default)]` for backward compat

### C. classify_tool Extension

Add `curate` category mapping `context_correct`, `context_deprecate`, `context_quarantine`.

### D. Knowledge Reuse Semantics Revision (knowledge_reuse.rs)

Change primary count from "entries in 2+ sessions" to "all entries delivered (any session)". Keep cross-session count as a sub-metric. Update `by_category` to count all deliveries, not just cross-session.

### E. Data Flow Debugging (tools.rs)

Add `tracing::debug!` at data flow boundaries in `compute_knowledge_reuse_for_sessions` to make future debugging possible. The root cause needs to be isolated during implementation -- it could be session_id mismatch, empty query results, or feature_cycle attribution gaps.

### F. Integration Tests

Add integration tests in `session_metrics.rs` and `knowledge_reuse.rs` that exercise the full computation path with MCP-prefixed tool names. These are still unit-style (no database needed for session_metrics, synthetic data for knowledge_reuse) but use realistic inputs that match production data format.

## Acceptance Criteria

- AC-01: `normalize_tool_name("mcp__unimatrix__context_search")` returns `"context_search"`; `normalize_tool_name("Read")` returns `"Read"` (passthrough for non-MCP tools)
- AC-02: `classify_tool` maps `mcp__unimatrix__context_search` to `"search"`, `mcp__unimatrix__context_store` to `"store"`, `mcp__unimatrix__context_correct` to `"curate"`, and bare names continue to work
- AC-03: `knowledge_served` counts PreToolUse events for `context_search`, `context_lookup`, `context_get` (both bare and MCP-prefixed)
- AC-04: `knowledge_stored` counts PreToolUse events for `context_store` (both bare and MCP-prefixed)
- AC-05: `knowledge_curated` counts PreToolUse events for `context_correct`, `context_deprecate`, `context_quarantine` (both bare and MCP-prefixed)
- AC-06: `SessionSummary` fields renamed from `knowledge_in`/`knowledge_out` to `knowledge_served`/`knowledge_stored` with serde aliases for backward compat
- AC-07: `FeatureKnowledgeReuse.delivery_count` reflects ALL unique entries delivered to agents for the feature (any session)
- AC-08: `FeatureKnowledgeReuse.cross_session_count` reflects entries appearing in 2+ distinct sessions
- AC-09: `FeatureKnowledgeReuse.by_category` reflects delivery counts (all entries, not just cross-session)
- AC-10: `FeatureKnowledgeReuse.category_gaps` identifies categories with active entries but zero delivery
- AC-11: `RetrospectiveReport.feature_knowledge_reuse` replaces `knowledge_reuse` with serde alias for backward compat
- AC-12: `knowledge_curated` has `#[serde(default)]` for backward compat with pre-col-020b data
- AC-13: Existing unit tests updated to reflect renamed fields and new categories
- AC-14: New tests cover MCP-prefixed tool names in `classify_tool`, knowledge flow counters, and tool distribution
- AC-15: New tests cover `FeatureKnowledgeReuse` computation with delivery_count > 0 for single-session data (regression for the "2+ sessions" filter bug)
- AC-16: Debug tracing added at data flow boundaries in `compute_knowledge_reuse_for_sessions`

## Constraints

- **Serde backward compatibility required.** Serialized retrospective reports from col-020 must deserialize with the new types. All renames must use `serde(alias)`, all new fields must use `serde(default)`.
- **No integration test infrastructure exists.** Both crates have zero `tests/` directories. Integration tests that require a running Store database cannot be added to `unimatrix-observe` (no Store dependency). Tests for `knowledge_reuse.rs` in `unimatrix-server` are unit tests with synthetic data because the module has a pure function signature that takes slices, not Store references.
- **`extract_file_path` must NOT be changed.** Claude-native tools are never MCP-prefixed. Applying normalization there would be a no-op but would add confusion.
- **The `compute_knowledge_reuse_for_sessions` data flow bug in tools.rs may require Store-level debugging.** The function uses three `spawn_blocking` calls with `??` unwrap chains. If the bug is in session_id format mismatch or query_log/injection_log SQL, the fix is in the store crate, not the computation module.
- **Re-export path in `unimatrix-observe/src/lib.rs`** must be updated when `KnowledgeReuse` is renamed to `FeatureKnowledgeReuse`.

## Resolved Questions

1. **`context_briefing` not counted in `knowledge_served`.** Briefing injection is opaque — consistent with col-020 non-goals. Just because the protocol ran it doesn't mean it was useful.

2. **`curate` is a separate `tool_distribution` category.** Curation (correct/deprecate/quarantine) is semantically distinct from creation (store).

3. **#193 root cause**: Issue #193 performed significant research identifying possible failure points in the data flow. These findings should be brought forward to the architect/implementer for validation — not assumed correct. The implementation should validate whether the bug is in Store queries, session_id format mismatch, or computation logic.

4. **Integration testing strategy**: The architect should evaluate whether the col-020/col-020b retrospective computation path should be tested through infra-001 style integration testing (Python, MCP JSON-RPC over stdio per `product/test/infra-001/USAGE-PROTOCOL.md`) in addition to Rust unit tests. The infra-001 harness exercises the compiled binary through the real MCP interface — this may be the appropriate level for validating end-to-end retrospective metrics.

## Tracking

- GitHub Issue #192: MCP tool name mismatch + rename knowledge_in/out
- GitHub Issue #193: KnowledgeReuse computation broken + rename + revise semantics
- GitHub Issue #194: https://github.com/dug-21/unimatrix/issues/194
