## ADR-001: AUTOINCREMENT for query_log Primary Key

### Context

All existing entity tables (entries, signal_queue, injection_log, audit_log) use named counters in the `counters` table for ID allocation. The pattern is: read counter, increment, write N rows with allocated IDs, update counter -- all in one transaction.

`query_log` is a new append-only telemetry table. It captures search query text and result metadata for downstream analysis (crt-019, col-021). Rows are never updated or deleted in normal operation. There is no need for predictable, contiguous IDs or cross-table ID coordination.

SQLite's AUTOINCREMENT guarantees monotonically increasing IDs without explicit counter management. The `observations` table (col-012) already uses this pattern successfully (Unimatrix #382).

Two ID allocation patterns now exist in the schema (SR-03). The decision boundary must be documented.

### Decision

Use `INTEGER PRIMARY KEY AUTOINCREMENT` for `query_log.query_id`. Do not add a `next_query_id` counter to the counters table.

**Decision boundary for future tables:**
- **Append-only log/telemetry tables** (observations, query_log): use AUTOINCREMENT. These tables have high write frequency, never need cross-table ID coordination, and benefit from the simpler write path (no counter read/update).
- **Entity tables** (entries, signals, injection_log, audit_log): use named counters. These tables have lower write frequency, may need batch allocation with contiguous ranges, and participate in cross-table relationships where predictable IDs matter.

### Consequences

- **Simpler write path**: `INSERT INTO query_log (...) VALUES (...)` without counter management. The `query_id` field in `QueryLogRecord` is ignored on insert and populated on read.
- **Non-contiguous IDs after VACUUM**: SQLite AUTOINCREMENT guarantees monotonic but not contiguous. Gaps after VACUUM are acceptable for telemetry data. col-021 export should order by `ts`, not `query_id`.
- **Two patterns to understand**: Future contributors must know the decision boundary above. This ADR and Unimatrix #382 document it.
- **No counter table bloat**: Avoids a 6th named counter for a table that does not need coordinated allocation.
