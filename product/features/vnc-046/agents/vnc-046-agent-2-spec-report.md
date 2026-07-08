# Agent Report — vnc-046-agent-2-spec

**Role:** Specification writer · **Deliverable:** `specification/SPECIFICATION.md`

## Output
- `product/features/vnc-046/specification/SPECIFICATION.md`

## Key decisions
- Preserved SCOPE AC-IDs (AC-01…AC-10) so they trace downstream; mapped each behavioral invariant
  (INV-T1/T2/T3, INV-K1/K2, INV-C1/C2) onto AC-01…AC-05.
- Encoded every behavioral invariant AC as **bidirectional** per SR-06/#5348: two-slug (N≥2) fixture, each slug
  driven through its OWN route, assert present-in-own AND absent-in-other in BOTH directions.
- Took P3 (5 config fields) IN-SCOPE and P2 confirmed in-scope per the uni-zero scope review resolutions of OQ-1/OQ-2.
- OQ-3: split the P3 config fields into behaviorally-observable (`signal_class_names`→`signal_class_counts`,
  observation categories→status, retention→purge) vs. no-public-surface (`store_config`, `inference_config`) — the
  latter two carry a DOCUMENTED white-box exception (AC-08 boot assertion + wiring-pin), never silent omission (SR-05).
- Made NFR-2 explicit: no `debug_assert!`-only guard for any isolation direction (compiled out of release).
- Reflected #800 as the fixture to EXTEND (not fork) and #925 as an architect subsume-vs-defense-in-depth call in
  the ADR (AC-10).

## Open questions for architect (non-blocking to spec)
- Confirm crt-056 per-slug `ServiceLayer` is config-driven (else P2 is deeper than resolve-per-request).
- Reconcile #925 subsume-vs-defense-in-depth in the ADR.
- Confirm #800 fixture owner before building INV-C fixtures.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (vnc-046 per-slug observe-path isolation, 20 hits) — #4950 resolve_store
  seam, #5082 vnc-038 per-slug observe (store-only, the gap completed here), #5217 per-slug config classification,
  #5519/#5533/#5579/#5594 personal-cloud isolation goal/capabilities. Plus GH #930 (5 comments), SCOPE.md,
  SCOPE-RISK-ASSESSMENT.md, uni-zero scope review.
- Stored: nothing — read-only tier; governing invariant already #5629, bidirectional-test lesson already #5348,
  defect specifics stay on GH #930.
