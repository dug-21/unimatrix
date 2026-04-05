# Alignment Report: bugfix-523

> Reviewed: 2026-04-05
> Artifacts reviewed:
>   - product/features/bugfix-523/architecture/ARCHITECTURE.md
>   - product/features/bugfix-523/specification/SPECIFICATION.md
>   - product/features/bugfix-523/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Hardening work directly supports intelligence pipeline reliability and production safety goals |
| Milestone Fit | PASS | Wave 1A / W1-4 NLI infrastructure hardening; no future-milestone scope pulled in |
| Scope Gaps | PASS | All four SCOPE.md items (NLI gate, log downgrade, NaN guards, session sanitization) are fully addressed |
| Scope Additions | WARN | Architecture introduces an ADR file reference and an explicit log-level test strategy decision that SCOPE.md delegated to the implementation phase — not a functional addition, but escalation of a deferred decision |
| Architecture Consistency | PASS | All four fixes are consistent with stated ADRs; integration surface table matches functional requirements |
| Risk Completeness | PASS | All six SCOPE-RISK-ASSESSMENT risks are traced to R-series risks in RISK-TEST-STRATEGY.md; all have test scenarios |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | None | All four SCOPE.md goals have corresponding FR entries and ACs |
| Addition | SR-03 resolution (log-level test strategy) | SCOPE.md identified SR-03 as a risk for spec to resolve; architecture made the binding decision (behavioral-only, no tracing-test harness) rather than deferring to spec/implementor. SPECIFICATION.md then offered both options with a preference, creating a two-document tension on who owns the decision. |
| Simplification | AC-04 / AC-05 log-level assertion | SCOPE.md prescribed full tracing-level AC-04 as written; architecture and spec agree to behavioral-only coverage. Rationale documented (lesson #3935, ADR-001(c) entry #4143). Acceptable if human confirms. |

---

## Variances Requiring Approval

### WARN-1: SR-03 Decision Ownership Split

**What**: SCOPE.md (SR-03) says the spec must resolve whether AC-04 requires a log-level assertion or behavioral-only coverage. The architecture document made this decision unilaterally (behavioral-only), citing lesson #3935 and a risk of `tracing-test` harness instability. SPECIFICATION.md then offered both options (Option A = tracing-test preferred, Option B = behavioral-only fallback) and deferred final selection to the IMPLEMENTATION-BRIEF — which does not yet exist.

**Why it matters**: The architecture has committed to behavioral-only in its Gate 3b language ("Any gate feedback requesting log-level assertions must be escalated to the Bugfix Leader, not unilaterally added by the implementation agent"). The specification simultaneously marks Option A (tracing-test) as "preferred". These two positions are in tension. At Gate 3b, the tester will face ambiguity about whether AC-04 behavioral-only is accepted or requires escalation.

**Recommendation**: Before delivery begins, the Bugfix Leader must resolve this in the IMPLEMENTATION-BRIEF with a single authoritative statement. Either:
- Accept behavioral-only (Option B) explicitly and update RISK-TEST-STRATEGY.md R-11 to reference the decision entry by ID; or
- Require Option A (tracing-test) and revise the architecture's Gate 3b acknowledgment language accordingly.

No functional fix is required to source documents — only decision clarity in the IMPLEMENTATION-BRIEF.

---

## Detailed Findings

### Vision Alignment

The product vision identifies several active intelligence and security concerns relevant to this batch:

- **NLI pipeline reliability**: The vision (W1-4, Wave 1A, WA-0) depends on the NLI inference tick running correctly and without tick congestion. Item 1 (NLI gate) directly addresses a 353-second observed tick under NLI load when `nli_enabled=false` — a production reliability defect in the Wave 1A infrastructure.

- **Config integrity**: The vision's domain-agnostic config externalization (W0-3, dsn-001) means operators configure fusion weights and threshold fields via TOML. Item 3 (NaN guards) closes a silent failure mode where a misconfigured NaN propagates into the scoring pipeline rather than failing fast at startup. This supports the vision's reliability posture.

- **Security surface**: The vision's security gap table lists session identity and untrusted client input as High-severity concerns. Item 4 (sanitize_session_id) closes the last UDS dispatch arm without the guard — consistent with the vision's commitment to narrowing the untrusted input surface ahead of the W2-3 OAuth milestone.

- **Observability**: The vision's learning layer depends on meaningful operational signals. Item 2 (log downgrade) removes warn spam (40-50 lines/tick) that degrades signal quality for operators monitoring the NLI tick pipeline.

No vision principle is contradicted. The batch is narrowly scoped to correctness and operational reliability — fully consistent with the vision's emphasis on trustworthiness and production safety.

### Milestone Fit

This batch is maintenance hardening against Wave 1A / W1-4 NLI infrastructure (already complete per vision roadmap). The defects addressed were discovered during ongoing operation of that infrastructure. No Wave 2 or Wave 3 capabilities are introduced:

- No new MCP tools (explicitly excluded in SCOPE.md Non-Goals)
- No schema changes
- No new dependencies
- No API surface changes
- NLI code is gated, not extended or removed

The batch correctly targets the current operational milestone (Wave 1A running in production) without pulling forward Wave 2 deployment or Wave 3 GNN training scope. Milestone discipline is maintained.

### Architecture Review

The architecture document is internally consistent and maps cleanly to SCOPE.md.

**Item 1 gate placement**: The architecture specifies the insertion sequence using structural landmarks (`// === PATH B entry gate ===`, after `run_cosine_supports_path` returns, before `get_provider().await`). This is the correct treatment of SR-01 — structural landmarks rather than line numbers, accommodating drift. ADR-001 (entry #4017) compliance is explicitly argued.

**Item 3 loop structure**: The architecture correctly distinguishes Group A (11 inline f32/f64 guards), Group B (6 fusion weights in loop), and Group C (2 phase weights in loop). The loop-body dereference form (`!value.is_finite() || *value`) vs. inline form (`!v.is_finite() || v`) is documented. This distinction is important for R-10 regression risk.

**SR-03 decision**: Architecture commits to behavioral-only log-level coverage (no `tracing-test`). The rationale (harness instability per lesson #3935, cost-benefit asymmetry) is sound. The architecture's Gate 3b language ("must be escalated to the Bugfix Leader, not unilaterally added") is appropriately protective. However, this conflicts with the specification's "Option A is preferred" statement (see WARN-1 above).

**ADR file reference**: ARCHITECTURE.md references `ADR-001-hardening-batch-523.md` as a file artifact. Per CLAUDE.md project rules, ADRs live in Unimatrix (via `/uni-store-adr`), not as files. This reference should point to a Unimatrix entry ID, not a filename. This is a documentation inconsistency, not a functional defect, and does not require blocking the delivery — but should be corrected.

**Integration surface table**: All integration points match the functional requirements in SPECIFICATION.md. No new interfaces. `sanitize_session_id` signature, `ERR_INVALID_PAYLOAD` constant, `ConfigError::NliFieldOutOfRange` variant, and `rayon_pool.spawn()` are all correctly identified as unchanged.

### Specification Review

The specification delivers complete, well-structured functional requirements for all four items.

**AC completeness**: All 29 ACs from SCOPE.md are reproduced verbatim in SPECIFICATION.md. The spec explicitly states "Every AC is required; none may be deferred." This is the correct treatment of SCOPE.md's acceptance criteria.

**FR-01 structural landmark**: The spec describes the insertion landmark in terms of the `// === PATH B entry gate ===` comment and the `run_cosine_supports_path(...)` call completion — not line numbers. This is consistent with SR-01's design recommendation.

**FR-03 field checklist**: The spec provides a complete 19-field table with types, guard forms, and valid ranges. Fields 1–11 (Group A) and Fields 12–19 (Groups B and C) are clearly distinguished. This satisfies SR-02's recommendation to use a named checklist rather than a count.

**SD-01 (SR-03 mitigation)**: The specification introduces SD-01, offering Option A (tracing-test) and Option B (behavioral-only) with Option A as "preferred" and delegating the final choice to the IMPLEMENTATION-BRIEF. This conflicts with the architecture's committed behavioral-only position. See WARN-1.

**Scope exclusion list**: SPECIFICATION.md's "NOT in Scope" section matches SCOPE.md Non-Goals exactly, including the post-PR-516 field exclusion and integer field exclusion. No functional scope creep detected.

**Domain model section**: The spec includes a Domain Models section that correctly defines Path A/B/C semantics, `nli_enabled` flag semantics, `category_map` miss behavior, `sanitize_session_id` contract, and the NaN trap. This is appropriate documentation for implementors and is not a scope addition.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is thorough and correctly traces all six SCOPE-RISK-ASSESSMENT risks.

**Scope risk traceability**: The traceability table at the end of RISK-TEST-STRATEGY.md maps all six SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-06) to R-series risks. All mappings are accurate:
- SR-01 → R-01, R-02 (gate boundary, ADR-001 compliance)
- SR-02 → R-03, R-06, R-07 (19-field coverage, presence count, field-name strings)
- SR-03 → R-11 (log-level gate report acknowledgment)
- SR-04 → R-07 (helper pattern usage)
- SR-05 → R-04 (guard insertion order)
- SR-06 → Integration Risks section

**R-11 references entry #4143**: The risk strategy cites Unimatrix entry #4143 as the authority for behavioral-only log-level coverage. The architecture cites lesson #3935. These appear to be different entries for related content. The tester will need to verify entry #4143 exists and covers the behavioral-only decision — if it does not, the gate report citation will be invalid. This is a minor traceability concern, not a blocking defect.

**R-05 coverage gap acknowledged**: The risk strategy correctly acknowledges that the wrong-site downgrade risk (R-05) is detectable only by code review, not by test assertion. The gate report requirement ("log level for non-finite cosine site verified by code review, not test assertion") is appropriate.

**Security risks section**: Item 4's injection risk is well-characterized. The blast radius analysis (from "arbitrary string as session key" to "ERR_INVALID_PAYLOAD, no registry call") is accurate. Item 3's NaN-as-DoS framing is correct — the security posture improves from "silent corruption" to "fail-fast diagnostic".

**Knowledge stewardship**: RISK-TEST-STRATEGY.md queried Unimatrix and found all relevant entries before authoring. No new cross-feature patterns were stored (correctly — the patterns cited are already stored). The stewardship is handled appropriately.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` (via `mcp__unimatrix__context_search`) for vision alignment patterns — found entries #2298 (config key semantic divergence), #3742 (deferred architecture branch scope addition WARN), #3337 (architecture diagram header divergence vs. spec). None directly apply to this batch; batch is a bugfix with no new abstractions or config keys.
- Stored: nothing novel to store — the SR-03 decision-ownership split pattern is feature-specific to batches that defer tracing-test decisions across design documents. It does not generalize as a cross-feature pattern without more instances. Pattern #3742 (architecture diverges from scope deferral) is the closest existing analog.
