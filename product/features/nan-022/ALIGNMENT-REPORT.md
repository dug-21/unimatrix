# Alignment Report: nan-022

> Reviewed: 2026-06-24
> Artifacts reviewed:
>   - product/features/nan-022/architecture/ARCHITECTURE.md
>   - product/features/nan-022/specification/SPECIFICATION.md
>   - product/features/nan-022/RISK-TEST-STRATEGY.md
> Scope source: product/features/nan-022/SCOPE.md + SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goal #4946 (personal-cloud); capability C0 #5304

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly serves goal #4946 "measured, not asserted" cloud-equals-local fidelity; is C0's (#5304) own-`done_when` proof artifact. |
| Milestone Fit | PASS | Vinculum/Nanoprobes parity proof for an already-designed goal; no future-milestone capability built. Test-only, no new server behavior. |
| Scope Gaps | PASS | All 12 SCOPE ACs and 6 dimensions traced into spec FRs and risk register; none dropped. |
| Scope Additions | PASS | Net-new constructs (outcome-class model, drift guard, transport-health preflight) are scope-mandated SR mitigations, not surface expansion. |
| Architecture Consistency | PASS | ARCHITECTURE → SPECIFICATION → RISK-TEST-STRATEGY are mutually consistent; ADRs, FRs, and risks cross-reference cleanly. |
| Risk Completeness | PASS | All 8 scope SRs traced to architecture risks and test scenarios; false-GREEN paths correctly elevated to High severity. |
| C0 Flip Bar (six vs three) | PASS | RESOLVED (human-confirmed 2026-06-25): all six dimensions block. The corrected C0 (#5304) `done_when` makes parity the total bar; the dimension list grows with the pipeline and never narrows the bar; any unreachable dimension is a human-signed documented exception. Design default `blocks_c0_proof=True` for all six is correct and aligned. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE AC-01..AC-12 maps to a spec FR/NFR (SPEC §6 traceability table) and a risk scenario. |
| Addition | Outcome-class model (PARITY-PASS/FAIL/INFRA-ERROR/INTRA-TRANSPORT-NONDETERMINISM) | Not literal in SCOPE Goals, but mandated by SCOPE-RISK SR-02/SR-04 ("define THREE outcome classes per dimension") and design rec #1/#2. In-scope. |
| Addition | Cross-dimension drift guard + single `FORBIDDEN_SEED_SITES` | Mandated by SR-05 ("single-source the contract... convention is not a guard"). In-scope. |
| Addition | Transport-health preflight (K5) for half-open hangs | Mandated by SR-02 ("design a per-leg transport-health preflight + bounded deadline"). In-scope. The #839 half-open class is now CLOSED (commit 5b6badad / PR #842); K5 is retained as defense-in-depth, not a gating dependency. |
| Simplification | (none) | No dimension simplified away; PreCompact (D5) explicitly kept in scope with a documented host-side call-out per OQ-3 fixed disposition rather than dropped. |

## Variances Requiring Approval

None at FAIL/VARIANCE level. The one item formerly flagged WARN (C0 flip bar) is now RESOLVED; no rework required, no approval blocks design.

**WARN-1 — C0 flip bar: six dimensions vs three `done_when` pillars — RESOLVED (human-confirmed 2026-06-25)**
1. **What**: The corrected C0 (#5304) `done_when` settles the question: "Parity is the bar; it is simple and total… the dimension list is the present expression of the parity bar and grows with the pipeline; it does not narrow the bar," with the disposition that any unreachable dimension is a human-signed documented exception, never silently excluded. #837 proves six dimensions (retrieval, behavioral, analytics/learning, proactive delivery, PreCompact, per-slug isolation); all six block the flip.
2. **Why it matters**: Confirms nan-022's full matrix passing IS the strict C0 bar. The design default `blocks_c0_proof=True` for all six is correct and aligned, with the documented-exception escape valve (e.g. the D5 PreCompact host-side gap) for a legitimately unreachable dimension.
3. **Resolution**: Accept as designed — confirmed, not a pending coin-flip. The registry `blocks_c0_proof` flag keeps any future re-disposition a data change (no code change). The human confirms before C0 is flipped (which this feature explicitly does NOT do, AC-12).

## Detailed Findings

### Vision Alignment (PASS)
Goal #4946 (personal-cloud) success criteria state full intelligence-pipeline fidelity "same over HTTPS as over local UDS" — and the product vision's framing is explicitly "measured, not asserted" (SCOPE problem statement, quoting C0). Capability C0 #5304 is unambiguous: "children all green does NOT auto-prove the rollup — it needs its own behavioral parity measurement to flip proven... a measurement feature with no blocking constituent." nan-022 IS that measurement feature. It advances the personal-cloud goal directly and respects architectural principle #3 (capability checks after identity resolution across UDS/HTTP) by proving behavioral parity across exactly those transports. No vision principle is contradicted; the test-only constraint (NFR-1) protects the "single binary server, zero required infra" principle by adding no production code.

### Milestone Fit (PASS)
The feature builds nothing for a future milestone. It is a proof artifact for an already-shipped serving arc (vnc-034 Waves 1–2, C5/C10/C11 all proven per #5304). It measures shipped behavior and explicitly does NOT invent new server behavior (SCOPE Non-Goals; SPEC §8). The "Does not broaden C0's surface beyond the six named dimensions" constraint enforces milestone discipline at the scope boundary. No premature capability.

### Architecture Review (PASS)
ARCHITECTURE generalizes the nan-021 single-`MetricVector` gate into a dimension-keyed matrix while consuming the substrate verbatim (§1 "what is preserved verbatim"). The seven ADRs each close a named scope risk (ADR-002→SR-02/SR-04, ADR-003→SR-05, ADR-004→SR-01/SR-03, ADR-005→SR-08, ADR-006→SR-07, ADR-007→SR-06). The "no net-new transport/cert/spawn code — fork smell to FLAG" boundary (§1, §7.1) defends AC-11 and the vision's single-binary/shipped-client principle. The integration surface (§7) names exact consumed-verbatim signatures vs net-new signatures, preventing the re-author hazard AC-04 forbids.

### Specification Review (PASS)
SPEC §6 gives a complete AC-01..AC-12 → FR/verification traceability table; every scope AC is covered. The four outcome classes are defined as ubiquitous language (§2) and enforced structurally (FR-10/FR-11). Critically, the spec preserves the scope's disposition-authority discipline (FR-16, NFR-8): the implementer/tester never silently widens an exclusion set or decides defect-vs-amendment — a PRODUCT/HUMAN call. This honors the SCOPE constraint and the recurring #5302 lesson. PreCompact (D5 §5) is kept in scope with the OQ-3-mandated documented-call-out behavior rather than dropped, matching the fixed scope disposition.

### Risk Strategy Review (PASS)
The RISK-TEST-STRATEGY frames the suite's cardinal failures correctly as false-RED and false-GREEN — the two ways a parity proof betrays the vision ("measured, not asserted" becomes "asserted, falsely"). The Scope Risk Traceability table maps all eight SRs to architecture risks and resolutions. False-GREEN paths (R-03, R-05, R-06, R-08, R-09, R-15) are correctly elevated to High severity regardless of likelihood because their blast radius is a wrongly-flipped C0 shipped to every remote deployment — exactly the vision-integrity stake. The off-Docker-teeth-before-tag discipline (R-10, #5267) and the security section's per-slug-isolation-as-security-property framing (D6) are appropriate and complete.

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search topic=vision) for vision alignment patterns -- surfaced #3742 (WARN when architecture adds a future branch beyond scope deferral intent), #3337 (artifact-string divergence), #2298 (config semantic divergence). None triggered here: the net-new constructs in nan-022 are scope-risk-mandated, not unrequested future branches, and the three artifacts use consistent terminology.
- Stored: nothing novel to store -- the only generalizable observation (a tightly-constrained test-only feature whose "additions" are all SR-mandated mitigations, correctly not flagged as scope creep) is one-feature-deep and matches existing pattern #3742's inverse; it becomes storable only if a 2nd feature shows architects mis-flagging SR-mandated mitigations as additions. Per the 2+-feature stewardship bar, deferred.
