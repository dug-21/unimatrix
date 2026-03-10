# Gate 3a Report: col-020

> Gate: 3a (Design Review)
> Date: 2026-03-10
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 6 components match architecture decomposition, boundaries, and ADRs |
| Specification coverage | PASS | All FR-01 through FR-08 and NFR-01 through NFR-05 have corresponding pseudocode |
| Risk coverage | PASS | All 15 risks (R-01 through R-15) mapped to test scenarios |
| Interface consistency | WARN | Minor naming discrepancy between Spec (AttributionCoverage) and pseudocode (AttributionMetadata) -- resolved by IMPLEMENTATION-BRIEF.md |

## Detailed Findings

### 1. Architecture Alignment
**Status**: PASS

**Component boundaries**: All six components (C1-C6) in pseudocode match architecture decomposition exactly.
- C1 (session_metrics) is a new file in unimatrix-observe -- matches Architecture section "C1: Session Metrics Module".
- C2 (types) extends unimatrix-observe/src/types.rs -- matches Architecture "C2: New Types".
- C3 (knowledge_reuse) is inline in the handler per ADR-001 -- matches Architecture "C3: Knowledge Reuse Computation".
- C4 (store_api) adds methods across query_log.rs, injection_log.rs, read.rs, topic_deliveries.rs -- matches Architecture "C4: Store API Extensions".
- C5 (report_builder) uses post-build mutation pattern with no signature change -- matches Architecture "C5: Report Builder Extension" decision.
- C6 (handler_integration) wires steps into context_retrospective handler -- matches Architecture "C6: Handler Integration".

**Interfaces match contracts**: The Integration Surface table in Architecture defines 8 interfaces. All 8 are reflected in pseudocode with matching signatures:
- `compute_session_summaries(records: &[ObservationRecord]) -> Vec<SessionSummary>` -- matches.
- `compute_context_reload_pct(summaries: &[SessionSummary], records: &[ObservationRecord]) -> f64` -- matches.
- `scan_query_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<QueryLogRecord>>` -- matches.
- `scan_injection_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<InjectionLogRecord>>` -- matches.
- `count_active_entries_by_category(&self) -> Result<HashMap<String, u64>>` -- matches.
- `set_topic_delivery_counters(&self, topic: &str, total_sessions: i64, total_tool_calls: i64, total_duration_secs: i64) -> Result<()>` -- matches.

**ADR compliance**:
- ADR-001 (server-side knowledge reuse): Pseudocode C3 is inline in handler, not in unimatrix-observe. Compliant.
- ADR-002 (idempotent counters): Pseudocode C4 uses UPDATE SET (absolute-set), not additive increment. Compliant.
- ADR-003 (attribution metadata): Pseudocode C2 defines AttributionMetadata and C6 populates it. Compliant.
- ADR-004 (file path extraction): Pseudocode C1 extract_file_path uses explicit tool-to-field mapping for Read/Edit/Write/Glob/Grep. Compliant.

**Data flow**: The pseudocode OVERVIEW.md data flow diagram matches Architecture lines 99-111 (Component Interactions section). Steps are in the same order: session summaries -> reload rate -> data loading -> knowledge reuse -> rework count -> attribution -> counter update -> report assignment.

**Error propagation**: All pseudocode new steps use best-effort pattern (Ok -> Some, Err -> warn + None) matching Architecture lines 117-121.

### 2. Specification Coverage
**Status**: PASS

**FR-01 (Session Summary)**: All sub-requirements FR-01.1 through FR-01.9 covered in pseudocode C1:
- FR-01.1: Grouping by session_id -- pseudocode compute_session_summaries groups records by session_id.
- FR-01.2: tool_distribution with PreToolUse filter -- pseudocode explicitly filters `record.hook == PreToolUse`.
- FR-01.3: top_file_zones top 5 -- pseudocode truncates to 5 after frequency sort.
- FR-01.4: File path extraction mapping -- pseudocode extract_file_path matches spec mapping (Read/Edit/Write -> file_path, Glob -> path). Grep added per ADR-004 (justified addition, not scope creep).
- FR-01.5: agents_spawned from SubagentStart -- pseudocode collects SubagentStart tool names.
- FR-01.6: knowledge_in/knowledge_out counts -- pseudocode counts search tools and context_store.
- FR-01.7: started_at and duration_secs -- pseudocode computes min/max timestamps with ms-to-sec conversion.
- FR-01.8: outcome from SessionRecord -- pseudocode initializes to None, handler enriches from SessionRecord.
- FR-01.9: Ordering by started_at ascending -- pseudocode sorts with session_id tiebreaker.

**FR-02 (Knowledge Reuse)**: All sub-requirements FR-02.1 through FR-02.6 covered in pseudocode C3:
- FR-02.1: tier1_reuse_count via cross-session entry deduplication -- pseudocode tracks entry_sessions map, filters to entries in 2+ sessions.
- FR-02.2: by_category breakdown -- pseudocode loads entry metadata and groups by category.
- FR-02.3: category_gaps -- pseudocode compute_gaps compares active categories to reused categories.
- FR-02.4: Server-side per ADR-001 -- pseudocode is inline in handler.
- FR-02.5: Graceful degradation with missing data -- pseudocode handles empty query_log/injection_log.
- FR-02.6: JSON parsing with fallback -- pseudocode parse_result_entry_ids uses serde_json with empty-vec fallback.

**FR-03 (Rework Session Count)**: Pseudocode C6 step 15 implements case-insensitive substring match on "result:rework" and "result:failed" per human-resolved variance documented in IMPLEMENTATION-BRIEF.md.

**FR-04 (Context Reload Pct)**: Pseudocode C1 compute_context_reload_pct implements:
- FR-04.1: Percentage of reloaded files in subsequent sessions.
- FR-04.2: Chronological ordering with session_id tiebreaker.
- FR-04.3: Returns 0.0 for single session.
- FR-04.4: Raw float, no interpretation labels.

**FR-05 (Report Extension)**: Pseudocode C2 defines all 5 new Option fields with serde attributes:
- FR-05.1: session_summaries -- present.
- FR-05.2: knowledge_reuse -- present.
- FR-05.3: rework_session_count -- present.
- FR-05.4: context_reload_pct -- present.
- FR-05.5: serde(default, skip_serializing_if) -- present on all fields.
- FR-05.6: attribution (as AttributionMetadata) -- present.

**FR-06 (Topic Deliveries Counter Update)**: Pseudocode C4 set_topic_delivery_counters and C6 step 17 implement idempotent absolute-set.

**FR-07 (Handler Integration)**: Pseudocode C6 wires all steps after existing pipeline with best-effort error handling.

**FR-08 (Store API Extensions)**: Pseudocode C4 defines all 4 new methods matching FR-08.1 through FR-08.4.

**NFR coverage**:
- NFR-01 (Performance): Batch queries chunked to 50 (C4 pseudocode). No new async coordination.
- NFR-02 (Backward compat): All new fields Optional with serde defaults (C2 pseudocode).
- NFR-03 (No regression): C5 test plan verifies existing tests pass unchanged.
- NFR-04 (Graceful degradation): Best-effort pattern in C6 pseudocode.
- NFR-05 (Attribution transparency): AttributionMetadata in C2 pseudocode.

**Scope additions**: None detected. Grep in file path mapping was explicitly approved (ADR-004). AttributionMetadata was added to address SR-07 (documented in IMPLEMENTATION-BRIEF.md). No unrequested features.

### 3. Risk Coverage
**Status**: PASS

Every identified risk from RISK-TEST-STRATEGY.md maps to at least one test scenario in the test plans:

| Risk | Priority | Test Plan Coverage |
|------|----------|-------------------|
| R-01 (JSON parsing) | High | knowledge_reuse.md: 4 tests (malformed, empty, null, duplicates) |
| R-02 (Data gaps) | High | knowledge_reuse.md: 3 tests (no query_log, no injection_log, both empty) |
| R-03 (Attribution coverage) | High | handler_integration.md: test_retrospective_produces_attribution |
| R-04 (Server-side testability) | High | knowledge_reuse.md: 4 core reuse tests (cross-session, same-session, dedup) |
| R-05 (Non-idempotent counters) | High | store_api.md: 3 tests (basic, idempotent, overwrite); handler_integration.md: 2 tests |
| R-06 (File path mapping gaps) | Med | session_metrics.md: 7 extract_file_path tests covering all tools + unknown + edge cases |
| R-07 (Concurrent session ordering) | Med | session_metrics.md: test_session_summaries_tiebreak_by_session_id |
| R-08 (Rework false positives) | Med | handler_integration.md: 3 rework tests (case-insensitive, null excluded, substring) |
| R-09 (Backward deserialization) | High | types.md: 4 tests (pre-col020 JSON, None omission, round-trip, partial fields) |
| R-10 (Empty topic) | High | session_metrics.md: 2 empty tests; knowledge_reuse.md: 2 empty tests; handler_integration.md: 2 empty tests |
| R-11 (Large IN clauses) | Low | store_api.md: test_scan_query_log_by_sessions_empty_ids, test_scan_injection_log_by_sessions_empty_ids |
| R-12 (Double-counting reuse) | Med | knowledge_reuse.md: 2 dedup tests (across sources, across sessions) |
| R-13 (Division by zero) | Med | session_metrics.md: test_reload_pct_no_files_in_later_sessions, test_reload_pct_single_session |
| R-14 (New steps abort pipeline) | High | handler_integration.md: 3 graceful degradation tests (knowledge reuse failure, session summary failure, counter failure) |
| R-15 (Inconsistent zones) | Low | session_metrics.md: 3 extract_directory_zone tests (absolute, relative, trailing slash) |

**Integration risks** from Risk Strategy are also covered:
- C1<->C6 (PreToolUse filtering): session_metrics.md test_session_summaries_filters_pretooluse_only.
- C3<->C4 (query_log field semantics): knowledge_reuse.md core reuse tests with seeded Store.
- C4 concurrent modification: store_api.md idempotent overwrite tests.

**Edge cases** from Risk Strategy are covered:
- Single observation record: session_metrics.md test_session_summaries_single_record.
- Same-session retrieval: knowledge_reuse.md test_knowledge_reuse_same_session_excluded.
- Duplicate entry IDs: knowledge_reuse.md test_knowledge_reuse_duplicate_ids_in_result.

### 4. Interface Consistency
**Status**: WARN

**Shared types alignment**: OVERVIEW.md defines SessionSummary, KnowledgeReuse, and AttributionMetadata with field definitions that match the C2 types.md pseudocode exactly.

**Cross-component data flow coherence**:
- C1 produces `Vec<SessionSummary>` consumed by C6 handler -- types match.
- C1 produces `f64` reload_pct consumed by C6 -- types match.
- C4 produces `Vec<QueryLogRecord>` and `Vec<InjectionLogRecord>` consumed by C3 -- types match existing Store types.
- C4 produces `HashMap<String, u64>` for active category counts consumed by C3 -- types match.
- C3 produces `KnowledgeReuse` consumed by C6 -- types match C2 definition.

**No contradictions** between component pseudocode files detected.

**Minor observation (WARN)**: The Specification uses `AttributionCoverage` (FR-05.6) while the Architecture, IMPLEMENTATION-BRIEF, and all pseudocode use `AttributionMetadata`. The OVERVIEW.md Naming Conventions section explicitly calls this out and resolves it: "Use AttributionMetadata (architecture is authoritative per IMPLEMENTATION-BRIEF.md resolved decisions)." This is a documented, intentional variance, not a bug. Marked as WARN for traceability.

**Parameter type variance**: The Specification FR-08.1 and FR-08.2 define batch scan methods with `session_ids: &[String]` while the Architecture and pseudocode use `session_ids: &[&str]`. The pseudocode follows the Architecture's Integration Surface table (`&[&str]`). This is consistent within the pseudocode set and follows Rust conventions (accepting `&str` is more flexible than requiring `&String`). The Specification's `&[String]` is the less-idiomatic form. Not a bug -- the pseudocode correctly follows the Architecture which takes precedence for implementation signatures.

## Rework Required

None.

## Scope Concerns

None.
