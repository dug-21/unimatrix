# Gate 3a Report: nxs-012

> Gate: 3a (Design Review)
> Date: 2026-05-28
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 5 components match ARCHITECTURE.md decomposition, interfaces, and ADR decisions |
| Specification coverage | WARN | All 29 FRs covered; Constraint 7 NaN fallback value (0) contradicts ADR-003 and Specification Key Terms (1.0) -- spec internal inconsistency, pseudocode follows ADR-003 correctly |
| Risk coverage | PASS | All 24 risks from RISK-TEST-STRATEGY.md mapped to test scenarios across the 5 test plans; 65 scenarios planned |
| Interface consistency | WARN | Implementation Brief lists `ExportCounts` return type for `export_graph_edges` which is undefined elsewhere; pseudocode uses `Result<u64>` for skip-aware version -- internally consistent but diverges from brief |
| Knowledge stewardship | PASS | Pseudocode agent has Queried entries; testplan agent has Queried + Stored (nothing novel with reason); architect and risk-strategist reports have storage evidence |

## Detailed Findings

### Architecture Alignment
**Status**: PASS

**Evidence**: Each pseudocode component maps 1:1 to the architecture's component breakdown (C1-C5):

- **C1 format-types**: Pseudocode defines GraphEdgeRow (9 fields), ObservationRow (10 fields), CycleEventRow (9 fields) matching Architecture Integration Surface table exactly. ExportRow variants use correct `serde(rename)` values. Field names, types, and nullability all match.
- **C2 export-functions**: Three new functions with correct SQL column selections, ORDER BY clauses (FR-01: source_id/target_id/relation_type; FR-02/FR-03: id), and NaN-safe weight handling (ADR-003, fallback 1.0). do_export integration places new calls after audit_log per FR-14.
- **C3 import-inserters**: Three functions with correct INSERT patterns -- plain INSERT for graph_edges (FR-09), explicit id for observations/cycle_events (ADR-006), NULL goal_embedding (ADR-004). 9/10/10 column bindings match architecture.
- **C4 import-pipeline**: ImportCounts extension (3 fields), ingest_rows match arms (3 new), drop_all_data DELETE ordering matches ADR-001 (observation_phase_metrics before observation_metrics before observations), format_version validation accepts 1|2 per ADR-002.
- **C5 skip-quarantined**: CLI flags (--skip-quarantined, --confirm) per ADR-009. Skip-set built inside DEFERRED transaction per ADR-008. 5 affected exporters receive skip_ids. 6 unaffected exporters unchanged. Dual-column checks for co_access (entry_id_a/entry_id_b) and graph_edges (source_id/target_id).

Technology choices (sqlx, serde_json, HashSet) consistent with ADRs. Build order (Wave 1-3) respects dependency graph.

### Specification Coverage
**Status**: WARN

**Evidence**: All 29 functional requirements (FR-01 through FR-29) have corresponding pseudocode. All 31 acceptance criteria (AC-01 through AC-31) have verification methods defined in test plans. No scope additions detected -- pseudocode implements exactly what the specification requires.

Non-functional requirements addressed:
- NFR-01/02: Performance addressed by using existing patterns (fetch_all, single transaction)
- NFR-03: File size additions estimated in specification; pseudocode does not introduce unexpectedly large additions
- NFR-04: Backward compatibility via format_version 1|2 acceptance
- NFR-05: Snapshot isolation -- pseudocode places all queries inside BEGIN DEFERRED
- NFR-06/07: Skip set memory/performance -- HashSet<i64> with O(1) lookup

**Issue (WARN)**: Specification Constraint 7 states the weight fallback should use `unwrap_or(Number::from(0))`, matching the entries.confidence pattern. However, ADR-003 explicitly decided on 1.0 fallback (because weight=0 would nullify edge significance), and the Specification's own Key Terms section says "graph_edges.weight uses Number::from_f64 with 1.0 fallback (ADR-003)." The pseudocode correctly follows ADR-003 (1.0). This is an internal inconsistency within the Specification -- Constraint 7 appears to be a copy-paste from the confidence pattern. The correct behavior is 1.0 per ADR-003.

No other specification gaps found.

### Risk Coverage
**Status**: PASS

**Evidence**: All 24 risks mapped to test scenarios:

**High priority (10 risks, 33 scenarios)**:
- R-01 (NaN weight): 5 scenarios in export-functions test plan -- NaN, INFINITY, NEG_INFINITY, normal precision, zero
- R-02 (FK cascade): 3 scenarios in import-pipeline test plan -- full clear, derived-only, FK ordering
- R-14 (transaction isolation): 1 unit + 1 integration in export-functions
- R-15 (NULL goal_embedding): 1 scenario in import-inserters (goal_embedding_null test)
- R-16 (cascade incompleteness): 4 integration tests in skip-quarantined -- per-table + integrated cascade
- R-17 (TOCTOU): 1 integration + code review in skip-quarantined
- R-18 (default path): 3 tests across skip-quarantined -- unchanged path, zero quarantined, confirm-alone
- R-19 (co_access dual-column): 2 tests -- 4-combination matrix + self-referencing
- R-20 (graph_edges dual-column): 2 tests -- 4-combination matrix + self-loop
- R-23 (--confirm bypass): 3 unit + 1 integration -- all 4 flag combinations

**Medium priority (7 risks, 18 scenarios)**: R-03, R-04, R-05, R-08, R-09, R-10, R-11 all have test plans with appropriate scenarios covering boundary values and round-trip verification.

**Low priority (7 risks, 14 scenarios)**: R-06, R-07, R-12, R-13, R-21, R-22, R-24 covered with appropriate test depth.

Integration and edge case scenarios from Risk Strategy are present: round-trip tests (AC-15), empty table exports (edge case #1), Unicode (edge case #6), weight=0.0 (edge case #7), all-quarantined (edge case #10), self-referencing co_access (edge case #11), self-loop graph_edges (edge case #12).

### Interface Consistency
**Status**: WARN

**Evidence**: Shared types defined in pseudocode OVERVIEW.md match per-component usage:
- GraphEdgeRow, ObservationRow, CycleEventRow defined in format-types.md, consumed by export-functions, import-inserters, and import-pipeline
- ExportRow variants used in import-pipeline ingest_rows match arms
- Data flow between components is coherent: export serializes via Map + write_row, import deserializes via serde tagged enum into row structs, inserters consume row structs

The wave-based build order properly handles the signature evolution: Wave 2 creates base export functions (no skip_ids), Wave 3 modifies them (adds skip_ids). The pseudocode files are self-consistent within each wave.

**Issue (WARN)**: The IMPLEMENTATION-BRIEF's function signatures section (line 146) shows `export_graph_edges` returning `Result<ExportCounts, Box<dyn Error>>`. The `ExportCounts` type is never defined in the Architecture, pseudocode, or anywhere else. The pseudocode uses `Result<(), Box<dyn Error>>` (base) and `Result<u64, Box<dyn Error>>` (skip-aware), which is internally consistent. The Implementation Brief artifact has a minor discrepancy that won't affect implementation since developers work from pseudocode, not the brief.

No contradictions found between component pseudocode files. The skip-quarantined pseudocode correctly identifies which exporters are affected (5) and which are not (6), matching the Architecture's filter cascade table.

### Knowledge Stewardship Compliance
**Status**: PASS

**Evidence**:
- **Pseudocode agent** (nxs-012-agent-1-pseudocode-report.md): Contains `## Knowledge Stewardship` section with 3 `Queried:` entries (context_briefing, context_search x2). Read-only agent -- queries are appropriate. No novel patterns to store.
- **Test plan agent** (nxs-012-agent-2-testplan-report.md): Contains `## Knowledge Stewardship` section with `Queried:` entry and `Stored: nothing novel to store -- test plan follows established patterns from prior export/import features (nan-001, nan-002)` with reason provided.
- **Architect** (nxs-012-agent-1c-architect-report.md): Has `## Unimatrix Actions` section with Deprecated (#4614) and Stored (#4615, #4616) entries. Active-storage agent fulfilled obligation.
- **Risk strategist** (nxs-012-agent-3c-risk-report.md): Contains `## Knowledge Stewardship` section with 4 `Queried:` entries and `Stored: nothing novel to store` with reason.

All design-phase agents have stewardship evidence.

## Rework Required

None.

## Notes

1. The Specification Constraint 7 internal inconsistency (NaN fallback 0 vs 1.0) should be corrected in a future spec update to match ADR-003. It does not block implementation since the ADR is authoritative and the spec's Key Terms section agrees with ADR-003.
2. The Implementation Brief's `ExportCounts` return type reference should be noted but does not block implementation since pseudocode is the implementation source.
