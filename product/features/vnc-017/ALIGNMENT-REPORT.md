# Alignment Report: vnc-017

> Reviewed: 2026-05-18
> Artifacts reviewed:
>   - product/features/vnc-017/architecture/ARCHITECTURE.md
>   - product/features/vnc-017/specification/SPECIFICATION.md
>   - product/features/vnc-017/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Agent: vnc-017-vision-guardian

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly serves the integrity chain and hash-chain provenance non-negotiables |
| Milestone Fit | PASS | Wave 1B scope; builds on shipped vnc-015/vnc-016 infrastructure; no future-wave scope introduced |
| Scope Gaps | PASS | All SCOPE.md goals, non-goals, and constraints are addressed in source docs |
| Scope Additions | WARN | Architecture adds fan-in ceiling (ADR-004, N=50) and source-validation guard that are not explicitly present in SCOPE.md; both are architecture decisions that close SCOPE.md open questions, but the ceiling value is a new design choice requiring human awareness |
| Architecture Consistency | WARN | SPEC FR-07 contains `Ok(true)/Ok(false)` return-contract table that contradicts Architecture ADR-003 and the actual `Result<(), EdgeRedirectError>` signature — a documentation defect identified by the RISK strategy as Critical R-01 but not corrected in the spec itself |
| Risk Completeness | PASS | RISK-TEST-STRATEGY covers all scope risks, adds 14 new risks, maps them to scenarios; security, failure modes, and edge cases all addressed |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | Fan-in ceiling at N=50 (ADR-004) | SCOPE.md SR-01 recommends the architect "document the acceptable edge-cardinality ceiling" but does not prescribe a specific value. Architecture introduces a concrete N=50 truncate-and-warn ceiling, which is a design decision beyond a documentation requirement. This is appropriate behavior from the architect, but represents a scoped-in constraint not present in SCOPE.md. |
| Addition | Source-validation guard before redirect_graph_edge (ADR-003/FR-06) | SCOPE.md SR-06 leaves the decision to the architect: "Architect must decide: does the redirect loop validate source entry status." Architecture decides skip-with-warn for quarantined/deprecated sources. This is the correct resolution, but the concrete decision is new scope introduced by the architect. |
| Addition | Response text truncation notation (ADR-004) | SCOPE.md OQ-01 defines response text as `"Redirected N incoming edges (M failed, see logs)"`. Architecture adds a truncation variant: `"Redirected N incoming edges (truncated from M, see logs)"`. This is a direct consequence of the N=50 ceiling decision; it is additive and consistent with SCOPE.md intent. |
| Simplification | Terminal-active resolution via direct new_entry.id (ADR-001) | SCOPE.md proposed `find_terminal_active` cache traversal as the mechanism, with a cold-cache fallback. Architecture simplifies to always using `new_entry.id` directly, with detailed rationale (Active-entry invariant, no cache-lock dependency, incremental correction model). Rationale is sound and documented. |

No scope gaps were identified. All five SCOPE.md goals are addressed; all six non-goals are explicitly preserved in SPECIFICATION and ARCHITECTURE as out-of-scope. All constraints from SCOPE.md appear in SPECIFICATION constraints C-01 through C-07 and architecture constraints section.

---

## Variances Requiring Approval

### WARN 1: SPECIFICATION FR-07 uses incorrect return-contract table (R-01 open defect)

**What**: SPECIFICATION FR-07 contains a return-contract table with `Ok(true)/Ok(false)` variants for `redirect_graph_edge`. The actual function signature is `Result<(), EdgeRedirectError>`, which has no `Ok(bool)` variant. Architecture ADR-003 provides the correct table (`Ok(())` = success/conflict, `Err(EdgeRedirectError)` = failure). The spec's FR-07 table and the ADR-003 table are contradictory.

**Why it matters**: This is an internal consistency defect between SPECIFICATION and ARCHITECTURE. The RISK-TEST-STRATEGY identifies it as Critical R-01 and explicitly states "The spec's FR-07 table must not be implemented as written." However, the defect was not corrected in the spec itself before being submitted for alignment review. An implementer working primarily from the SPECIFICATION (rather than cross-referencing ADRs) could implement the wrong return-contract, producing code that does not compile against the actual `redirect_graph_edge` signature or silently mis-handles the conflict case.

**Recommendation**: Correct SPECIFICATION FR-07 before delivery begins. The correct table is in ARCHITECTURE ADR-003. This is a documentation fix, not a design change — no approval needed for the substance, only for the correction action. The RISK-TEST-STRATEGY's scenario 1 for R-01 (compile-time verification) will catch an incorrect implementation, but the spec should not contradict the architecture.

---

### WARN 2: Fan-in ceiling value (N=50) is a new design choice not in SCOPE.md

**What**: SCOPE.md SR-01 says the architect should "document the acceptable edge-cardinality ceiling." Architecture ADR-004 sets N=50 as a concrete truncate-and-warn ceiling. The value 50 is derived from observed cardinality (no production entry exceeds single-digit incoming edges) and a latency budget assumption (~50ms inline), both stated in ADR-004.

**Why it matters**: The ceiling is an observable behavioral change — a context_correct call on an entry with >50 incoming edges will silently leave some edges un-redirected. This is an edge case in practice, but it is a permanent behavior limit. The rationale in ADR-004 is sound. Human awareness is warranted given the permanent truncation semantics.

**Recommendation**: Accept. The N=50 ceiling is well-justified in ADR-004, consistent with the partial-write posture, and observable via tracing::warn!. The DependencyOnDeprecated detection rule will continue to surface any unredirected stale edges. No change required unless the product owner has a different latency or correctness preference for high-fan-in entries.

---

## Detailed Findings

### Vision Alignment

vnc-017 directly serves two product vision non-negotiables:

**Hash chain integrity and correction chain model**: The vision states "Correction chain model: supersedes/superseded_by — extended by W1-1 but not modified." When entries are corrected, stale edges pointing at deprecated originals violate the graph state that underpins PPR traversal and DependencyOnDeprecated detection. vnc-017 closes this gap automatically, making the graph state consistent with the correction semantics the vision depends on.

**Immutable audit log / complete attribution**: The feature does not modify the audit log model. Redirected edges produce no new audit events (consistent with edge writes across the codebase), and the correction audit event already covers the entry operation.

**In-memory hot path preservation**: NFR-05 and NFR-09 explicitly prohibit accessing TypedGraphState in the redirect loop. The hot-path cache rebuild pattern is not touched.

**Graceful degradation**: ADR-003 and ADR-004 together ensure the feature degrades gracefully: partial redirects are acceptable, the correction always succeeds, and the DependencyOnDeprecated rule covers any un-redirected remainder.

The feature is narrowly scoped to a graph maintenance side-effect of `context_correct`. It does not introduce new MCP tools, background workers, or architectural layers. It is consistent with the vision principle of keeping Unimatrix a knowledge engine — not an orchestration engine — by automating a correctness property of the knowledge graph, not adding workflow logic.

### Milestone Fit

vnc-017 is Wave 1B work. The dependency chain is correct:
- W1B-1 (`vnc-015`) shipped `context_edge(mode="redirect")` and `redirect_graph_edge` — the infrastructure this feature reuses
- `vnc-016` shipped the `DependencyOnDeprecated` detection rule — the observable gap this feature closes

The feature does not pull in Wave 2 capabilities (HTTP, container, OAuth) or Wave 3 capabilities (GNN, synthesis). No future milestone scope was identified.

The Wave 1B section of the vision states: "agents declare typed relationships between entries at write time — ADR dependency chains, goal traceability, lesson→decision reasoning chains." vnc-017 ensures that correction operations on such entries do not leave stale graph state — a correctness requirement for the typed graph use case Wave 1B is building.

### Architecture Review

The architecture is well-structured and internally consistent with four exceptions noted:

**Resolved strengths**:
- ADR-001 correctly simplifies terminal-active resolution by exploiting the Active-entry invariant on `context_correct` calls. The complexity reduction (no TypedGraphState read lock, no cold-cache edge case, no fallback path) is fully justified.
- ADR-002 resolves SR-04 correctly: SQL-level Supersedes exclusion is cleaner than loop-level and co-locates the semantic explanation with the filtering logic.
- ADR-003 properly extends the established partial-write posture to auto-redirect, preventing correction failures from being surfaced as redirect infrastructure errors.
- ADR-004 provides an explicit latency budget for the fan-in ceiling, making the N=50 constant auditable.

**Minor gap**: The ARCHITECTURE `## Integration Surface` table lists `format_correct_success` with the existing signature and notes it "may receive redirect summary (or text is appended post-call; implementer's choice)." This is correctly identified as implementer-discretion. No concern.

**Supersedes edge direction note**: The architecture correctly observes that `build_typed_relation_graph` derives Supersedes from `entries.supersedes`, not from `GRAPH_EDGES`. ADR-002 documents this. The RISK strategy correctly flags (R-07) the Supersedes-only incoming edge case as an edge case to test explicitly.

### Specification Review

The specification is complete and addresses all acceptance criteria from SCOPE.md. FR-01 through FR-13 and NFR-01 through NFR-09 together cover every SCOPE.md goal and constraint.

**One defect** (WARN 1 above): SPECIFICATION FR-07 uses an incorrect return-contract table that contradicts ARCHITECTURE ADR-003. The correct contract is `Result<(), EdgeRedirectError>` with `Ok(())` covering both success and UNIQUE-conflict cases. The spec's `Ok(true)/Ok(false)` table is a copy-over from the SCOPE.md description of `write_graph_edge` (which does return `bool`), not from `redirect_graph_edge`. This must be corrected before delivery.

**AC completeness**: AC-01 through AC-16 cover all SCOPE.md acceptance criteria plus additional scenarios surfaced by the risk strategy. AC-16 (DependencyOnDeprecated clears after full redirect) is correctly added beyond SCOPE.md, closing the SR-07 gap. AC-09 (idempotent UNIQUE-conflict handling) is correctly added to address SR-02.

**Domain model section**: The ubiquitous language and entity definitions in the specification are precise and consistent with codebase terminology. No terminology drift detected.

### Risk Strategy Review

The RISK-TEST-STRATEGY is thorough and demonstrates knowledge of the codebase:

**Critical risks correctly identified (R-01, R-02, R-06)**:
- R-01 (spec/architecture return-contract contradiction) is the most actionable risk: it is a documentation defect that could mislead implementation. The test scenarios correctly gate on compile-time structural verification.
- R-02 (Supersedes exclusion level) is a valid internal consistency observation. The ADR-002 decision provides the tie-breaker (SQL level preferred), but the spec's FR-04 uses "loop-level" language before OQ-01 resolves it. The test scenario requiring a structural assertion on `query_incoming_edges` return values is the correct mitigation.
- R-06 (Contradicts bidirectionality with quarantined source) is correctly identified as critical. The 4-row atomicity of `redirect_graph_edge` for Contradicts edges means source validation must happen before the call, not inside it. The ADR-003 resolution (skip-with-warn) is correctly applied.

**Scope risk traceability**: All eight SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-08) are mapped to architecture risks and covered by test scenarios. The traceability table is complete.

**Security risks**: The security section correctly identifies that the only caller-controlled input to the redirect loop is `original_id`, which has already been validated as Active by the time the redirect loop runs. SQL injection is not a vector. The blast-radius analysis (N=50 ceiling prevents unbounded latency amplification from crafted hub entries) is sound.

**One gap in RISK**: R-09 ("Ok(false) counter ambiguity in AC-09") is labeled as concerning because "spec says `Ok(false)` is treated as success." This is the same R-01 root cause — the spec's incorrect return-contract table propagates into AC-09 language. Once FR-07 is corrected, R-09 resolves automatically: `Ok(())` always means `redirected++`, no ambiguity. The risk is real but dependent on the WARN 1 fix.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #2298 (config key semantic divergence from vision example) and #3742 (optional future branch in architecture must match scope intent — WARN if architecture and risk diverge from scope deferral). Entry #3742 is directly applicable: it warns that when architecture introduces an optional future branch (here, the N=50 ceiling deferring a future batching optimization), the risk strategy should explicitly track it. The RISK strategy does track this via R-05 and R-04, satisfying the pattern.
- Stored: nothing novel to store — the SPEC/Architecture return-contract contradiction (R-01) is a feature-specific defect, not yet a recurring cross-feature pattern. If a second feature ships with the same spec-contradicts-ADR documentation defect, store as a vision pattern then.
