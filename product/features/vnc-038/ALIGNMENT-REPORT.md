# Alignment Report: vnc-038

> Reviewed: 2026-06-17 (revision pass — post scope-revision: #735 fold-in, GATE-2 confirmation, ADR-006 tightening, ADR-008 added)
> Artifacts reviewed:
>   - product/features/vnc-038/architecture/ARCHITECTURE.md
>   - product/features/vnc-038/architecture/ADR-006-local-uds-identity-under-unified-resolver.md (revised — tightened)
>   - product/features/vnc-038/architecture/ADR-008-token-delivery-via-bundle-only.md (new)
>   - product/features/vnc-038/specification/SPECIFICATION.md
>   - product/features/vnc-038/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-038/SCOPE.md (+ SCOPE-RISK-ASSESSMENT.md)
> Vision source: product/PRODUCT-VISION.md; goal #4946 (`personal-cloud`)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances goal #4946 (`personal-cloud`); honors arch principles #6 (dumb client), #3 (capability checks post-identity), #8 (no secrets in any DB — reinforced by ADR-008). |
| Milestone Fit | PASS | Vinculum (MCP/connectivity) personal-cloud serving arc; no future-milestone capability built. Enterprise/RBAC/multi-tenant correctly deferred. |
| Scope Gaps | PASS | All six Goals + folded-in #735 carry-items and AC-01..AC-13 are carried into FR-01..FR-17 and the AC verification table. No SCOPE item dropped. |
| Scope Additions | PASS | No additions beyond SCOPE. ADR-006 (local binding), ADR-008 (token-via-bundle), #735 carry-items are all SCOPE-mandated (AC-10/11/12/13, NFR-06, RD-1/5), not net-new. |
| #735 Fold-In | PASS | CI-1/CI-2/CI-3 land on AC-11/12/13 + NFR-06; SR-06 sequencing collision dissolved by fold-in; R-11 superseded; mechanical cleanups (AC-12/13) correctly low-weighted. |
| Token-Delivery Posture (ADR-008) | PASS | Token never to stdout/logs; `v:2` bundle sole channel (cloud surface only); local non-regression gated by deployment context. Reinforces arch principle #8 + NFR-06. |
| Local-Binding Tightening (ADR-006) | WARN | Tightening (local bypasses the resolver; NOT a resolver key) is correct and GATE-2-grounded, but it diverges from one LITERAL phrase of goal #4946 ("single funnel … identical in local-UDS and cloud … no cloud-only isolation path the local install does not exercise"). Honors the goal's intent; supersedes the prior report's claim that local stays "on the same resolver." Surface to human. |
| Architecture Consistency | PASS | ADR-001..008 trace to SR/RD/Goal/AC; integration surface matches cited code; observe-funnel, v:2, local-binding, and token decisions consistent across all three docs. |
| Risk Completeness | PASS | R-01..R-15 cover every SR; new R-13 (local-direct-binding guard), R-14 (token-to-stdout), R-15 (mechanical cleanups) added; N=2 proof (#4974), parity atomicity (#4956), call-site audit (#2398) intact. |

Counts: 8 PASS, 1 WARN, 0 VARIANCE, 0 FAIL.

> Revision delta vs prior report: prior pass was 6 PASS / 0 WARN. This pass adds the #735 fold-in check and the ADR-008 token-posture check (both PASS), and re-classifies the local-binding handling as **WARN** because the ADR-006 tightening now contradicts goal #4946's literal "single funnel identical across local AND cloud" wording (the prior report asserted local stays "on the same resolver"; the revised design says local bypasses it). The divergence is from the goal's phrasing, not its intent — see Detailed Findings.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE Goal/AC (AC-01..AC-13, incl. folded-in #735 CI-1/2/3) maps to an FR + AC verification method (see traceability below). |
| Addition | (none) | No requirement appears in source docs that is absent from SCOPE. ADR-006/008 and the #735 carry-items are SCOPE-mandated. |
| Simplification | Restart-to-apply (no live reload) | Rationale: SCOPE RD-4/NFR-05 — acceptable for single-dev, not-always-on deployment; removes the live-reload build risk. Carried faithfully. |
| Simplification | Hard cut, no migration logic | Rationale: SCOPE RD-1/RD-5 — zero existing CLOUD/CONTAINER-HTTP users; GATE-2 confirmed deletions are HTTP-only and local is unaffected. Residual = the "zero existing cloud served stores" assumption (OQ-5), correctly surfaced, not buried. |
| Simplification | Local bypasses the resolver (ADR-006 tightening) | Rationale: GATE-2 code-cited analysis — local STDIO (`main.rs:1158`)/UDS (`main.rs:859`) already open the path-hash store directly and never touch the resolver; routing them through it would be a NEW cross-store path AC-10 forbids. This is a de-scoping of the prior "self-register local as a resolver key" design, not a broadening. See WARN below. |

### Scope -> Source traceability (spot check, revision)

| SCOPE | Spec FR | AC |
|-------|---------|-----|
| Goal 1 (mandatory identity / delete default) | FR-01, FR-14 | AC-01, AC-09, AC-10 |
| Goal 2/4 (uniform one-command register) | FR-02, FR-03, FR-04, FR-05 | AC-02, AC-03, AC-04 |
| Goal 3 (server-composed v:2 bundle) | FR-06, FR-07, FR-08 | AC-05 |
| Goal 5 (per-slug observe, no pollination) | FR-09, FR-10 | AC-06 |
| Goal 6 (#766 root-cause guarantee) | FR-11, FR-12 | AC-07, AC-08 |
| #735 CI-1 (token-to-stdout, NFR-06, ADR-008) | FR-15 | AC-11 |
| #735 CI-2 (router.rs ≤500, NFR-09) | FR-16 | AC-12 |
| #735 CI-3 (public_url.rs cleanup) | FR-17 | AC-13 |
| ADR-006 tightening (local direct binding, NOT a resolver key) | FR-14, C-13 | AC-10 |
| SR-05 (reserved-slug coupling) | FR-13 | (covered in AC-01 grammar assertions + R-08) |

## Variances Requiring Approval

No VARIANCE or FAIL. One **WARN** for human awareness (not a blocker):

1. **What**: The revised ADR-006 establishes that the local STDIO/UDS install keeps a **direct path-hash store binding and is NOT routed through the unified resolver** (it is not a resolver key). Goal #4946 states the isolation seam is "`resolve_store(request) -> Arc<Store>` … the single funnel through which all data resolves, **identical in local-UDS single-project and cloud multi-slug modes**" and warns "no cloud-only isolation path the local install does not exercise."
2. **Why it matters**: On a literal reading, the revised design now has TWO store-binding mechanisms (cloud = resolver; local = direct binding), which is the opposite of one "identical" funnel. It also supersedes the *prior* alignment report's own assertion (that pass's line 55: AC-10 "keeps local UDS on the same resolver"). A reviewer comparing the two passes will see the position flip.
3. **Recommendation**: **Accept.** The tightening honors the goal's deeper invariant while diverging from one phrase, because:
   - The goal's load-bearing rule is *"project identity comes from the transport, NEVER the request payload … the resolved store handle is the sole write capability."* ADR-006 preserves this exactly — local identity is the transport-derived path-hash, the directly-bound `Arc<Store>` is the sole write capability, and local is single-store, so cross-project mis-targeting remains unrepresentable.
   - The "no cloud-only isolation path local does not exercise" clause was written to stop a cloud isolation mechanism local never proves. GATE-2 shows the shipped code (vnc-034 ADR-004) **already** binds local directly and never routed it through a resolver — so forcing local through the resolver would *create* an untested local-only-via-cloud-path code path, the exact hazard the clause guards against (AC-10 forbids it; R-13 is the guard test).
   - Local is single-store by construction (one socket/process, one directly-bound store); it has no cross-project surface to prove, so it forfeits nothing by not entering the resolver.
   - **Suggested follow-up (non-blocking):** goal #4946's "single funnel identical across local AND cloud" wording is now slightly stale relative to the realized two-mechanism design. A one-line goal-entry correction ("transport-derived identity is the invariant; the cloud realizes it via the slug resolver, local via direct path-hash binding — two mechanisms, one invariant") would keep the goal aligned with shipped reality. Human's call; out of this feature's scope to make.

## Detailed Findings

### Vision Alignment — PASS (with the ADR-006 WARN above)
- **Goal #4946 fit is direct.** The feature operationalizes the goal's destination clause: one cloud serves N projects, routed by an operator-declared slug in the URL path (`/v1/{slug}/...`), each fully isolated. Deleting the no-slug default (RD-5) and making `register` uniform (Goal 2/4) is exactly the goal's "register a project vs attach a client" separation, closing the one path where attach could mis-target.
- **Architectural principle #6 (client is an adapter, not infrastructure)** is the explicit spine — the dumb-client invariant (ADR-001, NFR-01, C-01), cited verbatim in SCOPE, ARCHITECTURE, and SPECIFICATION.
- **Architectural principle #8 (no secrets in any DB)** is reinforced by the ADR-008 token-delivery posture and NFR-06: the bearer token is never emitted to stdout/logs and travels only in the validated `v:2` bundle; token/cert stay as files on the data volume.
- **The transport-derived-identity invariant** (goal #4946's load-bearing rule) is preserved on BOTH surfaces — cloud via the slug resolver, local via the direct path-hash binding. The literal "single funnel" phrasing is the only divergence (WARN above), not the invariant itself.
- **Defensive framing check (per spawn guidance): PASS — discipline held through the revision.** The no-cross-pollination guarantee is consistently framed as a concrete *routing* property, never elevated to a vision/goal claim:
  - SCOPE line 72: "The integrity guarantee (no pollination) is framed as a concrete routing property, not elevated to a vision goal (per prior guidance to avoid over-stating defensive structure)." SCOPE Constraint: "Integrity is the basis, not access control … Framed as a routing guarantee, not a security/authz feature."
  - SPEC NFR-03 and C-02 repeat the framing ("routing property protecting the hash chain … not an authz feature").
  - The ADR-006 tightening *strengthens* this discipline: it removes a defensive over-build (routing local through the resolver "for uniformity") and explicitly notes local "does NOT get the resolver's single-funnel isolation proof for free — but it does not need it" (ADR-006 Consequences). This is the "avoid overstating defensive structure" lesson applied correctly — local isolation is structural (single store), not an elevated guarantee.
  - ADR-008 frames token redaction as a credential-exposure routing/delivery decision (cloud HTTPS posture), not a new security/authz model. No grand-vision inflation detected.

### Milestone Fit — PASS
- Squarely in the Vinculum (`vnc`) MCP-server/connectivity phase, advancing the `personal-cloud` goal. No future-milestone capability is built ahead of need: RBAC, per-slug authz, multi-tenant, OAuth, cross-project sharing, owner store, monetization — all explicitly Non-Goals, matching the goal's "Out of scope (enterprise / separate private repo)" list.
- The enterprise seam is preserved additively (NFR-03: "additive on the C6 `BearerValidator` seam"), satisfying the goal's "Enterprise extends, never re-architects" contract without building it now.
- **#735 fold-in does not expand milestone scope.** The three carry-items (token redaction, `router.rs` extraction, dead-code cleanup) all land on surfaces vnc-038 already reworks (first boot, `router.rs`); they are completion/hygiene of the same serving-arc work, not new milestone capability.

### #735 Fold-In Review — PASS (new check this pass)
- **CI-1 (token-to-stdout)** → ADR-008 / FR-15 / AC-11 / NFR-06. Correctly elevated from "one-line cleanup" to a *decision* (it commits the bundle as the sole token channel and removes the console channel). Reconciled with local (ADR-006): redaction is deployment-context-gated so local is functionally unchanged.
- **CI-2 (`router.rs` ≤500 lines)** → FR-16 / AC-12 / NFR-09. Correctly framed as a natural outcome of the route-grammar rewrite, not separate work.
- **CI-3 (`public_url.rs` stale `dead_code`)** → FR-17 / AC-13. Trivial cleanup.
- **Sequencing collision dissolved.** Prior SR-06 ("#735 collision — same router/boot surface, sequence after #735 lands") is RESOLVED-BY-FOLD-IN, not merely mitigated: there is no longer a separate effort on the same surface (SCOPE Dependencies; OQ-4 resolved; R-11 superseded; risk strategy SR-06→R-11 mapping correctly marked superseded with R-14/R-15 carrying the residual carry-item risk). vnc-038 closes #735. This is a clean removal of a coordination risk, consistent across SCOPE, ARCHITECTURE, SPEC, and RISK-TEST-STRATEGY.
- **Proportionality held.** AC-12/13 (mechanical) are correctly low-weighted (R-15 Low/Low); the security-bearing CI-1 (R-14) is weighted Med/Med with a dedicated security-surface analysis. No over- or under-weighting.

### Token-Delivery Posture Review (ADR-008) — PASS (new check this pass)
- ADR-008 makes the `v:2` bundle the **sole** token-delivery channel for the cloud/container HTTP surface and redacts/gates the `http/token.rs:101` print so the token never reaches stdout or `tracing` output. This is a faithful realization of NFR-06 and AC-11, and reinforces arch principle #8.
- **No parallel "also print it" fallback** — correctly noted that a fallback would re-open the exposure NFR-06 closes.
- **Local reconciliation is sound.** ADR-008 Decision point 5 + the OQ-2/OQ-6 open questions correctly flag that delivery must confirm whether `token.rs:101` is HTTP-first-boot-only or shared with local; if shared, redaction is gated by deployment context (not removed outright) so local stays functionally unchanged (AC-10). This is a delivery-level confirmation, not an unresolved alignment gap.
- **Risk coverage present** (R-14 + the security-surface "First-boot token emission" entry): stdout/log no-token assertion, sole-channel assertion, and local-non-regression assertion are all specified.

### Architecture Review — PASS
- ADR-001..008 each trace to a SR/RD/Goal/AC (ARCHITECTURE ADR table). The dual-side cut (Rust encoder + JS decoder + corpus) is one atomic change (ADR-002), consistent with the goal's strict-parity contract.
- The integration surface table cites exact current signatures (`Bundle`, `ProjectKey`, `parse_project_key`, `MultiProjectRouter`, `ObserveContext`, `RESERVED_SLUGS`, `token.rs:101`, local boot binds at `main.rs:859`/`:1158`) with line refs, and states the planned v:2 shape — downstream agents invent no names.
- Observe folded onto the per-request funnel with no boot-bound `resolve_store(Default)` survivor (ADR-003) directly answers the #4974 ceremonial-funnel risk.
- **ADR-006 tightening is internally consistent.** The revision is reflected uniformly: ARCHITECTURE (component table "Local STDIO/UDS binding … NOT routed through the resolver"; data-flow note "Local STDIO/UDS is not shown and not involved"; OQ-2 RESOLVED), SPEC (FR-14, C-13, OQ-2 resolved, Ubiquitous Language "NOT self-registered as a resolver key"), RISK (R-13 the GATE-2 guard, R-07 re-scoped, SR-04→R-13 re-pointed). No stale "local-as-resolver-key" wording survives in the source docs. The one residual stale reference is in goal #4946 itself (external to this feature) — flagged as the WARN follow-up.

### Specification Review — PASS
- FR-01..FR-17 cover all six Goals + the three folded-in #735 carry-items; the AC table pins each AC-01..AC-13 to an existing test surface (seam/funnel, parity corpus, project-lifecycle fixture, hook transport, first-boot token-surface, line-count/grep), satisfying the cumulative-test-infra constraint (NFR-07/C-09).
- Ubiquitous Language is precise and matches goal #4946 vocabulary, and was updated for the revision (local "direct path-hash store binding … NOT self-registered as a resolver key").
- "NOT in Scope" mirrors SCOPE Non-Goals one-for-one, including the tightened local-binding statement. No requirement exceeds SCOPE.
- N=2 proof is a first-class constraint (C-11), correctly rejecting an N=1 green as proof.

### Risk Strategy Review — PASS
- R-01..R-12 intact; revision adds **R-13** (local routed through the resolver — the load-bearing GATE-2 guard, Critical), **R-14** (first-boot token to stdout/logs — High), **R-15** (mechanical #735 cleanups — Low). R-07 correctly re-scoped to the HTTP seam (local split out to R-13); R-11 superseded with clean SR-06 traceability.
- The risks that matter for the goal's architectural invariants are all covered: client-compose closed set (R-01), ceremonial observe funnel (R-02), v:2 parity/partial-rollout (R-03/R-04), genesis-clobber against the sacred hash chain (R-05), cross-pollination at N≥2 (R-09), loud-first-boot (R-10), local-binding regression (R-13), credential-on-logs (R-14).
- Security surfaces (bundle decode trust boundary, slug at parse edge / path traversal, TOML-injection, **first-boot token emission**) enumerated with mitigations to test — consistent with arch principles #8 (no secrets in any DB) and #3 (capability checks post-identity).
- Prior lessons applied rather than re-derived (#4974, #4956, #2398, #4452, #4311). Coverage summary proportionality is correct (R-15 Low; R-13 Critical as the GATE-2 guard).

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search, topic=vision, category=pattern) for vision-misalignment patterns — same low-relevance, feature-specific divergences as the prior pass (#2298 config-key, #3337 diagram-header, #4617 export-hash), none a recurring vision pattern applicable here. Also re-pulled goal #4946 for the local/cloud single-funnel wording that drives this pass's WARN.
- Stored: nothing novel to store. The one generalizable mechanic this revision surfaces — "a GATE-2 code-cited impact analysis can flip a design from 'route local under the unified resolver' to 'local bypasses it,' and the goal-entry wording then lags the realized design" — is a single-feature reconciliation, not yet a cross-feature (2+) pattern. The governing lesson ("avoid overstating defensive structure") already exists and was the correct lens for this pass. If a second feature later diverges from a goal's literal-but-stale invariant wording while preserving its intent, that would justify a `vision` pattern ("re-verify goal-entry wording against realized design after a scope revision; flag stale invariant phrasing as WARN + follow-up correction, not VARIANCE").
