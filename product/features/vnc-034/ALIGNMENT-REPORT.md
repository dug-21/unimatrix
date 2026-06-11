# Alignment Report: vnc-034

> Reviewed: 2026-06-11
> Artifacts reviewed:
>   - product/features/vnc-034/architecture/ARCHITECTURE.md
>   - product/features/vnc-034/specification/SPECIFICATION.md
>   - product/features/vnc-034/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-034/SCOPE.md
> Risk source: product/features/vnc-034/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md + goal #4946 (personal-cloud)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances goal #4946 (personal-cloud); honors all 8 architectural principles |
| Milestone Fit | PASS | Targets the OSS personal-cloud destination; enterprise capabilities held as seams, not pre-built |
| Scope Gaps | PASS | All 14 SCOPE goals + 6 contracts traced into spec FRs and AC-IDs |
| Scope Additions | PASS | No unrequested capability; the one new artifact (contract parity fixtures) is SCOPE-mandated |
| Architecture Consistency | PASS | C1–C6 lock identically across the three docs; integration signatures grounded in real code |
| Risk Completeness | PASS | All 11 scope risks (SR-01..11) traced to R-01..13 with coverage requirements |

**Counts:** PASS 6 · WARN 0 · VARIANCE 0 · FAIL 0

**Load-bearing product bet (1-client:1-project permanence): ALIGNED with the personal-cloud vision.** See dedicated analysis below. One item is surfaced for explicit human confirmation — not a variance, but a product decision the documents themselves correctly escalate.

## The Load-Bearing Bet: 1-client:1-project Permanence

The review was charged to scrutinize whether the permanent 1-client:1-project OSS/cloud boundary aligns with the personal-cloud vision. It does, and the documents frame it correctly.

**Vision basis (goal #4946, verbatim):** "1 client : 1 project is a knowledge-INTEGRITY boundary, not access control — a single client instance binds exactly one project; per-client fan-out across projects is gated... 1:1 removes the agent's ability to mis-target at all. RBAC is precisely what lets enterprise relax it safely." The goal entry explicitly states the OSS destination is "N clients : 1 project : 1 tenant" and that "True N:N client↔project is enterprise-only and requires RBAC."

**Why this aligns rather than conflicts with the personal-cloud / solo-developer vision:**

1. **The boundary is integrity, not artificial scarcity.** The basis is that a client bound to project B writing into project A's hash chain permanently and unrollbackably corrupts A (Architectural Principle #1 — hash chain integrity is immutable; #2 — append-only audit). The 1:1 bound makes mis-targeting *unrepresentable at the transport*, not merely rejected. This is the structural realization of the vision's immutability principles, not a deviation from convenience.

2. **The solo developer's need is fully met.** Goal #4946 and SCOPE both state a different project = a separate client instance / container, and same-project multi-connection (N clients : 1 project — the multi-LLM case) is explicitly allowed. The personal-cloud vision ("one container, one bearer token, one command") is about *operational simplicity*, not about one client fanning across projects. The documents do not trade away any solo-developer capability the vision promised.

3. **Enterprise relaxation is additive, never a re-architecture.** C5/C6 and NFR-09 hold the `BearerValidator` seam and slug-as-scope so that enterprise N:N (RBAC-gated `unimatrix_project` claim) extends the same seams. This satisfies the vision's non-negotiable "Enterprise extends, never re-architects."

**The documents correctly escalate the one genuine product question.** SPECIFICATION §"For the human" item A1 and SCOPE-RISK-ASSESSMENT A1 both flag the *one* assumption on which permanence rests: "a solo developer never legitimately needs one client across two projects." This is the right thing to surface — it is a product-posture confirmation, not a design defect. The vision (goal #4946) supports the bet; the human should affirm it is the intended permanent posture. The documents do not over-claim certainty; they present it as a load-bearing bet, which is the correct treatment.

**Conclusion:** No variance. The 1:1 boundary is faithful to the vision's integrity principles and to the personal-cloud destination as written in goal #4946. The single human-confirmation item is appropriately raised by the source documents themselves.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | All 14 SCOPE goals (Group A 1–5, Group B 6–10, Group C 11–14) map to spec FR-A1..A10, FR-B1..B9, FR-C1..C7, FR-X1..X5. |
| Gap | (none) | All 6 shared contracts C1–C6 specified as WHAT in spec and locked as concrete interfaces in architecture §3. |
| Addition | Cross-stack fingerprint parity fixtures (C1/C2 sub-deliverable) | NOT unrequested scope — SCOPE Constraints + Acceptance ("C2 fingerprint format identical server↔client") and SR-02 mandate it. Architecture §8 / spec NFR-14 / AC-CT-C2 realize it. Aligned. |
| Simplification | Process-per-project explicitly NOT built (in-process multi-store) | Rationale: SCOPE C4 + Principles #6/#7 — separate processes would tax single-binary and cost N× model memory; safe Rust + single-funnel provides isolation. Documented, aligned. |
| Simplification | No slug-listing endpoint in OSS (OQ-B → ADR-004) | Rationale: smallest attack surface; operator hands slug out-of-band. Consistent with "no unauthenticated endpoint beyond /health." Flagged for human confirm in architecture §10.2 (appropriate). |

Every SCOPE Non-Goal (no proxy TLS, no plaintext-to-client, no CA/SAN validation, no cross-project sharing, no OAuth/RBAC, no multi-tenant, no client multiplexing, no non-Linux server, no rate-limiting/`/metrics`, no CLAUDE.md append in init, no local-UDS change, no `npm link`) is restated verbatim in spec "NOT in Scope" and respected in the architecture. No Non-Goal is silently breached.

## Variances Requiring Approval

None.

## Items for Human Confirmation (raised by the source documents, not variances)

These are open questions the documents themselves correctly escalate. They do not block alignment; they are product/delivery decisions for the human.

1. **A1 — 1:1 permanence posture (load-bearing product bet).** Confirm that permanent 1-client:1-project is the intended OSS posture and a solo developer never legitimately needs one client across two projects. Vision goal #4946 supports it. *Recommendation: affirm — the integrity rationale is sound and enterprise relaxation is seam-additive.*
2. **OQ-A/B/C/D** (architecture §9 resolved via ADR-001..006; §10 surfaces residual confirmations): bundle wire form, no-slug-listing UX, Wave-1 `/v1/tools/` additive alias, wave-to-issue mapping. All resolved toward additive/minimal-surface defaults consistent with the vision. *Recommendation: accept the resolutions; they are vision-aligned.*
3. **Container HTTP-enable env-var name vs baked config** (architecture §10.1) — a delivery detail touching the C3 env contract. *Recommendation: defer to delivery; no vision impact.*

## Detailed Findings

### Vision Alignment

The feature is a direct, well-scoped advance on goal #4946 ("Individual developer-friendly deployment"). It turns the partially-shipped substrate (W2-2 HTTPS transport #658, nan-014 container #629, vnc-027 TS client #680) into a reachable cloud — exactly the "cloud track to the destination" the goal entry names, "now consolidated under umbrella vnc-034."

All 8 architectural principles are honored, several materially:
- **#1 hash-chain integrity / #2 append-only audit** — the entire 1:1 integrity rationale (C5) exists to protect these; the seam makes cross-chain corruption unrepresentable.
- **#3 capability checks at the service layer** — C6 keeps token-authorizes / slug-scopes / cert-secures as three concerns; the `BearerValidator` seam resolves identity after transport auth (FR carried; NFR-09).
- **#6 single binary, zero required infra** — explicitly invoked to reject process-per-project (architecture §1.1); container is the operator side, client is a pure-JS adapter < 250 KB (FR-B1/B8).
- **#7 in-memory hot path** — per-slug hot caches rebuilt by tick live *inside* the seam method (architecture §C4, R-01 scenario 4), not in a new edge.
- **#8 no secrets in any DB** — token/cert as files mode `0600`, never in any DB (NFR-05/06, AC-W1-S5).
- **#5 graceful degradation** — `ProjectKey::Slug` under the Wave-1 `DefaultResolver` returns `RouteError::UnknownProject`, not a panic (R-01 scenario 3); unwritable `/data` fails loud-and-actionable, no `.unwrap()` (NFR-03, R-11).

The feature does not touch the intelligence pipeline (self-learning, proactive-delivery goals) and correctly does not claim to — it is infrastructure under the personal-cloud goal, and marking those goals N/A is proportionate.

### Milestone Fit

Targets the OSS personal-cloud milestone precisely. Enterprise capabilities (multi-tenant, OAuth/JWT/RBAC, proxy TLS termination, cross-project sharing) are uniformly held as documented-but-degenerate seams (NFR-08/09, C6), never pre-built — satisfying Milestone Discipline (no future-milestone capability built early) and the vision's "extends, never re-architects." The two-wave cut (Wave 1 serving+client validated, Wave 2 routing additive against the validated base) is disciplined: it builds only what the destination needs now and proves the seam before populating it.

### Architecture Review

The architecture locks C1–C6 once across all three documents with no drift — the umbrella's entire reason for being. Integration surface (§7) cites exact existing signatures with file:line grounding (router.rs, tls.rs, token.rs, auth.rs, listener.rs, config.rs, project.rs), so the design meets real code, not a guess. The central spine — one `resolve_store` seam, two resolvers (slug | path-hash), identical local + cloud — is the highest-leverage decision and is treated as proof-grade (single funnel, no bypass, transport-derived identity). This directly answers the deferred-seam trap (SR-07) and the local-parity non-negotiable (SR-08 / NFR-10) by routing the Wave-1 single store *through* the seam, not around it (FR-X5, AC-W1-X1). Consistent and vision-faithful.

### Specification Review

Every SCOPE goal and contract is traced to a testable FR with an AC-ID, and each AC names a verification method. The cardinality block (spec §"Cardinality (load-bearing)") restates the 1:1 / N:1 model verbatim from goal #4946. NFR-09 explicitly invokes the ADR-007 vnc-025 `session_key` "documented-but-degenerate seam" precedent for enterprise-seam treatment — correct reuse of project pattern, not novel invention. The spec correctly states WHAT and defers HOW (wire form, seam injection) to the architecture, with the four genuinely-open OQs flagged where they bite acceptance. No requirement contradicts a Non-Goal.

### Risk Strategy Review

All 11 scope risks (SR-01..SR-11) trace cleanly to test-strategy risks R-01..R-13 with a Scope Risk Traceability table. The two security trust boundaries the scope-risk assessment flagged fix-before-merge (slug parser SR-09 → R-03; bundle parser SR-09 → R-05) are carried as security acceptance criteria (AC-W2-R6, AC-W1-C9) with allowlist/​schema/​length-cap coverage. The load-bearing integrity risk (R-06: 1:1 enforced at transport, not config) has a coverage requirement asserting *unrepresentability, not runtime rejection* — exactly matching the vision's "removes the agent's ability to mis-target at all." The critical seam-swap risk (R-01) carries 4 scenarios including a single-funnel source assertion and a resolver-swap test. Coverage is complete and risk-proportionate; no vision-relevant risk is unaddressed.

## Knowledge Stewardship
- Queried: /uni-query-patterns + context_search for vision alignment patterns (tag `vision`) -- hits (#2298, #3337, #4617) are feature-specific divergence patterns (config semantics, diagram/spec header drift, export hash scope), none generalize to this umbrella's scope/seam concerns. No applicable recurring alignment pattern found.
- Stored: nothing novel to store -- the variances surfaced here are feature-specific (1:1 permanence is a single load-bearing product bet, not a recurring cross-feature misalignment class). No generalizable vision pattern emerged. The documents' practice of self-escalating the load-bearing bet to the human is already strong; no anti-pattern to capture.
