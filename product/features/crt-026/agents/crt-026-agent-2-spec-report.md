# Agent Report: crt-026-agent-2-spec

**Role**: Specification Writer
**Feature**: crt-026 — WA-2 Session Context Enrichment
**Output**: `product/features/crt-026/specification/SPECIFICATION.md`

## Decisions Made

**D-1: AC-07 dropped.**
SCOPE.md AC-07 (explicit phase boost behavior) conflicts with OQ-03 resolved (`w_phase_explicit=0.0`
in crt-026). As directed by SR-04 in the risk assessment, AC-07 is explicitly excluded from the
specification and must not appear in the ACCEPTANCE-MAP. The spec documents why: the explicit phase
term is reserved at 0.0 as a W3-1 placeholder and the `phase_category_weight` mapping is not
implemented in this feature.

**D-2: `phase_explicit_norm: f64` added to `FusedScoreInputs`.**
Although the explicit boost is always `0.0` in crt-026, the field and its corresponding
`FusionWeights.w_phase_explicit` must be declared so `compute_fused_score` is the complete
W3-1 feature vector surface. An always-zero field at initialization is the correct W3-1 cold-start
pattern. Architect may choose to omit `phase_explicit_norm` from `FusedScoreInputs` and simply
multiply `w_phase_explicit * 0.0` inline — either is acceptable; the spec lists the field as the
cleaner W3-1 interface.

**D-3: `ServiceSearchParams` gains two fields, not one.**
The spawn prompt specified `session_id: Option<String>` only. The resolved OQ-02 decision (pre-resolve
histogram in handler, pass through `ServiceSearchParams`) requires a second field `category_histogram:
Option<HashMap<String, u32>>`. The spec defines both; this is the direct implementation of the
resolved decision.

**D-4: `p(category) = count(category) / total_stores` formula confirmed.**
The boost formula is the standard empirical probability. When the histogram is empty (total_stores = 0),
all `phase_histogram_norm` values are `0.0` — cold-start safety satisfied without a special case
(division by zero is guarded by the `None` / empty histogram check before the loop).

**D-5: Application order clarified.**
SR-09 raised the question of `status_penalty` ordering. The spec mandates
`final_score = compute_fused_score(...) * status_penalty`, consistent with OQ-01 (boost inside
`compute_fused_score`). This is already the current code structure; the spec makes it explicit as
a constraint (C-06).

## Open Questions Forwarded to Architect

- OQ-A: `InferenceConfig::validate()` acceptance of `sum=0.955`; existing test impact
- OQ-B: UDS `HookRequest::ContextSearch` session_id field name and sanitization pre-condition
- OQ-C: WA-4a forward-compatibility of pre-resolution pattern vs. `Arc<SessionRegistry>` on `SearchService`
- OQ-D: Confirmation that `status_penalty` is applied only AFTER `compute_fused_score` returns

## Self-Check

- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through AC-14; AC-07 explicitly dropped with rationale)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (lock hold, max boost delta, budget)
- [x] Domain Models section defines key terms
- [x] NOT in scope section is explicit
- [x] Output file is in `product/features/crt-026/specification/` only
- [x] No placeholder or TBD sections — unknowns flagged as open questions for architect
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

Queried: `/uni-query-patterns` for session context ranking, ServiceSearchParams, FusionWeights,
affinity boost architecture — found entries #3157 (pre-resolution pattern, OQ-02 resolved) and
#3156 (boost inside `compute_fused_score`, OQ-01 resolved). Both confirmed the resolved decisions
in SCOPE.md. No conflicting conventions found. No new patterns generated — spec decisions are
feature-specific and not yet generalizable.
