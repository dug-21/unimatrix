# Alignment Report: infra-003

> Reviewed: 2026-06-27 (regenerated against the soundness-fix revision of the bidirectional 2×2 design)
> Artifacts reviewed:
>   - product/test/infra-003/architecture/ARCHITECTURE.md (+ ADR-001…004)
>   - product/test/infra-003/specification/SPECIFICATION.md
>   - product/test/infra-003/RISK-TEST-STRATEGY.md
>   - product/test/infra-003/IMPLEMENTATION-BRIEF.md
> Scope source: product/test/infra-003/SCOPE.md (+ SCOPE-RISK-ASSESSMENT.md)
> Vision source: product/PRODUCT-VISION.md; goal `personal-cloud` (#4946)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Drives the `resolve_store` isolation seam named as a goal #4946 success criterion; bidirectional 2×2 proves both directions of the integrity boundary in the release artifact |
| Milestone Fit | PASS | Point-in-time proof; N3 stays `partial`; N5/#788 adoption now a durable #788 linkage (R-16) — no over-build, no overclaim |
| Scope Gaps | PASS | All 5 SCOPE goals and AC-01…AC-14 carried into all three source docs |
| Scope Additions | WARN | AC-15 (MCP per-session isolation) is a sound, in-intent strengthening present in the spec/acceptance-map but **not yet reflected in SCOPE.md** (SCOPE lists AC-01…14). Documentation reconciliation only — no human design decision required |
| Architecture Consistency | PASS | C1–C7 trace to FRs/ACs/risks/ADRs; the three soundness fixes (read-as-barrier, per-session isolation, non-substring markers) are consistent across spec, risk, and ADR-002/003 |
| Risk Completeness | PASS | 18 risks; all 12 SRs traced; false-GREEN weighted dominant; three former hazards now resolved-by-design; R-15/R-16 carry concrete #815/#788 linkage |

**Status counts:** 5 PASS · 1 WARN · 0 VARIANCE · 0 FAIL.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE goal (1–5) and AC-01…AC-14 is represented in spec FRs and architecture components C1–C7 |
| Addition (WARN) | AC-15 — MCP per-session isolation (SPEC AC-15 / FR-07.3 / C-13; RISK R-17) | The two MCP handshakes must each use their **own** `Mcp-Session-Id`; a crossed/reused session would mis-attribute the isolation under test (a false verdict, R-17). This is a **correctness sub-property of the already-approved MCP-write goal (SCOPE Goal 4)**, not new feature scope. SCOPE.md still enumerates AC-01…14, so the acceptance set has drifted to 15 in the spec. Acceptance-map is authoritative; **reconcile SCOPE.md to AC-01…15**. See Variances/awareness below. |
| Strengthening | Durability barrier reworked: unsound aggregate `du` ("A grew AND B grew") barrier removed; marker-keyed **read-as-barrier** (bounded retry-until-present; own-store timeout → INFRA never RED; wrong-store presence → RED); `store_size` demoted to liveness-only (SPEC FR-06/AC-10/C-08; RISK R-05 reclassified Crit→Med) | This **removes a false-RED soundness bug**: the old aggregate `du` barrier was satisfied by the first of a store's two writes and raced the second under `tokio::spawn` + `synchronous=NORMAL`. A net robustness gain within the same intent (a sound durability proof). No scope change. |
| Strengthening | Four **mutually non-substring** markers `infra003-{obs,mcp}-{a,b}-<run>` (SPEC NFR-05/FR-07.1/C-14; RISK R-18) | Closes a `LIKE '%marker%'` cross-match false-GREEN that mere "distinct" markers did not. Refines the existing four-marker requirement; no scope change. |
| Resolved | Slug B = `isolation-b`; R-15 (#815) and R-16 (#788) now carry concrete GitHub linkage | Prior WARN (slug B) remains resolved. R-15/R-16 advanced from feature-doc rows to in-PR-lockstep (#815 comment) and durable standing-lane (#788 comment) linkages — strengthening delivery discipline. |

## Variances Requiring Approval

**None.** No design deviation needs a human decision. One documentation-reconciliation item is flagged for awareness:

1. **WARN (reconcile, do not approve) — AC numbering drift.** SCOPE.md lists AC-01…14; the SPECIFICATION acceptance-map and ACCEPTANCE-MAP carry AC-01…**15** (AC-15 = MCP per-session isolation). AC-15 is a sound correctness guard within SCOPE Goal 4's already-approved MCP-write surface, not a new capability, so it does not require approval — but the SCOPE document should be updated to AC-01…15 (and the spec's "AC-IDs mirror SCOPE.md" note refreshed) so the two stay in sync for downstream traceability. Per the design intent, the acceptance-map is authoritative; this is a tidy-up.

## Detailed Findings

### Vision Alignment
PASS. Goal #4946 names "**One isolation seam across local AND cloud** — `resolve_store(request) -> Arc<Store>` … project identity comes from the transport … NEVER the request payload" as an explicit success criterion and frames 1-client:1-project as a "knowledge-INTEGRITY boundary." infra-003 drives exactly that seam and the bidirectional 2×2 proves both directions of the boundary in the **release artifact** — closing the symmetric failure (B mis-resolving into A) a one-directional test passed GREEN on. The soundness revision deepens this without altering intent: the read-as-barrier removes a false-RED that would have undermined the proof's credibility, per-session isolation (R-17) prevents a false *attribution* of the very isolation under test, and non-substring markers (R-18) prevent a false-GREEN cross-match. All map to architectural principles #1 (hash-chain integrity) and #3 (identity resolution at the seam). The integrity framing remains goal-grounded — no over-elevation of defensive structure.

### Milestone Fit
PASS. Still an explicitly **point-in-time** proof: N3 (#5161) stays `partial` (SPEC NFR-04; RISK R-14/R-16), N5/#788 mechanics out of scope, H2 deferred, shipped routing exercised unmodified. R-16 is now a durable #788 linkage (a posted comment requiring N5 to adopt this gate), which makes the "point-in-time → maintained" hand-off a tracked obligation rather than a doc note — strengthening, not scope expansion.

### Architecture Review
PASS. C1–C7 remain coherent and now carry the three soundness fixes consistently: C5 is the marker-keyed read-as-barrier (no aggregate `du`); C4 runs two independent handshakes each with its own captured `Mcp-Session-Id`; C6 reads four mutually non-substring markers. ADR-002 (read primitive + markers) and ADR-003 (bidirectional MCP probe) are the cited sources for these and align with the spec. The brief's earlier "spec is authoritative on query form" note still holds for the illustrative `count(*)` vs canonical `≥1 row` difference (cosmetic).

### Specification Review
PASS, with the one WARN above. FR-01…FR-08 and AC-01…AC-15 are internally consistent and testable. The new/changed requirements are sound:
- **FR-06 / AC-10 / C-08 (read-as-barrier):** correctly states there is **no** aggregate barrier, writes are strictly sequential per store, the positive-control query is a bounded retry-until-present whose marker becoming queryable *is* the durability proof, own-store pre-deadline miss = INFRA, at-deadline miss = RED, and the cross-store negative is gated on PRESENT. This is the sound model and it pins the `tokio::spawn` + `synchronous=NORMAL` race that the old `du` barrier mishandled.
- **FR-07.3 / AC-15 / C-13 (per-session isolation):** each `/v1/{slug}/mcp` probe captures and uses its own `Mcp-Session-Id`; no cross-route reuse; handshake/session failure = INFRA, wrong-store landing = RED.
- **NFR-05 / FR-07.1 / C-14 (non-substring markers):** the `infra003-{obs,mcp}-{a,b}-<run>` construction is explicitly mutually non-substring with a shared per-run nonce, with the rationale (LIKE substring read) stated.
The only inconsistency is the AC count vs SCOPE.md (WARN). The open architect-confirmation items (retry deadline value, `context_store` vs `context_correct`, `topic_signal` stability) are appropriately scoped as tester/architect implementation details, not unresolved scope.

### Risk Strategy Review
PASS, and notably rigorous. The register is now 18 risks with the three soundness fixes reflected as resolved-by-design and traced:
- **R-05 reclassified Critical→Med:** the unsound aggregate `store_size` barrier is replaced by the marker-keyed read-as-barrier; residual is correct INFRA-vs-RED discrimination + a bounded retry — a well-reasoned downgrade, not a dismissal (residual scenarios retained).
- **R-17 (crossed/reused `Mcp-Session-Id`, High):** correctly distinguished from R-01 ("handshake doesn't work") as "handshake works but with the wrong session," with own-session-per-route resolution and INFRA-vs-RED discipline.
- **R-18 (marker substring collision, Med):** correctly distinguished from R-12 (SQL/LIKE metacharacters) and resolved by the non-substring marker set.
- **R-15 / R-16:** concretized with in-PR-lockstep (#815 comment) and durable standing-lane (#788 comment) linkages.
All 12 SRs trace to at least one risk; SR-03 now maps to the read-as-barrier resolution, SR-07 to R-08+R-18, SR-10 to R-01/02/03/17. The failure-mode table is exit-state-discriminated (GREEN/RED/INFRA/SKIP), and crossed-session is listed as "structurally excluded." Coverage summary totals (4 Crit / 8 High / 4 Med / 2 Low = 18) are internally consistent.

## Awareness Notes (no human decision required)
- **AC numbering drift (the WARN):** reconcile SCOPE.md to AC-01…15; acceptance-map is authoritative meanwhile.
- **R-16 remains the highest-value carry item:** the gate's value decays to zero post-merge unless N5/#788 adopts it into the recurring lane — now tracked via the #788 comment; ensure delivery honors it.
- **R-15 in-PR lockstep:** the new-smoke-script invariant must be updated in the **same** PR that adds `multi-tenant-isolation-smoke.sh` (cross-linked on #815), or the invariant trips as a late surprise.

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns -- found #3742 (optional-future-branch divergence → WARN; not triggered — no in-scope branch is also marked deferred), #3337 (architecture-diagram vs spec string divergence; checked — spec/acceptance-map is canonical, illustrative ARCH queries are presence-equivalent), #2298 (config semantic divergence; N/A to a test-only feature).
- Stored: nothing novel to store -- the recurring "soundness strengthening within approved scope creates an AC-numbering drift between SCOPE and the acceptance-map" is a mild instance of the existing #3742 alignment pattern (source docs diverging from the scope doc) and does not warrant a new entry. The infra-003-specific fixes (read-as-barrier, per-session isolation, non-substring markers) are feature-specific test-design corrections, better captured as delivery/test patterns at retrospective than as vision-alignment knowledge.
