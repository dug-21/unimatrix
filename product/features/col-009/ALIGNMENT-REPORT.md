# Alignment Report: col-009

> Reviewed: 2026-03-02
> Artifacts reviewed:
>   - product/features/col-009/architecture/ARCHITECTURE.md
>   - product/features/col-009/specification/SPECIFICATION.md
>   - product/features/col-009/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope: product/features/col-009/SCOPE.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly implements "confidence feedback" and "Invisible Delivery" from vision |
| Milestone Fit | PASS | Correctly scoped to M5 Collective Phase; col-009 is explicitly listed |
| Scope Gaps | PASS | All 9 SCOPE goals addressed in source documents |
| Scope Additions | WARN | `injection_count` field on EntryAnalysis deferred to col-010 — minor addition to type definition not in SCOPE, but safely zeroed |
| Architecture Consistency | PASS | ADRs address all scope risks; component breakdown matches SCOPE proposed approach |
| Risk Completeness | PASS | 13 risks, 38 scenarios; all SR-XX scope risks traced; all AC-IDs covered |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `injection_count` on EntryAnalysis always 0 | SCOPE defines EntryAnalysis fields including `injection_count`. Specification notes it is "not col-009" (col-010 provides INJECTION_LOG data). Architecture defines the field in the struct. Populated as 0 at col-009. Rationale: the field is part of the intended schema; zeroing it avoids a future migration for col-010 to add it. |
| Simplification | `success_session_count` not explicitly tracked in signal | SCOPE goals describe tracking which entries correlated with success vs rework. Architecture accumulates success correlation only at the session level (success outcome → entry in helpful set). `success_session_count` in EntryAnalysis must be incremented at drain time. Specification FR-06.2 covers Flagged but does not specify `success_session_count` increment for Helpful. WARN: the field is present but its population path is not specified for Helpful signals. |

## Variances Requiring Approval

**No VARIANCE or FAIL classifications.** One WARN item requires awareness:

**WARN-01**: `success_session_count` population path not specified in FR-06.2

- **What**: `EntryAnalysis.success_session_count` is defined in the domain model (SPECIFICATION.md) and architecture (ARCHITECTURE.md), but FR-06.2 covers only the Flagged/rework accumulation path. The parallel increment for Helpful-signal sessions (`success_session_count += 1`) is not explicitly specified as a functional requirement.
- **Why it matters**: Without an FR covering this, implementation agents may not increment it, leaving `success_session_count` always 0. The retrospective analysis loses half its value (rework correlation without success baseline).
- **Recommendation**: Accept — add to specification before Session 2. Add FR-06.2b: "When `run_confidence_consumer` drains a Helpful SignalRecord, for each entry_id in the record, also increment `EntryAnalysis.success_session_count` in PendingEntriesAnalysis." This closes the gap without changing any other document.

## Detailed Findings

### Vision Alignment

**Product Vision (col-009 entry)**: "PostToolUse and Stop/TaskCompleted hooks that close the confidence evolution feedback loop without agent cooperation. Asymmetric signals: successful session → bulk `helpful=true` for injected entries (auto-applied via confidence pipeline); rework detected → entries flagged for human review in retrospective pipeline (never auto-downweighted)."

**Architecture alignment**: The ARCHITECTURE.md Component 3 (Signal Generation and Processing) implements exactly this: SessionClose triggers `process_session_close()` which calls confidence consumer (Helpful → `helpful_count`) and retrospective consumer (Flagged → `entries_analysis`). The asymmetric design is preserved — `unhelpful_count` is never touched by col-009 code.

**Specification alignment**: FR-06 covers retrospective consumer; FR-05 covers confidence consumer. No functional requirement modifies `unhelpful_count`. AC-06 explicitly verifies this invariant. The asymmetric design is testable.

**Vision section: "Invisible Delivery"**: col-009 operates entirely through hooks — no agent cooperation required, no MCP tool calls needed from agents to close the feedback loop. This is precisely the Invisible Delivery property described in the vision.

**Vision section: "auditable knowledge lifecycle"**: The architecture does not add audit trail entries for implicit signals, relying instead on `helpful_count` increments visible via `context_get`. This is acceptable — the confidence pipeline already records `helpful_count` changes via existing `updated_at` timestamps. No deviation from vision.

### Milestone Fit

**M5 Collective Phase** is the correct milestone. The product vision milestone dependency graph shows: `col-006 → col-007 → col-008 → col-009 (schema v4)`. col-009 is explicitly listed as "Confidence Feedback" with schema v4 (SIGNAL_QUEUE). The architecture correctly owns schema v4 (SIGNAL_QUEUE only); it does NOT include SESSIONS or INJECTION_LOG (those are col-010, schema v5). Milestone boundary respected.

**No M6 capabilities included**: The architecture does not implement thin-shell migration, multi-project support, or UI capabilities. No future-milestone scope creep.

**col-010 boundary respected**: The specification explicitly defers `from_structured_events()` col-002 alignment to col-010 (Resolved Design Decision #4). The architecture does not modify the JSONL parser. INJECTION_LOG is not created. Clean boundary.

### Architecture Review

**Schema v4 migration**: The architecture follows the established 3-step migration pattern (schema.rs constant bump + `migrate_v3_to_v4()` + `migrate_if_needed()` call). No entry scan-and-rewrite is needed — SIGNAL_QUEUE is new. This is simpler than prior migrations and lower risk. Consistent with ADR rationale.

**ADR-003 atomicity**: The `drain_and_signal_session` atomic design addresses SR-07 directly. The architecture's single-lock-acquisition pattern eliminates the race window identified in the scope risk assessment. Correct resolution.

**Session Intent Registry**: The architecture defines `SessionAction` as a closed enum (`ExplicitUnhelpful | ExplicitHelpful | Correction | Deprecation`) per SR-04 recommendation. No `Other(String)` escape hatch. FR-11.3 constrains col-009 use to ExplicitUnhelpful exclusion only. Future col-010+ consumers are the extension point. Clean scope boundary.

**`PendingEntriesAnalysis` cap**: Architecture specifies 1,000-entry cap with lowest-rework eviction, matching SR-05 recommendation. FR-06.3 specifies this in the specification. Covered.

**Files modified list**: Architecture provides a comprehensive file modification table (12 files). All modifications are within existing crates. No new binary or crate introduced. Consistent with "single binary" constraint.

### Specification Review

**AC-ID coverage**: All 13 ACs from SCOPE.md appear in SPECIFICATION.md with matching verification methods. No AC is dropped or weakened.

**Domain model completeness**: `SignalRecord`, `SessionAction`, `ReworkEvent`, `EntryAnalysis`, `SessionOutcome` are all defined with complete fields. The `// LAYOUT FROZEN` contract (ADR-001) is incorporated into FR-02.1.

**NFR-01.3**: "Signal generation does NOT add latency to the hook process response path." This is correct — the hook receives `HookResponse::Ack` from `SessionClose`, and consumers run inline in the server (not in the hook process). The 50ms hook latency budget is not affected by consumer processing.

**WARN-01 identified**: `success_session_count` population path missing in FR-06.2 (see above).

**Rework detection specification**: FR-04.5 and FR-07 together fully specify the PostToolUse event sourcing and threshold evaluation (edit-fail-edit × 3 per file per session). The `REWORK_EDIT_CYCLE_THRESHOLD = 3` constant is specified. R-03 test scenarios in RISK-TEST-STRATEGY cover both positive and negative threshold cases. Complete.

### Risk Strategy Review

**SR-XX traceability**: All 9 scope risks (SR-01 through SR-09) appear in the Scope Risk Traceability table. Every SR has a resolution — either mapped to an architecture risk with a mitigation, or resolved by design decision. No SR is orphaned.

**Coverage proportionality**: High-priority risks (R-01 race, R-02 migration, R-03 false positive, R-04 intercept, R-09 JSON extraction) have the most test scenarios (3–10 each). Low-priority risks (R-11, R-12, R-13) have 1–2 scenarios each. Proportional.

**Security risks**: SEC-01 (PostToolUse field extraction attacks), SEC-02 (confidence inflation blast radius), SEC-03 (context_retrospective drain attack) are all identified and linked to existing mitigations. No new attack surfaces are introduced by col-009 that are unaddressed.

**Failure modes**: All five failure modes (FM-01 through FM-05) describe system behaviour under partial failure — no FM describes a crash. Graceful degradation is the universal fallback. Consistent with the "exit 0" principle established in col-006 for hook processes.

**R-09 placement**: Likelihood is rated "High" for PostToolUse JSON field extraction failures. This is appropriate — Claude Code hook JSON format is not formally versioned and has historically changed between Claude Code versions. The test scenarios (10 cases) cover missing fields, wrong types, and all rework-eligible tool variants.
