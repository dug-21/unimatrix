# Alignment Report: crt-027

> Reviewed: 2026-03-23 (v2 — post-resolution re-review)
> Artifacts reviewed:
>   - product/features/crt-027/architecture/ARCHITECTURE.md
>   - product/features/crt-027/architecture/ADR-006-subagentstart-stdout-json-envelope.md
>   - product/features/crt-027/specification/SPECIFICATION.md
>   - product/features/crt-027/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope sources: product/features/crt-027/SCOPE.md
>                product/features/crt-027/SCOPE-RISK-ASSESSMENT.md
> Prior report: v1 produced 3 WARN, 0 FAIL. All three resolved — see §Resolution Evidence.

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directionally correct; deferred narrowing explicitly documented |
| Milestone Fit | PASS | Correctly targets Wave 1A; WA-5 contract surface correct |
| Scope Gaps | PASS | AC-SR01 confirmed via ADR-006; no open delivery blockers |
| Scope Additions | PASS | MIN_QUERY_WORDS now listed in SCOPE.md Goal #5 |
| Architecture Consistency | PASS | All SCOPE.md decisions implemented; SR risks fully addressed |
| Risk Completeness | PASS | RISK-TEST-STRATEGY covers all SCOPE-RISK-ASSESSMENT items with full traceability |

No variances requiring human approval. Two minor observations logged below.

---

## Resolution Evidence (v1 WARN Items)

### WARN-1 Resolved: MIN_QUERY_WORDS guard listed in SCOPE.md

SCOPE.md Goal #5 now explicitly reads: "Add `MIN_QUERY_WORDS: usize = 5` compile-time constant in `hook.rs`. UserPromptSubmit with fewer than 5 trimmed words produces no injection." ARCHITECTURE.md §2b specifies the constant and both guard forms (`.trim().is_empty()` + `.trim().split_whitespace().count()`). SPECIFICATION.md FR-05 mandates the guard with the same forms. The addition is no longer unlisted — it is a first-class Goal with full spec coverage.

### WARN-2 Resolved: SubagentStart stdout injection confirmed via ADR-006

ADR-006 confirms Claude Code supports SubagentStart context injection via the `hookSpecificOutput` JSON envelope. ARCHITECTURE.md §SR-01 is updated to "Confirmed." SPECIFICATION.md marks OQ-SR01 as RESOLVED, adds FR-04b (envelope format requirement), and adds AC-SR01 (CONFIRMED), AC-SR02 (envelope structure test), and AC-SR03 (UserPromptSubmit plain-text divergence test). The delivery blocker is closed. No spike required.

### WARN-3 Resolved: .trim().is_empty() guards fully specified

ARCHITECTURE.md §2 explicitly specifies `query.trim().is_empty()` for the SubagentStart empty guard and `query.trim().split_whitespace().count()` for the UserPromptSubmit word-count guard. SPECIFICATION.md FR-02 mandates `.trim().is_empty()` for SubagentStart. FR-05 mandates `.trim().split_whitespace().count()` for UserPromptSubmit. EC-01 is resolved — `"   "` (whitespace-only prompt_snippet) is correctly treated as absent. ARCHITECTURE.md §2a confirms this explicitly.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | WA-4a delivery scope narrower than product vision WA-4a | Vision WA-4a describes a phase-transition candidate cache drawn on PreToolUse. crt-027 routes SubagentStart to ContextSearch only. SCOPE.md §Non-Goals explicitly defers the candidate cache to W3-1. Documented with rationale. |
| Simplification | context_briefing does not use current_phase for category affinity | Vision WA-4b: "phase-condition the ranking using current_phase and category affinity." crt-027 uses the existing fused score only; phase-conditioned ranking deferred to W3-1. SCOPE.md §Non-Goals explicitly defers this. Documented and acceptable. |

No scope gaps. No unapproved scope additions.

---

## Variances Requiring Approval

None.

---

## Detailed Findings

### Vision Alignment

**WA-4a (SubagentStart routing):** The product vision WA-4a describes a phase-transition candidate cache rebuilt on phase transition and drawn from on PreToolUse events. crt-027 delivers a narrower but coherent step: SubagentStart routed through the existing ContextSearch pipeline with WA-2 histogram boost applied via the parent session_id. The deferral is explicit in SCOPE.md §Non-Goals. The feature advances the proactive delivery direction — SubagentStart now produces knowledge injection and an observation record instead of a fire-and-forget RecordEvent with no observation. This is a legitimate Wave 1A partial delivery.

**WA-4b (context_briefing as index):** Vision WA-4b: "Filter by topic, phase-condition the ranking using current_phase and category affinity, return structured top-k results." crt-027 delivers: active-only, k=20 flat index, three-step query derivation from session signals, WA-2 histogram boost. Phase-conditioned ranking is explicitly deferred to W3-1 in SCOPE.md §Non-Goals. The flat index format is a net improvement over BriefingService's section-partitioned output and the WA-5 dependency surface is correctly typed via IndexEntry and format_index_table.

**Proactive delivery direction:** The product vision states: "given what the agent knows, what they have been doing, and where they are in their workflow, surface the right knowledge — before they ask for it." crt-027 advances this at three surfaces: (1) subagent receives knowledge at spawn via SubagentStart injection, (2) SM calls context_briefing at every phase boundary per the updated delivery protocol, (3) WA-2 histogram boost feeds session context into briefing ranking when session_id is provided. All three are directionally correct.

**ADR-006 (SubagentStart JSON envelope):** The confirmed `hookSpecificOutput` envelope requirement is a clean architectural decision. The server remains unchanged (returns HookResponse::Entries); the hook process handles format divergence. This follows good separation-of-concerns — the server is agnostic to how its response is rendered to stdout. The graceful degradation fallback (observation still recorded if stdout is ignored) means WA-4a delivers value on both the confirmed injection path and any hypothetical degraded path.

### Milestone Fit

crt-027 is correctly positioned in Wave 1A — after WA-2 (category histogram boost, confirmed COMPLETE per commit c0a79a2) and before WA-5. The WA-5 dependency surface is correctly established: IndexEntry is a typed struct, format_index_table is a named function, SNIPPET_CHARS is referenced in the risk strategy as a 150-char constant. The feature does not touch Wave 2 (deployment), W3-1 (GNN), or any completed Wave 0/1 scope. Phase-conditioned ranking is fenced to W3-1. Milestone discipline is maintained.

### Architecture Review

**SR-01 (SubagentStart stdout) — CONFIRMED AND SPECIFIED.** ADR-006 documents the `hookSpecificOutput` JSON envelope requirement from Claude Code documentation. ARCHITECTURE.md §SR-01 is updated accordingly. The `write_stdout_subagent_inject` helper is specified with its exact signature and JSON structure. Unit tests for AC-SR01/SR02/SR03 are specified in the SPECIFICATION.md. This is fully resolved.

**SR-03 (EffectivenessStateHandle wiring) — RESOLVED.** `IndexBriefingService::new()` takes `effectiveness_state` as a required non-optional parameter following the ADR-004 crt-018b pattern. Missing wiring is a compile error. The cached_snapshot initialization follows the same pattern as BriefingService.

**SR-04 (format_compaction_payload test loss) — RESOLVED.** ARCHITECTURE.md §SR-04 provides an 11-row table mapping old test names to invariant survival decisions and new test forms. Two invariants (section ordering, deprecated indicator) are correctly retired with replacement invariants (confidence sort, active-only suppression). Nine invariants survive. The spec covers these as AC-16 through AC-21.

**SR-05/ADR-003 (UNIMATRIX_BRIEFING_K fate) — RESOLVED.** Env var deprecated, explicitly not read by IndexBriefingService, k=20 hardcoded. `parse_semantic_k()` deleted. AC-07 specifies the runtime test confirming the env var has no effect.

**SR-06/ADR-005 (WA-5 format contract) — RESOLVED.** IndexEntry is a typed struct. `format_index_table` is a named function. RISK-TEST-STRATEGY R-05 scenario 3 references a SNIPPET_CHARS constant. The compile-time surface is stable.

**Minor observation — `derive_briefing_query` location TBD.** ARCHITECTURE.md integration surface table notes the location as "TBD: services/briefing.rs or new services/query_derive.rs." This is an implementation-time decision with no design risk — FR-11 requires a shared function and R-06 tests confirm it; the file location does not affect correctness or the WA-5 surface. No WARN required.

**Minor observation — `SNIPPET_CHARS` constant named in RISK-TEST-STRATEGY but not in SPECIFICATION.md.** R-05 scenario 3 asserts `SNIPPET_CHARS` exists and equals 150. SPECIFICATION.md FR-10 and FR-12 specify the 150-char limit as a prose value, not a named constant. If the implementer uses an inline literal `150` rather than a named constant, R-05 scenario 3 will fail. The spec writer should consider adding `SNIPPET_CHARS: usize = 150` to the domain model or NFR-04 to make this explicit. Low impact — R-05 scenario 3 will catch any drift at Gate 3c regardless.

### Specification Review

The SPECIFICATION.md is complete. All prior gaps are closed. FR-04b (SubagentStart JSON envelope) and AC-SR01/SR02/SR03 are well-specified with exact JSON structure and divergent-path assertions. FR-02 mandates `.trim().is_empty()` for SubagentStart. FR-05 mandates `.trim().split_whitespace().count()` for UserPromptSubmit.

**AC-SR03 (UserPromptSubmit plain-text divergence)** adds a test that asserts UserPromptSubmit stdout does NOT contain `"hookSpecificOutput"`. This is a good regression guard — it ensures the two paths do not accidentally converge during implementation.

**NFR-05 and NFR-06** correctly address the mcp-briefing feature flag split and test count non-decrease requirement. AC-24 confirms the always-compiled path is exercised without the feature flag.

**OQ-SR08 (cold-state topic fallback quality)** remains open at low risk. The spec correctly handles the empty-result case: `Ok(vec![])` without error, format_compaction_payload returns None when both entries and histogram are empty (AC-18). No WARN — the risk is correctly classified as low and the graceful empty-result path is fully tested.

**`format_index_table(&[])` empty behavior:** R-05 scenario 4 in RISK-TEST-STRATEGY states `format_index_table(&[])` returns an empty string. SPECIFICATION.md does not have a corresponding AC for `format_index_table` itself on empty input (AC-18 covers `format_compaction_payload` only). The risk strategy closes this gap for the gate reviewer. No WARN — the coverage is present in the right-layer test document.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is thorough and unchanged from v1 in its coverage quality. It has been updated with the following:

- R-07 now references the confirmed SR-01 resolution and ADR-006. The manual smoke test is still required at Gate 3c for final confirmation, now framed as post-delivery validation rather than a pre-delivery blocker. The test note correctly states: "AC-SR01 in the spec must be explicitly marked OPEN or CONFIRMED before Gate 3c." Given ADR-006 confirms behavior via documentation, this item should be marked CONFIRMED before Gate 3c.
- The Scope Risk Traceability table covers all 9 SCOPE-RISK-ASSESSMENT items.
- 11 non-negotiable test names are listed for Gate 3c grep verification per lesson #2758.
- Security risks SR-A (prompt_snippet injection), SR-B (untrusted topic strings), SR-C (source field injection into hook column) are correctly identified and scoped.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #2298 (config key semantic divergence) and #2063 (single-file topology vs split-file vision language). Neither applicable to crt-027. The MIN_QUERY_WORDS unlisted-addition pattern from WARN-1 is now resolved by the scope update and does not generalize to a new pattern.
- Stored: nothing novel to store — all three resolved WARNs are feature-specific resolutions. WARN-1 (scope additions without Goals listing) is already a known pattern in the project. WARN-2 (unconfirmed external host behavior) was specific to Claude Code SubagentStart documentation state at design time. WARN-3 (whitespace guard specification gap) is a feature-specific spec completeness item. None of these resolutions represent a new recurring pattern worth storing.
