# crt-037 Researcher Agent Report

## Task

Research the problem space for adding `RelationType::Informs` to the TypedRelationGraph —
a cross-feature institutional memory bridge connecting empirical knowledge entries
(lesson-learned, pattern) to normative decisions (decision, convention) via NLI neutral-zone
detection with configurable domain category pairs.

## Outputs

- SCOPE.md: `product/features/crt-037/SCOPE.md`

## Key Findings

### 1. Root cause confirmed at the code level

`nli_detection_tick.rs` Phase 8 calls `write_inferred_edges_with_cap`, which evaluates only
`scores.entailment` and discards `scores.contradiction` and `scores.neutral`. The `NliScores`
struct has all three fields. The neutral score is computed every tick by the existing rayon
batch and silently dropped. No structural change is needed to access it — only a new branch.

### 2. Two distinct HNSW scans required

The existing candidate scan uses `supports_candidate_threshold` (default 0.50) as the cosine
pre-filter. The `Informs` pass targets pairs in the 0.45–0.50 cosine band that currently never
reach NLI scoring. A separate HNSW scan at `nli_informs_cosine_floor` (default 0.45) scoped to
cross-category pairs is needed. This is a structural addition to Phase 4, not a modification.

### 3. RelationType enum is the gating change

`build_typed_relation_graph` Pass 2b rejects any `GRAPH_EDGES` row whose `relation_type` string
is not recognized by `RelationType::from_str()` (R-10 guard, line 289). Writing "Informs" edges
to the DB before adding the enum variant would silently drop them on every graph rebuild. The
enum variant must land before or with the tick detection change.

### 4. PPR addition is mechanical

`personalized_pagerank` and `positive_out_degree_weight` each make exactly three
`edges_of_type` calls. Both require a fourth call for `Informs`. The pattern is identical to
`Prerequisite` (which already ships as a named variant with no active write path). The
direction semantics are correct: `Informs` A→B (lesson informs decision), seeding B propagates
mass back to A via the reverse random walk already implemented.

### 5. InferenceConfig extension is well-precedented

The config struct has 25+ fields with the exact serde/validate/default pattern the three new
fields will follow. The `informs_category_pairs: Vec<[String; 2]>` field type is the only
novel structure — arrays in TOML, serde-deserializable without custom logic.

### 6. Domain agnosticism constraint is clean

The issue's requirement that category strings not appear in detection logic is satisfiable by
passing `config.informs_category_pairs` as a reference into the tick. The detection logic
checks `category_pairs.iter().any(|[src_cat, tgt_cat]| ...)` — domain-agnostic.

### 7. crt-036 dependency confirmed as logistical only

ASS-034 Finding 8 verified: edge detection reads no activity tables. `observations`,
`query_log`, and `injection_log` retention changes in crt-036 do not affect the `Informs`
detection path at all. The dependency is the issue author's stated ordering preference.

## Open Questions Raised in SCOPE.md

1. Cap budget: combined `max_graph_inference_per_tick` or a new `max_informs_per_tick` field?
2. NLI batch structure: merge Informs candidates into existing Phase 7 batch (one rayon spawn)
   or run a second rayon spawn? One spawn is preferred by W1-2 contract intent.
3. `query_existing_informs_pairs` dedup: directional `(source, target)` or symmetric?
4. `nli.neutral > 0.5` threshold: hard constant or configurable fourth field?
5. Delivery gate: quantitative ICD assertion or post-delivery eval only?

## Files Read

- `product/research/ass-034/SCOPE.md`
- `product/research/ass-034/FINDINGS.md`
- `crates/unimatrix-engine/src/graph.rs` (full)
- `crates/unimatrix-engine/src/graph_ppr.rs` (full)
- `crates/unimatrix-engine/src/graph_ppr_tests.rs` (first 50 lines)
- `crates/unimatrix-server/src/services/nli_detection_tick.rs` (full via offset reads)
- `crates/unimatrix-server/src/infra/config.rs` (InferenceConfig struct + Default impl)
- `crates/unimatrix-server/src/services/nli_detection.rs` (write_nli_edge signatures)
- `product/features/crt-036/SCOPE.md` (dependency context)
- GH issue #466 body (via gh CLI)

## Unimatrix Entries Referenced

- #3650 — TypedRelationGraph pattern (edges_of_type boundary, two-pass build)
- #3713 — lesson: supports_edge_threshold too conservative (threshold calibration history)
- #2417 — ADR-001 crt-021: Typed edge weight model (SR-01, edges_of_type invariant)
- #3591 — ADR-001 col-029: EDGE_SOURCE_NLI named constant
- #3628 — ADR-003 col-030: bidirectional Contradicts query

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- returned 19 entries; #3650, #3713, #2417
  most relevant; confirmed no prior pattern for NLI neutral-zone cross-category detection.
- Stored: entry #3937 "NLI neutral-zone score is the detection signal for cross-category
  institutional memory edges" via /uni-store-pattern — generalizes to any future feature
  adding edge types that exploit the NLI neutral zone.
