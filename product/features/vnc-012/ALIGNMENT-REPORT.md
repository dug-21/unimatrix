# Alignment Report: vnc-012

> Reviewed: 2026-03-29 (re-check after specification updates)
> Artifacts reviewed:
>   - product/features/vnc-012/architecture/ARCHITECTURE.md
>   - product/features/vnc-012/specification/SPECIFICATION.md
>   - product/features/vnc-012/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Fix restores reliable knowledge delivery through the MCP dispatch layer |
| Milestone Fit | PASS | Infrastructure quality fix; no future milestone capabilities built |
| Scope Gaps | PASS | All SCOPE.md open questions resolved; no gaps remain |
| Scope Additions | PASS | No items in source docs exceed SCOPE.md; hard boundaries respected |
| Architecture Consistency | PASS | Architecture aligns with spec; ADR-003 now consistent with SPEC |
| Risk Completeness | PASS | Risk register thorough; all scope risks traced; edge cases and security covered |

---

## Prior Variances — Resolution Status

| Prior Finding | Prior Status | Current Status | Evidence |
|--------------|-------------|---------------|---------|
| VARIANCE 1: Python infra-001 IT-01/IT-02 excluded from SPEC, required by ARCHITECTURE | VARIANCE | RESOLVED | SPECIFICATION.md now contains explicit AC-13 (Rust) + IT-01 + IT-02 (Python infra-001, `@pytest.mark.smoke`) all marked required. "NOT In Scope" no longer excludes Python tests. |
| WARN 1: FR-13 (float JSON Number rejection via `visit_f64`) absent from SPEC FR/AC | WARN | RESOLVED | SPECIFICATION.md now contains FR-13 (float JSON Number rejection normative requirement) and AC-09-FLOAT-NUMBER (test prescription asserting `visit_f64` returns error, not truncated integer). |

Both prior variances are fully resolved.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | None | No simplifications in source docs |
| Gap | None | All SCOPE.md items (9 fields, 3 helpers, serde_util module, schema preservation, integration tests) addressed in all three source documents |
| Addition | None | Nothing in source docs exceeds SCOPE.md boundaries |

---

## Variances Requiring Approval

None. All checks pass. No VARIANCE or FAIL classifications in this review.

---

## Detailed Findings

### Vision Alignment

vnc-012 is an MCP server reliability fix. The product vision describes Unimatrix as a "self-learning knowledge integrity engine" where "nothing is merely stored — everything is attributed, hash-chained for integrity, scored by real usage, and correctable with full provenance." The MCP server is the delivery mechanism for all knowledge tools. A recurring deserialization failure that returns an MCP error to agents directly undermines the engine's usability and breaks attribution, correction, and confidence feedback loops silently. The vision's "integrity chain runs through all of it" applies at the tool dispatch layer.

The fix is correctly scoped to the Vinculum phase (MCP server). It does not touch the knowledge graph, intelligence pipeline, or Wave 1/1A features. No vision principle is undercut. PASS.

### Milestone Fit

The product roadmap does not assign this fix to a specific wave — it is an infrastructure quality item surfaced by observed agent behavior. It does not build Wave 2 or Wave 3 capabilities ahead of schedule. The scope explicitly excludes OAuth/token identity (W2-3), HTTP transport (W2-2), session conditioning (WA-1/WA-2), and GNN/confidence evolution (W3-1). No future milestone capabilities are built. PASS.

### Architecture Review

ARCHITECTURE.md is internally consistent and addresses all four SCOPE.md open questions via ADRs:

- ADR-001: `mcp/serde_util.rs` submodule placement
- ADR-002: `#[schemars(with = "T")]` for schema preservation
- ADR-003: Mandatory integration test in infra-001 (SR-03) — now consistent with SPEC
- ADR-004: Mandatory `None`-for-absent tests (SR-05)

The component breakdown (serde_util.rs new, tools.rs modified, mod.rs modified, infra-001 two new tests) is minimal and correct. The call-chain diagram accurately represents `Parameters<T>` transparent serde delegation. The `#[serde(default)]` requirement on all five optional fields is correctly identified as the primary correctness risk and handled in both architecture and spec. PASS.

### Specification Review

The specification is detailed and comprehensive. The prior two issues are resolved:

1. FR-13 is now present: "Float JSON Numbers... passed to any integer or usize field must be rejected with a serde error. The Visitor implementations must implement `visit_f64` (and `visit_f32`) to return `de::Error::invalid_type(de::Unexpected::Float(v), &self)`. Silent truncation... is forbidden."

2. IT-01 and IT-02 are now required in SPECIFICATION.md under "Integration Test — MCP Dispatch Path (SR-03)": both marked `@pytest.mark.smoke`, with rationale ("cover the transport layer that AC-13's Rust test cannot exercise directly"). The "NOT In Scope" section no longer contains any exclusion of Python infra-001 tests.

FR coverage maps cleanly to all nine fields and three helpers. AC coverage enumerates absent-field tests and null-field tests as distinct numbered criteria — addressing the SR-02 greenfield serde trap explicitly. OQ-04 is resolved by ARCHITECTURE.md (Rust in-process test via `ToolCallContext`). OQ-05 is resolved by FR-13 and AC-09-FLOAT-NUMBER. No open questions remain in SPEC that are delegated to the architect without resolution in another source document. PASS.

### Risk Strategy Review

RISK-TEST-STRATEGY.md is well-structured and correctly prioritized. R-02 coverage requirement now matches SPEC: "Both Rust AC-13 AND Python IT-01/IT-02 are required. Neither alone is sufficient." R-06 (float JSON Number) is marked RESOLVED with reference to FR-13 and AC-09-FLOAT-NUMBER, consistent with the updated SPEC.

The risk-to-scope-risk traceability table at SR-03 row now references both Rust AC-13 and Python IT-01/IT-02, consistent with SPEC. The failure modes table correctly documents float Number behavior as serde error (not panic). Security risks are concise and correct. Edge cases enumerate i64::MIN, i64::MAX, whitespace-padded strings, and boolean/array/object inputs. Knowledge Stewardship block is present with specific entry citations. PASS.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — found 3 entries: #2298 (config key semantic divergence), #3337 (architecture diagram informal headers diverge from spec), #3742 (optional future branch in architecture must match scope intent — WARN if architecture and risk diverge from scope deferral). Entry #3742 informed VARIANCE 1 in the prior review; that variance is now resolved.
- Stored: nothing novel to store — the prior report flagged that if the "spec non-goal contradicts architecture ADR" pattern recurred across 2+ features it would warrant a stored pattern. In this re-check both variances resolved cleanly via spec updates, so the pattern remains feature-specific and does not yet generalize. No entry stored.
