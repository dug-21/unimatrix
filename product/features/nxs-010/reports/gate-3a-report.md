# Gate 3a Report: nxs-010

> Gate: 3a (Design Review)
> Date: 2026-03-10
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 5 components map to architecture decomposition; interfaces, crate boundaries, and ADR decisions match |
| Specification coverage | PASS | All 8 functional requirements (FR-01 through FR-08) and 6 non-functional requirements have corresponding pseudocode |
| Risk coverage | PASS | All 14 risks mapped; 27 test scenarios across 4 test plans cover Critical/High/Med risks; 3 Low/Med risks accepted |
| Interface consistency | PASS (2 WARNs) | Shared types consistent across pseudocode files; two minor items flagged below |

## Detailed Findings

### Architecture Alignment
**Status**: PASS
**Evidence**:
- Components C1-C5 in pseudocode map 1:1 to Architecture section "Component Breakdown" (C1: Schema DDL, C2: Migration, C3: topic_deliveries, C4: query_log, C5: Search Pipeline Integration).
- Crate boundaries maintained: C1-C4 in unimatrix-store, C5 in unimatrix-server (UDS listener.rs + MCP tools.rs). Matches Architecture lines 7-10.
- ADR-001 (AUTOINCREMENT for query_log): Pseudocode schema-ddl.md uses `INTEGER PRIMARY KEY AUTOINCREMENT`. query-log.md omits query_id from INSERT column list. Consistent.
- ADR-002 (fire-and-forget writes): search-pipeline-integration.md UDS uses `spawn_blocking_fire_and_forget`, MCP uses `tokio::task::spawn_blocking` with dropped handle. Both log warn on failure, no retry. Matches ADR-002 and Architecture line 46.
- ADR-003 (backfill in main transaction): migration.md runs within existing `BEGIN IMMEDIATE` transaction. No separate transaction. Matches ADR-003 and Architecture line 109.
- Data flow in pseudocode OVERVIEW.md matches Architecture "Component Interactions" diagram (lines 50-90).
- Module registration (lib.rs pub mod + pub use) specified in OVERVIEW.md, matching Architecture lines 172-182.

### Specification Coverage
**Status**: PASS
**Evidence**:
- **FR-01** (topic_deliveries DDL): schema-ddl.md lines 15-25 reproduce the exact DDL from Specification FR-01.1. All 9 columns present with correct types and constraints.
- **FR-02** (query_log DDL): schema-ddl.md lines 26-38 reproduce DDL from FR-02.1. AUTOINCREMENT present (FR-02.3). Two indexes created (FR-02.2).
- **FR-03** (migration): migration.md covers FR-03.1 (guard `current_version < 11`), FR-03.2 (IF NOT EXISTS DDL), FR-03.3 (backfill SQL matches spec exactly), FR-03.4 (CURRENT_SCHEMA_VERSION=11), FR-03.5 (version update in transaction), FR-03.6 (main transaction), FR-03.7 (total_tool_calls=0), FR-03.8 (status='completed').
- **FR-04** (TopicDeliveryRecord + API): topic-deliveries.md defines all 4 methods matching FR-04.2-FR-04.5. Struct fields match FR-04.1.
- **FR-05** (QueryLogRecord + API): query-log.md defines struct and 2 methods matching FR-05.1-FR-05.3.
- **FR-06** (UDS integration): search-pipeline-integration.md UDS section covers FR-06.1 (fire-and-forget after injection_log), FR-06.2 (all field values), FR-06.3 (warn on failure).
- **FR-07** (MCP integration): search-pipeline-integration.md MCP section covers FR-07.1-FR-07.3.
- **FR-08** (shared constructor): query-log.md defines `QueryLogRecord::new` constructor used by both paths, satisfying FR-08.1.
- **NFR-01-06**: Addressed via design patterns (indexes for NFR-03, IF NOT EXISTS for NFR-04, serde_json for NFR-06).
- **No scope additions**: Pseudocode implements only what the Specification requires. No unrequested features.

### Risk Coverage
**Status**: PASS
**Evidence**:
- **R-01** (migration partial apply, High): migration test plan has 3 scenarios -- basic migration, idempotent re-run, partial re-run. Covers all R-01 test scenarios from Risk Strategy.
- **R-02** (backfill aggregates, Critical): migration test plan has 4 scenarios -- basic with known aggregates, no attributed sessions, NULL/empty feature_cycle exclusion, multiple topics. Covers all R-02 scenarios.
- **R-03** (AUTOINCREMENT, Med): query-log test plan has 2 scenarios -- autoincrement allocation, monotonic IDs. Covers R-03 scenarios.
- **R-04** (fire-and-forget panic, Critical): search-pipeline-integration test plan has 4 scenarios -- UDS write, MCP write, failure handling, session_id=None guard. Covers all R-04 scenarios.
- **R-05** (field divergence, High): search-pipeline-integration test plan has 2 scenarios -- field parity comparison, shared constructor verification. Covers R-05 scenarios.
- **R-06** (JSON edge cases, High): query-log test plan has 3 scenarios -- empty arrays, multi-element with boundary values, single-element. Covers all R-06 scenarios.
- **R-07** (nonexistent topic update, High): topic-deliveries test plan has 3 scenarios -- error on missing, increment, decrement. Covers all R-07 scenarios.
- **R-08** (fresh DB, Med): migration test plan has 1 scenario -- fresh database skips migration. Covers R-08.
- **R-09** (concurrent open, Med): Accepted risk, 0 scenarios. Risk Strategy accepts this (SQLite exclusive transaction provides serialization).
- **R-10** (INSERT OR REPLACE, Critical): topic-deliveries test plan has 2 scenarios -- replace semantics documented, overwrite verification. Covers R-10 scenarios.
- **R-11** (write lock contention, Med): Accepted risk. Sequential processing mitigates.
- **R-12** (scan ordering, Med): query-log test plan has 2 scenarios -- ordering verification, cross-session isolation. Covers R-12.
- **R-13** (whitespace topics, Low): Accepted risk, 0 scenarios. Consistent with Risk Strategy.
- **R-14** (NULL ended_at, High): migration test plan has 2 scenarios -- mixed NULL/non-NULL, all NULL. Covers both R-14 scenarios.

Total: 27 test scenarios across 14 risks. All Critical and High risks have test coverage. 3 Medium/Low risks accepted without tests (R-09, R-11, R-13), consistent with Risk Strategy.

### Interface Consistency
**Status**: PASS (2 WARNs)

**Evidence -- consistent interfaces**:
- `TopicDeliveryRecord` fields are identical in OVERVIEW.md shared types, topic-deliveries.md struct definition, and Architecture integration surface table (line 139).
- `QueryLogRecord` fields are identical in OVERVIEW.md shared types, query-log.md struct definition, and Architecture integration surface table (line 144).
- Store method signatures match across pseudocode files: topic-deliveries.md defines 4 methods, query-log.md defines 2 methods, search-pipeline-integration.md calls `insert_query_log` -- all consistent with Architecture integration surface.
- Data flow between components is coherent: migration (C2) creates tables that schema-ddl (C1) also defines via IF NOT EXISTS. topic-deliveries (C3) and query-log (C4) provide Store methods that search-pipeline-integration (C5) consumes.

**WARN 1 -- result_count type discrepancy (minor)**:
- Specification FR-05.1 says `result_count: i64`.
- Implementation Brief data structure says `result_count: u32`.
- Pseudocode OVERVIEW.md and query-log.md use `i64`, aligning with the Specification.
- The pseudocode agent flagged this as open question #2 with rationale: i64 matches Specification and SQLite INTEGER type.
- Impact: None. Pseudocode is internally consistent and aligns with the authoritative source (Specification). The Implementation Brief discrepancy is a documentation artifact that does not affect correctness.

**WARN 2 -- StoreError variant for missing topic (minor)**:
- topic-deliveries.md uses `StoreError::Deserialization(format!("topic_delivery not found: {}", topic))` for the missing-topic error in `update_topic_delivery_counters`.
- The pseudocode agent flagged this as open question #1: the implementation agent should decide whether to use `Deserialization` (existing variant, semantic mismatch) or add a new `TopicNotFound(String)` variant.
- Impact: The error behavior (return Err, not silent Ok) is correct per R-07. The variant choice is an implementation detail that does not affect the design's correctness or risk coverage.

## Rework Required

None. Both WARNs are acknowledged open questions for the implementation agent, not design gaps.

## Open Questions Forwarded to Implementation

1. **StoreError variant**: `update_topic_delivery_counters` needs an error for missing topic. Pseudocode uses `StoreError::Deserialization` as fallback. Implementation agent should choose: reuse `Deserialization`, or add `TopicNotFound(String)` to `StoreError`.
2. **result_count type**: Pseudocode uses `i64` per Specification. Implementation Brief says `u32`. Use `i64`.
