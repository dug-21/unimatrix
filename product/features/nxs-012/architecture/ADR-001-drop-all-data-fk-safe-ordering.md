## ADR-001: FK-Safe DELETE Ordering in drop_all_data for New Tables

### Context

`drop_all_data` in `import/mod.rs` clears all importable tables during `--force` import. The current implementation deletes 8 tables in FK-safe order (entry_tags before entries due to CASCADE FK).

nxs-012 adds 3 new tables to the export/import pipeline: `graph_edges`, `observations`, `cycle_events`. Two additional derived tables -- `observation_metrics` and `observation_phase_metrics` -- are NOT part of the export contract (they are computed aggregates), but they must be cleared during `--force` import to avoid stale aggregates after raw observations are replaced.

The FK dependency chain is:
- `observation_phase_metrics` FK -> `observation_metrics` (ON DELETE CASCADE)
- `observation_metrics` has no FK to `observations` (they share `feature_cycle` semantically but not via FK constraint)

Risk SR-06 from the scope risk assessment flags this as the highest-priority risk: if derived metric tables are not cleared, stale aggregates from the previous dataset persist alongside fresh raw observations, producing incorrect phase affinity scores in `context_briefing`.

Option A: Delete `observation_phase_metrics` and `observation_metrics` explicitly before the 3 new tables. Cascade would handle `observation_phase_metrics` if we only delete `observation_metrics`, but explicit ordering is clearer and does not rely on PRAGMA foreign_keys being ON during the DELETE batch.

Option B: Rely on CASCADE from `observation_metrics` delete to clear `observation_phase_metrics`. Requires PRAGMA foreign_keys = ON, which is set in Store::open() (nxs-004 ADR-003) but not guaranteed during the import path's raw pool access.

Option C: Do not clear derived metric tables. Risk stale aggregates.

### Decision

Option A. Delete `observation_phase_metrics` first, then `observation_metrics`, then `graph_edges`, `observations`, `cycle_events`. All five new DELETEs are inserted between the existing `vector_map` DELETE and the `entries` DELETE.

Explicit ordering does not depend on PRAGMA foreign_keys state and makes the dependency clear in code. The full DELETE order becomes:

```
entry_tags, co_access, feature_entries, outcome_index, agent_registry, vector_map,
observation_phase_metrics, observation_metrics,
graph_edges, observations, cycle_events,
entries, counters
```

### Consequences

- Stale derived aggregates are cleared even though they are not exported (SR-06 resolved)
- The metrics pipeline recomputes aggregates from the imported raw observations on next session activity
- Explicit ordering is self-documenting and safe regardless of PRAGMA foreign_keys state
- If new derived tables are added in the future, they must be added to `drop_all_data` even if they are not part of the export contract
