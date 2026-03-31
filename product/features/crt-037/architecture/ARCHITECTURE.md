# crt-037: Informs Edge Type — Architecture

## System Overview

crt-037 adds `RelationType::Informs` as a new positive edge type in the Unimatrix
TypedRelationGraph. `Informs` edges connect empirical knowledge entries (lesson-learned,
pattern) created in earlier feature cycles to normative knowledge entries (decision,
convention) created later. This bridges cross-feature institutional memory into PPR
traversal — surfacing lessons when decisions are queried.

Three crates are touched:

| Crate | Files Modified | Nature of Change |
|---|---|---|
| `unimatrix-engine` | `graph.rs`, `graph_ppr.rs` | Pure: new enum variant, fourth PPR edge-type call |
| `unimatrix-server` | `nli_detection_tick.rs`, `config.rs` | Detection: Phase 4b scan, Phase 8b write, 3 new config fields |
| `unimatrix-store` | `read.rs` | New query: `query_existing_informs_pairs` |

No schema migration. `GRAPH_EDGES.relation_type` is a free-text column; `"Informs"` is
stored as a string. No new ML model. The existing `NliServiceHandle` / `CrossEncoderProvider`
is reused. No new tick infrastructure. All detection runs inside `run_graph_inference_tick`.

---

## Component Breakdown

### A. RelationType Enum Extension (unimatrix-engine/src/graph.rs)

**Responsibility:** Extend `RelationType` with a sixth variant and update the two string
conversion methods.

**Boundary:** Pure, no I/O. Changes are additive: `as_str()` and `from_str()` get a new
arm each. `graph_penalty` and `find_terminal_active` are unchanged — both filter exclusively
to `Supersedes` edges via `edges_of_type`. `Informs` is not visible to penalty logic
(SR-01 / AC-24).

The module doc comment at `graph.rs:16` lists `Supports`, `CoAccess`, `Prerequisite` as
non-Supersedes examples; it must be updated to include `Informs`.

### B. PPR Traversal Extension (unimatrix-engine/src/graph_ppr.rs)

**Responsibility:** Include `Informs` edges in the reverse random walk so that lesson nodes
gain PPR mass when a decision node is seeded.

**Boundary:** Pure, no I/O. Follows the exact three-call pattern already in
`personalized_pagerank` and `positive_out_degree_weight`: add a fourth
`edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing)` call in both
functions. The AC-02 boundary (`edges_of_type` exclusively, no `.edges_directed()`) is
preserved.

PPR direction semantics are unchanged. `Direction::Outgoing` implements the reverse random
walk (transpose PPR): for an edge A→B (Informs: lesson A informs decision B), node A
accumulates mass from B's score when B is seeded. See ADR-001 (SR-07 resolution).

### C. InferenceConfig Extension (unimatrix-server/src/infra/config.rs)

**Responsibility:** Expose three new fields controlling the Informs detection pass. All
domain vocabulary (category pair strings) lives exclusively here — never in detection logic.

**New fields:**

```rust
/// Category pairs eligible for Informs detection.
/// Each pair [lhs, rhs] means an entry with category `lhs` can Inform
/// an entry with category `rhs`. Only config code may contain these strings;
/// detection logic receives them as runtime values (AC-22 / domain agnosticism).
///
/// Default: four software-engineering pairs (frozen at v1, see ADR-001 crt-037).
/// Valid: non-empty Vec; each element a [String; 2] array.
#[serde(default = "default_informs_category_pairs")]
pub informs_category_pairs: Vec<[String; 2]>,

/// HNSW cosine similarity floor for Informs candidate pre-filter.
/// Pairs with similarity < nli_informs_cosine_floor are excluded before NLI scoring.
/// Distinct from supports_candidate_threshold (0.50) — captures the 0.45–0.50 band
/// that contains semantically related cross-category pairs invisible to Supports path.
///
/// Default: 0.45. Range: (0.0, 1.0) exclusive.
#[serde(default = "default_nli_informs_cosine_floor")]
pub nli_informs_cosine_floor: f32,

/// Edge weight multiplier for Informs edges.
/// weight = similarity * nli_informs_ppr_weight (both f32).
/// Separate from Supports edge weight to allow per-deployment tuning of how strongly
/// institutional memory influences PPR traversal.
///
/// Default: 0.6. Range: [0.0, 1.0] inclusive.
#[serde(default = "default_nli_informs_ppr_weight")]
pub nli_informs_ppr_weight: f32,
```

**Default value functions:**

```rust
fn default_informs_category_pairs() -> Vec<[String; 2]> {
    vec![
        ["lesson-learned".to_string(), "decision".to_string()],
        ["lesson-learned".to_string(), "convention".to_string()],
        ["pattern".to_string(), "decision".to_string()],
        ["pattern".to_string(), "convention".to_string()],
    ]
}

fn default_nli_informs_cosine_floor() -> f32 { 0.45 }
fn default_nli_informs_ppr_weight() -> f32 { 0.6 }
```

**Validation additions to `validate()`:**

```rust
// nli_informs_cosine_floor: (0.0, 1.0) exclusive
if self.nli_informs_cosine_floor <= 0.0 || self.nli_informs_cosine_floor >= 1.0 {
    return Err(ConfigError::NliFieldOutOfRange { field: "nli_informs_cosine_floor", ... });
}

// nli_informs_ppr_weight: [0.0, 1.0] inclusive
if self.nli_informs_ppr_weight < 0.0 || self.nli_informs_ppr_weight > 1.0 {
    return Err(ConfigError::NliFieldOutOfRange { field: "nli_informs_ppr_weight", ... });
}
```

No cross-field invariant with `supports_candidate_threshold`. The Informs HNSW scan is
a separate scan at a separate threshold; the `candidate < edge` invariant does not apply.

### D. NliCandidatePair Tagged Union (unimatrix-server/src/services/nli_detection_tick.rs)

**Responsibility:** Carry per-pair metadata through the merged Phase 7 rayon batch so that
Phase 8 and Phase 8b can route correctly without index-matching parallel vectors (SR-08).
The enum variant is the routing discriminator; misrouting between Phase 8 and Phase 8b is a
compile error, not a runtime branch.

**Type definitions (in nli_detection_tick.rs, module-private):**

```rust
/// Tagged union carrying per-pair metadata from Phase 4/4b through Phase 7 to Phase 8/8b.
///
/// The enum variant is the routing discriminator. Phase 8 pattern-matches on
/// SupportsContradict; Phase 8b pattern-matches on Informs. Misrouting is a compile error.
///
/// SR-08: parallel index-matched vecs are the failure mode this type prevents.
/// FR-10: spec requires a tagged union so misrouting is a compile-time error.
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
/// everything it needs without defensive None checks. This eliminates the R-05
/// vacuous-pass risk present in a flat struct with Option<*> guard fields.
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

**Routing invariant:** `NliScores` is embedded in each variant. After Phase 7 `score_batch`
returns, scores are inserted into the constructed pairs via a zip. Phase 8 pattern-matches on
`NliCandidatePair::SupportsContradict`, applying `entailment > supports_edge_threshold`.
Phase 8b pattern-matches on `NliCandidatePair::Informs { candidate, nli_scores }`,
destructuring all guard fields directly from `candidate` — no unwrap, no Option handling.
The compiler enforces exhaustiveness at both match sites.

### E. Store Query: query_existing_informs_pairs (unimatrix-store/src/read.rs)

**Responsibility:** Pre-filter dedup for the Informs pass, mirroring
`query_existing_supports_pairs`.

**Signature:**

```rust
pub async fn query_existing_informs_pairs(&self) -> Result<HashSet<(u64, u64)>>
```

**SQL:** Directional — returns `(source_id, target_id)` without normalization:

```sql
SELECT source_id, target_id
FROM graph_edges
WHERE relation_type = 'Informs' AND bootstrap_only = 0
```

**Key difference from `query_existing_supports_pairs`:** Returns directional pairs as
`(source_id, target_id)` without min/max normalization. The temporal ordering guard
(`source.created_at < target.created_at`) means the reverse edge would never pass
detection. Symmetric dedup is unnecessary and could suppress valid edges (ADR-003).

The `INSERT OR IGNORE` backstop on the `UNIQUE(source_id, target_id, relation_type)` index
still applies as secondary protection.

---

## Component Interactions

```
run_graph_inference_tick (nli_detection_tick.rs)
  |
  ├─ Phase 2: query_by_status, query_entries_without_edges,
  |           query_existing_supports_pairs (unchanged),
  |           query_existing_informs_pairs (NEW)
  |
  ├─ Phase 3: select_source_candidates (unchanged — metadata-only cap)
  |
  ├─ Phase 4: HNSW scan @ supports_candidate_threshold (0.50)
  |           → NliCandidatePair { origin: SupportsContradict, ... }
  |
  ├─ Phase 4b (NEW): HNSW scan @ nli_informs_cosine_floor (0.45)
  |           cross-category filter via informs_category_pairs
  |           temporal guard (source.created_at < target.created_at)
  |           cross-feature guard (source.feature_cycle != target.feature_cycle)
  |           existing_informs_pairs pre-filter
  |           → NliCandidatePair { origin: Informs, source_category, feature_cycles, timestamps }
  |
  ├─ Phase 5: merged sort + cap truncation (Supports first, Informs second — ADR-002)
  |
  ├─ Phase 6: text fetch for all pairs in merged vec
  |
  ├─ Phase 7: single rayon spawn → score_batch on all pairs (W1-2)
  |           result: Vec<NliScores> index-aligned to Vec<NliCandidatePair>
  |
  ├─ Phase 8: iterate pairs where origin == SupportsContradict
  |           write Supports edge if entailment > supports_edge_threshold (unchanged)
  |
  └─ Phase 8b (NEW): iterate pairs where origin == Informs
              composite guard: neutral > 0.5 AND guards from pair metadata
              write Informs edge via write_nli_edge(source, target, "Informs",
                similarity * nli_informs_ppr_weight, ...)
```

---

## Phase 4b: Informs HNSW Scan Flow

Phase 4b runs after Phase 4 completes. It uses the same `source_candidates` pool
(already capped at `max_graph_inference_per_tick`).

**Algorithm:**

1. Build `informs_lhs_set: HashSet<&str>` from the left-hand sides of
   `config.informs_category_pairs` — used for O(1) source-category check.

2. Build `entry_meta: HashMap<u64, &EntryRecord>` from `all_active` for O(1) lookup of
   `created_at`, `feature_cycle`, and `category` of any entry ID.

3. For each `source_id` in `source_candidates`:
   - Look up `source_meta` from `entry_meta`. Skip if absent.
   - Skip if `source_meta.category` is not in `informs_lhs_set` (domain agnosticism: only
     configured left-hand categories are Informs sources).
   - Get embedding. Skip if absent (same as Phase 4 pattern).
   - HNSW search with `config.graph_inference_k` neighbors and EF_SEARCH=32.
   - For each neighbor result where `similarity >= config.nli_informs_cosine_floor`:
     - Skip self.
     - Look up `target_meta` from `entry_meta`. Skip if absent.
     - **Cross-category filter:** verify `[source_category, target_category]` is in
       `config.informs_category_pairs`. Skip if not.
     - **Temporal guard:** `source_meta.created_at < target_meta.created_at`. Skip if equal
       or reversed. This is the directional ordering that makes Informs meaningful: source
       was written before target.
     - **Cross-feature guard:** `source_meta.feature_cycle != target_meta.feature_cycle`.
       Skip if same feature cycle.
     - **Dedup:** check `existing_informs_pairs.contains(&(source_id, neighbor_id))`. Skip
       if already written.
     - **In-tick dedup:** check `seen_informs_pairs` (a `HashSet<(u64,u64)>` local to
       Phase 4b). Skip if already seen this tick. Insert on first encounter.
     - Append `NliCandidatePair::Informs { candidate: InformsCandidate { source_id,
       target_id: neighbor_id, cosine: similarity, source_created_at, target_created_at,
       source_feature_cycle, target_feature_cycle, source_category, target_category },
       nli_scores: /* populated after Phase 7 */ }` to `informs_candidate_pairs`.
       All `InformsCandidate` fields are required; the struct cannot be constructed with
       missing guard data.

4. The `informs_candidate_pairs` vec is **not** separately capped before Phase 5. The
   combined sort + truncation in Phase 5 applies the single `max_graph_inference_per_tick`
   cap across both lists, with Supports-first priority (ADR-002).

**Cosine comparison:** Phase 4 uses strict `>` (`similarity <= threshold` skips). Phase 4b
uses `>=` (`similarity < threshold` skips) because the 0.45 floor is inclusive — a pair
at exactly 0.45 is a valid Informs candidate. This matches AC-17 and AC-18.

---

## Phase 5: Combined Cap with Priority Ordering

After Phase 4 and Phase 4b, two candidate vecs exist:
- `supports_pairs: Vec<NliCandidatePair>` (origin = SupportsContradict)
- `informs_pairs: Vec<NliCandidatePair>` (origin = Informs)

**Priority algorithm:**

```
Step 1: Sort supports_pairs by existing Phase 5 criteria
        (cross-category first, isolated endpoint second, similarity desc).
        Truncate to max_graph_inference_per_tick.

Step 2: Compute remaining_capacity = max_graph_inference_per_tick - supports_pairs.len().

Step 3: Sort informs_pairs by similarity desc (cross-category is already guaranteed by
        Phase 4b filter; isolated-endpoint boost not applied to Informs pass).

Step 4: Truncate informs_pairs to remaining_capacity.

Step 5: merged_pairs = supports_pairs + informs_pairs (concatenate, order preserved).
```

This guarantees Supports/Contradicts detection is never starved by an Informs candidate
flood. In ticks where `supports_pairs.len() >= max_graph_inference_per_tick`, no Informs
pairs are scored. The count of Informs candidates that were cap-dropped must be logged
at debug level (SR-03 observability):

```rust
tracing::debug!(
    supports_candidates = supports_pairs.len(),
    informs_candidates_total = informs_pairs_before_cap.len(),
    informs_candidates_accepted = informs_pairs.len(),
    informs_candidates_dropped = informs_pairs_before_cap.len().saturating_sub(informs_pairs.len()),
    "graph inference tick: merged cap accounting"
);
```

---

## Phase 8b: Informs Write Loop

Phase 8b runs after Phase 8 completes, iterating the same `merged_pairs` / `nli_scores`
index-aligned vecs.

**Composite guard (SR-01):** A single predicate is not sufficient. All of the following
must hold before writing an Informs edge. With `NliCandidatePair` as a tagged union, guard
fields 3–5 destructure directly from `InformsCandidate` — no unwrap, no Option handling,
no R-05 vacuous-pass risk:

1. Pattern match on `NliCandidatePair::Informs { candidate, nli_scores }` (routing is
   enforced by the compiler; no explicit discriminator check needed)
2. `nli_scores.neutral > 0.5` (fixed neutral floor — model property, not domain tunable;
   see OQ-S2 resolution below)
3. `candidate.source_created_at < candidate.target_created_at` (temporal ordering
   re-verified from struct fields — not re-queried from DB)
4. `candidate.source_feature_cycle != candidate.target_feature_cycle` (cross-feature
   re-verified)
5. Category pair membership was verified in Phase 4b and is implicit in the `Informs`
   variant. No re-query needed.

**OQ-S2 resolution — neutral threshold calibration:** `NliScores.neutral` is a true
3-class softmax output. The model produces three class scores (entailment, neutral,
contradiction) that sum to 1.0 (invariant documented in `cross_encoder.rs`). `neutral`
is therefore a first-class probability, not a residual. A threshold of `neutral > 0.5`
means the model assigns more than 50% probability to the "unrelated" class — a strict
criterion indicating the pair lacks a meaningful directional relationship. This is the
correct calibration for the Informs guard: pairs that are semantically proximate (cosine
>= 0.45) but where the model is confident they are unrelated should not produce edges.
The 0.5 floor is not arbitrary; it is the majority-class threshold for the neutral
probability given a 3-class softmax.

**Write call:**

```rust
if let NliCandidatePair::Informs { candidate, nli_scores } = pair {
    if nli_scores.neutral > 0.5
        && candidate.source_created_at < candidate.target_created_at
        && candidate.source_feature_cycle != candidate.target_feature_cycle
    {
        let weight = candidate.cosine * config.nli_informs_ppr_weight;
        let metadata_json = format_nli_metadata_informs(&nli_scores);
        write_nli_edge(
            store,
            candidate.source_id,
            candidate.target_id,
            "Informs",         // RelationType::Informs.as_str()
            weight,
            timestamp,
            &metadata_json,
        ).await;
        informs_edges_written += 1;
    }
}
```

**Delivery note — `format_nli_metadata` and neutral score:** The existing
`format_nli_metadata` serializes only `entailment` and `contradiction`. For Informs edges,
the neutral score is the decision criterion (guard predicate 2 above). The delivery agent
should ensure that when writing Informs edges, `"nli_neutral": scores.neutral` is included
in the metadata JSON. This can be done by calling a dedicated `format_nli_metadata_informs`
variant or by extending `format_nli_metadata` to accept an optional flag. This is not
blocking delivery — the edge will be written correctly either way — but the neutral score
should be present for observability and post-hoc analysis.
```

**Cap in Phase 8b:** There is no secondary cap inside Phase 8b. The pre-Phase 5 truncation
already bounded `informs_pairs` to `remaining_capacity`. If all pass the composite guard,
all are written — this is intentional. The budget was reserved in Phase 5.

**Logging:**

```rust
tracing::debug!(
    informs_edges_written,
    informs_pairs_evaluated = informs_count,
    "graph inference tick: Informs write complete"
);
```

---

## PPR Direction Semantics for Informs

The `Informs` edge direction is `source → target` where source is empirical (lesson-learned,
pattern, earlier `created_at`) and target is normative (decision, convention, later
`created_at`). Example: Lesson #376 → ADR #2060.

In PPR, `Direction::Outgoing` implements the **reverse** random walk (transpose PPR). For an
edge A→B (`Informs`: lesson A informs decision B):

- When B (the decision) is seeded, mass flows in the reverse direction through the outgoing
  edges of A. Because A has an outgoing edge to B, A accumulates mass from B's score.
- Result: querying a decision surfaces the lesson that informed it.

This is the desired behavior. AC-05 must verify this explicitly by asserting that the
**lesson node** receives non-zero PPR mass when the **decision node** is seeded, not merely
that some node receives mass. The direction inversion makes it easy to write a test that
passes by accident if the assertion is too weak.

The fourth `edges_of_type` call in both `personalized_pagerank` and
`positive_out_degree_weight` uses `Direction::Outgoing`, identical to the existing three
calls for Supports, CoAccess, and Prerequisite. No direction argument changes anywhere.

---

## Technology Decisions

See ADR files:
- ADR-001-discriminator-tag-struct.md — Discriminator tag struct for merged NLI batch
- ADR-002-combined-cap-priority.md — Combined cap with Informs second-priority ordering
- ADR-003-directional-dedup.md — Directional dedup for `query_existing_informs_pairs`

Additional constraints inherited from prior features (not new decisions):
- AC-02 / edges_of_type boundary (crt-021, entry #2417): all PPR traversal via
  `edges_of_type()` — no `.edges_directed()` calls.
- EDGE_SOURCE_NLI = "nli" (col-029, entry #3591): Informs edges use the same source
  constant; no new source identifier.
- W1-2 contract: all `score_batch` calls via `rayon_pool.spawn()`. No new rayon spawn
  is introduced; the merged batch uses the single existing spawn.
- C-14 / R-09: rayon closure must remain sync-only CPU-bound. The merged closure body
  is identical to the current one — `score_batch` takes a `Vec<(&str, &str)>` regardless
  of pair origin.

---

## Integration Points

### unimatrix-engine ← unimatrix-server

`nli_detection_tick.rs` writes edge type `"Informs"` via `write_nli_edge`. The string
`"Informs"` must match `RelationType::Informs.as_str()` exactly. If these diverge, the
R-10 guard in `build_typed_relation_graph` will drop the edge silently and log a warning
(AC-03 / AC-04 detect this).

### unimatrix-store ← unimatrix-server

`query_existing_informs_pairs()` is called in Phase 2, exactly as
`query_existing_supports_pairs()`. It uses `read_pool()`.

### graph_ppr.rs ← run_graph_inference_tick

No direct coupling. PPR reads `GRAPH_EDGES` through `build_typed_relation_graph`. Once
`"Informs"` rows exist in `GRAPH_EDGES` and `RelationType::Informs` is recognized by
`from_str`, PPR picks them up automatically on the next graph rebuild.

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `RelationType::Informs` | enum variant | `unimatrix-engine/src/graph.rs` |
| `RelationType::Informs.as_str()` | `-> &'static str` = `"Informs"` | `graph.rs` |
| `RelationType::from_str("Informs")` | `-> Some(RelationType::Informs)` | `graph.rs` |
| `personalized_pagerank` (4th call) | `edges_of_type(idx, RelationType::Informs, Direction::Outgoing)` | `graph_ppr.rs` |
| `positive_out_degree_weight` (4th call) | `edges_of_type(idx, RelationType::Informs, Direction::Outgoing)` | `graph_ppr.rs` |
| `InferenceConfig::informs_category_pairs` | `Vec<[String; 2]>`, default 4 pairs | `config.rs` |
| `InferenceConfig::nli_informs_cosine_floor` | `f32`, default 0.45, range (0.0, 1.0) | `config.rs` |
| `InferenceConfig::nli_informs_ppr_weight` | `f32`, default 0.6, range [0.0, 1.0] | `config.rs` |
| `NliCandidatePair` | enum `{ SupportsContradict { source_id, target_id, cosine, nli_scores }, Informs { candidate, nli_scores } }` (module-private) | `nli_detection_tick.rs` |
| `InformsCandidate` | struct with 9 required (non-Option) fields: source_id, target_id, cosine, created_at×2, feature_cycle×2, category×2 (module-private) | `nli_detection_tick.rs` |
| `Store::query_existing_informs_pairs` | `async fn(&self) -> Result<HashSet<(u64, u64)>>` | `unimatrix-store/src/read.rs` |
| `write_nli_edge` (Informs call) | `(store, source_id, target_id, "Informs", weight: f32, ts, metadata)` | `nli_detection.rs` (pub(crate)) |

---

## Penalty Invariant Boundary (SR-05)

`graph_penalty` and `find_terminal_active` in `graph.rs` filter exclusively to `Supersedes`
edges via `edges_of_type`. This is documented in the module header (`graph.rs:16`). `Informs`
edges, like `Supports`, `CoAccess`, `Contradicts`, and `Prerequisite`, are present in the
graph but invisible to penalty logic. Any future feature adding a new penalty traversal path
must explicitly filter to `Supersedes` only — not to `RelationType::Informs` or any other
non-penalty type. The `edges_of_type` API enforces this at call sites; there is no global
filter.

---

## Open Questions

None. All OQs in SCOPE.md are resolved. Risks SR-01 through SR-08 are addressed:

- SR-01: Composite guard — NliCandidatePair is a tagged union (FR-10); all Phase 8b guard
  fields are non-Option on InformsCandidate. R-05 vacuous-pass risk eliminated at the type
  level. Phase 8b applies all guard predicates before writing.
- SR-02: Informs candidates share the Phase 5 combined cap (second-priority truncation bounds fan-out).
- SR-03: Cap-drop count logged at debug level per tick.
- SR-04: Default pair list is frozen at four. Expansion is explicitly deferred.
- SR-05: Penalty invariant documented in this section and in graph.rs module header.
- SR-06: crt-036 merge is a delivery gate check, not an architecture concern.
- SR-07: PPR direction confirmed; AC-05 must assert lesson node receives mass (not just non-zero somewhere).
- SR-08: NliCandidatePair is a tagged union — routing is by enum variant (compile-time
  enforced), not by index-matched parallel vecs or a runtime discriminator field.
- OQ-S2 (neutral threshold): NliScores.neutral is a true 3-class softmax output summing to
  1.0 with entailment and contradiction (invariant in cross_encoder.rs). The neutral > 0.5
  threshold is the majority-class criterion for the "unrelated" probability. Correctly
  calibrated; no change to the threshold value.
