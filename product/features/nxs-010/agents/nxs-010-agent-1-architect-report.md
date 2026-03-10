# Agent Report: nxs-010-agent-1-architect

## Task
Design the architecture for nxs-010 (Activity Schema Evolution).

## Artifacts Produced

- `product/features/nxs-010/architecture/ARCHITECTURE.md`
- `product/features/nxs-010/architecture/ADR-001-autoincrement-for-query-log.md`
- `product/features/nxs-010/architecture/ADR-002-fire-and-forget-query-log-writes.md`
- `product/features/nxs-010/architecture/ADR-003-backfill-in-main-migration-transaction.md`

## Unimatrix Entries

- #818: ADR-010-001 (AUTOINCREMENT for query_log PK)
- #819: ADR-010-002 (Fire-and-forget query_log writes)
- #820: ADR-010-003 (Backfill in main migration transaction)

## Key Decisions

1. **AUTOINCREMENT for query_log** (ADR-001): Append-only log uses SQLite AUTOINCREMENT, not a named counter. Documents the decision boundary between the two ID allocation patterns.
2. **Fire-and-forget writes** (ADR-002): Both UDS and MCP paths write query_log via spawn_blocking with warn-level logging on failure. No latency impact. Resolves SR-05.
3. **Single-transaction backfill** (ADR-003): Additive migration (CREATE TABLE + INSERT) runs in the main migrate_if_needed transaction. No separate transaction or backup needed.

## Risk Mitigations

- **SR-01** (DDL ordering): Architecture documents that migrate_if_needed runs BEFORE create_tables in Store::open(). Both emit IF NOT EXISTS DDL. Integration test recommended: open a v10 DB with nxs-010 code.
- **SR-02** (version collision): nxs-010 targets v11, guard is `current_version < 11`. Must merge after col-017. Documented in architecture.
- **SR-03** (AUTOINCREMENT divergence): ADR-001 documents the decision boundary between AUTOINCREMENT (logs) and counters (entities).
- **SR-05** (failure semantics): ADR-002 specifies exact failure behavior for both paths.
- **SR-07** (field parity): Architecture specifies a shared QueryLogRecord construction pattern with exact field mapping for both UDS and MCP.

## Open Questions

1. UDS path: skip query_log write when session_id is None/empty (recommended, matches injection_log guard).
2. MCP path: use empty string for session_id when None (recommended, MCP queries always analytically valuable).

## Status
Complete.
