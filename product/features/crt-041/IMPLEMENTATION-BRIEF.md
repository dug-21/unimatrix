# crt-041 Implementation Brief — Graph Enrichment: S1, S2, S8 Edge Sources

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-041/SCOPE.md |
| Architecture | product/features/crt-041/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-041/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-041/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-041/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| graph_enrichment_tick | pseudocode/graph_enrichment_tick.md | test-plan/graph_enrichment_tick.md |
| config | pseudocode/config.md | test-plan/config.md |
| edge_constants | pseudocode/edge_constants.md | test-plan/edge_constants.md |
| background | pseudocode/background.md | test-plan/background.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Bulk-enrich the GRAPH_EDGES table by implementing three SQL-only background tick edge sources — S1 (tag co-occurrence, Informs edges), S2 (structural vocabulary, Informs edges), and S8 (search co-retrieval, CoAccess edges) — raising total non-bootstrap edge count from ~1,086 toward ≥3,000. This edge density makes the PPR expander (Group 4) viable for surfacing entries outside the HNSW k=20 candidate set, and produces source-tagged edges (`source = 'S1'/'S2'/'S8'`) ready for GNN feature construction (W3-1).

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Module structure: new file vs extend nli_detection_tick.rs | New module `graph_enrichment_tick.rs` (Option B). nli_detection_tick.rs already exceeds 2,000 lines; S1/S2/S8 are SQL-only with no model dependency. Follows co_access_promotion_tick.rs pattern. | SCOPE.md §Design Decision 6 | architecture/ADR-001-graph-enrichment-module-structure.md |
| S2 SQL construction: parameterized vs string interpolation | `sqlx::QueryBuilder::push_bind` for every vocabulary term (Option B). Structurally eliminates SQL injection surface; operator vocabulary is never concatenated into the SQL string. SECURITY comment required at construction site. | SCOPE-RISK-ASSESSMENT.md SR-01 | architecture/ADR-002-s2-safe-sql-construction.md |
| S8 watermark strategy: event_id counter vs timestamp window | `counters` table, key `'s8_audit_log_watermark'`, tracking last-processed `event_id` (Option B). Monotonically increasing, no clock dependency, follows established counters pattern. Watermark written AFTER edge writes (at-least-once re-processing on crash; INSERT OR IGNORE handles idempotency). | SCOPE-RISK-ASSESSMENT.md SR-03, SR-08 | architecture/ADR-003-s8-watermark-strategy.md |
| GraphCohesionMetrics: add new fields vs use existing | No new fields (Option B). Both `cross_category_edge_count` and `isolated_entry_count` already exist from col-029. Eval gate reads existing fields. | SCOPE-RISK-ASSESSMENT.md SR-05 | architecture/ADR-004-graphcohesionmetrics-extension.md |
| InferenceConfig dual-maintenance: five new fields | Five new fields with identical values in both `default_*()` backing functions and `impl Default` struct literal. `s2_vocabulary` default is empty (operator opt-in; domain-agnostic per W0-3). Four numeric fields have range-bounded validate() checks. Test `test_inference_config_s1_s2_s8_defaults_match_serde` is mandatory pre-PR verification. | SCOPE-RISK-ASSESSMENT.md SR-07 | architecture/ADR-005-inferenceconfig-dual-maintenance-guard.md |
| S1 weight formula | `min(shared_tag_count * 0.1, 1.0)`. Range: [0.3, 1.0]. Cap applied in Rust after query result, not in SQL. | SCOPE.md §Design Decision 2 | — |
| S2 term matching | Space-padded word-boundary: `instr(lower(' ' \|\| content \|\| ' ' \|\| title \|\| ' '), lower(' ' \|\| ? \|\| ' ')) > 0`. Eliminates false positives (e.g., "api" matching "capabilities"). | SCOPE.md §Design Decision 4 | architecture/ADR-002-s2-safe-sql-construction.md |
| S2 default vocabulary | Empty (operator opt-in). 9-term ASS-038 list documented in config comment as recommended software-engineering starting point but not the default. | SCOPE.md §Design Decision 3 | architecture/ADR-005-inferenceconfig-dual-maintenance-guard.md |
| S8 tick placement | After S1/S2, gated by `current_tick % s8_batch_interval_ticks == 0`. Per-tick work first, then batched gate. | SCOPE.md §Design Decision 5 | architecture/ADR-001-graph-enrichment-module-structure.md |
| S8 batch cap semantics | Cap on pairs written, not on audit_log rows fetched. Fetch up to `max_s8_pairs_per_batch * 2` rows, expand pairs, stop at cap. Watermark advances only to last fully-processed row's event_id. | RISK-TEST-STRATEGY.md R-10 | architecture/ADR-003-s8-watermark-strategy.md |
| S8 quarantine filter method | Bulk pre-fetch: `SELECT id FROM entries WHERE id IN (...) AND status != 3` using chunked QueryBuilder. Reduces O(pairs) round-trips to O(1) bulk fetch. Chunks required to stay under SQLite 999-parameter limit (entry #3442). | RISK-TEST-STRATEGY.md §Integration Risks | architecture/ADR-003-s8-watermark-strategy.md |
| Eval gate scope | No new GraphCohesionMetrics fields needed. Eval reads existing `cross_category_edge_count` and `isolated_entry_count` via `context_status` after at least one complete tick post-delivery. | ALIGNMENT-REPORT.md VARIANCE-01 (resolved) | architecture/ADR-004-graphcohesionmetrics-extension.md |
| Near-threshold oscillation | Additive-only policy. Once written, S1/S2 edges persist until endpoint deletion or quarantine. No per-tick diff on tag-count drops. OQ-03 resolved: existing compaction (`background.rs:513`) is source-agnostic — S1/S2/S8 edges are removed when endpoints are quarantined/deleted, identical to NLI/co_access edges. | SCOPE.md §Design Decision 7 | — |

---

## Prerequisite Gate (MANDATORY — Run Before Writing Any Call Site)

Before implementing any S1/S2/S8 call sites, verify `write_graph_edge` exists:

```
grep -n "pub(crate) async fn write_graph_edge" \
    crates/unimatrix-server/src/services/nli_detection.rs
```

If absent, adding it is the first implementation step. The function signature must be:

```rust
pub(crate) async fn write_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f64,
    created_at: u64,
    source: &str,
    metadata: Option<&str>,
) -> bool
```

`write_nli_edge` must NOT be reused — it hardcodes `source='nli'` and would silently retag S1/S2/S8 edges as NLI-origin, corrupting GNN feature construction (entry #4025, R-07).

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | CREATE | New module with `run_graph_enrichment_tick`, `run_s1_tick`, `run_s2_tick`, `run_s8_tick`. Module constant `S8_WATERMARK_KEY`. Must stay under 500 lines; split tests to `graph_enrichment_tick_tests.rs` if needed. |
| `crates/unimatrix-server/src/services/graph_enrichment_tick_tests.rs` | CREATE (conditional) | Test helpers and integration tests extracted from the main module if it would exceed 500 lines. |
| `crates/unimatrix-server/src/services/mod.rs` | MODIFY | Register `graph_enrichment_tick` module. |
| `crates/unimatrix-server/src/background.rs` | MODIFY | Import and call `run_graph_enrichment_tick` after `run_graph_inference_tick`. Update tick-ordering invariant comment. S8 gate: `current_tick % config.s8_batch_interval_ticks == 0`. |
| `crates/unimatrix-server/src/infra/config.rs` | MODIFY | Add five new `InferenceConfig` fields: `s2_vocabulary`, `max_s1_edges_per_tick`, `max_s2_edges_per_tick`, `s8_batch_interval_ticks`, `max_s8_pairs_per_batch`. Update struct declaration, `default_*()` fns, `impl Default`, `validate()`, `merge_configs()`. Add `test_inference_config_s1_s2_s8_defaults_match_serde`. |
| `crates/unimatrix-store/src/read.rs` | MODIFY | Add `pub const EDGE_SOURCE_S1: &str = "S1"`, `pub const EDGE_SOURCE_S2: &str = "S2"`, `pub const EDGE_SOURCE_S8: &str = "S8"`, following the pattern for `EDGE_SOURCE_NLI` and `EDGE_SOURCE_CO_ACCESS`. |
| `crates/unimatrix-store/src/lib.rs` | MODIFY | Re-export `EDGE_SOURCE_S1`, `EDGE_SOURCE_S2`, `EDGE_SOURCE_S8` from crate root. |
| `crates/unimatrix-server/src/services/nli_detection.rs` | MODIFY (conditional) | If crt-040 did not ship `write_graph_edge`, add it here as first implementation step, with `write_nli_edge` delegating to it. |

---

## Data Structures

### New `InferenceConfig` Fields (in `infra/config.rs`)

```rust
// crt-041: graph enrichment tick fields
/// S2 vocabulary — domain terms for structural vocabulary matching.
/// Default: empty (S2 is a no-op). Operator opt-in.
/// Recommended software-engineering starting point (from ASS-038):
/// ["migration", "schema", "performance", "async", "authentication",
///  "cache", "api", "confidence", "graph"]
#[serde(default = "default_s2_vocabulary")]
pub s2_vocabulary: Vec<String>,

/// S1 per-tick edge write cap. Default: 200. Range: [1, 10000].
#[serde(default = "default_max_s1_edges_per_tick")]
pub max_s1_edges_per_tick: usize,

/// S2 per-tick edge write cap. Default: 200. Range: [1, 10000].
#[serde(default = "default_max_s2_edges_per_tick")]
pub max_s2_edges_per_tick: usize,

/// S8 batch frequency: runs every N ticks. Default: 10. Range: [1, 1000].
/// At default tick interval (~15 min), this is once per ~150 minutes.
#[serde(default = "default_s8_batch_interval_ticks")]
pub s8_batch_interval_ticks: u32,

/// S8 per-batch pair cap. Default: 500. Range: [1, 10000].
/// Cap applies to pairs expanded from audit_log rows, not to row count.
#[serde(default = "default_max_s8_pairs_per_batch")]
pub max_s8_pairs_per_batch: usize,
```

Dual-site defaults (both `default_*()` fn and `impl Default` must agree):

| Field | Default Value |
|-------|--------------|
| `s2_vocabulary` | `vec![]` |
| `max_s1_edges_per_tick` | `200` |
| `max_s2_edges_per_tick` | `200` |
| `s8_batch_interval_ticks` | `10` |
| `max_s8_pairs_per_batch` | `500` |

### `graph_edges` Table (schema v19, unchanged)

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

### Edge Write Parameters by Source

| Source | relation_type | source | weight | created_by | bootstrap_only |
|--------|--------------|--------|--------|-----------|----------------|
| S1 | `'Informs'` | `EDGE_SOURCE_S1` = `'S1'` | `min(shared_tags * 0.1, 1.0)` | `'s1'` | `0` |
| S2 | `'Informs'` | `EDGE_SOURCE_S2` = `'S2'` | `min(shared_terms * 0.1, 1.0)` | `'s2'` | `0` |
| S8 | `'CoAccess'` | `EDGE_SOURCE_S8` = `'S8'` | `0.25` (fixed) | `'s8'` | `0` |

### S8 Watermark Key

```rust
const S8_WATERMARK_KEY: &str = "s8_audit_log_watermark";
```

Stored in the `counters` table. Read via `counters::get`; written via `counters::set`. Default 0 (first run). Written AFTER edge inserts, not before.

---

## Function Signatures

### `graph_enrichment_tick.rs` (new)

```rust
/// Top-level entry point called from background.rs after run_graph_inference_tick.
/// Runs S1, S2, then conditionally S8 in fixed order.
pub(crate) async fn run_graph_enrichment_tick(
    store: &Store,
    config: &InferenceConfig,
    current_tick: u64,
)

/// S1 — tag co-occurrence Informs edges.
/// Runs every tick. No-op if no qualifying pairs above threshold.
pub(crate) async fn run_s1_tick(store: &Store, config: &InferenceConfig)

/// S2 — structural vocabulary Informs edges.
/// Runs every tick. Immediate no-op if config.s2_vocabulary is empty.
pub(crate) async fn run_s2_tick(store: &Store, config: &InferenceConfig)

/// S8 — search co-retrieval CoAccess edges.
/// Gated: current_tick % config.s8_batch_interval_ticks == 0.
pub(crate) async fn run_s8_tick(store: &Store, config: &InferenceConfig)
```

### `nli_detection.rs` (conditional add if crt-040 absent)

```rust
/// Generalized graph edge writer. Returns true if a new edge was inserted.
/// write_nli_edge delegates to this with source="nli", created_by="nli".
pub(crate) async fn write_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f64,
    created_at: u64,
    source: &str,
    metadata: Option<&str>,
) -> bool
```

### `read.rs` constants (new)

```rust
pub const EDGE_SOURCE_S1: &str = "S1";
pub const EDGE_SOURCE_S2: &str = "S2";
pub const EDGE_SOURCE_S8: &str = "S8";
```

---

## S1 SQL Pattern

Self-join on `entry_tags`. Dual-endpoint quarantine guard via JOIN on `entries` for both sides. `LIMIT` applied before GROUP BY result is fully materialized (delivery agent must verify query plan — `EXPLAIN QUERY PLAN` required, per R-04/NFR-03).

```sql
SELECT t1.entry_id AS source_id,
       t2.entry_id AS target_id,
       COUNT(*)    AS shared_tags
FROM entry_tags t1
JOIN entry_tags t2 ON t2.tag = t1.tag AND t2.entry_id > t1.entry_id
JOIN entries e1 ON e1.id = t1.entry_id AND e1.status = 0
JOIN entries e2 ON e2.id = t2.entry_id AND e2.status = 0
GROUP BY t1.entry_id, t2.entry_id
HAVING COUNT(*) >= 3
ORDER BY shared_tags DESC
LIMIT ?max_s1_edges_per_tick
```

Weight: `f64::min(shared_tags as f64 * 0.1, 1.0)` — computed in Rust after fetch.

## S2 SQL Pattern

Built dynamically using `sqlx::QueryBuilder::push_bind`. One CASE WHEN per vocabulary term per entry. Terms are NEVER interpolated as SQL string literals.

```rust
// SECURITY: vocabulary terms are push_bind parameters, never interpolated.
// Using string format!() for terms would introduce SQL injection via config.toml.
let mut qb = sqlx::QueryBuilder::new(
    "SELECT source_id, target_id, (s1_count + s2_count) AS shared_terms FROM (\
     SELECT e1.id AS source_id, e2.id AS target_id, ("
);
for term in &config.s2_vocabulary {
    qb.push("CASE WHEN instr(lower(' ' || e1.content || ' ' || e1.title || ' '), \
              lower(' ' || ");
    qb.push_bind(term.as_str());
    qb.push(" || ' ')) > 0 THEN 1 ELSE 0 END + ");
}
// ... repeat for e2, close subquery, add WHERE/ORDER BY/LIMIT
qb.push(") WHERE s1_count + s2_count >= 2 ORDER BY shared_terms DESC LIMIT ");
qb.push_bind(config.max_s2_edges_per_tick as i64);
```

S2 early return when vocabulary is empty:

```rust
if config.s2_vocabulary.is_empty() {
    return; // no-op; no SQL issued
}
```

Weight: `f64::min(shared_terms as f64 * 0.1, 1.0)` — computed in Rust.

## S8 Batch Algorithm

```
1. Load watermark from counters (key 's8_audit_log_watermark', default 0)
2. SELECT event_id, target_ids FROM audit_log
   WHERE operation = 'context_search' AND outcome = 0 AND event_id > watermark
   ORDER BY event_id ASC
   LIMIT max_s8_pairs_per_batch * 2   -- generous upper bound on rows
3. For each row:
   a. Parse target_ids JSON as Vec<u64>
   b. On parse failure: log warn! with event_id; mark row as processed; continue
   c. Build unordered pairs (a, b) where a < b
   d. Accumulate pairs; stop expanding when total pairs >= max_s8_pairs_per_batch
      (partial-row: watermark advances only to last fully-processed row's event_id)
4. Bulk validate: SELECT id FROM entries WHERE id IN (...chunk...) AND status = 0
   (chunked to stay under SQLite 999-parameter limit; see entry #3442)
   Build HashSet of valid entry IDs.
5. For each pair where both IDs in HashSet: write_graph_edge(..., source='S8', weight=0.25)
6. UPDATE watermark to MAX(event_id) of all processed rows (including parse-failed rows)
   -- watermark advances past malformed rows to prevent infinite re-scan (ADR-003)
7. Log tracing::info! with pairs_written, pairs_skipped_quarantined, new_watermark
```

---

## Tick Ordering (After crt-041)

```
compaction
→ co_access_promotion
→ graph-rebuild (TypedGraphState::rebuild)
→ PhaseFreqTable::rebuild
→ contradiction_scan (if embed adapter ready && tick_multiple)
→ extraction_tick
→ structural_graph_tick / run_graph_inference_tick   (always)
→ run_graph_enrichment_tick                          (always)
    → run_s1_tick                                    (always)
    → run_s2_tick                                    (always; no-op when s2_vocabulary empty)
    → run_s8_tick                                    (gated: tick % s8_batch_interval_ticks == 0)
```

S1/S2/S8 run AFTER `TypedGraphState::rebuild`. New edges are visible to PPR at the NEXT tick's rebuild (one-tick delay, same as co_access_promotion). The eval gate must run after at least one complete tick post-delivery.

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | No ML model. Pure SQL only. No ONNX, no rayon, no spawn_blocking. |
| C-02 | No schema migration. All writes to existing `graph_edges` (schema v19) and `counters` tables. Schema version stays at 19. |
| C-03 | Dual-endpoint quarantine guard mandatory on ALL three sources. JOIN `entries` on BOTH `source_id` and `target_id` filtering `status != 3`. Omitting either JOIN silently writes edges to quarantined entries (production bug, entry #3981). |
| C-04 | Additive-only edges. No reconciliation pass. S1/S2 edges persist until endpoint deletion or quarantine. Tag-drop reconciliation deferred (OQ-03 must be documented, not implemented). |
| C-05 | S2 parameterized SQL only. Vocabulary terms must be `push_bind` parameters in every code path. String interpolation prohibited (SR-01). |
| C-06 | write_graph_edge prerequisite gate. Verify function exists before writing any call site. write_nli_edge must NOT be reused. |
| C-07 | InferenceConfig dual-maintenance. Both `default_*()` backing fn AND `impl Default` struct literal must be updated atomically for all five new fields. |
| C-08 | validate() range checks mandatory for all four numeric fields (lower bound 1, not 0 — a value of 0 causes `LIMIT 0` silent disable or `% 0` panic). |
| C-09 | `graph_enrichment_tick.rs` must not exceed 500 lines. Extract tests to `graph_enrichment_tick_tests.rs` if needed. |
| C-10 | `inferred_edge_count` in `GraphCohesionMetrics` must continue to count only `source = 'nli'` edges. S1/S2/S8 edges must not be counted in that field. |
| C-11 | S8 watermark must be updated AFTER all edge writes for the batch. Writing before creates a gap where crash causes permanent loss. |
| C-12 | S8 batch cap is on pairs, not on audit_log rows fetched. Partial-row: watermark advances only to last fully-processed row's event_id. |
| C-13 | S8 quarantine filter uses chunked bulk SELECT (chunks of ≤999 IDs) to stay under SQLite's parameter limit (entry #3442). |
| C-14 | S8 malformed JSON: log warn! with event_id and advance watermark past that row. Do not leave watermark stuck behind a single malformed row. |
| C-15 | Eval gate requires at least one complete background tick post-delivery. New edges are not visible in PPR graph until `TypedGraphState::rebuild` runs on the following tick (SR-09). |

---

## Dependencies

| Dependency | Version | Usage |
|-----------|---------|-------|
| `sqlx` | workspace | `QueryBuilder::push_bind` for S2 dynamic SQL; pool access; row fetch |
| `serde` | workspace | `InferenceConfig` field serialization |
| `tracing` | workspace | `info!` summary logs; `warn!` on errors |
| `serde_json` | workspace | S8 `target_ids` JSON array parsing |

No new crate dependencies. All are already in the workspace manifest.

### Crt-040 Hard Prerequisite

`write_graph_edge(source: &str, ...)` must exist in `crates/unimatrix-server/src/services/nli_detection.rs`. This is a hard gate — delivery cannot proceed to S1/S2/S8 call sites without it. Run the verification grep before starting wave 1.

---

## NOT in Scope

- PPR expander (Group 4) — this feature builds the edge density that makes PPR viable; it does not implement PPR.
- S3, S4, S5 edge sources — fewer than 20 pairs at current corpus size; deferred until corpus ≥3,000 (SCOPE.md §Non-Goals).
- Behavioral Informs edges (Group 6) — depends on Group 5 infrastructure not yet shipped.
- New GraphCohesionMetrics fields — `cross_category_edge_count` and `isolated_entry_count` already exist from col-029 (ADR-004). No new fields added by crt-041.
- Per-source edge count breakdowns in `context_status` — deferred; would require new metrics fields.
- Tag-drop edge reconciliation — additive-only policy (Design Decision 7). OQ-03 resolved: existing compaction (`background.rs:513`) is source-agnostic and covers S1/S2/S8 edges. No reconciliation pass needed; no deferred work.
- `schema_version` migration — schema v19 is used as-is; no migration file required.
- cosine Supports detection — shipped in crt-040.
- S2 out-of-the-box vocabulary — empty default (domain-agnostic per W0-3). Operator must configure `s2_vocabulary` in `config.toml` to enable S2.

---

## Alignment Status

Source: ALIGNMENT-REPORT.md (2026-04-02)

**Overall: PASS with two documentation variances, both resolved before synthesis. No variances require human action at delivery start.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly serves W3-1 GNN edge-type tagging; signal_origin tagging via `source` column preserved; domain-agnostic S2 empty default satisfies W0-3. |
| Milestone Fit | PASS | Correctly positioned in Wave 1 intelligence foundation; no future-milestone scope creep. |
| Architecture Consistency | PASS | All five ADRs internally consistent; tick ordering, module placement, and integration points align across ARCHITECTURE.md and SPECIFICATION.md. |
| Risk Completeness | PASS | RISK-TEST-STRATEGY.md covers all 17 risks with scenario-level traceability. |
| Scope Gap (resolved) | RESOLVED | SCOPE.md §AC-17 listed the 9-term default for `s2_vocabulary`; §Design Decision 3 (same file) overrides to empty. All three source documents correctly implement empty default. Delivery follows empty default. |
| Spec-ADR variance (resolved) | RESOLVED | SPECIFICATION.md FR-33 declared two new `GraphCohesionMetrics` fields. ADR-004 established both fields already exist from col-029. ALIGNMENT-REPORT.md classifies this as blocking; both documents are now reconciled — delivery agent must NOT add new fields, as both are already present. |

**Delivery start instruction:** Confirm `cross_category_edge_count` and `isolated_entry_count` exist in `GraphCohesionMetrics` before touching `read.rs`. If they exist (col-029), skip the store-layer change entirely. If they are somehow absent, add them per the col-029 ADR-002 SQL definitions before proceeding.
