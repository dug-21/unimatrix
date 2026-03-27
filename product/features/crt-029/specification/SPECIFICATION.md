# Specification: crt-029 — Background Graph Inference (Supports Edges)

GH Issue: #412

---

## Objective

The knowledge graph currently gains `Supports` edges only during `run_post_store_nli`, which
fires once per `context_store` call and covers only the K nearest neighbours of the newly-stored
entry. Entries stored before NLI was enabled, entries with no recent neighbour writes, and
cross-category pairs that were never near-neighbours at store time remain isolated — unreachable
by PPR-based retrieval.

crt-029 adds a recurring background pass (`run_graph_inference_tick`) that systematically fills
the graph by iterating across the full active entry set using the existing NLI cross-encoder and
HNSW index. The pass is bounded, prioritised, and gated behind `nli_enabled`. Only `Supports`
edges are inferred; `Prerequisite` edge inference is deferred to W3-1.

---

## Functional Requirements

### FR-01 — New `run_graph_inference_tick` function

A new public async function `run_graph_inference_tick` must be added to
`crates/unimatrix-server/src/services/nli_detection.rs` (or a sibling module if file-size
constraints require a split; see C-08). The function:

- Returns immediately (no-op) when `nli_handle.get_provider()` returns `Err`, matching the
  no-op pattern in `maybe_run_bootstrap_promotion`.
- Fetches all `status = Active` entries as the candidate universe.
- Selects source candidates in priority order (FR-04) before calling `get_embedding`.
- Calls `get_embedding` only for selected source candidates, not for all active entries.
- Queries HNSW neighbours using `graph_inference_k` and similarity floor
  `supports_candidate_threshold`.
- Deduplicates `(A, B)` == `(B, A)` pairs.
- Pre-filters pairs where a `Supports` edge already exists in GRAPH_EDGES (skip before NLI).
- Truncates remaining pairs to `max_graph_inference_per_tick`.
- Dispatches all NLI scoring as a single `rayon_pool.spawn()` call (W1-2 contract; see C-01).
- Writes `Supports` edges when `entailment > supports_edge_threshold`.
- MUST NOT write `Contradicts` edges. The dedicated contradiction detection path is the sole
  writer of `Contradicts` edges. Mixing Contradicts writes in the tick creates a second code
  path with independent threshold drift risk (see C-13).
- Logs total edges written at `debug` level.

### FR-02 — Four new `InferenceConfig` fields

Four fields must be appended to `InferenceConfig` in `crates/unimatrix-server/src/infra/config.rs`:

| Field | Type | Default | Valid Range |
|---|---|---|---|
| `supports_candidate_threshold` | `f32` | `0.5` | `(0.0, 1.0)` exclusive |
| `supports_edge_threshold` | `f32` | `0.7` | `(0.0, 1.0)` exclusive |
| `max_graph_inference_per_tick` | `usize` | `100` | `[1, 1000]` |
| `graph_inference_k` | `usize` | `10` | `[1, 100]` |

All four fields must carry `#[serde(default = "...")]` annotations so TOML configs that omit
them use the specified defaults. All four must be included in `InferenceConfig::Default::default()`.

`graph_inference_k` is independent of `nli_post_store_k`. The post-store path is
latency-sensitive; the tick path is background. Sharing the knob creates invisible coupling.

### FR-03 — `InferenceConfig::validate()` extensions

`InferenceConfig::validate()` must be extended with:

- `supports_candidate_threshold` and `supports_edge_threshold` individually in `(0.0, 1.0)`
  exclusive.
- Cross-field invariant: `supports_candidate_threshold < supports_edge_threshold` (strict `<`);
  equal values are rejected. The boundary condition uses strict `>=` in the reject predicate
  (i.e., reject when `supports_candidate_threshold >= supports_edge_threshold`), matching the
  `nli_auto_quarantine_threshold > nli_contradiction_threshold` guard pattern.
- `max_graph_inference_per_tick` in `[1, 1000]`.
- `graph_inference_k` in `[1, 100]`.

### FR-04 — Priority ordering for candidate pair selection

Candidate pairs must be ranked before truncation to `max_graph_inference_per_tick`:

1. Cross-category pairs (source and target have different `category` values).
2. Pairs where either endpoint is an isolated entry (no existing non-bootstrap edge as source
   or target — determined via `Store::query_entries_without_edges()`).
3. Remaining pairs, ordered by HNSW similarity score descending.

This ordering maximises the observability impact per NLI call against the col-029 metrics
(`cross_category_edge_count`, `isolated_entry_count`).

### FR-05 — `Store::query_entries_without_edges()` helper

A new store method `query_entries_without_edges() -> Result<Vec<u64>>` must be added to
`unimatrix-store` (implementation in `read.rs` or a successor split module). It returns
the `id` values of all `status = Active` entries that have no non-bootstrap edge on either
endpoint. SQL form:

```sql
SELECT id FROM entries
WHERE status = 0
  AND id NOT IN (
    SELECT source_id FROM graph_edges WHERE bootstrap_only = 0
    UNION
    SELECT target_id FROM graph_edges WHERE bootstrap_only = 0
  )
```

Returns IDs only, not full entry content.

### FR-06 — Background tick call site

`run_graph_inference_tick` must be called from `background_tick_loop` in
`crates/unimatrix-server/src/services/background.rs`, after `maybe_run_bootstrap_promotion`,
gated on `inference_config.nli_enabled`. Runs on every tick — no tick-modulo interval gate.
`max_graph_inference_per_tick` is the sole throttle.

### FR-07 — Edge write conventions

All edges written by `run_graph_inference_tick` must:

- Use `INSERT OR IGNORE` for idempotency on the `UNIQUE(source_id, target_id, relation_type)`
  constraint.
- Set `source = EDGE_SOURCE_NLI` (the constant `"nli"` from `unimatrix-store::read`; see
  col-029 ADR-001, Unimatrix entry #3591).
- Set `bootstrap_only = false`.

### FR-08 — Supports-only NLI write pass per pair

For each scored pair, only the `entailment` score is evaluated for edge writing. Although
the NLI call returns both `entailment` and `contradiction` scores, the tick evaluates only
`entailment` — the `contradiction` score is discarded. The tick MUST NOT write `Contradicts`
edges (see C-13). Cap logic must not be inlined; it must be extracted into a unit-testable
function (SR-08 risk mitigation).

### FR-09 — Per-tick edge cap

The total number of edges written per tick must not exceed `max_graph_inference_per_tick`. The
cap counts Supports edges only — the tick writes no other edge types. Processing stops as soon
as the cap is reached.

### FR-10 — Source-candidate bound before embedding lookup

Source candidates must be independently bounded before any call to `get_embedding`. The source
candidate count must not exceed `max_graph_inference_per_tick`. This bound is enforced as a
separate selection step, prior to HNSW queries and prior to NLI scoring. This is the primary
mitigation for SR-02 (unbounded O(N) embedding scans).

---

## Non-Functional Requirements

### NFR-01 — TICK_TIMEOUT compliance

`run_graph_inference_tick` must complete within the existing `TICK_TIMEOUT` constant. At the
default cap of 100 pairs × ~0.5 ms/pair, NLI time is approximately 50 ms per tick — well within
budget. Operators who raise `max_graph_inference_per_tick` accept proportionally higher tick
latency.

### NFR-02 — W1-2 rayon contract (mandatory)

All `CrossEncoderProvider::score_batch` invocations within `run_graph_inference_tick` must
go via `rayon_pool.spawn()`. Never inline in async context. Never via `spawn_blocking`. This is
a hard constraint inherited from the existing NLI architecture (see nli_detection.rs file header
and SCOPE.md §Constraints).

### NFR-03 — No new crate dependencies

The implementation uses only `unimatrix-core`, `unimatrix-embed`, `unimatrix-store`, `sqlx`,
`tokio`, and `tracing`. No new external crates.

### NFR-04 — No schema migration

All required GRAPH_EDGES columns (`source`, `metadata`, `bootstrap_only`) exist in the current
schema. No `ALTER TABLE`, no schema version bump.

### NFR-05 — File size limit

`nli_detection.rs` is currently ~650 lines. If adding `run_graph_inference_tick` and helpers
would push the combined file past 800 lines, split the new function into
`nli_detection_tick.rs`. Do not merge a file exceeding 800 lines. The split is a judgment call
at implementation time; the 500-line soft target from `rust-workspace.md` is acknowledged but
the primary hard gate is 800 lines.

### NFR-06 — Rayon pool contention

`run_graph_inference_tick` and `run_post_store_nli` contend on the same rayon pool. The tick
must degrade gracefully (yield, not block) under pool saturation. The per-tick cap ensures the
tick's rayon dispatch is a single bounded batch, minimising contention duration. Pool behaviour
under concurrent post-store calls should be documented in the architect's ADR.

### NFR-07 — GRAPH_EDGES pre-filter query

The pre-filter that loads existing `Supports` edges into a `HashSet` uses an indexed lookup
on `GRAPH_EDGES`. The `UNIQUE(source_id, target_id, relation_type)` constraint implies an index
on these columns. At large graph sizes this is still a full table scan in memory; this is
acceptable for current scale. A covering index optimisation is a W3 concern and is not in scope
here. The architect must confirm the index exists and document the scale boundary.

---

## Acceptance Criteria

Each criterion carries the AC-ID from SCOPE.md for traceability. Additional criteria added by
this specification are marked with a dagger (†).

**AC-01** — `InferenceConfig` has four new fields: `supports_candidate_threshold: f32` (default
0.5), `supports_edge_threshold: f32` (default 0.7), `max_graph_inference_per_tick: usize`
(default 100), `graph_inference_k: usize` (default 10).
Verification: `cargo test -- test_inference_config_defaults`; assert each default value.

**AC-02** — `InferenceConfig::validate()` rejects configs where
`supports_candidate_threshold >= supports_edge_threshold` (strict `>=` predicate; equal values
are rejected).
Verification: unit test with equal values (e.g., both 0.7) must return `Err`.

**AC-03** — `InferenceConfig::validate()` rejects `supports_candidate_threshold` or
`supports_edge_threshold` outside `(0.0, 1.0)` exclusive (i.e., 0.0 and 1.0 are invalid).
Verification: unit tests with values 0.0 and 1.0 for each field must return `Err`.

**AC-04** — `InferenceConfig::validate()` rejects `max_graph_inference_per_tick` outside
`[1, 1000]` (0 and 1001 are invalid).
Verification: unit tests with values 0 and 1001 must return `Err`.

**AC-04b** — `InferenceConfig::validate()` rejects `graph_inference_k` outside `[1, 100]` (0
and 101 are invalid).
Verification: unit tests with values 0 and 101 must return `Err`.

**AC-05** — `run_graph_inference_tick` returns immediately (no-op) when
`nli_handle.get_provider()` returns `Err`. No store queries, no embeddings fetched.
Verification: unit test with a stub NliServiceHandle that always returns `Err`.

**AC-06** — `run_graph_inference_tick` queries only `status = Active` entries and uses HNSW
similarity > `supports_candidate_threshold` as the pre-filter. Uses `graph_inference_k` as the
HNSW neighbour count, independent of `nli_post_store_k`.
Verification: integration test verifying that a `Deprecated` entry is never a candidate source
or target; config with `graph_inference_k != nli_post_store_k` uses the tick-specific value.

**AC-06b** — Pairs where a `Supports` edge already exists in GRAPH_EDGES are skipped before NLI
scoring (pre-filter, not `INSERT OR IGNORE`). Existing confirmed pairs consume zero NLI budget.
Verification: unit test seeds an existing `Supports` edge; assert NLI scorer is not called for
that pair.

**AC-06c** — Source candidates are capped to `max_graph_inference_per_tick` BEFORE any
`get_embedding` call — not after. The cap is applied in `select_source_candidates` (Phase 3)
which operates on metadata only (IDs and category strings). `get_embedding` is called in Phase
4 only for the already-capped list. The ordering is: cap first, then fetch embeddings.
Verification: unit test with N active entries and cap=M asserts `get_embedding` is called at
most M times per tick, and that no `get_embedding` call precedes the completion of
`select_source_candidates`.

**AC-07** — Candidate pairs are processed in priority order: cross-category pairs first,
isolated entries (no existing non-bootstrap edges) second, remaining pairs by similarity
descending. When the cap is hit, lower-priority pairs are dropped.
Verification: unit test with a mix of cross-category, isolated, and same-category pairs and a
small cap asserts cross-category pairs are retained preferentially.

**AC-08** — For each tick, all NLI inference is dispatched as a single `rayon_pool.spawn()`
call (W1-2 contract). No `spawn_blocking`, no inline async NLI calls.
Verification: code review + clippy; unit test structure demonstrates single dispatch.

**AC-09** — A `Supports` edge `(A, B, "Supports")` is written when `score(A→B).entailment >
supports_edge_threshold` (strict `>`). Uses `INSERT OR IGNORE`.
Verification: unit test with mock scores above and at threshold; at-threshold pair must not
produce an edge.

**AC-10a** — TICK MUST NOT WRITE CONTRADICTS — `write_inferred_edges_with_cap` is a
Supports-only function. It accepts no `contradiction_threshold` parameter and writes no
`Contradicts` edges. The dedicated contradiction detection path (`run_post_store_nli` and the
contradiction scan in `infra/contradiction.rs`) is the sole writer of `Contradicts` edges.
Any `Contradicts` write in `nli_detection_tick.rs` is a gate-blocking defect.
Verification: `grep -n 'Contradicts' nli_detection_tick.rs` must return empty; code review
confirms `write_inferred_edges_with_cap` signature has no `contradiction_threshold` parameter.

**AC-11** — Total edges written per tick is bounded by `max_graph_inference_per_tick`. The cap
counts Supports edges only (the tick writes no other edge types).
Verification: unit test with cap=3 and 10 high-scoring pairs asserts exactly 3 edges written.

**AC-13** — All written edges carry `source = EDGE_SOURCE_NLI` ("nli").
Verification: integration test queries GRAPH_EDGES after tick; assert `source = 'nli'` on all
rows inserted by the tick.

**AC-14** — `run_graph_inference_tick` is called from `background_tick_loop` after
`maybe_run_bootstrap_promotion`, gated on `inference_config.nli_enabled`.
Verification: code review of `background.rs`; integration test with `nli_enabled = false`
asserts the function is not invoked.

**AC-15** — `Store::query_entries_without_edges()` returns the IDs of all active entries with no
non-bootstrap edge on either endpoint (source_id or target_id).
Verification: unit test seeds entries with and without edges; assert only edge-free entry IDs
are returned.

**AC-16** — Unit tests cover: no-NLI no-op, cross-category priority ordering, isolated-entry
priority, edge cap enforcement, pre-filter skips pairs with existing `Supports` edges, and
idempotency (duplicate pair write → `INSERT OR IGNORE` guard, no duplicate rows).
Verification: `cargo test -- nli_detection` (or `nli_detection_tick`) passes with all named
test cases present.

**AC-17** — TOML deserialization of `InferenceConfig` with the four new fields works correctly.
Fields absent from TOML use the specified defaults.
Verification: unit test parses a minimal TOML string without the four new fields; assert each
default value matches the spec.

**AC-18†** — All existing `InferenceConfig { ... }` struct literal constructions in
`crates/unimatrix-server/src/` are updated to include the four new fields or to use
`..InferenceConfig::default()` tail. No compile failure from missed struct literal updates.
Verification: `grep -rn 'InferenceConfig {' crates/unimatrix-server/src/` before merge; each
occurrence updated. Current count is 52 occurrences across `nli_detection.rs` and `config.rs`
(confirmed by grep at spec time). All must compile after the four fields are added.

**AC-19†** — `run_graph_inference_tick` writes NO `Contradicts` edges. The `contradiction_threshold`
parameter has been removed from `write_inferred_edges_with_cap`. No contradiction threshold
logic is present in the tick path. The tick is Supports-only. The dedicated contradiction
detection path remains the sole `Contradicts` writer.
Verification: `grep -n 'Contradicts\|contradiction_threshold\|nli_contradiction' nli_detection_tick.rs`
must return empty; `write_inferred_edges_with_cap` signature must have no `contradiction_threshold`
parameter.

---

## Domain Models

### Entry

An `EntryRecord` with `status = Active`. The tick operates exclusively on active entries. An
entry is **isolated** if it has no non-bootstrap edge (`bootstrap_only = 0`) as either
`source_id` or `target_id` in GRAPH_EDGES.

### Graph Edge

A row in GRAPH_EDGES with columns `(source_id, target_id, relation_type, weight, created_at,
created_by, source, bootstrap_only, metadata)`. A `UNIQUE(source_id, target_id, relation_type)`
constraint enforces deduplication. `INSERT OR IGNORE` is the write-idempotency pattern.

### Supports Edge

A graph edge with `relation_type = 'Supports'`. Indicates that the source entry semantically
entails or corroborates the target entry, as determined by the NLI cross-encoder. Written when
`entailment_score > supports_edge_threshold`.

### Contradicts Edge

A graph edge with `relation_type = 'Contradicts'`. Indicates semantic contradiction. NOT written
by the tick path — the tick is Supports-only. `Contradicts` edges are written exclusively by
the dedicated contradiction detection path (`run_post_store_nli`, the contradiction scan).
These edges interact with col-030 `suppress_contradicts` in `SearchService::search`; false
positives suppress valid results with no operator signal. The tick's Supports-only design
prevents it from producing false-positive `Contradicts` edges.

### EDGE_SOURCE_NLI

The constant `"nli"` (defined in `unimatrix_store::read`, exported from `unimatrix_store`). All
edges written by automated NLI inference carry this as the `source` column value (col-029
ADR-001).

### InferenceConfig

The `[inference]` TOML section struct. Carries all NLI-related configuration. After crt-029 it
holds four additional fields specific to the background graph inference tick path.

### Tick

One execution of `background_tick_loop`'s inner body. The tick calls
`run_graph_inference_tick` on every iteration when `nli_enabled = true`. The tick's NLI budget
is bounded by `max_graph_inference_per_tick` (pair count) and by the rayon pool (concurrent
ONNX threads).

### W1-2 Contract

Architectural constraint: all `CrossEncoderProvider::score_batch` calls must be dispatched via
`rayon_pool.spawn()`. Inline async NLI and `spawn_blocking` are both prohibited. Violation
blocks the tokio executor.

---

## User Workflows

### Operator: Enabling Background Graph Inference

1. Operator sets `nli_enabled = true` in the project or global TOML config.
2. Server starts; `NliServiceHandle` loads the cross-encoder model.
3. On each background tick, `run_graph_inference_tick` runs after bootstrap promotion.
4. Over successive ticks, `Supports` edges accumulate. The tick does NOT write `Contradicts`
   edges — that remains the dedicated contradiction detection path's responsibility.
5. Operator observes progress via `context_status` → `graph_cohesion` → `isolated_entry_count`,
   `cross_category_edge_count`, `inferred_edge_count`.

### Operator: Tuning the Tick

1. Operator sets `max_graph_inference_per_tick = 50` to reduce per-tick NLI cost.
2. Operator sets `supports_candidate_threshold = 0.6` and `supports_edge_threshold = 0.8` to
   raise the bar for edge inference.
3. Operator sets `graph_inference_k = 15` to widen the HNSW neighbour fan-out per source.
4. Server validates `supports_candidate_threshold < supports_edge_threshold` at startup;
   invalid configs abort with a structured error.

### Background Tick (automated)

1. `background_tick_loop` fires on its interval.
2. `maybe_run_bootstrap_promotion` runs (one-shot, idempotent).
3. If `nli_enabled = true`, `run_graph_inference_tick` runs.
4. Function checks NLI readiness; exits immediately if not ready.
5. Fetches active entry IDs + metadata.
6. Fetches isolated entry IDs via `query_entries_without_edges()`.
7. Selects source candidates (bounded to `max_graph_inference_per_tick`).
8. For each source candidate, fetches embedding via `get_embedding(id)`.
9. Queries `graph_inference_k` HNSW neighbours above `supports_candidate_threshold`.
10. Deduplicates and pre-filters already-supported pairs.
11. Sorts by priority; truncates to `max_graph_inference_per_tick`.
12. Dispatches all pairs to rayon for NLI scoring.
13. Writes `Supports` edges up to cap (no `Contradicts` edges written by tick).
14. Logs count at `debug` level.

---

## Constraints

**C-01 — W1-2 (mandatory hard constraint)**: `CrossEncoderProvider::score_batch` in the tick
path must go via `rayon_pool.spawn()`. `spawn_blocking` is prohibited. Inline async NLI is
prohibited. Violation blocks the tokio executor and is a gate-3c failure.

**C-02 — SQLite access via sqlx**: all new store queries use `sqlx::query` with the correct
pool (`read_pool()` for reads, `write_pool_server()` for writes). No raw `rusqlite` connections.
`query_entries_without_edges()` uses `read_pool()`.

**C-03 — INSERT OR IGNORE idempotency**: the `UNIQUE(source_id, target_id, relation_type)`
constraint is the deduplication backstop. The pre-filter avoids wasted NLI calls on already-
confirmed pairs; `INSERT OR IGNORE` handles any residual races.

**C-04 — No schema migration**: all required columns exist. No `ALTER TABLE`, no schema version
bump, no migration file.

**C-05 — No new crate dependencies**: the implementation is confined to existing workspace
crates and their transitive dependencies.

**C-06 — supports_edge_threshold intentionally higher than nli_entailment_threshold**: the
tick processes a much larger pair space than post-store NLI. The higher bar (default 0.7 vs.
post-store 0.6) reduces false positives. Both are independent config fields; changing one does
not affect the other.

**C-07 — SUPERSEDED by C-13**: the tick does not write `Contradicts` edges at all. C-07 was
the prior mitigation (explicit threshold floor); C-13 supersedes it with a stronger guarantee:
the tick has no Contradicts write path. The `nli_contradiction_threshold` field remains in
`InferenceConfig` for the post-store NLI path (`run_post_store_nli`), which is unchanged.

**C-08 — File size hard limit 800 lines**: `nli_detection.rs` currently ~650 lines. Adding
the tick function and helpers must not push the combined file past 800 lines. If it would,
split into `nli_detection_tick.rs`. This is a merge gate condition.

**C-09 — Supports-only inference (W3-1 deferred)**: crt-029 infers only `Supports` edges via
the tick. `Prerequisite` edge inference is explicitly out of scope. The `RelationType::Prerequisite`
variant already exists in the type system but has no tick write path in this feature.

**C-10 — No changes to run_post_store_nli**: the hot-path post-store NLI function is unchanged.
The tick is additive.

**C-13 — Tick MUST NOT write Contradicts edges**: `write_inferred_edges_with_cap` is a
Supports-only function. It evaluates only `entailment` scores and writes only `Supports` edges.
The `contradiction_threshold` parameter does not exist on this function. The dedicated
contradiction detection path is the sole writer of `Contradicts` edges. Rationale: mixing
Contradicts writes in the tick creates a second code path for contradiction inference that can
drift out of calibration with the dedicated path. This is a hard constraint; any `Contradicts`
write in `nli_detection_tick.rs` is a gate-3c failure.

**C-14 — R-09 rayon/tokio boundary (named constraint)**: the rayon closure passed to
`rayon_pool.spawn()` inside `run_graph_inference_tick` MUST be a synchronous CPU-bound closure
only. The following are PROHIBITED inside any rayon closure in this module:
- `tokio::runtime::Handle::current()`
- `.await` expressions
- Any function that internally calls `.await` or accesses the Tokio runtime
Rationale: rayon worker threads have no Tokio runtime; any of the above panics at runtime
with "no current Tokio runtime". This failure is compile-invisible and test-invisible in unit
tests that don't use the full runtime.
Detection method: `grep -n 'Handle::current\|\.await' nli_detection_tick.rs` — any match
inside a rayon closure is a gate-blocking defect. Manual inspection of the closure body is
required. The validator MUST NOT be the same agent that wrote the rayon closure
(independent review required).

**C-11 — InferenceConfig struct literal grep before merge (SR-07)**: before the PR is opened,
the implementor must run `grep -rn 'InferenceConfig {' crates/unimatrix-server/src/` and update
every occurrence to include the four new fields or to use `..InferenceConfig::default()` tail.
There are currently 52 occurrences (per grep at spec time). Missing updates cause compile
failures; this is a known gate-failure pattern from crt-023 (7 missed occurrences, Unimatrix
entry #2730).

**C-12 — compute_graph_cohesion_metrics pool**: the architect must confirm that
`compute_graph_cohesion_metrics` (col-029) uses `read_pool()` and not `write_pool_server()`.
If it uses the write pool, active inference ticks and operator `context_status` calls create
chronic write-pool contention. This is a pre-existing defect (SR-06) that should be surfaced
and either fixed in crt-029 or tracked as a follow-up with an explicit note in the
implementation brief.

---

## Dependencies

| Dependency | Type | Notes |
|---|---|---|
| `unimatrix-store` | Workspace crate | `Store::query_entries_without_edges()` (new), `query_graph_edges()`, `query_by_status()`, `EDGE_SOURCE_NLI` |
| `unimatrix-core` | Workspace crate | `VectorIndex::search()`, `VectorIndex::get_embedding()` |
| `unimatrix-embed` | Workspace crate | `CrossEncoderProvider::score_batch`, `NliScores` |
| `NliServiceHandle` | Internal (infra) | `get_provider()` — readiness gate |
| `RayonPool` | Internal (infra) | `spawn()` for W1-2 contract |
| `InferenceConfig` | Internal (infra/config.rs) | Four new fields |
| `background_tick_loop` | Internal (services/background.rs) | Call site for the new tick function |
| `maybe_run_bootstrap_promotion` | Internal (services/nli_detection.rs) | Tick ordering: new function runs after this |
| `write_edges_with_cap` | Internal (services/nli_detection.rs) | Reused or minimally adapted for the tick write path |
| `EDGE_SOURCE_NLI` | `unimatrix_store::read` | col-029 ADR-001; canonical `"nli"` constant |
| col-029 graph cohesion metrics | `context_status` | Observability layer for the tick's output |
| col-030 `suppress_contradicts` | `SearchService::search` | Always-on; motivates the contradiction threshold floor (C-07) |

---

## NOT in Scope

- **Prerequisite edge inference**: `RelationType::Prerequisite` exists but no tick write path
  for it is introduced. Deferred to W3-1.
- **Prerequisite bootstrap_only promotion**: removing `bootstrap_only = true` from existing
  Prerequisite rows is W3-1.
- **Changes to `run_post_store_nli`**: the hot path is untouched.
- **Changes to `TypedRelationGraph` or `build_typed_relation_graph`**: no in-memory structure
  changes.
- **Changes to search ranking or confidence scoring**: `Supports` edges affect graph structure
  but the tick introduces no changes to `graph_penalty` logic or confidence weights.
- **Changes to `contradiction::scan_contradictions`**: that is a status-diagnostic path in
  `infra/contradiction.rs`. The new tick writes persistent edges; it does not interact with
  `ContradictionScanCacheHandle`.
- **Auto-quarantine triggered by the tick**: auto-quarantine for `Contradicts` edges remains the
  responsibility of the existing auto-quarantine cycle counter. The tick does not add new
  quarantine triggers.
- **New ONNX models or rayon pools**: the existing `CrossEncoderProvider` and pool are reused.
- **Schema migration**: no new columns, no version bump.
- **New crate dependencies**.
- **Tick-modulo interval gate**: the tick runs every tick; `max_graph_inference_per_tick` is the
  throttle. An interval gate parameter is explicitly not added.
- **Alert or notification when isolated_entry_count changes**: observability is via existing
  `context_status` metrics only.

---

## Open Questions

**OQ-01 (for Architect)** — `compute_graph_cohesion_metrics` pool choice (SR-06): Confirm
whether the function uses `read_pool()` or `write_pool_server()`. If the write pool, this is a
pre-existing defect that must either be fixed in crt-029 or tracked explicitly. The spec
requires the architect to resolve this before the implementation brief is written.

**OQ-02 (for Architect)** — `write_edges_with_cap` reuse vs. variant (SR-08): The tick's cap
logic must be independently unit-testable. Determine whether the existing `write_edges_with_cap`
function (currently `async fn`, private) can be reused as-is with the tick's different thresholds,
or whether a named variant is required. If a variant, both must remain independently testable
without live ONNX. This decision must be recorded in an ADR.

**OQ-03 (for Architect)** — GRAPH_EDGES index coverage (SR-04): Confirm whether a covering
index on `(source_id, target_id, relation_type)` exists in the current schema, or whether the
`UNIQUE` constraint implicitly provides it. Document the scale boundary at which the pre-filter
becomes a write-pool concern.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 11 entries. Key findings: entry #3655
  confirms the two-bound pattern for background NLI tick (source-candidate cap before embedding,
  independent of NLI pair cap); entry #3591 confirms `EDGE_SOURCE_NLI = "nli"` constant is in
  `unimatrix_store::read`. Both findings incorporated into FR-10 and FR-07 respectively.
