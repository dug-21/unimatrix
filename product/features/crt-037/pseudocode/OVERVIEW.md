# crt-037 Pseudocode Overview: Informs Edge Type

## Component Map

Five components across three crates. All modifications — no new files.

| Component | File | Crate | Wave |
|-----------|------|-------|------|
| graph.rs | `crates/unimatrix-engine/src/graph.rs` | unimatrix-engine | 1 |
| config.rs | `crates/unimatrix-server/src/infra/config.rs` | unimatrix-server | 1 |
| read.rs | `crates/unimatrix-store/src/read.rs` | unimatrix-store | 1 |
| graph_ppr.rs | `crates/unimatrix-engine/src/graph_ppr.rs` | unimatrix-engine | 2 |
| nli_detection_tick.rs | `crates/unimatrix-server/src/services/nli_detection_tick.rs` | unimatrix-server | 2 |

## Wave Ordering Rationale

Wave 1 components are independent — each can compile in isolation after the change.

Wave 2 depends on Wave 1:
- `graph_ppr.rs` depends on `RelationType::Informs` being present in `graph.rs`.
- `nli_detection_tick.rs` depends on:
  - `RelationType::Informs.as_str()` from `graph.rs` (for the `"Informs"` write-path string)
  - `InferenceConfig::nli_informs_cosine_floor`, `nli_informs_ppr_weight`,
    `informs_category_pairs` from `config.rs`
  - `Store::query_existing_informs_pairs` from `read.rs`

Build order: Wave 1 components compile independently, then Wave 2 compiles against the
extended interfaces from Wave 1.

## Data Flow Between Components

```
config.rs
  |
  | informs_category_pairs (Vec<[String; 2]>)
  | nli_informs_cosine_floor (f32, default 0.45)
  | nli_informs_ppr_weight (f32, default 0.6)
  v
nli_detection_tick.rs ──Phase 2──> read.rs: query_existing_informs_pairs()
                                     returns HashSet<(u64, u64)> directional pairs
                        ──Phase 4b─> constructs NliCandidatePair::Informs { candidate, nli_scores }
                        ──Phase 8b─> calls write_nli_edge("Informs", weight, ...)
                                     weight = candidate.cosine * config.nli_informs_ppr_weight
                                     (nli_detection.rs:532 — unchanged pub(crate) fn)

graph.rs
  |
  | RelationType::Informs (new variant)
  | as_str() -> "Informs"
  | from_str("Informs") -> Some(RelationType::Informs)
  v
graph_ppr.rs ──personalized_pagerank──> 4th edges_of_type(Informs, Outgoing) call per node
             ──positive_out_degree_weight──> 4th edges_of_type(Informs, Outgoing) call
```

After `write_nli_edge` writes a row with `relation_type = "Informs"` to GRAPH_EDGES,
`build_typed_relation_graph` recognizes the string via `RelationType::from_str("Informs")`
(FR-02). On the next graph rebuild, PPR traversal picks up the edge automatically via the
fourth `edges_of_type` call in `graph_ppr.rs`.

## Shared Types Introduced

All types are module-private to `nli_detection_tick.rs`. None are exported.

### NliCandidatePair (tagged union — ADR-001)

```
enum NliCandidatePair {
    SupportsContradict {
        source_id: u64,
        target_id: u64,
        cosine: f32,
        nli_scores: NliScores,          // populated after Phase 7 score_batch
    },
    Informs {
        candidate: InformsCandidate,
        nli_scores: NliScores,          // populated after Phase 7 score_batch
    },
}
```

Derived: `Debug`, `Clone`. Module-private (no `pub`). The enum variant is the routing
discriminator. Phase 8 matches on `SupportsContradict`; Phase 8b matches on `Informs`.
Misrouting is a compile-time error. Parallel index-matched vecs are the failure mode this
type prevents (SR-08, C-11, FR-10).

### InformsCandidate (record struct — ADR-001)

```
struct InformsCandidate {
    source_id: u64,
    target_id: u64,
    cosine: f32,
    source_created_at: i64,       // Unix seconds; required — not Option
    target_created_at: i64,       // Unix seconds; required — not Option
    source_feature_cycle: String, // required — cross-feature guard; not Option
    target_feature_cycle: String, // required — cross-feature guard; not Option
    source_category: String,      // required — category pair filter; not Option
    target_category: String,      // required — category pair filter; not Option
}
```

Derived: `Debug`, `Clone`. Module-private. All nine fields are required (non-Option).
The compiler makes None-field vacuous-pass (R-05) impossible at construction time.
Phase 8b reads all guard fields directly from the struct — no unwrap, no Option handling.

### InferenceConfig new fields (in config.rs)

Three new fields added to the existing `InferenceConfig` struct:

```
informs_category_pairs: Vec<[String; 2]>   // serde default: 4 SE pairs
nli_informs_cosine_floor: f32              // serde default: 0.45
nli_informs_ppr_weight: f32               // serde default: 0.6
```

No new struct. Fields added inline following the `max_co_access_promotion_per_tick` pattern
(ADR-004 crt-034).

## Sequencing Constraints

1. Wave 1 must compile before Wave 2 begins implementation.
2. `graph.rs` must export `RelationType::Informs` before `graph_ppr.rs` adds the fourth
   `edges_of_type` call — both are in the same crate (`unimatrix-engine`), so Wave 1 and
   Wave 2 changes to `graph.rs` and `graph_ppr.rs` can be done in one PR, but `graph.rs`
   changes must land first within that PR.
3. `config.rs` and `read.rs` (Wave 1) must be complete before `nli_detection_tick.rs`
   (Wave 2) begins, because the tick function calls all three.
4. `crt-036` must merge to main before crt-037 Phase C delivery begins (C-15, logistical).

## Integration Points Not Changed

- `write_nli_edge` (`nli_detection.rs:532`) — `pub(crate)` function. Called by Phase 8b
  for `Informs` writes. No signature change. Accepts any `relation_type: &str`.
- `format_nli_metadata` — existing function. Produces `{"nli_entailment": f32,
  "nli_contradiction": f32}`. Informs edges use a new `format_nli_metadata_informs`
  variant (or inline `serde_json::json!`) that adds `"nli_neutral": nli_scores.neutral`.
  Existing function is not modified.
- `EDGE_SOURCE_NLI` constant — value `"nli"`. Used by `write_nli_edge` via the SQL bind
  (`'nli'` literal in the INSERT). No change needed.
- `current_timestamp_secs()` — `pub(crate)` helper. Reused unchanged.
- `select_source_candidates` — private helper. No change. Phase 4b uses the same
  `source_candidates` pool produced by this function.

## Penalty Invariant (SR-01, C-06, FR-13)

`graph_penalty` and `find_terminal_active` in `graph.rs` filter exclusively to
`RelationType::Supersedes` via `edges_of_type`. After adding `RelationType::Informs`,
these functions remain unchanged. `Informs` is present in the graph but invisible to
penalty traversal. The module doc comment at `graph.rs:16` must be updated to include
`Informs` in the list of non-Supersedes examples.
