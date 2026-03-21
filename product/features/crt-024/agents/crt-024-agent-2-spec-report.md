# Agent Report: crt-024-agent-2-spec

## Output

- `/workspaces/unimatrix/product/features/crt-024/specification/SPECIFICATION.md`

## Key Decisions Made

**SR-02 Resolved**: The product vision's four-term formula is explicitly declared illustrative.
The specification canonicalizes the six-term formula as the implementation target. This is called
out in FR-01 with a direct reference to SR-02 so downstream agents do not treat the vision formula
as authoritative.

**SR-03 Resolved**: AC-06 and FR-07 both specify the NLI re-normalization denominator as
`w_sim + w_conf + w_coac + w_util + w_prov` — all five non-NLI weights. The SCOPE.md AC-06
draft had a three-term denominator; the risk assessment identified this; the specification uses
the correct five-term denominator throughout.

**Utility delta negative range (OQ-03)**: Flagged as an open question for the architect rather
than hardcoding a clamp decision. The specification requires clamping to [0, 1] in FR-05 as the
default behavior but explicitly calls for the architect to decide and document the policy, because
treating the penalty side as zero loses signal.

**FusedSignals and ScoreWeights as named domain concepts**: These are specified in the Domain
Models section with their field definitions, establishing the vocabulary for architect, pseudocode,
and tester agents. `ScoreWeights::effective(nli_available: bool)` is specified as the re-normalization
entry point, which also answers how the re-normalization is scoped (derived value, original not
mutated).

**Step 6c as explicit pipeline step**: The boost_map prefetch is named as its own pipeline step
to make SR-07 (data-flow sequencing) a named constraint rather than an implicit assumption. This
should prevent the architect from treating it as a performance optimization they can defer.

**WA-2 extension contract**: Elevated to FR-09 (functional requirement) and OQ-04 (open question),
so the architect cannot treat it as a post-implementation concern. SR-04 requires this contract
be documented in the ADR before implementation.

## Open Questions for Architect

- OQ-01: Six default weight values — W3-1 initialization point, must be numerically verified.
- OQ-02: apply_nli_sort fate — remove (migrate tests) or retain as helper.
- OQ-03: utility_delta negative range — clamp, separate multiplier, or separate signal.
- OQ-04: WA-2 extension contract — variable-arity accumulator or fixed-field + re-validate.
- OQ-05: boost_map prefetch async sequencing with NLI batch scoring — confirm no race condition.

## Self-Check

- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through AC-14, all present)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (no latency regression, [0,1] range, determinism)
- [x] Domain Models section defines FusedSignals, ScoreWeights, BoostMap, StatusPenalty, NLI Absent vs. Disabled
- [x] NOT in scope section is explicit (14 exclusions)
- [x] Output file is in `product/features/crt-024/specification/` only
- [x] No placeholder or TBD sections — unknowns are flagged as open questions OQ-01 through OQ-05
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "ranking scoring pipeline inference weights similarity confidence normalization" -- Entry #2964 (signal fusion pattern), #2701 (ADR-002 crt-023 NLI primary signal), #2298 (config semantic divergence), #751 (updating golden regression values), #179 (ADR-003 lambda dimension re-normalization pattern), #485 (ADR-005 status penalty constants)
