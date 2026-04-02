# SPECIFICATION: crt-041 — Graph Enrichment (S1, S2, S8 Edge Sources)

## Objective

crt-041 adds three background tick edge sources — S1 (tag co-occurrence), S2 (structural
vocabulary), and S8 (search co-retrieval) — to bulk-enrich the GRAPH_EDGES table. The goal is
to raise total non-bootstrap edge count from ~1,086 toward ≥3,000 active-to-active edges,
enabling the PPR graph expander (Group 4) to traverse cross-category entries that lie outside
the HNSW k=20 candidate set. All three sources are pure SQL with no ML model; they run as
infallible background tick operations following the established `run_co_access_promotion_tick`
pattern.

---

## Functional Requirements

### S1 — Tag Co-Occurrence Edge Source

**FR-01** The system shall execute an S1 tick on every background tick cycle that queries the
`entry_tags` table for pairs of active entries sharing ≥3 tags via a SQL self-join, and
inserts `Informs` edges into `graph_edges` for each qualifying pair.

**FR-02** The S1 tick shall use `INSERT OR IGNORE` against the `UNIQUE(source_id, target_id,
relation_type)` constraint, making it idempotent across repeated runs on the same corpus.

**FR-03** The S1 tick SQL shall JOIN the `entries` table on BOTH `source_id` and `target_id`
endpoints, filtering to `status = 0` (Active) on both sides. Entries with any other status
(Deprecated = 1, Superseded = 2, Quarantined = 3) shall not appear as either endpoint.

**FR-04** Each S1 edge shall be written with:
- `relation_type = 'Informs'`
- `source = EDGE_SOURCE_S1` (constant value `'S1'`)
- `weight = min(shared_tag_count * 0.1, 1.0)` (range: 0.3 for 3 shared tags, capped at 1.0
  for 10+ shared tags)
- `created_by = 's1'`
- `bootstrap_only = 0`

**FR-05** The S1 SQL shall apply `ORDER BY shared_tag_count DESC LIMIT ?` so that the
highest-overlap pairs are prioritized when the result set exceeds `max_s1_edges_per_tick`.

**FR-06** S1 shall emit a `tracing::info!` summary log on completion reporting the number of
edges written and the number of candidate pairs evaluated. On SQL error, S1 shall log at
`warn!` and return without panicking (infallible tick pattern).

### S2 — Structural Vocabulary Edge Source

**FR-07** The system shall execute an S2 tick on every background tick cycle that dynamically
constructs a SQL query from `inference_config.s2_vocabulary`, finding pairs of active entries
sharing ≥2 terms from that vocabulary, and inserts `Informs` edges into `graph_edges`.

**FR-08** When `s2_vocabulary` is empty, the S2 tick shall return immediately with zero SQL
writes and zero log output beyond a debug trace. It shall not error or panic.

**FR-09** The S2 vocabulary term matching shall use the space-padded word-boundary pattern
via SQLite `instr()`:

```
instr(lower(' ' || e.content || ' ' || e.title || ' '), lower(' ' || ? || ' ')) > 0
```

Each vocabulary term shall be bound as a separate sqlx query parameter — never interpolated
as a string literal into the SQL (prevents SQL injection; SR-01).

**FR-10** The S2 tick shall use `INSERT OR IGNORE`, making it idempotent.

**FR-11** The S2 tick SQL shall JOIN `entries` on BOTH endpoints with `status = 0` (same
dual-endpoint quarantine guard as S1; FR-03).

**FR-12** Each S2 edge shall be written with:
- `relation_type = 'Informs'`
- `source = EDGE_SOURCE_S2` (constant value `'S2'`)
- `weight = min(shared_term_count * 0.1, 1.0)`
- `created_by = 's2'`
- `bootstrap_only = 0`

**FR-13** S2 shall apply `ORDER BY shared_term_count DESC LIMIT ?` to prioritize
highest-overlap pairs up to `max_s2_edges_per_tick`.

**FR-14** S2 shall emit a `tracing::info!` summary log on completion with edges written and
candidates evaluated. On SQL error, log at `warn!` and return (infallible).

### S8 — Search Co-Retrieval Edge Source

**FR-15** The system shall execute an S8 batch on ticks where
`current_tick % s8_batch_interval_ticks == 0`. S8 shall be skipped entirely on other ticks.

**FR-16** The S8 batch shall load the persistent watermark from the `counters` table under
key `'s8_audit_log_watermark'` (integer, default 0 when absent). Only `audit_log` rows with
`event_id > watermark` shall be processed.

**FR-17** The S8 batch shall query `audit_log` for rows where:
- `operation = 'context_search'`
- `outcome = 0` (Success)
- `event_id > watermark`

Rows with `operation != 'context_search'` (including `context_briefing`) and rows with
`outcome != 0` (failed or denied searches) shall be excluded.

**FR-18** For each qualifying `audit_log` row, the system shall parse `target_ids` as a JSON
array of `u64`. All unordered pairs `(a, b)` where `a < b` among the returned entry IDs shall
be candidate S8 pairs. On JSON parse failure for a given row, the system shall log the
`event_id` at `warn!` and advance the watermark past that row (no indefinite re-processing).

**FR-19** Each candidate pair shall be validated: both entry IDs must exist in `entries` with
`status = 0` (Active). Pairs where either endpoint is Quarantined, Deprecated, Superseded, or
absent shall be discarded without an INSERT.

**FR-20** Qualifying pairs shall be inserted with `INSERT OR IGNORE` into `graph_edges` with:
- `relation_type = 'CoAccess'`
- `source = EDGE_SOURCE_S8` (constant value `'S8'`)
- `weight = 0.25` (fixed; per ASS-038 GNN feature spec)
- `created_by = 's8'`
- `bootstrap_only = 0`

**FR-21** After all inserts succeed, the S8 batch shall update the `counters` table to set
`s8_audit_log_watermark = max(event_id processed in this batch)`. The watermark update shall
occur AFTER edge writes, not before. If the process crashes between edge writes and the
watermark update, S8 re-processes the same batch on the next run; `INSERT OR IGNORE` handles
the duplicate gracefully (at-least-once re-processing guarantee).

**FR-22** The S8 batch shall process at most `max_s8_pairs_per_batch` pairs per run. When the
candidate pair count exceeds this cap, pairs shall be taken in `event_id ASC` order to ensure
forward progress.

**FR-23** S8 shall emit a `tracing::info!` summary log on batch completion reporting pairs
written, pairs skipped (quarantined endpoints), and new watermark value. On SQL or parse error,
log at `warn!` and return (infallible).

### Named Constants

**FR-24** The following named constants shall be defined in `unimatrix-store::read` and
re-exported from `unimatrix-store::lib`:

| Constant            | Value      |
|---------------------|------------|
| `EDGE_SOURCE_S1`    | `"S1"`     |
| `EDGE_SOURCE_S2`    | `"S2"`     |
| `EDGE_SOURCE_S8`    | `"S8"`     |

These follow the pattern established by `EDGE_SOURCE_NLI` (col-029 ADR-001) and
`EDGE_SOURCE_CO_ACCESS` (crt-034 ADR-002).

### InferenceConfig Fields

**FR-25** `InferenceConfig` in `crates/unimatrix-server/src/infra/config.rs` shall gain five
new fields:

| Field                      | Type          | Default   | Valid Range   |
|----------------------------|---------------|-----------|---------------|
| `s2_vocabulary`            | `Vec<String>` | `[]` (empty; operator opt-in) | any length |
| `max_s1_edges_per_tick`    | `usize`       | `200`     | [1, 10000]    |
| `max_s2_edges_per_tick`    | `usize`       | `200`     | [1, 10000]    |
| `s8_batch_interval_ticks`  | `u32`         | `10`      | [1, 1000]     |
| `max_s8_pairs_per_batch`   | `usize`       | `500`     | [1, 10000]    |

The config comment for `s2_vocabulary` shall document the recommended 9-term
software-engineering starting point: `["migration", "schema", "performance", "async",
"authentication", "cache", "api", "confidence", "graph"]`.

**FR-26** Both `InferenceConfig::default()` (the `impl Default` block) and each field's
`#[serde(default = "fn")]` backing function shall be updated atomically and shall agree on the
same default values (SR-07; dual-maintenance trap documented in entry #4011).

**FR-27** The `validate()` method on `InferenceConfig` shall enforce the valid ranges above,
returning an error at startup for out-of-range values.

### Module Structure and Tick Ordering

**FR-28** All S1, S2, and S8 logic shall live in a new module:
`crates/unimatrix-server/src/services/graph_enrichment_tick.rs`.

**FR-29** The module shall expose a single public entry point:
`pub async fn run_graph_enrichment_tick(store, config, current_tick)` that calls S1, then S2,
then conditionally S8 in that fixed order.

**FR-30** `run_graph_enrichment_tick` shall be called from `run_single_tick` in `background.rs`
immediately after `run_graph_inference_tick` (structural_graph_tick). The updated tick ordering
shall be:

```
compaction
→ co_access_promotion
→ graph-rebuild
→ PhaseFreqTable::rebuild
→ contradiction_scan      (if embed adapter ready && tick_multiple)
→ extraction_tick
→ structural_graph_tick   (run_graph_inference_tick — always)
→ graph_enrichment_tick   (S1 always; S2 always; S8 every N ticks)
```

**FR-31** The tick ordering invariant comment in `background.rs` shall be updated to include
`graph_enrichment_tick` after `structural_graph_tick`.

### write_graph_edge Prerequisite

**FR-32** Before implementing S1/S2/S8 call sites, the delivery agent shall verify that
`write_graph_edge(source: &str, ...)` exists in `unimatrix-store`. If crt-040 shipped without
this function, crt-041 shall add it as the first implementation step. The existing
`write_nli_edge` must not be reused — it hardcodes `source='nli'` and `created_by='nli'`,
which would silently retag S1/S2/S8 edges as NLI-origin (entry #4025).

### GraphCohesionMetrics Fields

**FR-33** `GraphCohesionMetrics` (in `unimatrix-store::read`) already exposes two fields
added by col-029 (ADR-004, entry #3592) that serve as the crt-041 eval gate:

- `cross_category_edge_count: u64` — count of edges (bootstrap_only=0) where the `entries`
  record for `source_id` and the `entries` record for `target_id` have different `category`
  values. Computed by joining `graph_edges` to `entries` on both endpoints and filtering
  `e1.category != e2.category`.

- `isolated_entry_count: u64` — count of active entries that have no row in `graph_edges` as
  either `source_id` or `target_id` (degree = 0 across all relation types).
  Computed in Rust as `active_entry_count - connected_entry_count`.

No changes to `GraphCohesionMetrics` or `compute_graph_cohesion_metrics()` are required by
crt-041. The eval gate reads these existing fields. These fields appear in `context_status`
output and serve as the primary eval gate signals for crt-041.

---

## Non-Functional Requirements

**NFR-01 No ML model.** S1, S2, and S8 are pure SQL. No `rayon_pool.spawn()`, no ONNX
runtime calls, no `spawn_blocking` for computation. All database access uses
`store.write_pool_server()` directly, consistent with the `run_co_access_promotion_tick`
pattern.

**NFR-02 Infallible tick pattern.** No tick function (S1, S2, S8) may panic or propagate an
error that halts the tick loop. All SQL errors, JSON parse failures, and watermark update
failures shall be logged at `warn!` and the function shall return normally.

**NFR-03 Latency budget.** The combined S1+S2 execution on a corpus of ≤1,200 entries must
complete within 500 ms per tick (measured from first SQL call to last INSERT OR IGNORE
confirmation). S8, when it runs, must complete within 1,000 ms per batch. These bounds are
verified by integration tests with a synthetic corpus.

**NFR-04 No schema migration.** All three sources write to the existing `graph_edges` table
(schema v19). The S8 watermark uses the existing `counters` table. Schema version remains 19;
no migration file is required.

**NFR-05 Idempotency.** Running S1, S2, or S8 on the same corpus multiple times shall produce
no duplicate edges. `INSERT OR IGNORE` on the `UNIQUE(source_id, target_id, relation_type)`
constraint is the mechanism.

**NFR-06 Backward compatibility.** `inferred_edge_count` in `GraphCohesionMetrics` shall
continue to count only edges with `source = 'nli'`. S1, S2, and S8 edges are not counted in
that field. Existing metric semantics are unchanged.

**NFR-07 File size limit.** `graph_enrichment_tick.rs` shall not exceed 500 lines. If the
combined implementation (excluding tests) exceeds 500 lines, test helpers shall be extracted
to a `graph_enrichment_tick_tests.rs` sibling file, following the pattern in
`co_access_promotion_tick`.

**NFR-08 Eval gate.** After delivery, running at least one full background tick shall result
in `cross_category_edge_count` increasing and `isolated_entry_count` decreasing relative to
pre-delivery baseline values in `context_status`. MRR on
`product/research/ass-039/harness/scenarios.jsonl` shall not regress below the 0.2875
baseline. The eval gate must be run after at least one complete tick, not immediately after
server start (new edges are not visible in the PPR graph until `TypedGraphState::rebuild` runs
on the following tick; SR-09).

---

## Acceptance Criteria

**AC-01** S1 tick writes `Informs` edges (with `source = 'S1'`) for every active→active pair
sharing ≥3 tags. Verified by: insert a synthetic corpus with known tag overlap ≥3 on at least
one pair, run one tick, assert the edge appears in `graph_edges` with correct `source`,
`relation_type`, and `weight`.

**AC-02** S1 is idempotent: running S1 twice on the same corpus produces no duplicate edges.
Verified by: run the S1 tick twice on a fixed corpus, assert edge count is identical after the
second run.

**AC-03** S1 excludes quarantined-endpoint pairs. Verified by: mark one entry in a qualifying
pair as Quarantined (status=3), run S1, assert no edge is written for any pair where that
entry is an endpoint.

**AC-04** S1 respects `max_s1_edges_per_tick`: given a corpus producing N qualifying pairs
where N > cap, at most `max_s1_edges_per_tick` edges are written per tick. The written edges
correspond to the highest-overlap pairs (ORDER BY shared_tag_count DESC). Verified by: set
cap to 5, synthesize 10+ qualifying pairs with distinct overlap counts, run one tick, assert
≤5 edges written and all 5 correspond to highest-overlap pairs.

**AC-05** S1 weight is `min(shared_tag_count * 0.1, 1.0)`. Verified by: assert weight=0.3 for
3 shared tags, weight=0.5 for 5, weight=1.0 for 10, weight=1.0 for 12.

**AC-06** S2 tick writes `Informs` edges (with `source = 'S2'`) for active→active pairs
sharing ≥2 terms from `s2_vocabulary`. Verified by: configure vocabulary with 3 terms, insert
entries sharing 2 of those terms, run one tick, assert edge present with correct source.

**AC-07** S2 is a no-op when `s2_vocabulary` is empty: zero SQL writes, zero edges inserted.
Verified by: set `s2_vocabulary = []`, run S2, assert `graph_edges` row count unchanged.

**AC-08** S2 is idempotent: running S2 twice on the same corpus and vocabulary produces no
duplicate edges. Verified by: run S2 tick twice, assert row count unchanged after second run.

**AC-09** S2 excludes quarantined-endpoint pairs. Verified by the same pattern as AC-03,
applied to S2.

**AC-10** S2 vocabulary term matching uses space-padded word boundary (not raw substring).
Verified by: include term `"api"` in vocabulary, insert an entry whose content contains only
`"capabilities"` (not the standalone word `"api"`), run S2, assert no edge is written for
that entry (no false positive).

**AC-11** S2 vocabulary terms are bound as sqlx parameters, never interpolated. Verified by:
include a term containing a single quote (e.g., `"it's"`), run S2, assert no SQL error occurs
and results match expected pairs (injection guard, SR-01).

**AC-12** S2 respects `max_s2_edges_per_tick`. Verified by the same pattern as AC-04, applied
to S2.

**AC-13** S8 tick runs only on ticks where `current_tick % s8_batch_interval_ticks == 0`.
Verified by: set `s8_batch_interval_ticks = 5`, advance a mock tick counter to tick 4, run
the tick, assert zero `graph_edges` rows with `source='S8'`; then advance to tick 5, run
again, assert S8 rows appear.

**AC-14** S8 writes `CoAccess` edges with `source = 'S8'` and `weight = 0.25` for pairs
co-appearing in `context_search` audit_log rows. Verified by: insert two `context_search`
audit_log rows sharing entry IDs in `target_ids`, run S8, assert edges appear with correct
source, relation_type, and weight.

**AC-15** S8 watermark persists across runs: only `event_id > watermark` rows are processed.
Verified by: run S8 once (processes rows 1..5, watermark becomes 5), insert row 6, run S8
again, assert only row 6's pairs produce new edges.

**AC-16** S8 watermark is updated after edge writes. Verified by: simulate a mid-batch crash
(test: write edges, do not update watermark), run S8 again, assert no duplicate edges (INSERT
OR IGNORE) and watermark is now updated.

**AC-17** S8 excludes non-`context_search` operations. Verified by: insert a `context_briefing`
audit_log row with target_ids, run S8, assert no edges are written for those IDs via S8.

**AC-18** S8 excludes rows with `outcome != 0`. Verified by: insert a `context_search` row
with `outcome = 1` (denied), run S8, assert no edges are written for that row's pairs.

**AC-19** S8 excludes quarantined-endpoint pairs. Verified by the same pattern as AC-03,
applied to S8.

**AC-20** S8 handles malformed JSON in `target_ids` without halting: the row with malformed
JSON is logged at `warn!`, the watermark advances past that `event_id`, and subsequent rows
are processed normally. Verified by: insert a row with `target_ids = 'not-json'` between two
valid rows, run S8, assert the valid rows' pairs are written and watermark reflects the last
valid event_id.

**AC-21** S8 respects `max_s8_pairs_per_batch`. Verified by: configure cap to 3, insert
enough audit_log rows to generate 10+ pairs, run S8, assert ≤3 edges written.

**AC-22** Named constants `EDGE_SOURCE_S1 = "S1"`, `EDGE_SOURCE_S2 = "S2"`,
`EDGE_SOURCE_S8 = "S8"` are defined in `unimatrix_store::read` and accessible from
`unimatrix_store` crate root. Verified by: `use unimatrix_store::{EDGE_SOURCE_S1,
EDGE_SOURCE_S2, EDGE_SOURCE_S8}` compiles without error.

**AC-23** `InferenceConfig` gains all five fields with matching defaults in both `impl Default`
and each `#[serde(default = "fn")]` backing function. Verified by: `InferenceConfig::default()`
returns a struct where each new field equals its documented default; deserializing an empty
TOML produces an identical result (SR-07, entry #4011).

**AC-24** `InferenceConfig::validate()` rejects out-of-range values for all five new fields.
Verified by: set each field to 0 (below minimum), assert `validate()` returns an error naming
the field.

**AC-25** All three tick functions (S1, S2, S8) emit `tracing::info!` summary logs on
completion. On SQL error, they log at `warn!` and return `Ok(())` without panicking. Verified
by: inject a broken pool handle, run each tick, assert no panic and a `warn!` log is emitted.

**AC-26** The tick ordering in `background.rs` places `run_graph_enrichment_tick` after
`run_graph_inference_tick`. Verified by: code review and integration test asserting that S1
edges written in tick N are visible in `graph_edges` after tick N completes (not before).

**AC-27** The tick ordering invariant comment in `background.rs` is updated to include
`graph_enrichment_tick` after `structural_graph_tick`. Verified by: code review.

**AC-28** `write_graph_edge(source: &str, ...)` exists in `unimatrix-store` before any S1/S2/S8
call site is written. Delivery agent verifies this as a pre-flight check (SR-04). If absent,
adding it is the first implementation step.

**AC-29** `GraphCohesionMetrics` already exposes `cross_category_edge_count` (edges where
`e1.category != e2.category`) and `isolated_entry_count` (active entries with degree=0)
from col-029; crt-041 adds no new fields to this struct. Verified by: insert a synthetic
corpus with known cross-category edges and isolated entries, call
`compute_graph_cohesion_metrics`, assert both existing fields return expected values.

**AC-30** `inferred_edge_count` in `GraphCohesionMetrics` still counts only `source = 'nli'`
edges after crt-041 delivery. S1, S2, S8 edges are not counted in that field. Verified by:
insert edges for all four sources, assert `inferred_edge_count` equals only the NLI-source
count.

**AC-31** `graph_enrichment_tick.rs` is under 500 lines (excluding the test file if split).
Verified by: `wc -l` on the file at PR time.

**AC-32** At least one complete background tick runs post-delivery and `context_status` shows
`cross_category_edge_count > 0` and `isolated_entry_count` lower than pre-delivery baseline.
MRR on `product/research/ass-039/harness/scenarios.jsonl` is ≥ 0.2875. Verified by: post-
delivery eval run.

---

## Domain Model

### Ubiquitous Language

| Term | Definition |
|------|------------|
| **Edge source** | The named origin of a graph edge, stored in `graph_edges.source`. Distinguishes how each edge was inferred. Used by GNN feature construction to assign edge-type features for training (W3-1). |
| **S1** | Tag co-occurrence source. An `Informs` edge written when two active entries share ≥3 tags. Signal: structural metadata overlap. |
| **S2** | Structural vocabulary source. An `Informs` edge written when two active entries both contain ≥2 terms from the operator-configured `s2_vocabulary`. Signal: domain terminology co-occurrence in content/title. |
| **S8** | Search co-retrieval source. A `CoAccess` edge written when two active entries appear together in the result set of the same `context_search` call. Signal: behavioral co-retrieval by agents. |
| **Informs** | A directed `relation_type` value in `graph_edges` indicating that one entry provides context for another. Written by structural_graph_tick, S1, and S2. |
| **CoAccess** | A directed `relation_type` value in `graph_edges` indicating that two entries were accessed together. Written by co_access_promotion_tick and S8. |
| **Dual-endpoint quarantine guard** | The requirement that any SQL producing graph edges must JOIN `entries` on BOTH `source_id` and `target_id` and filter `status != Quarantined`. Omitting the second JOIN silently writes edges to quarantined entries (production bug documented in entry #3981). |
| **Watermark** | A persistent integer stored in the `counters` table under key `'s8_audit_log_watermark'`. Tracks the highest `audit_log.event_id` processed by S8. Enables incremental batch processing without reprocessing prior rows. |
| **Infallible tick** | A background tick function that never panics and never propagates an error that halts the tick loop. All failures are logged at `warn!` and the function returns normally. |
| **Additive-only** | The S1/S2/S8 edges accumulate over time; no reconciliation pass removes edges when tag or vocabulary overlap decreases. Edges persist until an endpoint entry is deleted or quarantined. |
| **GraphCohesionMetrics** | A struct returned by `compute_graph_cohesion_metrics()` in `unimatrix-store::read` summarizing graph health. Fields include edge counts, degree statistics, cross-category edge count, and isolation count. All fields exist from col-029; crt-041 adds no new fields to this struct. |
| **cross_category_edge_count** | Count of non-bootstrap edges where the two endpoint entries have different `category` values. Measures graph connectivity across knowledge domains. |
| **isolated_entry_count** | Count of active entries with degree=0 (no edge in `graph_edges` as either `source_id` or `target_id`). Measures graph sparsity. Computed as `active_entry_count - connected_entry_count`. |
| **EDGE_SOURCE_*** | Named string constants (`EDGE_SOURCE_S1`, `EDGE_SOURCE_S2`, `EDGE_SOURCE_S8`) defined in `unimatrix-store::read`, re-exported from crate root, holding the canonical source string values. |

### Entity Relationships

```
entries (id, status, category, content, title)
  |
  |--- entry_tags (entry_id, tag)          [used by S1]
  |
  |--- graph_edges (source_id, target_id,  [written by S1, S2, S8]
  |       relation_type, weight, source,
  |       created_by, bootstrap_only)
  |
  |--- audit_log (event_id, operation,     [read by S8]
          target_ids JSON, outcome)

counters (key, value)
  |--- 's8_audit_log_watermark' → u64      [state for S8]
```

### Edge Weight Semantics

| Source | Relation  | Weight Formula             | Range    |
|--------|-----------|----------------------------|----------|
| S1     | Informs   | `min(shared_tags * 0.1, 1.0)` | [0.3, 1.0] |
| S2     | Informs   | `min(shared_terms * 0.1, 1.0)` | [0.2, 1.0] |
| S8     | CoAccess  | `0.25` (fixed)             | 0.25     |
| NLI    | Informs   | derived from NLI score     | (0, 1]   |
| cosine | Supports  | cosine similarity          | (0, 1]   |

---

## User Workflows

### Operator Configuration (S2 Vocabulary)

An operator who wants S2 edges enabled adds `s2_vocabulary` to `config.toml`:

```toml
[inference]
s2_vocabulary = ["migration", "schema", "performance", "async",
                 "authentication", "cache", "api", "confidence", "graph"]
max_s1_edges_per_tick = 200
max_s2_edges_per_tick = 200
s8_batch_interval_ticks = 10
max_s8_pairs_per_batch = 500
```

Without this setting, S2 is disabled (empty default). S1 and S8 run unconditionally.

### Background Tick (Automated)

On every background tick, the daemon calls `run_graph_enrichment_tick`:
1. S1 queries tag co-occurrence pairs, writes Informs edges up to the cap.
2. S2 queries vocabulary co-occurrence pairs (if vocabulary non-empty), writes Informs edges.
3. If `current_tick % s8_batch_interval_ticks == 0`: S8 reads the watermark, processes new
   `context_search` audit_log rows, writes CoAccess edges, updates watermark.

New edges are not visible in the PPR graph until `TypedGraphState::rebuild` runs on the
following tick.

### Eval Gate (Post-Delivery)

After delivery, the operator or CI pipeline:
1. Starts the server and waits for at least one full tick to complete.
2. Calls `context_status` and checks `cross_category_edge_count > 0` and
   `isolated_entry_count < pre-delivery value`.
3. Runs the behavioral eval harness on `product/research/ass-039/harness/scenarios.jsonl`
   and asserts MRR ≥ 0.2875.

---

## Constraints

**C-01 No ML model.** S1, S2, and S8 must be pure SQL. No ONNX, no rayon, no
`spawn_blocking` for computation.

**C-02 No schema migration.** All writes go to existing tables (`graph_edges`, `counters`).
Schema version stays at 19.

**C-03 Dual-endpoint quarantine guard mandatory.** Every SQL that produces candidate pairs
for edge insertion must JOIN `entries` on BOTH `source_id` and `target_id` filtering
`status != Quarantined` (status = 3). This is not optional. Omitting either JOIN silently
creates edges to quarantined entries (entry #3981 production bug).

**C-04 Additive-only edges.** S1, S2, and S8 edges persist until an endpoint entry is deleted
or quarantined. No reconciliation pass removes edges when tag or vocabulary overlap decreases.
Tag-drop reconciliation is deferred.

**C-05 S2 parameterized SQL only.** Vocabulary terms must be sqlx bound parameters in every
code path. String interpolation of vocabulary terms into SQL is prohibited (SR-01).

**C-06 write_graph_edge prerequisite.** Delivery must verify `write_graph_edge(source: &str,
...)` exists before writing S1/S2/S8 call sites (SR-04). `write_nli_edge` must not be reused.

**C-07 InferenceConfig dual-maintenance.** For each of the five new fields, both the `impl
Default` struct literal and the `#[serde(default = "fn")]` backing function must be updated
in the same commit (SR-07, entry #4011). The deliver agent must verify agreement by running
`InferenceConfig::default()` and deserializing empty TOML.

**C-08 File size limit.** `graph_enrichment_tick.rs` must stay under 500 lines. Tests may be
split to a sibling `_tests.rs` file if needed.

**C-09 inferred_edge_count unchanged.** The `inferred_edge_count` field in
`GraphCohesionMetrics` counts only `source = 'nli'` edges and must not be altered by this
feature.

**C-10 S8 watermark ordering.** The watermark update must occur after edge writes complete.
Write-then-watermark ensures at-least-once re-processing on crash; `INSERT OR IGNORE`
makes re-processing safe.

**C-11 Tick ordering invariant.** `run_graph_enrichment_tick` runs after
`run_graph_inference_tick`. The invariant comment in `background.rs` must be updated.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `crt-040` | Hard prerequisite | Must be merged before crt-041 delivery begins. Provides `write_graph_edge(source: &str, ...)`. If absent, crt-041 adds it first. |
| `graph_edges` table (schema v19) | Existing schema | `UNIQUE(source_id, target_id, relation_type)` constraint provides idempotency. |
| `entry_tags` table | Existing schema | S1 self-join source. Indexes `idx_entry_tags_tag` and `idx_entry_tags_entry_id` exist. |
| `audit_log` table | Existing schema | S8 read source. `target_ids` is JSON TEXT. |
| `counters` table | Existing schema | S8 watermark storage. Key `'s8_audit_log_watermark'`. |
| `entries` table | Existing schema | Dual-endpoint status filtering. `status` column: 0=Active, 3=Quarantined. |
| `InferenceConfig` | Existing struct | Gains 5 new fields. Located in `crates/unimatrix-server/src/infra/config.rs`. |
| `GraphCohesionMetrics` | Existing struct | Gains 2 new fields. Located in `unimatrix-store::read`. |
| `EDGE_SOURCE_NLI`, `EDGE_SOURCE_CO_ACCESS` | Existing constants | Pattern to follow for `EDGE_SOURCE_S1/S2/S8`. |
| `run_co_access_promotion_tick` | Existing function | Implementation pattern to follow (infallible, write_pool_server, info!/warn! logs). |
| `sqlx` | Existing crate dependency | Parameterized query binding (required for S2 SQL injection prevention). |
| `serde_json` | Existing crate dependency | S8 `target_ids` JSON parsing. |
| `tracing` | Existing crate dependency | `info!` and `warn!` logging in tick functions. |

---

## NOT in Scope

- **PPR expander (Group 4)** — crt-041 builds edge density; expander implementation is a
  separate feature.
- **S3, S4, S5 edge sources** — Deferred until corpus ≥3,000 entries (ASS-038: <20 viable
  pairs at current size).
- **S6, S7 edge sources** — Not validated in ASS-038.
- **Behavioral Informs edges (Group 6)** — Depends on Group 5 infrastructure not yet shipped.
- **cosine Supports detection** — crt-040 (already shipped).
- **NLI Supports path modification** — No changes to contradiction detection or NLI path.
- **Supports edges from S1/S2/S8** — S1 and S2 write Informs only; S8 writes CoAccess only.
- **Tag-drop edge reconciliation** — Stale S1/S2 edges from entries that have since lost
  shared tags are not removed in this feature.
- **`inferred_edge_count` change** — Continues to count only `source='nli'` edges.
- **TypedRelationGraph or PPR scoring changes** — No modification to graph build logic.
- **New schema tables or migrations** — Schema version 19 unchanged.
- **Live config reload** — `s2_vocabulary` is read once at tick time from the immutable
  post-startup config. No live reload mechanism.
- **`signal_origin` column** — Does not exist; `graph_edges.source` is the canonical field.
  No new column added.
- **S8 processing of `context_briefing` rows** — Explicitly excluded.
- **S2 FTS5 integration** — Space-padded `instr()` pattern only; no SQLite FTS5.

---

## Open Questions for Architect

**OQ-01 (SR-02) S1 SQL LIMIT placement before GROUP BY materialization.**
The S1 self-join `GROUP BY ... HAVING COUNT(*) >= 3 ORDER BY ... LIMIT ?cap` may materialize
the full pair set before LIMIT is applied, depending on the SQLite query planner. At 10,000
entries with dense tags, the intermediate cartesian product could be millions of rows. The
architect must verify (via `EXPLAIN QUERY PLAN` or empirical timing at 10,000 entries) whether
the LIMIT fires before GROUP BY materialization, or whether the query must be restructured as
a two-phase query (pre-filter shared-tag candidates, then score). If restructuring is needed,
document the revised SQL pattern in the implementation brief.

**OQ-02 (SR-05) Category column for cross_category_edge_count.**
`cross_category_edge_count` joins `graph_edges` to `entries` on both endpoints and filters
`e1.category != e2.category`. The `category` column on `entries` is the correct field (string,
e.g., "decision", "pattern"). The architect should confirm there is no ambiguity between the
`category` column and tag-based category grouping, and that the SQL `e1.category != e2.category`
is the correct predicate.

**OQ-03 (SR-06) — RESOLVED. Compaction is source-agnostic; S1/S2/S8 edges are covered.**
Verified in `background.rs:513-515`: the compaction DELETE is:
```sql
DELETE FROM graph_edges
 WHERE source_id NOT IN (SELECT id FROM entries WHERE status != ?)
    OR target_id NOT IN (SELECT id FROM entries WHERE status != ?)
```
No filter on the `source` column. All edges whose endpoint is deleted or quarantined are
removed regardless of `source='S1'/'S2'/'S8'/'nli'/'co_access'`. The additive-only policy
(Design Decision 7) means S1/S2 edges persist while both endpoints are active — and are
cleaned when either endpoint is quarantined or deleted, exactly like all other edge types.

**OQ-04 (SR-09) Eval gate tick-wait procedure.**
The spec requires the eval gate be run after at least one full tick completes post-delivery.
The architect should confirm the recommended procedure: either (a) expose a tick-completion
signal via `context_status` (e.g., a `last_tick_at` timestamp), or (b) require a fixed wall-
clock wait based on `tick_interval_secs`. This determines how the CI eval gate can reliably
know when to query `context_status`.

**OQ-05 ASS-039 MRR baseline currency.**
The SCOPE.md eval gate references MRR 0.2875 from ASS-039 scenarios. If crt-040 shifted MRR
(cosine Supports detection), the baseline may need refreshing before crt-041 eval runs. The
architect should confirm whether the crt-040 delivery report updated the MRR baseline, and
provide the current value if it differs from 0.2875.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — retrieved 17 entries. Key findings: entry #4025
  (write_nli_edge pattern — do not reuse for new sources); entry #3592 (ADR-002 col-029 two-
  query graph cohesion metrics; isolated_entry_count computed in Rust as active - connected);
  entry #4026 (S8 watermark pattern confirmed); entry #3822 (near-threshold oscillation AC
  requirement for count-threshold promotions); entry #3591 (EDGE_SOURCE_NLI naming pattern).
  All findings incorporated into specification.
