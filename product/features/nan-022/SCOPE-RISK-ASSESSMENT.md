# Scope Risk Assessment: nan-022

Cross-transport parity suite (C0 proof artifact, #837). Test-only; extends nan-021
`infra-001`. Six dimensions × two transports (HTTPS bridge / stdio-UDS), one workload.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Retrieval ranking nondeterminism is partly UNFIXABLE in-test: HNSW approximate top-k membership flips from per-process OS entropy (no seed API, hnsw_rs 0.3.4) — verified in bugfix-742 (#4990), deferred to GH#746 — on top of HashMap-order (#2610) and `sort_unstable` ties. A naive exact-id-order assertion (AC-02) will flake. | High | High | Architect must design the retrieval comparator as ordered-set-with-tie-tolerance over a corpus large enough that the *stable* prefix is the parity signal; intra-transport instability is NOT a parity defect (OQ-2). Resolve OQ-2/OQ-3 tolerance before spec. |
| SR-02 | The #830 self-heal the suite couples to (ADR-002) covers only SIGNALLED (404) eviction, NOT silent half-open-socket eviction (#839, #5303). #839 is now CLOSED (landed via commit 5b6badad / PR #842, 2026-06-25); the C0 precondition is met and delivery is UNBLOCKED. The INFRA-ERROR classification for any half-open hang is retained as **defense-in-depth**, not a gating dependency. | High | Med | Architect must NOT treat any HTTPS-leg hang as a parity result. Design a per-leg transport-health preflight + bounded connect/idle deadline so a hang surfaces as an INFRA-ERROR exit class (distinct from RED/GREEN), never a dimension FAIL — defense-in-depth, since #839 is fixed. |
| SR-03 | Briefing/injection (proactive delivery) is embedding/cluster-ranked — SAME nondeterminism class as retrieval (SCOPE table, #4113/#4128) — plus session-state injection history. Two dimensions share one failure mode; one entropy source can falsely red BOTH. | High | High | Single-source the embedding/ranking tolerance policy across the retrieval AND briefing comparators (don't author two divergent tie policies — the #5302 drift hazard). One determinism strategy, two consumers. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Parity-gate design muddles two failure classes: (a) cross-transport divergence = real C0 parity defect → RED; (b) intra-transport nondeterminism (#2610, ties, cold-start) = pre-existing bug to FILE SEPARATELY, NOT a cloud-parity failure (OQ-2, fixed disposition). Conflating them poisons the red gate with non-cloud flakes. | High | High | Spec must define THREE outcome classes per dimension — PARITY-PASS, PARITY-FAIL (cross-leg, RED), INTRA-TRANSPORT-NONDETERMINISM (file as separate GH bug, NOT this gate). Intra-transport instability detected by running each leg's capture twice and diffing; divergence there is excluded from the parity verdict. |
| SR-05 | Six per-dimension exclusion sets, hand-authored on the nan-021 template, drift silently — the exact #5302 lesson (single-source the CONTRACT not just the DATA; nan-021 hit this twice). Six near-duplicate comparators multiply the surface. | Med | High | Architect: single-source the comparator framing/template and the forbidden-seed closed set across all six dimensions, OR add a cross-dimension equivalence guard. Convention ("conform to nan-021") is not a guard (#5302). |
| SR-06 | "One workload" (OQ-1) vs. retrieval/briefing needing a pre-seeded multi-entry store + non-trivial query set. A degenerate single-hit ranking gives a vacuous parity pass (the nan-021 R-03 thin-workload hazard). | Med | Med | Spec the seed corpus + query set as load-bearing: enough entries that ranking is a real ranking. Keep one identity/one token (SR-05/#832 defense) while augmenting the workload, per leaning (a). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | PreCompact restoration rides the hook `/observe` route (`wire.rs`/#670), NOT the MCP bridge. Measurability risk: the restored payload may have a host-side (CC) component the test-only harness cannot drive (OQ-3). | Med | Med | OQ-3 disposition is fixed: PreCompact stays IN scope; if not test-only-drivable it is a DELIVERY-TIME call-out, not a scope drop. Architect must determine at design time whether `CompactContext` is symmetrically capturable from both legs and flag the host-side gap explicitly if not. |
| SR-08 | Two HTTPS wire surfaces (MCP bridge vs `/observe`); routing a dimension to the wrong surface silently records NOTHING (the #5298 legacy/rework-frame gotcha) — a vacuous pass, not a loud fail. | Med | Med | Every observe-driven dimension must conform to the byte-identical 11-frame #5298 sequence; a missing-capture must ERROR (never empty-pass). Carry nan-021 R-03 "missing dimension errors" to all six. |

## Assumptions

- **(Goals 1–4, Constraints)** The nan-021 substrate generalizes cleanly to N dimensions. If a
  dimension's determinism cannot be made comparable (SR-01/SR-03), AC-02/AC-05 are unachievable
  without a production fix — which is a FILED bug, not absorbed (AC-10), and may leave C0 short of `proven`.
- **(Constraint: #830 coupling, ADR-002)** That the shipped self-heal makes a flake a clean #830 signal.
  #5303 showed this was NOT true for silent eviction (#839); #839 is now CLOSED (commit 5b6badad / PR #842,
  2026-06-25). The INFRA-ERROR half-open-hang classification is retained as defense-in-depth, not a blocker.
- **(AC-12 / OQ-6) — RESOLVED.** That all SIX dimensions must pass to flip C0 is CONFIRMED (human, 2026-06-25).
  The corrected C0 (#5304) `done_when` makes parity the total bar: the dimension list is the present
  expression of the bar and grows with the pipeline, it does not narrow the bar; any unreachable dimension is
  a human-signed documented exception, never silently excluded. Design default `blocks_c0_proof=True` for all
  six is correct and aligned.

## Design Recommendations

1. **SR-02/SR-08**: Introduce a distinct INFRA-ERROR / vacuous-capture exit class. A half-open-socket hang
   (the now-CLOSED #839 class, kept as defense-in-depth) or an empty capture must NEVER read as a dimension
   verdict. (Architect + spec.)
2. **SR-01/SR-03/SR-04**: One determinism/tolerance policy, single-sourced across retrieval + briefing;
   detect intra-transport instability by double-capture-and-diff and route it OUT of the red gate to a
   separately-filed bug. (Architect.)
3. **SR-05**: Single-source the six-comparator contract and the forbidden-seed set, or add a drift guard —
   do not rely on convention (#5302). (Architect.)
4. **SR-06**: Spec a non-degenerate seed corpus + query set so ranking parity is non-vacuous. (Spec.)
5. **SR-07**: Confirm PreCompact symmetric capturability at design time; flag any host-side gap as a
   delivery call-out per OQ-3. (Architect.)
