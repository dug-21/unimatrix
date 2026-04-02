## ADR-002: `apply_informs_composite_guard` Simplification — Remove NLI Guards

### Context

`apply_informs_composite_guard` currently enforces five guards:

```rust
fn apply_informs_composite_guard(
    nli_scores: &NliScores,
    candidate: &InformsCandidate,
    config: &InferenceConfig,
) -> bool {
    nli_scores.neutral > 0.5                              // guard 1 — NLI neutral zone
    && candidate.source_created_at < ...                  // guard 2 — temporal
    && (cross-feature check)                              // guard 3 — feature cycle
    && nli_scores.entailment <= config.supports_edge_threshold     // guard 4 — mutual exclusion
    && nli_scores.contradiction <= config.nli_contradiction_threshold  // guard 5 — mutual exclusion
}
```

After ADR-001 (control flow split), Informs candidates never enter the NLI batch. The
function is called in Path A, which runs without calling `get_provider()` and without
`NliScores`. Guards 1, 4, and 5 reference `NliScores` and become unreachable by construction.

**Guard 1 removal** (`nli_scores.neutral > 0.5`): The neutral zone score from an SNLI-trained
cross-encoder carries no reliable signal for Unimatrix knowledge entries. ASS-035 confirmed
task mismatch. The cosine floor at Phase 4b (raised to 0.50 by crt-039) provides an
equivalent structural quality filter without relying on a model trained on sentence-pair
entailment data. Keeping guard 1 would require either retaining the NLI batch for Informs
(contradicting ADR-001) or fabricating a synthetic `NliScores` struct, both unacceptable.

**Guards 4 and 5 removal** (mutual exclusion via NLI scores): These guards existed to prevent
a pair from being written as both a Supports edge and an Informs edge. The concern is valid
but the mechanism is the wrong layer. After ADR-001, Informs and Supports candidates are
processed in separate paths: Informs candidates are collected in Phase 4b using the category
pair filter and the inclusive cosine floor; Supports candidates are collected in Phase 4 using
the strict cosine threshold and no category filter. The overlap analysis in ARCHITECTURE.md
shows the sets are disjoint by construction at the 0.50 threshold boundary. Score-based
mutual exclusion at write time is redundant with structural separation at candidate selection
time, and applying it would require passing NLI scores to a function that no longer calls
the NLI model.

**Guards 2 and 3 retention** (temporal and cross-feature): These are structurally sound,
model-free checks that enforce correct Informs edge semantics regardless of NLI availability.
They remain in `apply_informs_composite_guard` as a defense-in-depth check at write time,
even though `phase4b_candidate_passes_guards` already evaluates them during candidate
selection. Duplicate evaluation is cheap and prevents any future code path from bypassing
the candidate guard function and reaching the write function directly.

### Decision

`apply_informs_composite_guard` is simplified to:

```rust
fn apply_informs_composite_guard(
    candidate: &InformsCandidate,
) -> bool {
    candidate.source_created_at < candidate.target_created_at
        && (candidate.source_feature_cycle.is_empty()
            || candidate.target_feature_cycle.is_empty()
            || candidate.source_feature_cycle != candidate.target_feature_cycle)
}
```

- The `nli_scores: &NliScores` parameter is removed.
- The `config: &InferenceConfig` parameter is removed (no remaining guard uses config fields).
- Guards 1, 4, and 5 are removed.
- Guards 2 and 3 are retained unchanged.

All call sites of `apply_informs_composite_guard` are updated. Tests that pass `NliScores`
to this function are updated to remove that argument.

The doc comment on `apply_informs_composite_guard` is updated to describe the two retained
guards only. The "five guards" enumeration and all `FR-11` mutual-exclusion references are
removed from the comment.

`format_nli_metadata_informs` is replaced by `format_informs_metadata` that records
structural fields only (cosine similarity, category pair). The `nli_neutral`, `nli_entailment`,
and `nli_contradiction` fields in the Informs edge metadata JSON are removed. The replacement
records fields that are meaningful for the structural path (e.g., `cosine`, `source_category`,
`target_category`).

### Consequences

- `apply_informs_composite_guard` no longer requires `NliScores` — the function can be
  called from Path A without any model interaction.
- Tests `test_phase8b_no_informs_when_neutral_exactly_0_5` and
  `test_phase8b_writes_informs_when_neutral_just_above_0_5` are deleted. They tested a
  guard that no longer exists. The behavior they validated (cosine floor boundary) is already
  covered by `test_phase8b_no_informs_when_cosine_below_floor` and the new floor tests.
- Tests for `apply_informs_composite_guard` that currently pass `informs_passing_scores()`
  as the first argument must remove that argument. The `informs_passing_scores()` helper
  may be retained for Phase 8 Supports tests if it is used there, or removed entirely if
  it was only used for Informs guard tests.
- Informs edge metadata no longer includes NLI score fields. Any downstream tooling that
  reads `nli_neutral` from Informs edge metadata will see it absent after the next tick cycle.
  No currently-shipped tooling reads this field (it was an observability field only).
- Future reintroduction of a mutual-exclusion guard (e.g., based on cosine range overlap)
  can be added as a new guard at Phase 4b candidate selection time, not at write time.
