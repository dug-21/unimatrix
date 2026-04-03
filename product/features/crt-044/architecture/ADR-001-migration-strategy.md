## ADR-001: v19→v20 Migration — Two-Statement Source-Scoped Back-fill

### Context

S1, S2, and S8 edges exist in GRAPH_EDGES as single-direction rows (lower_id → higher_id).
Three back-fill strategies were considered:

1. **One combined statement** filtering `relation_type IN ('Informs', 'CoAccess')` and `source IN ('S1', 'S2', 'S8')`. Rejected: impossible to correctly combine because S1/S2 are `relation_type='Informs'` and S8 is `relation_type='CoAccess'`; a combined WHERE would require an OR that crosses relation_type boundaries and risks future source values being silently included.

2. **Three separate statements** (one per source). Considered: maximally explicit, but S1 and S2 share identical back-fill logic and the same `relation_type='Informs'` — separating them adds no correctness benefit and duplicates SQL.

3. **Two statements** — one for `source IN ('S1', 'S2')` scoped to `relation_type='Informs'`, one for `source = 'S8'` scoped to `relation_type='CoAccess'`. Selected.

Entry #3889 (crt-035 lesson): filter by `source`, NOT by `created_by`. `created_by` alone misses tick-era edges. `source` is the correct discriminator and is the convention established by crt-035.

Entry #4078: S8 was not covered by the v18→v19 crt-035 migration because that migration filtered `source = 'co_access'` only. Each new edge source requires its own migration block.

The `NOT EXISTS` sub-query and `INSERT OR IGNORE` both provide idempotency — `NOT EXISTS` as defence-in-depth matching the v18→v19 template; `INSERT OR IGNORE` as the primary correctness safety net via `UNIQUE(source_id, target_id, relation_type)`.

### Decision

Add a single `if current_version < 20` block to `run_main_migrations` in `migration.rs` containing two SQL statements:

Statement A: `INSERT OR IGNORE INTO graph_edges ... SELECT (swap source_id/target_id) FROM graph_edges g WHERE g.relation_type = 'Informs' AND g.source IN ('S1', 'S2') AND NOT EXISTS (reverse)`

Statement B: `INSERT OR IGNORE INTO graph_edges ... SELECT (swap source_id/target_id) FROM graph_edges g WHERE g.relation_type = 'CoAccess' AND g.source = 'S8' AND NOT EXISTS (reverse)`

Both statements copy `g.source` into the inserted row so reverse edges carry the same source discriminator as their forward partner.

`CURRENT_SCHEMA_VERSION` bumped from 19 to 20. Schema version bump within the block (`UPDATE counters SET value = 20`) plus the final `INSERT OR REPLACE INTO counters ... CURRENT_SCHEMA_VERSION` at end of `run_main_migrations`.

### Consequences

- Existing single-direction S1/S2/S8 edges gain reverse partners in one migration run.
- `nli` and `cosine_supports` Informs edges are excluded by the `source IN ('S1', 'S2')` filter — no explicit exclusion clause needed.
- `co_access` CoAccess edges are excluded by `source = 'S8'` — already bidirectional from v18→v19.
- Migration is idempotent: running twice produces no duplicates and no errors.
- Future edge sources writing single-direction edges will need their own migration block — this pattern does not automatically cover new sources (see entry #4078 lesson).
- The two-statement structure is a readable template for future similar back-fills.
