# Gate 3a Report: crt-039

> Gate: 3a (Component Design Review)
> Date: 2026-04-02
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | Components match ARCHITECTURE.md; Option Z internal split; ADR-001/002/003 reflected in all pseudocode files |
| Specification coverage | PASS | All 18 FRs and 6 NFRs have corresponding pseudocode; no scope additions |
| Risk coverage (test plans) | PASS | All 12 risks (R-01 through R-12) mapped to test scenarios; all 18 ACs addressed |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component pseudocode; data flow coherent |
| ADR-001 (Item 1): get_provider() placement | PASS | Path B entry gate correctly placed after Phase 4b and Informs write loop |
| ADR-002 (Item 2): apply_informs_composite_guard signature | PASS | Exactly 2 guards (temporal + cross-feature); single `candidate` parameter |
| AC-13/FR-06 (Item 3): Explicit Supports-set subtraction | PASS | Subtraction step present using `informs_metadata.retain(...)` after Phase 4b loop |
| AC-17 (Item 4): Phase 4b observability log fields | WARN | All four fields present but log placement description is inconsistent across two locations in nli_detection_tick.md |
| R-01 mitigation (Item 5): No path from get_provider() Err to Supports write | PASS | Structural guarantee enforced by Path B entry gate; early-return before Phase 6 |
| R-04 mitigation (Item 6): NliCandidatePair::Informs / PairOrigin::Informs removed | PASS | Both variants removed in pseudocode; Phase 6/7/8 operate on SupportsContradict only |
| Ordering invariant (Item 7): contradiction_scan BEFORE structural_graph_tick | PASS | background.md pseudocode shows correct ordering |
| TC-01 / TC-02 separation (Item 8): two separate tests | PASS | Explicitly stated as separate tests in both test plans with different corpus setups |
| TR-01/TR-02/TR-03 removals (Item 9): explicitly called out | PASS | All three listed in test-plan/OVERVIEW.md Test Removal Checklist and in nli_detection_tick.md |
| Knowledge stewardship compliance | FAIL | Architect report (`crt-039-agent-1-architect-report.md`) is missing `## Knowledge Stewardship` section entirely |

---

## Detailed Findings

### Architecture Alignment
**Status**: PASS
**Evidence**: All three components (background.rs, nli_detection_tick.rs, config.rs) are correctly identified in OVERVIEW.md. Component boundaries match ARCHITECTURE.md section "Component Breakdown." The pseudocode/OVERVIEW.md data flow diagram is consistent with the ARCHITECTURE.md control flow diagram. ADR-001 Option Z (internal two-path split) is correctly reflected as a single function with Path A / Path B in nli_detection_tick.md. ADR-002 (guard simplification) is fully reflected. ADR-003 (floor raise to 0.50) is reflected in config.md.

---

### Specification Coverage
**Status**: PASS
**Evidence**: All functional requirements have corresponding pseudocode:
- FR-01 (unconditional call): background.md removes the `if nli_enabled` wrapper
- FR-02 (Phase 4b runs without NLI): nli_detection_tick.md Path A section
- FR-03 (Phase 8 gated): nli_detection_tick.md Path B entry gate
- FR-04/FR-05 (apply_informs_composite_guard 2-guard simplification): nli_detection_tick.md "Modified Function" section
- FR-06 (explicit Supports-set subtraction): nli_detection_tick.md Phase 4b section with `informs_metadata.retain(...)` and the `supports_candidate_set` HashSet construction
- FR-07 (floor default 0.5): config.md
- FR-08 (inclusive floor semantics): nli_detection_tick.md Phase 4b comment and TC-05/TC-06
- FR-09 (MAX_INFORMS_PER_TICK hard cap; dedup ordering): nli_detection_tick.md Phase 5 section
- FR-10 (contradiction scan labeling): background.md "After (pseudocode)" block
- FR-11 (tick ordering comment): background.md includes exact comment text
- FR-12 (module-level doc comment): nli_detection_tick.md "Module-Level Doc Comment Change" section
- FR-13 (category pair via config, no domain literals): Phase 4b comment explicitly notes C-07/C-12
- FR-14 (four-field observability log): nli_detection_tick.md "Phase 4b Observability Log" section

NFR-01 through NFR-07 addressed:
- NFR-01 (no rayon in Phase 4b): Path A section states "No rayon pool usage (NFR-01)"
- NFR-02 (throughput bound): MAX_INFORMS_PER_TICK = 25 in Phase 5
- NFR-03 (W1-2 contract): Phase 7 unchanged; module doc comment includes W1-2 contract
- NFR-04 (TOML override compatibility): config.md notes TOML overrides take precedence
- NFR-05 (eval gate MRR >= 0.2913): test-plan/OVERVIEW.md AC-11
- NFR-06 (file size): OVERVIEW.md documents net-negative ~43 lines; no extraction needed
- NFR-07 (zero behavioral change to contradiction scan): background.md zero-diff constraint documented

---

### Risk Coverage
**Status**: PASS
**Evidence**: test-plan/OVERVIEW.md Risk-to-Test Mapping covers all 12 risks:
- R-01 (Critical): TC-02 integration test
- R-02 (Critical): TR-01 removal + TC-01 + TC-02
- R-03 (Critical): TC-07 + boundary variant
- R-04 (Critical): cargo build compile-time + grep
- R-05 (High): AC-11 eval gate + AC-17 log
- R-06 (High): cargo test warnings-as-errors + compile-time
- R-07 (High): TC-01 zero-Supports-candidates corpus
- R-08 (High): Clippy + TC-01 metadata assertion
- R-09 (Med): Existing contradiction scan tests + diff audit
- R-10 (Med): AC-17 grep + code ordering inspection
- R-11 (Med): Code inspection + existing tick tests
- R-12 (Low): TC-05 + TC-06

Every RISK-TEST-STRATEGY.md risk has an explicit test scenario in the test plans.

---

### Interface Consistency
**Status**: PASS
**Evidence**: OVERVIEW.md type changes table matches nli_detection_tick.md type changes exactly:
- `NliCandidatePair::Informs` removed in both documents
- `PairOrigin::Informs` removed in both documents
- `apply_informs_composite_guard` signature `(candidate: &InformsCandidate) -> bool` in both
- `format_informs_metadata(cosine: f32, source_category: &str, target_category: &str) -> String` in both
- `run_graph_inference_tick` public signature unchanged in both

The config.rs component (OVERVIEW.md data flow entry) reads `config.nli_informs_cosine_floor` in Phase 4b, consistent with config.md's documented change. The outer gate removal in OVERVIEW.md is consistent with background.md.

---

### ADR-001 (Item 1): get_provider() Placement
**Status**: PASS
**Evidence**: nli_detection_tick.md "Path B Entry Gate (R-01)" section shows:

```
// === PATH A: Structural Informs write loop ===
[...Informs write loop and observability log...]

// === PATH B entry gate ===
if candidate_pairs.is_empty() { return; }
let provider = match nli_handle.get_provider().await { ... };
```

`get_provider()` is called only at Path B entry, after Phase 4b (lines 547-553 of pseudocode) and after the Informs write loop (lines 565-578) and observability log (line 578). The complete function skeleton at the end of nli_detection_tick.md (lines 522-603) confirms this ordering unambiguously.

---

### ADR-002 (Item 2): apply_informs_composite_guard Signature
**Status**: PASS
**Evidence**: nli_detection_tick.md "Modified Function: apply_informs_composite_guard" shows:

```rust
fn apply_informs_composite_guard(candidate: &InformsCandidate) -> bool {
    candidate.source_created_at < candidate.target_created_at
        && (candidate.source_feature_cycle.is_empty()
            || candidate.target_feature_cycle.is_empty()
            || candidate.source_feature_cycle != candidate.target_feature_cycle)
}
```

Exactly 1 parameter (`candidate: &InformsCandidate`). Exactly 2 guards (temporal + cross-feature). No `nli_scores`, no `config`. Matches ADR-002 specification precisely.

---

### AC-13 / FR-06 (Item 3): Explicit Phase 4 Supports-Set Subtraction
**Status**: PASS
**Evidence**: nli_detection_tick.md Phase 4b section (lines 242-258) shows:

```rust
let supports_candidate_set: HashSet<(u64, u64)> = candidate_pairs
    .iter()
    .flat_map(|(src, tgt, _)| [(*src, *tgt), (*tgt, *src)])
    .collect();

// After loop: explicit Supports-set subtraction (R-03, FR-06, AC-13).
informs_metadata.retain(|c| {
    !supports_candidate_set.contains(&(c.source_id, c.target_id))
        && !supports_candidate_set.contains(&(c.target_id, c.source_id))
});
```

This is a real subtraction step, not reliance on threshold arithmetic. The pseudocode explicitly includes both directions in the exclusion check (safe-by-construction). The function skeleton in lines 552-554 also shows `informs_metadata.retain(|c| !supports_candidate_set.contains(...))`.

---

### AC-17 (Item 4): Phase 4b Observability Log Fields
**Status**: WARN
**Issue**: The observability log placement description is inconsistent across two locations in nli_detection_tick.md.

Location 1 (Phase 4b Observability Log section, line ~265-296):
> "After dedup (including Supports-set subtraction), before Phase 5 truncation"
> "However, found/after_dedup/after_cap can be emitted before Phase 8b if needed."
> "Preferred placement: after Phase 5 truncation, before Phase 8b writes, to match spec intent of 'Phase 4b completion' (FR-14)."

Location 2 (Path A write loop, lines 404-412):
```
// Emit Phase 4b observability log (AC-17, FR-14).
// All four values are now known.
tracing::debug!(
    informs_candidates_found,
    informs_candidates_after_dedup,
    informs_candidates_after_cap,
    informs_edges_written,
    ...
```

The final function skeleton (line 578) also places the log after the Informs write loop (post-Phase 8b).

All four required fields are present (`informs_candidates_found`, `informs_candidates_after_dedup`, `informs_candidates_after_cap`, `informs_edges_written`). The inconsistency is whether the log is emitted before or after Phase 8b completes. The canonical placement in the function skeleton (after Phase 8b, all four values known) is correct and satisfies AC-17. The WARN is that the Phase 4b section's "preferred placement: before Phase 8b writes" conflicts with the skeleton, which may mislead the implementor.

This is a WARN, not a FAIL — the correct placement is clear from the skeleton, and all four required fields are present.

---

### R-01 Mitigation (Item 5): No Supports Write Without Provider
**Status**: PASS
**Evidence**: Path B entry gate in nli_detection_tick.md (lines 422-442):

```
let provider = match nli_handle.get_provider().await {
    Ok(p) => p,
    Err(_) => {
        tracing::debug!("graph inference tick: NLI provider not ready; Supports path skipped");
        return;
    }
};
```

This `return` on `Err` is positioned before Phase 6 (text fetch) and Phase 7/8 (NLI batch + Supports writes). No code path exists from `Err` to `write_nli_edge(..., "Supports", ...)`. The RISK-TEST-STRATEGY.md "Critical invariant for SR-04" (ARCHITECTURE.md) confirms: "Phase 4b has already written its edges and returned normally through Path A before this guard fires."

---

### R-04 Mitigation (Item 6): Dead Enum Variants Removed
**Status**: PASS
**Evidence**: nli_detection_tick.md "Type Changes" section documents removal of both variants:
- `NliCandidatePair::Informs { candidate: InformsCandidate, nli_scores: NliScores }` — removed
- `PairOrigin::Informs(InformsCandidate)` — removed

Phase 6 pseudocode (line ~445-483) shows `pair_origins` built exclusively from `PairOrigin::SupportsContradict { ... }`. Phase 7 match arm comment (line ~487-489) confirms: "The `.map(...)` in Phase 7 has only one arm: `PairOrigin::SupportsContradict { ... } => NliCandidatePair::SupportsContradict { ... }`. No `PairOrigin::Informs` arm." The Phase 8 loop uses `if let NliCandidatePair::SupportsContradict { ... } = pair` — exhaustive without wildcard.

---

### Ordering Invariant (Item 7): contradiction_scan BEFORE structural_graph_tick
**Status**: PASS
**Evidence**: background.md "After (pseudocode)" block shows:

```
// --- Contradiction scan (independent tick step) ---
if current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS) {
    if let Ok(adapter) = embed_service.get_adapter().await { ... }
}

// 2. Extraction tick (with timeout, #236)
match tokio::time::timeout(TICK_TIMEOUT, extraction_tick(...)).await { ... }

// --- Structural graph tick (always) ---
run_graph_inference_tick(...).await;
```

Contradiction scan appears before structural_graph_tick. The ordering invariant comment explicitly states: "contradiction_scan ... → extraction_tick → structural_graph_tick (always)." Consistent with FR-11, AC-07, and ARCHITECTURE.md ordering invariant.

---

### TC-01 and TC-02 Separation (Item 8)
**Status**: PASS
**Evidence**: test-plan/nli_detection_tick.md TC-01 and TC-02 are documented as distinct tests with:
- Different test names: `test_phase4b_writes_informs_when_nli_not_ready` vs `test_phase8_no_supports_when_nli_not_ready`
- Different corpus setups: TC-01 uses no Supports candidates (no pair above supports_candidate_threshold); TC-02 uses pairs above supports_candidate_threshold
- Explicit note in TC-01: "TC-01 is separate from TC-02 — do not combine into a single `#[tokio::test]`"
- test-plan/OVERVIEW.md: "Two independent assertions, not one combined test"

The R-02 coverage requirement from RISK-TEST-STRATEGY.md is satisfied.

---

### TR-01/TR-02/TR-03 Removals (Item 9)
**Status**: PASS
**Evidence**: test-plan/OVERVIEW.md "Test Removal Checklist" explicitly names all three:

| TR | Test Name | Reason |
|----|-----------|--------|
| TR-01 | `test_run_graph_inference_tick_nli_not_ready_no_op` | Asserts old no-op semantics |
| TR-02 | `test_phase8b_no_informs_when_neutral_exactly_0_5` | Tests removed neutral guard |
| TR-03 | `test_phase8b_writes_informs_when_neutral_just_above_0_5` | Tests removed neutral guard |

test-plan/nli_detection_tick.md "Tests to Remove (TR)" section lists the same three with matching names and reasons. The test-plan/OVERVIEW.md provides the exact grep command for gate-3c verification. All three removals are also called out in ARCHITECTURE.md's "Test Impact Summary."

---

### Knowledge Stewardship Compliance
**Status**: FAIL
**Evidence**: The architect agent report at `product/features/crt-039/agents/crt-039-agent-1-architect-report.md` contains no `## Knowledge Stewardship` section. The file ends at line 53 with the "Open Questions" section. No `Stored:`, `Declined:`, or `Queried:` entries are present anywhere in the file.

By contrast:
- `crt-039-agent-1-pseudocode-report.md` has a `## Knowledge Stewardship` section with Queried entries
- `crt-039-agent-2-testplan-report.md` has a `## Knowledge Stewardship` section with Queried entries
- `crt-039-agent-3-risk-report.md`, `crt-039-agent-2-spec-report.md`, `crt-039-agent-0-scope-risk-report.md` all have the section

The architect is an active-storage agent — it stores ADRs to Unimatrix (and did: #4017, #4018, #4019 are referenced in the report body). However, the required `## Knowledge Stewardship` block with `Stored:` entries for those three ADRs is absent from the report. This is a required stewardship report block, not optional.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Architect report missing `## Knowledge Stewardship` section | `crt-039-agent-1-architect` (or coordinator on its behalf) | Add `## Knowledge Stewardship` block to `product/features/crt-039/agents/crt-039-agent-1-architect-report.md` with `Stored:` entries for ADR-001 (#4017), ADR-002 (#4018), ADR-003 (#4019), and any Queried entries from the design phase. Format: `- Stored: entry #4017 "ADR-001 ..." via /uni-store-adr` |

---

## Notes for Implementor

The WARN on AC-17 log placement is informational only. The function skeleton at lines 522-603 of `pseudocode/nli_detection_tick.md` is the authoritative placement spec: the `tracing::debug!` call goes after the Phase 8b write loop (all four values known). The "Preferred placement: before Phase 8b writes" phrasing in the Phase 4b section should be treated as superseded by the skeleton.

The TC-07 boundary variant (`informs_metadata` retains the pair at exactly cosine 0.50 that was NOT in candidate_pairs) is correctly documented in the test plan but should note that the `make_informs_candidate` helper assigns `source_id` — the test plan at lines 250-258 of test-plan/nli_detection_tick.md does not set explicit source_id/target_id for the boundary pair, which means the implementor will need to ensure the boundary pair does NOT collide with the IDs used in the `candidate_pairs` HashSet. This is a minor implementation-phase concern, not a gate issue.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the architect stewardship omission pattern (agent report missing block when ADRs are clearly stored) may be a recurring pattern, but this is the first observed instance for this gate; a second instance across features is needed before storing as a lesson. The FAIL is logged in this gate report which serves as the record.
