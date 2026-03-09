# col-017: Vision Alignment Report

**Feature**: Hook-Side Topic Attribution
**Reviewer**: uni-vision-guardian
**Date**: 2025-03-09
**Verdict**: **ALIGNED** — no blocking variances

---

## Vision Criteria Assessment

### V1: Self-Learning Expertise Engine — ALIGNED

col-017 directly enables the self-learning loop. The observation pipeline captures 3,200+ events/day but the feedback loop is broken — `context_retrospective` returns empty for 100% of features because `sessions.feature_cycle` is never populated. This feature closes that gap by pushing topic extraction to the hook edge, enabling retrospective hotspot detection, metrics, and knowledge extraction to operate on attributed sessions.

**Alignment strength**: Strong. Without topic attribution, the entire cortical learning pipeline (crt-001 through crt-008, col-002 retrospective, col-013 extraction) cannot produce feature-scoped insights. This is a prerequisite, not an enhancement.

### V2: Trustworthy, Correctable, Auditable — ALIGNED

- **Auditable**: Per-event `observations.topic_signal` column creates an audit trail of every extraction decision. The resolved topic on `sessions.feature_cycle` can be traced back to individual signals.
- **Correctable**: Content-based fallback preserves the correction path — if hook extraction fails, retrospective attribution still works.
- **Trustworthy**: Majority vote with priority ordering (path > pattern > git) and `is_valid_feature_id` filtering provides calibrated confidence. False-positive mitigation is addressed in R4 and SR-2.

No variance.

### V3: Invisible Delivery via Hooks — ALIGNED

This is a hook-native feature. Topic extraction happens in `build_request()` — the hook process — with zero agent cooperation. Signals flow through the existing UDS wire protocol to the server, which accumulates and resolves silently. No new MCP tool, no agent action required. This is the "invisible delivery" model in its purest form: observation data enrichment without any agent knowing it happens.

**Alignment strength**: Strong. Extends the hook pipeline rather than adding agent-facing surface area.

### V4: Three-Leg Boundary (Files / Unimatrix / Hooks) — ALIGNED

| Leg | col-017 Contribution |
|-----|---------------------|
| Files | No new files in `.claude/`. Process stays in agent definitions. |
| Unimatrix | Session attribution enables knowledge effectiveness measurement downstream (crt-018). |
| Hooks | Core implementation lives in hook-side extraction + server-side accumulation. |

The feature correctly places all logic in the hook/server layer. No workflow choreography leaks into Unimatrix. No knowledge storage changes. Clean boundary.

### V5: Zero Cloud Dependency / Embedded Engine — ALIGNED

All extraction is pure string scanning — no network, no external service, no cloud API. The `extract_topic_signal()` facade reuses existing attribution functions that are tested with 20+ unit tests. Schema migration is SQLite-native. No new dependencies.

### V6: Cross-Domain Portability (ASS-009) — ALIGNED with minor note

The extraction functions (`extract_from_path`, `extract_feature_id_pattern`, `extract_from_git_checkout`) are pattern-based and tied to the `product/features/{id}/` directory convention and `alpha-digits` feature ID format. These patterns are configurable at the server level (category allowlist, content scanning patterns per ASS-009), and the feature ID format is a project convention rather than hardcoded domain logic.

**Minor note**: The `extract_from_path` function scans for `product/features/{id}/` specifically. In a non-Unimatrix deployment, this path pattern wouldn't match. However, the fallback chain ensures that `extract_feature_id_pattern` (generic `alpha-digits`) still works across domains. The architecture document's facade design (ADR-017-001) encapsulates this priority chain, making future path pattern additions straightforward.

**Classification**: Cosmetic variance — no action required for col-017 scope. Cross-domain path patterns are a vnc-005 (config externalization) concern.

### V7: Activity Intelligence Milestone Alignment — ALIGNED

col-017 is Wave 1 of the Activity Intelligence milestone. The product vision explicitly lists it:

> **col-017: Hook-Side Topic Attribution** — Hook extracts topic signals from tool inputs per-event. Server accumulates signals per session, resolves dominant topic on SessionClose.

The scope matches the vision description exactly. No scope creep, no missing elements.

---

## Variance Summary

| # | Variance | Classification | Severity | Action |
|---|----------|---------------|----------|--------|
| 1 | `extract_from_path` uses Unimatrix-specific directory pattern | Cosmetic | None | Deferred to vnc-005 config externalization |

**No blocking variances. No scope variances. No architectural variances.**

---

## Artifact Quality Assessment

| Artifact | Quality | Notes |
|----------|---------|-------|
| SCOPE.md | Strong | Clear problem/solution/boundaries. In/out scope well-defined. Dependencies explicit. |
| SCOPE-RISK-ASSESSMENT.md | Strong | 7 risks identified with likelihood/impact/mitigation. Top 3 flagged for architect attention. |
| ARCHITECTURE.md | Strong | 3 ADRs addressing the 3 architect-attention risks. Component architecture with clear data flow. Integration surfaces documented. |
| SPECIFICATION.md | Strong | 8 FRs, 4 NFRs, 22 ACs with full traceability. Constraints comprehensive. Domain model clean. |
| RISK-TEST-STRATEGY.md | Strong | 17 risks (7 scope + 3 architecture + 7 specification). 17 tests across 4 priority tiers. Coverage map complete. Accepted risks documented with rationale. |

### Specification Discrepancy

One minor discrepancy between ARCHITECTURE.md and SPECIFICATION.md:

- **Architecture (ADR-017-002)**: Defines `TopicTally { count: u32, last_seen: u64 }` struct with `HashMap<String, TopicTally>`.
- **Specification (FR-05.1)**: Uses `topic_counts: HashMap<String, u32>` and `topic_last_seen: HashMap<String, u64>` as two separate HashMaps.

Both are functionally equivalent. The architecture's single-struct approach is cleaner (one lookup per signal instead of two). The specification's two-map approach avoids introducing a new type. **Neither breaks vision alignment.** Implementation should follow architecture (single struct) as it's the more recent decision and explicitly addresses R2.

- **Specification (FR-01.2)**: Says "Make `extract_from_path`, `extract_feature_id_pattern`, and `extract_from_git_checkout` `pub`" — contradicts ADR-017-001 which says individual extractors remain private and only the facade is public.

**Recommendation**: Follow ADR-017-001 (facade-only public). FR-01.2 should be treated as superseded by the ADR.

---

## Verdict

**ALIGNED** — col-017 is a well-scoped, vision-aligned feature that closes a critical gap in the self-learning pipeline. All artifacts are high quality with full traceability. One cosmetic variance (path pattern portability) deferred to vnc-005. Two minor spec/architecture discrepancies noted for implementation guidance. Ready for implementation.
