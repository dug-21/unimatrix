# col-020b Specification: Retrospective Knowledge Metric Fixes

## Objective

Fix two bugs shipped with col-020 that render knowledge metrics non-functional in `RetrospectiveReport`: (1) MCP tool name mismatch causing `knowledge_in`, `knowledge_out`, and tool distribution to miss all Unimatrix MCP tool calls, and (2) `FeatureKnowledgeReuse` computation returning empty results due to an overly restrictive 2+ session filter. Additionally, rename fields for semantic clarity, add a `knowledge_curated` counter, and add test coverage for the computation paths with realistic MCP-prefixed tool names.

## Functional Requirements

### FR-01: Tool Name Normalization

FR-01.1: A `normalize_tool_name(tool: &str) -> &str` function SHALL strip the `mcp__unimatrix__` prefix from tool names, returning the bare name. For tool names without this prefix, return the input unchanged.

FR-01.2: `classify_tool` SHALL call `normalize_tool_name` before matching, so both `"context_search"` and `"mcp__unimatrix__context_search"` resolve to the same category.

FR-01.3: The knowledge flow counter filters (lines 157-171 in `session_metrics.rs`) SHALL call `normalize_tool_name` before matching tool names.

FR-01.4: `extract_file_path` SHALL NOT be changed. Claude-native tools (`Read`, `Edit`, `Write`, `Glob`, `Grep`) are never MCP-prefixed; normalization there would be a no-op.

### FR-02: classify_tool Category Mapping

FR-02.1: `classify_tool` SHALL map tool names to categories as follows (after normalization):

| Bare tool name | Category |
|---|---|
| `Read` | `read` |
| `Glob` | `read` |
| `Grep` | `read` |
| `Edit` | `write` |
| `Write` | `write` |
| `Bash` | `execute` |
| `context_search` | `search` |
| `context_lookup` | `search` |
| `context_get` | `search` |
| `context_store` | `store` |
| `context_correct` | `curate` |
| `context_deprecate` | `curate` |
| `context_quarantine` | `curate` |
| `SubagentStart` | `spawn` |
| (anything else) | `other` |

FR-02.2: MCP-prefixed variants (e.g., `mcp__unimatrix__context_correct`) SHALL resolve to the same category as their bare counterparts via FR-01.

FR-02.3: `context_briefing`, `context_status`, `context_enroll`, `context_retrospective` SHALL remain in `other`. These are administrative/diagnostic tools, not knowledge flow tools.

### FR-03: SessionSummary Field Renames and Additions

FR-03.1: `SessionSummary.knowledge_in` SHALL be renamed to `knowledge_served`.

FR-03.2: `SessionSummary.knowledge_out` SHALL be renamed to `knowledge_stored`.

FR-03.3: A new field `knowledge_curated: u64` SHALL be added to `SessionSummary`.

FR-03.4: `knowledge_served` SHALL count PreToolUse events where the normalized tool name is `context_search`, `context_lookup`, or `context_get`.

FR-03.5: `knowledge_stored` SHALL count PreToolUse events where the normalized tool name is `context_store`.

FR-03.6: `knowledge_curated` SHALL count PreToolUse events where the normalized tool name is `context_correct`, `context_deprecate`, or `context_quarantine`.

### FR-04: KnowledgeReuse Rename and Semantic Revision

FR-04.1: The `KnowledgeReuse` struct SHALL be renamed to `FeatureKnowledgeReuse`.

FR-04.2: `FeatureKnowledgeReuse.tier1_reuse_count` SHALL be renamed to `delivery_count`. This field counts ALL unique entry IDs delivered to agents for the feature across any number of sessions (including single-session entries).

FR-04.3: A new field `cross_session_count: u64` SHALL be added. This field counts unique entry IDs appearing in 2+ distinct sessions (the previous `tier1_reuse_count` semantics).

FR-04.4: `FeatureKnowledgeReuse.by_category` SHALL reflect delivery counts (all delivered entries), not just cross-session entries.

FR-04.5: `FeatureKnowledgeReuse.category_gaps` SHALL identify categories with active entries but zero delivery (not zero cross-session reuse).

### FR-05: RetrospectiveReport Field Rename

FR-05.1: `RetrospectiveReport.knowledge_reuse` SHALL be renamed to `feature_knowledge_reuse`. The type changes from `Option<KnowledgeReuse>` to `Option<FeatureKnowledgeReuse>`.

### FR-06: compute_knowledge_reuse Semantic Revision

FR-06.1: The primary count (`delivery_count`) SHALL include ALL unique entry IDs found in query_log `result_entry_ids` or injection_log for the feature's sessions, regardless of how many sessions reference them.

FR-06.2: The `cross_session_count` SHALL retain the existing 2+ session filter logic.

FR-06.3: `by_category` SHALL be computed from all delivered entries (not just cross-session entries).

FR-06.4: `category_gaps` SHALL compare against all delivered categories (not just cross-session categories).

FR-06.5: The function signature SHALL return `FeatureKnowledgeReuse` instead of `KnowledgeReuse`.

### FR-07: Data Flow Debugging

FR-07.1: `compute_knowledge_reuse_for_sessions` in `tools.rs` SHALL add `tracing::debug!` logging at each data flow boundary: after query_log load (count), after injection_log load (count), after active_cats load (count), and before returning the result (delivery_count, cross_session_count).

FR-07.2: The root cause of #193 (data flow producing empty results) SHALL be investigated during implementation. If the bug is in Store-layer SQL (session_id format mismatch, query construction), the fix may extend to `unimatrix-store`. If the Store fix is non-trivial, it SHALL be split into a separate issue; the field renames and normalization fixes ship independently.

### FR-08: Re-export Updates

FR-08.1: `unimatrix-observe/src/lib.rs` SHALL update the re-export from `KnowledgeReuse` to `FeatureKnowledgeReuse`.

FR-08.2: All import sites across the workspace that reference `KnowledgeReuse` SHALL be updated to `FeatureKnowledgeReuse`.

## Non-Functional Requirements

NFR-01: `normalize_tool_name` SHALL be O(1) -- a single prefix check with no allocations.

NFR-02: No new crate dependencies SHALL be introduced.

NFR-03: All existing tests SHALL continue to pass after updates (with field name adjustments).

NFR-04: The `tool_distribution` HashMap in `SessionSummary` is extensible by design -- consumers MUST NOT assume a fixed set of category keys. The new `curate` category is an additive change to a dynamic map, not a schema break.

## Acceptance Criteria

### AC-01: normalize_tool_name passthrough
`normalize_tool_name("mcp__unimatrix__context_search")` returns `"context_search"`; `normalize_tool_name("Read")` returns `"Read"`.
**Verification**: Unit test with both prefixed and bare tool names.

### AC-02: classify_tool MCP prefix handling
`classify_tool("mcp__unimatrix__context_search")` returns `"search"`, `classify_tool("mcp__unimatrix__context_store")` returns `"store"`, `classify_tool("mcp__unimatrix__context_correct")` returns `"curate"`. Bare names continue to work identically.
**Verification**: Unit test covering all entries in the FR-02.1 table for both bare and MCP-prefixed variants.

### AC-03: knowledge_served counts correctly
`knowledge_served` counts PreToolUse events for `context_search`, `context_lookup`, `context_get` (both bare and `mcp__unimatrix__`-prefixed).
**Verification**: Unit test with mixed bare and prefixed tool names; sum equals total count.

### AC-04: knowledge_stored counts correctly
`knowledge_stored` counts PreToolUse events for `context_store` (both bare and MCP-prefixed).
**Verification**: Unit test with prefixed `context_store` events; count is non-zero.

### AC-05: knowledge_curated counts correctly
`knowledge_curated` counts PreToolUse events for `context_correct`, `context_deprecate`, `context_quarantine` (both bare and MCP-prefixed).
**Verification**: Unit test with all three curation tools.

### AC-06: SessionSummary field renames with serde backward compat
`SessionSummary` fields renamed from `knowledge_in`/`knowledge_out` to `knowledge_served`/`knowledge_stored`. JSON containing `"knowledge_in": 5` deserializes into `knowledge_served == 5`.
**Verification**: Unit test deserializing col-020 era JSON with old field names into new struct.

### AC-07: FeatureKnowledgeReuse.delivery_count reflects all entries
`delivery_count` includes entries appearing in only 1 session.
**Verification**: Unit test with entries in a single session; `delivery_count > 0`, `cross_session_count == 0`.

### AC-08: FeatureKnowledgeReuse.cross_session_count is a sub-metric
`cross_session_count` reflects entries appearing in 2+ distinct sessions. `cross_session_count <= delivery_count` always holds.
**Verification**: Unit test with mix of single-session and multi-session entries; cross_session_count < delivery_count.

### AC-09: by_category reflects all deliveries
`by_category` counts all delivered entries, not just cross-session.
**Verification**: Unit test with single-session entries; `by_category` is non-empty.

### AC-10: category_gaps based on delivery not cross-session
`category_gaps` identifies categories with active entries but zero delivery.
**Verification**: Unit test with active categories where some have delivery and some do not.

### AC-11: RetrospectiveReport field rename with serde backward compat
`RetrospectiveReport.feature_knowledge_reuse` replaces `knowledge_reuse`. JSON containing `"knowledge_reuse": {...}` deserializes into `feature_knowledge_reuse`.
**Verification**: Unit test deserializing col-020 era JSON into new struct.

### AC-12: knowledge_curated backward compat
`knowledge_curated` has `#[serde(default)]` so pre-col-020b JSON (lacking this field) deserializes with `knowledge_curated == 0`.
**Verification**: Unit test deserializing SessionSummary JSON without `knowledge_curated` field.

### AC-13: Existing tests updated
All existing unit tests in `session_metrics.rs`, `types.rs`, and `knowledge_reuse.rs` updated to use new field names and pass.
**Verification**: `cargo test` in both `unimatrix-observe` and `unimatrix-server` crates.

### AC-14: New tests for MCP-prefixed tool names
New unit tests exercise `classify_tool`, knowledge flow counters, and tool distribution with `mcp__unimatrix__`-prefixed tool names.
**Verification**: Tests using `"mcp__unimatrix__context_search"`, `"mcp__unimatrix__context_store"`, `"mcp__unimatrix__context_correct"` as inputs.

### AC-15: Single-session delivery regression test
New test exercises `FeatureKnowledgeReuse` computation where entries appear in only 1 session. `delivery_count > 0` and `cross_session_count == 0`.
**Verification**: Unit test in `knowledge_reuse.rs` with single-session data.

### AC-16: Debug tracing at data flow boundaries
`compute_knowledge_reuse_for_sessions` in `tools.rs` logs query_log count, injection_log count, active_cats count, and result summary at `debug` level.
**Verification**: Code review; tracing instrumentation present after each `spawn_blocking` call.

## Backward Compatibility Requirements

### Serde Alias Strategy

All field renames use `#[serde(alias = "old_name")]` for deserialization backward compatibility. New fields use `#[serde(default)]`.

**Directionality constraint (SR-01 response):** `serde(alias)` only covers deserialization (reading old JSON with new types). Serialization always uses the new field name. This means:

- **Old consumer reading new output**: A consumer compiled with col-020 types reading a col-020b JSON response will silently drop the renamed fields (e.g., `feature_knowledge_reuse` is unknown to old `RetrospectiveReport` which expects `knowledge_reuse`). The old consumer will see `knowledge_reuse: None` due to `#[serde(default)]` on the old type.
- **This is acceptable** because:
  1. `RetrospectiveReport` is an MCP tool output type consumed ephemerally by LLM agents. It is never persisted to disk or database for later retrieval by a different binary version.
  2. MCP consumers are always the same running server binary that produced the report. There is no cross-version consumer scenario in production.
  3. The `skip_serializing_if = "Option::is_none"` pattern means old consumers already handle absent fields gracefully.
- **If cross-version consumption becomes a requirement**, the mitigation is `#[serde(rename = "old_name")]` (serialize as old name) instead of just `alias`. This is a future concern, not a col-020b requirement.

### Specific Serde Annotations

| Struct | Field | Annotation |
|---|---|---|
| `SessionSummary` | `knowledge_served` | `#[serde(alias = "knowledge_in")]` |
| `SessionSummary` | `knowledge_stored` | `#[serde(alias = "knowledge_out")]` |
| `SessionSummary` | `knowledge_curated` | `#[serde(default)]` |
| `FeatureKnowledgeReuse` | `delivery_count` | `#[serde(alias = "tier1_reuse_count")]` |
| `FeatureKnowledgeReuse` | `cross_session_count` | `#[serde(default)]` |
| `RetrospectiveReport` | `feature_knowledge_reuse` | `#[serde(default, skip_serializing_if = "Option::is_none", alias = "knowledge_reuse")]` |

### SR-02 Resolution: No existing serde(rename) on affected fields

Verified: `SessionSummary.knowledge_in`, `SessionSummary.knowledge_out`, `KnowledgeReuse.tier1_reuse_count`, and `RetrospectiveReport.knowledge_reuse` have no `#[serde(rename)]` annotations. Adding `alias` is safe.

## Domain Models

### Key Terms

- **normalize_tool_name**: Function that strips the `mcp__unimatrix__` prefix from MCP-qualified tool names. Claude Code's hook system records Unimatrix tools with this prefix; computation normalizes to bare names.
- **knowledge_served**: Count of knowledge retrieval tool calls (`context_search`, `context_lookup`, `context_get`) in a session. Replaces `knowledge_in`.
- **knowledge_stored**: Count of knowledge creation tool calls (`context_store`) in a session. Replaces `knowledge_out`.
- **knowledge_curated**: Count of knowledge curation tool calls (`context_correct`, `context_deprecate`, `context_quarantine`) in a session. New field.
- **delivery_count**: Total unique knowledge entries delivered to agents for a feature across all sessions. An entry appearing in query_log results or injection_log for any session counts as "delivered." Replaces `tier1_reuse_count`.
- **cross_session_count**: Subset of delivered entries that appear in 2+ distinct sessions. A sub-metric of delivery_count.
- **FeatureKnowledgeReuse**: Renamed from `KnowledgeReuse`. Measures knowledge utilization for a feature across its sessions.
- **tool_distribution**: HashMap of category string to count. Categories are extensible; `curate` is a new category added by col-020b.

### Entity Relationships

```
RetrospectiveReport
  |-- session_summaries: Vec<SessionSummary>
  |     |-- knowledge_served (retrieval count)
  |     |-- knowledge_stored (creation count)
  |     |-- knowledge_curated (curation count, new)
  |     |-- tool_distribution (now includes "curate" category)
  |-- feature_knowledge_reuse: FeatureKnowledgeReuse (renamed from knowledge_reuse)
        |-- delivery_count (all entries, renamed from tier1_reuse_count)
        |-- cross_session_count (2+ sessions, new)
        |-- by_category (all deliveries, not just cross-session)
        |-- category_gaps (zero-delivery categories, not zero-reuse)
```

## User Workflows

### Agent consuming a retrospective

1. Agent calls `context_retrospective(feature_cycle: "col-020b")`.
2. Server computes session summaries with correct MCP tool normalization.
3. `knowledge_served`, `knowledge_stored`, `knowledge_curated` reflect actual tool usage (non-zero when MCP tools were used).
4. `feature_knowledge_reuse.delivery_count` reflects all entries delivered, even for single-session features.
5. `feature_knowledge_reuse.cross_session_count` shows multi-session reuse as a sub-metric.

### Developer debugging knowledge flow

1. Enable `RUST_LOG=unimatrix_server=debug`.
2. Call `context_retrospective`.
3. Debug logs show query_log count, injection_log count, active category counts, and delivery/cross-session results at each data flow boundary.

## Constraints

C-01: Changes are scoped to 2 crates: `unimatrix-observe` and `unimatrix-server`. If #193 root cause is in `unimatrix-store`, that fix is a separate issue.

C-02: `extract_file_path` MUST NOT be modified. It handles only Claude-native tools which are never MCP-prefixed.

C-03: No changes to `UniversalMetrics`, `PhaseMetrics`, `MetricVector`, detection rules, or hotspot logic.

C-04: No changes to `ObservationSource` trait.

C-05: No changes to observation recording pipeline. Tool names are stored as-is; normalization is computation-time only.

C-06: `context_briefing` is NOT counted in `knowledge_served`. Briefing injection is opaque and its effectiveness is not measurable at the tool-call level.

C-07: Time-box #193 root cause investigation (SR-03 mitigation). If the bug is in Store SQL, split into a separate issue. The normalization fixes, field renames, and new tests are independently valuable and ship regardless.

## Dependencies

- `unimatrix-observe` (modified): `session_metrics.rs`, `types.rs`, `lib.rs`
- `unimatrix-server` (modified): `mcp/knowledge_reuse.rs`, `mcp/tools.rs`
- `unimatrix-store` (read-only dependency): `QueryLogRecord`, `InjectionLogRecord`, `Store::scan_query_log_by_sessions`, `Store::scan_injection_log_by_sessions`, `Store::count_active_entries_by_category`
- `unimatrix-core` (unchanged): `HookType`, `ObservationRecord`
- `serde` / `serde_json` (existing): serde alias and default attributes

## NOT In Scope

- **No changes to observation recording.** Tool names remain stored with MCP prefix as-is from Claude Code hooks.
- **No changes to detection rules.** The 21 existing rules are unaffected.
- **No Tier 2/3 knowledge reuse.** Semantic similarity and feedback-based measurement are future work.
- **No changes to `context_reload_pct` or `rework_session_count`.** These metrics work correctly.
- **No query_log or injection_log schema changes.** The SQL tables are not modified.
- **No infra-001 integration tests in this feature.** Testing is Rust unit tests with realistic inputs. Infra-001 coverage is a follow-up (SR-04 mitigation).
- **No new cross-crate test infrastructure.** Tests use existing patterns (synthetic data, pure function signatures).
- **No `serde(rename)` for bidirectional compat.** Only `serde(alias)` for deserialization. Serialization uses new names. See Backward Compatibility Requirements for rationale.

## Test Requirements

### Updated Existing Tests

- `session_metrics.rs::test_classify_tool_all_categories` -- update to include `curate` category entries.
- `session_metrics.rs::test_session_summaries_knowledge_in_out` -- rename assertions to `knowledge_served`/`knowledge_stored`.
- `types.rs::test_session_summary_serde_roundtrip` -- update field names.
- `types.rs::test_knowledge_reuse_serde_roundtrip` -- update to `FeatureKnowledgeReuse` with new fields.
- `types.rs::test_retrospective_report_roundtrip_with_new_fields` -- update field names.
- `knowledge_reuse.rs` -- all tests update `tier1_reuse_count` references to `delivery_count`, add `cross_session_count` assertions.

### New Tests Required

**session_metrics.rs:**
- `test_normalize_tool_name_mcp_prefix` -- verifies stripping of `mcp__unimatrix__` prefix.
- `test_normalize_tool_name_passthrough` -- verifies non-MCP tools pass through unchanged.
- `test_classify_tool_mcp_prefixed` -- verifies all MCP-prefixed tool names resolve to correct categories.
- `test_session_summaries_mcp_prefixed_knowledge_flow` -- computes session summaries with `mcp__unimatrix__context_search`, `mcp__unimatrix__context_store`, `mcp__unimatrix__context_correct` events; verifies `knowledge_served`, `knowledge_stored`, `knowledge_curated` are non-zero.
- `test_session_summaries_curate_in_tool_distribution` -- verifies `tool_distribution` contains `"curate"` key for `context_correct`/`context_deprecate`/`context_quarantine` events.

**types.rs:**
- `test_session_summary_deserialize_pre_col020b` -- deserializes JSON with `knowledge_in`/`knowledge_out` (no `knowledge_curated`); verifies alias mapping and default.
- `test_feature_knowledge_reuse_deserialize_from_old` -- deserializes JSON with `tier1_reuse_count` (no `cross_session_count`); verifies alias mapping and default.
- `test_retrospective_report_deserialize_old_knowledge_reuse_field` -- deserializes JSON with `knowledge_reuse` field name; verifies it populates `feature_knowledge_reuse`.

**knowledge_reuse.rs:**
- `test_knowledge_reuse_single_session_delivery` -- entries in only 1 session; `delivery_count > 0`, `cross_session_count == 0`.
- `test_knowledge_reuse_delivery_vs_cross_session` -- mix of single-session and multi-session entries; `delivery_count > cross_session_count`.
- `test_knowledge_reuse_by_category_includes_single_session` -- `by_category` populated even when entries appear in only 1 session.

## Open Questions

OQ-01: **#193 root cause.** Is the data flow bug in `compute_knowledge_reuse_for_sessions` caused by (a) session_id format mismatch between `SessionRecord.session_id` and `query_log.session_id`, (b) empty query_log/injection_log due to SQL query issues, or (c) feature_cycle attribution gaps? The architect/implementer should investigate with debug tracing before implementing the semantic revision. If the bug is in Store SQL, it becomes a separate issue per C-07.

OQ-02: **SR-05: Are there any persisted RetrospectiveReport instances?** The specification assumes RetrospectiveReport is ephemeral (MCP tool output only). If reports are cached in SQLite or written to disk, the serde alias strategy needs review. Current evidence suggests no persistence -- `is_cached` refers to in-memory memoization within a server session, not disk persistence.
