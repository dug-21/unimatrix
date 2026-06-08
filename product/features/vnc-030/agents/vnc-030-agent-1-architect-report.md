# vnc-030 Architect Agent Report

**Agent**: vnc-030-agent-1-architect · **Date**: 2026-06-08 · **Issue**: #699

## Artifacts
- `product/features/vnc-030/architecture/ARCHITECTURE.md`
- ADR-001 .. ADR-007 (see list below)

## ADR file list
- `architecture/ADR-001-cycle-tracker-lifecycle.md` (Unimatrix #4813)
- `architecture/ADR-002-stamp-decoration-fnf-path.md` (#4814)
- `architecture/ADR-003-cycle-stamp-wire-field.md` (#4815)
- `architecture/ADR-004-feature-source-precedence.md` (#4816)
- `architecture/ADR-005-topic-source-taxonomy-migration.md` (#4817)
- `architecture/ADR-006-stamp-miss-canary.md` (#4818)
- `architecture/ADR-007-cross-feature-seam-contracts.md` (#4819)

## Key decisions
1. New `cycles.js` module owns the tracker file; lifecycle is create-on-cycle_start / update-on-phase-end / delete-on-stop, never on Session*/Stop (no delete-on-close — Stop fires per turn). Worktree paths route through `config.resolve().stateDir`, never raw-cwd hashing.
2. Stamp attach is FNF-path decoration in `index.js`; `buildRequest` stays pure (minimizes vnc-027 rebase surface). Suppression = strip topic_signal at decoration; extraction code unchanged. Delta frames unstamped.
3. Additive `ImplantEvent.cycle_stamp: Option<CycleStampPayload{topic, phase?}>`, 7th ts-rs export, frozen-F1 safe; binding end-to-end round-trip AC at all three server record sites (#3486).
4. `FeatureSource::{Declared, Inferred(InferredOrigin{Registered,Voted})}`; new idempotent `apply_stamp`; both declared-vs-vote inversions flipped (session.rs:628 + close path), minimal-diff for crt-052. Registry touchpoints fenced to exactly four (SR-05).
5. `observations.topic_source` v27->v28 pragma-guarded migration; one write site per taxonomy value; no backfill.
6. `stamp_miss` canary as a design invariant: production trigger `stamp_miss/fnf_sends > 0.20`, pinned CLI claude 2.1.167, documented concurrent-session noise source.
7. Four written cross-feature seam contracts (vnc-027 seam-survival test, crt-052 close/sweep interface, #574 no-race + expiry, marker-recovery follow-up consuming crt-052 snapshot seam).

## Measured client size budget (SR-02)
Current hook-client: 99,997 B raw / ~63,259 B stripped. vnc-030 additions: ~3,900 B raw / ~2,050 B stripped. Fits both post-vnc-027 limits (160k raw / 100k stripped) with wide headroom. Fallback documented (fold cycles.js into state.js; move citations to OVERVIEW). Hard ordering dependency unchanged: client work lands strictly after vnc-027's ADR-005 gate rewrite.

## Open questions (for synthesizer / human)
1. **Marker-recovery follow-up issue must be filed at design-gate exit** (SR-07) with the crt-052 snapshot-seam dependency clause — owner unassigned. The Design Leader should create it.
2. **vnc-027 OQ5 worktree cwd dump is resolved** (cwd probe report) — the AC-08 test shape is settled; no longer blocking.
3. **Canary production threshold (0.20)** is a proposed default; delivery measures the live concurrent-session noise baseline during AC-07 and may revise it by human decision on #699.
4. **`enrich_topic_signal` doc/AC-08/FR-14 references** invert under ADR-004 — delivery owns the doc rewrite; flagged so it is not mistaken for a regression.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get -- found and applied vnc-027 ADR-005 size gate (#4806), ADR-004 hook-set reduction; #3486 (cycle field payload-construction gap); #4092/#4358/#1264 migration pragma guards; v9->v10 topic_signal precedent; #4772 (never pre-sanitize session key); vnc-024 ADR-001 #4726 (ts-rs export); #4140/#1067/#3382 attribution constraints.
- Stored: entries #4813-#4819 (ADR-001..007) via context_store category=decision. No separate pattern/lesson stored -- all novel findings are feature-specific ADRs; the reusable migration/ts-rs/size-gate patterns already exist in Unimatrix and were cited, not duplicated.
