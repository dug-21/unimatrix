# Alignment Report: crt-058

> Reviewed: 2026-07-05
> Artifacts reviewed:
>   - product/features/crt-058/architecture/ARCHITECTURE.md
>   - product/features/crt-058/specification/SPECIFICATION.md
>   - product/features/crt-058/RISK-TEST-STRATEGY.md
> Scope source: product/features/crt-058/SCOPE.md
> Scope risk: product/features/crt-058/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goals #4946, #5219, #4671

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances goal #5219 success criterion (graph stays integrity-consistent — no orphaned edges/dropped referrers); correctly keeps integrity as rationale, not a pillar; correctly disclaims self-learning. |
| Milestone Fit | PASS | Minimal enhancement in the Cortical graph area; no new table/migration/lifecycle; no future-milestone over-build. |
| Scope Gaps | PASS | All SCOPE goals 1–5 and AC-01…AC-09 map to spec FR/AC. No omissions. |
| Scope Additions | WARN | Tuple-level audit (`DELETE … RETURNING` + metadata JSON) exceeds SCOPE Goal 3's literal "entry id + count." Risk-endorsed (SR-01), traceable, additive — but a conscious expansion the human should confirm. |
| Architecture Consistency | WARN | Unresolved cross-document contradiction on the zero-case advisory rendering (spec AC-05 omit-at-zero vs architecture ADR-004 `Some(0)`→render `0`). Already tracked as spec Open Question 2 / risk R-04; blocks test authoring until decided. |
| Risk Completeness | PASS | R-01…R-11 map every SR-01…SR-06; covers audit, graceful-degradation, and graph-integrity principles plus weaponized-deprecation blast radius and post-commit atomicity. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE goal and AC is addressed. |
| Addition | Removed-edge tuple auditing | Architecture ADR-002 records `(source_id, target_id, relation_type)` tuples via `DELETE … RETURNING` into `AuditEvent.metadata`; SCOPE Goal 3 / AC-03 mandate only entry id + count. Rationale: SR-01 reconstructability of an irreversible delete. Endorsed by SCOPE-RISK-ASSESSMENT; specified as conditional AC-11. Reuses the existing audit path — not new persistence. Accepted addition, human confirmation advised. |
| Addition | AC-10 subset test + AC-11 tuple audit | Spec adds two ACs beyond SCOPE's AC-01…AC-09. Both are direct, traceable folds of scope risks SR-02 and SR-01 — not net-new feature scope. |
| Simplification | Machine-generated edges left to the tick | Rationale: disposable derived data, always regenerated Active-allowlisted; eager deletion buys nothing and risks divergence. Explicit in SCOPE Non-Goals and spec FR-02. Consistent across all three docs. |

## Variances Requiring Approval

1. **Zero-case advisory rendering contradiction (WARN — decision needed).**
   - **What**: Spec AC-05 / NFR-04 require a zero-edge deprecation to be indistinguishable from pre-feature output (advisory omitted). Architecture ADR-004 mandates `Some(0)` renders a literal `0` in every format. These cannot both hold. Surfaced as spec Open Question 2 and risk R-04 (High likelihood, "tester will block").
   - **Why it matters**: NFR-04 backward-compatibility versus the architecture's uniform-rendering contract; the tester cannot author AC-05 without a resolution. This is an internal source-document inconsistency, not a vision breach.
   - **Recommendation**: Human/architect resolves Open Question 2 (omit-at-zero vs render-`0`) before Stage 3a. Then reconcile spec AC-05 and architecture ADR-004 to state the same behavior.

2. **Audit granularity expansion beyond SCOPE's count-only (WARN — confirm).**
   - **What**: Architecture elects tuple-level audit (ADR-002) where SCOPE Goal 3 / AC-03 asked only for entry id + count.
   - **Why it matters**: SCOPE directs "keep the feature minimal … an audit record. No new persistence, no defensive scaffolding." Tuple auditing is a deliberate step past the literal minimal, justified by the delete being irreversible (SR-01).
   - **Recommendation**: Accept — the addition is risk-endorsed, reuses the wired audit path (no new persistence), and materially improves reconstructability of an irreversible delete. Confirm the granularity call at the design gate rather than silently adopting it. If declined, fall back to count-only per AC-11's conditional.

## Detailed Findings

### Vision Alignment
Goal #5219 (self-learning intelligence) carries the success criterion: "The typed knowledge graph stays integrity-consistent under correction — no orphaned edges, no dropped referrers." crt-058 extends exactly this property to the deprecation event: it resolves live dangling agent-authored references to a retired entry the instant the entry is retired, rather than up to ~900s later. This directly serves a stated goal criterion.

The feature correctly navigates the project's integrity-framing discipline (memory: avoid-overstating-defensive-structure; 1-client:1-project rationale):
- SCOPE §Value/Framing states integrity is "a documented rationale, not a product goal/vision pillar" and directs a minimal build — one delete, one inline count, one audit record, no new persistence, no defensive scaffolding. The architecture and spec honor this: single indexed statement, reused write path, reused audit path, no table/migration/lifecycle.
- The feature does not elevate integrity to a goal, and does not over-build: no detection rule revival, no cohesion metric, no findings table, no governance nudge (all explicit Non-Goals, consistent across docs).
- The "explicitly NOT self-learning / not drift-adaptation" disclaimer is correct and non-contradictory: maintaining the graph-integrity success criterion is not itself a learning mechanism — the feature feeds no model, confidence, or adaptation path. The disclaimer refers to mechanism, the goal criterion to a maintained property. No tension.

Architectural principles: consistent with #2 (audit append-only — the feature emits an AuditEvent), #4 (typed graph accuracy — keeps dependency data accurate), #5 (graceful degradation — non-fatal with tick backstop). Principle #1 (hash-chain immutability) is not implicated: `graph_edges` are derived relationship rows, not hash-chained entries; the tick already deletes them. Provenance is preserved through the audit record (the tuple-audit addition strengthens this).

No under-delivery: SCOPE goals 1–5 all land in spec FR-01…FR-09 and the architecture insertion plan.

### Milestone Fit
Cortical-phase graph maintenance, scoped to one chokepoint (`context_deprecate` step 6.5). No new table, migration, prune lifecycle, or tick change (FR-08 / AC-08). Nothing is built ahead of need: the eager delete pulls forward only the tick's existing blanket delete for one entry. No future-milestone capability is introduced. Milestone discipline holds.

### Architecture Review
The insertion point (`tools.rs:1413`, new step 6.5 after the step-5 idempotency guard and step-6 flip), the LOCKED predicate `(source_id=?1 OR target_id=?1) AND source=?2 RETURNING …`, and the `eager ⊆ tick` invariant are precisely specified and traceable to SCOPE constraints C-01…C-11. The invariant is made executable (ADR-003, entry #5460), citing bugfix-458 (#3910) and #5417 — matching the recurring "multi-pass same-table cleanup must not diverge" pattern.

One unresolved cross-document contradiction (zero-case rendering, Variance 1). One deliberate expansion (tuple auditing, Variance 2). Both are surfaced in the docs themselves (Open Question 2, ADR-002/AC-11), not hidden — hence WARN, not VARIANCE/FAIL.

### Specification Review
FR-01…FR-09 and AC-01…AC-11 are individually testable and state/behavior-based (SR-04 discipline: parse-based per-format matrix, per-source removal matrix). Requirement interpretations are documented. Two ACs beyond SCOPE's set (AC-10, AC-11) are direct folds of SR-02 and SR-01, not scope creep. Spec Open Question 2 is the one item requiring human resolution before test authoring.

### Risk Strategy Review
R-01…R-11 with full SR traceability; every SR-01…SR-06 has a mapped architecture risk and verifying scenario. Notable coverage the vision cares about: R-01 (the subset test's successor-less blind spot — adds an explicit chokepoint-exclusion assertion against the real handler, closing the one case that actually breaks `eager ⊆ tick`), R-03 (post-commit marshaling → irreversible delete with no audit — the atomicity closure item for delivery), R-06 (unguarded helper single-caller invariant), and the security note on weaponized high-degree deprecation (bounded to edges touching the one id — latency reduction, not new reach). Coverage is complete for the vision-relevant principles.

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search) for vision alignment patterns — surfaced #3742 (optional/added branch in architecture must match scope intent → WARN if it diverges from scope deferral), #2298 and #3337 (spec/vision divergence patterns). Applied #3742 to classify the tuple-audit expansion as a documented, risk-endorsed addition (WARN, not silent). Also confirmed #3910 (eager ⊆ tick basis) and crt-058 ADRs #5458/#5460 already stored.
- Stored: nothing novel. The variances here are feature-specific: the zero-case rendering contradiction is a within-artifact open question, and the tuple-audit expansion is a risk-endorsed local decision. Neither generalizes beyond #3742, which already captures the "architecture addition must trace to scope intent" pattern.
