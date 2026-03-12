# Scope Risk Assessment: nan-002

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | ONNX model download required on first import (~80MB network fetch). Import fails in air-gapped or CI environments without cached model. | High | Med | Architect should ensure clear error messaging when model is unavailable and document the pre-cache requirement. Consider a `--skip-embedding` dry-run mode. |
| SR-02 | Direct SQL INSERT bypasses Store API invariants (auto-ID, auto-hash, auto-timestamps). Any future Store schema change (new column, new default, new constraint) must be mirrored in import code manually. Historical evidence: ADR-004 (#336) and pattern (#344) document this tradeoff. | Med | High | Architect should centralize column lists or use shared constants between Store and import to reduce drift risk. |
| SR-03 | Re-embedding 500+ entries synchronously blocks the CLI. ONNX inference is CPU-bound; on slow machines or large knowledge bases the 60-second AC-17 target may be tight. | Med | Low | Batch size tuning (64) is specified; architect should ensure progress reporting is granular enough to distinguish "slow" from "stuck". |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `--force` drops all data before import but scope excludes merge/append. Users who accidentally run `--force` on a production database lose data with no undo. The only safety net is a prior export. | High | Med | Architect should consider a confirmation prompt or require `--force --yes` double-opt-in for destructive operations. |
| SR-05 | Schema version must match exactly (no migration of import data). Users with exports from older binaries must re-export, but the older binary may no longer be available. This creates a narrow compatibility window. | Med | Med | Spec should clarify the upgrade path: document that users should export immediately before upgrading binaries. |
| SR-06 | Scope excludes stdin (`--input -`) and decompression, but the non-goal text suggests pipe workflows (`zcat | import --input -`). This contradiction may confuse users. | Low | Med | Spec should remove the pipe example from non-goals or note it requires stdin support (deferred). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Import opens database via `Store::open()` while a running MCP server holds a connection. SQLite WAL mode allows concurrent reads but write transactions will contend. Scope mentions "warn if server running" but the PID check (vnc-004) is Linux-only. | Med | Med | Architect should use the existing PidGuard/flock mechanism to detect or block concurrent access cross-platform. |
| SR-08 | Export format contract (format_version 1, 8 tables, 26 entry columns) is implicit -- defined by nan-001 code, not a versioned schema file. If nan-001 export changes (new column, new table), import breaks silently. | High | Med | Architect should consider a shared format definition (struct or constant) between export and import modules to enforce contract coupling. |
| SR-09 | Serde deserialization of 26-column entries with nullable fields, JSON-in-TEXT columns, and edge cases (unicode, max integers) is a high-surface-area parsing problem. Historical lesson (#885): serde-heavy types need explicit test coverage. | Med | High | Test plan should include explicit deserialization fuzz/edge-case coverage per AC-23. Architect should reuse export serialization types for import deserialization. |

## Assumptions

1. **Export format stability** (SCOPE lines 39-48): Assumes nan-001 format_version 1 will not change before nan-002 ships. If nan-001 is modified concurrently, the import contract breaks.
2. **Empty database target** (SCOPE lines 26, 146): Assumes users can always create a fresh database. In multi-repo deployments, clearing a database may require coordination with running services.
3. **ONNX model determinism** (SCOPE lines 150-153): Assumes re-embedding with the same model produces identical vectors. Model updates between export and import are expected, but if the model is silently updated (hf-hub cache invalidation), search quality changes are invisible.
4. **Dependency ordering** (SCOPE line 45): Assumes nan-001 always emits tables in dependency order. If export code is refactored, FK violations on import are the only safety net.

## Design Recommendations

1. **Shared format types** (SR-08, SR-09): Define export/import row structs in a shared location. Both export serialization and import deserialization should use the same types. This eliminates format drift.
2. **Pre-flight checks** (SR-01, SR-07): Before starting the import transaction, verify: (a) ONNX model is available or downloadable, (b) no server holds the database lock, (c) target database is empty or `--force` is specified. Fail fast before any writes.
3. **Destructive operation safety** (SR-04): Require explicit confirmation for `--force` on non-empty databases, or at minimum log a prominent warning with the entry count being dropped.
