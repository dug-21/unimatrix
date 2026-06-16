# crt-055 Retrospective — Architect Report

**Agent**: crt-055-retro-architect (uni-architect) | **Mode**: retrospective | **Date**: 2026-06-16
**Feature**: crt-055 (#755), PR #761 merged — context_cycle_review redesign (durable per-cycle aggregates + dual reload + transcript fold)

---

## 1. Patterns

**New**: none. No genuinely new 2+-feature structure emerged at retro that the impl-phase agents hadn't already captured.

**Carried forward (verified high-quality, no drift)** — the 5 impl-phase patterns stored this cycle all pass their what/why/scope template and are component-specific reusable traps; left active:
- **#5066** review pipeline single mutable `ReviewAggregateState` accumulator threading single-writer discipline across multi-scope handler stages — strong, names 4 concrete gotchas.
- **#5062** fail-loud: per-metric availability vs always-directional behavioral signals as two orthogonal axes.
- **#5063** rank-1/2/3 reckoning: next_phase-driven phase model (no `cycle_phase_start` literal), #320 union dedup, rank-2 classifier reuse.
- **#5064** split registry-reading fold from pure summation to unit-test u64::MAX saturation; serde_json::Map for stable JSON.
- **#5065** compaction_reread must drive `overlap_count` per-session with that session's MIN boundary, not one shared boundary.

**Drift check on patterns this cycle FOLLOWED**: all clean, no correction:
- **#4178** (derived aggregates -> cycle_review_index, not cycle_events; single store_cycle_review writer; bump SUMMARY_SCHEMA_VERSION) — followed exactly; active and accurate.
- **#4750** (four returns / single writer), **#4153** (three-path bump), **#4484** (cascade-file before migration) — all honored per Gate 3b/3c; no drift surfaced.

**Skipped (and why)**:
- Multi-source accumulator across handler scopes — already #5066; not re-stored.
- `test-support` feature-flag cross-crate infra — already #747; followed cleanly, no increment.

## 2. Procedures

**Updated**: **#4967 -> #5071** ("How to run workspace tests without hanging") via context_correct. Added two crt-055-validated items the active procedure lacked:
1. **`--features test-support` is REQUIRED** for the feature-gated store/migration tests (`#![cfg(feature = "test-support")]`). Without it those targets report "running 0 tests" — EXPECTED, not a failure (matches the `migration_vN_to_vN+1.rs` convention). This is the exact trap that made the crash-recovered Component-1 migration file look like "0 tests."
2. **`--jobs 2` mitigates the `cc` linker OOM** on workspace runs that link the large server integration binaries (crt-055: default parallelism OOM-killed `cc`; `--jobs 2` -> 6436/0). Broader sibling of the existing per-`--test` ladder (same OOM root cause, different mitigation).

**Not stored (already covered)**:
- Mid-task agent-death recovery — **#4762** (audit-then-resume: diagnose cause, inventory partial state via git, establish a known-good floor with `cargo build` + targeted test, respawn fresh with partial-work preamble, audit-before-write) covers the crt-055 Component-1 double-death recovery precisely. The crt-055 incident is a clean re-application (verify partial tree compiles, run targeted `--features test-support`, confirm the 16-field Default-cascade `..Default::default()` edits are REQUIRED not scope creep, reconstruct the lost report as a factual note). No increment; not re-stored.

## 3. ADR status

10 crt-055 ADRs (active set: #5037/#5039/#5042/#5043/#5044/#5045/#5046/#5047/#5048/#5051). All validated by successful implementation (Gate 3b 7/7, Gate 3c 11/11). The earlier-numbered siblings (#5036/#5038/#5040/#5041 etc.) are properly DEPRECATED predecessors from in-design corrections — the correction chain is intact, no orphan stale ADRs.

**Corrected**: **ADR-005 #5047 -> #5068** via context_correct (prose fix, in-bounds — DECISION unchanged). The active ADR-005 still carried the inherited error "compute_context_reload_pct returns a percentage" / `round(pct x 100)`. The live function returns a **FRACTION in [0.0,1.0]**; implementation correctly used `round(fraction x 10000)` (reload_overlap.rs:217). The basis-points-INTEGER decision was correct and validated; only the multiplier wording was wrong. Corrected to cite the fraction and the live encoding.

**Validated, no change**: ADR-006 #5048 (millis->seconds gate normalization — the other inherited-assumption hazard, but this one was caught and corrected IN design; impl confirms it), ADR-002 #5037 (single writer / four returns — Gate 3c confirms one write site at tools.rs:3032), ADR-007 #5042 (read-before-purge — inversion test load-bearing), ADR-008 #5043, ADR-010 #5045 (auto_close idempotent before pipeline) — all sound.

No ADR was revealed wrong/incomplete by implementation. No supersession needed (no human-approval flag raised).

## 4. Lessons

**New (2)**:
- **#5069** — "ADR/brief prose about an EXISTING function's return shape must be verified against the live signature before encoding math on it." The fraction-vs-percentage drift. `Supports` edge -> #5068 (the corrected ADR-005, which embodies the fix). Distinct from #1496: this is units/shape-of-an-existing-function drift, not cross-artifact constant divergence.
- **#5070** — "Reconciling a worked-example/numeric defect must grep ALL artifacts including the routing/overview index docs." The AC-22 two-iteration rework: first pass fixed the 5 named files but missed `test-plan/OVERVIEW.md` (the routing doc); a second validator pass caught it. `Supports` edge -> #1496. #1496 says define-once/cross-check-siblings; #5070 adds that the sibling set MUST include routing/index docs and the sweep is grep-by-old-value, not fix-the-named-files.

**Not stored**: the 950% ratio oddity (runtime, not a knowledge artifact — see findings), and the agent-death recovery (already #4762).

## 5. Retrospective findings (hotspot-derived + follow-ups)

- **Hotspots are largely benign/expected for an ADR-heavy 3-crate feature.** `adr_count 10` (3.3x typical), `edit_bloat`, `context_load_before_first_write_kb` 4.1σ, `mutation_spread 101 files` — these track the feature's intrinsic size (10 ADRs, 9 components, design artifacts spanning crt-055 AND its paired producer crt-054). #1271 and #3809 (normalize hotspots by component count; separate code-vs-artifact mutation) already cover this class; no new lesson. The `mutation_spread` warning is specifically the design-artifact inflation #3809 describes — code-only mutation was the 11 new `.rs` files.
- **`tool_failure_hotspot` (Read failed 8x) + `session_timeout 7.6h` + `cold_restart 64-min`** map to the documented infra instability (Component-1 dev agent died TWICE on upstream API 500s) and the multi-day design->delivery span. Recovery was handled correctly per #4762. Not a process defect.
- **Gate 3a took 2 iterations for 1 defect** — the AC-22 incremental-reconciliation miss. Now captured as #5070; the prevention (grep-by-old-value across ALL artifacts incl. routing docs) should shorten future worked-example reworks to one pass.

**Follow-up recommendation (not a retro store)**:
- **950% ratio display** — the shipped tool renders `Knowledge reuse: 19 of 2 (950%)`. The #320 union num/den semantics produce a >100% ratio (cross-feature served 19 vs intra-cycle stored 2). The durable column populates correctly and num/den is not pre-divided (correct per design), but the **ratio presentation is confusing**. Recommend a follow-up GH issue to clamp/relabel the displayed ratio (e.g. distinguish "served across all cycles" from "stored this cycle" rather than dividing them into a single percent). Functional, not a defect — display polish only.

## 6. Relationship edges

Two asserted, both meet the HIGH traversal-necessity bar (a future agent must follow the link to avoid a wrong decision):
- **#5069 `Supports` #5068** — a future agent reading the corrected ADR-005 (basis-points encoding) must be able to reach the lesson explaining WHY the multiplier is x10000-of-a-fraction not x100-of-a-percentage, or risk re-introducing the off-by-100x against the live `-> f64`. One-clause: the lesson is the evidence that validates the corrected encoding.
- **#5070 `Supports` #1496** — a future agent reconciling a numeric/worked-example defect via #1496 must traverse to #5070 to learn the sibling set includes routing/index docs and the sweep is grep-by-old-value; following #1496 alone reproduces the crt-055 two-iteration miss. One-clause: #5070 is the direct extension that closes #1496's coverage gap.

No `Prerequisite` or `Contradicts` edges — bar not met. Supersession (ADR-005 #5047->#5068) was handled by context_correct, not an edge, per convention.

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search / context_get (crt-055 stored set #5037-#5048/#5051/#5062-#5066; served-lesson #1496; followed-patterns #4178/#4750-class; existing procedures #4967/#4339/#747; recovery #4762) — verified the 12 cycle stores are correctly categorized and the ADR correction chain is intact (deprecated predecessors retired by provenance, not orphaned).
- Stored: lesson #5069 "ADR prose vs live signature" (Supports #5068); lesson #5070 "grep ALL artifacts incl. routing docs" (Supports #1496); corrected ADR-005 #5047 -> #5068 (fraction/x10000 prose fix); corrected procedure #4967 -> #5071 (added --features test-support + --jobs 2). No new patterns — impl-phase patterns #5062-#5066 already cover the reusable structures; agent-death recovery already #4762.
