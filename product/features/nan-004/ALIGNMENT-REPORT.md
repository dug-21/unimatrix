# Alignment Report: nan-004

> Reviewed: 2026-03-12
> Artifacts reviewed:
>   - product/features/nan-004/architecture/ARCHITECTURE.md
>   - product/features/nan-004/specification/SPECIFICATION.md
>   - product/features/nan-004/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/nan-004/SCOPE.md
> Scope risk source: product/features/nan-004/SCOPE-RISK-ASSESSMENT.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly implements the "Platform Hardening & Release" milestone goal |
| Milestone Fit | PASS | Correctly targets the next milestone; no future-milestone scope creep |
| Scope Gaps | PASS | All 17 acceptance criteria from SCOPE.md are addressed in source documents |
| Scope Additions | WARN | Minor additions that are reasonable but not explicitly requested in SCOPE.md |
| Architecture Consistency | PASS | Component breakdown maps cleanly to scope; delivery phasing is sensible |
| Risk Completeness | PASS | All 10 scope risks traced; 15 architecture risks identified with test scenarios |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | Init subcommand location | SCOPE proposes JS shim routing to Rust `init`; Architecture (C4) implements init entirely in Node.js, invoking Rust binary only for DB creation and validation. Rationale: simpler file manipulation in JS. Spec leaves it as open question #1. Acceptable — the architecture decision is sound. |
| Simplification | Hook command paths | SCOPE Resolved Q5 says "PATH-based via `node_modules/.bin/`". Architecture ADR-001 overrides this to absolute paths, correctly addressing SR-09. The scope risk assessment flagged this as the top risk. This is a well-reasoned deviation. |
| Addition | `model-download` subcommand (C8) | Not explicitly mentioned in SCOPE.md acceptance criteria but implied by the postinstall behavior (AC-02). Architecture adds a dedicated Rust subcommand for this. Reasonable — needed to implement the postinstall ONNX download. |
| Addition | `--version` flag (FR-05) | SCOPE mentions a `version` subcommand. Spec adds `--version` flag as an alternative. Minor ergonomic addition. |
| Addition | `UNIMATRIX_BINARY` env var override (C3) | Architecture adds an environment variable fallback for binary resolution. Not in SCOPE.md. Useful for development/testing; low risk. |
| Addition | `--project-dir` flag on Rust binary (C7) | Architecture adds a `--project-dir` CLI flag. Not in SCOPE.md acceptance criteria. Required for the init command to trigger DB creation at the correct project path. Implementation necessity. |
| Addition | `verbose` flag on Rust binary (C7) | Architecture shows `--verbose` / `-v` flag. Not in SCOPE.md. Minor observability addition. |

## Variances Requiring Approval

None. All deviations are simplifications or implementation-necessary additions that fall within the spirit of the scope. No VARIANCE or FAIL items identified.

## Detailed Findings

### Vision Alignment

The product vision states nan-004 as: "npm/npx distribution of Rust binary. Platform-specific binary compilation (linux x64, darwin arm64/x64). npm package that downloads the right binary (esbuild/turbo pattern). Schema migration on startup. Semantic versioning. Includes mechanical wiring: settings.json (MCP server + hooks), ONNX model pre-download, schema pre-creation, skill file installation."

All source documents directly implement this vision statement:
- **npm distribution**: Architecture C1-C3, Spec FR-06 through FR-09.
- **Platform-specific compilation**: Architecture C10, Spec FR-28 through FR-33 (linux-x64 initially, extensible).
- **esbuild/turbo pattern**: Architecture C1 explicitly names this pattern.
- **Schema migration on startup**: Already exists (`migrate_if_needed()`); SCOPE confirms no new work needed.
- **Semantic versioning**: Architecture C9, Spec FR-23 through FR-25.
- **Mechanical wiring**: Architecture C4-C5, Spec FR-10 through FR-19.
- **ONNX model pre-download**: Architecture C6, Spec FR-20 through FR-22.
- **Schema pre-creation**: Architecture C4 step 6, Spec FR-15.
- **Skill file installation**: Architecture C4 step 5 (C1 bundles all 13), Spec FR-14.

The vision's "zero cloud dependency" principle is maintained — the npm package distributes a self-contained binary; the ONNX model is the only external download and it gracefully degrades.

The vision's "auditable knowledge lifecycle" and "invisible delivery" principles are not directly exercised by this feature (it is infrastructure), but are preserved — the init command wires the hooks that enable invisible delivery, and the database creation preserves the audit infrastructure.

**Status: PASS**

### Milestone Fit

nan-004 is listed under "Platform Hardening & Release — NEXT" in the product vision. The source documents stay within this milestone's goals:
- No Graph Enablement (future milestone) capabilities are introduced.
- No Activity Intelligence features are pulled in.
- The feature focuses exclusively on making the existing engine shippable.

The scope appropriately defers darwin platforms, public npm publishing, and Windows support — all consistent with "start with linux-x64 only" in SCOPE.md and the vision's "first multi-repo deployments" framing.

The initial version of 0.5.0 (reflecting 44 features shipped, ~1700 tests) is a reasonable semantic version choice that signals maturity without claiming 1.0 stability.

**Status: PASS**

### Architecture Review

The architecture document (ARCHITECTURE.md) is well-structured with 11 components (C1-C11) that map cleanly to the scope's proposed approach:

**Strengths:**
- Clear component boundaries with explicit responsibilities.
- 4-wave delivery phasing respects dependency graph (C7/C9 first, then package structure, then init, then release infra).
- Integration surface table precisely references existing crate functions and file locations.
- ADR references for all 5 technology decisions.
- Hook command format section explicitly shows the absolute-path resolution (addressing SR-09).

**Observations:**
- Architecture open question #1 (ONNX shared library bundling) and #2 (binary size) are appropriately flagged as needing CI validation rather than being resolved speculatively. This is good engineering discipline.
- The `init` subcommand is shown in the Rust CLI struct (C7) but the architecture text says init logic is in Node.js (C4). The Rust `init` subcommand would only be reached if someone calls the binary directly (not via npx). The specification's open question #1 acknowledges this design tension. The architecture's actual implementation (C4 in JS, calling Rust only for DB/validation) is coherent — the Rust `Command::Init` variant may be vestigial or a direct-binary fallback. This is a minor ambiguity, not a conflict.

**Status: PASS**

### Specification Review

The specification covers all 17 acceptance criteria from SCOPE.md with traceable functional requirements:

| SCOPE AC | Spec Coverage |
|----------|--------------|
| AC-01 | FR-06, FR-07, FR-08, NFR-06 |
| AC-02 | FR-20, FR-21, FR-22 |
| AC-03 | FR-11 |
| AC-04 | FR-12, FR-13 |
| AC-05 | FR-14 |
| AC-06 | FR-15 |
| AC-07 | FR-16 |
| AC-08 | FR-19 |
| AC-09 | FR-09 |
| AC-10 | FR-28, FR-29, FR-30, FR-31, FR-32 |
| AC-11 | FR-24 |
| AC-12 | FR-08 |
| AC-13 | FR-17 |
| AC-14 | FR-18 |
| AC-15 | FR-23 |
| AC-16 | FR-25 |
| AC-17 | FR-26, FR-27 |

The specification adds appropriate non-functional requirements (NFR-01 through NFR-07) that operationalize the scope's constraints. NFR-04 (Ubuntu 22.04 LTS baseline) directly addresses SR-02. NFR-07 (backward compatibility for binary rename) addresses SR-06.

The "NOT in Scope" section in the specification correctly mirrors SCOPE.md's non-goals with additional detail (e.g., explicitly excluding agent definition copying, CLAUDE.md generation, cross-compilation). No scope creep detected.

The specification has 5 open questions. Questions 1-3 are implementation details appropriately deferred to delivery. Question 4 (UserPromptSubmit tee command) is a good catch — the SCOPE shows this pattern but does not explicitly discuss whether init should reproduce it. Question 5 (npm auth in CI) is a configuration concern correctly flagged for delivery.

**Status: PASS**

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is thorough:

**Strengths:**
- 15 risks identified, covering all 10 scope risks (SR-01 through SR-10) with explicit traceability table at the bottom.
- Risk-to-scenario mapping provides concrete, testable scenarios for each risk (51 total scenarios across all priorities).
- Critical risks (R-01: settings.json merge corruption, R-02: absolute path invalidation) receive the most detailed treatment with 7 and 4 scenarios respectively.
- Security risks section covers postinstall script safety, npm token handling, path traversal in skill copy, and blast radius of a compromised binary.
- Edge cases section is practical (no `.git`, root `/`, settings.json as directory, spaces in paths, concurrent init runs).
- Failure modes table maps every anticipated failure to expected behavior.

**Observations:**
- The coverage summary table has a count mismatch: it says "4 (R-03, R-04, R-07, R-09, R-15)" for High priority but that is 5 risks, not 4. Similarly "5 (R-05, R-06, R-08, R-11, R-13, R-14)" lists 6 risks. These are minor counting errors in the summary table — all risks are properly covered in the detailed sections.
- R-12 (binary rename breaks existing hooks) is rated Low likelihood, which is correct since this is a single-consumer repo. The architecture's ADR-002 (atomic rename) addresses it.
- The document does not explicitly discuss rollback strategy if a published npm package is defective. This is a minor gap — npm unpublish within 72 hours is the standard mechanism, but it could be noted.

**Status: PASS**

## Knowledge Stewardship

- Queried: /query-patterns for vision alignment patterns -- no results (category "pattern" returned unrelated entries; no prior vision alignment reviews exist in the knowledge base)
- Stored: nothing novel to store -- this is the first vision guardian review; no recurring misalignment patterns can be identified from a single feature. The review found no variances, so there are no misalignment patterns to record. If future nan-* features show similar "implementation-necessary additions" (env var overrides, CLI flags), that may become a pattern worth storing.

## Self-Check

- [x] ALIGNMENT-REPORT.md follows the template format
- [x] All checks are evaluated (none skipped without N/A justification)
- [x] Every VARIANCE and FAIL includes: what, why it matters, recommendation — N/A (none found)
- [x] Scope gaps and scope additions are both checked
- [x] Evidence is quoted from specific document sections, not vague references
- [x] Report path is correct: `product/features/nan-004/ALIGNMENT-REPORT.md`
- [x] Knowledge Stewardship report block included
