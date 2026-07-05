# Alignment Report: nxs-014

> Reviewed: 2026-07-05
> Artifacts reviewed:
>   - product/features/nxs-014/architecture/ARCHITECTURE.md
>   - product/features/nxs-014/architecture/ADR-001-chain-verify-core-placement.md
>   - product/features/nxs-014/architecture/ADR-002-weak-mode-threat-boundary.md
>   - product/features/nxs-014/architecture/ADR-003-correction-chain-population-and-verify-semantics.md
>   - product/features/nxs-014/specification/SPECIFICATION.md
>   - product/features/nxs-014/RISK-TEST-STRATEGY.md
> Scope: product/features/nxs-014/SCOPE.md (SETTLED, D-1..D-4)
> Scope risk: product/features/nxs-014/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goal #5474 (Knowledge Integrity); capability #5478 (KI-CHAIN-XV)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Weak mode moves `context_correct` from *violating* Architectural Principle 1 to satisfying it literally; advances the Knowledge Integrity goal's north-star leg. |
| Milestone Fit | PASS | Nexus-phase storage/schema work. Correctly declines to build the post-RBAC MCP tool and strong cascade/anchor — factored-not-built. Good milestone discipline. |
| Scope Gaps | PASS | All seven SCOPE ACs (AC-01..07) carried into the spec; each goal traced to an FR. No gaps. |
| Scope Additions | PASS | Spec adds AC-08..AC-12 / FR-04 as risk-hardening; all trace to SCOPE-RISK-ASSESSMENT items (A-1, SR-01, SR-05, D-4, NFR-02). One genuinely new behavior (FR-04 reject empty predecessor hash) is defensive and justified — see below. |
| Architecture Consistency | PASS | Verify-core placement in `unimatrix-store` (ADR-001) realizes D-4's transport-agnostic mandate without a dependency cycle; ADR-002/003 reaffirm frozen scope, invent no new policy. |
| Risk Completeness | PASS | Register covers the two ship-broken paths (two-site half-fix, Deprecated-predecessor false alarm), README re-drift, and the frozen-hash tripwire. Threat model matches the goal's integrity backbone. |

**One variance requires a human decision** — a knowledge-governance item (capability #5478 wording vs "proven"), not a code or design defect. See below.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | — | None. SCOPE Goals 1-4 → FR-01/02, FR-05, FR-11, FR-10; ACs 01-07 all carried. |
| Addition | FR-04 / AC-08 — reject correction when `original.content_hash` is empty, naming `original_id` | New write-path behavior not named in SCOPE. Traces to SCOPE-RISK-ASSESSMENT A-1 (doug-reviewed doc). Defensive guard preventing a laundered bad-state from becoming a silent legacy-skip. Acceptable — bounded, justified, prevents a real integrity hole. |
| Addition | AC-09..AC-12 (CLI contract, no-MCP-tool, frozen-hash, no-migration) | Verification for constraints already settled in D-4 / Non-Goals / NFR. Hardening, not new surface. Acceptable. |
| Simplification | Strong cryptographic cascade + external HEAD anchor deferred to north-star | Rationale: hash-format change with legacy-collision + test-vector + external-reference blast radius; cascade without an out-of-DB anchor does not reach true tamper-evidence anyway (ADR-002). Documented, consistent across scope/arch/risk. Aligned with goal #5474's explicit north-star framing. |
| Simplification | Forward-only legacy (no backfill migration, schema stays v30) | Rationale: a backfill blesses only current stored content, cannot retro-verify history — a false baseline (D-2). Documented. |

## Variances Requiring Approval

### V-1 — Capability #5478 (KI-CHAIN-XV) says tamper-EVIDENT; weak mode delivers tamper-RECORDED. Reconcile before marking `proven`. (VARIANCE)

1. **What**: SCOPE Goal 5 is "Advance capability KI-CHAIN-XV (#5478) ... to `proven`, with behavioral evidence." But capability #5478's `name` and `why` are worded in tamper-**EVIDENCE** terms — `why` literally reads "correction history is tamper-RECORDED (id-linked) but *not tamper-EVIDENT*; a silently altered superseded entry is undetectable." Weak mode delivers exactly the tamper-RECORDED state the `why` names as *insufficient*. The capability's `done_when`, by contrast, *is* fully satisfied by weak mode (populate `previous_hash = predecessor.content_hash` + version increment + fail-loud chain-verify). So the acceptance clause and the name/why disagree about what "proven" means.
2. **Why it matters**: If #5478 is marked `proven` on nxs-014 evidence without touching its wording, the capability graph will assert tamper-EVIDENCE is proven while the delivered guarantee — and the corrected README (AC-06) — say tamper-RECORDED. That reproduces the exact overclaim GH #912 exists to close, one level up in the knowledge graph, and trips the "avoid overstating defensive structure" lesson. Affects goal #5474 (Knowledge Integrity) integrity of the claim-floor/north-star model.
3. **Recommendation**: Before #5478 is marked `proven`, either (a) re-word #5478 to tamper-RECORDED and split a sibling `KI-CHAIN-XV-STRONG` (cascade + external anchor) carrying the tamper-EVIDENT promise onto the north-star, or (b) mark #5478 `proven` only against a re-scoped tamper-RECORDED `done_when` and open the strong sibling. This is a knowledge-governance action for the human/vision session (doug flagged it non-blocking for delivery; it is blocking for the `proven` marking). It does not gate the code work.

## Detailed Findings

### Vision Alignment (PASS)

The central question posed for this review — is shipping WEAK mode and deferring the strong chain + anchor a variance against Architectural Principle 1 and the Knowledge Integrity goal? — resolves to **aligned, not a variance**, on three independent grounds:

1. **Principle 1 is about population, not cascade.** Principle 1 (PRODUCT-VISION.md:56): "`content_hash` and `previous_hash` on every entry — never skipped, backdated, or made optional." The *current* code (`write_ext.rs:539` `previous_hash: String::new()`) is the live violation. Weak mode populates `previous_hash` on every correction — moving the code from violation to literal compliance. Principle 1 nowhere requires folding `previous_hash` into the digest (that is the strong cascade). So deferring strong mode does not offend Principle 1; wiring weak mode *satisfies* it.

2. **The goal's own structure names the cross-version chain as north-star.** Goal #5474 splits into claim-floor (proven) and north-star (never terminal). Its north-star text: "a cross-version cryptographic hash chain so a tampered superseded entry is detectable (currently UNWIRED — previous_hash empty on correction, violates Architectural Principle 1; tracked GH #912)." nxs-014 delivers the *wiring* of that north-star leg; the strong cascade + poison-resistance + contradiction-free serving remain north-star by the goal's own definition. Advancing a north-star leg without completing the never-terminal north-star is the intended shape.

3. **The engine promise stays truthful.** ADR-002 + AC-06/AC-07 correct the README from "tamper-evident" to tamper-RECORDED and pin the threat model durably (README + ADR). This closes the unbacked-claim half of GH #912. The vision Story ("hash-chained for integrity") and goal claim-floor (per-entry `content_hash`, append-only audit, authoritative supersession chain) remain true and are explicitly not under-sold (FR-11).

Honest-README assessment (AC-06/AC-07): **adequately closes the overclaim.** AC-06 removes the unqualified "tamper-evident" correction-chain claim and states the accidental-corruption + API-surface-tamper (tamper-recorded) scope; AC-07 requires the threat model recorded in a durable place (README section and/or ADR-002) so downstream agents cannot silently re-upgrade it. The claim boundary is pinned to ADR-002, not left to prose — this is the correct durability mechanism.

### Milestone Fit (PASS)

Nexus (`nxs`) phase = storage/vectors/embedding/schema. The feature is a store-crate write-path fix plus a store-crate verify core and a server CLI subcommand — squarely in phase. Milestone discipline is *positive* here: the architecture explicitly factors the verify core to be MCP-wrappable but builds **no** MCP tool (D-4, FR-09, Out-of-Scope 1), gating that on RBAC (an Alcove/enterprise concern). It does not build future-milestone capability it does not need. Prior pattern #3742 (optional future branch must match scope deferral, WARN if arch/risk diverge from scope) — checked and clean: the north-star deferral is stated identically across SCOPE Non-Goals, ARCHITECTURE "Explicitly Out of Scope," and RISK-TEST "zero tests on a branch that does not exist."

### Architecture Review (PASS)

ADR-001 places `verify_entries` (pure, I/O-free) in the leaf crate `unimatrix-store`, with `validate_hashes` and the CLI as thin callers — one integrity oracle enforcing the content-hash-AND-chain-link check, satisfying D-4's transport-agnostic mandate by construction and avoiding a dependency cycle (server already depends on store). This is the correct resolution of SR-03. ADR-002 freezes `compute_content_hash` and pins the tamper-RECORDED boundary. ADR-003 mandates fixing *both* literal sites (struct `:539-540` and INSERT bind `:582-583`) with a DB-read-back test — directly closing SR-06, the headline two-site half-fix. The architecture invents no new policy; each decision locates a settled scope choice in code. Open Q1 (`query_all_entries` must return Deprecated predecessors) is correctly surfaced to spec/dev and covered by R-02.

### Specification Review (PASS)

Eleven FRs + six NFRs trace cleanly to SCOPE and the risk doc; the traceability table maps every AC to source and risk. The false-green guard is explicit and correct: AC-01/AC-02 mandate DB read-back, not in-memory-record assertion (C-04) — the exact failure mode SR-06/R-01 warn about. Frozen-hash constraint (C-01, NFR-01, AC-10) is pinned as a review tripwire against inline scope-creep into strong mode. Legacy tolerance (C-02, FR-06, AC-03) matches the existing `import/mod.rs:429` convention, preventing the SR-02 false-positive alarm on forward-only data. The one behavior beyond SCOPE (FR-04 empty-predecessor rejection) is defensive and traced to A-1 — see Scope Additions.

### Risk Strategy Review (PASS)

Twelve risks, correctly prioritizing the two "ships broken" paths as Critical (R-01 two-site half-fix; R-02 Deprecated-predecessor false alarm) with DB-read-back and loader-status guard scenarios. The frozen-hash tripwire (R-06), README re-drift (R-11), and import atomicity on a tampered corpus (R-05, ROLLBACK-proven-by-state) are covered. The Security Risks section correctly states the out-of-tier limitation (a coordinated multi-row DB-write rewrite defeats detection) as a *documented* boundary and explicitly instructs testers **not** to write a test asserting detection of it — preventing a test that would misrepresent the guarantee. This matches the threat model in ADR-002 and the goal's integrity backbone (Principles 1 and 2). Complete for the vision's integrity concerns.

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search, topic=vision) for alignment patterns -- most relevant hit #3742 "optional future branch in architecture must match scope intent — WARN if architecture/risk diverge from scope deferral"; applied and found clean (north-star deferral is stated consistently across scope/arch/risk, so no WARN). #2298/#3337 (config/diagram divergence) not applicable to this feature.
- Stored: nothing novel to store -- the one variance (capability wording promises tamper-EVIDENCE while the delivered/settled guarantee is tamper-RECORDED, blocking a `proven` marking) is feature-specific to #5478 and already flagged in SCOPE + ADR-002 for the vision session; it does not yet generalize across 2+ features. The reusable guard it *would* generalize to — "a capability's name/why must be reconciled to the delivered guarantee before it is marked proven, or 'proven' becomes an overclaim" — is worth storing only if it recurs; noting here for the next occurrence rather than storing a single-instance pattern.
