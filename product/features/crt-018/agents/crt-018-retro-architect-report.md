# Retrospective Architect Report: crt-018

Agent: crt-018-retro-architect
Date: 2026-03-11

## 1. Patterns

### New Entries

| ID | Title | Reason |
|----|-------|--------|
| #1042 | Pure Computation Engine Module Pattern | Second instance (after confidence.rs) validates the zero-I/O engine module structure. Documents layout, conventions, and integration point. |
| #1043 | Subquery Dedup Before JOIN Aggregation | New SQL technique from R-03 fix. Prevents count inflation when junction table has duplicate composite keys. |

### Existing Patterns -- Verified Still Accurate

| ID | Title | crt-018 Usage | Status |
|----|-------|---------------|--------|
| #726 | SQL Aggregation Struct | EffectivenessAggregates follows StatusAggregates pattern exactly. | Accurate, no update needed. |
| #882 | Best-Effort Optional Computation | Phase 8 uses Option + match + warn + None for graceful degradation. | Accurate, no update needed. |
| #320 | Intermediate Serialization Struct | EffectivenessReportJson + sub-structs with skip_serializing_if. | Accurate, no update needed. |

### Skipped (One-Off Work)

| Component | Reason |
|-----------|--------|
| DataWindow struct construction from flat fields | Specific to avoiding store-to-engine dependency. The broader principle (avoid cross-crate type imports when flat fields suffice) is already covered by #755 (Dependency Inversion). |
| Match loop for EffectivenessCategory instead of HashMap | Minor deviation from pseudocode due to lacking Hash derive. Not a reusable pattern -- just an implementation detail. |

## 2. Procedures

### StatusService Phase Addition

No existing procedure for "adding a new phase to StatusService::compute_report" was found. The closest is #323 (How to add a new service to ServiceLayer), which covers service creation but not phase extension.

However, crt-018's Phase 8 follows the exact same structure as Phases 2-7 (spawn_blocking, store query, computation, assign to report field). This is covered by existing patterns (#882 Best-Effort, #726 SQL Aggregation) rather than requiring a dedicated procedure. The pattern composition is sufficient -- no new procedure needed.

### Consolidated Store Query Pattern (ADR-001)

ADR-001 follows the existing pattern documented in #704 (ADR-004 crt-013: Single StatusAggregates). The technique is the same: one method, one lock_conn(), multiple SQL queries, one return struct. The existing ADR + pattern entry cover this adequately. No new procedure.

### Testing Technique for Pure Computation Modules

The test structure (separate test module files, exhaustive boundary tests at every named constant, overflow guards) is documented in the new pattern #1042. No separate procedure needed.

## 3. ADR Status

All 4 ADRs were validated by successful implementation and all 3 gates passing first attempt.

| ADR | Unimatrix ID | Status | Notes |
|-----|-------------|--------|-------|
| ADR-001: Consolidated effectiveness query | #1038 | VALIDATED | Implementation improved on pseudocode SQL with subquery dedup (R-03 fix). Design decision correct. |
| ADR-002: NULL topic handling | #1039 | VALIDATED | Dual-layer defense (SQL + engine) implemented. Redundant but harmless. All NULL/empty tests pass. |
| ADR-003: Data window indicator | #1040 | VALIDATED | DataWindow constructed from flat store fields (not cross-crate import). Correct separation. |
| ADR-004: Configurable noisy trust sources | #1041 | VALIDATED | NOISY_TRUST_SOURCES constant with .contains() works as designed. Passed as parameter for test isolation. |

No ADRs flagged for supersession. No prior ADRs were superseded by crt-018.

## 4. Lessons

| ID | Title | Summary |
|----|-------|---------|
| #1044 | Risk-based test strategy predicted and caught COUNT DISTINCT bug | R-03 risk identified during design directed the store agent to the right area. Agent improved the fix beyond what pseudocode specified (subquery vs COUNT DISTINCT). Validates risk strategy as attention-direction tool, not prescriptive solution. |

### Additional Observations (Not Stored -- Feature-Specific)

- **Clean single-session delivery**: crt-018 was designed and delivered in a single conversation with zero rework. Contributing factors: (a) tight scope with no schema migration, (b) pure computation module enabled exhaustive unit testing without DB fixtures, (c) consolidated store method minimized integration surface, (d) risk strategy pre-identified the one bug that materialized.
- **Calibration binary vs weighted**: The spec-vs-architecture discrepancy (FR-04 weighted calibration vs architecture's bool type) was flagged at Gate 3a and accepted. This is a design trade-off, not a defect. Follow-on crt-018b could revisit if weighted calibration is needed.
