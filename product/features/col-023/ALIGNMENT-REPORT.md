# Alignment Report: col-023

> Reviewed: 2026-03-21
> Artifacts reviewed:
>   - product/features/col-023/architecture/ARCHITECTURE.md
>   - product/features/col-023/specification/SPECIFICATION.md
>   - product/features/col-023/RISK-TEST-STRATEGY.md
> Scope reviewed:
>   - product/features/col-023/SCOPE.md
>   - product/features/col-023/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly addresses a named Critical/Medium gap in the vision; W1-5 goals met |
| Milestone Fit | PASS | Correctly placed in Wave 1 (Intelligence Foundation); unblocks W3-1 as intended |
| Scope Gaps | PASS | All SCOPE.md goals and acceptance criteria are addressed in source docs |
| Scope Additions | WARN | Minor spec additions beyond SCOPE.md (EC-04 reserved "unknown" domain, EC-07 overlap semantics, EC-08/09 rule validation edge cases) — benign but undeclared |
| Architecture Consistency | PASS | Architecture resolves all open questions from SCOPE.md; ADRs are internally consistent |
| Risk Completeness | PASS | 14 risks identified and mapped; all scope risks traced; non-negotiable tests named |
| FR-06 Conflict | VARIANCE | SPECIFICATION.md retains FR-06 (Admin runtime override) in full despite ADR-002 removing it from W1-5 scope. This is the known conflict flagged by the risk strategy as a gate-entry blocker. |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | None | All five SCOPE.md goals covered in source docs |
| Addition | EC-04: "unknown" reserved domain | RISK-TEST-STRATEGY adds a requirement to reject `source_domain = "unknown"` as an explicit registration attempt. Not in SCOPE.md. Consistent with the architecture but undeclared in scope. |
| Addition | EC-07: overlapping event_type across packs | Test case for two domain packs declaring the same `event_type` string. Not in SCOPE.md. Architecture resolves it (source_domain set server-side by ingress, not event_type lookup), but the edge case surface was not scoped. |
| Addition | EC-08/09: rule descriptor validation | Rejection of `window_secs = 0` and cross-domain rule file mismatch. Not in SCOPE.md. Flows naturally from the DSL design but was not listed as a scoped requirement. |
| Simplification | AC-08 / FR-06 conflict | SCOPE.md Goal #5 (Admin runtime override) was included as a goal but the architecture resolves it as "removed from scope" via ADR-002. The spec retains FR-06 in contradiction of ADR-002. This is the primary variance requiring human resolution (see below). |

---

## Variances Requiring Approval

### 1. FR-06 retained in SPECIFICATION.md despite ADR-002 removing it (VARIANCE — gate-entry blocker)

**What**: SCOPE.md Goal #5 states "Expose runtime re-registration for Admin callers as an override mechanism." The architecture (ADR-002) explicitly removes Admin runtime re-registration from W1-5 scope, citing that config-only is simpler, reproducible, and version-controllable. The SPECIFICATION.md retains FR-06 in full — four sub-requirements (FR-06.1 through FR-06.4), Workflow 3 (Admin runtime pack override), a constraint (C-04), a dependency reference, and AC-08 (the corresponding acceptance criterion). The RISK-TEST-STRATEGY independently flags this as R-04, classified Critical/High probability, and identifies it as "a gate-entry blocker."

The human has confirmed config-only is correct and FR-06 should be treated as a spec defect.

**Why it matters**: FR-06.1 has an unresolved open question (OQ-01) identifying the target MCP tool for the Admin override path. Implementing FR-06 without naming the tool risks schema breakage on whichever tool is chosen. AC-08 cannot pass if FR-06 is not built; if FR-06 is not built, AC-08 must be removed. As currently written the specification is internally inconsistent — it contains both ADR-002's decision (config-only) in the ADR table and FR-06 (runtime override) in the requirements — and an implementor following the spec will build the wrong thing.

**Recommendation**: Remove FR-06 (all four sub-requirements), Workflow 3, AC-08, C-04's reference to the Admin runtime override, and the `context_enroll` model reference in the Dependencies section that exists solely to support FR-06. OQ-01 closes by cancellation. The risk strategy's R-04 test scenario 3 ("if FR-06 is out of scope: assert DomainPackRegistry has no MCP write path") becomes the implementation obligation. No new action is needed from the architect — ADR-002 already captures the decision.

---

## Detailed Findings

### Vision Alignment

The vision document (PRODUCT-VISION.md) names W1-5 explicitly and lists its exact deliverables. It identifies the `HookType` enum as a Medium-severity domain-coupling gap and the observation metrics schema (`bash_for_search`, `coordinator_respawn`) as a Low-severity gap, both under "Domain Coupling (strayed from)." It also lists W1-5's business outcome verbatim: "Any domain — SRE operations, environmental monitoring, scientific research, legal review — can connect its native event stream to the learning layer without code changes."

The three source documents address exactly this: replacing `HookType` with string-typed `event_type` + `source_domain`, making metrics configurable via `domain_metrics_json`, and rewriting the 21 detection rules to be domain-aware. The backward compatibility requirement (zero regression for Claude Code) matches the vision's statement that "the 'claude-code' default domain pack must preserve identical behavior for all existing sessions."

The vision states: "Domain pack registration is config-file-driven, not runtime MCP calls." This sentence matches ADR-002 precisely. The SPECIFICATION.md's retention of FR-06 (runtime MCP registration) is the only point of vision misalignment.

Security requirements in the vision (payload 64 KB, depth ≤ 10, source_domain `[a-z0-9_-]` max 64 chars, Admin-gated runtime registration, sandboxed extraction rules) are all addressed in the specification's NFR section and the architecture's ADR-007.

### Milestone Fit

W1-5 is correctly placed as the fifth and final Wave 1 item. Its dependencies are satisfied: W0-0 (daemon mode), W0-1 (sqlx), W0-3 (config externalization) are all marked COMPLETE. W1-1 through W1-4 are either complete or parallel-track; W1-5 does not depend on W1-4.

The vision states W1-5 unblocks W3-1 (GNN training signal) by "providing a domain-neutral event substrate for implicit training labels." The specification resolves the ambiguity about what "fully functional" means for W3-1's gate in AC-05: W3-1 requires that the pipeline accepts multi-domain events and that detection rules gate correctly on `source_domain`; no production multi-domain rules are pre-required. This narrowing is appropriate — it allows W1-5 to ship without building SRE or scientific domain packs.

The effort estimate is 5–7 days, matching the vision's estimate exactly.

### Architecture Review

The architecture is coherent and well-grounded. Key strengths:

- The four-phase approach (generalize core type → domain pack registry → rewrite detection rules → generalize metrics) follows the wave-based compilation-gated refactoring pattern (Unimatrix entry #377) that the codebase has used successfully.
- The component breakdown correctly identifies all five affected subsystems: `unimatrix-core`, `unimatrix-observe`, `unimatrix-store`, `unimatrix-server` config, and `unimatrix-server` observation service.
- ADR-006 resolves SR-02 cleanly: `UniversalMetrics` typed struct is the single canonical representation. The `domain_metrics_json` extension column is a side-channel, not a second live representation. This eliminates the SR-02 serialization round-trip risk.
- ADR-005 (mandatory `source_domain` guard as the first filter in all domain-specific rules) directly addresses SR-07, the cross-domain false finding risk that the risk strategy elevates to Critical.
- ADR-003 resolves SR-01 by introducing `RuleEvaluator` as a host struct for temporal aggregation, acknowledging that `json_pointer` alone is insufficient for temporal window rules.
- The `DomainPackRegistry` as `Arc<RwLock<_>>` initialized at startup (not persisted) correctly mirrors the `CategoryAllowlist` pattern already in production.

The architecture table lists "Runtime Admin re-registration — Removed from scope (config-only) — ADR-002" which is the correct and final decision. The architecture is internally consistent on this point.

OQ-3 (source_domain not stored in observations table) and OQ-5 (HookType constants module visibility) are left open for the spec writer — both are resolved in the specification. OQ-3 is resolved by the architecture's statement that source_domain is always inferred server-side for W1-5 and the spec's confirmation of passthrough behavior. OQ-5 is resolved by FR-01.2 declaring HookType as `pub const` strings.

### Specification Review

The specification is thorough and well-structured. All SCOPE.md acceptance criteria are represented with verification methods. FR-01 through FR-05 and NFR-01 through NFR-11 are internally consistent and match the architecture.

The one structural defect is FR-06. The spec's own open question (OQ-01) — "Which existing MCP tool is extended for Admin runtime domain pack registration?" — is unanswered, and the architecture's ADR-002 has already answered it by removing the requirement. FR-06 appears as a fully-formed requirement section (four sub-requirements, a workflow, AC-08) but references an unresolved open question for its core design element. This is a direct consequence of the spec being written after the architecture removed the capability, with FR-06 retained in error.

Minor observation: FR-05.5 states "UNIVERSAL_METRICS_FIELDS const shall be updated to include `domain_metrics_json` as the 22nd entry." The architecture's ADR-006 states "UNIVERSAL_METRICS_FIELDS const unchanged (21 entries)." This is a low-severity inconsistency within the specification itself — the structural test section (R-11 in the risk strategy) requires 22 entries. The specification's FR-05.5 wording ("updated to include `domain_metrics_json` as the 22nd entry") is the technically correct statement given that the test is being updated; the architecture's "unchanged" refers to the 21 existing entries not changing, not the const itself. The implementor should follow FR-05.5 and the R-11 test scenarios.

### Risk Strategy Review

The risk strategy is the strongest of the three documents. Fourteen risks are identified, classified by severity and likelihood, and individually mapped to concrete test scenarios. All eight scope risks from SCOPE-RISK-ASSESSMENT.md are traced in the final traceability table.

R-04 (FR-06/ADR-002 conflict) is correctly identified as Critical, flagged as a gate-entry blocker, and given three test scenario branches depending on resolution. The risk strategy's treatment of this risk is accurate and actionable.

Risk coverage against the vision's security requirements is complete: SEC-01 (payload byte-count for UTF-8), SEC-02 (recursive depth check stack overflow), SEC-03 (field_path injection via JSON Pointer escapes), and SEC-05 (future ingress paths not enforcing server-side source_domain) are all addressed with test scenarios and rationale.

The integration risks (IR-01 through IR-04) catch system-level assembly issues that the individual unit tests would miss — notably IR-01 (DomainPackRegistry not injected into SqlObservationSource means all events get `source_domain = "unknown"`) and IR-03 (`compute_universal()` called on non-claude-code records without a source_domain guard). These represent genuine implementation pitfalls.

One gap in the risk strategy: EC-04 (rejecting `source_domain = "unknown"` as a registration name) is tested as an edge case but no risk register entry elevates it. The reserved string semantics are a correctness constraint with real consequences if violated (registered "unknown" events would be indistinguishable from passthrough events). This is low severity given ADR-007's startup failure policy, but the reservation should be explicitly enforced in the startup validation code.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns, scope additions, milestone discipline — found entry #2298 (config key semantic divergence pattern) and entry #2063 (file topology vs vision language / milestone discipline pattern). Neither applies directly to col-023; col-023's scope alignment is clean except for the FR-06 spec defect.
- Stored: nothing novel to store — the FR-06 retention pattern (architecture ADR removes a requirement, spec retains it verbatim) is feature-specific to the design process for col-023 and is already captured in R-04 of the risk strategy. If the same pattern (spec writer retaining a scope item that an ADR has removed) recurs across two or more features, a pattern entry under topic `vision` would be warranted.
