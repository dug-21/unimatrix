# Alignment Report: vnc-014

> Reviewed: 2026-04-22
> Artifacts reviewed:
>   - product/features/vnc-014/architecture/ARCHITECTURE.md
>   - product/features/vnc-014/specification/SPECIFICATION.md
>   - product/features/vnc-014/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/vnc-014/SCOPE.md
> Scope risk source: product/features/vnc-014/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances audit integrity and hash-chain non-negotiables |
| Milestone Fit | PASS | Wave 2 / compliance prerequisite; correctly scoped before OAuth (W2-3) |
| Scope Gaps | PASS | All SCOPE.md goals are addressed in source documents |
| Scope Additions | WARN | `build_context_with_external_identity()` Seam 2 overload ships a W2-3 activation path not requested in SCOPE.md goals |
| Architecture Consistency | PASS | Architecture, specification, and risk strategy are internally consistent |
| Risk Completeness | WARN | SEC-02 JSON injection risk identified in RISK-TEST-STRATEGY.md is not fully resolved — `serde_json` serializer is recommended but not mandated in SPECIFICATION.md |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `cycle_events` gap not fixed | SCOPE.md Goal 1 targets `audit_log` attribution only. `cycle_events` for Codex/Gemini is an explicit non-goal in all three source documents. Rationale is sound: different root cause, separate work stream. |
| Addition | `build_context_with_external_identity()` Seam 2 overload | SCOPE.md Goals 1–5 do not mention shipping a W2-3 bearer-auth activation seam. The architecture and spec both proactively ship `Option<&ResolvedIdentity>` in the new function signature, with the intent of shrinking W2-3's implementation surface. This is a scope addition relative to SCOPE.md — the function itself is needed (to read rmcp session ID), but the `external_identity: Option<&ResolvedIdentity>` parameter is a forward-compatibility addition beyond what vnc-014 requires. |
| Addition | `ResolvedIdentity` stub type | Directly consequential to the Seam 2 addition above. SCOPE.md does not request this type. Its placement (`unimatrix-server` vs `unimatrix-core`) is an open question (OQ-A in SPECIFICATION.md) that could affect W2-3 delivery. |
| Simplification | `gc_audit_log` no-op / removal | Removing GC rather than replacing it is a correctness-driven simplification. SCOPE.md does not mention GC. The change is mandated by the append-only triggers and is the correct decision (ADR-005). Documented explicitly. |

---

## Variances Requiring Approval

### 1. Seam 2 Overload — Scope Addition (WARN)

**What**: `build_context_with_external_identity()` ships with `Option<&ResolvedIdentity>` as the fifth parameter. In vnc-014 this is always `None`. The architecture document (Component 7, Integration Surface table) explicitly frames this as a forward-compatibility surface for W2-3 bearer middleware. A new stub type `ResolvedIdentity` is introduced with an unresolved placement question (OQ-A).

**Why it matters**: SCOPE.md Goals 1–5 request server-side session attribution and the ASS-050 schema migration. They do not request a W2-3 seam. The addition is a known pattern risk (pattern #3742: "Optional future branch in architecture must match scope intent"). The seam is not harmful, but it carries a W2-3 dependency risk: if `ResolvedIdentity` is placed in the wrong crate (RISK-TEST-STRATEGY.md R-07), W2-3 must move it, causing a signature-breaking change in an already-shipped function.

**Recommendation**: Accept the addition (the function is architecturally necessary for the rmcp session ID lookup regardless; only the fifth parameter is extra scope), but require the architect to resolve OQ-A before delivery begins. Placement of `ResolvedIdentity` in `unimatrix-core` vs `unimatrix-server` must be a closed decision — not deferred to the implementation agent — to prevent R-07 from materializing.

---

### 2. SEC-02 JSON Injection — Mitigation Gap (WARN)

**What**: RISK-TEST-STRATEGY.md SEC-02 identifies that the `metadata` JSON construction using format string concatenation (`format!(r#"{{"client_type":"{}"}}"`, ...)`) escapes `"` but does not escape `\`, `\n`, or other JSON-special characters. The risk document recommends using `serde_json::json!` instead. SPECIFICATION.md FR-10 describes the same format string approach from SCOPE.md without mandating `serde_json`. RISK-TEST-STRATEGY.md EC-06 adds a test scenario for JSON injection strings. The architecture document (Attribution Population Rules) repeats the same format-string pseudocode without a correction.

**Why it matters**: The product vision non-negotiable "Hash chain integrity is immutable" and "Audit log is append-only and complete" presuppose that audit records are valid and trustworthy. A `clientInfo.name` containing `\` or `"}` would produce malformed or injected JSON in the `metadata` column, corrupting compliance evidence that downstream tooling (W2-3) will depend on. SEC-02 in the risk strategy correctly identifies this as a blast radius risk — it is not resolved by test coverage alone.

**Recommendation**: Require the specification to mandate `serde_json::json!` (or equivalent proper serializer) for `metadata` construction in FR-10, not as an implementation option but as a requirement. The test scenarios in EC-06 are necessary but not sufficient — the format-string pattern must be prohibited in the spec. This is a pre-delivery spec correction, not a delivery-phase decision.

---

## Detailed Findings

### Vision Alignment

VNC-014 directly advances two product vision non-negotiables:

1. **"Audit log is append-only and complete."** The feature installs DDL triggers (`BEFORE UPDATE`, `BEFORE DELETE`) that enforce this guarantee at the SQLite layer for the first time. Previously the audit log's append-only semantics were a convention, not an enforcement. This is a clear vision advancement.

2. **"Every operation attributed and logged."** `agent_attribution` provides transport-attested (non-spoofable) client identity — `clientInfo.name` for OSS, JWT `sub` for enterprise (W2-3). This directly addresses the product vision statement: "hash-chained for integrity, scored by real usage, and correctable with full provenance."

The feature is also correctly positioned as prerequisite infrastructure for Wave 2 OAuth (W2-3), where `agent_attribution` becomes the W2-3 `jwt_sub` field — the same column populated from a different source. The two-field attribution model (spoofable `agent_id` for routing vs non-spoofable `agent_attribution` for compliance — ADR-007) is consistent with the vision's stated security threat model progression: "Wave 0 — daemon-local (hardened) ... agent_id per-call model: friction, unreliable, spoofable" is listed as a High severity security gap to be closed by W2-3.

No vision principles are violated or shortcut.

---

### Milestone Fit

The feature is correctly placed. The product vision positions audit trail hardening and multi-client attribution as Wave 2 prerequisites (before W2-3 OAuth). VNC-014:

- Delivers the ASS-050 schema migration mandated by prior research
- Adds the `agent_attribution` column that W2-3 JWT middleware will populate
- Ships the `build_context_with_external_identity()` Seam 2 signature that W2-3 will activate

No Wave 3 capabilities are claimed or implemented. No Wave 1A intelligence pipeline features are touched. The feature is appropriately narrow.

---

### Architecture Review

The architecture is internally consistent and well-structured. Key observations:

**Strengths:**
- The component interaction diagram accurately reflects all data flows from `initialize` through `client_type_map` to `AuditEvent` construction.
- ADR-003 (remove `build_context()` entirely rather than wrapping) is the correct choice and eliminates the R-05 risk of missed call sites through compile-time enforcement.
- ADR-005 (remove `gc_audit_log` rather than no-op it) is the correct response to append-only triggers — a deletion-based GC is semantically incompatible with audit immutability.
- The `Arc<Mutex<HashMap>>` vs `DashMap` decision (ADR-001) is appropriate for current concurrency bounds, with explicit W2-2 deferral.
- The "Removed method: `build_context()`" section is explicit and verifiable.

**Concern (WARN — Seam 2):**
The architecture ships `build_context_with_external_identity()` with `Option<&ResolvedIdentity>` as a deliberate forward-compatibility seam. This is documented in the architecture's Integration Surface table and explicitly flagged as "Seam 2 Forward-Compatibility Surface (W2-3)." The open question OQ-A (crate placement for `ResolvedIdentity`) is noted but not resolved in the architecture document. Per pattern #3742, optional future branches in architecture must match scope intent — this one is explicit and acknowledged, which satisfies the WARN threshold rather than elevating to VARIANCE.

**Minor note:**
Architecture OQ-2 notes three `background.rs` audit event construction sites at lines 1197, 1252, and 2267. The architecture directs these to use `..AuditEvent::default()`. This is correct, but the delivery agent must verify these are the only non-tool-call sites (OQ-2 is not resolved in the architecture document). This is a delivery-phase risk (R-12 in RISK-TEST-STRATEGY.md) and is adequately tracked there.

---

### Specification Review

The specification is comprehensive and correctly maps all SCOPE.md goals to functional requirements.

**SCOPE.md Goal mapping:**
- Goal 1 (capture `clientInfo.name` at `initialize`): FR-01, FR-02
- Goal 2 (bind to rmcp session ID): FR-01, FR-02, FR-03
- Goal 3 (propagate to audit_log): FR-04, FR-09, FR-10
- Goal 4 (ASS-050 four-column migration): FR-05, FR-06, FR-07, FR-08
- Goal 5 (zero behavioral regression): NFR-03, AC-03

Acceptance criteria are complete (AC-01 through AC-12). All 12 tool handlers are named explicitly in FR-04. The domain models section clearly distinguishes `agent_id` (spoofable) from `agent_attribution` (non-spoofable) — this distinction is the semantic core of the feature.

**FR-10 weakness (SEC-02 gap):**
FR-10 describes the `metadata` construction as: "Escape any double-quote characters in `ct` by replacing `"` with `\"`." This is the same incomplete escaping from SCOPE.md. RISK-TEST-STRATEGY.md SEC-02 identifies this as a blast-radius risk and recommends `serde_json`. FR-10 does not adopt this recommendation — it specifies the same format-string approach. EC-06 adds a test for JSON injection strings, but a test does not prevent the wrong implementation from shipping if the spec mandates the wrong approach. This is the most significant specification gap.

**C-06 (ResolvedIdentity stub):**
The specification defers the crate placement decision (OQ-A) to the architect. This is appropriate for a specification document, but it means the delivery agent will make this decision by default if OQ-A is not resolved before delivery begins. R-07 in RISK-TEST-STRATEGY.md correctly flags the consequence: a crate placement mistake requires a signature change in an already-shipped function.

---

### Risk Strategy Review

The risk strategy is thorough and well-calibrated. All SCOPE-RISK-ASSESSMENT.md risks (SR-01 through SR-08) are traced to architecture risks and resolved or accepted with rationale.

**Coverage assessment:**
- All 15 risks (R-01 through R-15) have test scenarios.
- 4 security risks (SEC-01 through SEC-04) have dedicated scenarios.
- 8 edge cases (EC-01 through EC-08) are defined.
- 4 integration risks (IR-01 through IR-04) are documented.
- 6 failure modes (FM-01 through FM-06) are documented.

**Calibration:**
- R-01 (append-only triggers break DELETE paths) correctly classified Critical — the blast radius is production runtime failure.
- R-02 (schema version cascade) correctly classified Critical — 7+ touchpoints are named.
- R-03 (cross-session attribution bleed) correctly classified Critical — compliance integrity.

**SEC-02 (JSON injection) assessment:**
SEC-02 is correctly identified as High severity. The recommended mitigation (use `serde_json::json!`) is correct. However, the risk strategy marks this as "mitigation required" while EC-06 provides a test for it — but the test is parametric/property-based, which requires the implementation agent to interpret the requirement. The spec (FR-10) does not adopt the mitigation, leaving a gap between the risk document's recommendation and the implementation contract. This gap is the basis for the WARN in the Summary table.

**R-13 (`serde(default)` produces `""` for metadata):**
This risk is correctly identified and well-analyzed. The resolution (separate serde path from construction path, document the distinction) is appropriate. This is a subtle but real risk — the `Default` impl must produce `"{}"` while `serde(default)` produces `""`, and both are correct for their respective contexts.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `vision` — found patterns #2298 (config key semantic divergence), #3742 (optional future branch in architecture), #3337 (architecture diagram header divergence), #3158 (deferred scope leaves AC references live). Pattern #3742 directly applies to the Seam 2 addition and informed the WARN classification.
- Stored: nothing novel to store at this time. The Seam 2 forward-compatibility pattern and SEC-02 JSON injection risk are feature-specific combinations not yet seen as a recurring cross-feature pattern. If SEC-02-class gaps (spec does not adopt risk mitigation recommendation) appear in a second feature, a pattern entry should be created.
