# Agent Report: crt-043-agent-2-spec

## Output

`product/features/crt-043/specification/SPECIFICATION.md`

## Acceptance Criteria Count

14 ACs (AC-01 through AC-14).

SCOPE.md had 12 ACs. This specification expands them:

| SCOPE.md AC | SPECIFICATION.md AC | Change |
|-------------|---------------------|--------|
| AC-01 | AC-01 | Sharpened: specifies verification via `pragma_table_info` |
| AC-02 | AC-02 | Sharpened: added stub/mock verification method |
| AC-03 | AC-03 | Sharpened: added dimension assertion (384) and `decode_goal_embedding` round-trip |
| AC-04 | AC-04 | Split into two sub-cases: (a) unavailable service, (b) absent/empty goal |
| AC-05 | AC-05 | Sharpened: `cargo metadata --no-deps` as verification method |
| AC-06 | AC-06 | Unchanged in intent |
| AC-07 | AC-07 | Sharpened: `pragma_table_info` verification |
| AC-08 | AC-08 | Sharpened: compile-without-warnings requirement added |
| AC-09 | AC-09 | Sharpened: read-back assertion added |
| AC-10 | AC-10 | Split into two sub-cases: active phase and no active cycle |
| AC-11 | AC-11 | Sharpened: both columns checked; version not re-bumped |
| AC-12 | AC-12 | Sharpened: 7 explicit scenarios enumerated |
| (new) | AC-13 | INSERT-before-UPDATE ordering guarantee (SR-01 → testable criterion) |
| (new) | AC-14 | decode_goal_embedding helper ships with write path (SR-02 → testable criterion) |

## Key Decisions Made

1. **AC-13 added** to surface SR-01 (INSERT/UPDATE race) as a concrete, testable
   acceptance criterion. SCOPE.md left this as an architectural open question without
   an AC. The spec requires an architect ADR and an integration test or code-review
   assertion before the PR can be considered complete.

2. **AC-14 added** to enforce SR-02 (paired decode helper). SCOPE.md mentioned it in
   risk assessment but did not make it a blocking AC. Promoting it here ensures Group 6
   read sites have the decoder available without implementing it independently.

3. **FR-B-09 distinguished from FR-B-10**: absent/empty goal (no warn, no spawn) vs.
   embed service unavailable (warn emitted, no spawn) are separate functional
   requirements with different observable behaviors.

4. **FR-C-06 and C-05**: phase values stored as-is. SR-05 in SCOPE-RISK-ASSESSMENT.md
   recommended normalization at write time, but the spawn prompt says "no allowlist
   enforcement" and phase is a structured system value — callers should provide canonical
   values. Normalization at query time (Group 6) is the correct boundary. The spec
   documents canonical phase values in Domain Models as advisory.

5. **OQ-03 surfaced as explicit open question** (composite index on `topic_signal, phase`).
   FR-C-07 makes the evaluation mandatory before PR opens, closing SR-06.

## Scope Gaps Found

None. All 12 SCOPE.md ACs are covered. Two new ACs added from SCOPE-RISK-ASSESSMENT.md
risks (SR-01, SR-02) that were missing from the original AC list.

Item A (dropped — already exists) is excluded per SCOPE.md; the NOT in scope section
makes this explicit to prevent scope creep.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entries #3374, #3396, #1277, #2998,
  #2999, #3001 were directly relevant and incorporated into requirements.
  - #3374: enrich_topic_signal pre-capture pattern → FR-C-03 timing contract
  - #3396: col-025 cycle_events goal ADR → migration lineage, FR-M-03 pattern
  - #1277: col-022 migration → pragma_table_info + NULL-default convention
  - #2998/#3001: crt-025 phase snapshot ADRs → SessionState.current_phase confirmed
