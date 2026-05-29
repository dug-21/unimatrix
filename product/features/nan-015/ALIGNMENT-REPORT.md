# Alignment Report: nan-015

> Reviewed: 2026-05-29
> Artifacts reviewed:
>   - product/features/nan-015/architecture/ARCHITECTURE.md
>   - product/features/nan-015/specification/SPECIFICATION.md
>   - product/features/nan-015/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Completes originally intended volume separation from ASS-043; aligns with W2-1 container packaging goals |
| Milestone Fit | PASS | Nanoprobes infrastructure feature targeting W2-1; appropriate milestone |
| Scope Gaps | PASS | All SCOPE.md goals and acceptance criteria addressed in source docs |
| Scope Additions | PASS | No material scope additions beyond what SCOPE.md requested |
| Architecture Consistency | WARN | Cache path precedence ordering contradicts between architecture and specification (see V-01) |
| Risk Completeness | PASS | 15 risks with full scenario mapping; scope risk traceability complete |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | CI release.yml changes | Architecture states "None to release.yml itself" (ARCH line 50). SCOPE Constraint 7 and RISK R-10 flag that smoke tests may need updating. No conflict -- architecture correctly notes the Dockerfile change propagates automatically, while risk strategy covers the audit need. |

No scope gaps. No scope additions. All 11 acceptance criteria from SCOPE.md are reflected 1:1 in the SPECIFICATION acceptance criteria table. All 6 goals from SCOPE.md are addressed. All 7 non-goals from SCOPE.md are echoed in the SPECIFICATION "NOT in Scope" section.

## Variances Requiring Approval

### V-01: Cache Path Precedence Ordering Inconsistency (WARN)

1. **What**: The ARCHITECTURE (lines 17-24) defines the resolution precedence as: (1) `EmbedConfig.cache_dir` field, (2) `UNIMATRIX_MODEL_CACHE` env var, (3) `dirs::cache_dir()`, (4) `.unimatrix/models`. The SPECIFICATION contradicts this in two places: Constraint C-04 (lines 261-264) and Domain Models `resolve_cache_dir()` definition (line 171) both list the env var as highest priority, with the config field second. The RISK-TEST-STRATEGY scenario R-01.3 (line 33) agrees with the ARCHITECTURE: "field wins over env var."

2. **Why it matters**: Implementers reading the SPECIFICATION will code the wrong precedence order. The ARCHITECTURE's ordering is the correct design intent (explicit config field should override env var, per ADR-002), but the SPECIFICATION is the document delivery agents typically implement from. This could result in a bug where container-specific env var overrides an operator's explicit `cache_dir` config entry, which is the opposite of the intended behavior.

3. **Recommendation**: Fix the SPECIFICATION C-04 and Domain Models sections to match the ARCHITECTURE's ordering: config field > env var > dirs > fallback. No human approval needed -- this is a documentation fix, not a design change. The ARCHITECTURE and RISK-TEST-STRATEGY already agree on the correct ordering.

## Detailed Findings

### Vision Alignment

The product vision (PRODUCT-VISION.md, W2-1 section, lines 445-460) describes container packaging with named volumes and clean backup/recovery. The vision states "ONNX models baked into the image" -- reflecting the nan-014 shipped design. nan-015 explicitly addresses this by separating models to a shared volume, which is the ASS-043 originally intended design that nan-014 simplified.

Key vision principles satisfied:

- **Zero infrastructure** (vision line 708): Preserved. Docker Compose with two named volumes is still zero-infrastructure from an operator perspective. Auto-download preserves zero-config startup.
- **Graceful degradation** (vision line 711): Preserved. Existing `EmbedServiceHandle` / `NliServiceHandle` state machines with retry and fallback are unchanged. Architecture explicitly documents all failure modes (ARCH lines 107-111).
- **Single binary** (vision line 707): Preserved. No new services introduced.
- **Container is optional** (vision line 708): Preserved. Non-container behavior is explicitly unchanged (SPEC NFR-02, SCOPE Constraint 5).

The feature also aligns with the vision's security cross-cutting concerns: hash chain integrity is not affected, audit log is not affected, capability checks are not affected. The shared volume does widen the supply-chain attack surface (writable volume vs. baked-in models), but this is acknowledged in the risk strategy (R-03) and mitigated by documenting `:ro` hardening and hash pinning guidance (AC-11).

### Milestone Fit

nan-015 is a Nanoprobes-phase (build/deploy/CI) feature targeting W2-1 (Container Packaging). This is the correct milestone. The feature does not build capabilities for future milestones -- it completes an originally intended W2-1 design element that was simplified during nan-014 delivery.

SCOPE.md explicitly positions this as completing the ASS-043 recommendation after nan-014's simplification, which is appropriate milestone discipline. No premature Wave 3 or Wave 2 extension work is included.

### Architecture Review

The architecture is well-scoped and minimal:

- **Single code change**: One function modification (`resolve_cache_dir()`) to add env var check. All 7 non-test call sites already use `EmbedConfig::default()` with `cache_dir: None`, so the env var insertion captures all of them without modifying any call site (ARCH lines 140-152). This is clean engineering.
- **Complete call site audit**: Architecture enumerates all 8 call sites (7 non-test + 1 test) with file:line references (ARCH lines 139-151). Thorough.
- **Error boundaries**: All three failure categories (download, permission, corruption) are documented with their propagation paths and recovery behavior (ARCH lines 107-111).
- **Components NOT affected**: Explicit verification that 7 components need no changes (ARCH lines 129-137). This is good practice for infrastructure changes.
- **Open questions**: Two open questions are reasonable and low-risk (eval harness in container -- not currently done; GGUF future path -- explicitly deferred).

Architecture ADRs (3 recorded in Unimatrix: #4650, #4651, #4652) cover the key decisions: env var approach, precedence chain, and shared volume default permissions.

### Specification Review

The specification is thorough with 13 functional requirements and 6 non-functional requirements, mapping cleanly to the 11 acceptance criteria from SCOPE.md. All constraints from SCOPE.md are reflected and extended with risk-informed constraints (C-08 verify-then-load ordering, C-09 partial file corruption, C-10 volume driver assumptions).

Key strength: The specification explicitly documents what is NOT in scope (7 items matching SCOPE non-goals plus 2 additions: `--cache-dir` CLI flag and atomic write for download). These additions are correct simplification decisions, not scope gaps.

The precedence ordering inconsistency noted in V-01 is the only issue.

### Risk Strategy Review

The risk strategy is comprehensive:

- **15 risks** covering all scope risks (SR-01 through SR-08) with full traceability (Scope Risk Traceability table, lines 228-237).
- **26+ test scenarios** across all risk levels.
- **Coverage summary** by priority: 2 Critical (6 scenarios), 5 High (11 scenarios), 6 Medium (7 scenarios), 2 Low (2 scenarios).
- **Integration risks** section (lines 192-197) identifies the four key integration seams: env var spelling consistency, Dockerfile-to-compose mount matching, NliConfig propagation, and CLI-vs-daemon concurrent access.
- **Edge cases** section (lines 199-206) covers 6 edge cases including relative paths, NFS drivers, disk full, file deletion during runtime, and long downloads.
- **Security risks** section (lines 208-213) addresses the expanded attack surface from writable shared volumes, env var injection, path traversal, and volume mount substitution.

The risk strategy correctly identifies that the embedding model SHA-256 gap (R-03) is the highest-impact security risk but correctly classifies it as out of scope (#651), with documentation-based mitigation (AC-11). This is proportional risk management -- the gap existed before nan-015 (models were writable at `/data/.cache/` on the data volume too), and nan-015's contribution is to document it explicitly.

Minor note: The coverage summary table (line 240) lists "5" risks at High but then enumerates "R-02, R-03, R-04, R-05, R-10" -- that is 5 risks, which matches. However, the table header says "Risk Count: 4" for High priority, which is a cosmetic typo. Not actionable.

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns -- found #2298 (config key semantic divergence pattern), #3337 (architecture-spec header divergence pattern), #3158 (deferred scope resolution AC ambiguity). The config key divergence pattern (#2298) is thematically relevant: it describes cases where the same key has different semantics across documents. V-01 (precedence ordering disagreement) is a variant of this same pattern.
- Stored: nothing novel to store -- the precedence ordering inconsistency between architecture and specification is a feature-specific documentation error, not a generalizable cross-feature pattern. The existing pattern #2298 already captures the broader category.
