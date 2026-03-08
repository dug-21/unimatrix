# nxs-009: Scope Risk Assessment

**Feature**: nxs-009 — Observation Metrics Normalization
**Date**: 2026-03-08
**Mode**: Scope-Risk (Phase 1b)

> Note: Unimatrix was unavailable during risk assessment. Historical lesson-learned queries could not be executed. Risks are identified from codebase analysis and nxs-008 precedent.

---

## SR-01: Cross-Crate Type Migration Breaks Downstream Consumers

**Risk**: Moving `MetricVector`, `UniversalMetrics`, and `PhaseMetrics` from `unimatrix-observe` to `unimatrix-core` requires updating all import paths across unimatrix-observe, unimatrix-server, and any tests. The col-013 precedent (`ObservationRecord` move) provides a pattern, but MetricVector has more consumers: baseline computation, detection rules, metrics computation, report building, and the retrospective MCP tool.

**Likelihood**: Medium
**Impact**: Medium (compilation failures, missed import sites)
**Mitigation**: Re-export from `unimatrix-observe` for backward compatibility (same pattern as col-013). Enumerate all import sites before moving.

---

## SR-02: Bincode Deserialization During Migration May Fail on Corrupted Data

**Risk**: The v8-to-v9 migration must deserialize every existing bincode blob. If any blob is corrupted, truncated, or was written by a different bincode config version, the migration fails and blocks database opening. The nxs-008 migration had the same risk pattern — it handled it by skipping undeserializable entries. MetricVector uses `bincode::config::standard()` which is position-dependent; any struct field reordering between writes would cause silent data corruption.

**Likelihood**: Low (MetricVector has been stable since col-002)
**Impact**: High (database cannot open if migration fails)
**Mitigation**: Migration should skip/log corrupted blobs rather than aborting. Insert default MetricVector for unreadable entries. Test migration with both valid and intentionally corrupted blobs.

---

## SR-03: Two-Table Write Atomicity for Phase Metrics

**Risk**: Writing a MetricVector now requires INSERT into `observation_metrics` + N INSERTs into `observation_phase_metrics`. If the write is not transactional, a crash between the two operations leaves orphaned or incomplete data. The current `store_metrics` is a single INSERT OR REPLACE — the new version needs a transaction.

**Likelihood**: Medium
**Impact**: Medium (inconsistent metric data for one feature)
**Mitigation**: Wrap the multi-table write in a SQLite transaction. Use DELETE + INSERT pattern within the transaction (matching the INSERT OR REPLACE semantics of the current API). Foreign key CASCADE handles cleanup on delete.

---

## SR-04: UniversalMetrics Field Count Growth

**Risk**: UniversalMetrics currently has 21 fields. Each new detection rule or metric adds a column to the SQL table. Unlike the bincode blob (where `serde(default)` handles new fields transparently), adding a SQL column requires a schema migration. This could make future metric additions more expensive than today.

**Likelihood**: High (metrics are expected to grow as detection rules expand)
**Impact**: Low (ALTER TABLE ADD COLUMN is cheap in SQLite; migration pattern is well-established)
**Mitigation**: Accept this as a tradeoff. Document that new UniversalMetrics fields require a schema version bump. Consider whether a "custom metrics" overflow column (JSON) is warranted for rarely-queried metrics — but only if field growth becomes problematic.

---

## SR-05: Store Crate Dependency on Core Types Creates Coupling

**Risk**: Currently `unimatrix-store` has no dependency on `unimatrix-observe` and only a light dependency on `unimatrix-core` (for `EntryRecord` and related types). Adding `MetricVector` to the store's public API increases the coupling surface. If MetricVector gains complex dependencies (e.g., on detection types), they propagate to the store.

**Likelihood**: Low (MetricVector is a plain data struct with no behavior dependencies)
**Impact**: Low (unimatrix-core is already a shared dependency)
**Mitigation**: Move only the data structs (`MetricVector`, `UniversalMetrics`, `PhaseMetrics`) to unimatrix-core. Keep computation functions (`compute_metric_vector`, `compute_baselines`) in unimatrix-observe. The data types have no dependencies beyond serde and std collections.

---

## SR-06: Baseline Computation Performance Change

**Risk**: `list_all_metrics()` currently returns raw bytes. The caller deserializes only non-current features for baseline comparison. After normalization, `list_all_metrics()` returns fully constructed `MetricVector` objects including phase data from a JOIN query. If the table grows large, the JOIN could be slower than a simple blob fetch. However, the table is indexed by PRIMARY KEY and expected to remain small (one row per retrospected feature — currently dozens, eventually hundreds).

**Likelihood**: Low
**Impact**: Low (small table, infrequent operation)
**Mitigation**: Benchmark if >100 features exist. Consider a `list_universal_metrics_only()` variant that skips the phase JOIN if performance matters.

---

## SR-07: SQLite Column Count Limit

**Risk**: After normalization, `observation_metrics` will have 23 columns (1 PK + 1 computed_at + 21 universal metrics). SQLite's default SQLITE_MAX_COLUMN is 2000, so this is not a practical concern. However, if UniversalMetrics grows aggressively (see SR-04), very wide tables can affect query planning.

**Likelihood**: Very Low
**Impact**: Very Low
**Mitigation**: No action needed. 23 columns is well within normal SQLite usage.

---

## Top 3 Risks for Architect Attention

1. **SR-03 (Two-Table Write Atomicity)** — Architecture must specify transaction boundaries for the multi-table write pattern.
2. **SR-02 (Migration Deserialization Failure)** — Architecture must define error handling for corrupted blobs during v8→v9 migration.
3. **SR-01 (Cross-Crate Type Migration)** — Architecture must specify the type relocation strategy and backward-compatibility re-exports.
