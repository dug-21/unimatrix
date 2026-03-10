# Scope Risk Assessment: nxs-010

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | DDL-before-migration ordering: `create_tables()` runs new table DDL that references v11 schema before migration completes on v10 databases. Historical failure pattern (Unimatrix #376). | High | High | Architect must verify init sequence: migrate v10->v11 FIRST, then create_tables with IF NOT EXISTS. Both migration and create_tables emit identical DDL — ensure no conflict or double-run side effects. |
| SR-02 | Schema version collision: CURRENT_SCHEMA_VERSION is already 10 (col-017). If col-017 and nxs-010 develop concurrently on separate branches, both may claim v11 or produce conflicting migration blocks. | High | Med | Serialize landing order. nxs-010 must merge strictly after col-017. CI should assert schema version monotonicity. |
| SR-03 | AUTOINCREMENT vs counter pattern divergence: All existing ID allocation uses named counters in the counters table. query_log introduces SQLite AUTOINCREMENT, creating two ID allocation patterns. Future features may inconsistently choose between them. | Low | Med | Document the decision boundary (append-only logs use AUTOINCREMENT; entity tables use counters). Architect should add an ADR. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Backfill quality depends on col-017 attribution completeness. If col-017 attribution is partial or buggy, topic_deliveries backfill produces inaccurate aggregates that downstream features (col-020, crt-018) will treat as ground truth. | Med | Med | Backfill should log count of attributed vs total sessions. Consider a re-backfill mechanism or make col-020 recompute from raw session data rather than trusting backfill aggregates. |
| SR-05 | query_log write in search pipeline introduces a coupling between storage (nxs) and server (vnc/col) layers. Scope says "fire-and-forget" but does not define failure behavior — silent drop? log warning? retry? | Med | High | Spec must define failure semantics explicitly. If the store is locked or write fails, search must not degrade. Match injection_log precedent exactly. |
| SR-06 | No GC policy for query_log is a declared non-goal, but ~6K rows/year estimate assumes current usage. If hook-driven search frequency increases (col-018 adds UserPromptSubmit dual-route), volume could be 3-5x higher. | Low | Low | Architect should size query_log for 30K rows/year and confirm SQLite index performance at that scale. Not blocking but worth a note in architecture. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Two search paths (UDS and MCP) must both write query_log with identical schema. Divergent field population (e.g., session_id availability differs between paths) could produce inconsistent data. | Med | Med | Architect should define a shared `QueryLogRecord` builder or constructor that both paths use, ensuring field parity. |
| SR-08 | col-017 dependency is critical-path: hook-side topic attribution must land and be validated before nxs-010 backfill is meaningful. If col-017 ships with bugs, nxs-010 backfill silently produces garbage. | High | Med | Gate nxs-010 delivery on col-017 integration test passing. Do not merge nxs-010 until col-017 attribution is validated on real session data. |

## Assumptions

- **Schema version 10 is stable** (SCOPE.md line 19, 41): Assumes col-017's v10 migration is merged and tested. If v10 migration has bugs, v10->v11 migration inherits them.
- **Backfill runs in single transaction** (SCOPE.md line 58, 176): Assumes ~500 sessions. If production databases have significantly more attributed sessions, transaction duration could cause lock contention during startup.
- **Fire-and-forget pattern is proven** (SCOPE.md line 66, 173): Assumes injection_log's fire-and-forget pattern has no latency impact. This is validated but query_log writes are larger (JSON arrays of entry IDs and scores).
- **AUTOINCREMENT is sufficient** (SCOPE.md line 74): Assumes no need for predictable or contiguous IDs in query_log. If col-021 export needs stable ordering, AUTOINCREMENT gaps after vacuum could surprise.

## Design Recommendations

- **SR-01, SR-02**: Architect should explicitly document the init sequence (migrate then create_tables) and add a test that opens a v10 database with nxs-010 code — verifying migration runs before DDL. Reference Unimatrix #375 procedure.
- **SR-05, SR-07**: Spec should define a `QueryLogRecord` struct and shared write function used by both UDS and MCP paths, with explicit error-drop semantics.
- **SR-04, SR-08**: Delivery should gate on col-017 validation. Backfill should be re-runnable (idempotent INSERT OR IGNORE is already proposed — confirm it handles counter updates correctly on re-run).
