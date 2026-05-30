# Alignment Report: vnc-023

> Reviewed: 2026-05-30
> Artifacts reviewed:
>   - product/features/vnc-023/architecture/ARCHITECTURE.md
>   - product/features/vnc-023/specification/SPECIFICATION.md
>   - product/features/vnc-023/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/vnc-023/SCOPE.md
> Scope risk source: product/features/vnc-023/SCOPE-RISK-ASSESSMENT.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | CVE resolution and protocol compliance directly advance the personal-cloud goal. No vision contradictions. |
| Milestone Fit | PASS | Vinculum-phase dependency upgrade. No future-milestone capabilities introduced. |
| Scope Gaps | PASS | All 12 acceptance criteria from SCOPE.md are fully addressed in specification and architecture. |
| Scope Additions | PASS | No material scope additions detected. Source documents faithfully implement SCOPE.md. |
| Architecture Consistency | PASS | Lean architecture appropriate for a dependency upgrade. ADR-003 isolation boundary respected. |
| Risk Completeness | PASS | 13 risks mapped, all scope risks traced, edge cases and security risks well-covered. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| — | No gaps detected | All 12 ACs (AC-01 through AC-12) from SCOPE.md appear verbatim in SPECIFICATION.md with verification methods specified. |
| — | No additions detected | Source documents do not introduce requirements beyond what SCOPE.md requests. |
| Simplification | Architecture is "correspondingly lean" | Rationale: dependency upgrade, not feature build. Architecture omits component diagrams and data flow models that would be expected for a feature build. Appropriate for scope. |

## Variances Requiring Approval

None. All source documents align with SCOPE.md and the product vision. No VARIANCE or FAIL classifications.

## Detailed Findings

### Vision Alignment

**Strategic goal advancement**: vnc-023 directly advances the **personal-cloud** goal (#4676). The goal's success criteria include "TLS required for all HTTPS connections" and "Token is the sole authorization credential." CVE-2026-42559 (DNS rebinding via Host header) is a direct threat to these criteria — an attacker can bypass bearer token protection via DNS rebinding on the HTTPS transport. Resolving the CVE is a necessary condition for the personal-cloud goal's security posture.

**Architectural principles**:

| Principle | Applicability | Assessment |
|-----------|--------------|------------|
| 1. Hash chain integrity | N/A | Dependency upgrade does not touch knowledge storage. |
| 2. Audit log append-only | N/A | No audit log changes. |
| 3. Capability checks at service layer | Directly relevant | Extension propagation (AC-07, R-01) ensures ResolvedIdentity survives rmcp processing so capability checks can function. The risk strategy correctly identifies this as the highest-priority integration test. PASS. |
| 4. Typed relationship graph | N/A | No graph changes. |
| 5. Graceful degradation | N/A | No ML capabilities affected. |
| 6. Single binary, zero infrastructure | PASS | No new infrastructure requirements. Config addition is backward-compatible. |
| 7. In-memory hot path | N/A | No search/analytics changes. |
| 8. No secrets in database | PASS | `allowed_origins` is a config value, not a secret. Bearer tokens remain in environment/config only. |

**Protocol compliance**: Upgrading from MCP 2024-11-05 to 2025-11-25 is vision-aligned — the personal-cloud goal requires MCP-compatible clients to "connect identically via HTTPS." Advertising a stale protocol version risks compatibility warnings or degraded behavior, contradicting this criterion.

**Implementation description enrichment (Opp 20)**: Adding `.with_description("Self-learning knowledge engine for agentic workflows")` to the MCP initialize response aligns with the product vision's identity statement. The description string accurately reflects the vision document's opening line. PASS.

### Milestone Fit

vnc-023 is a Vinculum-phase feature (MCP server layer). All changes are within the Vinculum scope:
- `server.rs` — ServerHandler implementation
- `router.rs` — McpAdapter transport coupling
- `config.rs` — HttpConfig (transport configuration)
- `Cargo.toml` — dependency management

No Cortical (learning), Collective (orchestration), or Matrix (UI) capabilities are introduced. The two opportunistic enhancements (Opp 11, Opp 20) are strictly within the Vinculum transport/config layer. No milestone discipline violation.

### Architecture Review

**Appropriate scope**: The architecture document is lean — 7 components (C1-C7), a component interaction diagram, and an implementation ordering. This is proportional to a dependency upgrade that touches 3-4 files with ~100 lines of changes.

**ADR-003 isolation boundary respected**: The architecture correctly identifies that ADR-003 concentrates all rmcp coupling in ~100 lines across 3 files. Changes stay within this boundary. The architecture does not propose expanding the rmcp coupling surface.

**Scope risk resolution**: All 7 scope risks (SR-01 through SR-07, plus SR-08 through SR-10) are addressed in the architecture:
- SR-01 (feature flags): RESOLVED with verification evidence
- SR-02 (MSRV): RESOLVED
- SR-03 (initialize signature): Design decision documented with compile-first strategy
- SR-04 (http crate): RESOLVED
- SR-07 (schemars): RESOLVED
- SR-08 (extension propagation): Integration test strategy specified

**Open questions carried forward**: 3 open questions remain in the architecture (ServerInfo Default+non_exhaustive, allowed_origins interaction, serve_client location). All are appropriate compile-time discoveries — the architecture correctly defers them to implementation rather than speculating. This matches the compile-first strategy in ADR-001.

**Technology decisions**: 4 ADRs referenced, all stored in Unimatrix (#4700, #4701, #4702). Decisions are traceable and justified.

### Specification Review

**AC traceability**: All 12 acceptance criteria from SCOPE.md are reproduced in the specification with explicit verification methods. No AC is weakened or strengthened.

**Functional requirements**: 12 FRs (FR-01 through FR-12) map cleanly to the 8 goals and 12 ACs in SCOPE.md:
- FR-01 through FR-06: Direct mappings to SCOPE.md goals 1-8
- FR-07: Extension propagation (SCOPE.md goal 4, AC-07)
- FR-08: Implementation description (SCOPE.md goal 5, AC-08)
- FR-09: Origin validation config (SCOPE.md goal 6, AC-09)
- FR-10: Initialize override (SCOPE.md goal 7/AC-12)
- FR-11-12: Quality gates (AC-10, AC-11)

**Non-functional requirements**: 6 NFRs are appropriate and do not exceed scope:
- NFR-01 (zero behavioral regression): Directly from SCOPE.md "Proposed Approach" behavioral validation
- NFR-02 (backward-compatible config): From SCOPE.md constraints
- NFR-03 (compilation performance): Reasonable for a dependency upgrade
- NFR-04 (dependency compatibility): From SCOPE.md constraints
- NFR-05 (MSRV): From SCOPE.md constraints and open questions
- NFR-06 (security posture): From SCOPE.md problem statement (CVE resolution)

**Not-in-scope section**: 12 explicit exclusions match SCOPE.md non-goals exactly. No non-goal was silently promoted to in-scope.

**Domain models**: Well-defined, accurate to the codebase. No invented abstractions.

**Constraints**: 10 constraints (C-01 through C-10) are traceable to SCOPE.md constraints and scope risk assessment. C-09 (three-file change boundary) is a useful guardrail from ass-065 that the specification correctly includes.

### Risk Strategy Review

**Risk coverage**: 13 risks identified, covering:
- 2 Critical (R-01 extension propagation, R-02 initialize signature)
- 4 High (R-03 struct migration, R-04 config wiring, R-05 CVE resolution, R-10 http version)
- 5 Medium (R-06 behavioral defaults, R-07 UDS transport, R-08 serve_client, R-09 config compat, R-11 ErrorData)
- 2 Low (R-12 description string, R-13 origin/host interaction)

**Scope risk traceability**: All 10 scope risks (SR-01 through SR-10) are mapped to architecture risks with resolution status. Complete traceability.

**Security risks**: 4 security surfaces identified — Host header (CVE), Origin header (Opp 11), extension propagation (authorization bypass), config injection (dismissed with rationale). This is thorough for a dependency upgrade.

**Edge cases**: 6 edge cases documented, including the `..Default::default()` footgun with `#[non_exhaustive]` (edge case 3) and keep_alive during long tool execution (edge case 4). These demonstrate the risk strategy goes beyond compilation concerns to behavioral correctness.

**Test scenario count**: 25 scenarios across 13 risks. Proportional — not over-specified for a patch-level effort, but sufficient to cover the critical paths.

**R-01 (extension propagation)**: Correctly classified as Critical. The risk strategy requires a test that "must fail if propagation breaks" — this follows the gate-fix pattern (#4452). The negative scenario (test must detect the absence) is particularly important because extension loss is silent at runtime.

**R-05 (CVE resolution)**: Verification includes Cargo.lock inspection and behavioral verification that `allowed_hosts` defaults are not overridden. Appropriate — the CVE fix is via rmcp defaults, so the risk is that application code accidentally clears those defaults.

**Integration risks section**: Correctly identifies the 4-hop config chain (R-04) and the http crate TypeId footgun (R-10 + R-01) as the highest integration risks. Both are well-known Rust patterns with no compile-time diagnostic.

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns -- no vision-specific alignment patterns found in Unimatrix. Closest match was #2298 (config key semantic divergence) which is feature-specific, not a generalizable vision alignment pattern.
- Stored: nothing novel to store -- vnc-023 is a clean dependency upgrade with no variances. The alignment patterns observed (CVE fix aligns with personal-cloud security posture, opportunistic enhancements stay within the file-touch boundary) are feature-specific and do not generalize into recurring misalignment patterns.
