# Alignment Report: crt-018b

> Reviewed: 2026-03-14
> Artifacts reviewed:
>   - product/features/crt-018b/architecture/ARCHITECTURE.md
>   - product/features/crt-018b/specification/SPECIFICATION.md
>   - product/features/crt-018b/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-018b/SCOPE.md
> Scope risk source: product/features/crt-018b/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances the auditable knowledge lifecycle and retrieval quality goals |
| Milestone Fit | PASS | Targets Search Quality Enhancements milestone; correct dependency position (after crt-019) |
| Scope Gaps | PASS | All four SCOPE goals covered by source docs |
| Scope Additions | PASS | Generation counter (ADR-001) resolved: Option A selected, SPECIFICATION §NOT in Scope item 7 removed |
| Architecture Consistency | PASS | Generation counter variance resolved (Option A): ADR-001, Component 1, Component 3 are authoritative |
| Risk Completeness | PASS | RISK-TEST-STRATEGY covers all 8 SCOPE-RISK-ASSESSMENT items; adds 6 additional risks |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | None | All four SCOPE goals (utility signal in search, effectiveness-weighted briefing, auto-quarantine with N-cycle guard, configurable threshold) are addressed in all three source documents |
| Addition | Generation counter / `EffectivenessSnapshot` cache | ARCHITECTURE Component 3 (ADR-001) adds `generation: u64` + `Arc<Mutex<EffectivenessSnapshot>>` shared-cache (SR-02 recommendation). **Resolved**: Option A selected — SPECIFICATION §NOT in Scope item 7 removed; generation counter is in scope. |
| Simplification | None | No scope items were simplified without documentation |

---

## Variances

### VARIANCE 1 — Generation Counter — RESOLVED (Option A)

**Resolution**: Option A selected. Generation counter (ADR-001) is in scope. SPECIFICATION §NOT in Scope item 7 removed. R-06 test scenarios stand. ARCHITECTURE is authoritative on this point.

---

## Detailed Findings

### Vision Alignment

**Finding: PASS**

The product vision identifies the "Search Quality Enhancements" milestone as the current target milestone, with crt-018b on Track A (Confidence & Effectiveness). The vision states: "Boost Effective entries, penalize Ineffective/Noisy. Auto-quarantine entries consistently below utility threshold after N cycles." The three source documents collectively deliver exactly this: an additive utility signal in re-ranking (ARCHITECTURE Component 3, SPECIFICATION FR-05/FR-06), a briefing tiebreaker (ARCHITECTURE Component 4, SPECIFICATION FR-07/FR-08), and the N-cycle auto-quarantine mechanism (ARCHITECTURE Component 5, SPECIFICATION FR-10/FR-12).

The feature honors all three strategic legs of the vision:
- **Files**: No changes to workflow files or agent definitions.
- **Unimatrix**: Improves what `context_search` and `context_briefing` return — directly advancing the "trustworthy, retrievable, ever-improving" knowledge goal.
- **Hooks**: No hook-side changes, as expected for a retrieval quality feature.

The "auditable knowledge lifecycle" principle is specifically honored: auto-quarantine writes a rich audit event (FR-11) with entry_title, classification, consecutive_cycles, threshold, and reason — enough for operators to diagnose and reverse false positives. Workflow 4 in SPECIFICATION.md documents the operator recovery path.

The "invisible delivery" principle is preserved: the utility signal operates inside the existing search and briefing pipelines with no agent cooperation needed. Agents continue calling `context_search` and `context_briefing` unchanged; they receive better-ranked results automatically.

### Milestone Fit

**Finding: PASS**

The product vision roadmap places crt-018b on Track A of the Search Quality Enhancements milestone, with an explicit dependency on crt-019 ("confidence spread established before adding another signal"). SPECIFICATION.md §Dependencies §External / Prior Features confirms crt-018 and crt-019 must both be merged. SPECIFICATION AC-17 item 4 adds an integration test prerequisite confirming crt-019's adaptive weight is non-zero in the test fixture (also captured in RISK-TEST-STRATEGY R-14). This dependency discipline is correctly observed.

No features from future milestones (Graph Enablement or beyond) are pulled in. The feature does not add petgraph, does not touch co-access transitivity, and does not introduce new MCP tools. Milestone discipline is maintained.

### Architecture Review

**Finding: PASS (generation counter variance resolved — Option A)**

The overall architecture is sound and follows established patterns correctly:

- **ConfidenceState mirroring**: `EffectivenessState` as `Arc<RwLock<_>>` with background tick as sole writer and read-lock snapshots at query time is a direct application of the crt-019 ConfidenceState pattern. The SCOPE explicitly calls for this pattern and ARCHITECTURE Component 1 delivers it correctly.

- **Additive delta formula**: ARCHITECTURE Component 3 provides a complete combined scoring formula with all active signals at both crt-019 spread extremes. This directly resolves SCOPE-RISK-ASSESSMENT SR-04. The utility delta is correctly positioned inside the `status_penalty` multiplication (ADR-003), preventing Effective classification from overriding lifecycle penalties for Deprecated/Superseded entries.

- **Error semantics**: ARCHITECTURE Component 2 adopts the hold-on-error semantics for `consecutive_bad_cycles` on `compute_report()` failure (ADR-002), resolving SCOPE-RISK-ASSESSMENT SR-07. The `tick_skipped` audit event is specified (Component 6).

- **BriefingService constructor**: `EffectivenessStateHandle` is a required constructor parameter (ADR-004), resolving SCOPE-RISK-ASSESSMENT SR-06 and R-06.

- **Auto-quarantine audit richness**: The audit event schema in Component 6 includes `title`, `topic`, `category`, `classification`, `consecutive_cycles` — resolving SCOPE-RISK-ASSESSMENT SR-03.

- **Write lock release before SQL**: ARCHITECTURE Component 5 states "After the `EffectivenessState` write in the background tick, scan `consecutive_bad_cycles`" — the write lock logic in Component 2 shows auto-quarantine check happens while holding the write lock (step 3: "Check auto-quarantine (Component 4) while holding the write lock"). This is in direct conflict with NFR-02 in SPECIFICATION which requires the write lock to be released before any SQL write. RISK-TEST-STRATEGY R-13 identifies this as a Critical risk.

  Closer reading: ARCHITECTURE Component 5 says quarantine calls go through the `spawn_blocking` path. The write lock section (Component 2 step 3) says the check happens while the lock is held, but the quarantine SQL call happens *after* in Component 5. The flow diagram shows the auto-quarantine scan as part of the write-lock section but the `store.quarantine_entry()` call as a separate step. The architecture does not explicitly state the write lock is dropped between the scan and the SQL call. SPECIFICATION NFR-02 is explicit: "write lock on `EffectivenessState` in `maintenance_tick()` must be held only for the duration of the in-memory map update... It must be released before any SQL write (auto-quarantine) is issued." The implementation team must resolve this by dropping the write guard before calling `quarantine_entry()`.

- **Generation counter**: Variance resolved (Option A) — ARCHITECTURE (ADR-001) is authoritative; SPECIFICATION §NOT in Scope item 7 removed.

### Specification Review

**Finding: PASS**

The specification is thorough, well-structured, and traceable. All 18 acceptance criteria from SCOPE.md are reproduced faithfully and extended with additional precision:

- SCOPE AC-01 through AC-18 are present in SPECIFICATION and enhanced with explicit verification methods.
- FR-01 through FR-14 cover all SCOPE goals plus the additional NFRs needed for correctness (lock budget, write lock duration, cold-start safety, no retroactive quarantine on deployment).
- The NOT in Scope section correctly defers embedding/ML training (SCOPE Non-Goal 4), retrospective per-entry contribution (SCOPE Non-Goal 5), and retroactive quarantine (SCOPE Non-Goal 8).

The combined formula in FR-06 with explicit numerical worked examples at both spread extremes directly satisfies SCOPE-RISK-ASSESSMENT SR-04. The SETTLED_BOOST constraint in FR-04 with explicit comparison to co-access max (0.03) satisfies SR-05.

One observation: FR-07 uses a 3-2-1-0-0 scale for effectiveness_priority (`Effective(3) > Settled(2) > Unmatched(1) = nil(1) > Noisy(0) = Ineffective(0)`), while ARCHITECTURE Component 4 uses a different scale (`Effective: 2, Settled: 1, None/Unmatched: 0, Ineffective: -1, Noisy: -2`). The ordering semantics are equivalent (Effective ranks highest, Noisy/Ineffective rank lowest, Settled intermediate), but the numeric values differ and the Noisy/Ineffective relationship inverts within the SPECIFICATION scale (both 0) versus ARCHITECTURE (-1/-2). This is a minor inconsistency in a sort-key function that has no behavioral impact as long as Ineffective = Noisy at the bottom tier, but the implementation team should pick one canonical set of values and use them consistently in code.

### Risk Strategy Review

**Finding: PASS**

The RISK-TEST-STRATEGY provides complete coverage of all SCOPE-RISK-ASSESSMENT items plus adds six original risks not identified in SCOPE:

**SCOPE risks fully addressed**:
- SR-01 (restart resets counters): Accepted risk, documented as Constraint 6/NFR-07 and in the Scope Risk Traceability table.
- SR-02 (HashMap clone growth): Addressed by ADR-001 / generation counter (see Variance 1).
- SR-03 (auto-quarantine false positive): R-11, R-03, FR-11/FR-14/Workflow 4.
- SR-04 (delta-vs-confidence-weight interaction): R-05, R-14, FR-06 combined formula, ADR-003.
- SR-05 (Settled boost dominance): R-10, Constraint 5.
- SR-06 (BriefingService wiring miss): R-06 (with ADR-004 compile-error mitigation), R-09.
- SR-07 (tick error increments counter): R-04, R-08, R-13, ADR-002.
- SR-08 (crt-019 dependency not exercised): R-14, AC-17 item 4, Constraint 11.

**Additional risks identified by RISK-TEST-STRATEGY** beyond SCOPE:
- R-01 (double-lock deadlock) — Critical, not in SCOPE-RISK-ASSESSMENT
- R-02 (delta applied at inconsistent call sites) — Critical
- R-03 (bulk auto-quarantine SQLite contention) — Critical
- R-06 (generation cache not shared across service clones) — predicated on ARCHITECTURE including generation counter
- R-12 (`auto_quarantined_this_cycle` field not populated) — Low
- R-13 (write lock held during SQL) — Critical

All fourteen risks have test scenarios mapped to acceptance criteria. The coverage summary accounts for 42 scenarios across four priority tiers. Security risks (DoS via env var, audit identity, lock poison) are present and appropriately scoped.

The integration risks section adds one novel scenario not in SCOPE: silent Phase 8 empty-result overwriting live classifications. This is a real failure mode and the test scenario (inject empty-result fixture, verify no empty-state replacement) is well-defined.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for topic `vision` — Unimatrix MCP tools not available as deferred tools in this execution context; no results retrieved. Pattern query was attempted but deferred tools were not resolvable.
- Stored: nothing novel to store — the generation-counter contradiction between ARCHITECTURE and SPECIFICATION is feature-specific (not a recurring cross-feature pattern). The write-lock-before-SQL ambiguity is already captured as R-13 and pattern #1366 in the knowledge base. No new generalizable patterns were identified in this review.
