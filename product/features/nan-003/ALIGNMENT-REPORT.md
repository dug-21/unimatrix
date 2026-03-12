# Alignment Report: nan-003

> Reviewed: 2026-03-11
> Artifacts reviewed:
>   - product/features/nan-003/architecture/ARCHITECTURE.md
>   - product/features/nan-003/specification/SPECIFICATION.md
>   - product/features/nan-003/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/nan-003/SCOPE.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | VARIANCE | PRODUCT-VISION.md defines nan-003 as "Project Initialization" (schema, ONNX, `npx unimatrix init`); SCOPE.md delivers Claude Code skills only — heavy lifting re-assigned to nan-004 |
| Milestone Fit | PASS | Platform Hardening milestone goals served; `.claude/` scaffolding aspect of vision is delivered |
| Scope Gaps | PASS | All 14 SCOPE.md acceptance criteria are addressed across all three source documents |
| Scope Additions | WARN | SPECIFICATION.md FR-05(c) adds `outcome` to the CLAUDE.md category guide; SCOPE.md's enumerated list and ARCHITECTURE.md block template both omit `outcome` |
| Architecture Consistency | WARN | Two cross-document inconsistencies: (1) `outcome` category in spec vs. arch template; (2) spec treats ADR-002 sentinel fallback as an open question despite architecture having decided it |
| Risk Completeness | PASS | All 7 scope risks traced; integration risks, edge cases, and security risks mapped with test scenarios |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | None identified | All SCOPE.md ACs (AC-01 through AC-14) are addressed in source documents |
| Addition | `outcome` category in CLAUDE.md block | SPECIFICATION.md FR-05(c) lists `outcome` in the category convention guide. SCOPE.md AC-01 enumerates: decision/pattern/procedure/convention/lesson-learned. ARCHITECTURE.md Component 3 block template matches SCOPE.md (no `outcome`). Spec and arch diverge here. |
| Simplification | Existing-entries threshold unresolved | ARCHITECTURE.md open question #3 proposes "≥3 active entries" threshold for re-seed warning. RISK-TEST-STRATEGY.md R-10 references this threshold. SPECIFICATION.md FR-14 omits any numeric threshold. Rationale: architect left it open for spec to decide; spec did not decide. |

---

## Variances Requiring Approval

### VARIANCE 1 — Vision Scope Drift: nan-003 Re-Scoped From Installation to Skills

**What**: PRODUCT-VISION.md defines nan-003 as:
> *"Project Initialization — First-run experience for new repos. Schema creation, ONNX model download, initial configuration, `.claude/` scaffolding. Target: `npx unimatrix init`."*

The delivered SCOPE.md defines nan-003 as two Claude Code skills (`/unimatrix-init`, `/unimatrix-seed`) that set up the CLAUDE.md Unimatrix block and seed the knowledge store. Schema creation, ONNX model download, initial configuration, and `npx unimatrix init` are **explicitly excluded** as Non-Goals, deferred to nan-004.

**Why it matters**: The PRODUCT-VISION.md is the canonical milestone roadmap. Delivering nan-003 as "onboarding skills" against a vision entry of "project initialization" means:
1. The vision's nan-003 goals (schema, ONNX, `npx unimatrix init`) are now split across nan-003 + nan-004, but the PRODUCT-VISION.md does not reflect this split — it still shows all of it under nan-003.
2. If nan-004 is not delivered, the vision's description of nan-003 is never fully satisfied. The feature is marked complete against an incomplete scope.
3. Traceability: a future developer reading PRODUCT-VISION.md would not find `/unimatrix-init` or `/unimatrix-seed` — they would find "schema creation" and "npx unimatrix init."

**Recommendation**: Human approval required. Two options:
- **Accept**: Update PRODUCT-VISION.md to split the description — nan-003 = "Onboarding Skills (CLAUDE.md setup + knowledge seeding)" and nan-004 = "Installation, packaging, schema, ONNX, npx." Keeps the roadmap accurate.
- **Reject**: Return scope to align with vision description — bring schema/ONNX/install into nan-003. This is the higher-risk path given SCOPE-RISK-ASSESSMENT.md concerns about the bootstrap paradox.

The SCOPE-RISK-ASSESSMENT.md note — *"Scoping out `settings.json` wiring (to nan-004) is correct, but leaves a documentation gap"* — indicates the researcher recognized this split and accepted it. The PRODUCT-VISION.md simply hasn't caught up.

---

## Detailed Findings

### Vision Alignment

The PRODUCT-VISION.md Platform Hardening milestone lists:
> *"nan-003: Project Initialization — First-run experience for new repos. Schema creation, ONNX model download, initial configuration, `.claude/` scaffolding. Target: `npx unimatrix init`."*

The SCOPE.md introduces nan-003 as "Unimatrix Onboarding Skills" delivering `/unimatrix-init` (CLAUDE.md append + agent scan) and `/unimatrix-seed` (knowledge seeding). The skills explicitly exclude: "Installing the Unimatrix binary, ONNX model download, or wiring settings.json — those are **nan-004**."

**Strategic alignment is strong** for the portion delivered: the three-layer chain (CLAUDE.md awareness → skill invocation → agent behavior) is consistent with the vision's "Files define the process / Unimatrix holds the expertise / Hooks connect them" architecture. The `.claude/` scaffolding element of the vision's description is delivered.

**Strategic misalignment exists** on the re-assignment: the core "project initialization" concepts from the vision (schema creation, binary install, `npx unimatrix init`) do not appear in nan-003 at all. They are assumed to be in nan-004, but this assumption is not documented in PRODUCT-VISION.md.

One additional note: the SCOPE.md and ARCHITECTURE.md correctly call out that the `uni-init` agent handles brownfield bootstrap from `.claude/` files, while nan-003 skills handle new repo onboarding via CLAUDE.md. This distinction is well-handled and aligns with the vision's "three-layer chain" principle (alc-001 research conclusion confirmed).

### Milestone Fit

nan-003 targets the "Platform Hardening & Release" milestone. The milestone goal is: "Unimatrix works — now make it shippable. First multi-repo deployments require... documentation."

The skills delivery (CLAUDE.md scaffolding + knowledge seeding) directly enables first multi-repo deployments. A developer in a new repo can now: (1) get CLAUDE.md wired via `/unimatrix-init`, (2) populate foundational knowledge via `/unimatrix-seed`. These are necessary for multi-repo adoption.

**PASS**: The partial scope (skills layer) fits the milestone even though the full vision description of nan-003 is not delivered.

### Architecture Review

The ARCHITECTURE.md is well-structured and addresses all SCOPE.md requirements:
- Component 1 (`/unimatrix-init`) covers AC-01 through AC-05, AC-11, AC-12, AC-14
- Component 2 (`/unimatrix-seed`) covers AC-06 through AC-09, AC-13
- Component 3 (CLAUDE.md block template) provides a concrete deliverable template, **which does NOT include `outcome` in the category table** — consistent with SCOPE.md's five-category enumeration
- Component 4 (agent scan algorithm) covers AC-04 with specific pattern-check logic
- Component 5 (seed state machine) addresses SR-01 (primary scope risk) via explicit STOP gate phrasing
- Component 6 (entry quality gate) addresses the What/Why/Scope requirement

Six ADRs are logged and traced back to scope risks. All scope risks (SR-01 through SR-07) have ADR coverage.

**Two cross-document issues identified**:

**Issue A** — `outcome` category discrepancy: ARCHITECTURE.md Component 3 block template shows five categories (decision, pattern, procedure, convention, lesson-learned). SPECIFICATION.md FR-05(c) adds `outcome` as a sixth. SCOPE.md AC-01 aligns with the architecture (five categories). The spec addition is not authorized by SCOPE.md or ARCHITECTURE.md.

**Issue B** — ADR-002 sentinel fallback not reflected in spec: ARCHITECTURE.md ADR-002 "Versioned Sentinel + Head-Check Fallback" decides that the skill must check both the start AND the last 30 lines of CLAUDE.md when the file exceeds 200 lines. SPECIFICATION.md open question 2 asks: "Should `/unimatrix-init` add a secondary idempotency check?" — treating as undecided what the architecture already decided. RISK-TEST-STRATEGY.md R-04 test scenario 4 correctly references ADR-002 (e.g., "Verify the SKILL.md instruction includes the head-check fallback"). The spec should close this gap by incorporating the ADR-002 decision.

### Specification Review

The SPECIFICATION.md is comprehensive (27 FRs, 9 NFRs, 14 ACs) and closely follows the SCOPE.md structure. All 14 acceptance criteria from SCOPE.md are mapped to FR-level requirements. The domain model section is clear and consistent with the architecture.

**Three items warrant attention**:

1. **FR-05(c) adds `outcome`**: The category guide in the CLAUDE.md block includes `outcome` per FR-05(c), but SCOPE.md AC-01 and ARCHITECTURE.md Component 3 both enumerate only five categories. This is an unexplained addition. (See Scope Additions table.)

2. **FR-14 missing threshold**: FR-14 specifies calling `context_search` to check for existing seed entries and warning if found. It does not specify the numeric threshold at which the warning triggers. ARCHITECTURE.md open question #3 proposes "≥3 active entries" and RISK-TEST-STRATEGY.md R-10 scenario 2 references this threshold. The spec should resolve this explicitly.

3. **Open question 2 vs ADR-002**: As noted in Architecture Review, the spec should absorb ADR-002's sentinel tail-check decision and close open question 2.

One positive addition in the spec: Open question 3 from the architecture ("if human rejects entire Level 0 batch, should skill halt or re-propose?") is addressed in the RISK-TEST-STRATEGY.md Failure Modes table ("Print '0 entries stored. Re-invoke with more specific guidance.' and DONE") but not in the spec FRs. The RISK-TEST-STRATEGY.md effectively backstops the spec gap here; however, having the behavior in the spec (not just the test strategy) would be cleaner.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is thorough and well-structured. All 7 scope risks from SCOPE-RISK-ASSESSMENT.md are traced through architecture decisions to test scenarios:

| SR | Architecture Decision | Risk(s) | Coverage |
|----|----------------------|---------|----------|
| SR-01 | ADR-001 (STOP gates) | R-01, R-07, R-08 | 4 scenarios; all state transitions |
| SR-02 | ADR-002 (sentinel fallback) | R-04 | 4 scenarios; all sentinel locations |
| SR-03 | Platform constraint (accepted) | R-01–R-03, R-06–R-08 | Mitigated by STOP gates + quality gate |
| SR-04 | Prerequisites section | R-13 | Code review scenario |
| SR-05 | FR-07/AC-12 disambiguation | — | No risk assigned; correct |
| SR-06 | ADR-003 (pre-flight first) | R-05, R-09 | 2 scenarios |
| SR-07 | EXISTING_CHECK state | R-10 | 2 scenarios |

The security section correctly identifies the lowest blast-radius risk (agent file read → terminal output only) and the highest (seed entries stored via `context_store` becoming visible to future agents). The quality gate + human approval chain is correctly identified as the primary defense against adversarial README content.

The integration risks section identifies an unresolved behavior for mid-batch `context_store` failures: "Should the skill report the failure and continue with remaining entries, or halt?" This is a legitimate gap — the spec (FR-27) only says "fail gracefully with clear error"; the behavior for partial batch failures is unspecified.

**PASS with one note**: the mid-batch `context_store` failure path behavior should be resolved in the specification before implementation.

---

## Knowledge Stewardship

- Queried: `/query-patterns` for `vision` topic, `pattern` category — **no results** (empty category)
- Queried: semantic search for "vision alignment patterns scope drift additions gaps" — **no results**
- Stored: nothing novel to store — the scope drift pattern (PRODUCT-VISION.md not updated when SCOPE.md deliberately re-assigns scope) is the most generalizable finding, but one feature is insufficient evidence for a pattern entry. Flag for review after nan-004 completes: if nan-004 SCOPE.md also diverges from vision description, store as a recurring pattern ("vision roadmap lags SCOPE.md when scope is split across features").
