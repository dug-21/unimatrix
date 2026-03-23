# Alignment Report: crt-027

> Reviewed: 2026-03-23
> Artifacts reviewed:
>   - product/features/crt-027/architecture/ARCHITECTURE.md
>   - product/features/crt-027/specification/SPECIFICATION.md
>   - product/features/crt-027/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope sources: product/features/crt-027/SCOPE.md
>                product/features/crt-027/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | WARN | WA-4a scoped narrower than vision WA-4a; explicitly deferred — acceptable with caveats |
| Milestone Fit | PASS | Correctly targets Wave 1A; WA-5 dependency surface correct |
| Scope Gaps | WARN | AC-SR01 (SubagentStart stdout verification) remains OPEN in spec; no spike assigned |
| Scope Additions | WARN | `MIN_QUERY_WORDS` guard on UserPromptSubmit not in SCOPE.md goals or non-goals; added in Proposed Approach |
| Architecture Consistency | PASS | All SCOPE.md design decisions faithfully implemented; SR risks addressed explicitly |
| Risk Completeness | PASS | RISK-TEST-STRATEGY is thorough; traceability table covers all SCOPE-RISK-ASSESSMENT items |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | AC-SR01 open with no spike assigned | SCOPE.md SR-01 recommendation: "add a spike task or fallback design before architecture begins." SPECIFICATION.md marks AC-SR01 as OPEN and says "architect must add a spike task or pivot." Neither document assigns a spike issue or confirms the pivot. This leaves a delivery blocker unresolved at the start of Wave 3b. |
| Addition | `MIN_QUERY_WORDS = 5` guard on `UserPromptSubmit` | Present in SCOPE.md §Proposed Approach and carried into architecture/spec/risk. Not listed in SCOPE.md Goals or Non-Goals. It is a net-new behaviour change on the `UserPromptSubmit` path, not a consequence of routing SubagentStart. |
| Simplification | WA-4a delivery scope narrower than product vision WA-4a | Vision WA-4a is "phase-transition candidate cache, drawn from on PreToolUse." crt-027 WA-4a is "route SubagentStart to ContextSearch only." SCOPE.md correctly declares this deferral in §Non-Goals. This is an explicit, documented simplification with rationale. |
| Simplification | `context_briefing` does not use `current_phase` for category affinity | Vision WA-4b: "phase-condition the ranking using `current_phase` and category affinity." crt-027 uses the existing fused score only; phase-conditioned ranking deferred to W3-1. SCOPE.md §Non-Goals explicitly defers this. Documented and acceptable. |

---

## Variances Requiring Approval

### WARN-1: `MIN_QUERY_WORDS` guard — unlisted scope addition

1. **What**: SCOPE.md §Proposed Approach adds a `MIN_QUERY_WORDS: usize = 5` guard to the `UserPromptSubmit` arm of `build_request`. This changes existing runtime behaviour: prompts with fewer than 5 words that previously triggered a `ContextSearch` injection will now fall through to `generic_record_event` (no injection). This is not listed in SCOPE.md Goals (which covers only SubagentStart and `context_briefing`) and is not listed in Non-Goals.

2. **Why it matters**: The product vision WA-4 objective is proactive delivery — "agents receive relevant knowledge before they search for it." Suppressing short-prompt injection narrows delivery rather than expanding it. The change is defensible (short prompts like "yes" or "ok" likely produce low-signal injections), but because it modifies an existing delivery surface it requires explicit human sign-off, not silent inclusion.

3. **Recommendation**: Accept as a bundled improvement — it is well-motivated, reduces injection noise, and the constant is named for future config exposure. Document it in SCOPE.md §Proposed Approach as a Goal or a named addition so it is not invisible to the feature boundary.

---

### WARN-2: AC-SR01 (SubagentStart stdout injection) — open blocker with no spike assigned

1. **What**: SCOPE-RISK-ASSESSMENT.md SR-01 is rated High severity / Med likelihood and is described as CRITICAL. The recommendation is explicit: "Before architecture begins, verify Claude Code SubagentStart hook stdout behavior via a 30-minute spike (or cite existing documentation)." ARCHITECTURE.md acknowledges the risk remains unconfirmed and documents graceful degradation. SPECIFICATION.md marks AC-SR01 as OPEN and says the architect must add a spike task or pivot. No spike has been filed, no documentation reference cited, and no pivot decision recorded.

2. **Why it matters**: If Claude Code ignores SubagentStart hook stdout, WA-4a's injection value proposition is zero. The server correctly records `ObservationRow` entries (which is still better than the current state), but no knowledge is injected into subagents. This is the primary user-facing outcome of WA-4a. Shipping without confirming or denying this behaviour means the feature may ship delivering no injection — and the team will not know until a manual smoke test is run post-merge.

3. **Recommendation**: Before Gate 3b (delivery begins), either (a) file a spike and complete it, or (b) cite specific Claude Code documentation confirming SubagentStart hook stdout is injected, or (c) explicitly pivot WA-4a to session-state-only recording and remove AC-SR01 from the AC list. Do not enter Wave 3b with this blocker unresolved. The architecture's graceful degradation is sound — this is about communicating to stakeholders what they will (and will not) receive.

---

## Detailed Findings

### Vision Alignment

**WA-4a (SubagentStart routing):** The product vision WA-4a describes a phase-transition candidate cache drawn on PreToolUse events. crt-027 delivers something narrower: SubagentStart routed to the existing ContextSearch pipeline. This is explicitly deferred in SCOPE.md §Non-Goals: "WA-4a is NOT a phase-transition candidate cache." The architecture acknowledges the deferral. This is a legitimate Wave 1A partial delivery — the vision describes the end state, and this feature is one step toward it.

**WA-4b (context_briefing as index):** Vision WA-4b: "Filter by topic (current feature cycle). Phase-condition the ranking using `current_phase` and category affinity. Return structured top-k results." crt-027 delivers: active-only, high-k=20 flat index, query derivation from session signals, no phase-conditioned ranking. The phase-conditioned ranking deferral to W3-1 is explicitly called out in SCOPE.md §Non-Goals. The flat index format is a meaningful improvement over the current `BriefingService` and the WA-5 dependency surface is correctly established.

**Proactive delivery direction:** The feature moves in the right direction — from reactive (fire-and-forget RecordEvent on SubagentStart) to proactive (ContextSearch injection). The product vision's high-level goal of "agents receive relevant knowledge before they search for it" is advanced by both WA-4a and WA-4b even at this partial scope.

**Session-conditioned intelligence:** Vision: "given what the agent knows, what they have been doing, and where they are in their workflow, surface the right knowledge — before they ask for it." crt-027 uses `session_id` for WA-2 histogram boost in briefing (threading session context into the ranking pipeline) and routes SubagentStart through the same ContextSearch path that UserPromptSubmit uses. This is directionally correct.

### Milestone Fit

crt-027 is correctly positioned in Wave 1A — after WA-2 (histogram boost, COMPLETE per `c0a79a2`) and before WA-5. The WA-5 dependency surface (IndexEntry typed struct, format_index_table, flat CompactPayload path) is correctly defined as a forward-compatible interface. The feature does not touch Wave 2 (deployment) or W3-1 (GNN) scope. The SCOPE.md non-goals explicitly fence off W3-1 phase-conditioned ranking. Milestone discipline is maintained throughout.

### Architecture Review

**SR-01 (SubagentStart stdout) — UNCONFIRMED BUT DEGRADED GRACEFULLY.** The architecture documents a clean fallback: if Claude Code ignores stdout, the server still records an ObservationRow with `hook: "SubagentStart"` and the topic_signal still feeds the histogram. No error, no non-zero exit code. This is strictly better than the current state (fire-and-forget RecordEvent). The architecture handles the uncertainty correctly but the pre-delivery spike requirement from SCOPE-RISK-ASSESSMENT.md is not resolved (see WARN-2).

**SR-03 (EffectivenessStateHandle wiring) — RESOLVED.** `IndexBriefingService::new()` takes `effectiveness_state` as a required, non-optional parameter. Missing wiring is a compile error. The ADR-004 crt-018b pattern is correctly applied.

**SR-04 (format_compaction_payload test loss) — RESOLVED.** ARCHITECTURE.md §SR-04 enumerates 11 test replacements with named invariants, outcome of each (survives / removed), and new form. This is a complete resolution matching the SCOPE-RISK-ASSESSMENT recommendation.

**SR-05/ADR-003 (UNIMATRIX_BRIEFING_K fate) — RESOLVED.** Env var deprecated and explicitly not read by IndexBriefingService. k=20 hardcoded in constructor. Parse_semantic_k() deleted. Decision documented.

**SR-06/ADR-005 (WA-5 format contract) — RESOLVED.** IndexEntry is a typed struct; format_index_table is a named function; SNIPPET_CHARS constant proposed. The compile-time surface is stable.

**One minor architecture gap — `derive_briefing_query` location TBD.** The integration surface table in ARCHITECTURE.md notes `derive_briefing_query` location as "TBD: services/briefing.rs or new services/query_derive.rs." This is an implementation detail left open, not a design risk. The shared function requirement is clear from FR-11, R-06, and the integration surface specification.

### Specification Review

The SPECIFICATION.md is thorough and well-structured. All 15 functional requirements map cleanly to SCOPE.md acceptance criteria. NFR-05 explicitly addresses the mcp-briefing feature flag compile boundary. NFR-06 preserves test count (non-decreasing).

**AC-SR01 OPEN status.** The spec correctly marks AC-SR01 as an open blocking question. The delivery team must resolve it before Gate 3b. The spec text ("the architect MUST resolve this before delivery begins") is clear.

**AC-22 and AC-23 (MIN_QUERY_WORDS boundary tests).** These acceptance criteria are thorough and precise. The boundary cases (exactly 4 words vs. exactly 5 words) are specified, and the SubagentStart exclusion from the guard is explicitly tested. This level of specification for the unlisted addition is actually stronger than many scoped items.

**OQ-SR08 (cold-state topic fallback).** The spec leaves this as a "Low risk" open question for the architect to validate against the live knowledge base. Given that step 3 correctly falls back to `Ok(vec![])` without error, the risk is appropriately classified as low. No VARIANCE.

**Backward compatibility.** The spec correctly preserves `role` and `task` in `BriefingParams` (C-02), retains `HookRequest::Briefing` wire variant (C-04), and documents `UNIMATRIX_BRIEFING_K` deprecation (C-08). These are all correct.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is the strongest artifact in this set. It demonstrates direct traceability from SCOPE-RISK-ASSESSMENT.md risks through to test scenarios. The Scope Risk Traceability table at the end of the document maps all 9 SCOPE-RISK-ASSESSMENT items to architecture risk IDs and resolution evidence.

**Critical risks (R-01, R-03) have 16 combined required scenarios** — this is a high-coverage bar that will catch the most likely regression vectors (serde field omission, test deletion without replacement).

**R-07 manual gate item** (AC-SR01) is correctly designated as non-automatable and must be marked OPEN or CONFIRMED before Gate 3c. This is consistent with the WARN-2 variance above.

**Security risks (SR-A, SR-B, SR-C)** are correctly identified. SR-A (prompt_snippet as attacker-controlled input flowing to embedding) is a real attack surface. SR-B (untrusted topic/task strings in query derivation) is correctly flagged. SR-C (source field injection into hook column) is a minor data integrity risk.

**EC-01 (whitespace-only prompt_snippet)** is an important edge case. The existing `if query.is_empty()` guard does not catch `"   "`. The risk is documented but not resolved in the architecture or specification. The recommendation to use `query.trim().is_empty()` should be confirmed by the spec writer before delivery.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #2298 (config key semantic divergence) and #2063 (single-file topology vs split-file vision language). Neither is directly applicable to crt-027. The most relevant recurring pattern here — scope additions bundled into implementation without explicit Goals listing — is represented by WARN-1 (MIN_QUERY_WORDS). This pattern has appeared across features.
- Stored: nothing novel to store — WARN-1 (unlisted scope additions in Proposed Approach) is a recurring pattern already observed, and the WA-4a deferral simplification is feature-specific to the Wave 1A/W3-1 boundary. The SR-01 open blocker pattern (unconfirmed external host behavior at design time) is feature-specific and does not generalize beyond Claude Code hook behavior.
