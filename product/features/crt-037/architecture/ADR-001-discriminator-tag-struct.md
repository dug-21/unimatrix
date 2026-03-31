## ADR-001: Discriminator Tag Struct for Merged NLI Batch

### Context

SCOPE.md OQ-2 resolved that the Supports/Contradicts and Informs candidate pairs must be
merged into a single Phase 7 rayon batch to maintain the W1-2 contract (one `rayon_pool.spawn`
per tick). After scoring, Phase 8 writes Supports edges and Phase 8b writes Informs edges.

Two options for carrying origin and per-pair guard metadata through the batch:

1. **Parallel index-matched vecs**: one `Vec<(u64, u64, f32)>` for all pairs and a separate
   `Vec<PairOrigin>` + several additional `Vec<T>` for guard data (feature_cycles, timestamps,
   categories). Phase 8/8b index into these by position.

2. **Single discriminator struct**: one `Vec<NliCandidatePair>` where each element carries
   its `origin` tag plus all guard metadata as named fields.

SR-08 (SCOPE-RISK-ASSESSMENT.md) identifies option 1 as the failure mode: if the tag routing
in Phase 8/8b diverges from how Phase 4b attaches metadata, Informs pairs may be silently
routed to the Supports write path and dropped. Index misalignment is a compile-invisible bug
that can occur any time the vec construction and the write loop are maintained separately.

The existing `scored_input: Vec<(u64, u64, String, String)>` in the current tick is a
precedent for using a single struct-like tuple — but it lacks a discriminator, and adding a
fifth field (origin) while keeping others as a parallel vec is the indexed-misalignment trap.

### Decision

Define two module-private types in `nli_detection_tick.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PairOrigin {
    SupportsContradict,
    Informs,
}

#[derive(Debug, Clone)]
struct NliCandidatePair {
    source_id: u64,
    target_id: u64,
    similarity: f32,
    origin: PairOrigin,
    // Informs-only guard data (None for SupportsContradict pairs):
    source_category: Option<String>,
    target_category: Option<String>,
    source_feature_cycle: Option<String>,
    target_feature_cycle: Option<String>,
    source_created_at: Option<i64>,
    target_created_at: Option<i64>,
}
```

The merged batch for Phase 6 and Phase 7 is `Vec<NliCandidatePair>`. After scoring, the
`Vec<NliScores>` is index-aligned to this vec (one `NliScores` per `NliCandidatePair`, in
order). The length-mismatch guard already present after Phase 7 (line 267 in
`nli_detection_tick.rs`) applies to the merged vec identically.

Phase 8 iterates all pairs, processes those with `origin == SupportsContradict`, skips
`Informs`. Phase 8b iterates all pairs, processes those with `origin == Informs`, skips
`SupportsContradict`. This is a simple match arm per iteration — no separate loop, no
second index.

The existing `write_inferred_edges_with_cap` function takes `pairs: &[(u64, u64)]`. To
avoid changing its signature, Phase 8 builds a temporary `write_pairs` slice on the fly
(as the current code already does at line 288), but filtered to `SupportsContradict` only.
Alternatively, `write_inferred_edges_with_cap` can be refactored to accept
`&[NliCandidatePair]` — this is an implementation agent decision. The discriminator struct
interface is the invariant; the write helper's internal signature is not.

Phase 8b does NOT use `write_inferred_edges_with_cap`. It calls `write_nli_edge` directly
(as `pub(crate)`) for each qualifying Informs pair, applying the composite guard inline.

### Consequences

- SR-08 misrouting is eliminated. The origin field is co-located with source/target IDs on
  the same struct; Phase 4b cannot attach metadata to one vec while indexing from another.
- Guard data for Phase 8b (feature_cycles, timestamps, categories) is carried as named
  `Option<T>` fields on `NliCandidatePair`. Accessing `pair.source_feature_cycle` at write
  time is self-documenting; accessing `feature_cycle_vec[i]` is not.
- `NliCandidatePair` for `SupportsContradict` origin carries `None` for all Informs-only
  fields. This is a small allocation cost — accepted because the cap bounds the vec size to
  at most `max_graph_inference_per_tick` (default 100) entries.
- The existing length-mismatch guard between `nli_scores.len()` and `scored_input.len()`
  is preserved verbatim. The only change is that `scored_input` becomes `Vec<NliCandidatePair>`.
- Tests for Phase 8b routing must explicitly verify that an Informs pair with
  `origin == SupportsContradict` is not written as an Informs edge, and vice versa.
