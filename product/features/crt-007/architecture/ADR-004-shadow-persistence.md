## ADR-004: Shadow Mode Persistence in SQLite

### Context

Shadow mode logs need to persist across sessions for accuracy tracking and promotion decisions. The evaluation data (neural predictions vs rule outcomes) must be queryable for aggregation and reporting.

Options:
- **a) Flat JSONL file**: Simple but not queryable without parsing. col-012 already eliminated JSONL in favor of SQLite.
- **b) bincode binary file**: Fast serialization but opaque, not JOINable with other Unimatrix tables.
- **c) SQLite table**: JOINable with observations, sessions, and signals. Queryable with standard SQL. Consistent with col-012 decision.

### Decision

Shadow evaluation logs go into a `shadow_evaluations` SQLite table in the existing Unimatrix database. Schema:

```sql
CREATE TABLE IF NOT EXISTS shadow_evaluations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    rule_name TEXT NOT NULL,
    rule_category TEXT NOT NULL,
    neural_category TEXT NOT NULL,
    neural_confidence REAL NOT NULL,
    convention_score REAL NOT NULL,
    rule_accepted INTEGER NOT NULL,
    digest BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_shadow_eval_ts ON shadow_evaluations(timestamp);
```

The table is created during schema initialization (schema version bump). Digest is stored as raw bytes (32 * 4 = 128 bytes per row) for potential re-evaluation with updated models.

### Consequences

- **Easier**: Standard SQL aggregation for accuracy metrics. JOINable with observations table for richer analysis. Consistent with platform direction (col-012).
- **Harder**: Adds write pressure to SQLite database during extraction ticks. Schema version bump required.
- **Mitigated**: Shadow logs are written in batch after the extraction tick completes. Expected volume is low (max 10 proposals per hour from rate limit).
