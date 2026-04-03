# Agent Report: crt-042-agent-2-spec

## Task
Produce SPECIFICATION.md for crt-042 (PPR Expander).

## Output
`product/features/crt-042/specification/SPECIFICATION.md`

## AC Count
26 acceptance criteria: AC-00 through AC-25.
- AC-00: SR-03 prerequisite gate (blocking — S1/S2 directionality check before Phase 0 impl)
- AC-01: Flag-off bit-identical regression safety
- AC-02 to AC-12: Phase 0 invocation, behavioral traversal correctness, seed exclusion, cap, degenerate cases
- AC-13 to AC-15: Quarantine safety and embedding skip
- AC-16: Traversal boundary (edges_of_type only)
- AC-17 to AC-21: Config backward-compat and validation (always enforce, not flag-gated)
- AC-22 to AC-23: Eval profile and eval gate
- AC-24: Latency instrumentation (debug! trace, mandatory)
- AC-25: Cross-category regression test (core behavioral guarantee)

## Key Decisions Made

1. **All traversal ACs are behavioral** — no AC references Direction:: constants. Each AC
   describes what entry surfaces given a specific graph topology (seed B, edge B→A → A appears).
   Cites entry #3754 explicitly in the spec.

2. **AC-00 is a blocking prerequisite gate** — SR-03 upgraded from "delivery check" to a
   hard gate: no Phase 0 code may be written until S1/S2 directionality is confirmed. If
   single-direction, a crt-041 write-site back-fill issue must be filed and resolved first.

3. **Config validation always enforces ranges** — expansion_depth [1,10] and
   max_expansion_candidates [1,1000] are validated at server start regardless of
   ppr_expander_enabled. Reverses the NLI "validate only when enabled" anti-pattern.

4. **NFR-01 latency instrumentation is mandatory** — the debug! trace with wall-clock ms is
   a prerequisite for enabling the flag by default, not optional instrumentation. Wired as
   NFR-01 and AC-24.

5. **Combined ceiling documented** — Phase 0 max 200 + Phase 5 max 50 = 250 entries beyond
   k=20. Documented in NFR-08 and the domain model. SR-04 resolved by explicit documentation;
   OQ-03 asks architect whether a hard pool cap should be enforced in search.rs.

6. **graph_expand quarantine contract is on the caller** — FR-06 makes explicit that
   graph_expand is pure and quarantine-free; callers own the check. SR-07 mitigation.

7. **Four open questions surfaced for architect** — OQ-01 (SR-03 directionality), OQ-02
   (SR-01 O(1) embedding lookup path), OQ-03 (SR-04 combined pool cap), OQ-04 (SR-05 eval
   gate failure owner).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — 17 ranked entries returned. Key entries
  applied: #3754 (direction semantics lesson — all traversal ACs written behaviorally), #3627
  (edges_of_type sole traversal boundary — FR-04, AC-16), #3817 (InferenceConfig dual-site
  atomic change — FR-07), #2730/#4044 (hidden test sites pattern — FR-07), #3769
  (InferenceConfig procedure — FR-07). Server responsive, no unavailable results.
- Searched: "InferenceConfig serde default config validation", "quarantine check
  SecurityGateway is_quarantined", "graph traversal edges_of_type SR-01". All returned
  relevant entries that were incorporated into requirements.
