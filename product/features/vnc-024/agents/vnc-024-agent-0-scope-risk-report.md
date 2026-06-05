# Agent Report: vnc-024-agent-0-scope-risk

**Mode:** scope-risk
**Output:** product/features/vnc-024/SCOPE-RISK-ASSESSMENT.md (41 lines)

## Summary
9 scope-level risks: 4 High, 4 Med, 1 Low. F1 is plumbing but locks a contract F2–F5 inherit; the dominant risk class is *silent infidelity* — codegen/round-trip/dispatch that looks correct but isn't.

| Severity | Count | IDs |
|----------|-------|-----|
| High | 4 | SR-01, SR-02, SR-04, SR-07 |
| Med | 4 | SR-03, SR-05, SR-08, SR-09 |
| Low | 1 | SR-06 |

## Top 3 for architect/spec attention
1. **SR-07 — secrets-to-disk hole (principle 8).** The `listener.rs:849` generic-observation fall-through persists raw `transcript_delta` bytes (may contain secrets) to durable storage. Sequencing is not a safety property. Matches recurring silent-fallthrough failure (#4311). Accept-and-drop guard is REQUIRED; its negative test should be a gate prerequisite, covering HTTP and UDS.
2. **SR-01/SR-02 — ts-rs fidelity.** Codegen may model tagged-enum/flatten structurally yet mis-handle serde behavior (None-vs-omission). #885/#3557: this is the single most-omitted test category and a prior gate-failure cause. Round-trip fixtures must be the contract authority, not the generated `.ts`.
3. **SR-04 — ship a contract F2/#670 inherit without re-negotiation.** If the wire surface omits a delta/retention field downstream needs, F2 re-opens the contract — defeating F1. Freeze emitted bindings + retention enum against the ass-069 Q2/Q7 field list before merge.

## Knowledge Stewardship
- Queried: context_search for ts-rs/serde drift, risk patterns, secrets-to-disk — found #3557 (dual-direction serde round-trip pattern), #885 (serde test coverage gate-failure lesson), #4311 (silent-fallthrough gate-prerequisite ordering), #3953 (tagged-union type authority). All directly informed SR-01/02/07. Strong, applicable history.
- Stored: nothing novel to store — the recurring patterns this feature triggers (#885, #3557, #4311) already exist and cover the serde/silent-fallthrough risk class precisely. No cross-feature (2+) pattern visible here that isn't already captured.
