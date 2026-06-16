# Agent Report — crt-054-agent-2-spec (Specification)

**Task**: Regenerate SPECIFICATION.md for the producer-only re-scope (2026-06-16). Cover only Surface A (`compaction_events` table) and Surface B (`activity_snapshot()` fold + `[transcript_signals]` config). Overwrite the stale Jun 14 spec.

## Output
- `product/features/crt-054/specification/SPECIFICATION.md` (overwritten — prior wider-scope version replaced)

## Counts
- Functional requirements: 21 (FR-A1..A7, FR-B1..B10, FR-C1..C4)
- Non-functional requirements: 9 (NFR-1..NFR-9)
- Acceptance criteria: 15 (AC-01..AC-15), each with a verification method
- Binding constraints restated: 8
- Open questions: 4

## Constraint→AC tie-through (the binding edges)
- Held-route coverage → AC-06 (mandatory non-empty-source regression guard; registered-only/no-op does not satisfy)
- Fold survives to review → AC-07 (read-before-purge ordering)
- Late-bind attribution honesty → AC-03 (row written for undeclared session) + AC-12 (no fabricated zero)
- Lock ordering at write seam → AC-04 (SR-01)
- Never reintroduce #750 → AC-13 (no single-hook-event dependence)
- Bytes-only / no token surface → AC-15, NFR-2 (SR-06)
- Schema-version sequencing → AC-01, NFR-8 (SR-04, merge-order-assigned 29/30)
- Integer-width no-truncation → AC-14 (SR-03)

## Key decisions / interpretations
- **Followed the crt-055 binding contract over prior crt-054 ADRs.** Prior ADR-001/004 (#4999/#5002) describe an `ActivitySnapshot` with `saw_compaction`/`reload_after_compaction` latches and a `[u32;16]` literal. The contract's `ActivitySnapshot` is `{ bytes_total, delta_count, class_counts[MAX_SIGNAL_CLASSES] }` — no latches. Spec uses the contract shape and flags the residue for architect `context_correct` (SR-05). Also flagged stale ADR-008 (#5006) per SCOPE line 93.
- **Removed-scope exclusions made explicit**: no `cycle_review_index`, no `SUMMARY_SCHEMA_VERSION` bump, no reload/reckoning, no token field, no `reread`/`compaction` class, no vnc-036 wire change.
- `high_water` documented as server-captured at handler (not wire-precise), populated-and-reserved (SR-10).

## Open questions surfaced (for architect / design)
1. Lock ordering + transaction shape at `handle_compact_payload` — on-path vs deferred (SR-01).
2. `MAX_SIGNAL_CLASSES` value + default `error`/`refusal` catalog — align jointly with crt-055 design (SR-07).
3. Checked/saturating u64/u32 → i64 conversion at the producer→consumer boundary (SR-03).
4. SM coordination point assigning schema versions 29/30 by merge order (SR-04).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — prior crt-054/crt-052 ADRs returned; identified wider-scope residue that the binding contract supersedes. No storage (read-only tier).

---

# Rework — open-question resolutions (2026-06-16)

**Task:** Update SPECIFICATION.md to match the four resolved open questions and updated ADR-002 / ADR-007. Targeted edits only.

## Changes (FR / AC / NFR)

| Resolution | Edits |
|------------|-------|
| 1. MAX_SIGNAL_CLASSES = 16 pinned (== crt-055, not <=16) | Domain term def, NFR-6, AC-11 (adds `== 16` assertion), Open Q2 |
| 2. Default catalog = error + refusal only, calibrate-before-lock | FR-C2 tightened to "only"; new FR-C2a (delivery-time calibration, directional-not-precise); AC-10 ("exactly the two"); new AC-10a (calibration verification) |
| 3. compacted_at = Unix seconds documented in DDL comment | new AC-01a (inspect DDL/migration for explicit "Unix SECONDS" comment; ts/1000 noted as crt-055's) |
| 4. Cross-gate integration test co-owned with crt-055 | new AC-16 (just-after = post / just-before = pre across the seam; marked co-owned) |
| 5. INSERT-failure observability | new AC-04a (named counter compaction_events_insert_failed, non-blocking; fault-injection asserts counter increments) |
| 6. Producer-clean width contract | NFR-5 rewritten (producer cast-free u64/u32; ->i64 is crt-055's at persist); AC-14 rewritten (producer-side grep asserts no narrowing cast) |
| Open Questions section | All four marked RESOLVED with ADR cross-refs; residual AC-16 co-ownership coordination item noted (non-blocking) |
| Knowledge Stewardship | Updated query/findings block |

Added: FR-C2a, AC-01a, AC-04a, AC-10a, AC-16. Modified: domain term MAX_SIGNAL_CLASSES, FR-C2, NFR-5, NFR-6, AC-10, AC-11, AC-14, Open Questions, Knowledge Stewardship. Unaffected sections left untouched.

## Residual open question
- AC-16 ownership assignment: the cross-gate seconds-semantics test is co-owned; which feature's test plan physically lands it (with the other referencing it) is an SM coordination call at the producer/consumer test-plan handoff. Spec marks it co-owned and flags it; not blocking.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned updated ADR-002 (catalog/MAX_SIGNAL_CLASSES=16), crt-055 ADR-008 (#5043) config shape, crt-055 ADR-006 (#5048) reread boundary, crt-054 ADR-005 (#5030) never-persist envelope, ADR-010 (#5034) Wave B precondition. Findings confirm all four open questions resolved by ADR-002/ADR-007 + crt-055 FINAL producer contract (Binding constraint 8 = canonical seconds gate). No new knowledge stored (read-only tier).
