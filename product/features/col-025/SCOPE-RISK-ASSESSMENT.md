# Scope Risk Assessment: col-025

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Schema migration cascade: v15→v16 ALTER TABLE requires updating all older migration test files that assert `schema_version` (entry #2933). Missing test updates will cause CI failures unrelated to the feature. | Med | High | Architect must identify every migration test file asserting schema_version ≤ 15 and include them in scope. |
| SR-02 | Unbounded goal text: SCOPE.md §Constraints states no truncation at the storage layer. A malformed or pathological caller providing a multi-megabyte goal string will store it in `cycle_events` and later load it into `SessionState` in-memory. | Med | Low | Architect should specify a max-byte guard at the tool handler layer (SCOPE.md already suggests this). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | `SubagentStart` precedence rule is new and not tested in isolation. SCOPE.md §Goals-5 and §Resolved-3 specify `prompt_snippet → current_goal → topic`. This is the only injection path where goal is *not* automatically handled by `derive_briefing_query` — it requires explicit branching. Missed or inverted precedence here degrades injection quality silently. | Med | Med | Spec writer must include a dedicated AC for the SubagentStart path when prompt_snippet is non-empty AND goal is set (goal must NOT win). |
| SR-04 | `sessions.keywords TEXT` column is explicitly excluded from this feature (SCOPE.md §Non-Goals). If delivery accidentally touches that column or the sessions table schema, it risks coupling two independently tracked cleanups. | Low | Low | Architect should note the columns-to-avoid boundary explicitly in ARCHITECTURE.md. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Session resume path (server restart) requires a DB read for goal that the start path does not. Entry #324 (lesson-learned: session gaps cause expensive re-reads) and entry #3027 (phase-snapshot pattern) show that state reconstruction on resume is a historically problematic path. If the indexed lookup (`cycle_events WHERE cycle_id = ? AND event_type = 'cycle_start'`) returns no row (e.g. cycle started before v16), `current_goal` must be `None` — the NULL-fallback must be tested explicitly. | High | Med | Architect must confirm the resume query handles pre-v16 NULL rows and document the fallback contract. |
| SR-06 | `derive_briefing_query` is shared between the MCP handler and the UDS handler. A change to `synthesize_from_session` affects both paths simultaneously. If the function signature or return semantics change, silent behavioral divergence between briefing-via-tool and briefing-via-hook is possible without dedicated tests for each path. | Med | Med | Spec writer must include ACs that exercise both the MCP briefing path and the UDS CompactPayload path independently. |

## Assumptions

- **SCOPE.md §Background / cycle_events schema**: Assumes current schema version is exactly v15 (col-024 delivered v14→v15). If any in-flight work has already bumped to v16, this feature owns a collision. SCOPE.md §Constraints states "no other schema changes are in-flight" — this must be verified at implementation start.
- **SCOPE.md §Background / SessionState**: Assumes `SessionState` and `SessionRegistry` are stable. If col-024 or any concurrent feature modifies these structs, the `current_goal` field addition may conflict.
- **SCOPE.md §Goals-4**: Assumes `synthesize_from_session` is a pure, sync function with no existing callers returning meaningful data. If any consumer of its current return value exists and relies on `None` semantics, returning `Some(goal)` is a behavioral change for those consumers.

## Design Recommendations

- **SR-05 (High)**: The resume-path DB read is the only async/fallible operation introduced by this feature. Architect should confirm error handling: if the `cycle_events` lookup fails (e.g., DB error), does the session registration proceed with `current_goal = None` or does it fail? Graceful degradation to `None` is consistent with SCOPE.md §Non-Goals (backward compatibility).
- **SR-01 (Med)**: Schema migration test cascade (entry #2933) is a known CI trap. Include test file audit in the delivery checklist.
- **SR-03 (Med)**: The SubagentStart injection precedence is the only hand-coded branching in this feature. All other paths are handled automatically by `derive_briefing_query`. Isolate this branch in a dedicated unit test.
