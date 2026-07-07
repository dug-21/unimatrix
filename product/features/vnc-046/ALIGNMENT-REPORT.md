# Alignment Report: vnc-046

> Reviewed: 2026-07-07
> Artifacts reviewed:
>   - product/features/vnc-046/architecture/ARCHITECTURE.md (+ ADR-001…005)
>   - product/features/vnc-046/specification/SPECIFICATION.md
>   - product/features/vnc-046/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-046/SCOPE.md, SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goal #5519 (personal-cloud), goal #5474 (integrity)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly realizes goal #5519's OSS per-project isolation invariant + C0 fidelity; honors the `resolve_store` one-funnel principle and Architectural Principles 3/6/7. |
| Milestone Fit | PASS | On the personal-cloud cloud track (completes vnc-038 funnel); enterprise boundaries (NG-1/2/7) correctly deferred as seam-only. No future-milestone over-build. |
| Scope Gaps | PASS | All 4 SCOPE goals and AC-01…AC-10 addressed across the three docs. No under-delivery. |
| Scope Additions | PASS | No unapproved additions. P3 in-scope per resolved OQ-1; ADR-005 (#925) is a SCOPE-requested reconciliation, not new work. |
| Architecture Consistency | PASS | AC set matches SCOPE 1:1; ADRs trace to SR-01…09; internal cross-doc consistency holds. One minor P3-fallback wording tension (non-blocking). |
| Risk Completeness | WARN | Primary behavioral gate (AC-06 suite + INV-C proof) depends on OPEN #800 fixture with unconfirmed owner. Surfaced by the docs but unresolved — flag for human. |

Counts: PASS 5, WARN 1, VARIANCE 0, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE goal (observe-path fidelity, structural isolation, solution-independent behavioral tests, loud-at-boot class guard) and every AC-01…AC-10 is present in SPECIFICATION and traced in RISK-TEST-STRATEGY. |
| Addition | (none) | P3 (5-field config-snapshot family) lands in-scope per SCOPE OQ-1 → resolved IN-SCOPE by `reviews/uni-zero-scope-review.md`; not an unapproved expansion. New resolver methods (`registry_for`/`pending_for`/`services_for`) have live consumers on the observe path — not an inert seam (checked against pattern #5611). |
| Decision | #925 kept open (ADR-005) | SCOPE SR-09 asked the architect to reconcile subsume-vs-defense-in-depth in the ADR. ADR-005 verdict: NOT subsumed (orthogonal metrics plane). Human owns the close/keep call — docs correctly do not auto-close/auto-file. |
| Simplification | White-box exception for 2 config fields | `store_config` + `inference_config` lack a clean public surface; covered by AC-08 boot assertion + wiring-pin unit, recorded as a documented AC-06 exception (SCOPE OQ-3 resolution). Rationale documented and enumerated, not silently omitted. |

## Variances Requiring Approval

None. No VARIANCE or FAIL. One WARN for awareness (does not require approval to proceed, but the human should confirm before delivery):

- **WARN — #800 fixture dependency underpins the first-class deliverable.** The behavioral isolation suite (Goal 3 / AC-06) and the INV-C1/C2 config-parity proof are the primary acceptance gate, and both are specified to *extend* the OPEN #800 multi-slug HTTP fixture rather than fork one (SR-08, ARCH OQ-3, SPEC Dependencies). Fixture owner is unconfirmed. If #800 slips or its fixture shape diverges, the primary gate for this feature has no vehicle. **Recommendation:** confirm #800 status/owner before Session 2 delivery; the docs correctly flag this but leave it unresolved.

## Detailed Findings

### Vision Alignment
Goal #5519 names, verbatim as an OSS-in-scope invariant (not enterprise deferral): "one cloud (single tenant) serves N projects... each fully isolated (own DB, vector index, hash chain, analytics; **no cross-project sharing in OSS**)" and "One isolation seam across local AND cloud — `resolve_store(request)` is the single funnel... cross-project contamination structurally impossible." The SCOPE problem statement establishes this invariant is **currently not met** on cloud for the transcript, knowledge-read, and config paths (the observe path resolves only the store per vnc-038; every other handle stayed daemon-global). The feature completes that funnel at the seam — the exact mechanism the goal prescribes — rather than patching instances.

- Advances goal #5519's north-star C0 ("full intelligence-pipeline fidelity over HTTPS == local"): AC-07 asserts HTTPS observe-path fidelity equals local UDS.
- Closes a live security/privacy leak (P2, SR-07/R-09): observe-path briefing/search/compact reading the wrong project's *persisted* knowledge — adjacent to integrity goal #5474 (contradictory/foreign data never reaching an agent; persistable via distillation, so the contamination is durable, not transient — R-15).
- Honors **Architectural Principle 3** (resolution after identity — `ProjectKey` parsed pre-write, then per-slug resolution on the funnel), **Principle 6** (no wire/client change; adapter is not infra — NFR-3/NG-5), and **Principle 7** (in-memory hot path — O(1) `Arc`-clone `*_for`, no DB at query time — NFR-1).
- Honors the goal's one-funnel discipline explicitly: FR-12 places resolution methods beside `resolve_store`/`adapter_for`, **no parallel slug→X side-map** (the vnc-034 #4974 ceremonial-funnel guard).

### Milestone Fit
This is on the personal-cloud cloud track (vnc-034 Wave-2 multi-project-routing lineage), completing the vnc-038 (#5082) per-request store funnel. Enterprise boundaries are correctly held out as seam-only and explicitly deferred: NG-1 (per-user/OAuth-subject isolation), NG-2 (multi-tenant), NG-7 (cross-project sharing / owner-store fan-out) — all matching goal #5519's "Out of scope (enterprise)" list. No future-milestone capability is built ahead of need. The AC-08 boot-assertion *class* census (guarding the whole "constructor-default never overwritten" field family, not just the enumerated 9) is not over-build: it is a cheap compile-time + boot guard proportional to the documented "inventory grew 2→9" recurrence risk (SR-02), mandated by SCOPE Goal 4 / FR-13.

### Architecture Review
ADR-001…005 each trace to named scope risks (SR-01…09) and the governing pattern #5629 (construction parity + funnel completeness + `Arc::ptr_eq` boot guard). The integration surface enumerates exact signatures (new `registry_for`/`pending_for`/`services_for` returning `RouteError`; reshaped `ObserveContext`; 3 appended `build_project_server` params) so downstream agents do not invent them. Error-boundary reasoning is sound: a `*_for` `Err` after `resolve_store` already succeeded is a boot-wiring contradiction → 500, never 404, made unreachable by the ADR-003 boot assertion. Consistency with SPEC and RISK holds (AC set 1:1; INV-T/K/C map identical; white-box exception fields identical). Minor, non-blocking: ARCH OQ-1 preserves a "if speed forces a cut, P1+P2 are the floor" fallback while SPEC OQ-1 records P3 as "RESOLVED — IN-SCOPE." Both agree P3 lands now; the architecture merely documents a contingency the SCOPE itself named as the negotiable boundary. No realignment needed.

### Specification Review
FR-1…FR-14 group cleanly under the three fix patterns (P1/P2/P3) plus vestigial deletion and the isolation-seam/boot-guard, each testable and named in an AC. All ten AC IDs are preserved from SCOPE for downstream trace. Every behavioral invariant AC (AC-01…AC-05) is explicitly **bidirectional at N≥2** per lesson #5348 / pattern #5172 — closing the one-directional false-GREEN hole (SR-06). The five SCOPE open questions are recorded as resolved (via the uni-zero scope review) with their dispositions, so downstream agents see the resolution in the authoritative doc — not only in a synthesizer brief. No stale contradiction across the three source docs was found.

### Risk Strategy Review
R-01…R-16 map every SR-01…09 forward and add test-design-level risks (false-GREEN vectors: hand-passed handles R-02, source-assertion census blind to argument threading R-03/#5427, seed-vs-derive-over-wire R-08/#5285, lenient test-double re-admitting the bypass R-06). The Invariant→Scenario map enforces bidirectional, assembled-production-wiring, N≥2 coverage with an explicit coverage-enumeration requirement (AC-06) that names the two white-box-only config fields — so a coverage gap is visible, not implied. Security-risk section correctly ranks the P2 knowledge-read leak highest and ties the distillation-persistence blast radius (R-15) to durability. One completeness caveat drives the WARN above: the strategy leans on the #800 fixture (R-12) for the primary gate, and #800 is open with an unconfirmed owner.

## Knowledge Stewardship
- Queried: `/uni-query-patterns` (context_lookup topic=vision, category=pattern) for prior vision-alignment patterns — surfaced #3742 (optional future branch in architecture must match scope deferral → WARN if diverges), #5611 (inert enterprise seam with no live consumer → prefer DEFER over ship-inert), #2298/#3337 (config/diagram-vs-spec divergence). Checked this feature against both governing watch-fors: #3742 does not fire (P3 is taken IN-SCOPE per resolved OQ-1, not deferred-then-contradicted); #5611 does not fire (the new `*_for` resolver methods have live observe-path consumers — not an inert seam). Also queried goal #5519 (personal-cloud, the OSS per-project isolation invariant this realizes) and #5474 (integrity, the P2 leak's durability dimension).
- Stored: nothing novel — this review found zero variances, so there is no recurring misalignment pattern to generalize. The relevant cross-feature patterns (#3742, #5611) already exist and neither was tripped. Feature-specific findings live in this report.
