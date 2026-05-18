# Alignment Report: vnc-016

> Reviewed: 2026-05-18
> Artifacts reviewed:
>   - product/features/vnc-016/architecture/ARCHITECTURE.md
>   - product/features/vnc-016/specification/SPECIFICATION.md
>   - product/features/vnc-016/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope sources: product/features/vnc-016/SCOPE.md, product/features/vnc-016/SCOPE-RISK-ASSESSMENT.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Closes a silent false-negative in detection pipeline; directly supports hash-chain integrity and correctness non-negotiables |
| Milestone Fit | PASS | W1B maintenance / AC-12 closure; no future-wave scope pulled in |
| Scope Gaps | PASS | All nine ACs from SCOPE.md are addressed across all three source documents |
| Scope Additions | PASS | No deliverables added beyond SCOPE.md; AC-09 Rust negative companion is an elaboration of the stated AC, not new scope |
| Architecture Consistency | PASS | Four components map 1:1 to SCOPE.md deliverables; ADR-001 present; error-swallowing constraint acknowledged and explicitly deferred |
| Risk Completeness | WARN | R-06 (trust-level gate silently skips `feature_entries`) is a Phase 2a discovery absent from SCOPE-RISK-ASSESSMENT; adequately addressed in spec (C-01) and risk strategy, but the `unwrap_or_else` follow-up issue (R-05 recommendation) is not linked to a concrete GitHub issue |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | SCOPE.md AC-09 "Rust unit test" | Architecture and spec both include a positive-path and negative-path Rust unit test; SCOPE.md names only "a Rust unit test" as AC-09. The two-test elaboration is additive and correct — the negative companion is what prevents an unscoped JOIN from passing the positive path. No new scope; better coverage than the minimum AC-09 definition. |

No scope gaps. No scope additions.

## Variances Requiring Approval

None. All checks resolve to PASS or WARN. The single WARN does not block delivery.

## Detailed Findings

### Vision Alignment

The product vision states: "Hash chain integrity is immutable" and "Audit log is append-only and complete." The `DependencyOnDeprecated` rule is part of the `context_cycle_review` detection pipeline, which is the mechanism by which agents observe knowledge-base health. A silent false-negative in this rule means stale Prerequisite edges (deprecated-source entries) pass undetected through cycle reviews. vnc-016 restores the intended invariant.

The vision also states: "The integrity chain is the product's defensible moat." The `query_stale_prerequisite_edges_for_cycle` bug directly undermines the defensibility of the detection layer — a graph-health check that silently fires `vec![]` on a column-name error is, in effect, disabled. Fixing it and establishing a Rust unit test as the regression guard is congruent with the vision's posture on correctness.

No intelligence pipeline, proactive delivery, or Wave 2+ capabilities are touched. The feature is narrowly scoped to the defect and its test coverage.

### Milestone Fit

vnc-016 is categorized as a Vinculum phase feature (MCP server layer). Its immediate predecessor is vnc-015 (Typed Edge Write Path, W1B-1, COMPLETE). vnc-016 closes AC-12 (PARTIAL) from that feature. W1B-2 (`context_graph` tool) is the next in-flight item (#596–598). The vision dependency graph does not require vnc-016 to complete before W1B-2, but delivering it before W1B-2 ships reduces the risk of detection-rule silent failures compounding across a larger graph surface. Timing is appropriate.

### Architecture Review

**Component 1 (SQL Fix)**: Single-token change (`fe.feature_cycle` → `fe.feature_id`) with confirmed analysis of caller count (two callers: the definition and one call site). The impact surface is well-bounded. The architecture documents that the `unwrap_or_else` pattern in `tools.rs:2169-2177` is intentionally unchanged — this is a correct architectural boundary decision for vnc-016, as changing the error-handling strategy requires independent design review.

**Component 2 (Rust Unit Test)**: Placed in `read.rs mod tests`, co-located with the function under test. The placement rationale references ADR-001 (Unimatrix #4449). The seeding sequence maps directly to the SQL query's three-way JOIN, testing the exact conditions the fix must satisfy. Both positive-path (assert non-empty, assert pair value) and negative-path (assert empty when no `feature_entries` row) tests are described.

**Component 3 (Harness Client Extension)**: `feature_cycle: str | None = None` keyword-only addition to `context_store()`. The `if feature_cycle is not None: args["feature_cycle"] = feature_cycle` guard correctly ensures the key is absent (not null) when not provided — consistent with `StoreParams.feature_cycle: Option<String>` without `#[serde(default)]`. Backward compatibility is explicit: existing call sites are unaffected.

**Component 4 (Integration Tests)**: Two pytest functions in the existing `test_tools.py` vnc-015 section. The architecture provides complete pseudocode for both tests, including the cycle_id binding pattern, step ordering, and assertion structure. The memoization bypass (`force=True`) is required and documented.

**Concern noted but not a variance**: The architecture acknowledges that `unwrap_or_else` in `tools.rs:2169-2177` remains unchanged. The RISK-TEST-STRATEGY (R-05) recommends a follow-up issue to change the log level from WARN to ERROR for this failure mode. The architecture does not reference a specific GitHub issue number for this follow-up. This is not a delivery blocker but creates a gap: if the follow-up is not tracked, the R-05 concern will surface again in a future feature without attribution.

**Pattern #3337 check** (architecture diagram informal headers diverging from spec — a known recurring alignment issue): ARCHITECTURE.md and SPECIFICATION.md use consistent terminology throughout. Component names, function signatures, and AC references are identical between the two documents. No header or naming divergence detected.

**Pattern #3742 check** (optional future branch in architecture must match scope intent — WARN if architecture and risk diverge from scope deferral): The architecture's only explicit deferral is the `unwrap_or_else` → ERROR log-level change. This deferral appears identically in SCOPE.md (Non-Goals), ARCHITECTURE.md (Known Architectural Constraint), and RISK-TEST-STRATEGY (R-05 recommendation). No divergence.

### Specification Review

All nine ACs from SCOPE.md map to named FRs (FR-01 through FR-05) and NFRs. The spec resolves all three SCOPE.md OQs (OQ-01 confirmed yes, OQ-02 confirmed no, OQ-03 confirmed no) with a rationale for each. The constraints (C-01 through C-10) are the primary value-add of the spec — they translate risk findings into hard implementation requirements. Key constraints:

- **C-01** (from SR-02): `feature_cycle` must be at `context_store` time for entry A — non-deferrable, non-reorderable. Critical constraint, correctly elevated.
- **C-02** (from SR-03): `force=True` is mandatory in both integration tests. Hard constraint.
- **C-03**: Observation seeding must use the identical `cycle_id`.

The spec's domain model section (entries, feature_entries, graph_edges, stale Prerequisite edge, feature cycle, DependencyOnDeprecatedRule, context_cycle_review, analytics write path) provides a clear reference for implementers encountering the codebase for the first time on this feature.

**FR-02.4** (no new test infrastructure — use existing store-layer test harness) directly implements the product principle that test infrastructure is cumulative. This is consistent with the `CLAUDE.md` rule: "extend existing fixtures and helpers, never create isolated scaffolding."

**Phase 2a discovery (R-06)**: The spec includes `agent_id="human"` as a hard requirement in the 7-step scenario (FR-04.2 step 2, FR-04.2 step 7). The SCOPE-RISK-ASSESSMENT does not include this risk, but the RISK-TEST-STRATEGY elevates it to High severity / High priority (R-06). The spec closes the gap via C-01 and the explicit agent_id specification. This is appropriate handling of a design-phase discovery.

### Risk Strategy Review

The risk strategy covers 10 numbered risks with priority classification (4 Critical, 3 High, 2 Medium, 1 Low). All SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-06) are traceable to RISK-TEST-STRATEGY risks via the traceability table. One additional risk (R-06: trust-level gate) is a Phase 2a discovery not in SCOPE-RISK-ASSESSMENT — it is addressed.

The risk strategy references historical Unimatrix entries for precedent (entry #4177 for tautological assertion, entry #4445 for the exact `unwrap_or_else` silent-failure pattern). This is correct use of the knowledge base and supports the vision principle of capturing lessons for future agents.

**R-05 follow-up issue gap (WARN)**: R-05 states "the `unwrap_or_else` in `tools.rs:2169-2177` is out of scope for vnc-016 but should be flagged in a follow-up issue for hardening." No issue number is referenced. This is not a delivery blocker, but without a concrete tracking artifact the hardening recommendation will not be acted on. Human should create a GitHub issue for this before closing vnc-016.

**R-09 (Low, serde missing-key vs null)**: Resolved by architecture analysis alone (no test scenario required). This is an appropriate disposition for a Low-priority risk with a clear architectural argument.

**Fail-first verification (R-01, R-03)**: The risk strategy correctly requires that both the Rust unit test and the integration test positive path FAIL against the unfixed `read.rs` before the SQL fix is applied. This is the gold standard for a bugfix test — it confirms the test is exercising the defect, not a different code path. Implementers must be instructed to verify this before merging.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #2298 (config key semantic divergence), #3337 (architecture diagram header divergence from spec), #3742 (optional future branch in architecture vs scope intent), #3426 (formatter golden-output regression), #3771 (KnowledgeConfig parallel list defaults). Applied #3337 and #3742 checks explicitly in Architecture Review section.
- Stored: nothing novel to store — this feature's alignment is clean and the patterns match existing entries. The R-06 trust-level discovery (agent_id="human" required for `feature_entries` write path) is feature-specific to the `UsageService.record_access` trust gate and does not generalize beyond tests that depend on `feature_entries` population. It is already captured in Unimatrix entry #103 (ADR-007). No new cross-feature pattern to store.
