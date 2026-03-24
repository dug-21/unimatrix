# Agent Report: col-025-agent-2-spec

## Output

- `/workspaces/unimatrix/product/features/col-025/specification/SPECIFICATION.md`

## Key Decisions Made

1. **AC-12 added for SR-03**: The risk assessment flagged that the SubagentStart precedence inversion (prompt_snippet non-empty + goal set → prompt_snippet must win) was not covered by the SCOPE.md ACs. AC-12 closes this gap with an explicit, isolated test requirement.

2. **AC-13 added for SR-02**: SCOPE.md §Constraints noted a max-byte guard was possible at the tool layer but left it to the architect. Given SR-02 (Med severity, unbounded string in SessionState), FR-03 mandates the guard (2 048 bytes) and AC-13 verifies rejection behaviour. The exact limit is flagged as OQ-03 for the architect to confirm.

3. **AC-14 and AC-15 added for SR-05**: SR-05 (High severity) identified two distinct null-fallback cases on the resume path: no matching row vs. DB error. Both require `current_goal = None` + successful registration. Split into two separate ACs for isolated test coverage.

4. **AC-16 added for SR-01**: Pattern #2933 (schema version cascade) is a known CI trap. FR-06 + AC-16 make the test-file audit a delivery requirement, not a post-hoc fix.

5. **FR-03 (goal byte guard) added**: Not in SCOPE.md functional requirements but implied by §Constraints. Elevated to a functional requirement with a defined limit to ensure the architect picks up the implementation detail.

6. **synthesize_from_session pure-function constraint hardened**: SCOPE.md §Constraints mentions it; NFR-04 makes it a non-functional requirement with explicit rationale (both MCP and UDS hot paths, O(1) cost). This protects against a future refactor that inadvertently adds I/O.

## Open Questions

- **OQ-01**: Should `sessions.keywords TEXT` cleanup be batched with v16 migration? (SR-04 scope boundary risk — for architect.)
- **OQ-02**: Logging severity for non-fatal resume-path DB error — confirm convention vs. existing session-reconstruction error paths. (For architect.)
- **OQ-03**: Is 2 048 bytes the correct max-byte limit for goal, or does a project-standard constant exist? Should a named constant `MAX_GOAL_BYTES` be introduced? (For architect.)

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for schema migration (cycle_events, ALTER TABLE, idempotency, version cascade) — found patterns #1264, #2933, #3000, #370/#681; all incorporated.
- Queried: `/uni-query-patterns` for SessionState / derive_briefing_query / session resume — found #3325, #3210, #3297; incorporated into domain model and dependency tables.
- Queried: `/uni-query-patterns` for SubagentStart / UDS injection / prompt_snippet — found #3230, #3251, #3243, #3324; incorporated into FR-09 and domain model.
- Queried: `/uni-query-patterns` for acceptance criteria / testable requirements conventions — found #133, #138; applied throughout.
