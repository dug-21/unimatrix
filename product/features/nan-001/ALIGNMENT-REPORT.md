# Alignment Report: nan-001

> Reviewed: 2026-03-11
> Artifacts reviewed:
>   - product/features/nan-001/architecture/ARCHITECTURE.md
>   - product/features/nan-001/specification/SPECIFICATION.md
>   - product/features/nan-001/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Infrastructure feature supports Platform Hardening milestone; preserves trust/lifecycle/integrity pillars |
| Milestone Fit | PASS | Correctly targets Platform Hardening; nan-* prefix is designated for build/deploy/CI |
| Scope Gaps | PASS | All SCOPE.md requirements addressed in source documents |
| Scope Additions | PASS | No scope additions detected beyond SCOPE.md |
| Architecture Consistency | WARN | Two minor concerns: serde_json preserve_order approach and FR-09.2 signature divergence |
| Risk Completeness | PASS | 15 risks covering all 9 scope risks; 37 test scenarios; strong traceability |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | SR-02/SR-04 shared column definitions | Architecture explicitly defers shared column list derivation to post-v1. SCOPE.md recommended it; spec marks it "not mandated" (Spec NOT in Scope, final bullet). Rationale documented: schema v11 is stable, hardcoding acceptable for v1. |

No scope gaps. No scope additions.

## Variances Requiring Approval

None. All findings are PASS or WARN. No variances requiring human approval.

## Detailed Findings

### Vision Alignment

**Status: PASS**

The product vision identifies the "auditable knowledge lifecycle" as the core value proposition and lists "nan-*: CLI binary, Docker, CI integration, release automation" under Platform Hardening & Release (PRODUCT-VISION.md line 128). nan-001 directly enables this by providing a portable export format that preserves all learned signals -- confidence scores, usage counts, correction chains, co-access patterns, and audit records.

Key vision principles satisfied:
- **Trust + Lifecycle + Integrity**: Export preserves content_hash/previous_hash correction chains, audit_log (append-only compliance trail), and all confidence/helpfulness signals. The format contract ensures these survive round-trip through nan-002.
- **Self-contained embedded engine with zero cloud dependency**: Export is a local CLI subcommand, no network, no cloud. Aligns with the local-first architecture.
- **Cross-domain portability (ASS-009)**: Export format is domain-agnostic -- it dumps raw table data without domain-specific assumptions.

### Milestone Fit

**Status: PASS**

PRODUCT-VISION.md lists Platform Hardening & Release as depending on Activity Intelligence + Graph Enablement, with "nan-*" features explicitly called out (line 128). SCOPE.md correctly identifies this feature as a Platform Hardening prerequisite for multi-repo deployments.

The feature does not pull in capabilities from future milestones. It does not require Activity Intelligence or Graph Enablement tables (those are correctly excluded as ephemeral/derived). The scope explicitly restricts export to the 8 long-term tables.

### Architecture Review

**Status: WARN (minor)**

The architecture is well-structured, follows established codebase patterns (hook subcommand, Store::open, direct SQL access), and directly addresses the highest-priority scope risk (SR-07: transaction isolation via BEGIN DEFERRED).

**Concern 1: serde_json key ordering approach**

The architecture document (Row Serialization section, line 64) states: "serde_json::Map (which is backed by BTreeMap internally when the preserve_order feature is NOT enabled; see ADR-003 for the chosen approach)." ADR-003 is then listed as choosing "Explicit insertion order via serde_json::Map + sequential insert" (line 132). The risk strategy (R-11) identifies that enabling preserve_order affects all serde_json::Map usage crate-wide, and R-06 identifies key ordering as a determinism concern.

The architecture text is slightly ambiguous about whether preserve_order is used or not. The parenthetical says "when preserve_order is NOT enabled" but ADR-003 is described as "insertion order via sequential insert." The specification (NFR-03, line 105) says "use a BTreeMap<String, Value>" as one option, which would not need preserve_order. This ambiguity is minor -- the implementation can resolve it either way -- but the architecture should have been clearer about the chosen mechanism.

Classification: WARN. Implementation can resolve this. Both approaches achieve deterministic output.

**Concern 2: FR-09.2 function signature mismatch**

The specification (FR-09.2, line 86) defines the public entry function as: `pub fn run_export(store: &Store, output: Option<&Path>) -> Result<()>`. The architecture (line 41-44) defines it as: `pub fn run_export(project_dir: Option<&Path>, output: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>`. These differ in two ways: (a) the first parameter (Store reference vs project_dir path), and (b) the error type (Result<()> vs Result<(), Box<dyn Error>>).

The architecture version is more self-contained (handles Store::open internally), while the spec version expects the caller to open the Store. The architecture's component interaction diagram (line 77-106) shows run_export calling Store::open itself, consistent with the architecture signature.

Classification: WARN. Minor inconsistency between spec and architecture. The architecture's approach (taking project_dir, opening Store internally) is cleaner and aligns with the component interaction diagram. The implementation should follow the architecture signature.

### Specification Review

**Status: PASS**

The specification is thorough and well-structured:
- All 18 acceptance criteria from SCOPE.md are present with matching AC-IDs and verification methods.
- Complete field mappings for all 8 tables with SQL types, JSON types, nullability, and notes.
- All 10 SCOPE.md non-goals are carried through to the "NOT in Scope" section.
- Type encoding rules explicitly address the JSON-in-TEXT column handling (SR-01/SR-03).
- Transaction isolation is specified (FR-07, Constraint 10).
- User workflows are practical and map to the three use cases from SCOPE.md (backup, transfer, inspection) plus a concurrent-access scenario.
- The spec explicitly notes that column list derivation from a shared definition is "not mandated" for v1, appropriately scoping the work.

### Risk Strategy Review

**Status: PASS**

The risk strategy is comprehensive:
- 15 risks identified, covering all 9 scope risks (SR-01 through SR-09) with explicit traceability in the Scope Risk Traceability table.
- 37 test scenarios total across Critical (12), High (13), Medium (10), and Low (2) priorities.
- Critical risks (R-01, R-03, R-04, R-05) align with the highest-impact format contract concerns.
- Edge cases section (10 items) covers boundary conditions like empty strings vs NULL, epoch timestamps, JSONL-breaking characters, and orphaned tags.
- Security risks correctly identified as low-severity (local-only, read-only operation) with appropriate user-awareness notes about sensitive data in audit_log.
- Integration risks correctly identify the Store API contract dependency and the serde_json preserve_order crate-wide impact.
- Failure modes table covers all expected error paths.

## Knowledge Stewardship

- Queried: /query-patterns for vision alignment patterns -- not executed (test infrastructure context, no Unimatrix MCP server available in this worktree)
- Stored: nothing novel to store -- this is a clean infrastructure feature with no generalizable misalignment patterns. All documents are well-aligned with scope and vision. The minor WARN findings (key ordering ambiguity, function signature mismatch) are feature-specific implementation details, not recurring patterns.
