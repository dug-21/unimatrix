# Alignment Report: col-025

> Reviewed: 2026-03-24
> Artifacts reviewed:
>   - product/features/col-025/architecture/ARCHITECTURE.md
>   - product/features/col-025/specification/SPECIFICATION.md
>   - product/features/col-025/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/col-025/SCOPE.md
> Scope risk: product/features/col-025/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances the session-conditioned intelligence pipeline described in Wave 1A |
| Milestone Fit | PASS | Wave 1A / WA-4b territory; SCOPE.md is correctly scoped to improve the briefing query path |
| Scope Gaps | WARN | One SCOPE.md open question (OQ-01 on `sessions.keywords`) not closed by architecture; SR-05 error-handling contract partially resolved |
| Scope Additions | WARN | RISK-TEST-STRATEGY introduces a 4 096-byte UDS truncation limit not present in SCOPE.md; spec sets 2 048-byte MCP limit that differs |
| Architecture Consistency | PASS | All five SCOPE.md changes map to named components; ADRs present for all key decisions |
| Risk Completeness | PASS | All SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-06) are addressed and traced |

**Overall: PASS with two WARNs. No VARIANCE or FAIL items. Feature is ready to proceed to delivery.**

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | SCOPE.md OQ-01 — `sessions.keywords` cleanup batching decision | SCOPE.md §Non-Goals and §Constraints state the column is excluded and tracked separately. ARCHITECTURE.md §Columns explicitly out of scope names it. However, SPECIFICATION.md OQ-01 asks the architect to document the decision explicitly in ARCHITECTURE.md; the architecture does list it but does not record whether batching was considered and rejected. Delivery should confirm this is a conscious defer, not an oversight. |
| Gap | SCOPE.md §Constraints: "no other schema changes in-flight — must be verified at implementation start" | No source document records a verification result. This is a runtime-start check; the architecture correctly flags it as an open assumption. Delivery must perform this check before writing the migration. |
| Addition | RISK-TEST-STRATEGY §R-07, §Failure Modes: 4 096-byte UDS truncation path | SCOPE.md §Constraints says "no truncation at the storage layer" and suggests "a max-byte check if desired". SCOPE-RISK-ASSESSMENT SR-02 recommends a guard at the tool handler layer. ADR-005 in ARCHITECTURE.md establishes two distinct limits: 2 048 bytes for the MCP path (rejection) and 4 096 bytes for the UDS path (truncation with warn log). SCOPE.md does not define the UDS truncation limit or the truncation-vs-rejection split. This is a scope addition that materially affects behavior. |
| Simplification | SCOPE.md §Background: SubagentStart uses `prompt_snippet → current_goal → topic` | SCOPE.md describes the precedence but notes the ARCHITECTURE.md open question OQ-03 about whether the SubagentStart arm receives session state reliably. ARCHITECTURE.md open question OQ-03 acknowledges this and defers confirmation to delivery. The spec includes AC-08, AC-12 covering both branches. Rationale is sound; delivery must confirm session_id availability in the SubagentStart arm before coding. |

---

## Variances Requiring Approval

No VARIANCE or FAIL items. The two WARNs below are informational and require awareness but not blocking approval.

### WARN-1: Dual Byte-Limit Split (MCP 2 048 / UDS 4 096) Not in SCOPE.md

**What**: SCOPE.md §Constraints states "no truncation at the storage layer" and suggests a max-byte check at the tool layer. ADR-005 (ARCHITECTURE.md) establishes two distinct limits: MCP path rejects at 2 048 bytes; UDS path truncates with a warn log at 4 096 bytes. The 4 096 UDS limit and the truncation behavior are additions to what SCOPE.md asked for.

**Why it matters**: SCOPE.md's author envisioned a single tool-layer guard. The architecture has split this into two different behaviors on two different paths with different limits. The UDS truncation path means a goal that would be rejected on the MCP path could arrive at the UDS handler via an internally-generated `ImplantEvent` and be silently truncated. The R-07 test scenario in RISK-TEST-STRATEGY explicitly calls out the panic risk at the UDS truncation boundary (char-boundary-safe truncation required).

**Recommendation**: Accept the split design — it is technically justified (the UDS path is fire-and-forget and cannot return an error) — but the human should confirm that the 4 096-byte UDS limit and char-boundary-safe truncation are intentional design decisions, not gaps. The R-07 test requirement (UTF-8 boundary test) is non-negotiable per RISK-TEST-STRATEGY.

---

### WARN-2: OQ-03 (`MAX_GOAL_BYTES` constant) Unresolved in Architecture

**What**: SPECIFICATION.md OQ-03 asks the architect to confirm the 2 048-byte limit or substitute a project-standard value and decide whether to introduce a named constant (`MAX_GOAL_BYTES`). ARCHITECTURE.md ADR-005 establishes the 4 096-byte UDS limit via the name `MAX_GOAL_BYTES` but does not address whether this constant is shared with the MCP 2 048-byte check or whether two separate constants exist.

**Why it matters**: If the delivery engineer implements a single `MAX_GOAL_BYTES = 4096` constant and applies it to both paths, the MCP path will accept 4 096 bytes instead of 2 048, violating AC-13 (which tests 2 049-byte rejection). SPECIFICATION.md §FR-03 sets the MCP limit at 2 048. These are two different numbers that the architecture names with one constant name.

**Recommendation**: Delivery should define two constants (`MCP_MAX_GOAL_BYTES = 2048` and `UDS_MAX_GOAL_BYTES = 4096`) or clarify that the single `MAX_GOAL_BYTES` applies only to the UDS path. The spec's AC-13 test (2 049-byte rejection on MCP) is the governing acceptance criterion.

---

## Detailed Findings

### Vision Alignment

**Evidence**: The product vision (Wave 1A) states: "No session-conditioned relevance — every query treated identically [High severity gap]" and "No proactive delivery — all surfaces are reactive [High severity gap, Roadmapped — Wave 1A]." WA-4b describes `context_briefing` as "a targeted surface triggered by the SM at phase transitions" and notes: "Because no agent-provided task is available on this path, `derive_briefing_query` step 2 MUST supply `current_goal`."

col-025 directly implements what the vision identified as the `synthesize_from_session` hook: "step 2: synthesize_from_session(state) ← currently returns None / weak signals." SCOPE.md §Background §derive_briefing_query confirms this design intent.

The feature does not add scoring pipeline changes, embedding changes, or new retrieval modes — all deferred per vision's milestone discipline. The goal embedding deferral (SCOPE.md Non-Goals item 1) correctly avoids WA-2/W3-1 territory.

**Finding**: PASS. The feature is laser-focused on filling the `synthesize_from_session` gap that Wave 1A requires, without overreaching into W3-1 GNN territory.

---

### Milestone Fit

**Evidence**: The product vision places col-025's problem statement squarely in Wave 1A. WA-4 (Proactive Delivery, COMPLETE via crt-027) established the `derive_briefing_query` three-step function. col-025 fills step 2, which WA-4 explicitly left as a future addition point.

No Wave 2 capabilities (container, OAuth, HTTP transport) are touched. No W3-1 capabilities (GNN, learned weights) are pre-built. The schema migration (v15 → v16) follows the incremental migration pattern used throughout the Nexus and Cortical phases.

**Finding**: PASS. Feature targets the correct Wave 1A slot. All future-milestone capabilities are explicitly listed as Non-Goals with rationale.

---

### Architecture Review

**Evidence**:

1. **Component coverage**: All five SCOPE.md Goals map to named architecture components:
   - Goal 1 (wire protocol) → Component 4 (MCP CycleParams)
   - Goal 2 (persistence) → Component 1 (Schema Migration) + Component 3 (Cycle Event Handler)
   - Goal 3 (SessionState cache) → Component 2 (SessionState Extension) + Component 5 (Session Resume)
   - Goal 4 (derive_briefing_query) → Component 6 (Briefing Query Derivation)
   - Goal 5 (UDS injection) → Component 7 (SubagentStart Injection Precedence)

2. **ADR discipline**: Five ADRs present — durability tier (ADR-001), synthesize_from_session semantics (ADR-002), SubagentStart branch (ADR-003), resume DB failure degradation (ADR-004), byte-length guard (ADR-005). All key decisions documented.

3. **SR-01 migration cascade addressed**: ARCHITECTURE.md §Migration Test Cascade explicitly names the three test files requiring update — `migration_v14_to_v15.rs`, `sqlite_parity.rs`, `sqlite_parity_specialized.rs`. Delivery checklist requirement is present.

4. **Integration surface documented**: New and modified interfaces are enumerated with signatures. The `Store::get_cycle_start_goal` read helper is new; its signature `async fn(&self, &str) -> Result<Option<String>>` is appropriate for the resume path.

5. **Pattern #3337 risk (architecture diagram header divergence)**: ARCHITECTURE.md uses component names in prose and the interaction diagram. These are consistent with the specification's domain model and FR numbering. No header divergence detected.

**Potential concern**: ARCHITECTURE.md §Open Questions OQ-03 asks delivery to "confirm the session_id is reliably available in the SubagentStart arm of `dispatch_request`." This is an unresolved question at architecture sign-off. The RISK-TEST-STRATEGY does not add a risk for this gap; it is buried in an open question. If session_id is not available on that path, the SubagentStart goal branch cannot be implemented as designed.

**Finding**: PASS with note. Architecture is complete and well-structured. Delivery must resolve OQ-03 before implementing Component 7.

---

### Specification Review

**Evidence**:

1. **FR coverage**: All ten functional requirements (FR-01 through FR-10) map directly to SCOPE.md Goals and Constraints. FR-03 (2 048-byte MCP guard) is an addition to SCOPE.md but is consistent with SCOPE-RISK-ASSESSMENT SR-02's recommendation and SCOPE.md's own "can be enforced at the tool layer" note.

2. **AC coverage**: 16 acceptance criteria (AC-01 through AC-16). SCOPE.md defined 11 (AC-01 through AC-11). The spec adds AC-12 (SubagentStart inversion guard), AC-13 (byte-length rejection), AC-14 (resume no-row), AC-15 (resume DB error + warn log), AC-16 (schema version test cascade). These additions directly address SR-03, SR-02, and SR-05 from SCOPE-RISK-ASSESSMENT — appropriate and expected.

3. **NFR coverage**: Six NFRs present. NFR-01 (zero-cost hot path) and NFR-04 (pure synthesize_from_session) are explicit constraints from SCOPE.md §Constraints. NFR-02 and NFR-03 cover backward compatibility per AC-10. NFR-05 (verbatim storage) and NFR-06 (schema version assertion coverage) complete the picture.

4. **Open questions OQ-01, OQ-02, OQ-03**: All three are deferred to the architect or delivery. This is appropriate given the spec's role. OQ-01 (keywords cleanup batching) is a Non-Goal defer. OQ-02 (log level convention) is a delivery-time codebase check. OQ-03 (byte-limit constant) directly contributes to WARN-2 above.

5. **Empty-string goal behavior gap**: RISK-TEST-STRATEGY §Edge Cases lists "goal = '' (empty string, not null)" and states "assert treated as `None`." SPECIFICATION.md NFR-05 states "goal text is stored without transformation." These are in tension: if `goal = ""` is stored verbatim as an empty string, the downstream `synthesize_from_session` must distinguish `Some("")` from `Some("real goal")`. The spec does not include an FR covering this edge case. The RISK-TEST-STRATEGY correctly identifies it but attributes it to R-11, not as a spec gap. Delivery will need to make a decision here.

**Finding**: PASS with note on empty-string goal behavior. The tension between NFR-05 (verbatim storage) and the R-11 test expectation (empty = None) should be resolved explicitly before delivery begins. No formal VARIANCE because RISK-TEST-STRATEGY acknowledges it and marks it "spec must clarify."

---

### Risk Strategy Review

**Evidence**:

1. **SR traceability**: RISK-TEST-STRATEGY §Scope Risk Traceability maps all six SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-06) to architecture risks and resolutions. Every SR has a named resolution.

2. **Risk register completeness**: 12 risks (R-01 through R-12). Coverage pyramid: 0 Critical, 5 High priority, 5 Medium priority, 2 Low priority. The 5 High-priority risks all have ≥ 3 test scenarios as required.

3. **Non-negotiable tests identified**: Five named tests required by lesson #2758 (gate-3c check):
   - `migration_v15_to_v16.rs` with idempotency scenario
   - SubagentStart inversion guard (AC-12)
   - UTF-8 char-boundary truncation at 4 096-byte boundary
   - Full column-value assertion on `insert_cycle_event` round-trip
   - DB error on resume → None + warn log + registration succeeds

4. **Pattern #3337 application**: The prior pattern warns that architecture diagram informal headers diverge from spec, causing testers to assert against wrong strings. The RISK-TEST-STRATEGY avoids this by using AC-IDs (AC-01 through AC-16) rather than informal strings as assertion anchors. PASS on this pattern.

5. **R-07 UDS truncation elevated to High**: The risk strategy elevates R-07 (High × Low) to the High tier because panic in the UDS listener terminates the server process. This is correct escalation; the UDS listener is a critical path.

6. **R-09 empty-string goal**: R-11 §Edge Cases notes "goal = ''" with "spec must clarify." R-11 test scenario 3 states "assert behavior equivalent to `goal = None`." This is a reasonable default assumption but the spec does not confirm it. See note under Specification Review above.

**Finding**: PASS. Risk strategy is thorough and correctly traces all scope risks. The empty-string goal edge case is flagged as requiring spec clarification — not a strategy gap, but a delivery input gap.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found #2298 (Config key semantic divergence: same TOML key, different weights payload than vision), #2964 (Signal fusion: sequential sort passes cause NLI override), #3337 (Architecture diagram informal headers diverge from spec — testers assert against wrong strings). Pattern #3337 was directly applicable and checked against all three source documents. Patterns #2298 and #2964 were not directly relevant to col-025.
- Stored: nothing novel to store — the two WARNs (dual byte-limit split and MAX_GOAL_BYTES naming ambiguity) are feature-specific implementation clarification issues, not recurring cross-feature misalignment patterns. The empty-string goal behavior tension (NFR-05 vs R-11) is too feature-specific to generalize.
