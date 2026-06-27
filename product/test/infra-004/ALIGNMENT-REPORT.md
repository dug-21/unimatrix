# Alignment Report: infra-004

> Reviewed: 2026-06-27
> Artifacts reviewed:
>   - product/test/infra-004/architecture/ARCHITECTURE.md
>   - product/test/infra-004/specification/SPECIFICATION.md
>   - product/test/infra-004/RISK-TEST-STRATEGY.md
> Scope source: product/test/infra-004/SCOPE.md
> Scope-risk source: product/test/infra-004/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md
> Goal: goal:personal-cloud (#4946); capability N3 (#5161), N4

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances the integrity basis of `goal:personal-cloud`; enforces, not over-builds. |
| Milestone Fit | PASS | Closes the only remaining N3 blocker (standing gate #788 unwired); N3 partial→proven, N4 advanced. |
| Scope Gaps | PASS | All four deliverables + AC-01..15 traced across the three docs. |
| Scope Additions | WARN | Diagnostic full-log echo + concrete INFRA marker not named in SCOPE (low risk, derived from scope-risk rec); echo introduces a workflow-command-injection surface (mitigated). |
| Architecture Consistency | WARN | #3337 pattern: architecture's `e.g.` INFRA marker is treated as a literal by RISK-TEST R-09 while the spec leaves it unpinned. |
| Risk Completeness | PASS | 15 risks, all SR-01..09 traced; central vision risk (silently-vacuous enforcement) covered thoroughly. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE AC (AC-01..15) and deliverable (warmup barrier, lane wiring, cold-model GREEN, blocking flip) is addressed in ARCHITECTURE (C-WB/C-TS/C-LN/C-FLIP) and SPECIFICATION (FR-1..20). |
| Addition | Diagnostic-capture-first full smoke-log echo on every C-TS path (ARCH §6, RISK R-10) | Not named in SCOPE; derived from SCOPE-RISK-ASSESSMENT Design Rec 1 ("diagnostic-capture-first"). Justified for the never-green-on-tag tax. Introduces a workflow-command-injection surface (own-image, mitigated by `-qxE` full-line marker anchor). See WARN below. |
| Addition | Concrete INFRA marker string `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN` (ARCH §4) | SCOPE/spec only require "a distinct, greppable marker." Concretization, not a true expansion — but see Architecture Consistency WARN. |
| Simplification | amd64-only blocking; no arm64 lane (D-3) | Rationale: `resolve_store` routing is architecture-independent Rust; a leak manifests identically on both arches. Human-signed trade-off. Aligned. |
| Simplification | No automated chronic-INFRA escalation (SR-07/R-15); visibility = `::warning::` + greppable marker only | Rationale: "no new mechanism" scope discipline; stable marker leaves a cheap follow-up seam. Flagged as accepted human-vigilance risk — see Variance 1. |

## Variances Requiring Approval

1. **What**: The headline DoD — "a cross-tenant leak cannot ship a release" — is conditional on human vigilance against chronic INFRA. A blocking lane that goes INFRA-dark (cold-HF throttle, wrong-tag pull-404, chronic warmup miss) does NOT block the manifest and the release ships with isolation unverified. Mitigation is visibility only (`::warning::` + greppable marker), unenforced (SR-07 / R-15 / OQ-3).
   - **Why it matters**: Cross-tenant integrity is a stated `goal:personal-cloud` invariant (#4946) and N3's reason-for-being. "The gate is blocking" ≠ "the gate verified isolation this run." Without an enforced escalation, the integrity guarantee the feature claims to realize is genuine for RED but silently vacuous for chronic-INFRA — the feature's own central risk.
   - **Recommendation**: ACCEPT as documented human-vigilance residual for infra-004 (consistent with "no new mechanism" scope discipline), OR direct a follow-up escalation (tracked INFRA count / threshold across N releases). The docs are honest and the marker is deliberately stable to make the follow-up cheap. This is OQ-3 — it needs an explicit human decision before merge, because it qualifies the DoD outcome claim and the N3 `proven` assertion (AC-14).

## Detailed Findings

### Vision Alignment — PASS
The feature serves `goal:personal-cloud` (#4946) directly: the goal names cross-project write integrity as a knowledge-INTEGRITY boundary and `resolve_store` as the single isolation seam ("cross-project mis-targeting is unrepresentable... the resolved store handle is the sole write capability"). infra-004 converts the existing behavioral proof from detection to enforcement, realizing architectural principles #1 (hash-chain integrity immutable — a mis-route corrupts the wrong chain) and #3 (capability/isolation at the service layer).

Notably, the feature does NOT over-build (a recurring failure mode this codebase guards against): it is test/CI-only (NFR-5, no `crates/` change), reuses existing idioms (no new readiness mechanism), and actively cuts scope where signal is near-zero (arm64 D-3; chronic-INFRA escalation out of scope). This is proportionate enforcement of an already-stated goal invariant, not defensive gold-plating.

### Milestone Fit — PASS
N3 (#5161) is currently `partial`; its note cites two blockers: (i) the standing gate (#788) is unwired, and (ii) C5/#787 per-slug surface open. SCOPE correctly records that blocker (ii) is already resolved (C5/#5190 proven via crt-056/#789, merged 2026-06-19), so this feature closes the only remaining blocker (i). N3 partial→proven (AC-14) and N4 advanced (warmup barrier + visible INFRA) are the right milestone outcomes.

Minor note (not a variance): the live N3 entry (#5161) note still lists BOTH blockers as open — it is stale relative to the C5 resolution. Delivery's AC-14 update will refresh it; flagged only so the delivery agent rewrites the full note, not just the status field.

### Architecture Review — WARN (consistency)
Architecture is internally coherent: the C-WB/C-TS/C-LN/C-FLIP decomposition matches SCOPE's four steps; the §3 exit-code→outcome table and §4 blocking-semantics mapping are precise and preserve RED>INFRA>GREEN end-to-end; §5 explicitly contains the SR-04 blast radius (only script-exit-2 maps to non-blocking; all harness failures fail closed). ADRs 001–004 cover the load-bearing decisions.

The one consistency concern is the **#3337 pattern** (architecture sample treated as authoritative by the tester): ARCHITECTURE §4 introduces the INFRA marker with `(e.g. [infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN)` — i.e. illustrative — but RISK-TEST-STRATEGY R-09 scenario 1 asserts that exact literal as the marker, while SPECIFICATION FR-11/NFR-2 only require "a distinct, greppable marker" without pinning the string. If the implementer picks a different marker than the architecture's illustrative one, the tester's literal assertion fails for a reason unrelated to correctness.
- **Recommendation**: pin the canonical marker string in SPECIFICATION (promote it from illustrative to normative), OR annotate the architecture string "illustrative only." Either removes the divergence. Minor — does not block.

### Specification Review — PASS
FR-1..20 trace AC-01..15 verbatim and add testable phrasing + verification methods. Domain-model table is strong and goal-anchored (cross-tenant isolation defined as the integrity invariant; vacuous-enforcement named as the central hazard). NFR-1..7 capture determinism, visibility, single-source-of-truth, stub-seam provability, change confinement, tri-state invariance, additive-lib safety. OQ-1 (blast radius) is resolved by ARCH §5; OQ-2 (GHCR write from branch) is correctly routed to the architect to verify early; OQ-3 (chronic-INFRA) is correctly routed to the human (Variance 1).

### Risk Strategy Review — PASS
Coverage is thorough and vision-relevant: the dominant failure class is correctly framed as silently-vacuous enforcement (isolation never actually verified), not gate-absence. Two Critical risks (R-01 ceremonial barrier, R-05 swallowed-exit-code false-green) target the exact ways the integrity guarantee could be hollow. All SR-01..09 map to R-NN with resolutions; security section flags the diagnostic-echo workflow-command-injection surface and confirms the `-qxE` full-line anchor prevents a forged GREEN credit; failure-modes and edge-cases tables are complete. The scope-addition (full-log echo) is the source of the one residual security surface — accepted as own-image and mitigated, but it is a surface SCOPE did not enumerate (Scope Additions WARN).
