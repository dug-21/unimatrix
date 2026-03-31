# Agent Report: 469-agent-1-fix

**Feature:** bugfix-469
**Issue:** GH #469 — [crt-037] Relax feature_cycle attribution requirement in Informs candidate guard

## Summary

Fixed three guard sites in `nli_detection_tick.rs` that incorrectly blocked any Informs candidate where either entry lacked a `feature_cycle`. The correct semantic is: block only intra-feature pairs (both non-empty AND equal). Entries with empty feature_cycle have unknown provenance and are valid Informs candidates.

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/services/nli_detection_tick.rs`

## Changes Made

**Site 1 (~line 286 after edit) — Phase 4b source pre-filter loop:**
Removed the `if source_meta.feature_cycle.is_empty() { continue; }` block entirely. This was a premature pre-filter encoding wrong semantics.

**Site 2 (~line 762) — `phase4b_candidate_passes_guards`:**
Replaced two sequential `is_empty()`/equality checks with a single unified predicate:
```rust
// Block intra-feature pairs only — empty feature_cycle means unknown provenance, allow it.
if !source_feature_cycle.is_empty()
    && !target_feature_cycle.is_empty()
    && source_feature_cycle == target_feature_cycle
{
    return false;
}
```

**Site 3 (~line 800) — `apply_informs_composite_guard`:**
Replaced `candidate.source_feature_cycle != candidate.target_feature_cycle` with:
```rust
(candidate.source_feature_cycle.is_empty()
    || candidate.target_feature_cycle.is_empty()
    || candidate.source_feature_cycle != candidate.target_feature_cycle)
```
This handles the newly-reachable both-empty path after Sites 1 and 2 relaxation.

**Comment updates:**
- `InformsCandidate` struct field comments updated from "required — cross-feature guard; not Option" to "empty string means pre-attribution entry; Informs detection allows this"
- `phase4b_candidate_passes_guards` doc comment AC-15 updated from "both non-empty" to "block only when both feature_cycles are non-empty AND equal (intra-feature)"
- `apply_informs_composite_guard` doc guard 3 updated to describe the relaxed semantics

## New Tests

1. `test_phase4b_accepts_source_with_empty_feature_cycle` — source="" target="crt-037" → passes guard
2. `test_phase4b_accepts_target_with_empty_feature_cycle` — source="crt-037" target="" → passes guard
3. `test_phase4b_accepts_both_empty_feature_cycle` — source="" target="" → passes guard (newly-reachable path)
4. `test_apply_informs_composite_guard_both_empty_passes` — both empty at Site 3 → passes guard

Existing test `test_phase8b_no_informs_when_same_feature_cycle` (same non-empty cycle) preserved and still passes.

## Tests

- nli_detection_tick module: **52 passed, 0 failed**
- Full suite: 2583 passed, 1 failed (`col018_topic_signal_from_file_path` — pre-existing flaky test unrelated to this fix; fails due to embedding model initialization race in concurrent test run)

## Issues / Blockers

None. Pre-existing clippy warnings in `unimatrix-engine/src/auth.rs` (collapsible_if) not introduced by this fix.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — Entry #3957 returned directly, documenting this exact bug pattern: "Cross-feature guard over-restriction: is_empty() OR blocks entries with unknown provenance". The lesson was already recorded, confirming the fix approach.
- Stored: nothing novel to store — entry #3957 already captures this pattern in full, including the three-site manifestation, the root cause (conflating intra-feature suppression with attribution completeness), and the prevention test matrix (same non-empty/different non-empty/one-empty + one non-empty).
