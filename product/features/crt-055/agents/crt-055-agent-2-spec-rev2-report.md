# crt-055 — Specification revision 2 (agent report)

**Agent**: crt-055-agent-2-spec-rev2
**Task**: Apply three binding product-owner decisions (2026-06-16) to SPECIFICATION.md. No code, no git.
**Artifact**: `product/features/crt-055/specification/SPECIFICATION.md`

## Decisions applied

1. **`context_reload_pct` = basis-points INTEGER (0–10000), not REAL.** Resolves prior Open Q4. Encoding: `compute_context_reload_pct` percentage × 100 rounded to nearest integer (37.5% → 3750). Every metric column on `cycle_review_index` is now integer (uniform with crt-047). No `is_finite()`/non-finite-float guard AC applies to this column — integer storage removes the footgun.
2. **Behavioral signals are coarse/directional.** `transcript_error_count`, `transcript_refusal_count`, `signal_class_counts_json` are unvalidated content-opaque regex matches; report renders them with a directional/coarse qualifier, distinct from exactly-counted aggregates. Presentation-honesty sibling to the "unavailable"-not-"0" guard.
3. **Clock/unit is a binding contract with integration test.** `compaction_reread` gate compares PostToolUse `ts` against `compaction_events.compacted_at` in Unix seconds; all timestamps normalized to seconds first (millis→seconds). Promoted from open question to binding requirement.

## Spec changes

- **FR-16** rewritten: basis-points INTEGER column, `NOT NULL DEFAULT 0`, encoding spelled out, resolves Open Q4.
- **FR-14** split: kept the gate definition; added **FR-14b** (clock/unit binding contract — normalize to seconds first).
- **FR-11b** added: coarse-signal honesty (directional qualifier in rendering).
- Domain model `context_reload` entry updated to basis-points integer.
- **AC-20** added: basis-points encoding verifiable (37.5%→3750, rounding, `pragma_table_info` type INTEGER not REAL).
- **AC-21** added: coarse-signal presentation honesty (qualifier present on behavioral signals, absent on exact aggregates).
- **AC-22** added: INTEGRATION-TEST clock/unit consistency (read 1s after compaction counts; millis-vs-seconds mismatch caught by normalization).
- **AC-12** annotated to align with FR-14b (kept seconds-aligned wording).
- **Constraints 9 & 10** added (clock/unit binding; integer-only metric columns).
- **Open Questions**: added a "Resolved (binding decisions)" subsection documenting Q4 (basis-points), clock/unit promotion, and behavioral-signal coarseness. Clock/unit removed from open status.

## No-float-guard note

The spec had no pre-existing `is_finite()`/non-finite-float AC scoped to `context_reload_pct` (AC-14 covers producer-width i64 conversion of transcript folds, unaffected). Integer encoding inherently removes the float footgun; captured the INTEGER-type assertion in AC-20.

## Open questions remaining (unchanged)

- Q1 Rank 1–3 column shapes (architect, vs ass-077 RQ-2)
- Q2 `compaction_reread` multi-compaction boundary selection (architect; note ADR-006 #5041 gates on earliest `compacted_at`)
- Q3 Knowledge-that-helped durability (design-session call)
- Q4 Default domain-neutral signature catalog (human/product; crt-054 producer concern, crt-055 confirms `0=error`, `1=refusal`)
- Q5 Internal wave order (design session)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-005 #5040 (dual reload, two columns/two gates/one engine, never collapsed) and ADR-006 #5041 (compaction_reread gates on earliest compacted_at per session); both consistent with current FR-16/FR-17 and FR-15. Read-only tier — no storage (spec decisions are feature-specific; retro may promote any that generalize).
