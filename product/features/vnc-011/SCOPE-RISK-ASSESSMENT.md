# Scope Risk Assessment: vnc-011

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | RetrospectiveReport struct has 15+ fields accumulated across 6 features (col-002 through col-020b). Formatter must handle every Optional field being None for older/cached reports. | Med | High | Architect should enumerate all None-path combinations and ensure the formatter degrades gracefully for each. |
| SR-02 | k=3 random example selection is non-deterministic — same report produces different markdown on each call. Complicates testing and debugging. | Low | High | Consider seeded RNG or deterministic "first 3 by timestamp" for reproducibility. |
| SR-03 | Changing `evidence_limit` default from 3 to 0 is a behavioral change for `format: "json"` consumers. Any existing automation relying on evidence in JSON responses will silently lose data. | High | Med | Architect should assess whether default change applies only to markdown format or globally. SCOPE says globally — confirm intent. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | SCOPE excludes "actionability tagging" but PRODUCT-VISION.md lists it as a vnc-011 deliverable (`[actionable]/[expected]/[informational]`). Scope/vision mismatch may cause confusion in future planning. | Low | Med | Acknowledge the deferral explicitly in architecture. No action needed for this feature. |
| SR-05 | "No changes to RetrospectiveReport struct" constraint means all grouping/collapsing logic is purely in the formatter. If the struct later needs formatter-friendly fields (e.g., pre-grouped findings), this becomes a refactor. | Med | Low | Accept for MVP. Formatter-only approach is the right first step. |
| SR-06 | Zero-activity phase suppression heuristic (`tool_call_count <= 1 AND duration_secs == 0`) may hide phases with a single meaningful tool call and zero duration (e.g., fast single-call phases). | Low | Low | Document the heuristic clearly; it can be tuned post-ship. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | col-020b is "in progress" per SCOPE dependencies. If `FeatureKnowledgeReuse` type shape changes before vnc-011 ships, the formatter breaks. | Med | Med | Formatter should consume col-020b types via the existing serde structs, not assume field stability beyond what is already merged. |
| SR-08 | Formatter lives in `unimatrix-server` but consumes types from `unimatrix-observe`. Adding a format parameter to `RetrospectiveParams` (server-side) while report generation is observe-side creates a cross-crate concern if format ever needs to influence report building. | Low | Low | Keep formatter strictly server-side as SCOPE intends. Note the boundary for future reference. |

## Assumptions

- **SCOPE "Key Constraint"**: All logic is formatter-only, report pipeline untouched. If any collapse logic requires data not currently in the struct (e.g., per-session finding attribution), the formatter-only constraint breaks.
- **SCOPE "Dependencies"**: col-020 is COMPLETE. If `session_summaries` field shape was finalized in col-020, this is safe. If col-020b changes the session summary types, SR-07 applies.
- **SCOPE "Success Criteria #10"**: 80% token reduction target assumes typical reports resemble the nxs-010 example (76KB, 3% signal). Smaller reports with fewer Normal baselines may see less reduction.

## Design Recommendations

- **SR-03 is the highest-priority risk.** Changing `evidence_limit` default globally is a breaking behavioral change for JSON consumers. Architect should consider making the default change format-dependent (0 for markdown, 3 for json) or documenting it as an intentional breaking change.
- **SR-01 requires exhaustive None-handling.** The formatter must be tested against a minimal report (all Optional fields None) and a maximal report (all populated). The type has grown through 6 features — enumerate the combinations.
- **SR-07 can be mitigated by waiting for col-020b merge** before finalizing the formatter's knowledge reuse section, or by coding defensively against Option::None for that section.
