# Vision Alignment Report: col-013 Extraction Rule Engine

## Alignment Assessment

### Overall: PASS

col-013 directly implements the product vision's "Passive Knowledge Acquisition" pipeline (ASS-015). It is explicitly listed in the roadmap under Milestone 5 > Passive Knowledge Acquisition Features, with a detailed entry in PRODUCT-VISION.md. Every architectural decision and specification requirement traces to vision statements.

## Dimension Analysis

### 1. Strategic Direction: PASS

**Vision statement**: "Agents don't even need to ask -- Unimatrix delivers knowledge automatically via Claude Code's hook system. [...] The system gets better with every feature delivered."

**col-013 alignment**: Transforms Unimatrix from requiring explicit `context_store` calls to passively extracting knowledge from behavioral signals. This is the first concrete step toward the "self-learning" capability promised in the vision. The extraction rules detect knowledge gaps, conventions, dead knowledge, friction patterns, and file dependencies -- all without agent cooperation.

**Vision statement**: "With col-012/013: knowledge base self-populates from observation via rule-based extraction with quality gates."

**col-013 alignment**: Exact match. The feature delivers rule-based extraction with the specified quality gates.

### 2. Feature Scope: PASS

**Vision roadmap entry**: "5 initial rules: knowledge gap, implicit convention, dead knowledge, recurring friction, file dependency. Quality gate pipeline. Auto-extracted entries with trust_source: 'auto'. Automatic background maintenance. context_status becomes read-only. CRT refactors included. Absorbs col-005."

**col-013 scope**: All items present. No scope creep beyond vision. The Proposed entry status was explicitly deferred (documented in SCOPE.md Non-Goals) -- this is acceptable as it requires broader MCP tool changes.

### 3. CRT Integration: PASS

**Vision CRT integration table**:
- crt-002: "auto" trust_source (~5 lines) -- in col-013 spec
- crt-003: single-entry contradiction check (~30 lines) -- in col-013 spec
- crt-005: per-trust_source lambda (~40 lines) -- in col-013 spec
- crt-005: maintenance relocation (~100 lines) -- in col-013 spec

**col-013 alignment**: All four CRT refactors included at the specified scope.

### 4. Dependency Chain: PASS

**Vision dependency graph**:
```
col-012 -> col-013 -> crt-007 -> crt-008 -> crt-009
```

**col-013 alignment**: col-012 is a prerequisite (merged, commit 1ea06a2). col-013 does not pull forward any crt-007+ scope. The ExtractionRule trait is designed to be extensible for neural model integration in crt-007.

### 5. Architecture Principles: PASS

| Vision Principle | col-013 Alignment |
|-----------------|-------------------|
| Per-repo scope | Extraction scoped to single project's data directory |
| Session-based server | Background tick runs within session lifetime via tokio::spawn, not daemon |
| spawn_blocking pattern | All CPU-bound work via spawn_blocking |
| Fire-and-forget | Extraction writes are fire-and-forget |
| Existing test infrastructure | Reuses tempfile patterns, test-support features |
| Schema evolution via ALTER TABLE | No schema change needed (uses existing entries table) |

### 6. col-005 Absorption: PASS

**Vision note**: "col-005 (Auto-Knowledge Extraction) was originally blocked until 5+ feature retrospectives accumulated; it is now absorbed into col-013 which applies cross-feature validation gates instead of a hard retrospective count threshold."

**col-013 alignment**: col-005's three tiers (structural conventions, procedural knowledge, dependency graphs) map to rules 2 (ImplicitConventionRule), 4 (RecurringFrictionRule), and 5 (FileDependencyRule). Cross-feature validation gates replace the hard count threshold.

### 7. Behavioral Rules: PASS

| Rule | Compliance |
|------|-----------|
| Anti-stub | No TODOs or placeholders in spec |
| No files to root | All artifacts in product/features/col-013/ |
| Test infrastructure cumulative | Reuses existing fixtures, extends detection rule test patterns |

## Variances

### V-01: Proposed Entry Status Deferred (MINOR)

**Vision**: "status: Proposed for rules with 0.4-0.6 extraction confidence" (from ASS-015 feature scoping)

**col-013**: All entries stored as Active. Proposed status deferred because it requires new status handling across all MCP tools.

**Assessment**: Acceptable variance. The confidence score still reflects extraction certainty. Low-confidence auto-entries naturally rank lower via the trust_score mechanism. A follow-up can add Proposed status when the MCP tool infrastructure is ready.

### V-02: Extraction Cadence 15 Minutes vs "Hourly or Faster" (MINOR)

**Vision**: "Background maintenance tick (~1 hour)" in ASS-015 scoping.

**col-013**: 15-minute default interval per human guidance to "optimize for small incremental runs."

**Assessment**: Improvement over vision. More responsive extraction at negligible per-run cost. Aligns with human's explicit instruction.

### V-03: "auto" Trust Score 0.35 vs Vision's Implicit Range

**Vision**: trust_score range: human=1.0, system=0.7, agent=0.5, other=0.3. ASS-015 specifies "auto -> 0.35".

**col-013**: Matches ASS-015 exactly: "auto" => 0.35.

**Assessment**: Not a variance -- exact alignment with ASS-015 specification.

## Variance Summary

| ID | Type | Severity | Requires Approval |
|----|------|----------|-------------------|
| V-01 | Deferral | Minor | No (documented, follow-up planned) |
| V-02 | Enhancement | Minor | No (human-directed improvement) |

## Counts

- PASS: 7 dimensions
- WARN: 0
- VARIANCE: 2 (both minor, neither requires approval)
- FAIL: 0
