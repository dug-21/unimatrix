# Agent Report: crt-025-agent-2-spec

**Role**: Specification Writer
**Feature**: crt-025 — WA-1: Phase Signal + FEATURE_ENTRIES Tagging
**Output**: `product/features/crt-025/specification/SPECIFICATION.md`

## Deliverable

`/workspaces/unimatrix/product/features/crt-025/specification/SPECIFICATION.md`

## Key Decisions Made

1. **SR-05 resolved (cross-cycle comparison in scope)**: The product vision WA-1 section explicitly lists cross-cycle comparison as a deliverable. The spec includes it as FR-10 with the threshold rule: silently omit when fewer than 2 prior features have phase-tagged data.

2. **SR-06 addressed (canonical phase vocabulary)**: Added a dedicated canonical phase vocabulary table (`scope`, `design`, `implementation`, `testing`, `gate-review`) as a protocol-level constraint. The engine is not modified to enforce vocabulary membership; only format is enforced.

3. **`AnalyticsWrite::FeatureEntry` variant extended**: The spec mandates adding `phase: Option<String>` to the variant struct and capturing it at enqueue time (not drain-flush time). This is the resolution to SR-07.

4. **`PhaseNarrative` as new optional field on `RetrospectiveReport`**: The spec introduces this type to hold ordered events, rework detection, per-phase category counts, and cross-cycle comparison. It uses `#[serde(skip_serializing_if = "Option::is_none")]` for backward compatibility.

5. **`outcome` category: 7 categories post-removal**: `CategoryAllowlist::new()` drops from 8 to 7. All existing tests asserting `validate("outcome").is_ok()` become failures that the implementor must fix.

6. **AC numbering**: Kept original AC-01 through AC-15 from SCOPE.md and added AC-16 (hook path `phase-end` handling) and AC-17 (test coverage consolidation). All SCOPE AC-IDs are present.

## SR Handling

| Risk | Resolution |
|------|-----------|
| SR-01 | FR-05.5: `current_phase` mutation is synchronous, not queued behind drain |
| SR-02 | Delegated to architect per open question; seq monotonicity or advisory noted |
| SR-03 | FR-08.6: All `validate("outcome").is_ok()` tests must be updated |
| SR-04 | Confirmed: `sessions.keywords` has no consumers; C-04 leaves column in place |
| SR-05 | Resolved in scope: cross-cycle comparison is FR-10 |
| SR-06 | Canonical phase vocabulary table added to spec |
| SR-07 | FR-06.2: phase captured at enqueue time on `AnalyticsWrite::FeatureEntry` |
| SR-08 | FR-07.6: migration integration test explicitly required |

## Open Questions

None remaining. One architect decision required: SR-02 seq monotonicity enforcement strategy (per-`cycle_id` serialization vs. advisory `seq` with `timestamp` as tie-breaker). Documented as an open question in the spec's open-questions section.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for phase tagging, schema migration, analytics drain, session state, acceptance criteria patterns — found ADRs #1273, #681, #836, lesson-learned #981, pattern #2125, pattern #2987. No prior phase-tagging precedent; domain is new.
