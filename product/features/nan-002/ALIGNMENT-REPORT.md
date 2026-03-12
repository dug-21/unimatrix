# Alignment Report: nan-002

> Reviewed: 2026-03-12
> Artifacts reviewed:
>   - product/features/nan-002/architecture/ARCHITECTURE.md
>   - product/features/nan-002/specification/SPECIFICATION.md
>   - product/features/nan-002/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Import completes the backup/restore cycle required by Platform Hardening milestone |
| Milestone Fit | PASS | Correctly targets Platform Hardening; no future-milestone capability creep |
| Scope Gaps | PASS | All SCOPE.md goals and acceptance criteria are addressed in source documents |
| Scope Additions | WARN | Specification FR-06 column list differs from Architecture EntryRow -- see Detailed Findings |
| Architecture Consistency | VARIANCE | Architecture and Specification disagree on which 26 entry columns exist -- 3 columns differ between them |
| Risk Completeness | PASS | All 9 scope risks traced; 15 architecture risks identified with test scenarios |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Consistency Issue | Entry column list mismatch | Architecture EntryRow has `source`, `correction_count`, `embedding_dim`; Specification FR-06 has `allowed_topics`, `allowed_categories`, `target_ids` instead. Both claim 26 columns. Only one can be correct per the actual DDL. |
| Simplification | No confirmation prompt for --force | SCOPE-RISK-ASSESSMENT SR-04 recommended `--force --yes` double-opt-in. Architecture ADR-003 chose stderr warning only. Spec explicitly defers double-opt-in to future iteration. Rationale documented. |
| Simplification | No --skip-embedding dry-run mode | SCOPE-RISK-ASSESSMENT SR-01 suggested this. Spec explicitly lists it under "NOT in Scope" with rationale. Acceptable. |

## Variances Requiring Approval

1. **What**: Architecture `format::EntryRow` (ARCHITECTURE.md line 154) defines 26 fields including `source: String`, `correction_count: i64`, `embedding_dim: i64` but excludes `allowed_topics`, `allowed_categories`, `target_ids`. Specification FR-06 (SPECIFICATION.md line 66) defines 26 columns including `allowed_topics`, `allowed_categories`, `target_ids` but excludes `source`, `correction_count`, `embedding_dim`.

   **Why it matters**: The import module must INSERT into the exact columns defined by the entries DDL. If the Architecture's EntryRow struct is wrong, deserialization will fail at runtime or silently drop columns. If the Specification's column list is wrong, implementation agents will target the wrong schema. This is a contract-level disagreement between two source documents that both claim authority over the same data structure. The actual DDL in `schema.rs` is the ground truth, and exactly one of these lists is wrong.

   **Recommendation**: Resolve before implementation. Query `PRAGMA table_info(entries)` or read `schema.rs` to determine the actual 26 columns. Update the incorrect document. This is a blocking issue for implementation correctness -- the shared `format.rs` types must match the DDL exactly. Classification: VARIANCE (not FAIL, because the error is in documentation, not in shipped code, and the round-trip test AC-15 would catch it during implementation).

## Detailed Findings

### Vision Alignment

The product vision states the Platform Hardening milestone requires "backup/restore, initialization, packaging, and documentation" for first multi-repo deployments. nan-002 (Knowledge Import) directly serves the restore half of backup/restore. The vision document (line 91) explicitly lists nan-002: "Restore from export dump. Re-embed all entries on import. Hash chain integrity validation. Schema version compatibility check. CLI subcommand."

All three source documents align with this vision:
- Architecture describes a CLI subcommand that restores from JSONL, re-embeds, validates integrity -- matching the vision description verbatim.
- Specification operationalizes each vision bullet as functional requirements (FR-02 through FR-09).
- Risk strategy covers the key technical risks (direct SQL, ONNX dependency, hash validation edge cases).

The feature does not attempt to build capabilities from future milestones (Graph Enablement, Semantic Routing, etc.). No vision principle is contradicted.

**Status: PASS**

### Milestone Fit

nan-002 is one of five features in the Platform Hardening milestone. It depends on nan-001 (Knowledge Export, marked complete in the vision). It does not depend on or pull from Graph Enablement or future horizons.

The feature scope is appropriately constrained:
- No merge/append mode (which would be a multi-project concern from Future Horizons)
- No MCP tool exposure (CLI only, matching the milestone's infrastructure focus)
- No incremental import (appropriately deferred)

**Status: PASS**

### Architecture Review

The architecture is well-structured with clear component breakdown, data flow diagram, error boundary table, and integration surface. Key strengths:

1. **Shared format types (ADR-001)** directly addresses scope risk SR-08 (implicit format contract). This is a sound architectural decision that provides compile-time contract enforcement.
2. **Re-embed after DB commit (ADR-004)** is a pragmatic decision with clear rationale -- bounds transaction duration and makes partial restore (DB without vectors) useful rather than catastrophic.
3. **Direct SQL INSERT (ADR-002)** is consistent with established patterns (nan-001, migration code). The rationale is well-documented.
4. **Component interactions diagram** (lines 56-89) provides clear step-by-step flow for implementation agents.

**Issue identified**: The `format::EntryRow` struct definition (line 154) lists fields that do not match the Specification's FR-06 column list. Specifically:

Architecture has: `source`, `correction_count`, `embedding_dim`
Specification has: `allowed_topics`, `allowed_categories`, `target_ids`

These are mutually exclusive sets of 3 columns. Since both documents claim exactly 26 columns, one must be wrong. The `allowed_topics`, `allowed_categories`, and `target_ids` fields appear in the `agent_registry` table context elsewhere, which suggests the Specification may have incorrectly pulled agent_registry columns into the entry column list. Conversely, `source` and `correction_count` and `embedding_dim` could be legacy columns removed in a schema migration. The actual DDL in `schema.rs` is the authority.

The Architecture's open question 1 (line 174) about `CURRENT_SCHEMA_VERSION` accessibility is pragmatically resolved -- read from counters table after Store::open(). This is consistent with the export approach.

**Status: VARIANCE** (column list disagreement between architecture and specification)

### Specification Review

The specification is thorough, covering 12 functional requirements, 5 non-functional requirements, 27 acceptance criteria (matching SCOPE.md exactly), 4 user workflows, 10 constraints, and explicit "NOT in Scope" section.

Key observations:

1. **All 27 acceptance criteria from SCOPE.md are present** in the specification's AC table (lines 156-184) with verification methods. No scope gaps.
2. **User workflows** (lines 234-266) cover the three use cases from SCOPE.md (backup/restore, cross-project transfer, multi-repo deployment) plus a fourth (import with integrity warning). The fourth workflow is a natural elaboration, not a scope addition.
3. **Constraints** faithfully translate SCOPE.md constraints with added specificity from the scope risk assessment.
4. **NOT in Scope** section (lines 313-327) explicitly addresses items deferred by SCOPE.md and scope risk recommendations (stdin, confirmation prompts, skip-embedding dry-run). This is good practice.
5. **FR-06 column list discrepancy** noted above. The specification lists `allowed_topics`, `allowed_categories`, `target_ids` as entry columns. These are more commonly associated with agent_registry. This needs verification against the DDL.

**Status: PASS** (contingent on column list resolution)

### Risk Strategy Review

The risk-test strategy is comprehensive:

1. **15 risks identified** covering SQL divergence (R-01), deserialization edge cases (R-02), counter collisions (R-03), destructive operations (R-04), embedding failure modes (R-05), FK violations (R-06), hash validation (R-07), concurrency (R-08), model availability (R-09), floating-point fidelity (R-10), unknown discriminators (R-11), performance (R-12), audit ID collisions (R-13), path resolution (R-14), and injection (R-15).
2. **All 9 scope risks traced** in the "Scope Risk Traceability" table (lines 244-254). Each scope risk maps to architecture risks or specification constraints.
3. **36 test scenarios** across 4 priority levels (Critical: 9, High: 10, Medium: 14, Low: 3).
4. **Edge cases section** (lines 204-214) covers 9 boundary conditions including empty export, single-entry export, all-null optionals, zero-length file, and header-only file.
5. **Security risks** (lines 216-226) explicitly address SQL injection, path traversal, crafted JSON, and resource exhaustion with mitigations.

One observation: R-12 (large import performance) is classified Low/Low. Given that the SCOPE explicitly sets a 60-second target for 500 entries (AC-17), this risk warrants at least one explicit performance test, which is covered.

**Status: PASS**

## Knowledge Stewardship

- Queried: /query-patterns for vision alignment patterns -- no results (category: pattern, topic: vision). No prior alignment reviews found in the knowledge base.
- Stored: nothing novel to store -- the column list discrepancy between architecture and specification is feature-specific, not a generalizable pattern. If this type of inter-document inconsistency recurs across features, it would warrant a pattern entry about requiring DDL verification when source documents enumerate database columns.
