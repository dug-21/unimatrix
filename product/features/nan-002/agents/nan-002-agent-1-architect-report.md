# Agent Report: nan-002-agent-1-architect

## Task
Design architecture for nan-002 (Knowledge Import) -- CLI subcommand to restore a Unimatrix knowledge base from a nan-001 export dump.

## Artifacts Produced

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/nan-002/architecture/ARCHITECTURE.md`

### ADR Files
1. `/workspaces/unimatrix/product/features/nan-002/architecture/ADR-001-shared-format-types.md` (Unimatrix #1143)
2. `/workspaces/unimatrix/product/features/nan-002/architecture/ADR-002-direct-sql-insert.md` (Unimatrix #1144)
3. `/workspaces/unimatrix/product/features/nan-002/architecture/ADR-003-force-flag-safety.md` (Unimatrix #1145)
4. `/workspaces/unimatrix/product/features/nan-002/architecture/ADR-004-embedding-after-commit.md` (Unimatrix #1146)

## Key Design Decisions

1. **Shared format types** (ADR-001): New `format.rs` module with typed deserialization structs for the JSONL format_version 1 contract. Both export and import reference these types, catching format drift at compile time. Addresses SR-08.

2. **Direct SQL INSERT** (ADR-002): Import bypasses Store API to preserve original IDs, timestamps, confidence, and hashes. Uses `Store::open()` for DDL, then `store.lock_conn()` for raw SQL within a single IMMEDIATE transaction. Prior art: Unimatrix #336, #344.

3. **--force safety** (ADR-003): Stderr warning with entry count, no interactive prompt. Scriptable for CI/CD. Addresses SR-04.

4. **Embedding after commit** (ADR-004): Database transaction commits before re-embedding. Bounds transaction duration. ONNX failure does not lose imported data. Addresses SR-01.

## Scope Risks Addressed

| Risk | Resolution |
|------|-----------|
| SR-08 (implicit format contract) | ADR-001: shared format types with compile-time coupling |
| SR-01 (ONNX model dependency) | ADR-004: embedding after DB commit; failure preserves data |
| SR-04 (destructive --force) | ADR-003: stderr warning with entry count, no interactive prompt |

## Unimatrix Knowledge Consumed

- #1098: ADR-002 nan-001 (Explicit Column-to-JSON Mapping) -- informed format type design
- #336: ADR-004 nxs-006 (Import Uses Store::open() Then Raw SQL) -- prior art for ADR-002
- #344: Store::open() + Raw SQL Hybrid pattern -- prior art for ADR-002
- #1102: Sync CLI Subcommand Pattern -- structural template for import module
- #1104: Procedure for adding sync CLI subcommand -- registration steps
- #1097: ADR-001 nan-001 (Snapshot Isolation) -- transaction pattern reference

## Open Questions

1. **Schema version constant**: `CURRENT_SCHEMA_VERSION` is `pub(crate)` in unimatrix-store. Import should read schema_version from the counters table after `Store::open()` (same as export) rather than requiring visibility changes.

2. **Feature_entries/outcome_index DDL**: Implementation agents should verify exact column names from `schema.rs` before writing INSERT statements for these tables.

## Knowledge Stewardship

Stored:
- #1143: ADR-001 nan-002 (Shared Format Types)
- #1144: ADR-002 nan-002 (Direct SQL Insert)
- #1145: ADR-003 nan-002 (Force Flag Safety)
- #1146: ADR-004 nan-002 (Embedding After Commit)

Queried:
- #1098: ADR-002 nan-001 (Explicit Column-to-JSON Mapping)
- #336: ADR-004 nxs-006 (Import Uses Store::open() Then Raw SQL)
- #344: Store::open() + Raw SQL Hybrid pattern
- #1102: Sync CLI Subcommand Pattern
- #1104: Procedure for adding sync CLI subcommand
- #1097: ADR-001 nan-001 (Snapshot Isolation)

## Status
Complete.
