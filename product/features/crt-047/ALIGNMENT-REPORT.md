# Alignment Report: crt-047

> Reviewed: 2026-04-06
> Artifacts reviewed:
>   - product/features/crt-047/architecture/ARCHITECTURE.md
>   - product/features/crt-047/specification/SPECIFICATION.md
>   - product/features/crt-047/RISK-TEST-STRATEGY.md
> Scope sources reviewed:
>   - product/features/crt-047/SCOPE.md
>   - product/features/crt-047/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Curation health metrics serve the self-learning integrity engine vision |
| Milestone Fit | PASS | Correctly positioned as Cortical phase instrumentation; no future-milestone capability pulled in |
| Scope Gaps | PASS | All SCOPE.md goals and ACs are addressed across architecture and spec |
| Scope Additions | WARN | `corrections_system` column and `first_computed_at` ordering column added by architecture beyond SCOPE.md; both are warranted but deserve acknowledgment |
| Architecture Consistency | FAIL | ADR-001 and ADR-003 in ARCHITECTURE.md directly contradict SPECIFICATION.md on two load-bearing technical decisions; implementor receives irreconcilable blueprints |
| Risk Completeness | PASS | RISK-TEST-STRATEGY is unusually thorough; explicitly surfaces the architecture/spec contradictions and all open questions |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `corrections_system` column and struct field | SCOPE.md does not request this field; it appears as a resolved decision in ARCHITECTURE.md ADR-002 and in SPECIFICATION.md FR-01/FR-04 as optional/informational. Architecture ADR-gated it but includes it in the integration surface table unconditionally. |
| Addition | `first_computed_at` column on `cycle_review_index` | SCOPE.md specifies five new columns (AC-01, FR-08); ARCHITECTURE.md ADR-001 adds a sixth column `first_computed_at` to solve the `computed_at` ordering problem raised in SCOPE-RISK-ASSESSMENT SR-07. The column count change has cascading effects on DDL, struct, and migration tests. |
| Simplification | `deprecations_total` sourced from AUDIT_LOG | SCOPE.md background research discusses corrections from ENTRIES and deprecation via AUDIT_LOG join (Proposed Approach section). ARCHITECTURE.md ADR-003 resolves to an ENTRIES-only approach for orphan attribution. The SPECIFICATION reverts to AUDIT_LOG join for FR-05 and FR-06. The RISK-TEST-STRATEGY classifies this as a Critical contradiction (R-01). Rationale for the divergence is incomplete in the spec. |

---

## Variances Requiring Approval

### FAIL-01: Architecture ADR-003 contradicts Specification FR-05 / FR-06 on orphan attribution query

**What**: ARCHITECTURE.md ADR-003 resolves to an ENTRIES-only query for orphan attribution (using `status = Deprecated AND superseded_by IS NULL` at review time, no AUDIT_LOG join). SPECIFICATION.md FR-05 and FR-06 specify an AUDIT_LOG join (`operation = 'context_deprecate'`, `timestamp BETWEEN cycle_start_ts AND review_call_ts`) for both orphan attribution and `deprecations_total`. These are incompatible SQL designs for the same feature. The RISK-TEST-STRATEGY correctly classifies this as Risk R-01 (Critical, High likelihood) and states that "whichever path an implementor chooses, it will fail gate review against the other artifact."

**Why it matters**: This is not a nuance or a style difference. The two approaches produce different values for `deprecations_total` and `orphan_deprecations` whenever deprecations occur outside cycle windows or when AUDIT_LOG and ENTRIES disagree. An implementor cannot satisfy both documents simultaneously. Gate review will reject whichever document was not followed. The tester cannot write a conformance test without first knowing which document governs.

**Recommendation**: The Implementation Brief must choose one approach definitively before pseudocode begins. The RISK-TEST-STRATEGY (R-01) recommends proving the two approaches produce equivalent results on a known fixture before committing. If they are equivalent (as the test strategy hypothesizes), the SPECIFICATION should be updated to match ADR-003's ENTRIES-only approach for simplicity and to eliminate the AUDIT_LOG dependency. If they are not equivalent (i.e., `deprecations_total` semantics differ), the two documents must be reconciled with explicit rationale for the chosen definition. This variance must be resolved before delivery is authorized.

---

### FAIL-02: Architecture ADR-001 introduces `first_computed_at` column; Specification FR-10 uses `feature_cycle DESC` ordering — these are incompatible window selection strategies

**What**: ARCHITECTURE.md ADR-001 resolves to add a seventh column `first_computed_at` to `cycle_review_index` and orders the baseline window by `first_computed_at DESC WHERE first_computed_at > 0`. This directly addresses SCOPE-RISK-ASSESSMENT SR-07 (non-deterministic ordering when `force=true` mutates `computed_at`). SPECIFICATION.md FR-10 specifies `ORDER BY feature_cycle DESC LIMIT N` as the ordering key. The RISK-TEST-STRATEGY classifies this as Risk R-02 (Critical, High likelihood) and identifies EC-05: `feature_cycle` sorts alphabetically by phase prefix (alc, col, crt, nxs, vnc), not temporally, so cross-phase windows are incorrectly ordered.

**Why it matters**: If the implementor follows FR-10 (spec), `force=true` on a historical cycle will perturb the baseline window — the SR-07 problem ADR-001 was created to solve — and cross-phase cycles will be ordered incorrectly. If the implementor follows ADR-001 (architecture), they add a seventh column that FR-08 does not list, causing DDL, migration, struct, and AC-14 tests to fail against the specification. The column count discrepancy (5 per SCOPE.md, 6 per FR-08 plus corrections_system, or 7 per ADR-001) is directly contradictory across documents.

**Recommendation**: The Implementation Brief must select ADR-001 (first_computed_at ordering) over FR-10, and explicitly document the spec deviation. SPECIFICATION.md FR-08, FR-10, and the domain models section must be updated before delivery begins to reflect the actual column count and ordering approach. AC-14 must be updated to assert the `first_computed_at` column. This variance must be resolved before delivery is authorized.

---

### WARN-01: `corrections_system` field added beyond SCOPE.md scope

**What**: SCOPE.md does not mention `corrections_system`. ARCHITECTURE.md ADR-002 adds it as an informational field (excluded from agent/human totals). SPECIFICATION.md FR-01 and FR-04 include it. The RISK-TEST-STRATEGY (R-09) notes that if it is included in the struct but omitted from the DDL (or vice versa), a round-trip store/retrieve will silently lose the field — a correctness risk.

**Why it matters**: This is a scope addition. The field has minor operational value (exposing system/cortical-implant write volume). It is low-risk technically but represents unscoped work that accumulates across features. The DDL disagreement between ARCHITECTURE.md (includes `corrections_system` in `CycleReviewRecord` integration surface table) and SPECIFICATION.md FR-08 (lists only five columns, omits `corrections_system`) is a concrete inconsistency that must be resolved before delivery.

**Recommendation**: Decide whether `corrections_system` is stored (in `cycle_review_index` DDL) or computed-only (derived at query time, never persisted). Document the decision in the Implementation Brief. If stored, update FR-08 to list six columns. If computed-only, confirm the struct field is not bound in the `INSERT OR REPLACE`. Either is acceptable — the ambiguity is not.

---

### WARN-02: OQ-SPEC-01 (AUDIT_LOG outcome filter) is open in the Specification

**What**: SPECIFICATION.md "NOT In Scope" section notes that `context_deprecate` failed calls should not be counted as orphan deprecations, and marks this as "in-scope-but-ADR-gated." OQ-SPEC-01 remains open. This is contingent on whether the AUDIT_LOG join approach (now in conflict — see FAIL-01) is selected. If resolved toward ENTRIES-only (ADR-003), the risk is vacuous. If the AUDIT_LOG join is selected, failed `context_deprecate` calls will inflate orphan counts unless filtered.

**Why it matters**: An unresolved query behavior that silently inflates a health metric is directly contrary to the feature's purpose (accurate curation signal). It is contingent on FAIL-01 resolution, so it cannot be closed before FAIL-01 is resolved.

**Recommendation**: Close OQ-SPEC-01 in the same decision that closes FAIL-01. If ENTRIES-only is chosen: document OQ-SPEC-01 as vacuous and close. If AUDIT_LOG join is chosen: specify `outcome = 'Success'` filter, update FR-05, and add a test scenario to AC-04.

---

## Detailed Findings

### Vision Alignment

crt-047 is well-aligned with the product vision's "self-learning knowledge integrity engine" framing. The vision states that "everything is attributed, hash-chained for integrity, scored by real usage, and correctable with full provenance." Curation health metrics make the *correction process itself* observable — whether agents are catching and fixing drift in-flow, or whether drift is accumulating unnoticed. This directly serves the "ever-improving" and "trustworthy" integrity properties.

The feature is explicitly read-only at write paths, does not modify Lambda (which the vision has already restructured in #520), and adds a complementary behavioral signal rather than extending the structural integrity metric. The vision's Critical Gaps section notes "Intelligence pipeline is additive boosts, not a learned function" as a roadmapped concern. crt-047 does not address this gap (it is not in scope to do so) and does not conflict with W3-1's training signal architecture.

The rolling σ baseline design — self-calibrating against the corpus's own history — is consistent with the vision's principle of domain-agnostic configuration. No hardcoded thresholds are required; the signal adapts to each deployment's curation patterns.

**Finding**: PASS. No vision conflicts identified.

---

### Milestone Fit

crt-047 is a Cortical phase feature (crt- prefix), targeting the learning and drift detection layer. The Cortical phase spans learning, confidence evolution, contradiction detection, and coherence gate. Curation health measurement is a natural Cortical responsibility: it instruments the correction process to detect whether the knowledge base is being actively maintained.

The feature does not pull in Wave 2 deployment capabilities (OAuth, HTTP transport, containerization) or Wave 3 intelligence capabilities (GNN, session-conditioned relevance, MissedRetrieval). It operates entirely within the existing Wave 1 / Wave 1A infrastructure: sqlx dual-pool, cycle_review_index, ENTRIES, AUDIT_LOG, cycle_events, and the unimatrix_observe baseline pattern. No future-milestone capability is anticipated.

The schema migration (v23 → v24) is consistent with the incremental migration pattern established across Cortical features. The SUMMARY_SCHEMA_VERSION bump follows the ADR-002 (crt-033) policy.

**Finding**: PASS. Feature is correctly scoped to its milestone.

---

### Architecture Review

The architecture is structurally sound in its component decomposition:
- Two-layer change surface (`unimatrix-store` for schema, `unimatrix-server` for compute) is correct and consistent with existing patterns.
- Pool discipline table (ADR-001 crt-033 alignment: reads on `read_pool()`, writes on `write_pool_server()`) is explicitly documented and correctly applied.
- Pre-planned extraction to `services/curation_health.rs` (SR-06) avoids the reactive mid-delivery refactor that has caused prior delivery turbulence.
- The interaction diagram correctly shows compute-before-write ordering (read ENTRIES, then INSERT OR REPLACE into cycle_review_index), consistent with I-01 in the risk strategy.

**Critical finding**: ADR-001 (ordering key: `first_computed_at`) and ADR-003 (ENTRIES-only orphan attribution) are sound technical decisions that address real risks raised in the SCOPE-RISK-ASSESSMENT. However, neither ADR was absorbed back into the SPECIFICATION before documents were finalized. The SPECIFICATION still specifies the superseded design on both points. This is a document finalization failure, not an architectural problem — the architecture got it right, the spec was not updated.

**Finding**: Architecture is technically sound. However, the two ADRs that supersede SCOPE.md defaults (ADR-001, ADR-003) were not propagated to the SPECIFICATION. This creates the FAIL-01 and FAIL-02 contradictions. The architecture document itself is internally consistent.

---

### Specification Review

The SPECIFICATION is thorough in coverage: all 16 SCOPE.md acceptance criteria have corresponding FR, NFR, or AC entries, the domain model section is complete, the user workflow section is explicit, and the constraints section addresses all SCOPE-RISK-ASSESSMENT items. The force=true three-case decomposition (SR-05) is well-handled.

**Critical findings**:

1. **FR-10 vs. ADR-001** (see FAIL-02): FR-10 specifies `ORDER BY feature_cycle DESC LIMIT N` as the baseline window ordering key. ADR-001 in the architecture superseded this with `first_computed_at`. FR-10 was never updated. EC-05 in the risk strategy explicitly identifies that `feature_cycle` alphabetical order does not equal temporal order across phase prefixes — this is a known correctness defect in FR-10 that ADR-001 exists to fix.

2. **FR-05 / FR-06 vs. ADR-003** (see FAIL-01): FR-05 and FR-06 specify the AUDIT_LOG join approach. ADR-003 resolves to ENTRIES-only. The specification was not updated to reflect ADR-003.

3. **FR-08 column count** (see WARN-01, WARN-02): FR-08 lists five columns; `corrections_system` (included in ADR-002 and the integration surface table) is absent from FR-08. If `first_computed_at` is also added per ADR-001, FR-08 describes four fewer columns than will actually be implemented. AC-14 is written against the five-column expectation and will fail.

4. **OQ-SPEC-01 open**: The outcome-filter question on the AUDIT_LOG orphan query remains open in the specification. Contingent on FAIL-01 resolution but should be explicitly closed.

5. **OQ-SPEC-02 open**: `corrections_system` field disposition is marked ADR-gated but the ADR (ADR-002) has already resolved to include it. The specification should close OQ-SPEC-02 by citing ADR-002.

**Finding**: The specification has significant internal-consistency issues stemming from two unabsorbed architectural ADRs. Coverage of SCOPE.md is complete. The remaining issues are document synchronization failures that make the spec unsuitable for delivery handoff in its current form.

---

### Risk Strategy Review

The RISK-TEST-STRATEGY is the strongest of the three source documents. It:
- Correctly identifies R-01 and R-02 as Critical-priority contradictions between the architecture and specification.
- Names the exact FR numbers, ADR numbers, and line numbers in question.
- Requires the Implementation Brief to resolve both before pseudocode begins.
- Covers all SCOPE-RISK-ASSESSMENT items (SR-01 through SR-08) in the traceability table.
- Adds seven additional risks not in the SCOPE-RISK-ASSESSMENT, including R-07 (first_computed_at upsert clobbering — a concrete SQLite INSERT OR REPLACE trap), R-05 (DEFAULT-0 legacy row baseline contamination), and R-04 (corrections_total accounting contradiction between FR-03/FR-04 and ADR-002).
- Identifies EC-05 (feature_cycle alphabetical-not-temporal ordering) as a correctness defect.
- Coverage requirements are specific: named constants, exact test counts, mandatory negative assertions (AC-12 no-side-effect test).

The risk strategy's self-awareness about the architectural contradictions is notable. By surfacing R-01 and R-02 at Critical priority, it effectively flags that the design documents are not ready for delivery without remediation.

**One gap**: R-08 (OQ-SPEC-01 outcome filter) is contingent on FAIL-01 resolution. The risk strategy notes this correctly. It should be explicitly closed in the Implementation Brief as part of the FAIL-01 resolution.

**Finding**: PASS. Risk coverage is complete and unusually thorough. The risk strategy correctly identifies all FAIL-level concerns in this report as Critical risks, demonstrating alignment between the risk strategy and this review.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #2298 (config key semantic divergence), #3742 (optional future branch architecture/risk divergence from scope), #3337 (architecture diagram informal headers diverge from spec). Pattern #3742 is directly applicable: architecture ADRs that diverge from SCOPE.md defaults must be propagated to the specification before delivery authorization. This feature's FAIL-01 and FAIL-02 are exactly this pattern (ADR-001, ADR-003 not absorbed into spec).
- Stored: nothing novel from this review — the "unabsorbed ADR" pattern is captured in #3742. The specific form (architecture ADR resolves a scope-raised open question, but spec is not updated) is consistent with the existing pattern. No new entry warranted; pattern #3742 covers it.
