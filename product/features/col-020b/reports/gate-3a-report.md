# Gate 3a Report: col-020b

> Gate: 3a (Component Design Review)
> Date: 2026-03-10
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 7 components map to architecture C1-C7; boundaries, interfaces, and ADRs respected |
| Specification coverage | PASS | All FR-01 through FR-08 and AC-01 through AC-16 covered by pseudocode |
| Risk coverage | PASS | All 13 risks (R-01 through R-13) mapped to test scenarios; critical risks R-06 and R-08 addressed |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage; data flow is coherent |

## Detailed Findings

### Architecture Alignment
**Status**: PASS
**Evidence**:

1. **Component boundaries match architecture decomposition.** The architecture defines 7 components (C1-C7) across 5 files in 2 crates. Each pseudocode file maps 1:1 to an architecture component:
   - C1 (tool-name-normalizer) -> `session_metrics.rs` -- matches architecture Section "C1: Tool Name Normalizer"
   - C2 (tool-classification) -> `session_metrics.rs` -- matches "C2: Tool Classification Extension"
   - C3 (knowledge-curated-counter) -> `session_metrics.rs` -- matches "C3: Knowledge Curated Counter"
   - C4 (type-renames) -> `types.rs` -- matches "C4: Type Renames"
   - C5 (knowledge-reuse-semantics) -> `knowledge_reuse.rs` -- matches "C5: Knowledge Reuse Semantics Revision"
   - C6 (data-flow-debugging) -> `tools.rs` -- matches "C6: Data Flow Debugging"
   - C7 (re-export-update) -> `lib.rs`, `tools.rs` -- matches "C7: Re-export Update"

2. **Interfaces match defined contracts.** The architecture's Integration Surface table (lines 196-207) defines 10 interface points. Each pseudocode file references the correct signatures:
   - `normalize_tool_name` is `fn(&str) -> &str` in both architecture and pseudocode C1
   - `classify_tool` is `fn(&str) -> &'static str` in both architecture and pseudocode C2
   - `compute_knowledge_reuse` returns `FeatureKnowledgeReuse` in both architecture and pseudocode C5
   - `compute_knowledge_reuse_for_sessions` returns `Result<FeatureKnowledgeReuse>` in both architecture and pseudocode C6

3. **Technology choices consistent with ADRs.** Architecture references 5 ADRs. Pseudocode respects each:
   - ADR-001: `normalize_tool_name` is private in `session_metrics.rs` (C1 pseudocode confirms "Private helper")
   - ADR-002: Test plans use Rust unit tests only, no infra-001 (test plan OVERVIEW.md confirms)
   - ADR-003: Serde alias used for unidirectional compat (C4 pseudocode specifies `#[serde(alias = ...)]`)
   - ADR-004: `FeatureKnowledgeReuse` stays in `unimatrix-server` (C5 pseudocode modifies `knowledge_reuse.rs` in server crate)
   - ADR-005: C6 pseudocode adds debug tracing without scope expansion

4. **Pseudocode matches existing code structure.** Verified against actual source files:
   - `session_metrics.rs`: `classify_tool` at line 187, `build_session_summary` at line 103, `knowledge_in`/`knowledge_out` at lines 157-171 -- pseudocode C1/C2/C3 reference correct locations
   - `types.rs`: `SessionSummary` at line 170 with `knowledge_in`/`knowledge_out`, `KnowledgeReuse` at line 194 with `tier1_reuse_count` -- pseudocode C4 correctly identifies fields to rename
   - `lib.rs`: re-export of `KnowledgeReuse` at line 32 -- pseudocode C7 correctly targets this
   - `knowledge_reuse.rs`: `compute_knowledge_reuse` at line 55 returning `KnowledgeReuse`, Step 5 filter at lines 108-121 -- pseudocode C5 correctly identifies the semantic change location
   - `tools.rs`: `compute_knowledge_reuse_for_sessions` at line 1619 returning `KnowledgeReuse`, caller at line 1288 setting `report.knowledge_reuse` -- pseudocode C6 correctly targets these

### Specification Coverage
**Status**: PASS
**Evidence**:

Every functional requirement has corresponding pseudocode:

| Requirement | Pseudocode Component | Coverage |
|-------------|---------------------|----------|
| FR-01.1 (normalize_tool_name) | C1: exact function signature and implementation | Full |
| FR-01.2 (classify_tool calls normalizer) | C2: `let normalized = normalize_tool_name(tool)` | Full |
| FR-01.3 (knowledge counter normalization) | C3: `.map(normalize_tool_name)` in all 3 counters | Full |
| FR-01.4 (extract_file_path NOT changed) | C1: explicitly states "NOT applied to extract_file_path" | Full |
| FR-02.1 (category mapping table) | C2: match arms reproduce exact table from spec | Full |
| FR-02.2 (MCP prefix handling) | C2: normalization before match | Full |
| FR-02.3 (admin tools stay "other") | C2: explicitly listed as remaining in "other" | Full |
| FR-03.1-3.6 (SessionSummary renames) | C4: field renames with serde annotations; C3: counter logic | Full |
| FR-04.1-4.5 (FeatureKnowledgeReuse) | C4: type rename; C5: semantic revision | Full |
| FR-05.1 (RetrospectiveReport rename) | C4: field rename with serde alias | Full |
| FR-06.1-6.5 (compute_knowledge_reuse revision) | C5: Steps 5a/5b split, all_entry_ids vs cross_session_ids | Full |
| FR-07.1 (debug tracing) | C6: 4 tracing::debug! statements at data flow boundaries | Full |
| FR-07.2 (#193 investigation) | C6: debug log enables diagnosis; ADR-005 boundary acknowledged | Full |
| FR-08.1 (lib.rs re-export) | C7: explicit re-export change | Full |
| FR-08.2 (import site updates) | C7: lists all import sites + grep verification | Full |

Non-functional requirements addressed:
- NFR-01: C1 pseudocode specifies "O(1), zero allocation" using `strip_prefix`
- NFR-02: No new dependencies in any pseudocode
- NFR-03: C4/C5 pseudocode detail every existing test that needs field name updates
- NFR-04: C2/C3 pseudocode treat `curate` as additive to HashMap

No scope additions detected. Pseudocode implements only what the specification requires. Constraints C-01 through C-07 are respected (no Store changes, no extract_file_path changes, no detection rule changes, no ObservationSource changes, no recording pipeline changes, no briefing counting, time-boxed #193).

### Risk Coverage
**Status**: PASS
**Evidence**:

All 13 risks from the Risk-Based Test Strategy are mapped to test scenarios in the component test plans:

| Risk | Priority | Test Plan | Scenarios | Adequate? |
|------|----------|-----------|-----------|-----------|
| R-01 (normalize edge cases) | High | tool-name-normalizer.md | 8 tests covering standard, passthrough, double prefix, empty, case-sensitive, different server | Yes |
| R-02 (serde alias drops fields) | Med | type-renames.md | 6 scenarios: alias deserialization for SessionSummary, FeatureKnowledgeReuse, RetrospectiveReport + round-trips | Yes |
| R-03 (serde default incorrect zero) | Med | type-renames.md | 3 scenarios: default when absent, value preserved when present | Yes |
| R-04 (delivery_count miscount) | High | knowledge-reuse-semantics.md | 5 new tests + updated existing tests covering single-session, multi-session, dedup, cross-source | Yes |
| R-05 (by_category wrong set) | Med | knowledge-reuse-semantics.md | 4 scenarios: single-session by_category, category_gaps delivery-based | Yes |
| R-06 (#193 data flow) | Critical | data-flow-debugging.md | Code review checklist (4 log points) + error handling review (4 checks). ADR-002 accepts unit test gap. | Yes (within ADR-002 scope) |
| R-07 (re-export compile) | Low | re-export-update.md | Compilation gate + grep verification | Yes |
| R-08 (MCP-prefix gap) | Critical | tool-classification.md + knowledge-curated-counter.md | 7 MCP-prefixed classify_tool tests + full session summary with MCP inputs | Yes |
| R-09 (curate mapping error) | Low | tool-classification.md | Exhaustive bare-name test + admin tools exclusion test | Yes |
| R-10 (inconsistent normalization) | High | knowledge-curated-counter.md | Mixed bare + prefixed test proving all 3 counters normalize | Yes |
| R-11 (curate key breaks consumers) | Low | knowledge-curated-counter.md | Curate key presence/absence tests | Yes |
| R-12 (spawn_blocking swallow) | Med | data-flow-debugging.md | Code review for error propagation, None vs Some(zeroed) | Yes |
| R-13 (new field names in output) | Low | type-renames.md | Serialization assertions in round-trip tests | Yes |

Integration risks from the strategy are covered:
- session_metrics <-> types: C3 test plan exercises field writes; C4 test plan exercises serde compat
- knowledge_reuse <-> types: C5 test plan asserts both delivery_count and cross_session_count
- tools <-> knowledge_reuse: C6 pseudocode updates caller field name and return type
- lib.rs re-export: C7 test plan uses compilation gate

Edge cases from risk strategy addressed:
- Empty tool name: C1 test plan scenario 6
- Prefix-only tool name: C1 test plan scenario 5
- Double prefix: C1 test plan scenario 4
- Zero entries delivered: C5 existing test `test_knowledge_reuse_zero_sessions`
- Duplicate entry IDs across sources: C5 new test `test_knowledge_reuse_dedup_across_query_and_injection_same_session`
- Mixed bare and MCP-prefixed: C3 test plan `test_session_summaries_mixed_bare_and_prefixed`

### Interface Consistency
**Status**: PASS
**Evidence**:

1. **Shared types in OVERVIEW.md match per-component usage.** The OVERVIEW defines 3 type changes:
   - `SessionSummary`: fields `knowledge_served`, `knowledge_stored`, `knowledge_curated` with exact serde annotations. C3 pseudocode constructs the struct with these exact field names. C4 pseudocode defines the struct with matching annotations.
   - `FeatureKnowledgeReuse`: fields `delivery_count`, `cross_session_count` with exact serde annotations. C5 pseudocode returns this type with both fields populated. C4 pseudocode defines the struct.
   - `RetrospectiveReport.feature_knowledge_reuse`: C6 pseudocode sets this field (Change 7). C4 pseudocode defines the field with serde alias.

2. **Data flow between components is coherent.** The OVERVIEW data flow diagram shows:
   - ObservationRecord -> session_metrics (C1/C2/C3) -> SessionSummary (C4): C3 pseudocode produces SessionSummary with new fields
   - tools.rs (C6) -> knowledge_reuse.rs (C5) -> FeatureKnowledgeReuse (C4): C6 pseudocode calls C5, C5 returns FeatureKnowledgeReuse
   - FeatureKnowledgeReuse -> RetrospectiveReport (C4): C6 Change 7 sets `report.feature_knowledge_reuse`

3. **No contradictions between component pseudocode files.** Verified:
   - C1 defines `normalize_tool_name`. C2 calls it (`let normalized = normalize_tool_name(tool)`). C3 calls it (`.map(normalize_tool_name)`). Consistent.
   - C4 defines `FeatureKnowledgeReuse` struct. C5 returns it. C6 receives it. C7 re-exports it. All reference the same field names.
   - Build order in OVERVIEW (C4 -> C7 -> C1 -> C2 -> C3 -> C5 -> C6) respects dependencies. No circular dependency.

4. **OVERVIEW data flow diagram matches pseudocode.** The knowledge_served counter is labeled [C3] in the diagram but is actually a change to C1's call site within C3. This is consistent -- the OVERVIEW correctly attributes it to C3 (the counter component) which depends on C1 (the normalizer).

## WARN: Minor Observations

1. **C5 pseudocode presents two approaches for Step 6b** (double-lookup vs resolved_entries HashMap). The preferred approach is marked but the implementer should use the "preferred for cleanliness" variant. This is guidance, not a gap.

2. **C6 pseudocode Change 8** lists specific line numbers for RetrospectiveReport literal updates in tools.rs tests. Line numbers may drift during implementation. The grep-based discovery approach documented in C7 is the reliable fallback.

## Rework Required

None.

## Scope Concerns

None.
