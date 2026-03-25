# Alignment Report: col-027

> Reviewed: 2026-03-25
> Artifacts reviewed:
>   - product/features/col-027/architecture/ARCHITECTURE.md
>   - product/features/col-027/specification/SPECIFICATION.md
>   - product/features/col-027/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/col-027/SCOPE.md
> Scope risk assessment: product/features/col-027/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly repairs the observation pipeline described in W1-5; strengthens behavioral signal quality for W3-1 |
| Milestone Fit | PASS | Collective-phase correctness fix; no future-milestone capabilities pulled in |
| Scope Gaps | PASS | All 7 goals from SCOPE.md are addressed by the source documents |
| Scope Additions | WARN | Architecture introduces `extract_error_field()` as a named function — a minor structural addition not listed in SCOPE.md; architecturally sound but worth noting |
| Architecture Consistency | PASS | Follows col-023 ADR-001 (string constants), fire-and-forget transport, no schema migration; two-site atomic fix correctly specified |
| Risk Completeness | PASS | 14 risks registered; all 8 SCOPE-RISK-ASSESSMENT items traceable; security section included; edge cases covered |

**Overall: PASS with one WARN.** No variances requiring approval. The WARN is informational.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `data_quality_note` in retrospective output (SR-06) | SCOPE-RISK-ASSESSMENT recommends a lightweight `data_quality_note` field for pre-col-027 retrospectives. All three source documents explicitly accept SR-06 as a follow-on non-goal. Rationale documented in SCOPE.md §Non-Goals and SPECIFICATION.md §NOT in Scope. Acceptable. |
| Addition | `extract_error_field()` named sibling function | SCOPE.md describes error extraction as a behavior (extract from `payload["error"]`), not as a named function. ARCHITECTURE.md introduces `extract_error_field()` as a distinct, named sibling to `extract_response_fields()`. This is an architecturally motivated design choice (ADR-002 col-027), not a scope expansion. Informational. |
| Addition | `terminal_counts` variable rename in `PermissionRetriesRule` | SCOPE.md specifies fixing the differential; it does not prescribe the internal variable name. ARCHITECTURE.md (ADR-004) renames `post_counts` to `terminal_counts` to clarify intent. Implementation detail; no external surface change. Informational. |

No scope gaps found. Every goal from SCOPE.md §Goals (items 1–7) is addressed:

| SCOPE.md Goal | Addressed In |
|---------------|-------------|
| 1. Register PostToolUseFailure in settings.json | ARCHITECTURE Component 1; SPEC FR-01; RTS R-14 |
| 2. Implement PostToolUseFailure handler in build_request() | ARCHITECTURE Component 2; SPEC FR-03; RTS R-05, R-08 |
| 3. Add extract_observation_fields support | ARCHITECTURE Component 4; SPEC FR-04; RTS R-01, R-03 |
| 4. Add hook_type::POSTTOOLUSEFAILURE constant | ARCHITECTURE Component 3; SPEC FR-02; RTS R-11 |
| 5. Update PermissionRetriesRule to exclude PostToolUseFailure from differential | ARCHITECTURE Component 5a; SPEC FR-05; RTS R-02, R-04 |
| 6. Add ToolFailureRule | ARCHITECTURE Component 6; SPEC FR-07; RTS R-06, R-07, R-13 |
| 7. Update permission_friction_events computation | ARCHITECTURE Component 5b; SPEC FR-06; RTS R-02, R-12 |

---

## Variances Requiring Approval

None. The single WARN is informational and does not require human approval.

---

## Detailed Findings

### Vision Alignment

PASS.

col-027 directly serves two product vision concerns.

**W1-5 Observation Pipeline Generalization** (PRODUCT-VISION.md, currently "IN PROGRESS"): The vision states "Replace HookType closed enum with ObservationEvent { event_type: String, ... }. Rewrite all 21 detection rules to operate on the generic event schema." col-027 operates on this exact layer — it fixes the event ingest path to correctly handle a hook event type the pipeline missed entirely. The fix is consistent with the string-based hook_type architecture col-023 established (ADR-001 in observation.rs, referenced in ARCHITECTURE.md §Technology Decisions and SPEC §Constraints C-02 and NFR-06).

**W3-1 GNN Training Signal Quality** (PRODUCT-VISION.md, "Roadmapped"): The vision describes W1-5 behavioral signals (re-search, rework, successful phase completion) as the training data for W3-1. PRODUCT-VISION.md, W1-5 section: "W1-5 is the behavioral signal collection layer. Without it, the observation pipeline generates only Claude Code-specific training labels." Tool failure events are behavioral signals. Every session since nan-002 has had a corrupted behavioral signal (false PermissionRetries). col-027 restores signal integrity. This is not explicitly named in the vision roadmap but is directly implied — corrupted signals in the observation layer contaminate the data that feeds the intelligence pipeline.

**Domain Coupling gap: HookType enum tied to Claude Code events** (PRODUCT-VISION.md §Critical Gaps, status "In progress — col-023 / W1-5"): col-027 follows the string-constants pattern rather than extending any enum, consistent with the vision's direction for this gap.

The feature does not extend into future-milestone capabilities. It is purely corrective within the existing observation pipeline.

### Milestone Fit

PASS.

col-027 is a Collective-phase (`col`) correctness fix. It targets the observation and retrospective pipeline built in col-002, col-002b, and related features. The change is additive at the schema level (no migration, SPEC NFR-03) and forward-only at the data level (SCOPE.md §Non-Goals, SPEC §NOT in Scope).

Nothing in the source documents reaches into Wave 1A (session context, GNN training, proactive delivery), Wave 2 (deployment, OAuth), or Wave 3 (learned relevance function) capabilities. The two most "advanced" touches in the feature — `ToolFailureRule` and the Pre-Post differential fix — are squarely within the existing col-002 detection rule framework.

The SCOPE.md explicitly defers: PermissionRetriesRule rename (col-028), ToolFailureRule threshold configuration (follow-on), `data_quality_note` field (follow-on), error message classification (follow-on). These are all appropriate milestone-discipline decisions for a correctness fix.

### Architecture Review

PASS.

**Component coverage**: All six components (hook registration, hook dispatcher, core constants, storage layer, Pre-Post differential fix, ToolFailureRule) are fully specified with responsibilities, constraints, and integration surface signatures. The Integration Surface table (ARCHITECTURE.md §Integration Surface) provides concrete function signatures including `extract_error_field()` return type and `ToolFailureRule` trait implementation.

**SR-01 / SR-07 mitigation (highest-risk items)**: The SCOPE-RISK-ASSESSMENT flagged these as combined-highest-risk. ARCHITECTURE.md resolves both via ADR-002: a separate `extract_error_field()` function that reads `payload["error"]` as a plain string, never calling `extract_response_fields()`. The call-site separation makes the `error`-vs-`tool_response` distinction explicit and compiler-enforced (distinct function, not runtime conditional). This directly implements the SCOPE-RISK-ASSESSMENT recommendation: "implement `extract_response_fields()` to accept a named-field hint ... rather than probing field names at runtime."

**SR-08 (two-site atomic update)**: ARCHITECTURE.md Component 5 and ADR-004 mandate the same-commit delivery of both metrics.rs and friction.rs fixes. SPEC FR-06.4 couples AC-05/AC-06/AC-07 explicitly. RISK-TEST-STRATEGY R-02 adds a cross-site assertion test requirement (both sites must be tested in one test function). This is a thorough, multi-layer enforcement of SR-08.

**SR-04 (blast radius audit)**: The SCOPE-RISK-ASSESSMENT asked for an "explicit detection rule audit table" per rule. ARCHITECTURE.md §Detection Rule Audit provides this: all 21 rules assessed, two require action (PermissionRetriesRule, compute_universal), all others are explicitly confirmed as "no action — distinct string." SPEC FR-08 formalizes this as a functional requirement.

**No schema migration**: Correct per SPEC NFR-03 and ARCHITECTURE.md §System Overview. The `observations.hook TEXT` column has no enum constraint; the new event type string requires no migration.

**One minor note (informational)**: ARCHITECTURE.md §Open Questions declares "None. SR-06 is explicitly out of scope." SR-06 (data quality caveat for pre-col-027 retrospectives) appears in the SCOPE-RISK-ASSESSMENT as a "Consider" recommendation, not a requirement. The source documents correctly treat it as a non-goal. However, a future consumer building metric trends across features will encounter this gap silently. The risk is accepted and documented; no architecture action is needed for col-027.

### Specification Review

PASS.

**AC traceability**: All 12 SCOPE.md acceptance criteria are carried forward in SPECIFICATION.md §Acceptance Criteria with verification steps added. The spec notes that AC-05, AC-06, and AC-07 are coupled (SR-08), which is the correct coupling per SCOPE-RISK-ASSESSMENT.

**FR completeness**: FR-01 through FR-08 cover every goal item. FR-08 (Detection Rule Audit) is a notable addition beyond the SCOPE goal list — it converts the SCOPE-RISK-ASSESSMENT SR-04 recommendation into a formal functional requirement (FR-08.1 through FR-08.3). This is appropriate and strengthens the feature.

**NFR completeness**: NFR-01 (latency), NFR-02 (defensive parsing), NFR-03 (no schema migration), NFR-04 (test helper pattern), NFR-05 (test count baseline), NFR-06 (string constant discipline) are all present. NFR-04 adds the `make_failure` helper requirement, directly addressing the test infrastructure pattern from SCOPE.md §Constraints.

**Constraint section**: C-06 explicitly prohibits reuse of `extract_response_fields()` for error extraction, locking in the ADR-002 architectural decision at the spec level. This is good defense-in-depth against the SR-01 risk.

**ToolFailureRule rule name discrepancy (informational)**: SCOPE.md AC-08 states `rule_name = "tool_failures"`. SPECIFICATION.md FR-07.4 and AC-08 both state `rule_name = "tool_failure_hotspot"`. ARCHITECTURE.md Component 6 states `Rule name: "tool_failures"`. The architecture and scope use `"tool_failures"`; the specification uses `"tool_failure_hotspot"`. This is an internal naming inconsistency across documents. It does not affect correctness — the implementer will use one name — but could cause confusion. Noted as informational; the spec should be treated as authoritative for implementation since it carries the verification step.

### Risk Strategy Review

PASS.

**Risk register completeness**: 14 risks registered, spanning all seven SCOPE goals. The SCOPE-RISK-ASSESSMENT's 8 items (SR-01 through SR-08) are all represented, with the highest-severity items (SR-01, SR-07, SR-08) mapped to Critical/High priority risks (R-01, R-03, R-02).

**Security section**: RISK-TEST-STRATEGY includes a Security Risks section covering untrusted input via `payload["error"]` (potential stored XSS or log injection) and `tool_input` depth in topic_signal extraction. Both are bounded by the existing ADR-007 col-023 ingest security bounds. The 500-char truncation for `response_snippet` is confirmed as blast-radius limiting. Appropriate for this feature's risk profile.

**Edge cases**: The edge cases section covers: empty observation set, exactly-at-threshold (3 failures), multiple tools multiple findings, failure with empty error string, error string exactly 500 chars, `is_interrupt: true` in payload, tool_name present but error absent, server not running on failure. This is comprehensive.

**Failure modes table**: All failure modes are paired with expected behaviors. The "tool_name absent" failure mode correctly specifies `obs.tool = None` with graceful skip in ToolFailureRule — consistent with SPEC FR-04.2.

**Scope Risk Traceability table**: Every SR item from SCOPE-RISK-ASSESSMENT is traced to an architecture decision, a risk in the register, and a resolution status. SR-06 and SR-05 are explicitly marked "Accepted" with rationale. This is a clean, complete traceability chain.

**Rule name note (informational)**: RISK-TEST-STRATEGY R-13 states `name() == "tool_failure_hotspot"`, consistent with SPECIFICATION.md but inconsistent with SCOPE.md/ARCHITECTURE.md (which use `"tool_failures"`). Same informational note as above.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #2298 (config key semantic divergence from vision example), #3337 (architecture diagram header divergence from spec), #3426 (formatter features underestimate section-order regression risk). None of these patterns apply to col-027's risk profile directly. col-027 is an observation pipeline correctness fix with no formatter, config, or diagram-header risk surface.
- Stored: nothing novel to store — col-027 variances are feature-specific (ToolFailureRule rule name inconsistency across documents is a minor cross-document naming drift, not a generalizable pattern). The three source documents show strong alignment overall; no recurring misalignment pattern visible that would generalize to future features.
