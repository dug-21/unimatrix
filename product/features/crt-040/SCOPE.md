# crt-040: Cosine Supports Edge Detection

## Problem Statement

The NLI post-store path that previously detected `Supports` edges was removed in crt-038
as part of the conf-boost-c formula migration (`run_post_store_nli` deleted, `NliStoreConfig`
struct deleted). The result: zero `Supports` edges are ever written in production (NLI scores
never exceeded 0.45 on this corpus anyway — confirmed by ASS-035). The graph currently
consists of `CoAccess` and `Informs` edges only, with no `Supports` signal.

The PPR expander (Group 4 roadmap) requires a sufficiently dense, typed graph to be
meaningful. `Supports` edges are the entailment backbone of the typed relation graph.
Without them, the GNN training data (ASS-038) has no `Supports` signal and PPR traversal
lacks the entailment dimension designed for it.

ASS-035 validated a cosine-similarity-based replacement: at threshold ≥ 0.65, cosine
similarity over production embeddings correctly identifies 6/8 labeled true `Supports` pairs
with 0/10 false positives (including 5 compatible-category cross-feature negative controls).
The `same_feature_cycle` filter is not required for correctness at this threshold — cosine
alone is sufficient — but reduces the candidate search space.

This feature replaces the deleted NLI detection path with a pure-cosine `Supports` edge
detection mechanism running inside `structural_graph_tick` (which was decoupled from the
NLI gate in crt-039 and now runs unconditionally).

## Goals

1. Implement cosine-based `Supports` edge detection in `run_graph_inference_tick` (Path A
   extension or dedicated Path C), writing edges when cosine similarity ≥ 0.65 and the
   category pair is in `informs_category_pairs`.
2. Tag every new `Supports` edge with `signal_origin = 'cosine_supports'` in the
   `graph_edges.source` column so GNN feature construction can distinguish the signal source.
3. Add a config field `supports_cosine_threshold` (default 0.65) to `InferenceConfig` with
   range validation matching the pattern for `nli_informs_cosine_floor`.
4. Never write unlabeled edges — every edge produced by this path must carry a distinct,
   queryable `source` value.
5. Pass the existing eval gate: no MRR regression vs conf-boost-c baseline (MRR = 0.2875
   on behavioral ground truth, `product/research/ass-039/harness/scenarios.jsonl`).

## Non-Goals

- This feature does NOT replace or modify the NLI Supports path (Phase 8 / Path B). That
  path remains gated by `get_provider()` and is independent of this feature.
- This feature does NOT implement the PPR expander (Group 4). Graph enrichment enables the
  expander but does not include it.
- This feature does NOT implement S1, S2, or S8 edge sources. Only cosine Supports detection
  is in scope.
- This feature does NOT add a `signal_origin` schema column — the existing `source` column
  in `graph_edges` (TEXT) serves this role. No migration needed.
- This feature does NOT change the contradiction detection path.
- This feature does NOT change the Informs detection logic (Path A). The `informs_category_pairs`
  config field is reused as-is, not extended.
- This feature does NOT require same_feature_cycle filtering (ASS-035 Group D confirmed it
  is not needed for correctness at threshold ≥ 0.65), though it may be included as an
  optional optimization.
- This feature does NOT add same-category Supports pairs (e.g., `lesson-learned` →
  `lesson-learned`). Only the `informs_category_pairs` allow-list is used.

## Background Research

### What Was Deleted in crt-038/crt-039

**crt-038 deletions** (confirmed from codebase):
- `run_post_store_nli`: The primary NLI Supports detection path, called synchronously after
  `context_store`. Queried HNSW neighbors and ran them through the NLI cross-encoder.
- `NliStoreConfig`: Config struct for the post-store NLI path — held `nli_post_store_k` and
  per-call edge write caps. Now unused (but `nli_post_store_k` remains in `InferenceConfig`
  as a dead field with stale doc comment).
- `parse_nli_contradiction_from_metadata` and 5 cascaded tests.
- `nli_auto_quarantine_allowed`, `NliQuarantineCheck` — NLI-gated quarantine path.
- `maybe_run_bootstrap_promotion`, `run_bootstrap_promotion`.

**crt-039 deletions**:
- `NliCandidatePair::Informs` variant (now only `SupportsContradict` exists).
- `PairOrigin::Informs` variant.
- The NLI neutral guard from `apply_informs_composite_guard` (guards 4+5 removed).
- The outer `if nli_enabled` gate from `background.rs` that wrapped the entire tick.

### Current State of `structural_graph_tick`

`run_graph_inference_tick` in
`crates/unimatrix-server/src/services/nli_detection_tick.rs` now has two paths:

- **Path A (unconditional)**: Structural Informs detection. HNSW cosine scan with
  `nli_informs_cosine_floor = 0.50` floor, `informs_category_pairs` filter, temporal
  ordering guard (`source_created_at < target_created_at`), cross-feature guard (rejects
  same `feature_cycle`), and dedup via `existing_informs_pairs`. Writes up to
  `MAX_INFORMS_PER_TICK = 25` edges per tick using `write_nli_edge(...)`. Source field
  written: `'nli'` (via the hardcoded `'nli'` literal in the INSERT).
- **Path B (NLI-gated)**: `get_provider()` gates Phases 6/7/8. Writes `Supports` edges
  when `nli_scores.entailment > supports_edge_threshold`. Currently dead in production
  (`nli_enabled=false` default).

The tick ordering invariant (non-negotiable per codebase comment):
`compaction → co_access_promotion → graph-rebuild → contradiction_scan
(if embed adapter ready + tick_multiple) → extraction_tick → structural_graph_tick (always)`

### Schema for `signal_origin`

There is no `signal_origin` column. The `graph_edges` table has a `source` column (TEXT,
NOT NULL DEFAULT ''). It is the semantic equivalent of `signal_origin` in the roadmap
context:
- `'nli'` — written by `write_nli_edge()` for both `Informs` and `Supports` edges (via
  hardcoded literal in the INSERT statement in `nli_detection.rs`)
- `'co_access'` — written by co_access promotion tick (`EDGE_SOURCE_CO_ACCESS` constant)
- `''` (empty) — bootstrap edges written by migration

The roadmap's `signal_origin='cosine_supports'` maps to writing `source = 'cosine_supports'`
in the `graph_edges.source` column. No schema migration is required.

Named constant `EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports"` must be added to
`read.rs` and re-exported from `unimatrix-store::lib.rs`, following the pattern established
by `EDGE_SOURCE_NLI` (col-029 ADR-001) and `EDGE_SOURCE_CO_ACCESS` (crt-034 ADR-002).

### `write_nli_edge` Reuse

The existing `write_nli_edge` function hardcodes `'nli'` as both `created_by` and `source`
in the INSERT. To write `source = 'cosine_supports'`, either:
1. Add a `source: &str` parameter to `write_nli_edge`, or
2. Add a new `write_graph_edge` function that accepts `source` as a parameter.

Option 2 is cleaner given the function is used for both Informs (source='nli') and the
incoming Supports path (source='cosine_supports') — the name `write_nli_edge` would become
misleading when used for non-NLI edges. A new general `write_graph_edge` function is
preferred. The existing `write_nli_edge` can delegate to it or remain as a wrapper.

### Category Pair Filter

`informs_category_pairs` defaults to:
```
["lesson-learned", "decision"],
["lesson-learned", "convention"],
["pattern", "decision"],
["pattern", "convention"],
```
These are the same pairs used by the Informs path. The roadmap specifies this allow-list
for cosine Supports detection. Reusing it avoids adding a new config field and is consistent
with ASS-035 validation methodology.

### Dedup Pattern

`existing_supports_pairs` is already pre-fetched in Phase 2 of the tick and used as a
HashSet pre-filter in Phase 4. The cosine Supports path can reuse this set directly. INSERT
OR IGNORE is the backstop (same as Informs path).

### Module Rename Note

The module comment in `nli_detection_tick.rs` explicitly defers a rename to
`graph_inference_tick` to "Group 3 when NLI is fully removed from Phase 8." crt-040 does
not remove Phase 8 — NLI Supports via the NLI model still works when `nli_enabled=true`.
Module rename is out of scope.

### `MAX_INFORMS_PER_TICK` Pattern

Informs uses `MAX_INFORMS_PER_TICK = 25` as an independent budget constant that does not
share the `max_graph_inference_per_tick` cap used by the NLI Supports path. The cosine
Supports path needs a similar independent budget constant or a new config field. The
roadmap does not specify which — this is an open question.

### GraphCohesionMetrics: `inferred_edge_count`

`GraphCohesionMetrics.inferred_edge_count` currently counts edges with `source = 'nli'`.
After this feature, there will be cosine Supports edges with a different source value. The
eval gate for crt-040 is "supports_coverage increase" in graph cohesion metrics. The
`supports_edge_count` field already counts all `Supports` edges regardless of source —
this is the correct metric for the eval gate. No change to `GraphCohesionMetrics` is
strictly required, but `inferred_edge_count` is now semantically stale (it was NLI-only).

### Lesson-Learned: `supports_edge_threshold` Calibration Trap

Entry #3713: `supports_edge_threshold = 0.7` caused near-zero NLI Supports edge writes
because retrospective/lesson-learned entries produce NLI scores in 0.6–0.69. The same trap
applies to cosine threshold selection. ASS-035 empirically validated 0.65 against the
actual corpus. Default must be 0.65, not higher.

## Proposed Approach

Add a "Path C" to `run_graph_inference_tick` — a pure-cosine Supports detection path that:

1. Runs after Path A (Informs writes) and before Path B gate (or alongside Path B).
2. Iterates over HNSW candidates from Phase 4 that have cosine ≥ `supports_cosine_threshold`
   (0.65 default) AND whose `[source_category, target_category]` pair is in
   `informs_category_pairs`.
3. Skips pairs already in `existing_supports_pairs` (Phase 2 pre-filter).
4. Writes `Supports` edges with `source = 'cosine_supports'` using either a generalized
   `write_graph_edge` function or a `source`-parameterized wrapper.
5. Is capped independently (new `MAX_COSINE_SUPPORTS_PER_TICK` constant or config field).

**Key design rationale:**
- Uses Phase 4's `candidate_pairs` vec (already HNSW-expanded) rather than a new HNSW
  scan. This avoids duplicating the embedding lookup and leverages the existing
  `supports_candidate_threshold` pre-filter.
- Category pair filter reuses `informs_category_pairs` (no new config field required
  beyond `supports_cosine_threshold`).
- Does not touch Path A or Path B. Both remain unchanged.
- No temporal ordering guard (unlike Informs). ASS-035 found no benefit from temporal
  filtering for Supports pairs — the semantic relationship is symmetric ("A supports B"
  is meaningful regardless of creation order).
- No `same_feature_cycle` guard required (ASS-035 Group D confirmed correctness without it).

**Config changes:**
- Add `supports_cosine_threshold: f32` to `InferenceConfig` with default 0.65, range
  (0.0, 1.0) exclusive, same validation pattern as `nli_informs_cosine_floor`.

**Store changes:**
- Add `EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports"` constant to `read.rs`.
- Re-export from `lib.rs`.

**nli_detection.rs / write helper:**
- Generalize `write_nli_edge` OR add `write_graph_edge(source: &str, ...)` sibling.

## Acceptance Criteria

- AC-01: When `candidate_pairs` contains a pair with cosine ≥ `supports_cosine_threshold`
  AND `[source_category, target_category]` is in `informs_category_pairs`, a `Supports`
  edge is written with `source = EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports"`.
- AC-02: When cosine < `supports_cosine_threshold`, no `Supports` edge is written for
  that pair by the cosine Supports path.
- AC-03: When the category pair is NOT in `informs_category_pairs`, no `Supports` edge is
  written for that pair even if cosine ≥ threshold.
- AC-04: Pairs already in `existing_supports_pairs` are skipped (INSERT OR IGNORE is
  the backstop; pre-filter is an optimization).
- AC-05: Cosine Supports detection runs unconditionally — it is NOT gated by `nli_enabled`
  or `get_provider()`.
- AC-06: Informs path (Path A) behavior is unchanged — `informs_category_pairs` usage in
  Path A and guards are not modified.
- AC-07: NLI Supports path (Path B) behavior is unchanged — Phase 6/7/8 and
  `supports_edge_threshold` are not modified.
- AC-08: `EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports"` is defined as a named constant
  in `unimatrix-store::read` and re-exported from the crate root.
- AC-09: `supports_cosine_threshold` config field added to `InferenceConfig` with default
  0.65, range (0.0, 1.0) exclusive. `InferenceConfig::validate()` rejects out-of-range
  values with a structured error.
- AC-10: `InferenceConfig::default()` returns `supports_cosine_threshold = 0.65`.
- AC-11: The cosine Supports write path uses the generalized edge writer with
  `source = EDGE_SOURCE_COSINE_SUPPORTS` — NOT the hardcoded `'nli'` source.
- AC-12: The cosine Supports path is capped per tick (constant or config field). It does not
  consume the `max_graph_inference_per_tick` budget used by Path B.
- AC-13: Tick ordering invariant is preserved. Cosine Supports detection runs inside
  `run_graph_inference_tick` which always runs last in the tick sequence.
- AC-14: Eval gate passes: no MRR regression vs baseline 0.2875 on
  `product/research/ass-039/harness/scenarios.jsonl` after delivery.
- AC-15: `inferred_edge_count` in `GraphCohesionMetrics` continues to count only
  `source = 'nli'` edges (backward compat). If a new metric is added for cosine-origin
  edges, it is additive — existing metric is unchanged.

## Constraints

- **No new HNSW scan in Path C.** Must reuse `candidate_pairs` from Phase 4. Adding a
  separate HNSW scan would double the per-tick embedding lookup budget.
- **`informs_category_pairs` reuse is mandatory.** The spec requires this filter; no new
  allow-list config field is in scope.
- **`write_nli_edge` hardcodes `'nli'` as source.** The edge writer must be generalized
  or a sibling added before cosine Supports edges can be written with the correct source.
  Changing the hardcoded literal in `write_nli_edge` would silently retag all existing
  Informs and NLI Supports edges — NOT acceptable. Must be a new code path.
- **W1-2 contract:** ALL `CrossEncoderProvider::score_batch` calls via `rayon_pool.spawn()`.
  The cosine Supports path must NOT call `score_batch` (it is pure cosine, no model).
  `spawn_blocking` is also prohibited in this path.
- **R-09 Rayon/Tokio boundary:** Cosine lookup (`vector_index.get_embedding`, `search`) is
  synchronous. The cosine Supports path remains in the async Tokio context, same as Path A.
- **500-line file limit:** `nli_detection_tick.rs` is already large (> 2,000 lines including
  tests). If Path C adds significant code, consider whether extraction to a helper module is
  warranted. Delivery agent must evaluate at implementation time.
- **No migration required.** The `source` column already exists in `graph_edges`.
- **Backward compat on `inferred_edge_count`:** Existing status reporting counts `source='nli'`
  as `inferred_edge_count`. Do not change this meaning — cosine Supports edges must be
  queryable by their distinct source value but are not required to be counted in the same
  metric.
- **Prerequisite verified:** crt-039 (Group 2) is merged (PR #486). `structural_graph_tick`
  runs unconditionally. This is confirmed.

## Resolved Design Decisions

1. **Budget: `MAX_COSINE_SUPPORTS_PER_TICK = 50` (constant).** Cosine lookup against an
   already-scanned candidate set has no model cost. Follows the `MAX_INFORMS_PER_TICK`
   pattern. Config promotion is easy later if an operator needs it — don't speculate now.

2. **Canonical direction only.** Phase 4's dedup normalization already produces `(lo, hi)`
   pairs where `lo = source_id.min(&neighbour_id)`. Path C output is naturally canonical.
   Writing both directions doubles edge count without meaningful PPR benefit at current
   graph density.

3. **`weight = cosine`, no multiplier.** `nli_informs_ppr_weight` existed because the
   Informs path needed independent PPR weight tuning separate from the detection threshold.
   For cosine Supports, the cosine value at ≥ 0.65 is already a high-confidence signal and
   serves naturally as the PPR weight. No config knob — no validated use yet.

4. **Metadata: `{"cosine": f32}`.** Shorter, consistent with how cosine values appear
   elsewhere in the codebase.

5. **`inferred_edge_count`: leave as-is.** The eval gate uses `supports_edge_count` (all
   Supports regardless of source) — that's what matters. `inferred_edge_count` is a stale
   name but renaming it is a separate concern. File a follow-up issue; out of scope here.

6. **Remove `nli_post_store_k` now.** Already touching `InferenceConfig` to add
   `supports_cosine_threshold`. `nli_post_store_k` has no consumer since
   `run_post_store_nli` was deleted in crt-038. Removing it is safe — serde ignores
   unknown fields in existing config.toml files. One-line removal prevents future agents
   from being confused by a dead field with a stale doc comment.

## Cross-Feature Note: impl Default Trap (pattern #4011)

crt-040 and crt-041 both touch `InferenceConfig` and add new config fields. Pattern #4011
(caught during crt-038 Gate 3b) documents that `impl Default for InferenceConfig` has
hardcoded field values that are SEPARATE from the `default_w_*()` backing functions. Any
new field added via serde `#[serde(default = "...")]` must ALSO be set explicitly in
`impl Default` — otherwise `InferenceConfig::default()` returns a different value than
deserialization of an empty config. This is a silent behavioral divergence.

For crt-040: `supports_cosine_threshold` must be set in BOTH:
1. `#[serde(default = "default_supports_cosine_threshold")]` — deserialization default
2. `impl Default for InferenceConfig { ... supports_cosine_threshold: 0.65 ... }` — code default

Architecture and spec must call this out explicitly. Delivery must verify via a unit test
that `InferenceConfig::default().supports_cosine_threshold == 0.65`.

## Architecture Note (for Phase 2a)

Verify that the `GRAPH_EDGES` unique constraint covers `(source_id, target_id,
relation_type)` WITHOUT including `source`. When `nli_enabled=true`, Path B and Path C
could both attempt a `Supports` edge for the same pair in the same tick. `INSERT OR IGNORE`
will silently discard the second write — that is correct behavior (one edge per pair per
type). The architecture must call this out explicitly so delivery does not treat the silent
discard as a bug.

## Tracking

https://github.com/dug-21/unimatrix/issues/488
