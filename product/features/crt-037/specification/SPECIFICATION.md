# crt-037: Informs Edge Type — Cross-Feature Institutional Memory Bridge
## SPECIFICATION.md

---

## Objective

Add `RelationType::Informs` as a new positive edge type that bridges empirical knowledge from
past feature cycles (lessons, patterns) to normative knowledge in later feature cycles
(decisions, conventions). The detection pass runs within the existing
`run_graph_inference_tick` and writes edges when a candidate pair satisfies a composite guard:
cross-category, temporal ordering, cross-feature, cosine floor, and NLI neutral score. The new
edge type participates in PPR traversal so that seeding on a decision node surfaces the
lesson-learned entries that informed it.

---

## Functional Requirements

**FR-01**: `RelationType` in `graph.rs` gains a sixth variant `Informs`. `from_str("Informs")`
returns `Some(RelationType::Informs)` and `as_str()` returns `"Informs"`. Case sensitivity
matches the existing five variants (exact case).

**FR-02**: `build_typed_relation_graph` accepts `GraphEdgeRow` records with
`relation_type = "Informs"` and includes them in the output `TypedRelationGraph`. The R-10
unknown-type `warn` guard does not fire for `"Informs"`.

**FR-03**: `personalized_pagerank` gains a fourth `edges_of_type` call per node per iteration
for `RelationType::Informs`. All traversal remains via `edges_of_type` exclusively — no direct
`.edges_directed()` calls. The direction is `Direction::Outgoing` consistent with the
reverse-walk contract (entry #3744): for an `Informs` edge A→B (lesson A, decision B), mass
flows from B back to A when B is in the seed set.

**FR-04**: `positive_out_degree_weight` gains a matching fourth `edges_of_type` call for
`RelationType::Informs` so out-degree normalization accounts for `Informs` edges.

**FR-05**: `InferenceConfig` gains three new fields:
- `informs_category_pairs: Vec<[String; 2]>` — serde default: four software engineering pairs
  (see §Domain Models for the canonical list). Deserialized from TOML as an array of two-element
  string arrays.
- `nli_informs_cosine_floor: f32` — serde default `0.45`, range `(0.0, 1.0)` exclusive.
- `nli_informs_ppr_weight: f32` — serde default `0.6`, range `[0.0, 1.0]` inclusive.

**FR-06**: `InferenceConfig::validate()` rejects `nli_informs_cosine_floor` at or outside the
exclusive bounds `(0.0, 1.0)` (i.e., rejects `<= 0.0` and `>= 1.0`). It rejects
`nli_informs_ppr_weight` outside the inclusive bounds `[0.0, 1.0]`. The default
`InferenceConfig` passes `validate()`.

**FR-07**: `Store` gains a `query_existing_informs_pairs` method returning the set of
directional `(source_id, target_id)` pairs already present in `GRAPH_EDGES` with
`relation_type = "Informs"`. This mirrors `query_existing_supports_pairs`. Scope: directional
only — no symmetric expansion.

**FR-08**: `run_graph_inference_tick` gains Phase 4b: a second HNSW scan at
`nli_informs_cosine_floor` (default 0.45), distinct from the existing Phase 4 scan at
`supports_candidate_threshold` (default 0.50). Phase 4b produces `InformsCandidate` records
(see §Domain Models). The scan is filtered to source entries whose category is a left-hand side
in `informs_category_pairs`; pairs not matching the cross-category filter are excluded before
NLI scoring.

**FR-09**: Phase 4b applies the following guards before NLI scoring; pairs failing any guard
are discarded before entering the rayon batch:
- **Cross-category guard**: `(source.category, target.category)` must be a pair in
  `informs_category_pairs`.
- **Temporal ordering guard**: `source.created_at < target.created_at` (strictly less than).
- **Cross-feature guard**: `source.feature_cycle != target.feature_cycle`. Pairs with equal
  or null feature_cycle values on both sides are excluded.
- **Cosine floor guard**: `cosine >= nli_informs_cosine_floor`.
- **Dedup guard**: `(source_id, target_id)` not already in `query_existing_informs_pairs`.

**FR-10**: The Phase 7 rayon batch merges `Supports`/`Contradicts` candidates and `Informs`
candidates into a single spawn. Each element in the batch is a tagged union
(`NliCandidatePair`; see §Domain Models) carrying a discriminator identifying which write path
applies. The discriminator ensures Phase 8 (Supports/Contradicts write) and Phase 8b (Informs
write) route exclusively from their respective variant — misrouting is a compile-time error,
not a runtime branch.

**FR-11**: Phase 8b iterates scored `Informs` pairs. For each pair, it re-applies the composite
guard (neutral score threshold + category/temporal/cross-feature metadata stored in the
`InformsCandidate` record). An `Informs` edge is written iff all of the following hold:
- `nli.neutral > 0.5` (fixed threshold, not configurable).
- The `InformsCandidate` metadata confirms the pair passed all Phase 4b guards (guard
  re-verification against stored metadata; not a second DB read).
- `nli.entailment` and `nli.contradiction` scores do not individually exceed the
  `supports_edge_threshold` or `contradicts_edge_threshold` respectively (penalty/terminal
  exclusion SR-01 invariant; an entry already handled by `Supports`/`Contradicts` path must
  not additionally receive an `Informs` edge from the same pair).

**FR-12**: `Informs` edges are written via `write_nli_edge` with:
- `edge_type = "Informs"` (string literal; equals `RelationType::Informs.as_str()`).
- `weight = similarity * nli_informs_ppr_weight` (both f32, product is f32).
- `source = EDGE_SOURCE_NLI` constant (value `"nli"`).
- Timestamp: current UTC seconds from `current_timestamp_secs()`.

**FR-13**: `graph_penalty` and `find_terminal_active` must not traverse `Informs` edges. Both
functions filter exclusively to `RelationType::Supersedes` via `edges_of_type`. This invariant
is enforced structurally by the `edges_of_type` boundary (ADR-001 crt-021, entry #2417) and
must remain intact after this feature.

**FR-14**: The `Informs` candidate slice is capped before NLI scoring. The combined
`Supports`/`Contradicts` candidate count has priority; `Informs` candidates consume the
remaining budget up to `max_graph_inference_per_tick`. The number of `Informs` candidates
processed and the number discarded by the cap must be logged per tick at debug level. This
satisfies the SR-03 observability requirement.

**FR-15**: Domain vocabulary (the four default category pair strings: `"lesson-learned"`,
`"decision"`, `"pattern"`, `"convention"`) must not appear as string literals in
`nli_detection_tick.rs`. These strings exist only in `InferenceConfig` default/serde code and
are passed into the tick as config values.

---

## Non-Functional Requirements

**NF-01 (Tick latency budget)**: The Phase 4b HNSW scan must not increase p95 tick duration by
more than **50 ms** compared to the pre-crt-037 baseline on a graph of 1,000 active entries
with `max_graph_inference_per_tick = 100`. This bound is derived from the existing tick p95
target of ~200 ms (established in crt-029); a 25% budget addition is the maximum acceptable
for a second scan that is explicitly second-priority. If the Phase 4b candidate slice is empty
(no matching category sources), the overhead must be < 5 ms.

**NF-02 (Cap safety)**: The combined NLI batch sent to rayon Phase 7 must not exceed
`max_graph_inference_per_tick` pairs in total across both `Supports`/`Contradicts` and
`Informs` candidates. This is the sole tick-level throttle (ADR-003 crt-029, entry #3658).
No new top-level config field for the Informs slice size is introduced; the slice is bounded
by `max_graph_inference_per_tick - supports_candidate_count`.

**NF-03 (No regression)**: All existing tests for `graph.rs`, `graph_ppr.rs`,
`nli_detection_tick.rs`, and `config.rs` must pass unchanged after this feature. The test
count must not decrease.

**NF-04 (Sync-only rayon closure)**: The rayon closure in Phase 7 remains synchronous
CPU-bound. After the change, `grep -n 'Handle::current' nli_detection_tick.rs` returns empty
(C-14 / R-09 contract). No `tokio::runtime::Handle::current()` and no `.await` inside any
rayon-spawned closure.

**NF-05 (No schema migration)**: `GRAPH_EDGES.relation_type` is a free-text column with no
CHECK constraint. No DDL change is required or permitted. The string `"Informs"` is stored
and retrieved as-is.

**NF-06 (No new ML model)**: The existing `NliServiceHandle` / `CrossEncoderProvider` is
reused. No new ONNX session, no new model file, no new inference infrastructure.

**NF-07 (No new tick)**: Detection runs inside `run_graph_inference_tick`. No additional
background task or timer is registered.

**NF-08 (Weight finitude)**: `Informs` edge weight (`similarity * nli_informs_ppr_weight`)
must be finite (not NaN, not ±Inf) before any write path accepts it. This follows the ADR-001
crt-021 invariant (entry #2417) requiring all weight values validated finite.

---

## Acceptance Criteria

Each criterion lists its ID from SCOPE.md, its statement, and the verification method.

| AC-ID | Statement | Verification Method |
|-------|-----------|---------------------|
| AC-01 | `RelationType::from_str("Informs")` returns `Some(RelationType::Informs)` | Unit test in `graph.rs` |
| AC-02 | `RelationType::Informs.as_str()` returns `"Informs"` | Unit test in `graph.rs` |
| AC-03 | `build_typed_relation_graph` with a `GraphEdgeRow` containing `relation_type = "Informs"` succeeds and includes the edge in the output graph | Unit test: build graph with one `Informs` row, assert edge present |
| AC-04 | `build_typed_relation_graph` does not emit a `warn` for `"Informs"` edge rows (R-10 guard does not fire) | Unit test: capture log output, assert no `warn` level entries mentioning `"Informs"` |
| AC-05 | `personalized_pagerank` propagates mass through `Informs` edges: a graph with seed on a decision node and an `Informs` edge from a lesson node to the decision node results in a non-zero PPR score **on the lesson node specifically** | Unit test in `graph_ppr.rs`: construct two-node graph (lesson A, decision B), add `Informs` A→B edge, seed at B, assert `scores[A] > 0.0` |
| AC-06 | `positive_out_degree_weight` includes `Informs` edge weights in the out-degree sum | Unit test: node with one `Informs` edge and no other edges; assert `positive_out_degree_weight` returns the edge weight, not zero |
| AC-07 | `InferenceConfig` deserializes `informs_category_pairs` from TOML; empty TOML yields four default software engineering pairs | Unit test in `config.rs`: parse empty TOML, assert default pairs present |
| AC-08 | `InferenceConfig` deserializes `nli_informs_cosine_floor` from TOML; empty TOML yields `0.45` | Unit test in `config.rs`: parse empty TOML, assert field equals `0.45_f32` |
| AC-09 | `InferenceConfig` deserializes `nli_informs_ppr_weight` from TOML; empty TOML yields `0.6` | Unit test in `config.rs`: parse empty TOML, assert field equals `0.6_f32` |
| AC-10 | `InferenceConfig::validate()` rejects `nli_informs_cosine_floor` outside `(0.0, 1.0)` exclusive (i.e., rejects 0.0 and 1.0 as boundary values) | Unit test: validate with value `0.0` → error; validate with value `1.0` → error; validate with `0.45` → ok |
| AC-11 | `InferenceConfig::validate()` rejects `nli_informs_ppr_weight` outside `[0.0, 1.0]` inclusive (i.e., accepts 0.0 and 1.0, rejects `-0.01` and `1.01`) | Unit test: validate with `-0.01` → error; `1.01` → error; `0.0` → ok; `1.0` → ok |
| AC-12 | The default `InferenceConfig` passes `validate()` | Unit test: `InferenceConfig::default()` then `validate()` returns `Ok(())` |
| AC-13 | `run_graph_inference_tick` writes an `Informs` edge when category pair matches `informs_category_pairs`, cosine ≥ `nli_informs_cosine_floor`, `nli.neutral > 0.5`, `source.created_at < target.created_at`, and feature cycles differ | Integration test: inject two entries satisfying all guards, run tick, assert `GRAPH_EDGES` contains row with `relation_type = "Informs"` |
| AC-14 | `run_graph_inference_tick` does NOT write an `Informs` edge when `source.created_at >= target.created_at` | Integration test: same pair as AC-13 but with `source.created_at = target.created_at`; assert no `Informs` row written |
| AC-15 | `run_graph_inference_tick` does NOT write an `Informs` edge when `source.feature_cycle == target.feature_cycle` | Integration test: same pair but same feature cycle string; assert no `Informs` row written |
| AC-16 | `run_graph_inference_tick` does NOT write an `Informs` edge when category pair is NOT in `informs_category_pairs` | Integration test: pair with categories `("decision", "decision")`; assert no `Informs` row written |
| AC-17 | `run_graph_inference_tick` does NOT write an `Informs` edge when `cosine < nli_informs_cosine_floor` | Integration test: cosine 0.44 with default floor 0.45; assert no `Informs` row written |
| AC-18 | The `Informs` HNSW scan uses `nli_informs_cosine_floor` (default 0.45), distinct from `supports_candidate_threshold` (default 0.50) | Unit test / code inspection: assert Phase 4b scan invocation uses the `nli_informs_cosine_floor` config field, not `supports_candidate_threshold` |
| AC-19 | `Informs` edges written to `GRAPH_EDGES` have `source = "nli"` (EDGE_SOURCE_NLI constant) | Integration test: after tick, assert `source` column equals `"nli"` on the written row |
| AC-20 | `Informs` edge weight equals `similarity * nli_informs_ppr_weight` (both f32) | Unit test: given known similarity `s` and weight `w`, assert written edge weight equals `s * w` within f32 epsilon |
| AC-21 | The rayon closure in Phase 7 remains sync-only after the change | CI gate: `grep -n 'Handle::current' nli_detection_tick.rs` returns empty; enforced as a build-time assertion or CI lint step |
| AC-22 | Domain vocabulary strings (`informs_category_pairs` default values) do not appear as string literals in `nli_detection_tick.rs` | CI gate: `grep -n '"lesson-learned"\|"decision"\|"pattern"\|"convention"' nli_detection_tick.rs` returns empty |
| AC-23 | An `Informs` pair already in `GRAPH_EDGES` is not written a second time on subsequent ticks (dedup via `query_existing_informs_pairs` pre-filter + `INSERT OR IGNORE` backstop) | Integration test: run tick twice; assert `GRAPH_EDGES` contains exactly one `Informs` row for the qualifying pair |
| AC-24 | `graph_penalty` and `find_terminal_active` do NOT traverse `Informs` edges; they continue to filter exclusively to `Supersedes` edges | Unit test: graph with only an `Informs` edge; assert `graph_penalty` returns `FALLBACK_PENALTY` (no penalty contribution from `Informs`); assert `find_terminal_active` returns empty |

---

## Domain Models

### Ubiquitous Language

| Term | Definition |
|------|------------|
| **Informs edge** | A directed edge `A → B` in `TypedRelationGraph` with `relation_type = RelationType::Informs`. Asserts that empirical knowledge in entry A (written earlier, from a prior feature cycle) directly informs normative knowledge in entry B (written later, from a different feature cycle). |
| **Empirical knowledge** | An entry whose category is a left-hand side of an `informs_category_pairs` pair: by default `lesson-learned` or `pattern`. Represents knowledge derived from operational experience or observed failure. |
| **Normative knowledge** | An entry whose category is a right-hand side of an `informs_category_pairs` pair: by default `decision` or `convention`. Represents a choice or rule that governs future behavior. |
| **Category pair** | An ordered two-element tuple `[left_category, right_category]` from `informs_category_pairs`. The left category is the empirical side (source), the right is the normative side (target). Default: `["lesson-learned", "decision"]`, `["lesson-learned", "convention"]`, `["pattern", "decision"]`, `["pattern", "convention"]`. |
| **Cosine floor** | The minimum cosine similarity required for a candidate pair to enter Phase 4b processing. Configured as `nli_informs_cosine_floor` (default 0.45). Distinct from and lower than `supports_candidate_threshold` (0.50) to surface semantically related cross-category pairs missed by the existing `Supports` scan. |
| **Temporal ordering guard** | A precondition on `Informs` edge creation: `source.created_at < target.created_at`. The empirical entry must predate the normative entry. Enforces directionality as a time-causal constraint, not a category-alone constraint. |
| **Cross-feature guard** | A precondition on `Informs` edge creation: `source.feature_cycle != target.feature_cycle`. Informs edges bridge distinct feature cycles; intra-cycle connections use `Supports`. |
| **Composite guard** | The full set of preconditions that must simultaneously hold for an `Informs` edge to be written: cross-category + temporal ordering + cross-feature + cosine floor + `nli.neutral > 0.5` + dedup. No single predicate is sufficient. |
| **Neutral threshold** | The fixed value `0.5`. An NLI neutral score above this threshold, combined with the other composite guard conditions, is the signal that a pair carries an empirical-to-normative relationship. This is a model-property constant, not a configurable domain knob. |
| **ICD (Inter-Cycle Distance)** | A post-delivery metric tracking whether PPR traversal surfaces entries from different feature cycles given a seed from a recent cycle. Not a gate criterion; measured at first tick and ~3-tick accumulation as post-delivery tracking. |
| **Discriminator tag** | A compile-time enum variant embedded in `NliCandidatePair` that identifies whether a scored pair routes to the `Supports`/`Contradicts` write path or the `Informs` write path. Misrouting is a compile-time error. |
| **EDGE_SOURCE_NLI** | Named constant with value `"nli"`. Written to `graph_edges.source` for all NLI-inferred edges including `Informs` (ADR-001 col-029, entry #3591). |

### Key Entities

#### `RelationType` (extended enum)

```
Supersedes    -- penalty/supersession traversal only
Contradicts   -- NLI contradiction signal
Supports      -- NLI entailment signal; positive PPR
CoAccess      -- behavioral co-occurrence; positive PPR
Prerequisite  -- reserved
Informs       -- NEW: empirical→normative cross-feature bridge; positive PPR
```

Representation: `as_str()` returns the variant name exactly. `from_str` is case-sensitive.
Penalty traversal functions (`graph_penalty`, `find_terminal_active`) use `Supersedes` only.
PPR positive traversal uses `Supports`, `CoAccess`, `Prerequisite`, and `Informs`.

#### `InformsCandidate` (record)

Produced by Phase 4b. Carries all metadata required by Phase 8b without a second DB read.

| Field | Type | Description |
|-------|------|-------------|
| `source_id` | `u64` | Entry ID of the empirical (left-hand) entry |
| `target_id` | `u64` | Entry ID of the normative (right-hand) entry |
| `cosine` | `f32` | Cosine similarity from the HNSW scan |
| `source_category` | `String` | Category of the source entry |
| `target_category` | `String` | Category of the target entry |
| `source_created_at` | `i64` | Unix timestamp (seconds) of source entry creation |
| `target_created_at` | `i64` | Unix timestamp (seconds) of target entry creation |
| `source_feature_cycle` | `Option<String>` | Feature cycle of the source entry |
| `target_feature_cycle` | `Option<String>` | Feature cycle of the target entry |

Note: `nli_scores` are not stored in `InformsCandidate` — they are the output of Phase 7 scoring and are associated by position with the batch element. The tagged union `NliCandidatePair` carries the `InformsCandidate` in its `Informs` variant alongside the `NliScores` after scoring.

#### `NliCandidatePair` (tagged union)

Used as the element type of the merged Phase 7 rayon batch.

```
NliCandidatePair::SupportContradicts {
    source_id: u64,
    target_id: u64,
    cosine: f32,
    nli_scores: NliScores,   // populated after Phase 7
}

NliCandidatePair::Informs {
    candidate: InformsCandidate,
    nli_scores: NliScores,   // populated after Phase 7
}
```

Phase 8 matches on `SupportContradicts` only. Phase 8b matches on `Informs` only. The
compiler enforces exhaustive matching — a new variant cannot be silently ignored.

#### `InferenceConfig` (extended fields)

Three new fields added to the existing struct following the `max_graph_inference_per_tick`
pattern (ADR-004 crt-034, entry #3826):

| Field | Type | Default | Range | Validation |
|-------|------|---------|-------|------------|
| `informs_category_pairs` | `Vec<[String; 2]>` | 4 SE pairs (see below) | — | Non-empty not enforced; empty disables Informs detection |
| `nli_informs_cosine_floor` | `f32` | `0.45` | `(0.0, 1.0)` exclusive | `validate()` rejects ≤ 0.0 or ≥ 1.0 |
| `nli_informs_ppr_weight` | `f32` | `0.6` | `[0.0, 1.0]` inclusive | `validate()` rejects < 0.0 or > 1.0 |

Default `informs_category_pairs` (four pairs, software engineering domain):
1. `["lesson-learned", "decision"]`
2. `["lesson-learned", "convention"]`
3. `["pattern", "decision"]`
4. `["pattern", "convention"]`

This list is frozen at four entries for v1. Expansion is deferred work (SR-04).

### Relationships

```
lesson-learned entry (A)  ---[Informs, weight=cosine*ppr_weight]--->  decision entry (B)

PPR seed at B → Direction::Outgoing walk → A accumulates score
               (reverse walk surfaces the lesson that informed the decision)
```

---

## User Workflows

### Workflow 1: Background tick discovers an Informs relationship

1. `run_graph_inference_tick` runs on schedule.
2. **Phase 4**: existing HNSW scan at 0.50 selects `Supports`/`Contradicts` candidates.
3. **Phase 4b**: second HNSW scan at `nli_informs_cosine_floor` (0.45). For each source entry
   whose category is a left-hand side of `informs_category_pairs`:
   - Find HNSW neighbors at cosine ≥ 0.45.
   - For each neighbor: apply cross-category, temporal ordering, cross-feature, and dedup
     guards. Discard failing pairs.
   - Surviving pairs are wrapped as `NliCandidatePair::Informs` with full `InformsCandidate`
     metadata.
4. **Phase 5**: combined candidate count (Supports/Contradicts + Informs) is capped to
   `max_graph_inference_per_tick`. Informs candidates are second-priority; they consume
   remaining budget. Discarded count is logged.
5. **Phase 7**: merged batch sent to rayon. `score_batch` scores all pairs. `NliScores`
   (entailment, neutral, contradiction) populated for every element.
6. **Phase 8**: `SupportContradicts` variants routed to existing write logic.
7. **Phase 8b**: `Informs` variants evaluated: if `nli.neutral > 0.5` and all
   `InformsCandidate` metadata guards confirmed, write `Informs` edge via `write_nli_edge`.
8. Logged: candidates processed, candidates cap-dropped, edges written.

### Workflow 2: PPR query surfaces a lesson-learned from a decision seed

1. Agent submits a query; context pipeline builds `TypedRelationGraph`.
2. `personalized_pagerank` is called with a seed set containing a decision entry (e.g.,
   entry #2060, ADR about migration connection sequencing).
3. Power iteration traverses `Direction::Outgoing` on each node, accumulating mass via four
   edge types: `Supports`, `CoAccess`, `Prerequisite`, `Informs`.
4. The lesson-learned entry (e.g., entry #376, DDL-before-migration lesson) has an `Informs`
   edge pointing to the decision entry. Mass flows backward from the decision seed to the
   lesson node.
5. The lesson-learned entry receives a non-zero PPR score and is included in re-ranked results.

### Workflow 3: Operator customizes category pairs

1. Operator edits `[inference]` section of the project TOML config.
2. Sets `informs_category_pairs = [["lesson-learned", "decision"]]` to restrict to only the
   highest-signal pair.
3. Config merge applies project-over-global semantics (non-default project value wins).
4. Next tick uses the overridden pair list; `nli_detection_tick.rs` receives the list as
   config — no code change, no domain strings in detection logic.

---

## Constraints

### Technical (Non-Negotiable)

**C-01 (No schema migration)**: `GRAPH_EDGES.relation_type` is free-text with no CHECK
constraint. No DDL change is required or permitted. Inserting `"Informs"` requires no migration.
*Risk*: if a CHECK constraint was added post-ASS-034, insertion would fail silently or error.
Delivery must confirm the column definition before Phase C work begins.

**C-02 (No new ML model)**: Must reuse the existing `NliServiceHandle` / `CrossEncoderProvider`.
No new ONNX session or model file.

**C-03 (No new tick)**: Detection runs inside `run_graph_inference_tick`. No new background
task, timer, or Tokio task is registered.

**C-04 (W1-2 contract)**: All `score_batch` calls run via `rayon_pool.spawn()`. No inline
async NLI. No `spawn_blocking`.

**C-05 (C-14 / R-09 — rayon sync-only)**: The rayon closure in Phase 7 must remain
synchronous CPU-bound. No `tokio::runtime::Handle::current()` and no `.await` inside any
rayon-spawned closure after this change.

**C-06 (SR-01 — penalty exclusion)**: `graph_penalty` and `find_terminal_active` filter
exclusively to `Supersedes` via `edges_of_type`. `Informs` must not be visible to penalty
logic. This invariant is documented as an explicit boundary (not just an AC) so future
crt-03x additions cannot silently include `Informs` in penalty traversal.

**C-07 (AC-02 / edges_of_type boundary)**: All PPR traversal via `edges_of_type()` — no
direct `.edges_directed()` calls in traversal functions. Established by ADR-001 crt-021
(entry #2417).

**C-08 (max_graph_inference_per_tick sole throttle)**: The `Informs` HNSW scan shares the
existing `max_graph_inference_per_tick` budget. No new top-level cap field. `Informs`
candidates are second-priority and consume remaining budget after `Supports`/`Contradicts`
candidates are allocated (OQ-1 resolution).

**C-09 (neutral threshold is fixed)**: `nli.neutral > 0.5` is a fixed constant in detection
logic. It is not configurable. Adding a config field for the neutral threshold would
parameterize the model output, not the domain — out of scope for v1 (OQ-4 resolution).

**C-10 (default pairs frozen at four)**: The default `informs_category_pairs` list is frozen
at four software engineering pairs for v1. Expansion requires a new feature scope (SR-04).

**C-11 (discriminator tag is typed)**: The merged Phase 7 batch element type is a tagged
union (`NliCandidatePair`) with compile-time variant matching. Parallel lists matched by
index are prohibited — this is the failure mode identified in SR-08.

**C-12 (domain strings in config only)**: Category strings that constitute domain vocabulary
(`"lesson-learned"`, `"decision"`, `"pattern"`, `"convention"`) must not appear as string
literals in `nli_detection_tick.rs`. They live only in `InferenceConfig` serde/default code.

**C-13 (weight finitude)**: All `Informs` edge weights must be validated finite before write
(NaN / ±Inf rejected), consistent with ADR-001 crt-021 (entry #2417).

**C-14 (PPR direction — Outgoing is reverse walk)**: The fourth `edges_of_type` call for
`Informs` in `personalized_pagerank` must use `Direction::Outgoing`, consistent with the
established reverse-walk contract (entry #3744). `Direction::Incoming` would silently produce
zero mass flow from lesson nodes to decision seeds.

### Dependency

**C-15 (crt-036 must merge first)**: crt-036 (Intelligence-Driven Retention Framework) must
be merged to main before crt-037 delivery begins Phase C work. Zero technical blocker exists
in detection logic, but logistical precedence is stated in GH#466. Track crt-036 merge
status at delivery gate-in.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `crt-036` | Feature (logistical) | Must merge first per GH#466 |
| `unimatrix-engine/src/graph.rs` | Source file | Add `RelationType::Informs` variant; extend `as_str()`, `from_str()` |
| `unimatrix-engine/src/graph_ppr.rs` | Source file | Add fourth `edges_of_type` call in `personalized_pagerank` and `positive_out_degree_weight` |
| `unimatrix-server/src/services/nli_detection_tick.rs` | Source file | Add Phase 4b, Phase 8b; extend Phase 7 batch type |
| `unimatrix-server/src/infra/config.rs` | Source file | Add three `InferenceConfig` fields with serde defaults and `validate()` range checks |
| `unimatrix-store` (Store) | Crate | Add `query_existing_informs_pairs` method |
| `NliServiceHandle` / `CrossEncoderProvider` | Existing infrastructure | Reused; no change |
| `write_nli_edge` (`nli_detection.rs:532`) | `pub(crate)` function | Reused; no change |
| `EDGE_SOURCE_NLI` constant | Named constant | Reused; entry #3591 |
| `current_timestamp_secs()` | `pub(crate)` helper | Reused; no change |
| `petgraph` (via `unimatrix-engine`) | Crate dependency | `Direction::Outgoing` enum variant; already present |

---

## NOT in Scope

The following are explicitly excluded to prevent scope creep:

- **Config-extensible relation types** (`[[inference.relation_types]]` TOML blocks). Full
  extensibility design is deferred per SCOPE.md §Non-Goals.
- **Schema migration**. `"Informs"` is stored as a free-text string; no DDL change.
- **`Extended(String)` or open-ended `RelationType` variants**. `Informs` is a named enum
  variant; open extension is deferred.
- **Changes to `run_post_store_nli`**. `Informs` detection is background-tick-only; not
  triggered on `context_store`.
- **Changes to the `Contradicts` detection path or `suppress_contradicts`**.
- **Textual reference extraction (`Mentions` edges)**.
- **Feature co-membership detection (`ImplementsDecision` edges)**.
- **LLM-at-store-time annotation**. Annotation is a caller responsibility.
- **Changes to graph compaction**, build order, or `VECTOR_MAP`.
- **Changes to `write_inferred_edges_with_cap`**. The `Informs` write path uses `write_nli_edge`
  directly, not `write_inferred_edges_with_cap`.
- **ICD as a delivery gate criterion**. ICD is a post-delivery tracking metric. The gate
  requires functional correctness + zero regression only (OQ-5 resolution).
- **Configurable neutral threshold**. The `nli.neutral > 0.5` threshold is a fixed constant.
- **Symmetric dedup in `query_existing_informs_pairs`**. Dedup is directional
  `(source_id, target_id)` only (OQ-3 resolution).
- **A fifth or additional default `informs_category_pairs` entry**. Frozen at four for v1.

---

## Open Questions Surfaced During Specification

**OQ-S1 (schema assumption verification)**: SCOPE.md and the risk assessment both note the
assumption that `GRAPH_EDGES.relation_type` has no CHECK constraint. This must be confirmed by
the architect against the current DDL before Phase C work begins. If a constraint exists,
adding `"Informs"` requires a schema migration, which is blocked by C-01.

**OQ-S2 (NliScores.neutral computation)**: The risk assessment flags that if the NLI
cross-encoder model returns only two-class output (entailment / contradiction), then
`NliScores.neutral` may equal `1 - entailment - contradiction` and carry higher noise. The
composite guard (`neutral > 0.5`) may have lower precision than assumed. The architect should
confirm the actual model output dimensionality and whether the neutral value is a direct model
logit or a residual.

**OQ-S3 (Phase 4b scan scope)**: FR-08 specifies the HNSW scan is filtered to source entries
whose category is a left-hand side in `informs_category_pairs`. The scan must determine
source-candidate categories before the HNSW call. Confirm whether `select_source_candidates`
currently returns category metadata alongside IDs, or whether a join/secondary lookup is
needed. If a secondary lookup is needed, the HNSW embedding fetch and category fetch may need
to be combined or sequenced explicitly.

**OQ-S4 (Phase 5 cap priority ordering)**: C-08 specifies that `Supports`/`Contradicts`
candidates are first-priority and `Informs` takes remaining budget. The precise point in the
tick where the cap split is computed (before Phase 4b scan, after Phase 4b scan, or at batch
merge time) affects whether Phase 4b can produce zero candidates due to full cap. The architect
should specify the cap split point to avoid a tick where 100 Supports candidates leave zero
budget for Informs without any log signal.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 17 entries; relevant: entry #2417
  (ADR-001 crt-021 typed edge weight model and edges_of_type boundary), entry #3656 (ADR-001
  crt-029 nli_detection_tick module structure), entry #3658 (ADR-003 crt-029 source-candidate
  bound from max_graph_inference_per_tick), entry #3826 (ADR-004 crt-034
  max_co_access_promotion_per_tick InferenceConfig pattern).
- Queried: `mcp__unimatrix__context_search` (three queries) — additionally retrieved entry
  #3744 (PPR Direction::Outgoing reverse-walk pattern, confirmed direction contract for AC-05
  and C-14).
