# Agent Report: col-020-agent-2-spec

## Status: COMPLETE

## Artifacts Produced
- `/workspaces/unimatrix/product/features/col-020/specification/SPECIFICATION.md`

## Key Decisions

1. **AttributionCoverage struct added (FR-05.6)** -- Addresses SR-07 directly. Reports `attributed_sessions` vs `total_sessions` so consumers can gauge metric trustworthiness. Not in SCOPE.md but required by the risk assessment.

2. **Idempotent counter updates via absolute replacement (FR-06.2)** -- SCOPE.md proposed `update_topic_delivery_counters()` (additive). Spec mandates absolute setter per SR-09 recommendation to prevent double-counting on repeated retrospective runs.

3. **Concurrent session tiebreaker (FR-04.2)** -- SCOPE.md noted concurrent sessions as a constraint but did not specify behavior. Spec defines lexicographic session_id tiebreaker for identical timestamps, addressing SR-06.

4. **Server-side knowledge reuse confirmed (FR-02.4)** -- Adopted Option B from SCOPE.md. Flagged that an ADR should document this architectural exception per SR-08.

5. **Rework patterns are case-sensitive substring matches (FR-03.1)** -- Per resolved question #1 in SCOPE.md. Documented the exact patterns (`result:rework`, `result:failed`) per SR-03 recommendation.

6. **File path extraction failure mode (FR-01.4)** -- Per SR-04, malformed/missing paths are silently skipped with debug logging, not errors.

## Self-Check Results
- [x] All 16 AC-IDs from SCOPE.md present (AC-01 through AC-16)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets where possible
- [x] Domain Models section defines key terms (6 domain models)
- [x] NOT in scope section is explicit (8 exclusions matching SCOPE.md non-goals)
- [x] Output file in specification/ directory only
- [x] No placeholder or TBD sections

## Open Questions

1. **ADR for server-side computation (SR-08)**: Should the architect create a formal ADR documenting that knowledge reuse computation lives in unimatrix-server as a deliberate exception, or should this be treated as a pattern shift where future cross-table analytics also live server-side?

2. **Store API for absolute counter set**: The existing `update_topic_delivery_counters()` is additive. Does the architect prefer a new method (`set_topic_delivery_counters`) or modifying the existing method signature? The spec assumes a new method (FR-08.4) to avoid breaking existing callers.

3. **Attribution coverage for cached reports**: When returning a cached report (`is_cached: true`), should `attribution_coverage` be persisted from the last computation or remain None? The spec currently says None for cached reports (FR-07.4). If persistence is desired, it needs storage.
