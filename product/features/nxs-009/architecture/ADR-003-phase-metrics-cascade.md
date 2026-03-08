## ADR-003: Phase Metrics Foreign Key with CASCADE

### Context

`observation_phase_metrics` references `observation_metrics.feature_cycle`. When a parent row is deleted (or replaced), orphaned phase rows must be cleaned up. Options:

1. **ON DELETE CASCADE** — automatic cleanup by SQLite engine
2. **Manual DELETE in application code** — explicit cleanup before/after parent operations
3. **No foreign key** — application-level integrity only

### Decision

Option 1: `FOREIGN KEY (feature_cycle) REFERENCES observation_metrics(feature_cycle) ON DELETE CASCADE`.

This matches the `entry_tags` pattern (ADR-006 in nxs-008). The database enforces referential integrity. The write path also uses explicit DELETE+INSERT for the replace case (since INSERT OR REPLACE on the parent row does not trigger CASCADE for replaced rows in SQLite — it performs DELETE+INSERT internally, which does trigger CASCADE).

Note: `PRAGMA foreign_keys = ON` is already set in `Store::open()` (ADR-003 from nxs-004).

### Consequences

- **Easier**: Referential integrity guaranteed. No orphaned phase rows. Consistent with entry_tags pattern.
- **Harder**: Must ensure `PRAGMA foreign_keys = ON` in all paths including migration. Already handled by Store::open().
