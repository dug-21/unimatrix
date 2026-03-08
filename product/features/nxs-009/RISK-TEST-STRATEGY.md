# nxs-009: Risk-Test Strategy

**Feature**: nxs-009 — Observation Metrics Normalization
**Date**: 2026-03-08
**Mode**: Architecture-Risk (Phase 2a+)

---

## Scope Risk Traceability

| Scope Risk | Architecture Mitigation | Status |
|-----------|------------------------|--------|
| SR-01 (Cross-crate type migration) | ADR-001: Types in unimatrix-store with re-exports from observe and core | Mitigated |
| SR-02 (Bincode deserialization during migration) | ADR-002: Self-contained deserializer in migration_compat.rs; skip+default on failure | Mitigated |
| SR-03 (Two-table write atomicity) | Architecture specifies SQLite transaction wrapping for store_metrics; ADR-003: CASCADE FK | Mitigated |
| SR-04 (UniversalMetrics field count growth) | Accepted tradeoff; ALTER TABLE ADD COLUMN for future fields; documented in ARCHITECTURE.md | Accepted |
| SR-05 (Store crate dependency coupling) | ADR-001: Types stay in store (leaf crate), no new dependency edges | Mitigated |
| SR-06 (Baseline computation performance) | Two-query approach with single-pass merge; table expected small | Accepted |
| SR-07 (SQLite column count limit) | 23 columns well within 2000 limit | Non-issue |

---

## Architecture-Level Risks

### R-01: INSERT OR REPLACE Cascade Behavior

**Category**: Correctness
**Severity**: High
**Likelihood**: Medium

**Risk**: SQLite's `INSERT OR REPLACE` on a table with foreign key children triggers a DELETE+INSERT internally, which fires ON DELETE CASCADE on the child table. This means the explicit `DELETE FROM observation_phase_metrics WHERE feature_cycle = ?1` in the write path may be redundant — or worse, could conflict with the cascade timing within the transaction.

**Test Scenario**: Store a MetricVector with phases ["3a", "3b"]. Store a replacement with phases ["3a", "3c"]. Verify only ["3a", "3c"] exist after. Verify no orphaned rows.

**Mitigation**: The architecture already specifies DELETE+INSERT for phases within a transaction. The explicit DELETE is safe even if CASCADE also fires — deleting already-deleted rows is a no-op. Test must verify correctness.

### R-02: Migration Transaction Scope

**Category**: Data integrity
**Severity**: High
**Likelihood**: Low

**Risk**: The v8→v9 migration drops and recreates the `observation_metrics` table. If the transaction rolls back after the DROP but before re-creation, the table is lost. SQLite DDL within transactions is supported but behavior varies by version.

**Test Scenario**: Inject a failure after DROP TABLE but before data insertion. Verify rollback restores the original table.

**Mitigation**: Follow the nxs-008 pattern: backup file before migration, full transaction wrapping. The backup at `{path}.v8-backup` provides recovery if the transaction mechanism fails.

### R-03: Column-Field Name Drift

**Category**: Maintainability
**Severity**: Medium
**Likelihood**: Medium

**Risk**: SQL column names must exactly match Rust struct field names (C-06 in spec). If a future developer adds a field to `UniversalMetrics` without adding the corresponding SQL column (or vice versa), the mismatch causes silent data loss or runtime errors.

**Test Scenario**: Unit test that programmatically verifies the SQL CREATE TABLE columns match the `UniversalMetrics` struct fields.

**Mitigation**: Write a compile-time or test-time assertion that enumerates struct fields (via a constant array of field names) and compares against the SQL column list. This catches drift early.

### R-04: list_all_metrics Two-Query Merge Correctness

**Category**: Correctness
**Severity**: Medium
**Likelihood**: Low

**Risk**: The `list_all_metrics()` implementation uses two queries (one for universal metrics, one for all phase metrics) and merges them in a single pass. If the ORDER BY in the phase query doesn't match the ordering of the universal query, phase metrics could be attached to the wrong feature.

**Test Scenario**: Store 5 MetricVectors with interleaved phase data. Verify each returned MetricVector has exactly its own phases.

**Mitigation**: Both queries use `ORDER BY feature_cycle`. The merge loop matches on feature_cycle string equality. Test with multiple features having overlapping phase names.

### R-05: Bincode Config Mismatch in Migration Deserializer

**Category**: Data integrity
**Severity**: High
**Likelihood**: Low

**Risk**: The self-contained migration deserializer must use exactly `bincode::config::standard()` — the same config used by `serialize_metric_vector()` in the observe crate. If the config differs (e.g., using `legacy()` or `big_endian()`), deserialization produces garbage data without errors.

**Test Scenario**: Serialize a MetricVector with the current observe serializer, then deserialize with the migration deserializer. Verify field-by-field equality.

**Mitigation**: The migration test must roundtrip through the actual production serializer to verify config parity. The migration deserializer struct fields must be in exactly the same order as the original.

### R-06: re-export Breakage in Downstream Crates

**Category**: Build
**Severity**: Medium
**Likelihood**: Low

**Risk**: After moving types to unimatrix-store and adding re-exports in observe, any crate that imports `unimatrix_observe::serialize_metric_vector` or `unimatrix_observe::deserialize_metric_vector` will fail to compile. These functions are being removed (FR-09).

**Test Scenario**: `cargo build --workspace` must pass. Search for all call sites before removal.

**Mitigation**: Enumerate all callers of `serialize_metric_vector` and `deserialize_metric_vector` before removing. Currently: `unimatrix-server/src/mcp/tools.rs` (2 calls each). Update simultaneously.

---

## Risk Summary

| Risk | Severity | Likelihood | Test Priority |
|------|----------|-----------|---------------|
| R-01 | High | Medium | P1 — integration test |
| R-02 | High | Low | P1 — migration test |
| R-03 | Medium | Medium | P1 — structural test |
| R-04 | Medium | Low | P2 — integration test |
| R-05 | High | Low | P1 — migration test |
| R-06 | Medium | Low | P2 — build verification |

## Test Strategy

### Unit Tests (unimatrix-store)

1. **Roundtrip**: Store and retrieve MetricVector with all fields populated (AC-02)
2. **Replace semantics**: Store, replace with different phases, verify only new phases present (AC-03)
3. **Empty phases**: Roundtrip MetricVector with no phases (AC-12)
4. **List all with phases**: Multiple vectors, verify correct phase attachment (AC-04, R-04)
5. **Column-field alignment**: Assert SQL columns match struct field names (R-03)

### Migration Tests (unimatrix-store)

6. **v8→v9 happy path**: Pre-populate v8 blob, open, verify columnar data (AC-05)
7. **v8→v9 corrupted blob**: Pre-populate invalid blob, open, verify default MetricVector inserted (AC-06)
8. **Migration deserializer parity**: Serialize with observe, deserialize with migration compat, verify equality (R-05)
9. **Backup creation**: Verify `{path}.v8-backup` created during migration (NFR-01)

### Integration Tests (unimatrix-store)

10. **CASCADE delete**: Delete parent row, verify phase rows gone (AC-07)
11. **Transaction atomicity**: Verify multi-table write is atomic (NFR-02)

### Server Tests (unimatrix-server)

12. **Retrospective output unchanged**: Run context_retrospective with observation data, verify output matches pre-nxs-009 format (AC-08)
13. **Status count unchanged**: Verify retrospected feature count (AC-09)

### Build Verification

14. **No bincode helpers**: Verify `serialize_metric_vector` and `deserialize_metric_vector` not in observe public API (AC-10)
15. **Re-export works**: Verify `unimatrix_observe::MetricVector` compiles (AC-11)
16. **Workspace builds**: `cargo build --workspace` and `cargo test --workspace` pass (R-06)
