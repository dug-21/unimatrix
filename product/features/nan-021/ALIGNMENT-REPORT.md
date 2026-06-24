# Alignment Report: nan-021

> Reviewed: 2026-06-24
> Artifacts reviewed:
>   - product/features/nan-021/architecture/ARCHITECTURE.md
>   - product/features/nan-021/specification/SPECIFICATION.md
>   - product/features/nan-021/RISK-TEST-STRATEGY.md
> Scope source: product/features/nan-021/SCOPE.md
> Scope risk source: product/features/nan-021/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md (+ goal #4946, capability #5191)
> Reviewer: nan-021-vision-guardian

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances goal #4946 (personal-cloud) marquee promise C0 — "full pipeline fidelity over HTTPS == local". Exercises shipped surfaces as-is, principle 5 (graceful degradation), 6 (single binary), 3 (capability checks) all respected. |
| Milestone Fit | PASS | Pure test-infra fixture; targets C0's `done_when` (measurement, not new capability). No future-milestone capability built. |
| Scope Gaps | PASS | All five SCOPE Goals + AC-01..AC-07 map to FRs/ACs across the three docs. |
| Scope Additions | WARN | FR-10 / ADR-006 (symmetric durability barrier) and NFR-7 / ADR-002 (idle-window minimization + self-heal reliance) are net-new fixture-orchestration mechanisms not named in SCOPE. Both are derived from RISK-TEST R-05/R-06 and explicitly preserve NFR-1/NFR-2 (no production change). Sound, but they extend the implementation surface beyond the literal SCOPE text — note for human awareness. |
| Architecture Consistency | PASS | ARCHITECTURE, SPECIFICATION, RISK-TEST agree on substrate (D-1 hybrid), bridge-in-path (D-2), parity contract (D-5), no-seed (SR-06), false-green gate (D-3). ADR table ↔ FR table ↔ risk register are mutually traceable. |
| Risk Completeness | PASS | RISK-TEST R-01..R-14 cover the C0-relevant failure surface (parity flakiness, exclusion-set correctness, bridge bypass, false-green, attribution degradation). Scope Risk Traceability table maps SR-01..SR-06 → R-* → resolution. |

**Status counts:** PASS 5, WARN 1, VARIANCE 0, FAIL 0.

## C0 "Advances vs Proves" Framing — Consistency Audit (requested focus)

The framing is **consistent and correct across all artifacts and the source-of-truth capability entry.**

- **Capability #5191 (source of truth):** `status: partial`; `done_when` = "for a remote slug, retrieval AND behavioral signals AND analytics/learning all function at parity with a local-UDS deployment of the same workload (measured, not asserted)"; explicit firewall: "children all green does NOT auto-prove the rollup — it needs its own behavioral parity measurement to flip proven." nan-021 produces exactly that measurement artifact.
- **SCOPE.md:** lines 8-11, Goal 4 (line 65-66), Non-Goals (line 151 equivalent), D-4 (line 305-308), Tracking (line 321-324) — all state nan-021 **ADVANCES** C0 as its **named evidence artifact** and **does NOT flip C0 to `proven`**; the flip is held by **#837** which *consumes* this artifact.
- **ARCHITECTURE.md:** System Overview (lines 19-21) — "ADVANCES C0 ... #837 holds the `proven` flip; nan-021 produces the artifact #837 consumes — D-4."
- **SPECIFICATION.md:** Objective (line 12) and NOT-in-Scope (line 151) — "ADVANCES capability C0 (#5191); it does not flip C0 to `proven` (held by #837)."
- **RISK-TEST-STRATEGY.md:** header frames the fixture as the parity proof; R-02 impact note (line 44) — "C0 advances on a false artifact" if the exclusion set is over-broad, reinforcing that the artifact's integrity (not the flip) is this feature's responsibility.

No artifact overstates nan-021 as proving or flipping C0. The boundary with #837 is named identically in all four places. **No variance.**

## Scope Boundary Audit — Parity Surface (requested focus)

SCOPE constrains the parity surface to the **observe → topic_signal → cycle_review** pipeline (behavioral-signal + analytics half), explicitly excluding retrieval (`context_search`/`get`) over the bridge (covered by vnc-039 AC-03). This boundary is held consistently:

- SCOPE Non-Goals: "NOT broadening C0's parity surface beyond the cycle/observe/review pipeline."
- SPECIFICATION NOT-in-Scope (line 147): identical exclusion of retrieval re-test.
- ARCHITECTURE Component Breakdown + parity assertion (lines 137-141): asserts only `MetricVector` parity + derived `topic_signal` + non-empty — no retrieval assertion introduced.
- No artifact adds production behavior. NFR-1/AC-06 (zero production-code diff) is asserted in all three docs with a concrete `git diff` verification (`product/test/infra-001/**` + CI lane + docs only). **No drift into production behavior; no broadening beyond the named parity surface.**

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | All SCOPE Goals 1-5 and AC-01..AC-07 are carried into SPECIFICATION FR-1..FR-10 / AC-01..AC-07 and the RISK-TEST register. |
| Addition | FR-10 / ADR-006 — symmetric observe-durability barrier | Net-new fixture-orchestration step (deadline-poll gating both `cycle_review` calls). Not in SCOPE text; derived from RISK-TEST R-06 (#5265 fire-and-forget WAL). Explicitly scoped as "fixture orchestration only — no production code change" and "preserves NFR-1." Mechanism, not capability expansion. See WARN below. |
| Addition | NFR-7 / ADR-002 — idle-window minimization + shipped self-heal reliance | Net-new constraint on drive ordering (spawn-bridge-last, drive-immediately) to survive rmcp keep_alive eviction (#5280/#830). Derived from RISK-TEST R-05. Explicitly forbids re-implementing reconnection (would violate NFR-2). Mechanism, not capability expansion. See WARN below. |
| Simplification | (none material) | The fixture asserts the analytics+signal half only; SCOPE itself authorizes this (retrieval is vnc-039's job), so it is a documented scope boundary, not a simplification. |

## Variances Requiring Approval

**None requiring approval (no VARIANCE or FAIL).**

One **WARN** is noted for human awareness (does not block):

1. **What:** SPECIFICATION FR-10 (durability barrier) and NFR-7 (idle-window minimization + self-heal reliance), each backed by a net-new ADR (ADR-006, ADR-002), introduce fixture-orchestration mechanisms not named in SCOPE.md or its Resolved Decisions (D-1..D-6).
   - **Why it matters:** These are the kind of architect-introduced additions the vision-guardian pattern #3742 flags — risk-document test requirements (R-05, R-06 are Critical) become load-bearing on mechanisms the human did not explicitly approve in SCOPE. The difference from a true variance: both additions are (a) strictly test-orchestration, (b) explicitly bounded to preserve NFR-1 (zero production diff) and NFR-2 (no fork / no re-authoring shipped behavior), and (c) traceably derived from the SCOPE-RISK-ASSESSMENT's own assumptions (SR-01 brittleness; the #5265 WAL and #5280 eviction behaviors named there). They make the existing parity assertion *correct and non-flaky* rather than expanding scope.
   - **Recommendation:** ACCEPT. The additions are necessary for AC-04 to be a true (non-vacuous, non-flaky) parity proof and are consistently reflected across all three source documents (ADR-006/ADR-002 in ARCHITECTURE, FR-10/NFR-7 + AC-02/AC-04 in SPECIFICATION, R-05/R-06 + Scope Risk Traceability in RISK-TEST) — satisfying the four-part consistency closure from pattern #3742. No SCOPE update is strictly required since they are pure test mechanism preserving the no-production-change invariant; optionally note them in SCOPE's Proposed Approach for provenance.

## Detailed Findings

### Vision Alignment
nan-021 directly serves goal #4946 (`personal-cloud`): its core success criterion is "full intelligence-pipeline fidelity ... same over HTTPS as over local UDS." The fixture is the *measurement* that proves this for the behavioral-signal + analytics half. Architectural principles respected: P3 capability checks (bearer flushed only after pin match — exercised as-is), P5 graceful degradation (relies on shipped keep_alive self-heal #830, never re-implements — NFR-7), P6 single-binary / client-is-adapter (drives the shipped image + JS bridge, no new infrastructure). The fixture also honors the memory-noted standing rule that GH CI is JS-client-only and Rust/container validation lives in the release workflow (SCOPE Constraints; D-3; AC-05) — correctly homing the gate in a `workflow_dispatch`/tag lane, not `pull_request`.

### Milestone Fit
This is a test-infrastructure deliverable on the Nanoprobes (`nan`) track, cumulative on infra-001/nan-019. It builds no future-milestone capability — C0's `done_when` is explicitly a *measurement* feature, not new function (capability #5191 note: "now a measurement feature with no blocking constituent"). The proven flip is correctly deferred to #837, so nan-021 does not over-reach into capability-flip territory it does not own.

### Architecture Review
ARCHITECTURE's six-ADR table is mutually traceable to SPECIFICATION's ten FRs and RISK-TEST's R-register. The D-1 hybrid substrate (Docker smoke owns HTTPS standup + bridge cycle; Python harness owns UDS baseline + comparator) is stated identically in all three docs. The open questions (OQ-1..OQ-5 in ARCHITECTURE; OQ-1..OQ-4 in SPECIFICATION) concern implementation seams (projectHash read-back, cross-language workload manifest, single-execution orchestration, exclusion-set enumeration) — all are design-phase detail within the locked decisions, not vision/scope deviations. The cumulative-extension constraint (AC-07/NFR-2/SR-04) is enforced by the Component Breakdown table naming each helper's parent asset, with C4 (comparator + workload driver) flagged as the sole substantial net-new module.

### Specification Review
FR-1..FR-9 map 1:1 onto SCOPE Goals 1-5 and AC-01..AC-07. FR-10 and NFR-7 are the two additions (see WARN). The no-seed contract (FR-6 / AC-03) is precise: both forbidden seed sites (`_seed_observation_sql_lifecycle` SQL, `make_stamped_event` struct) are named and a static reachability assertion is required. The parity contract (FR-7 / AC-04) correctly enumerates the D-5 exclusion set as a *closed, named* set with out-of-set divergence treated as a real failure — guarding against the over-broad-exclusion vacuity (R-02) that would let C0 advance on a false artifact.

### Risk Strategy Review
RISK-TEST is architecture-risk mode, weighted to "what gives a wrong verdict" — appropriate for a measurement fixture whose entire value is verdict integrity. The two Critical risks most load-bearing for vision fidelity are R-01 (incomplete exclusion set → flake → reactive widening → R-02) and R-02 (over-broad set → vacuous green → C0 advances on a false artifact). Both have mutation-style coverage requirements (force a structural field to differ → comparator must fail). The Scope Risk Traceability table closes SR-01..SR-06 to R-* with named resolutions. Security section correctly frames the fixture as exercising (not weakening) the shipped trust boundary, with the one deliberate exception (`emit_bundle` stderr suppressed to avoid logging the bearer) called out — consistent with principle 8 (no secrets persisted; credstore mode 0600 under hermetic `$HOME`).

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search category=pattern) for vision alignment patterns — strong hit #3742 ("optional future branch / architect addition must match scope intent — WARN if architecture+risk treat a non-scoped mechanism as in-scope with test requirements"); applied to classify the FR-10/NFR-7 additions as WARN-accept rather than VARIANCE. Also #2298 (config semantic divergence) and #3337 (informal-header divergence) — not applicable here (no config/sample-output surface).
- Stored: nothing novel to store — the recurring pattern (architect-introduced test-orchestration mechanism, no-production-change-preserving, derived from the risk doc) is already captured as #3742 and the framing-consistency check is feature-specific (C0/#837 boundary). The variances here are feature-specific and do not generalize beyond what #3742 already covers.
