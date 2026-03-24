# Alignment Report: col-025

> Reviewed: 2026-03-24 (revision 3 — final clean report post human resolution)
> Artifacts reviewed:
>   - product/features/col-025/architecture/ARCHITECTURE.md
>   - product/features/col-025/architecture/ADR-001 through ADR-006
>   - product/features/col-025/specification/SPECIFICATION.md
>   - product/features/col-025/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/col-025/SCOPE.md
> Scope risk: product/features/col-025/SCOPE-RISK-ASSESSMENT.md
> Agent ID: col-025-vision-guardian-final

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances Wave 1A session-conditioned intelligence; no overreach |
| Milestone Fit | PASS | Correct Wave 1A slot; all future-milestone capabilities explicitly deferred |
| Scope Gaps | PASS | No items in SCOPE.md are unaddressed; delivery-time checks identified and documented |
| Scope Additions | PASS | ADR-003 routing reversal and ADR-006 `CONTEXT_GET_INSTRUCTION` accepted by human |
| Architecture Consistency | WARN | ARCHITECTURE.md §New Interfaces and §Integration Surface carry stale 4096 references; ADR-005 and SPECIFICATION.md are authoritative at 1024 |
| Risk Completeness | PASS | All scope risks (SR-01 through SR-06) traced; 14-risk register complete; 9 non-negotiable tests identified |

**Overall: PASS with one residual WARN. No VARIANCE or FAIL. The two WARNs from prior revisions are resolved as follows: WARN-1 (byte-limit split) is resolved — ADR-005 and SPECIFICATION.md are consistent at `MAX_GOAL_BYTES = 1024`. WARN-2 (`CONTEXT_GET_INSTRUCTION` scope addition) is accepted by the human. One residual WARN remains: stale 4096 references in ARCHITECTURE.md and RISK-TEST-STRATEGY.md that were not updated when the constant was settled at 1024. Delivery should overwrite these at the point of implementation. Feature is ready to proceed to delivery.**

---

## Resolution Status of Prior WARNs

### WARN-1 (byte-limit split) — RESOLVED

The prior two-WARN state arose from a multi-revision history:

- Revision 1: two separate constants (`MCP_MAX_GOAL_BYTES = 2048`, `UDS_MAX_GOAL_BYTES = 4096`) with naming ambiguity.
- Revision 2: ADR-005 settled one constant at 4096, but SPECIFICATION.md still said 2048 — a numeric conflict.
- Final state: ADR-005 (the authoritative architecture decision) is settled at `MAX_GOAL_BYTES = 1024`. SPECIFICATION.md FR-03, FR-03 enforcement text, AC-13a, AC-13b, Constants table, Ubiquitous Language table, and Constraints section all consistently state 1024. The ADR and the spec agree.

Human confirmation: accepted as resolved.

### WARN-2 (`CONTEXT_GET_INSTRUCTION` scope addition) — ACCEPTED

ADR-006 adds a named constant `CONTEXT_GET_INSTRUCTION` prepended as a header to every `format_index_table` output. This was not in SCOPE.md's five Goals. It is a bounded, well-motivated addition (agents must know to call `context_get` after receiving a briefing table; the header makes the injected index actionable). The human has explicitly accepted this scope addition.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition (accepted) | ADR-006: `CONTEXT_GET_INSTRUCTION` header on all `format_index_table` output | SCOPE.md contains no mention of this constant. Human has explicitly accepted this addition. AC-18 and R-11 cover delivery requirements. |
| Addition (documented) | ADR-003: SubagentStart routes to `IndexBriefingService` when goal present; goal wins over prompt_snippet | SCOPE.md §Goals-5 described `prompt_snippet (non-empty) → current_goal → topic`. ADR-003 reverses to `goal → prompt_snippet → topic` and routes to `IndexBriefingService` instead of `ContextSearch`. Intentional, reasoned reversal: prompt_snippet is spawn boilerplate, not semantic intent. ARCHITECTURE.md, SPECIFICATION.md, and RISK-TEST-STRATEGY.md all reflect this design. |
| Gap | SCOPE.md §Constraints: "no other schema changes in-flight — must be verified at implementation start" | No source document records a verification result. Delivery-time check; not a design gap. Noted for the delivery engineer. |
| Simplification | SCOPE.md OQ-01 — `sessions.keywords` cleanup batching | Explicitly excluded by ARCHITECTURE.md §Columns explicitly out of scope. Defer confirmed. |

---

## Variances Requiring Approval

None. No VARIANCE or FAIL items are present.

The one remaining WARN is a stale-reference issue in secondary document sections, not a design conflict. It does not require human approval but is noted below for delivery awareness.

---

## Detailed Findings

### Vision Alignment

**Evidence**: The product vision (Wave 1A) identifies two High-severity gaps:
- "No session-conditioned relevance — every query treated identically" (High, Roadmapped Wave 1A)
- "No proactive delivery — all surfaces are reactive" (High, Roadmapped Wave 1A / WA-4)

WA-4b (COMPLETE via crt-027) established `derive_briefing_query` and its three-step priority function, explicitly leaving step 2 (`synthesize_from_session`) as a future addition point — "currently returns None / weak signals." col-025 fills exactly that step-2 slot with a direct statement of feature intent.

The goal embedding deferral (SCOPE.md Non-Goals) correctly avoids W3-1 and WA-2 territory. No scoring weights, fused inputs, or GNN signals are modified. The `CONTEXT_GET_INSTRUCTION` addition (ADR-006) is consistent with "deliver the right knowledge at the right time" — the header makes injected index output immediately actionable for agents.

**Finding**: PASS. Feature is precisely targeted at the Wave 1A step-2 gap with no overreach into future milestones.

---

### Milestone Fit

**Evidence**: col-025 targets the Wave 1A / WA-4b layer. WA-4 (COMPLETE) established the proactive delivery infrastructure; col-025 improves the query signal feeding that infrastructure without touching any Wave 2 capabilities (OAuth, container, HTTP) or W3-1 capabilities (GNN, learned weight update). The schema migration (v15 → v16) follows the established incremental ALTER TABLE pattern used throughout the Nexus and Cortical phases. The six ADRs cover only decisions within this scope tier.

**Finding**: PASS. Correct milestone, correct scope tier.

---

### Architecture Review

**Evidence**:

1. **Component coverage**: All five SCOPE.md Goals map to named architecture components (Goal 1 → Component 4; Goal 2 → Components 1+3; Goal 3 → Components 2+5; Goal 4 → Component 6; Goal 5 → Component 7). ADR-006 is embedded in Component 6 / `format_index_table`.

2. **ADR completeness**: Six ADRs present — durability tier (ADR-001), synthesize_from_session semantics (ADR-002), SubagentStart branch routing (ADR-003), resume DB failure degradation (ADR-004), byte-length guard (ADR-005), CONTEXT_GET_INSTRUCTION constant (ADR-006). All key decisions are documented with alternatives considered and rejected.

3. **ADR-003 and SPECIFICATION.md are consistent**: SPECIFICATION.md FR-09 and AC-08/AC-12 reflect the revised precedence (`goal → prompt_snippet`, routing to `IndexBriefingService`). No divergence between ADR-003 and the spec.

4. **ADR-005 settled at 1024**: ADR-005 §Decision states `pub const MAX_GOAL_BYTES: usize = 1024`. SPECIFICATION.md agrees at 1024 throughout. This is the authoritative value.

5. **Stale 4096 references (residual WARN)**: ARCHITECTURE.md §New Interfaces (line 187) states `pub const usize = 4096`. ARCHITECTURE.md §Integration Surface (line 210) also states `pub const usize = 4096`. These are stale references from the period before ADR-005 was finalized at 1024. They do not represent a design conflict — the ADR is the decision authority — but delivery must update these two lines when implementing the constant. Similarly, RISK-TEST-STRATEGY.md §Security section and §Scope Risk Traceability reference `MAX_GOAL_BYTES = 4096`; these are also stale and should be updated during delivery.

6. **Open delivery-time questions**: ARCHITECTURE.md OQ-03 (SubagentStart `session_id` availability timing) remains open and is flagged for delivery resolution before implementing Component 7. R-12 in RISK-TEST-STRATEGY covers the wiring risk with an integration test requirement.

**Finding**: PASS with stale-reference WARN. Architecture decisions are complete and internally consistent. ADR-005 at 1024 is authoritative; two ARCHITECTURE.md table entries and two RISK-TEST-STRATEGY.md references carry the old 4096 value and must be corrected at delivery time.

---

### Specification Review

**Evidence**:

1. **FR coverage (FR-01 through FR-12)**: All SCOPE.md Goals map to FRs. FR-11 (empty/whitespace normalization) and FR-12 (CONTEXT_GET_INSTRUCTION header) are additions beyond SCOPE.md's Goals; both are grounded in ADR decisions and risk remediation. FR-11 resolves the empty-string goal tension from prior revision. FR-12 covers ADR-006.

2. **AC coverage (AC-01 through AC-18)**: The full set of acceptance criteria covers the five primary SCOPE.md goals and all identified risks. AC-12 (SubagentStart inversion guard), AC-13a/b (byte-length enforcement on both paths), AC-14/AC-15 (resume DB null/error), AC-16 (schema version cascade), AC-17 (empty/whitespace normalization), and AC-18 (CONTEXT_GET_INSTRUCTION once-only) are all grounded in identified risks or ADR requirements.

3. **MAX_GOAL_BYTES consistency in SPECIFICATION.md**: FR-03, AC-13a, AC-13b, the Constants table, the Ubiquitous Language table, and the Constraints section all state 1024. No internal conflict within the specification.

4. **Empty-string normalization is fully resolved**: FR-11 explicitly settles that empty or whitespace-only goal must be normalized to `None` at the MCP handler. AC-17 covers both `""` and `"   "` cases. NFR-05 (verbatim storage) is consistent with FR-11's carve-out. No tension remains.

5. **Open questions for delivery**: OQ-01 (`sessions.keywords` cleanup batching), OQ-02 (log level convention for `tracing::warn!`), and OQ-03 (CONTEXT_GET_INSTRUCTION exact wording) are all delivery-time concerns, not specification gaps.

**Finding**: PASS. Specification is internally consistent at 1024 and fully aligned with the settled ADR decisions.

---

### Risk Strategy Review

**Evidence**:

1. **Risk register (14 risks)**: All six SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-06) map to named register entries with architecture resolutions. R-13 (UDS truncate-then-overwrite retry) and R-14 (old binary connecting to v16 schema) were added in prior revisions when ADR-005 was settled.

2. **SR traceability complete**: SR-01 → R-02 (migration cascade); SR-02 → R-07+R-13 (byte guard + retry); SR-03 → R-04+R-12 (SubagentStart precedence + wiring); SR-04 → accepted (columns-to-avoid documented); SR-05 → R-03 (resume DB failure); SR-06 → resolved by ADR-002 architecture (single shared function).

3. **Nine non-negotiable test scenarios**: The Coverage Summary identifies nine non-negotiable gate-3c tests, each tied to a specific risk ID and acceptance criterion. The set is complete relative to the settled design.

4. **Stale 4096 in security section**: RISK-TEST-STRATEGY.md §Security section states `MAX_GOAL_BYTES = 4096` twice. This is a stale reference; the correct value is 1024. The R-07 test scenarios use the abstract `MAX_GOAL_BYTES` name (not a literal), so the test logic is correct regardless of the stale prose reference. Delivery should update the two prose references when implementing.

5. **R-12 integration test requirement is appropriate**: The SubagentStart goal-present branch calls `IndexBriefingService::index` — a new call site. The risk strategy correctly requires an integration test (not just a unit test) to verify the wiring from `dispatch_request` is functional. This is the architecturally most novel path in the feature.

**Finding**: PASS with noted stale references. Risk strategy is thorough and complete. Stale 4096 references in prose sections do not affect test logic.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found #2298 (config key semantic divergence), #425 (model registry), #181 (embedding pipeline pattern). None directly applicable to col-025 vision alignment. Pattern #2298 (same key, different values across documents) is structurally similar to the WARN-1 history here but that pattern pertains to TOML config key semantics.
- Stored: nothing novel to store. The residual stale-reference WARN (correct constant in ADR + spec, stale value in secondary doc table entries) is a document-synchronization artifact of iterative ADR revision. The pattern does not generalize beyond this feature's revision history; the root cause (ADR settled after spec and architecture tables were drafted) is a known artifact of the col-025 design cycle, not a cross-feature misalignment pattern.
