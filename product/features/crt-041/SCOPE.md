# crt-041: Graph Enrichment — S1, S2, S8 Edge Sources

## Problem Statement

The production GRAPH_EDGES table has ~1,086 active→active edges (as of the crt-039 baseline).
The PPR expander (Group 4) requires a sufficiently dense graph to traverse cross-category
entries that lie outside the HNSW k=20 candidate set. ASS-038 confirmed that 6/10 UC
ground-truth entries are completely outside k=20 — the PPR expander is the only architecture
that can surface them, and it needs dense edges to walk through.

crt-039 shipped the structural_graph_tick and decoupled it from the NLI gate. crt-040 shipped
cosine Supports detection. The graph still needs bulk enrichment before the expander is viable.

Three signal sources were validated in ASS-038 at 1,134 active entries:
- **S1** (tag co-occurrence): 1,052 new Informs edges from pairs sharing ≥3 tags
- **S2** (structural vocabulary): 1,830 new Informs edges from pairs sharing ≥2 domain terms
- **S8** (search co-retrieval): 2,770 new CoAccess edges from audit_log search result pairs

Combined yield: ~5,652 new edges on top of the ~1,086 baseline → target ≥3,000 met.
All three sources are SQL-only (no model, no rayon pool, no embedding lookups), making them
cheap background tick operations.

All edges must carry a distinct `source` value (the existing `graph_edges.source` TEXT column
is the `signal_origin` field from the roadmap) so that GNN feature construction (W3-1) can
distinguish edge types for training.

## Goals

1. Implement S1 background tick: for every pair of active entries sharing ≥3 tags, write an
   `Informs` edge with `source = 'S1'`. SQL-only. Idempotent (INSERT OR IGNORE). Capped per
   tick to bound latency.
2. Implement S2 background tick: for every pair of active entries sharing ≥2 terms from a
   configurable domain vocabulary list, write an `Informs` edge with `source = 'S2'`. Term
   matching is full-word against `content || title` via SQL. Vocabulary in `InferenceConfig`.
3. Implement S8 batch tick: periodically read `audit_log` for `context_search` operations and
   extract co-retrieved pairs (entries appearing together in the same search result set across
   sessions). Write `CoAccess` edges with `source = 'S8'`. Batched every N ticks (not
   every tick — audit_log reads are heavier and the signal is not real-time).
4. Add named constants `EDGE_SOURCE_S1`, `EDGE_SOURCE_S2`, `EDGE_SOURCE_S8` to
   `unimatrix-store::read`, re-exported from the crate root, following the existing pattern
   for `EDGE_SOURCE_NLI` and `EDGE_SOURCE_CO_ACCESS`.
5. Add config fields to `InferenceConfig` for all tunable parameters: S2 vocabulary list,
   per-tick caps for S1/S2, batch interval for S8, and S8 per-batch cap.
6. Plug S1 and S2 into the existing `structural_graph_tick` ordering, and S8 into the
   maintenance tick sequence (with its own N-tick gate).
7. Pass the eval gate: `cross_category_edge_count` increases and `isolated_entry_count`
   decreases in `GraphCohesionMetrics` after delivery. No MRR regression vs baseline 0.2856
   on `product/research/ass-039/harness/scenarios.jsonl`.

## Non-Goals

- This feature does NOT implement the PPR expander (Group 4). Edge density enables it; this
  feature only builds the density.
- This feature does NOT add a new `signal_origin` schema column. The existing
  `graph_edges.source` TEXT column is the canonical signal-origin field. No migration needed.
- This feature does NOT implement cosine Supports detection (crt-040, already shipped).
- This feature does NOT implement behavioral Informs edges (Group 6 — depends on Group 5
  infrastructure not yet shipped).
- This feature does NOT implement S3, S4, S5 edge sources. ASS-038 confirmed these yield
  fewer than 20 viable pairs at current corpus size and are deferred until corpus ≥3,000.
- This feature does NOT process content using any ML model. All three sources are pure SQL
  or SQL + string matching; no ONNX, no rayon pool, no spawn_blocking.
- This feature does NOT modify the contradiction detection path or the NLI Supports path.
- This feature does NOT add Supports edges (S1/S2 write Informs; S8 writes CoAccess).
- This feature does NOT change the TypedRelationGraph build logic or PPR scoring.
- This feature does NOT change how `inferred_edge_count` in `GraphCohesionMetrics` is
  computed. That field counts `source = 'nli'` edges only — unchanged.
- This feature does NOT add a schema migration for new tables. All writes go to
  `graph_edges` (schema v19, existing).

## Background Research

### Schema: `signal_origin` maps to `graph_edges.source`

No `signal_origin` column exists. `graph_edges.source` (TEXT NOT NULL DEFAULT '') is the
equivalent. Current values in production:
- `'nli'` — written by `write_nli_edge()` in `nli_detection.rs` (hardcoded literal)
- `'co_access'` — written by the co_access promotion tick (`EDGE_SOURCE_CO_ACCESS` constant)
- `''` (empty) — bootstrap edges written during v12→v13 migration

The roadmap's `signal_origin='S1'/'S2'/'S8'` maps directly to writing `source = 'S1'`/
`'S2'`/`'S8'` in the `graph_edges.source` column. Named constants in `read.rs` follow the
pattern established by `EDGE_SOURCE_NLI` (col-029 ADR-001) and `EDGE_SOURCE_CO_ACCESS`
(crt-034 ADR-002).

### GRAPH_EDGES Schema (schema v19)

```sql
CREATE TABLE graph_edges (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id      INTEGER NOT NULL,
    target_id      INTEGER NOT NULL,
    relation_type  TEXT    NOT NULL,
    weight         REAL    NOT NULL DEFAULT 1.0,
    created_at     INTEGER NOT NULL,
    created_by     TEXT    NOT NULL DEFAULT '',
    source         TEXT    NOT NULL DEFAULT '',
    bootstrap_only INTEGER NOT NULL DEFAULT 0,
    metadata       TEXT    DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
)
```

INSERT OR IGNORE on the UNIQUE(source_id, target_id, relation_type) constraint provides
idempotency for all three sources. No weight refresh logic is needed for S1/S2 (tag overlap
and term overlap are deterministic) or S8 (fixed weight 0.25 per ASS-038).

**Critical finding from entry #3981 (lesson-learned):** When promoting co_access pairs or
writing graph edges, the candidate SELECT must JOIN against entries for BOTH endpoint columns
(entry_id_a AND entry_id_b) to filter quarantined endpoints. Filtering only one side silently
promotes quarantined entries as edge targets. S1, S2, and S8 SQL must JOIN entries on BOTH
source_id and target_id with `status != Quarantined`.

### Tag Storage: `entry_tags` table

```sql
CREATE TABLE entry_tags (
    entry_id INTEGER NOT NULL,
    tag      TEXT    NOT NULL,
    PRIMARY KEY (entry_id, tag),
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
)
```
Indexes: `idx_entry_tags_tag` (on tag) and `idx_entry_tags_entry_id` (on entry_id).

S1 SQL pattern (standard self-join on entry_tags):
```sql
SELECT t1.entry_id AS source_id, t2.entry_id AS target_id, COUNT(*) AS shared_tags
FROM entry_tags t1
JOIN entry_tags t2 ON t2.tag = t1.tag AND t2.entry_id > t1.entry_id
JOIN entries e1 ON e1.id = t1.entry_id AND e1.status = 0  -- Active only
JOIN entries e2 ON e2.id = t2.entry_id AND e2.status = 0
GROUP BY t1.entry_id, t2.entry_id
HAVING COUNT(*) >= 3
ORDER BY shared_tags DESC
LIMIT ?cap
```
Weight formula (from ASS-038): `shared_tags_count / max_tag_overlap` (ratio ≤ 1.0), or
simply `shared_tags_count * 0.1` as a simpler approximation. The exact formula is an open
question for the design phase.

### S2: Structural Vocabulary

S2 uses a configurable domain vocabulary list stored in `InferenceConfig`. In ASS-038,
9 domain terms were used: `["migration", "schema", "performance", "async", "authentication",
"cache", "api", "confidence", "graph"]`. These match against `content || ' ' || title` via
SQL LIKE or instr(). Pairs sharing ≥2 terms get an `Informs` edge with `source='S2'`.

The vocabulary list must live in `InferenceConfig` as `Vec<String>` — not hardcoded in SQL —
so operators can customize it for their domain (roadmap: "domain-agnostic vocabulary in
config"). An empty list disables S2 without error.

The SQL for a 9-term vocabulary would require 9 CASE WHEN expressions or a dynamically
constructed query. For operator-customizable vocabulary of arbitrary length, the implementation
must build the SQL query dynamically from `inference_config.s2_vocabulary` at tick time.

Weight formula from ASS-038: `shared_term_count / 10` (capped at 1.0 for 10+ shared terms).

### S8: Search Co-Retrieval from audit_log

`audit_log` schema (from db.rs and audit.rs):
```sql
CREATE TABLE audit_log (
    event_id   INTEGER PRIMARY KEY,
    timestamp  INTEGER NOT NULL,
    session_id TEXT    NOT NULL,
    agent_id   TEXT    NOT NULL,
    operation  TEXT    NOT NULL,
    target_ids TEXT    NOT NULL DEFAULT '[]',  -- JSON array of u64
    outcome    INTEGER NOT NULL,
    detail     TEXT    NOT NULL DEFAULT ''
)
```

`context_search` operations write their returned entry IDs to `target_ids` as a JSON array.
S8 reads these rows and creates co-retrieved pairs from entries that appear together within the
same search result set. ASS-038 used `search_service` operations to generate 2,770 pairs from
411 entries (21.3% coverage), using JSON parsing on `target_ids`.

**Critical consideration**: audit_log does not have a per-row watermark or "processed" flag.
S8 must track which audit_log rows have been processed to avoid re-processing on every tick.
Options: (a) store the last-processed `event_id` in a counter, or (b) use a per-run timestamp
window. Either requires state persistence. The `counters` table is the correct place for an
S8 watermark counter (follows the established pattern for `next_audit_event_id`).

S8 is a batch operation (not run every tick). ASS-038 recommended computing it as a periodic
batch rather than real-time. A configurable `s8_batch_interval_ticks` (default: 10, runs every
10 ticks = every ~150 minutes at default tick interval) gates execution.

Per-batch cap: configurable `max_s8_pairs_per_batch` (default: 500) to bound latency. Pairs
are unordered within a search result; for N results returned, up to N*(N-1)/2 pairs are
possible per search event. At k=20 HNSW, each search returns at most 20 entries → 190 pairs
per event. With the 500-pair batch cap, at most ~3 search events are processed per S8 batch.

Weight: fixed 0.25 per ASS-038 GNN feature spec (`weight = float [0.0, 1.0]`, S8 = 0.25).

### Tick Ordering Invariant (non-negotiable, crt-039)

Current ordering (from background.rs comment):
```
compaction → co_access_promotion → graph-rebuild → PhaseFreqTable::rebuild
  → contradiction_scan (if embed adapter ready && tick_multiple)
  → extraction_tick
  → structural_graph_tick (always)
```

S1 and S2 are Informs-edge sources. They should run inside (or alongside) `structural_graph_tick`
since they write Informs edges to GRAPH_EDGES, which is rebuilt by `TypedGraphState::rebuild`
earlier in the tick. Placement options:
- **Option A**: S1 and S2 as new phases inside `run_graph_inference_tick` (same file as Path A
  structural Informs). This follows the pattern where `structural_graph_tick` is the centralized
  graph-write phase.
- **Option B**: S1 and S2 as separate tick functions called after `structural_graph_tick`.

S8 writes CoAccess edges and is batched (runs every N ticks). It should run in a position
analogous to `run_co_access_promotion_tick` (before or after, not inside structural_graph_tick).
Because S8 writes to GRAPH_EDGES and graph-rebuild has already run by that point, S8 edges
will be visible at the NEXT tick's graph-rebuild — this is acceptable (same delay as
co_access_promotion_tick already has).

**Placement recommendation (open for design):** Run S1/S2 inside or immediately after
`run_graph_inference_tick`. Run S8 as a separate function after `run_graph_inference_tick`,
gated by `current_tick.is_multiple_of(s8_batch_interval_ticks)`.

### Existing Per-Tick Cap Patterns

From the codebase survey:
- `MAX_INFORMS_PER_TICK = 25` — module constant in `nli_detection_tick.rs` (not config)
- `max_graph_inference_per_tick: usize = 100` — `InferenceConfig` field (NLI Supports path)
- `max_co_access_promotion_per_tick: usize = 200` — `InferenceConfig` field (co_access tick)

S1 and S2 are pure SQL with no per-pair model cost, so higher default caps are appropriate
(similar to co_access: 200+). S8 involves JSON parsing from audit_log rows, so a more
conservative default is reasonable. Design agent must decide: module constants vs config fields.

Entry #3826 (Unimatrix) warns: background tick features that promote rows via a count threshold
must explicitly specify near-threshold oscillation behavior before implementation.

Entry #3675 (Unimatrix) warns: background tick NLI fans — two independent bounds needed. S1/S2
are simpler (no model scoring), but the same two-bound thinking applies: (1) cap the number of
candidate pairs selected, and (2) cap the number of edges written per tick.

### InferenceConfig: Where New Config Fields Live

`InferenceConfig` in `crates/unimatrix-server/src/infra/config.rs` holds all background tick
parameters. The struct uses `#[serde(default)]` on all fields with private default functions.
Two-level invariant: `InferenceConfig::default()` must match the compiled defaults (not the
serde default functions), and the `validate()` method rejects out-of-range values at startup.

New fields for this feature:
- `s2_vocabulary: Vec<String>` — domain term list for S2 (default: empty — operator opt-in;
  9-term software-engineering list from ASS-038 documented in config comment as starting point)
- `max_s1_edges_per_tick: usize` — per-tick S1 edge write cap (default: 200)
- `max_s2_edges_per_tick: usize` — per-tick S2 edge write cap (default: 200)
- `s8_batch_interval_ticks: u32` — how often S8 batch runs (default: 10)
- `max_s8_pairs_per_batch: usize` — per-batch S8 pair cap (default: 500)

### Existing `write_graph_edge` Generalization (crt-040 prerequisite)

crt-040 was scoped to add a general `write_graph_edge(source: &str, ...)` function to replace
the hardcoded `'nli'` literal in `write_nli_edge`. Whether crt-040 actually shipped this
generalization or left it as a follow-up is an open question. If `write_graph_edge` exists
after crt-040, S1/S2/S8 can reuse it. If not, it must be added in crt-041.

### GraphCohesionMetrics: Eval Gate Fields

`GraphCohesionMetrics` in `read.rs` (line ~1710):
- `total_non_bootstrap_edge_count`: all edges with `bootstrap_only = 0`
- `supports_edge_count`: all `Supports` relation edges
- `mean_entry_degree`: mean in+out degree
- `inferred_edge_count`: edges with `source = 'nli'` (NLI-origin only, not changed)

The eval gate for crt-041 is:
- `cross_category_edge_count` — not yet a field in `GraphCohesionMetrics`. Will need adding.
- `isolated_entry_count` — also not yet a field. Measures entries with 0 edges.

A new graph density metric reporting function may be needed for the eval gate, or the existing
`context_status` output can be augmented with edge-count-by-source breakdowns.

## Proposed Approach

Implement each source as an independent, infallible background tick function following the
`run_co_access_promotion_tick` design pattern (direct write_pool, no rayon, no model,
tracing::info! summary log on completion, warn! on error, never panic).

**S1 implementation:**
1. SQL self-join on `entry_tags` to find pairs sharing ≥3 tags among active (non-quarantined)
   entries. JOIN entries on BOTH sides to exclude quarantined endpoints (lesson from entry #3981).
2. INSERT OR IGNORE into `graph_edges` with `relation_type='Informs'`, `source='S1'`,
   `weight = shared_tag_count * 0.1` (or ratio formula — open question AC-S1-W).
3. Capped by `max_s1_edges_per_tick`. ORDER BY shared_tag_count DESC so high-overlap pairs
   are prioritized.
4. Located in a new file `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`.
5. Called from `run_single_tick` after `run_graph_inference_tick`.

**S2 implementation:**
1. Dynamically build a SQL query from `inference_config.s2_vocabulary`. For each term, add
   a `CASE WHEN instr(lower(e.content || ' ' || e.title), lower(?term)) > 0 THEN 1 ELSE 0 END`
   expression. Sum and filter pairs where sum >= 2.
2. INSERT OR IGNORE with `relation_type='Informs'`, `source='S2'`,
   `weight = shared_term_count * 0.1`.
3. Capped by `max_s2_edges_per_tick`.
4. Located in the same `graph_enrichment_tick.rs` file.
5. Called from `run_single_tick` after S1 (both after `run_graph_inference_tick`).
6. No-op (return immediately) when `s2_vocabulary.is_empty()`.

**S8 implementation:**
1. Load the S8 watermark from counters table (`s8_audit_log_watermark`, default 0).
2. Query `audit_log` for rows with `event_id > watermark AND operation = 'context_search'
   AND outcome = 0 (Success)`. Parse `target_ids` JSON for each row. Build co-retrieved pairs.
3. For each pair (both active, non-quarantined — validate via JOIN or pre-filter), INSERT OR
   IGNORE with `relation_type='CoAccess'`, `source='S8'`, `weight=0.25`.
4. Update the watermark counter to `max(event_id processed)`.
5. Capped by `max_s8_pairs_per_batch`. Run only when
   `current_tick.is_multiple_of(s8_batch_interval_ticks)`.
6. Located in the same `graph_enrichment_tick.rs` file.
7. Called from `run_single_tick`, gated by `current_tick % s8_batch_interval_ticks == 0`.

**Tick ordering after this feature:**
```
compaction → co_access_promotion → graph-rebuild → PhaseFreqTable::rebuild
  → contradiction_scan (if embed adapter ready && tick_multiple)
  → extraction_tick
  → structural_graph_tick (run_graph_inference_tick — always)
  → graph_enrichment_tick (S1/S2 always; S8 every N ticks)
```

## Acceptance Criteria

- AC-01: S1 tick writes `Informs` edges for active→active pairs sharing ≥3 tags, with
  `source = EDGE_SOURCE_S1 = 'S1'`.
- AC-02: S1 uses `INSERT OR IGNORE` — running S1 twice on the same corpus produces no
  duplicate edges (idempotency).
- AC-03: S1 excludes edges where either endpoint has `status = Quarantined` (3). JOIN on
  entries for both source_id and target_id with `status != Quarantined`.
- AC-04: S1 is capped per tick at `max_s1_edges_per_tick`. Pairs are selected in descending
  shared-tag-count order so high-overlap pairs are prioritized.
- AC-05: S2 tick writes `Informs` edges for active→active pairs sharing ≥2 terms from
  `s2_vocabulary`, with `source = EDGE_SOURCE_S2 = 'S2'`.
- AC-06: S2 is a no-op (zero SQL writes) when `s2_vocabulary` is empty.
- AC-07: S2 uses `INSERT OR IGNORE` — idempotent across tick runs.
- AC-08: S2 excludes quarantined-endpoint pairs (same dual-JOIN guard as S1, AC-03).
- AC-09: S2 is capped per tick at `max_s2_edges_per_tick`. Pairs selected in descending
  shared-term-count order.
- AC-10: S8 tick writes `CoAccess` edges for pairs co-appearing in `context_search`
  audit_log results, with `source = EDGE_SOURCE_S8 = 'S8'` and `weight = 0.25`.
- AC-11: S8 uses a persistent watermark stored in the `counters` table under key
  `'s8_audit_log_watermark'`. Only rows with `event_id > watermark` are processed.
  Watermark is updated after each successful batch.
- AC-12: S8 runs only on ticks where `current_tick % s8_batch_interval_ticks == 0`.
- AC-13: S8 uses `INSERT OR IGNORE` — idempotent; re-processing a batch produces no
  duplicate edges.
- AC-14: S8 excludes quarantined-endpoint pairs. Both entry_ids in a co-retrieved pair
  must have `status != Quarantined` before the edge is written.
- AC-15: S8 is capped at `max_s8_pairs_per_batch` pairs per batch.
- AC-16: Named constants `EDGE_SOURCE_S1 = 'S1'`, `EDGE_SOURCE_S2 = 'S2'`,
  `EDGE_SOURCE_S8 = 'S8'` are defined in `unimatrix-store::read` and re-exported from
  the crate root (`lib.rs`), following the pattern for `EDGE_SOURCE_NLI` and
  `EDGE_SOURCE_CO_ACCESS`.
- AC-17: `InferenceConfig` gains five new fields with correct defaults, validation, and
  `Default::default()` matching compiled defaults (the crt-038 `impl Default` trap — entry
  #4011):
  - `s2_vocabulary: Vec<String>` (default: empty — operator opt-in; 9-term software-engineering list documented in config comment)
  - `max_s1_edges_per_tick: usize` (default: 200, range: [1, 10000])
  - `max_s2_edges_per_tick: usize` (default: 200, range: [1, 10000])
  - `s8_batch_interval_ticks: u32` (default: 10, range: [1, 1000])
  - `max_s8_pairs_per_batch: usize` (default: 500, range: [1, 10000])
- AC-18: Each tick function (S1, S2, S8) emits a `tracing::info!` summary log with edges
  written/skipped counts on completion. Infallible: SQL errors logged at `warn!`, tick
  continues.
- AC-19: S1, S2, S8 run AFTER `run_graph_inference_tick` in the tick sequence. The
  existing ordering invariant is not modified up to and including structural_graph_tick.
- AC-20: Tick ordering invariant comment in `background.rs` is updated to include the new
  `graph_enrichment_tick` step after `structural_graph_tick`.
- AC-21: Eval gate passes: `context_status` output (or a new graph-density query) shows
  increased `total_non_bootstrap_edge_count` and decreased isolated-entry count after
  delivery. No MRR regression vs 0.2856 baseline on behavioral scenarios.
- AC-22: S8 does not process `context_briefing` rows (only `context_search` rows). The
  audit_log filter is `operation = 'context_search'`.
- AC-23: S8 only processes rows with `outcome = 0` (Success). Failed or denied searches are
  excluded from co-retrieval pair extraction.
- AC-24: The `graph_enrichment_tick.rs` module is under 500 lines (workspace 500-line rule).
  If the combined S1+S2+S8 implementation exceeds 500 lines excluding tests, extract test
  helpers to a `_tests.rs` sibling file following the pattern in co_access_promotion_tick.

## Constraints

- **No ML model.** S1, S2, S8 are pure SQL. No `rayon_pool.spawn()`, no ONNX, no
  `spawn_blocking`. All write_pool calls use `store.write_pool_server()` directly.
- **No new schema tables.** All three sources write to the existing `graph_edges` table.
  S8 watermark uses the existing `counters` table. Schema version stays at 19 — no migration
  needed.
- **Dual-endpoint quarantine guard mandatory.** Entry #3981 documents the production bug:
  missing JOIN on the second endpoint silently writes edges to quarantined entries. Both S1,
  S2, and S8 must filter `status != Quarantined` for BOTH endpoints.
- **write_graph_edge generalization dependency.** If crt-040 did not add a general
  `write_graph_edge(source: &str, ...)` function, crt-041 must add it. Existing `write_nli_edge`
  hardcodes `'nli'` as source — it cannot be reused for S1/S2/S8 without changing the source
  value, which would silently retag existing NLI edges (NOT acceptable).
- **S8 watermark consistency.** The S8 watermark must be updated AFTER edges are written,
  not before. If the edge write succeeds and the watermark update fails, S8 re-processes the
  same batch on the next run (idempotent INSERT OR IGNORE handles this correctly).
- **S2 vocabulary must be loaded at tick time from InferenceConfig.** Do not cache the
  vocabulary outside the tick invocation — the config is treated as immutable after startup
  (no live reload), so this is safe, but the SQL must be built from the current config value.
- **500-line file limit.** New `graph_enrichment_tick.rs` module; split tests to
  `graph_enrichment_tick_tests.rs` if needed.
- **InferenceConfig::default() must match serde defaults** (entry #4011 trap, crt-038).
  Both the `impl Default` block and the `#[serde(default = "fn")]` annotations must agree.
- **Backward compat:** `inferred_edge_count` in `GraphCohesionMetrics` continues to count
  only `source = 'nli'` edges. S1/S2/S8 edges are not counted in that field.
- **crt-040 prerequisite:** `structural_graph_tick` runs unconditionally (crt-039 confirmed).
  crt-040 (cosine Supports) is assumed to be shipped before crt-041 delivery begins.

## Design Decisions (Open Questions Resolved)

1. **write_graph_edge availability — hard prerequisite from crt-040.**
   crt-040 is scoped to add `write_graph_edge(source: &str, ...)`. Declare it as a hard
   prerequisite: delivery agent verifies the function exists before writing S1/S2/S8 call
   sites. If crt-040 shipped without it (AC-11 violation), crt-041 adds it.

2. **S1 weight formula — `shared_tag_count * 0.1`, capped at 1.0.**
   Global normalization adds O(N) cost for marginal semantic gain. `shared_tag_count * 0.1`
   produces weights 0.3–1.0 for 3–10 shared tags — meaningful for PPR traversal. Follows
   ASS-038. Cap at 1.0.

3. **S2 default vocabulary — empty (operator opt-in).**
   Shipping the 9-term ASS-038 list as default re-couples behavior to the software-engineering
   domain, violating the product vision's domain-agnostic requirement (W0-3). Default is empty
   (S2 is a no-op out of the box). The 9-term list (`migration`, `schema`, `performance`,
   `async`, `authentication`, `cache`, `api`, `confidence`, `graph`) is documented prominently
   in the config comment as the recommended software-engineering starting point. The production
   Unimatrix deployment sets it in `config.toml`.

4. **S2 term matching — space-padded word boundary.**
   `instr(lower(' ' || content || ' ' || title || ' '), lower(' ' || term || ' ')) > 0`
   Eliminates false positives (e.g., "api" matching "capabilities"). No new dependencies,
   pure SQLite. Not raw substring (`instr(lower(text), lower(term))`), not FTS5.

5. **S8 tick placement — after S1/S2 (S1 → S2 → S8 on tick_multiple).**
   Per-tick work first, then batched gate. Natural ordering.

6. **Separate module — `graph_enrichment_tick.rs`.** Confirmed. `nli_detection_tick.rs` is
   already >2,000 lines; S1/S2/S8 are structurally different (SQL-only, no HNSW/model).

7. **Near-threshold oscillation — additive-only, stated explicitly in ACs.**
   Once an S1 or S2 edge is written it persists until endpoint deletion or quarantine.
   Per-tick diffing on tag-count drops would be expensive with no real benefit — the graph is
   designed to grow, not shrink on minor fluctuations. Reconciliation is deferred.

8. **Eval gate metrics — add to `GraphCohesionMetrics` in crt-041.**
   `cross_category_edge_count` and `isolated_entry_count` are ongoing health signals for
   `context_status`, not one-time delivery report queries. Both fields are in scope for crt-041.

**Cross-feature note (crt-040 / crt-041):** Both features add `InferenceConfig` fields and
must ensure `impl Default` matches serde defaults (entry #4011 trap, documented in AC-17).
crt-040 delivery team must carry the same callout.

## Tracking

https://github.com/dug-21/unimatrix/issues/489
