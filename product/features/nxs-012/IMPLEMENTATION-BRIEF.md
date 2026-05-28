# Implementation Brief: nxs-012 — Export/Import Complete Persistent State Coverage

GH Issue: #631

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nxs-012/SCOPE.md |
| Architecture | product/features/nxs-012/architecture/ARCHITECTURE.md |
| Specification | product/features/nxs-012/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nxs-012/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nxs-012/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| format-types | pseudocode/format-types.md | test-plan/format-types.md |
| export-functions | pseudocode/export-functions.md | test-plan/export-functions.md |
| import-inserters | pseudocode/import-inserters.md | test-plan/import-inserters.md |
| import-pipeline | pseudocode/import-pipeline.md | test-plan/import-pipeline.md |
| skip-quarantined | pseudocode/skip-quarantined.md | test-plan/skip-quarantined.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Extend the export/import CLI pipeline from 8 tables to 11 by adding `graph_edges`, `observations`, and `cycle_events`, bumping `format_version` from 1 to 2. Additionally, add a `--skip-quarantined` flag on the **export** subcommand (with `--confirm` safeguard) that omits quarantined entries (status=3) and all rows referencing them, producing a clean snapshot for database rebuilds. Import remains a simple full-restore with no filtering logic.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| FK-safe DELETE ordering in drop_all_data | Delete observation_phase_metrics, observation_metrics before observations; explicit ordering independent of PRAGMA foreign_keys state | SR-07 | architecture/ADR-001-drop-all-data-fk-safe-ordering.md |
| Format version acceptance range | Accept 1 (legacy) and 2 (current), reject 0 and 3+; export always writes 2 | SR-04 | architecture/ADR-002-format-version-acceptance-range.md |
| f64 NaN safety for graph_edges.weight | Number::from_f64 with fallback to 1.0 (column DEFAULT), not 0 | SR-01 | architecture/ADR-003-f64-nan-safety-graph-edges-weight.md |
| goal_embedding exclusion strategy | Exclude from SELECT entirely; CycleEventRow has 9 fields, no goal_embedding; inserter binds NULL explicitly | SR-03 | architecture/ADR-004-goal-embedding-exclusion-strategy.md |
| graph_edges.id omission | Synthetic AUTOINCREMENT id omitted from export (unreferenced); SQLite assigns fresh ids on import | Constraint #5 | architecture/ADR-005-graph-edges-id-omission.md |
| observations.id and cycle_events.id preservation | Export id, import with explicit id binding (watermark/ordering significance); plain INSERT surfaces duplicates | Constraint #5 | architecture/ADR-006-observations-cycle-events-id-preservation.md |
| Export-side skip-quarantined filter | Pre-query HashSet<i64> built inside DEFERRED transaction; passed to 5 affected exporters; supersedes ADR-007 (import-side design) | SR-02, SR-08 | architecture/ADR-008-export-side-skip-quarantined.md |
| --confirm safeguard for --skip-quarantined | CLI flag (not interactive prompt); abort before DB access if missing; silently ignored without --skip-quarantined | SR-09 | architecture/ADR-009-confirm-safeguard-for-skip-quarantined.md |

## Files to Create/Modify

All files are in `crates/unimatrix-server/src/`. No new files created.

| File | Action | Summary |
|------|--------|---------|
| `format.rs` | Modify | Add GraphEdgeRow (9 fields), ObservationRow (10 fields), CycleEventRow (9 fields) structs; add 3 ExportRow enum variants |
| `export.rs` | Modify | Add export_graph_edges, export_observations, export_cycle_events functions; bump format_version to 2; add skip_ids parameter to do_export and 5 affected exporters; add --skip-quarantined and --confirm CLI flags; add skip-set construction query inside DEFERRED transaction; add skip summary reporting |
| `import/inserters.rs` | Modify | Add insert_graph_edge (plain INSERT, 9 cols), insert_observation (explicit id, 10 cols), insert_cycle_event (explicit id, 9 cols + NULL goal_embedding) |
| `import/mod.rs` | Modify | Add 3 fields to ImportCounts; add 3 match arms to ingest_rows; extend drop_all_data with 5 DELETEs (observation_phase_metrics, observation_metrics, graph_edges, observations, cycle_events); extend print_summary; update format_version validation to accept 1 or 2; extend record_provenance detail string |

## Data Structures

### New Row Structs (format.rs)

```rust
#[derive(Debug, Deserialize)]
pub struct GraphEdgeRow {
    pub source_id: i64,
    pub target_id: i64,
    pub relation_type: String,
    pub weight: f64,
    pub created_at: i64,
    pub created_by: String,
    pub source: String,
    pub bootstrap_only: i64,
    pub metadata: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ObservationRow {
    pub id: i64,
    pub session_id: String,
    pub ts_millis: i64,
    pub hook: String,
    pub tool: Option<String>,
    pub input: Option<String>,
    pub response_size: Option<i64>,
    pub response_snippet: Option<String>,
    pub topic_signal: Option<String>,
    pub phase: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CycleEventRow {
    pub id: i64,
    pub cycle_id: String,
    pub seq: i64,
    pub event_type: String,
    pub phase: Option<String>,
    pub outcome: Option<String>,
    pub next_phase: Option<String>,
    pub timestamp: i64,
    pub goal: Option<String>,
}
```

### ExportRow Enum (format.rs) — 3 New Variants

```rust
#[serde(rename = "graph_edges")]
GraphEdge(GraphEdgeRow),

#[serde(rename = "observations")]
Observation(ObservationRow),

#[serde(rename = "cycle_events")]
CycleEvent(CycleEventRow),
```

### ImportCounts (import/mod.rs) — 3 New Fields

```rust
pub graph_edges: u64,
pub observations: u64,
pub cycle_events: u64,
```

### Skip Set (export.rs)

```rust
// Built inside BEGIN DEFERRED transaction, before any export_* call
let skip_ids: HashSet<i64> = if skip_quarantined {
    sqlx::query_scalar("SELECT id FROM entries WHERE status = 3")
        .fetch_all(&mut *conn).await?
        .into_iter().collect()
} else {
    HashSet::new()
};
```

## Function Signatures

### New Export Functions (export.rs)

```rust
async fn export_graph_edges(
    pool: &SqlitePool, writer: &mut impl Write, skip_ids: &HashSet<i64>
) -> Result<ExportCounts, Box<dyn Error>>

async fn export_observations(
    pool: &SqlitePool, writer: &mut impl Write
) -> Result<u64, Box<dyn Error>>

async fn export_cycle_events(
    pool: &SqlitePool, writer: &mut impl Write
) -> Result<u64, Box<dyn Error>>
```

### Modified Export Functions (export.rs) — gain skip_ids parameter

```rust
async fn do_export(
    pool: &SqlitePool, writer: &mut impl Write, skip_ids: &HashSet<i64>
) -> Result<(), Box<dyn Error>>

// 4 existing exporters gain skip_ids:
async fn export_entries(pool, writer, skip_ids: &HashSet<i64>) -> ...
async fn export_entry_tags(pool, writer, skip_ids: &HashSet<i64>) -> ...
async fn export_co_access(pool, writer, skip_ids: &HashSet<i64>) -> ...
async fn export_feature_entries(pool, writer, skip_ids: &HashSet<i64>) -> ...
```

### Modified Public Entry Points (export.rs)

```rust
pub fn run_export(
    project_dir: Option<&Path>, output: Option<&Path>,
    skip_quarantined: bool, confirm: bool
) -> Result<(), Box<dyn Error>>

pub fn run_export_with_base(
    project_dir: Option<&Path>, output: Option<&Path>, base_dir: &Path,
    skip_quarantined: bool, confirm: bool
) -> Result<(), Box<dyn Error>>
```

### New Inserter Functions (import/inserters.rs)

```rust
pub async fn insert_graph_edge(
    conn: &mut SqliteConnection, r: &GraphEdgeRow
) -> Result<(), Box<dyn Error>>

pub async fn insert_observation(
    conn: &mut SqliteConnection, r: &ObservationRow
) -> Result<(), Box<dyn Error>>

pub async fn insert_cycle_event(
    conn: &mut SqliteConnection, r: &CycleEventRow
) -> Result<(), Box<dyn Error>>
```

## Export Column Mappings

### graph_edges (9 columns, ORDER BY source_id, target_id, relation_type)

| JSON key | SQL column | Type | Nullable | Notes |
|----------|-----------|------|----------|-------|
| `source_id` | source_id | i64 | no | |
| `target_id` | target_id | i64 | no | |
| `relation_type` | relation_type | String | no | |
| `weight` | weight | f64 | no | Number::from_f64 with NaN->1.0 fallback (ADR-003) |
| `created_at` | created_at | i64 | no | |
| `created_by` | created_by | String | no | |
| `source` | source | String | no | |
| `bootstrap_only` | bootstrap_only | i64 | no | |
| `metadata` | metadata | String/null | yes | nullable_text helper |

### observations (10 columns, ORDER BY id)

| JSON key | SQL column | Type | Nullable | Notes |
|----------|-----------|------|----------|-------|
| `id` | id | i64 | no | preserved through import (ADR-006) |
| `session_id` | session_id | String | no | |
| `ts_millis` | ts_millis | i64 | no | |
| `hook` | hook | String | no | |
| `tool` | tool | String/null | yes | nullable_text |
| `input` | input | String/null | yes | nullable_text |
| `response_size` | response_size | i64/null | yes | nullable_int |
| `response_snippet` | response_snippet | String/null | yes | nullable_text |
| `topic_signal` | topic_signal | String/null | yes | nullable_text |
| `phase` | phase | String/null | yes | nullable_text |

### cycle_events (9 columns, ORDER BY id)

| JSON key | SQL column | Type | Nullable | Notes |
|----------|-----------|------|----------|-------|
| `id` | id | i64 | no | preserved through import (ADR-006) |
| `cycle_id` | cycle_id | String | no | |
| `seq` | seq | i64 | no | |
| `event_type` | event_type | String | no | |
| `phase` | phase | String/null | yes | nullable_text |
| `outcome` | outcome | String/null | yes | nullable_text |
| `next_phase` | next_phase | String/null | yes | nullable_text |
| `timestamp` | timestamp | i64 | no | |
| `goal` | goal | String/null | yes | nullable_text |

## drop_all_data DELETE Order (ADR-001)

```sql
-- Existing (unchanged)
DELETE FROM entry_tags;
DELETE FROM co_access;
DELETE FROM feature_entries;
DELETE FROM outcome_index;
DELETE FROM agent_registry;
DELETE FROM vector_map;

-- NEW: derived metric tables (FK-safe ordering)
DELETE FROM observation_phase_metrics;
DELETE FROM observation_metrics;

-- NEW: 3 exported tables
DELETE FROM graph_edges;
DELETE FROM observations;
DELETE FROM cycle_events;

-- Existing (unchanged)
DELETE FROM entries;
DELETE FROM counters;
```

## Skip-Quarantined Filter Cascade (ADR-008)

| Table Exporter | Check Column(s) | Action When Match |
|----------------|-----------------|-------------------|
| export_entries | id (in skip_ids) | Skip write_row |
| export_entry_tags | entry_id | Skip write_row |
| export_feature_entries | entry_id | Skip write_row |
| export_co_access | entry_id_a OR entry_id_b | Skip write_row |
| export_graph_edges | source_id OR target_id | Skip write_row |
| export_observations | (none) | Always write |
| export_cycle_events | (none) | Always write |
| export_counters | (none) | Always write |
| export_outcome_index | (none) | Always write |
| export_agent_registry | (none) | Always write |
| export_audit_log | (none) | Always write |

## Format Version Validation (ADR-002)

```rust
match header.format_version {
    1 | 2 => Ok(()),
    v => Err(format!(
        "unsupported format_version: {v}. This binary supports format_version 1 and 2."
    ))
}
```

## Constraints

1. **Schema v27 stable** — all 3 target tables exist; no migration changes needed
2. **format.rs tagged enum backward incompatibility** — new ExportRow variants unreadable by old binaries; format_version 2 guard prevents silent failures
3. **Transaction isolation** — export: BEGIN DEFERRED (read snapshot); import: BEGIN IMMEDIATE (write lock); new tables participate in same transactions
4. **AUTOINCREMENT id divergence** — graph_edges.id omitted (ADR-005); observations.id and cycle_events.id preserved (ADR-006)
5. **BLOB exclusion** — cycle_events.goal_embedding excluded from export (ADR-004); inserter binds NULL
6. **f64 NaN safety** — graph_edges.weight uses Number::from_f64 with 1.0 fallback (ADR-003)
7. **Plain INSERT for graph_edges** — surfaces UNIQUE constraint violations as data corruption detection
8. **audit_log append-only trigger** — drop_all_data excludes audit_log; new tables have no such triggers
9. **Skip-set query inside DEFERRED snapshot** — prevents TOCTOU races (SR-02, ADR-008)
10. **Consistent skip-set checking across 5 exporters** — missed check produces orphaned rows (SR-08)
11. **--confirm is a CLI flag, not interactive** — automation/CI compatible (ADR-009)

## Dependencies

### Crates
- `sqlx` — all database queries (export reads, import writes)
- `serde` / `serde_json` — JSONL serialization, ExportRow tagged enum deserialization
- `unimatrix-store` — SqlxStore, PoolConfig, compute_content_hash, AuditEvent

### Existing Components
- `crates/unimatrix-server/src/export.rs` — do_export, write_header, write_row, nullable_int, nullable_text
- `crates/unimatrix-server/src/format.rs` — ExportHeader, ExportRow enum, per-table row structs
- `crates/unimatrix-server/src/import/mod.rs` — ImportCounts, ingest_rows, drop_all_data, print_summary, record_provenance
- `crates/unimatrix-server/src/import/inserters.rs` — per-table insert_* functions

### External Services
None. Export/import is a local CLI operation.

## NOT in Scope

- **goal_embedding BLOB export** — model-version-specific bincode; lazy reconstruction post-import
- **bootstrap_only filtering** — all edges exported regardless of flag
- **Observations filtering/compaction** — retention GC already bounds the set
- **observation_metrics / observation_phase_metrics export** — derived aggregates recomputed from raw observations
- **shadow_evaluations / query_log / sessions export** — operational/ephemeral tables per nan-001/nan-002
- **Merge/append import mode** — import is full restore only
- **Streaming/pagination** — all tables use fetch_all
- **Import-side quarantine filtering** — filtering moved to export side (ADR-007 SUPERSEDED by ADR-008)
- **Interactive confirmation prompts** — CLI flags only (ADR-009)

## Alignment Status

**Overall: PASS** — upholds all vision non-negotiables (hash chain integrity, immutable audit log, ACID storage, graceful degradation).

Two informational WARNs, neither requiring approval:

1. **FR-17 scope addition**: SPECIFICATION.md adds record_provenance for new table counts (no SCOPE.md AC). Justified by vision audit log completeness principle.
2. **R-14 traceability gap**: Risk R-14 (transaction isolation) has no corresponding scope risk entry. Test coverage is present (2 scenarios). Architecture risks are not required to trace back to scope risks.

No VARIANCE or FAIL findings. No vision variances to resolve.
