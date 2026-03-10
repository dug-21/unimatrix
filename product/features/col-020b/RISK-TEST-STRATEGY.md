# Risk-Based Test Strategy: col-020b

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | normalize_tool_name misses edge-case prefixes (double prefix, partial prefix, case variation) | High | Med | High |
| R-02 | Serde alias annotations silently drop fields on deserialization due to ordering or interaction with existing attributes | High | Low | Med |
| R-03 | Serde default for new fields (knowledge_curated, cross_session_count) produces incorrect zero instead of signaling absence | Med | Med | Med |
| R-04 | FeatureKnowledgeReuse delivery_count regression — semantic change from 2+ sessions to all-delivery miscounts or double-counts entries | High | Med | High |
| R-05 | by_category and category_gaps computed against wrong entry set after semantic revision (cross-session instead of all-delivery) | Med | Med | Med |
| R-06 | #193 data flow returns empty slices silently — debug tracing added but root cause unresolved; semantic revision masks the bug | High | High | Critical |
| R-07 | Re-export rename (KnowledgeReuse to FeatureKnowledgeReuse) missed at an import site, causing compilation failure | Low | Low | Low |
| R-08 | Existing tests pass with bare names but never exercise MCP-prefixed tool names — regression protection gap | High | High | Critical |
| R-09 | classify_tool curate category omits a curation tool or includes a non-curation tool | Med | Low | Low |
| R-10 | knowledge_served/stored/curated counters apply normalization inconsistently (one counter normalizes, another does not) | High | Med | High |
| R-11 | tool_distribution HashMap gains curate key, breaking downstream consumers that assume fixed category set | Med | Low | Low |
| R-12 | compute_knowledge_reuse_for_sessions spawn_blocking chain silently swallows errors, returning None instead of propagating | Med | Med | Med |
| R-13 | Round-trip serialization produces JSON with new field names that old test fixtures or log parsers cannot read | Low | Med | Low |

## Risk-to-Scenario Mapping

### R-01: normalize_tool_name misses edge-case prefixes
**Severity**: High
**Likelihood**: Med
**Impact**: MCP tool calls continue to fall through to "other" category; knowledge counters remain 0 despite fix attempt.

**Test Scenarios**:
1. Input `"mcp__unimatrix__context_search"` returns `"context_search"` (standard case)
2. Input `"mcp__unimatrix__mcp__unimatrix__context_search"` (double prefix) returns `"mcp__unimatrix__context_search"` — only one layer stripped
3. Input `"mcp__unimatrix__"` (prefix only, no tool name) returns `""` (empty string)
4. Input `""` (empty string) returns `""` without panic
5. Input `"MCP__UNIMATRIX__context_search"` (case variation) returns unchanged — prefix is case-sensitive
6. Input `"mcp__other_server__context_search"` (different MCP server) returns unchanged — only unimatrix prefix stripped
7. Input `"context_search"` (bare name) returns `"context_search"` (passthrough)
8. Input `"Read"` (Claude-native tool) returns `"Read"` (passthrough)

**Coverage Requirement**: Unit tests for normalize_tool_name covering all 8 scenarios above.

### R-02: Serde alias silently drops fields
**Severity**: High
**Likelihood**: Low
**Impact**: Deserializing col-020 era JSON produces structs with zeroed fields instead of correct values. Retrospective reports from logs/fixtures become silently corrupted.

**Test Scenarios**:
1. Deserialize JSON with `"knowledge_in": 5` into SessionSummary; assert `knowledge_served == 5`
2. Deserialize JSON with `"knowledge_out": 3` into SessionSummary; assert `knowledge_stored == 3`
3. Deserialize JSON with `"tier1_reuse_count": 7` into FeatureKnowledgeReuse; assert `delivery_count == 7`
4. Deserialize JSON with `"knowledge_reuse": {...}` into RetrospectiveReport; assert `feature_knowledge_reuse` is populated
5. Round-trip: serialize new struct, deserialize back; assert all fields preserved
6. Deserialize JSON containing BOTH old and new field names (conflict); verify behavior is defined (serde uses the last occurrence)

**Coverage Requirement**: Serde backward compat tests in types.rs for each renamed field. Evidence: Unimatrix #646 documents serde(default) patterns; #371 documents migration compatibility isolation.

### R-03: Serde default produces incorrect zero
**Severity**: Med
**Likelihood**: Med
**Impact**: Pre-col-020b JSON missing `knowledge_curated` or `cross_session_count` deserializes with 0, which is correct for "no data" but indistinguishable from "data was present but count was zero." Consumers cannot tell if the field is absent vs zero.

**Test Scenarios**:
1. Deserialize SessionSummary JSON without `knowledge_curated` field; assert `knowledge_curated == 0`
2. Deserialize FeatureKnowledgeReuse JSON without `cross_session_count` field; assert `cross_session_count == 0`
3. Deserialize SessionSummary JSON WITH `knowledge_curated: 5`; assert value preserved (not overridden by default)

**Coverage Requirement**: Unit tests verifying default behavior for each new field.

### R-04: delivery_count semantic change miscounts
**Severity**: High
**Likelihood**: Med
**Impact**: delivery_count over-counts (double-counting entries across query_log and injection_log) or under-counts (still applying the 2+ sessions filter).

**Test Scenarios**:
1. Single-session data with 3 entries in query_log: `delivery_count == 3`, `cross_session_count == 0` (regression for old 2+ sessions bug)
2. Single-session data with entries in both query_log and injection_log for same entry IDs: deduplication produces correct count
3. Multi-session data: `delivery_count >= cross_session_count` invariant holds
4. Entry in query_log for s1, injection_log for s2: counted once in delivery_count, once in cross_session_count
5. Entry in query_log for s1, query_log for s2, injection_log for s2: counted once in delivery_count, once in cross_session_count

**Coverage Requirement**: Unit tests in knowledge_reuse.rs exercising single-session, multi-session, and cross-source scenarios. AC-07, AC-08.

### R-05: by_category and category_gaps use wrong entry set
**Severity**: Med
**Likelihood**: Med
**Impact**: by_category only shows cross-session entries (old behavior) instead of all delivered entries. category_gaps reports false gaps for categories with single-session delivery.

**Test Scenarios**:
1. Single-session entries with known categories: by_category is non-empty (not filtered out by 2+ sessions)
2. Active category with single-session delivery: NOT in category_gaps
3. Active category with zero delivery: IS in category_gaps
4. No active categories: category_gaps is empty

**Coverage Requirement**: Unit tests in knowledge_reuse.rs. AC-09, AC-10.

### R-06: #193 data flow returns empty slices silently
**Severity**: High
**Likelihood**: High
**Impact**: Even with correct semantics, delivery_count remains 0 because upstream data loading produces empty query_log/injection_log slices. The semantic revision masks this bug — it appears "fixed" (no longer filtered to 2+ sessions) but data never arrives.

**Test Scenarios**:
1. Verify debug tracing logs record counts after each spawn_blocking call (code review)
2. Verify compute_knowledge_reuse_for_sessions propagates errors from Store calls (not swallowing with `??`)
3. Verify session_id values passed to scan_query_log_by_sessions match the format stored in query_log table
4. Manual validation: run context_retrospective on a feature with known MCP usage, check debug logs for non-zero counts

**Coverage Requirement**: Code review for tracing instrumentation (AC-16). The end-to-end data flow is NOT covered by unit tests (ADR-002 accepts this gap). If debug logs show zero counts, a follow-up issue is required per ADR-005.

### R-07: Re-export rename missed at import site
**Severity**: Low
**Likelihood**: Low
**Impact**: Compilation failure — caught immediately by cargo build.

**Test Scenarios**:
1. `cargo build --workspace` succeeds after rename

**Coverage Requirement**: Compilation gate. No dedicated test needed.

### R-08: Existing tests never exercise MCP-prefixed tool names
**Severity**: High
**Likelihood**: High
**Impact**: The original bug (#192) was shipped because tests used bare names matching the (incorrect) implementation. Without MCP-prefixed test inputs, the normalization fix cannot be verified.

**Test Scenarios**:
1. classify_tool with `"mcp__unimatrix__context_search"` returns `"search"`
2. classify_tool with `"mcp__unimatrix__context_store"` returns `"store"`
3. classify_tool with `"mcp__unimatrix__context_correct"` returns `"curate"`
4. Full session summary computation with MCP-prefixed events: knowledge_served > 0, knowledge_stored > 0, knowledge_curated > 0
5. tool_distribution with MCP-prefixed events: contains "search", "store", "curate" keys with correct counts

**Coverage Requirement**: New unit tests in session_metrics.rs using MCP-prefixed inputs. AC-02, AC-03, AC-04, AC-05, AC-14.

### R-09: classify_tool curate category mapping error
**Severity**: Med
**Likelihood**: Low
**Impact**: A curation tool (context_correct, context_deprecate, context_quarantine) falls through to "other", or a non-curation tool is classified as "curate."

**Test Scenarios**:
1. Each of context_correct, context_deprecate, context_quarantine maps to "curate"
2. context_briefing, context_status, context_enroll, context_retrospective map to "other" (not "curate")

**Coverage Requirement**: Exhaustive classify_tool test covering all tool names in the FR-02.1 mapping table.

### R-10: Inconsistent normalization across counters
**Severity**: High
**Likelihood**: Med
**Impact**: One counter normalizes (e.g., knowledge_served) but another does not (e.g., knowledge_stored), causing partial fix — some metrics work, others still show 0.

**Test Scenarios**:
1. Session with MCP-prefixed context_search, context_store, and context_correct events: all three counters (knowledge_served, knowledge_stored, knowledge_curated) are non-zero
2. Session mixing bare and MCP-prefixed tool names: counters correctly sum both forms

**Coverage Requirement**: Integration-style unit test in session_metrics.rs with mixed tool name formats. AC-03, AC-04, AC-05.

### R-11: tool_distribution curate key breaks consumers
**Severity**: Med
**Likelihood**: Low
**Impact**: Downstream code parsing tool_distribution assumes fixed keys and panics or ignores the new "curate" category.

**Test Scenarios**:
1. Verify tool_distribution is HashMap<String, u64> (dynamic keys, not enum)
2. Verify curate category appears in tool_distribution when curation tools are used
3. Verify curate category is absent when no curation tools are used

**Coverage Requirement**: Unit test verifying curate key presence/absence. NFR-04 establishes this is additive.

### R-12: spawn_blocking error swallowing
**Severity**: Med
**Likelihood**: Med
**Impact**: A Store query failure in compute_knowledge_reuse_for_sessions is caught by `??` unwrap, propagated as Err, but the caller handles it with `tracing::warn` and returns `feature_knowledge_reuse: None`. The report appears complete but with missing data.

**Test Scenarios**:
1. Verify the caller logs a warning when compute_knowledge_reuse_for_sessions returns Err
2. Verify the report has `feature_knowledge_reuse: None` (not Some with zeroed fields) when data loading fails
3. Code review: confirm no `unwrap()` on spawn_blocking JoinHandle that could panic

**Coverage Requirement**: Code review for error handling path. Unit test for the None vs Some(zeroed) distinction if feasible.

### R-13: New field names in serialized output
**Severity**: Low
**Likelihood**: Med
**Impact**: Serialized JSON uses `knowledge_served` instead of `knowledge_in`. Log parsers, grep scripts, or test fixtures searching for old field names will miss results.

**Test Scenarios**:
1. Serialize new SessionSummary; verify JSON contains `knowledge_served` (not `knowledge_in`)
2. Serialize new RetrospectiveReport; verify JSON contains `feature_knowledge_reuse` (not `knowledge_reuse`)

**Coverage Requirement**: Serialization assertion in round-trip tests.

## Integration Risks

1. **session_metrics.rs <-> types.rs**: SessionSummary field renames must be synchronized. If session_metrics.rs writes to `.knowledge_served` but the struct field is still `knowledge_in`, compilation fails (caught). If the serde alias is on the wrong field, deserialization silently zeroes the value (not caught without tests).

2. **knowledge_reuse.rs <-> types.rs**: The return type changes from KnowledgeReuse to FeatureKnowledgeReuse. The function must populate both `delivery_count` and `cross_session_count` — omitting cross_session_count would compile (defaults to 0) but silently drop the sub-metric.

3. **tools.rs <-> knowledge_reuse.rs**: tools.rs calls `compute_knowledge_reuse` and receives FeatureKnowledgeReuse. If tools.rs still constructs `KnowledgeReuse` manually anywhere (e.g., error fallback path), the type mismatch is a compile error. But if it constructs `FeatureKnowledgeReuse` with missing fields, defaults hide the omission.

4. **lib.rs re-export <-> unimatrix-server imports**: The re-export rename must match the import in unimatrix-server. A stale import causes compilation failure (caught immediately).

## Edge Cases

1. **Empty tool name**: `normalize_tool_name("")` and `classify_tool("")` must not panic. Empty string should classify as "other."
2. **Tool name is exactly the prefix**: `normalize_tool_name("mcp__unimatrix__")` returns `""`. `classify_tool("mcp__unimatrix__")` returns "other" (empty after normalization falls through to default).
3. **Double prefix**: `"mcp__unimatrix__mcp__unimatrix__context_search"` strips only one layer, producing `"mcp__unimatrix__context_search"` which then classifies as... `"context_search"` after a second normalization? No — classify_tool calls normalize_tool_name once. The double-prefixed name would produce `"mcp__unimatrix__context_search"` after one strip, which would then NOT match any bare name in the match arms, falling through to "other." This is correct behavior but should be tested.
4. **Zero entries delivered**: FeatureKnowledgeReuse with delivery_count=0, cross_session_count=0, empty by_category — valid state when no MCP searches happened.
5. **Duplicate entry IDs across query_log and injection_log**: Same entry ID in both sources for same session should count as 1 delivery, not 2.
6. **Malformed result_entry_ids JSON**: Already handled (existing tests cover this). The parse function returns empty vec on malformed input.
7. **Mixed bare and MCP-prefixed in same session**: Both `context_search` and `mcp__unimatrix__context_search` in the same session should both count toward knowledge_served.

## Security Risks

This feature does not accept external/untrusted input. All data flows are internal:
- Tool names come from Claude Code's hook system (trusted)
- query_log and injection_log records come from the Store (trusted)
- No user-facing input parsing is added

The `normalize_tool_name` function performs a prefix strip on trusted strings. No injection, traversal, or deserialization attack surface is introduced.

**Blast radius**: If normalize_tool_name were compromised (e.g., returned attacker-controlled string), the impact is limited to incorrect tool classification and knowledge counting — no code execution, no data mutation, no privilege escalation.

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|-------------------|----------|
| normalize_tool_name receives unexpected prefix format | Returns input unchanged (passthrough) | Correct — unknown prefixes are not Unimatrix tools |
| Serde alias deserialization fails | Field defaults to 0 (via serde(default)) | Acceptable for new fields; concerning for renamed fields (silent data loss) |
| compute_knowledge_reuse_for_sessions Store query fails | Returns Err; caller logs warning, sets feature_knowledge_reuse to None | Report is incomplete but not invalid; debug tracing aids diagnosis |
| query_log has zero rows for valid sessions | delivery_count = 0 | Correct if no searches happened; debug tracing distinguishes from bug |
| Session ID format mismatch between tables | scan_query_log_by_sessions returns empty vec | Silent failure — debug tracing is the diagnostic path (ADR-005) |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (serde alias unidirectional compat) | R-02, R-13 | ADR-003 defines unidirectional strategy. Tests verify both deserialization (alias) and serialization (new names). Bidirectional not needed — reports are ephemeral. |
| SR-02 (serde alias + rename interaction) | R-02 | ADR-003 confirms no existing serde(rename) on affected fields. Risk cleared by investigation. |
| SR-03 (#193 root cause unbounded scope) | R-06 | ADR-005 defines time-box and scope boundary. Debug tracing (C6) provides diagnostic path. Store-layer fix is a separate issue if needed. |
| SR-04 (integration test scope ambiguity) | R-08 | ADR-002 decides Rust-only tests for col-020b. Infra-001 deferred to follow-up. |
| SR-05 (RetrospectiveReport consumer impact) | R-11, R-13 | ADR-003 confirms reports are ephemeral (no persistence). tool_distribution is extensible HashMap. |
| SR-06 (re-export path rename) | R-07 | Compile-time detection. grep for all KnowledgeReuse imports before implementing. |
| SR-07 (curate category in tool_distribution) | R-11 | NFR-04 establishes tool_distribution as extensible. New category is additive. |
| SR-08 (cross-crate test infrastructure gap) | R-08 | ADR-002 keeps tests within existing unit test patterns. No new cross-crate infrastructure. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-06, R-08) | 9 scenarios + code review + manual validation |
| High | 3 (R-01, R-04, R-10) | 15 scenarios |
| Medium | 4 (R-02, R-03, R-05, R-12) | 14 scenarios |
| Low | 4 (R-07, R-09, R-11, R-13) | 7 scenarios + compilation gate |
