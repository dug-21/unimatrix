## ADR-001: NliCandidatePair Tagged Union for Merged NLI Batch

### Context

SCOPE.md OQ-2 resolved that the Supports/Contradicts and Informs candidate pairs must be
merged into a single Phase 7 rayon batch to maintain the W1-2 contract (one `rayon_pool.spawn`
per tick). After scoring, Phase 8 writes Supports edges and Phase 8b writes Informs edges.

Three options for carrying origin and per-pair guard metadata through the batch were
considered:

1. **Parallel index-matched vecs**: one `Vec<(u64, u64, f32)>` for all pairs and a separate
   `Vec<PairOrigin>` + several additional `Vec<T>` for guard data (feature_cycles, timestamps,
   categories). Phase 8/8b index into these by position.

2. **Flat discriminator struct with `Option<*>` fields**: one `Vec<NliCandidatePair>` where
   each element carries an `origin: PairOrigin` tag plus all guard metadata as `Option<T>`
   named fields (None for SupportsContradict pairs, Some for Informs pairs).

3. **Tagged union (Rust enum)**: `NliCandidatePair` is itself an enum where each variant
   carries exactly the fields required for its write path — no `Option` fields, no
   discriminator tag needed.

SR-08 (SCOPE-RISK-ASSESSMENT.md) identifies option 1 as the failure mode: index misalignment
is a compile-invisible bug. Option 2 improves on 1 by co-locating all data, but retains a
runtime risk: an `Informs`-origin pair can have `source_created_at: None` — Phase 8b must
then handle that case defensively or risk a panic. If a guard is omitted, the pair silently
passes with a vacuous check (R-05 vacuous-pass risk).

Option 3 eliminates this at the type level. When the struct is a tagged union, an `Informs`
variant carries all guard fields as non-`Option` values — the compiler enforces that they
are present. Phase 8b cannot accidentally skip a guard because the fields are always populated.
Misrouting between Phase 8 and Phase 8b is a compile error, not a runtime branch.

A design review pass after initial architecture identified option 2 (flat struct) in the
original ADR as inconsistent with FR-10 in the specification, which specifies a tagged union.
This corrected ADR adopts the spec model.

### Decision

Define `NliCandidatePair` as a module-private Rust enum in `nli_detection_tick.rs`:

```rust
/// Tagged union carrying per-pair metadata from Phase 4/4b through Phase 7 to Phase 8/8b.
///
/// The enum variant is the routing discriminator. Phase 8 pattern-matches on
/// SupportsContradict; Phase 8b pattern-matches on Informs. Misrouting is a compile error,
/// not a runtime branch. SR-08: parallel index-matched vecs are the failure mode this
/// type prevents.
#[derive(Debug, Clone)]
enum NliCandidatePair {
    SupportsContradict {
        source_id: u64,
        target_id: u64,
        cosine: f32,
        nli_scores: NliScores,
    },
    Informs {
        candidate: InformsCandidate,
        nli_scores: NliScores,
    },
}

/// Carries all Phase 4b guard metadata for an Informs candidate.
/// All fields are required (non-Option): the compiler guarantees Phase 8b has
/// everything it needs without defensive None checks.
#[derive(Debug, Clone)]
struct InformsCandidate {
    source_id: u64,
    target_id: u64,
    cosine: f32,
    source_created_at: i64,         // Unix timestamp seconds; required for temporal guard
    target_created_at: i64,
    source_feature_cycle: String,   // required for cross-feature guard
    target_feature_cycle: String,
    source_category: String,        // required for category re-verification at write time
    target_category: String,
}
```

The merged batch for Phase 6 and Phase 7 is `Vec<NliCandidatePair>`. `NliScores` is embedded
in each variant directly rather than maintained as a parallel vec. After Phase 7 `score_batch`
returns, scores are inserted into the constructed pairs. The length-mismatch guard present
after Phase 7 continues to apply during score insertion.

Phase 8 pattern-matches on `NliCandidatePair::SupportsContradict`, ignores `Informs` arms.
Phase 8b pattern-matches on `NliCandidatePair::Informs { candidate, nli_scores }` — all
guard fields destructure directly from `candidate` with no unwrap, no Option handling.

Phase 8b does NOT use `write_inferred_edges_with_cap`. It calls `write_nli_edge` directly
(as `pub(crate)`) for each qualifying Informs pair, applying the composite guard inline.

### Consequences

- SR-08 misrouting is a compile error. Accessing `SupportsContradict` fields in Phase 8b
  code is a type error — the compiler rejects it.
- R-05 vacuous-pass risk is eliminated. Phase 8b guard fields (`source_created_at`,
  `source_feature_cycle`, etc.) are non-Option on `InformsCandidate`; there is no path
  where a guard is skipped because a field was None.
- Phase 4b must populate all `InformsCandidate` fields before appending to the candidate
  vec. This is an intentional constraint: incompletely specified pairs cannot be constructed.
- The `NliScores` embedding in each variant removes the need for a parallel `Vec<NliScores>`
  and eliminates the separate length-mismatch guard. Score insertion becomes a zip operation.
- The existing length-mismatch guard between raw `score_batch` output and the input text vec
  is preserved; only the downstream parallel-scores vec is removed.
- Tests for Phase 8b routing must explicitly verify that a `SupportsContradict` variant is
  not written as an Informs edge, and vice versa. Pattern exhaustiveness is enforced by the
  compiler, so missing arms are caught at build time.
