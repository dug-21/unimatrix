# crt-052-agent-2-spec — Specification Writer Report

## Deliverable
`product/features/crt-052/specification/SPECIFICATION.md`

## Summary
Refined SCOPE AC-01..AC-13 into 15 FRs, 9 NFRs, 13 ACs (+2 supplementary verification criteria),
14 constraints, and 6 domain models, each AC carrying an explicit verification method. SR-01..SR-09
mapped into constraints, domain-model properties, and AC verifications.

## Key decisions / interpretations
- Snapshot seam typed as `Vec<SessionTranscriptSnapshot>` (owned bytes + elided_bytes/hole_info/
  high_water per session) — single shape consumed by both candidate selection and #700 (Constraint 4).
- Candidate selection specified as a SEPARATE consumer of the seam, not baked in (per decided note).
- `topic_source` specified as soft ordering preference only, never a hard filter (SR-06 / OQ-1).
- AC-03 independence made verifiable: committed provenance header asserting anchors-before-port OR
  different-author, regex not consulted during labeling.
- AC-11 specified as a hard named test (`continuity_simulated_lifecycle`) with the full per-turn-drain
  sequence and ≥3 drain cycles — flagged as the only pre-merge primary-path proof.
- AC-06 content-leak verification specified as extending the vnc-025 AC-12 grep/test gate to new paths.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — #4742 (named/take-shaped seam), #4750 (four success
  returns), #3793/#3794/#3795 (crt-033 memoization persist = AC-06 trap), RetrospectiveReport
  optional-field precedent. Read-only tier; no storage.
