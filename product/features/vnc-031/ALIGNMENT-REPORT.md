# Alignment Report: vnc-031

> Reviewed: 2026-06-10
> Artifacts reviewed:
>   - product/features/vnc-031/architecture/ARCHITECTURE.md
>   - product/features/vnc-031/specification/SPECIFICATION.md
>   - product/features/vnc-031/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-031/SCOPE.md
> Scope risk source: product/features/vnc-031/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md + goal #4934 (personal-cloud)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances `personal-cloud` goal: clean `init --remote` migration for any consumer, one battle-tested ownership-aware path. No vision principle contradicted. |
| Milestone Fit | PASS | Root-cause fix beside the active OSS-cloud track (nan-016 #681, F-series). Completes a data-source migration already shipped (crt-052 #706, vnc-027 #4811); builds no future-milestone capability. |
| Scope Gaps | PASS | All four SCOPE Goals and AC-01..AC-10 are addressed by FR-01..FR-16 and AC-01..AC-10 in the spec. No goal dropped. |
| Scope Additions | PASS | No capability added beyond SCOPE. The five open questions are resolved within SCOPE's recommended bounds (no signature change, no `UNIMATRIX_PATTERNS` change). |
| Architecture Consistency | PASS | ADR-001/002/003 trace to SR-01..SR-07 and the SCOPE Open Questions; object-identity keep-rule, registered-events-only partition, parity-gated script retire. Install-surface only. |
| Risk Completeness | PASS | R-01..R-15 map every SR-01..SR-07 plus the OQ-B base-branch dependency; the two binding gates (identity-not-string, parity-on-real-input) are the focus, not the happy path. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Coverage | Goal 1 (cross-group prune for managed events) | SPEC FR-01, FR-04; ARCH Step 3c; AC-01, AC-02. |
| Coverage | Goal 2 (preserve every existing semantic) | SPEC FR-05, FR-06, FR-10, FR-11, FR-12, FR-13; AC-03..AC-05, AC-07. |
| Coverage | Goal 3 (regression coverage) | SPEC AC-01..AC-08; RISK-TEST R-01..R-13 scenarios. |
| Coverage | Goal 4 (retire bespoke script prune) | SPEC FR-14, FR-15, FR-16; ARCH ADR-003; AC-09; RISK-TEST R-04 (P1-P8 table). |
| Boundary held | Non-Goals 1-7 | Spec "NOT in Scope" mirrors SCOPE Non-Goals exactly: no `EVENT_MATCHERS` change, no `isUnimatrixHook` change, no foreign-hook prune, no opt-out-path change, no signature change, no Rust/server/transport change. |
| Simplification | None | No requested capability simplified or deferred. |

No scope gaps. No scope additions.

## Variances Requiring Approval

None.

Two items already correctly surfaced by the artifacts for human/delivery sign-off (not new variances — they are SCOPE-originated open questions carried forward intact):

1. **OQ-4 behavioral change (survive -> pruned)** — SCOPE flagged, and ARCHITECTURE ADR-002 records it as "human-approved." Any `isUnimatrixHook === true` entry under a non-managed matcher is now reconciled (pruned) rather than ignored. This is the one genuine behavioral change and is consistent with the project-wide ownership model. Already approved per SCOPE; noted here for traceability, not re-opened.

2. **OQ-B / R-14 delivery-base dependency** — the lossless-prune justification depends on crt-052 #706 and vnc-027 #4811 being present on the *delivery base branch*, not merely "on main as of writing." ARCHITECTURE (Open Questions) and RISK-TEST R-14 both make this a pre-delivery gate. This is a delivery-time verification, correctly deferred — not a design variance.

## Detailed Findings

### Vision Alignment

The relevant strategic goal is `personal-cloud` (#4934): "One container, one bearer token, one command" with "full intelligence-pipeline fidelity ... same over HTTPS as over local UDS." vnc-031 directly serves this: it makes `init`, `init --remote`, and the dogfood switchover all produce a *clean* hook migration from a single shipped `mergeSettings` path, removing the per-consumer reimplementation that nan-016 was forced into. The goal's own delivery note tracks the nan-016 (#681) UDS dogfood switchover; vnc-031 is the root-cause completion of that line.

Architectural principles are respected. Principle 6 ("the client is an adapter ... not infrastructure") is exactly the surface touched — the JS/TS install client — with no Rust/server/daemon change (Non-Goal 5, NFR-04). No hash-chain, audit-log, capability-check, or secrets principle is in play; this is install-artifact reconciliation, not knowledge-engine behavior. Marking principles 1-5, 7, 8 N/A is proportionate for an install-surface feature.

The pruned broad `"*"` `PreToolUse` telemetry is not a vision regression: SCOPE and ARCHITECTURE establish the replacement observation source (PostToolUse duplicate signal + transcript-fed cycle-review distillation, crt-052 #706) shipped, so the self-learning pipeline loses no signal. This is a completed migration, not an information-loss decision originated here.

### Milestone Fit

The feature sits beside the active OSS-cloud finalization track and builds nothing for a future milestone. It removes a script-level workaround (nan-016 ADR-003 intent: "one battle-tested code path") rather than adding capability. Scope is deliberately minimal (NFR-03), four file areas (NFR-01). No milestone-discipline concern.

### Architecture Review

ARCHITECTURE is tightly bounded to SCOPE. Three ADRs each resolve a SCOPE Open Question / SR:
- ADR-001 (keep-target by object identity) closes SR-01 by making the zero-uni-hook failure unrepresentable by construction — stronger than the string-compare the SCOPE proposed approach hinted at, and explicitly chosen to remove fragility. This is a correctness *improvement* within scope, not an addition.
- ADR-002 resolves OQ-2/OQ-3/OQ-4 to SCOPE's recommended answers (prune all uni entries outside the managed group; registered events only; prune per human approval).
- ADR-003 retires the script prune with a binding parity-on-real-legacy-input gate (SR-04), not a blind delete.

The Step 3c placement (after Step 3, before Step 3b) and the partition (`events` vs `HOOK_EVENTS \ events`, union all / intersection empty) match SCOPE SR-03 exactly. Signature unchanged (OQ-1). No init.js logic change. Consistent throughout.

### Specification Review

FR-01..FR-16 are individually verifiable and each traces to a SCOPE Goal/AC and an SR. The "NOT in Scope" section mirrors SCOPE Non-Goals 1-7 with no leakage. Domain vocabulary (managed event, keep-target, cross-group prune, stale `"*"` hook) is precise and stable. AC-01..AC-10 carry named verification tests extending existing fixtures (NFR-06, test-discipline), honoring the cumulative-test-infrastructure rule. The three spec Open Questions (OQ-A keep mechanism, OQ-B base dependency, OQ-C harness attribution) are implementation/delivery concerns, correctly routed to architect/SM rather than expanding scope.

### Risk Strategy Review

The risk strategy correctly identifies that ADR-001 shifts residual risk away from the original highest-severity correctness risk (SR-01) toward two implementation-time gates: (a) identity-keep silently degrading to a string compare (R-01), and (b) script-retire parity not proven on real legacy input (R-04). Both are marked Critical with discriminating scenarios — the shape-varying near-twin keep test and the P1-P8 parity table on real legacy input. Foreign-hook blast-radius widening to all groups (SR-02) is covered by the near-miss-survives security-relevant test (R-07). Every SR-01..SR-07 maps to at least one guarded scenario; the OQ-B base-branch dependency is carried as R-14. Coverage is complete and proportionate to an install tool that rewrites user `.claude/settings.json`.

## Knowledge Stewardship
- Queried: /uni-query-patterns + context_search for vision/install-surface alignment patterns -- no recurring vision-alignment pattern found (top hits #2298 config-divergence, #4809 hook-registration verification were low-relevance / not alignment patterns).
- Stored: nothing novel to store -- vnc-031 is a clean, fully-scoped root-cause fix with no alignment variance; the one behavioral change (OQ-4) and the base-branch dependency (OQ-B) were both already surfaced by the source artifacts. No cross-feature misalignment pattern to generalize from a single PASS review.
