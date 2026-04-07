# Agent Report: crt-050-agent-2-spec

**Role**: Specification Writer
**Feature**: crt-050 — Phase-Conditioned Category Affinity (Explicit Read Rebuild)

## Output

`/workspaces/unimatrix/product/features/crt-050/specification/SPECIFICATION.md`

## Key Decisions Made

### D-01: AC-SV-01 added as blocking gate on SR-01
SCOPE.md listed SR-01 (double-encoding) as a constraint. Code inspection of merged crt-049 confirmed the risk is real and present: `listener.rs` lines 2693–2696 serialize `tool_input` as `serde_json::to_string(v)` (producing a string, not an object), and `knowledge_reuse.rs` lines 76–103 document this explicitly as the hook-path storage form. A pure-SQL `json_extract(input, '$.id')` approach silently returns NULL for all hook-path rows. The spec adds AC-SV-01: architect must choose Option A (two-phase extraction) or Option B (storage normalization) and document it as an ADR before implementation. This is not a suggestion — it is a merge-blocking gate.

### D-02: AC-14 added for minimum coverage threshold (SR-03)
SCOPE-RISK-ASSESSMENT SR-03 flagged that the MRR gate alone is insufficient when explicit-read signal is sparse. The spec adds FR-17 (configurable `min_phase_session_coverage`) and AC-14 (gate on distinct `(phase, session_id)` count), ensuring `use_fallback = true` is set when data volume is too sparse to produce meaningful phase weights. This prevents misleading sparse-data weights from degrading scoring.

### D-03: AC-15 added for NULL feature_cycle degradation (SR-05)
SCOPE-RISK-ASSESSMENT SR-05 noted that sessions predating col-022 have `feature_cycle = NULL`, causing the outcome weighting join to silently produce zero rows. The spec adds AC-15 and FR-10, explicitly requiring weight 1.0 (not error, not fallback escalation) for those sessions.

### D-04: phase_category_weights() aggregation is normalized bucket size
SCOPE.md stated the method returns "normalized bucket size (probability distribution)". The spec formalizes this: each value = `bucket_weighted_freq_sum / total_weighted_freq_for_phase`, summing to 1.0 per phase. This matches the stated goal of answering "given phase P, how likely is category C to be useful?"

### D-05: OQ-02 promoted from silence to tracked open question
SR-06 (infer_gate_result module boundary) was a "recommendation" in SCOPE-RISK-ASSESSMENT. The spec promotes it to a named open question (OQ-02) that the architect must resolve before implementation, since duplicate implementations of outcome substring matching create vocabulary drift risk.

### D-06: 17 functional requirements from 13 acceptance criteria
The 13 SCOPE.md ACs were expanded to 17 FRs + AC-SV-01 + AC-14 + AC-15, capturing the implementation constraints (FR-04 through FR-07, FR-10, FR-16, FR-17) that were implicit in SCOPE.md constraints and risk assessments but not represented as testable ACs.

## Open Questions (forwarded to architect)

- OQ-01: SR-01 storage contract — Option A (two-phase SQL+Rust) vs Option B (storage normalization)?
- OQ-02: SR-06 — extract `infer_gate_result()` to shared module or inline with reference comment?
- OQ-03: Default value for `min_phase_session_coverage` — spec suggests 10, architect decides.
- OQ-04: crt-036 diagnostic applicability — does K-cycle retention alignment check apply to `observations`?

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — entry #4222 confirmed the three non-negotiable SQL differences (ms epoch, tool prefix, CAST); entry #3917 provided the crt-036 diagnostic context; entries #3677 and #3699 confirmed cold-start contracts. All four applied.
- SR-01 storage contract status confirmed by direct inspection of `listener.rs` and `knowledge_reuse.rs` at merged crt-049 HEAD.
