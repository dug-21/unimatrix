# crt-037: Informs Edge Type — Cross-Feature Institutional Memory Bridge

## Problem Statement

The TypedRelationGraph populates edges through two automated mechanisms: `CoAccess` (behavioral
co-occurrence) and `Supports`/`Contradicts` (NLI entailment). Both mechanisms are blind to a
specific class of relationship: **a lesson or empirical pattern from a past feature cycle that
directly informs a design decision in a later feature cycle.**

Live data confirms the failure mode:

- 868 of 1,093 active entries (79%) are isolated from PPR traversal — zero edges.
- Every NLI `Supports` edge is within the same category. The `lesson-learned → decision` bridge
  has zero representation across 31,680 candidate pairs.
- Isolated entries are systematically older (decisions isolated 2.6x older than connected;
  conventions 3.5x). The entries containing the hardest-won institutional knowledge are the
  least accessible to later agents.

**Confirmed missed bridge (ASS-034 Finding 3):** Lesson #376 ("DDL-before-migration ordering
causes post-merge production failures", written nxs-008, March 5) has never been served to any
session in 26 days. ADR #2060 ("Migration Connection Sequencing", nxs-011, March 17) addresses
the same architectural invariant — 12 days later, zero edge between them.

**Root cause:** The NLI detection tick scores candidate pairs as entailment / neutral /
contradiction. `Supports` edges require `entailment > 0.6`. The neutral result — the exact zone
where "past failure informs later decision" relationships live — is computed every tick and
**currently discarded**. The neutral zone produces zero edges of any kind.

Additionally, the `Informs` candidates overlap but extend beyond the existing `Supports` path:
the NLI candidate scan uses `cosine > 0.5`, but `Informs` requires a separate HNSW scan at
`cosine ≥ 0.45` because the 0.45–0.50 band contains semantically related cross-category pairs
that never reach NLI scoring at all today.

---

## Goals

1. Add `RelationType::Informs` as a new positive edge type in `graph.rs`, writable by the NLI
   tick, visible to PPR traversal.
2. Add a fourth branch to `run_graph_inference_tick` in `nli_detection_tick.rs` that detects
   neutral-zone cross-category pairs and writes `Informs` edges with temporal ordering and
   feature-cycle separation guards.
3. Run a separate HNSW scan at `nli_informs_cosine_floor` (default 0.45) for `Informs`
   candidates, scoped to cross-category pairs matching `informs_category_pairs` config.
4. Add `Informs` as a positive edge type in `graph_ppr.rs` (`personalized_pagerank` and
   `positive_out_degree_weight`) so PPR traversal reaches empirical knowledge from decision seeds.
5. Expose three new `InferenceConfig` fields: `informs_category_pairs`, `nli_informs_cosine_floor`,
   `nli_informs_ppr_weight` — defaults covering the software engineering domain.
6. Enforce domain agnosticism: the strings `"lesson-learned"`, `"decision"`, `"pattern"`,
   `"convention"` must not appear in detection logic — only in compiled default config values.

---

## Non-Goals

- Config-extensible relation types (full `[[inference.relation_types]]` TOML blocks). The
  ASS-034 research resolved that `RelationType::Informs` as a fixed enum variant with a
  configurable category-pair list is the correct scope for v1. The full extensibility design
  is explicitly deferred.
- Any schema migration. `GRAPH_EDGES.relation_type` is already a free-text column; "Informs"
  is stored as a string, no DDL change needed.
- A new ML model. This feature reuses the existing NLI cross-encoder session (`NliServiceHandle`)
  already running the `Supports`/`Contradicts` detection passes.
- A new background tick or tick infrastructure. `Informs` detection runs within the existing
  `run_graph_inference_tick` call.
- `Extended(String)` or open-ended `RelationType` variants. Adding `Informs` as a named enum
  variant is sufficient; open extension is deferred.
- Changes to the post-store NLI path (`run_post_store_nli` in `nli_detection.rs`). The
  `Informs` detection is background-tick-only — not triggered on `context_store`.
- Changes to the `Contradicts` detection path or `suppress_contradicts`. Those paths are
  unaffected by this feature.
- Textual reference extraction (`Mentions` edges) or feature co-membership detection
  (`ImplementsDecision` edges). These were identified in ASS-034 as future work.
- LLM-at-store-time annotation. Unimatrix is LLM-agnostic; annotation is a caller
  responsibility, not an internal inference step.
- Changes to graph compaction, build order, or `VECTOR_MAP`. `Informs` edges use the same
  write path as `Supports`.
- Changes to `write_inferred_edges_with_cap`. The `Informs` write path uses `write_nli_edge`
  directly (already `pub(crate)`) under its own cap tracking within `max_graph_inference_per_tick`.

---

## Background Research

### Codebase State

**`RelationType` enum** (`crates/unimatrix-engine/src/graph.rs:75`): Five variants —
`Supersedes`, `Contradicts`, `Supports`, `CoAccess`, `Prerequisite`. `from_str` is case-sensitive
and returns `None` for unknown strings, causing `build_typed_relation_graph` Pass 2b to skip any
edge row whose `relation_type` string is not one of these five (line 289: R-10 guard). Adding
`Informs` requires extending both `as_str()` and `from_str()`.

**PPR positive edges** (`crates/unimatrix-engine/src/graph_ppr.rs`): `personalized_pagerank`
makes three separate `edges_of_type` calls per node per iteration — `Supports`, `CoAccess`,
`Prerequisite`. `positive_out_degree_weight` has a matching three-call structure. Both functions
must gain a fourth call for `Informs`. The comment at line 89 already says "Three separate
`edges_of_type` calls (AC-02 — no `.edges_directed()` allowed)" — the pattern is clear.

**NLI detection tick** (`crates/unimatrix-server/src/services/nli_detection_tick.rs`):
`run_graph_inference_tick` has eight phases. Phase 8 calls `write_inferred_edges_with_cap` which
writes `Supports`-only, discarding the `contradiction` score. The `NliScores` struct contains
`entailment`, `neutral`, and `contradiction` fields — the neutral score is already available
from the same `score_batch` call, it is simply never evaluated. The `Informs` pass requires a
second HNSW scan (Phase 4b) at the lower cosine floor, with the cross-category filter and
temporal ordering applied before scoring.

**`InferenceConfig`** (`crates/unimatrix-server/src/infra/config.rs`): Already holds 25+ fields
with serde defaults and a `validate()` method. The pattern for adding new fields with default
functions, range validation, and TOML tests is well-established and can be followed exactly.
`supports_candidate_threshold < supports_edge_threshold` cross-field invariant (line 875) sets
the precedent for any cross-field validation needed for `nli_informs_cosine_floor`.

**`write_nli_edge`** (`nli_detection.rs:532`): `pub(crate)` function that accepts
`source_id`, `target_id`, edge type string, weight (f32), timestamp, and metadata JSON. The
`Informs` write path will call this function with `"Informs"` as the edge type string — no new
write function needed.

**`EDGE_SOURCE_NLI` constant** (ADR-001 col-029, entry #3591): The string `"nli"` is the named
constant written to `graph_edges.source` for NLI-inferred edges. `Informs` edges use this same
source value — no new source identifier.

**`query_existing_supports_pairs`**: This query pre-filters known `Supports` pairs. The `Informs`
pass needs a parallel `query_existing_informs_pairs` query to deduplicate across ticks. The
existing `INSERT OR IGNORE` backstop still applies, but a pre-filter avoids redundant NLI scoring.

**PPR direction semantics** (graph_ppr.rs:37-38): PPR uses `Direction::Outgoing` to implement
the reverse random walk. An entry A (lesson-learned) that has an `Informs` edge to B (decision)
means A points to B. When B is seeded, mass flows backward to A — surfacing the lesson-learned
when the decision is in the query context. This is exactly the desired behavior.

**crt-036 dependency**: crt-036 stabilizes retention of `observations`, `query_log`, and
`injection_log`. ASS-034 Finding 8 confirms edge detection reads no activity tables — the
technical dependency is zero. The dependency is logistical: crt-036 should merge first per the
issue's stated dependency.

### Key ADR Constraints Inherited

- **ADR-001 (crt-021, entry #2417)**: All traversal via `edges_of_type()` exclusively — no
  `.edges_directed()` calls. `Informs` must use this boundary in PPR.
- **ADR-003 (crt-029)**: `max_graph_inference_per_tick` is the sole throttle on tick NLI budget.
  The `Informs` HNSW scan must share or subordinate within this cap.
- **col-030 (entry #3628)**: NLI writes unidirectional edges. `Informs` follows the same pattern —
  direction is determined by temporal ordering (source.created_at < target.created_at).
- **C-14 / R-09**: The rayon closure in Phase 7 must be synchronous CPU-bound only. No
  `tokio::runtime::Handle::current()`, no `.await`. This constraint applies to any new NLI batch.
- **W1-2 contract**: All `score_batch` calls via `rayon_pool.spawn()`. Never inline async,
  never `spawn_blocking`.

---

## Proposed Approach

### Phase A: Engine changes (pure, no I/O)

1. Add `RelationType::Informs` to `graph.rs`. Extend `as_str()` and `from_str()`.
2. Add `Informs` to PPR: fourth `edges_of_type` call in `personalized_pagerank` inner loop and
   in `positive_out_degree_weight`.

### Phase B: Config changes

3. Add three fields to `InferenceConfig`:
   - `informs_category_pairs: Vec<[String; 2]>` — serde default = 4 software engineering pairs
   - `nli_informs_cosine_floor: f32` — default 0.45, range (0.0, 1.0) exclusive
   - `nli_informs_ppr_weight: f32` — default 0.6, range [0.0, 1.0]
4. Extend `validate()` with range checks for the two scalar fields. No cross-field invariant
   with `supports_candidate_threshold` — `Informs` candidates are a separate scan, not subject
   to the existing `candidate < edge` invariant.
5. Add config merge logic in the project/global fusion path.

### Phase C: Detection (tick changes)

6. Add `query_existing_informs_pairs` to `Store` — pre-filter for the `Informs` dedup loop,
   mirroring `query_existing_supports_pairs`.
7. Add Phase 4b to `run_graph_inference_tick`: a second HNSW scan at `nli_informs_cosine_floor`,
   filtered to source entries whose category is a left-hand side in `informs_category_pairs`.
   The cross-category, temporal ordering, and feature-cycle separation filters apply here before
   the pair reaches NLI scoring.
8. The Phase 7 rayon batch already includes all scored pairs. After the existing Phase 8
   `Supports`-only write, add a new Phase 8b `Informs` write loop: iterate `nli_scores`, apply
   `nli.neutral > 0.5` + per-pair category/temporal/feature-cycle guards (from Phase 4b
   metadata), write `Informs` edge with `weight = similarity * nli_informs_ppr_weight`.

**Cap accounting**: the `max_graph_inference_per_tick` cap applies across both the existing
`Supports` candidates and the new `Informs` candidates combined. The simplest implementation
reserves the cap for the existing `Supports` path (unchanged) and adds a subordinate cap for
the `Informs` scan (e.g., min(config.max_graph_inference_per_tick, informs_candidate_count)).
This avoids starvation of either pass and preserves the infallible-tick guarantee.

---

## Acceptance Criteria

- AC-01: `RelationType::from_str("Informs")` returns `Some(RelationType::Informs)`.
- AC-02: `RelationType::Informs.as_str()` returns `"Informs"`.
- AC-03: `build_typed_relation_graph` with a `GraphEdgeRow` containing `relation_type = "Informs"`
  succeeds and includes the edge in the output graph.
- AC-04: `build_typed_relation_graph` no longer emits a `warn` for `"Informs"` edge rows
  (i.e., the R-10 guard does not fire for `"Informs"`).
- AC-05: `personalized_pagerank` propagates mass through `Informs` edges. A graph with seed on
  a decision node and an `Informs` edge from a lesson node to the decision node results in a
  non-zero PPR score on the lesson node.
- AC-06: `positive_out_degree_weight` includes `Informs` edge weights in the out-degree sum.
- AC-07: `InferenceConfig` deserializes `informs_category_pairs` from TOML. Empty TOML
  deserializes to the four software engineering default pairs.
- AC-08: `InferenceConfig` deserializes `nli_informs_cosine_floor` from TOML. Empty TOML
  deserializes to 0.45.
- AC-09: `InferenceConfig` deserializes `nli_informs_ppr_weight` from TOML. Empty TOML
  deserializes to 0.6.
- AC-10: `InferenceConfig::validate()` rejects `nli_informs_cosine_floor` outside `(0.0, 1.0)`.
- AC-11: `InferenceConfig::validate()` rejects `nli_informs_ppr_weight` outside `[0.0, 1.0]`.
- AC-12: The default `InferenceConfig` passes `validate()`.
- AC-13: `run_graph_inference_tick` with an entry pair matching `informs_category_pairs`,
  cosine ≥ `nli_informs_cosine_floor`, `nli.neutral > 0.5`, `source.created_at < target.created_at`,
  and different `feature_cycle` values writes an `Informs` edge to `GRAPH_EDGES`.
- AC-14: `run_graph_inference_tick` does NOT write an `Informs` edge when
  `source.created_at >= target.created_at` (temporal ordering guard).
- AC-15: `run_graph_inference_tick` does NOT write an `Informs` edge when
  `source.feature_cycle == target.feature_cycle` (cross-feature guard).
- AC-16: `run_graph_inference_tick` does NOT write an `Informs` edge when the category pair
  is NOT in `informs_category_pairs` (domain agnosticism guard).
- AC-17: `run_graph_inference_tick` does NOT write an `Informs` edge when
  `cosine < nli_informs_cosine_floor` (cosine floor guard).
- AC-18: The `Informs` HNSW scan uses the `nli_informs_cosine_floor` threshold (default 0.45),
  distinct from the `supports_candidate_threshold` (default 0.50).
- AC-19: `Informs` edges written to `GRAPH_EDGES` have `source = "nli"` (EDGE_SOURCE_NLI).
- AC-20: `Informs` edge weight equals `similarity * nli_informs_ppr_weight` (both f32).
- AC-21: The rayon closure in Phase 7 remains sync-only. The `grep -n 'Handle::current' nli_detection_tick.rs` gate returns empty after the change.
- AC-22: Domain vocabulary (`informs_category_pairs` default values) does not appear as string
  literals in `nli_detection_tick.rs` — only in `InferenceConfig` default/serde code.
- AC-23: An `Informs` edge pair already in `GRAPH_EDGES` is not written a second time on
  subsequent ticks (dedup via `query_existing_informs_pairs` pre-filter + `INSERT OR IGNORE`).
- AC-24: `graph_penalty` and `find_terminal_active` do NOT traverse `Informs` edges. They
  continue to filter exclusively to `Supersedes` edges (SR-01 invariant preserved).

---

## Constraints

**Technical (non-negotiable):**
- No schema migration. `GRAPH_EDGES.relation_type` is free-text; "Informs" is stored as a string.
- No new ML model. Must reuse the existing `NliServiceHandle` / `CrossEncoderProvider`.
- No new tick infrastructure. Detection runs inside `run_graph_inference_tick`.
- W1-2 contract: all `score_batch` calls via `rayon_pool.spawn()`. No inline async NLI.
- C-14 / R-09: rayon closure remains synchronous CPU-bound. No `tokio` handle access inside rayon.
- SR-01: `graph_penalty` and `find_terminal_active` filter exclusively to `Supersedes`. `Informs`
  must not be visible to penalty logic.
- AC-02 / edges_of_type boundary: all PPR traversal via `edges_of_type()` — no direct
  `.edges_directed()` calls.
- `max_graph_inference_per_tick` is the sole tick-level throttle. The `Informs` HNSW scan
  must share this budget (not introduce an unbounded second scan).

**Design (from ASS-034 and GH#466):**
- `informs_category_pairs` defaults must not be hardcoded in detection logic. They live only
  in `InferenceConfig` default/serde code and are passed into the tick as config.
- `nli_informs_ppr_weight` is separate from `Supports` edge weight to allow per-deployment
  tuning of how strongly institutional memory influences PPR.
- `Informs` edges are directional: source (empirical) created before target (normative).
  Direction is enforced by temporal ordering, not by category alone.
- The separate HNSW scan at 0.45 is necessary because the existing `Supports` scan at 0.50
  does not surface pairs in the 0.45–0.50 cosine band.

**Dependency:**
- crt-036 must be merged before crt-037 delivery begins (per GH#466). No technical blocker
  exists in the detection logic itself, but logistical precedence is stated in the issue.

---

## Open Questions — Resolved

1. **Cap budget split** → **Combined with Informs as second-priority.** `Informs` candidates
   consume remaining capacity after `Supports`/`Contradicts` candidates are processed. The
   existing NLI pass has higher precision signal and must not be crowded out in ticks where
   the `Informs` candidate pool is large. Architecture must specify the priority ordering
   explicitly — not a flat merge of all candidates into one pool.

2. **NLI batch structure** → **Merge into Phase 7 with discriminator tag.** One rayon spawn
   maintains the W1-2 contract. Each pair in the merged batch carries a tag identifying its
   purpose (`Supports`/`Contradicts` vs `Informs`) so Phase 8 and Phase 8b can route correctly.
   The pairs cannot be an undifferentiated `Vec` — the write logic for each type differs
   (entailment threshold vs neutral threshold + temporal filter).

3. **`query_existing_informs_pairs` scope** → **Directional `(source_id, target_id)`.** The
   temporal ordering filter means the reverse edge would never pass detection anyway. Symmetric
   dedup is unnecessary and could suppress valid edges if data had ordering anomalies. Directional
   is both semantically correct and simpler.

4. **`nli.neutral > 0.5` threshold** → **Fixed constant.** The 0.5 floor is a model property
   (NLI neutral score reliability), not domain vocabulary. The three config fields cover all
   domain-tunable knobs. A fourth field for the neutral floor would parameterize the model,
   not the domain.

5. **Delivery gate structure** → **Post-delivery eval for ICD; functional correctness + zero
   regression at gate.** The graph needs several ticks to accumulate `Informs` edges before
   ICD moves measurably. Gate structure: functional correctness (Informs edges written for
   known cross-category pairs, temporal ordering respected) + zero regression on CC@5/ICD/MRR.
   Measure ICD delta at first tick and ~3-tick accumulation as post-delivery tracking.

---

## Tracking

GH Issue #466: https://github.com/unimatrix/unimatrix/issues/466
