# Gate 3a Report: col-026

> Gate: 3a (Component Design Review) — Rework Iteration 1
> Date: 2026-03-25
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | Component boundaries, crates, ADR compliance all confirmed |
| Specification coverage | WARN | 16-metric FR-11 table used correctly; spec/arch naming gap (GAP-3) documented but non-blocking |
| Risk coverage | PASS | R-07 fix confirmed: `test_section_order` expected_order now has Baseline Outliers before Findings, matching SPECIFICATION §FR-12 and formatter pseudocode |
| Interface consistency | WARN | GAP-1 (PhaseStats start_ms/end_ms) documented open gap; implementation agent must resolve. All other interfaces consistent. |
| Knowledge stewardship | PASS | Both agent reports contain stewardship blocks with Queried and Stored entries |

---

## Detailed Findings

### Check 1: Architecture Alignment
**Status**: PASS

**Evidence**:
- Component 1 (`types.rs`): All new types (`PhaseStats`, `ToolDistribution`, `GateResult`, `EntryRef`) and struct extensions match ARCHITECTURE.md §Components 1–2 exactly.
- Component 2 (`tools.rs`): `compute_phase_stats(&[CycleEventRecord], &[ObservationRecord]) -> Vec<PhaseStats>` signature matches ARCHITECTURE.md §Component 3 integration surface.
- Component 3 (`knowledge_reuse.rs`): `entry_meta_lookup: impl Fn(&[u64]) -> HashMap<u64, EntryMeta>` closure added as second parameter per ADR-003. Single-call semantics confirmed.
- Component 4 (`retrospective.rs`): Formatter owns all threshold language replacement via `format_claim_with_baseline`. Detection rules explicitly noted as untouched (ADR-004). All 9 threshold sites enumerated.
- Component 5 (`report.rs`): `compile_cycles` recommendation text replaced at both sites per ADR-005.
- ADR-001: `is_in_progress: Option<bool>` enforced throughout. `derive_is_in_progress` handles all three states: `None` (empty events), `Some(true)` (start, no stop), `Some(false)` (stop confirmed).
- ADR-002: `cycle_ts_to_obs_millis()` called at every phase boundary conversion (three callsites confirmed: `cycle_start`, `cycle_phase_end`, `cycle_stop` branches). No inline `* 1000` appears.
- ADR-003: `entry_meta_lookup` receives full ID slice in a single call; skipped when ID set is empty. Chunking at 100 per pattern #883 documented.
- ADR-004: Formatter pseudocode states "Detection rules are not touched." `format_claim_with_baseline` is private to `retrospective.rs`.
- ADR-005: Both `compile_cycles` sites in `report.rs` replaced with iterative-compilation framing. `permission_retries` allowlist text explicitly confirmed unchanged.
- `events` slice is borrowed (not moved): OVERVIEW.md data flow comment states "events is BORROWED from here on — ownership never moved"; both `compute_phase_stats` and `build_phase_narrative` take `&events`.
- FeatureKnowledgeReuse three construction sites all enumerated: Site 1 (`knowledge_reuse.rs` early returns), Site 2 (`types.rs` test fixtures), Site 3 (`retrospective.rs` test fixtures).

### Check 2: Specification Coverage
**Status**: WARN

**Evidence — PASS aspects**:
- FR-01 through FR-15: All functional requirements have corresponding pseudocode implementations.
- FR-11 (What Went Well): Formatter pseudocode correctly implements all 16 metrics from SPECIFICATION §FR-11 (not the 10-metric table from ARCHITECTURE.md).
- FR-12 section order: Formatter pseudocode implements the 12-section order. Sections 7–9 in the pseudocode are: `// 7. Baseline Outliers (universal — existing)`, `// 8. Findings (enhanced — FR-09/10/14)`, `// 9. Phase Outliers (existing)`. Matches SPECIFICATION §FR-12 exactly.
- FR-13 knowledge reuse: All five new fields (`total_served`, `total_stored`, `cross_feature_reuse`, `intra_cycle_reuse`, `top_cross_feature_entries`) present with correct serde attributes.
- NFR-02 (no inline `* 1000`): Explicitly enforced.
- NFR-04 (`Option<bool>` not plain `bool`): Enforced per ADR-001.
- NFR-05 (numbered section comments): Documented in formatter pseudocode.

**Issues (WARN — non-blocking)**:
- SPECIFICATION §Domain Models uses `source_cycle: String` for `EntryRef`; all pseudocode uses `feature_cycle: String`. Pseudocode flags this as GAP-3 and recommends architecture wins. Implementation agent must confirm field name before coding.
- `pass_breakdown: Vec<(u64, u64)>` appears in SPECIFICATION §Domain Models but absent from ARCHITECTURE.md and pseudocode (GAP-4). Architecture is the authoritative scope definition; this field can be omitted from the first implementation.

### Check 3: Risk Coverage
**Status**: PASS

**Evidence**:
- R-01 (inline `* 1000`): Three scenarios covered, including static grep (`test_phase_stats_static_no_inline_multiply`) and overflow guard. All R-01 strategy scenarios present.
- R-02 (phase window extraction): Four edge-case paths covered (no `cycle_phase_end`, zero-duration, empty phase name, no observations in window).
- R-03 (GateResult inference): All 8 scenarios including "compass" substring case, empty string, None, multi-keyword collision, and evaluation order (Rework > Fail > Pass).
- R-04 (batch query fewer rows): Partial lookup, all missing, empty ID set skip, call-count verification all covered.
- R-05 (`is_in_progress` three-state): All three derivation states plus serde roundtrip for None case. Formatter rendering for all three states.
- R-06 (metric direction): `test_what_went_well_direction_table_all_16_metrics` data-driven over all 16 metrics; both favorable and unfavorable directions; outlier exclusion tested.
- R-07 (section order): **Fixed.** `test_section_order` expected_order array now has `"## Outliers"` (Baseline Outliers) at position 6 and `"## Findings"` at position 7 — Baseline Outliers before Findings. This matches SPECIFICATION §FR-12 and the formatter pseudocode (`// 7. Baseline Outliers`, `// 8. Findings`). The contradiction from the initial gate is resolved.
- R-08 (threshold regex): `format_claim_with_baseline` three paths tested; threshold=0 guard; all 9 claim formats; AC-13 combined test.
- R-09 (attribution path): All three path labels tested; None case covered.
- R-10 (hotspot phase annotation): Multi-phase tie-breaking, no phase_stats, out-of-bounds timestamp all covered.
- R-11 (threshold audit snapshot): `test_threshold_language_count_snapshot` source-scan test specified.
- R-12 (`Some(vec![])` vs `None`): JSON shape difference documented and tested; handler canonicalization to `None` on empty result specified.
- R-13 (construction site migration): CI compilation gate specified.

### Check 4: Interface Consistency
**Status**: WARN

**Evidence — PASS aspects**:
- Shared types in OVERVIEW.md match per-component pseudocode: `PhaseStats`, `ToolDistribution`, `GateResult`, `EntryRef` field definitions are identical between OVERVIEW.md and retrospective-report-extensions.md.
- `FeatureKnowledgeReuse` new fields consistent across OVERVIEW.md, knowledge-reuse-extension.md, and test-plan/knowledge-reuse-extension.md.
- `compute_phase_stats` signature consistent across OVERVIEW.md, phase-stats.md, and architecture integration surface table.
- `compute_knowledge_reuse` extended signature with `current_feature_cycle: &str` and `entry_meta_lookup: G` parameters is consistent across knowledge-reuse-extension.md and its test plan.
- Attribution path labels (three string literals) are identical across phase-stats.md pseudocode and formatter-overhaul.md test plan.

**Issues (WARN — non-blocking)**:
- GAP-1: `PhaseStats` as defined in Component 1 and ARCHITECTURE.md lacks `start_ms: i64` and `end_ms: Option<i64>` fields. The formatter pseudocode `build_phase_annotation_map` function identifies this gap and documents two resolution options (add to struct, or pass events to formatter). Implementation agent must resolve before implementing `build_phase_annotation_map`. The pseudocode recommends adding `start_ms` and `end_ms` to `PhaseStats`.
- GAP-3 (EntryRef field naming): `feature_cycle` vs `source_cycle` discrepancy between architecture and specification domain models. Architecture wins; implementation agent must confirm.
- GAP-6: `cycle_ts_to_obs_millis` return type — SPECIFICATION says `-> u64`, ARCHITECTURE and col-024 implementation use `-> i64`. Implementation agent must verify actual type in `services/observation.rs` before writing code.

### Check 5: Knowledge Stewardship
**Status**: PASS

**Evidence**:
- `col-026-agent-1-pseudocode-report.md` contains `## Knowledge Stewardship` block:
  - `Queried:` entries for formatter patterns (#3426), Option<bool> pattern (#3420), domain-specific formatter pattern (#949), ADR patterns (#3421–#3425).
  - No store entry — pseudocode agent is a read-only agent per role. Appropriate.
- `col-026-agent-2-testplan-report.md` contains `## Knowledge Stewardship` block:
  - `Queried:` entries for ADR search, testing procedures, retrospective formatter testing patterns.
  - `Stored:` entry #3427 "col-026: pattern" for golden section-order test and Option-bool serde test patterns.
- Both agents follow the stewardship protocol correctly for their role type.

---

## Rework Required

None.

---

## Warnings (Non-blocking — carry forward to Gate 3b)

1. **GAP-1 (PhaseStats start_ms/end_ms)**: Implementation agent must resolve before implementing `build_phase_annotation_map`. The pseudocode flags this and recommends adding `start_ms: i64` and `end_ms: Option<i64>` to `PhaseStats`. If added, they must be populated in `compute_phase_stats` and the architecture should be updated. If not added, the fallback heuristic must be used and documented.

2. **GAP-3 (EntryRef field naming)**: Architecture (`feature_cycle`) and SPECIFICATION §Domain Models (`source_cycle`) differ. Architecture wins, but the implementation agent must confirm before coding to avoid downstream JSON consumer breakage.

3. **GAP-6 (cycle_ts_to_obs_millis return type)**: Verify `-> i64` vs `-> u64` in `services/observation.rs` before writing phase-stats code.

4. **ARCHITECTURE.md vs SPECIFICATION §FR-06 Knowledge column format**: Architecture uses `3↓ 0↑` notation; spec uses `{n} served, {n} stored`. The pseudocode adopts the architecture notation. Implementation agent must be consistent.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the section-order inversion fix (test-plan/pseudocode disagreement on Baseline Outliers vs Findings position) is a col-026-specific finding, not a generalizable pattern beyond what the gate reports already capture.
