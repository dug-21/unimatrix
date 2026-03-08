# nxs-009: Implementation Brief

**Feature**: nxs-009 — Observation Metrics Normalization
**GH Issue**: #103

---

## What

Decompose the `observation_metrics` bincode blob into SQL columns. The last remaining bincode blob in the schema becomes a 23-column table (`observation_metrics`) plus a junction table (`observation_phase_metrics`) for variable-shape phase data. Schema advances from v8 to v9.

## Why

- Metrics locked in opaque blobs prevent SQL analytics, cross-feature queries, and external tooling
- col-015 (E2E validation) needs SQL assertions on metric data
- Graph enablement milestone needs JOINable metrics for correlation analysis
- Eliminates unnecessary bincode serialize/deserialize in the retrospective hot path

## Key Decisions

| Decision | ADR | Rationale |
|----------|-----|-----------|
| MetricVector types in unimatrix-store | ADR-001 | Store is leaf crate; cannot import from core. Same pattern as EntryRecord. Re-exports maintain all import paths. |
| Self-contained migration deserializer | ADR-002 | No new dependency edges. Frozen snapshot of v8 format in migration_compat.rs. Same pattern as nxs-008. |
| Phase metrics FK with CASCADE | ADR-003 | Referential integrity. Same pattern as entry_tags. PRAGMA foreign_keys already ON. |

## Implementation Order

### Wave 1 (single wave — all changes tightly coupled)

**Step 1: Add types to unimatrix-store**
- Create `crates/unimatrix-store/src/metrics.rs`
- Define `MetricVector`, `UniversalMetrics`, `PhaseMetrics` with serde derives and `#[serde(default)]`
- Add `pub mod metrics;` to store's `lib.rs`
- Re-export from `lib.rs`: `pub use metrics::{MetricVector, UniversalMetrics, PhaseMetrics};`

**Step 2: Update schema in db.rs**
- Replace `observation_metrics` table definition (23 columns, no BLOB)
- Add `observation_phase_metrics` table definition (junction table with FK CASCADE)

**Step 3: Add migration v8→v9**
- In `migration_compat.rs`: add `deserialize_metric_vector_v8()` with self-contained serde structs
- In `migration.rs`: add v8→v9 migration block
  - Read all (feature_cycle, data BLOB) rows
  - Deserialize each via compat deserializer (skip+default on failure)
  - Backup database file at `{path}.v8-backup`
  - DROP old table, CREATE new tables, INSERT migrated data
  - Bump schema_version to 9
- Update `CURRENT_SCHEMA_VERSION` to 9

**Step 4: Update store read/write API**
- `write_ext.rs`: Change `store_metrics(&str, &[u8])` to `store_metrics(&str, &MetricVector)`
  - Transaction: INSERT OR REPLACE parent + DELETE+INSERT phases
- `read.rs`: Change `get_metrics(&str) -> Option<Vec<u8>>` to `get_metrics(&str) -> Option<MetricVector>`
  - Two queries: parent row + phase rows
- `read.rs`: Change `list_all_metrics() -> Vec<(String, Vec<u8>)>` to `list_all_metrics() -> Vec<(String, MetricVector)>`
  - Two queries with single-pass merge

**Step 5: Update unimatrix-observe**
- `types.rs`: Remove `MetricVector`, `UniversalMetrics`, `PhaseMetrics` definitions
- `types.rs`: Remove `serialize_metric_vector()`, `deserialize_metric_vector()`
- `types.rs`: Add re-exports: `pub use unimatrix_store::{MetricVector, UniversalMetrics, PhaseMetrics};`
- Remove bincode roundtrip tests for MetricVector (they now live in store)

**Step 6: Update unimatrix-core**
- `lib.rs`: Add re-export: `pub use unimatrix_store::{MetricVector, UniversalMetrics, PhaseMetrics};`

**Step 7: Update unimatrix-server**
- `mcp/tools.rs` (context_retrospective):
  - Remove `serialize_metric_vector()` call — pass `&MetricVector` to `store.store_metrics()`
  - Remove `deserialize_metric_vector()` call — use `MetricVector` directly from `store.get_metrics()`
  - Remove per-item `deserialize_metric_vector()` in `list_all_metrics()` loop — already typed
- `services/status.rs`: Update `list_all_metrics()` call site (type change only)

**Step 8: Update tests**
- `crates/unimatrix-store/tests/sqlite_parity.rs`: Update `test_store_and_get_metrics`, `test_list_all_metrics` to use typed API
- Add new tests per RISK-TEST-STRATEGY.md (roundtrip, replace, cascade, migration)
- Add column-field alignment structural test (R-03)

## Risks to Watch

1. **R-01 (CASCADE behavior)**: Test INSERT OR REPLACE with phases carefully
2. **R-05 (Bincode config parity)**: Migration deserializer must use `bincode::config::standard()` exactly
3. **R-03 (Column-field drift)**: Add structural test comparing SQL columns to Rust fields

## Files Changed

| File | Change Type |
|------|------------|
| `crates/unimatrix-store/src/metrics.rs` | NEW — type definitions |
| `crates/unimatrix-store/src/lib.rs` | EDIT — add mod + re-exports |
| `crates/unimatrix-store/src/db.rs` | EDIT — new table schema |
| `crates/unimatrix-store/src/migration.rs` | EDIT — add v8→v9 block, bump version |
| `crates/unimatrix-store/src/migration_compat.rs` | EDIT — add v8 deserializer |
| `crates/unimatrix-store/src/write_ext.rs` | EDIT — typed store_metrics |
| `crates/unimatrix-store/src/read.rs` | EDIT — typed get_metrics, list_all_metrics |
| `crates/unimatrix-store/tests/sqlite_parity.rs` | EDIT — update + new tests |
| `crates/unimatrix-observe/src/types.rs` | EDIT — remove defs, add re-exports |
| `crates/unimatrix-observe/src/lib.rs` | EDIT — update re-exports if needed |
| `crates/unimatrix-core/src/lib.rs` | EDIT — add re-exports |
| `crates/unimatrix-server/src/mcp/tools.rs` | EDIT — remove bincode calls |
| `crates/unimatrix-server/src/services/status.rs` | EDIT — type change |
