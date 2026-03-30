# Scope Risk Assessment: crt-034

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Weight normalization uses global MAX(count) across all qualifying pairs; in a large co_access table the extra query runs every tick, competing with write_pool_server() on the same SQLite writer | Med | Low | Architect should evaluate whether the MAX query can be combined with the batch fetch as a subquery to eliminate the second round-trip |
| SR-02 | INSERT OR IGNORE + conditional UPDATE is a two-step per-pair loop; SQLite write pool contention during a busy tick can cause individual statements to time out silently (infallible contract absorbs the error, leaving an edge un-promoted until the next tick) | Med | Med | Architect should consider a single UPSERT or CTE that handles insert-or-update atomically, reducing write pool hold time per pair |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | The scope defers GC of sub-threshold CoAccess edges to #409, but the promotion tick will continuously re-evaluate and potentially re-promote pairs whose count oscillates around the threshold. The delta guard (0.1) suppresses weight churn but not promotion churn for near-threshold pairs | Low | Med | Spec writer should add an AC covering near-threshold pairs: confirm promotion is idempotent and that no-op INSERT + weight-within-delta produces zero DB writes |
| SR-04 | One-directional edge matching bootstrap is called out as a known limitation (SCOPE.md §Known Limitation). PPR seeds from min-id → max-id only. If the architect intends to fix directionality in a follow-up, the current write structure (source = entry_id_a, target = entry_id_b) must be documented as a v1 contract so the follow-up can safely write the reverse edge without collision | Low | Low | Architect should record the directionality constraint as an ADR so the follow-up has a clear protocol for adding reverse edges without double-counting |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Hard sequencing dependency on GH #409: if #409 ships first and prunes co_access rows before crt-034 runs, qualifying pairs with count >= 3 that haven't been promoted are permanently lost. Signal loss is silent — no error, no warning (SCOPE.md §Constraints) | High | Med | Confirm issue ordering is tracked at the GH milestone level; architect should add a defensive log in the promotion tick if the qualifying-pair query returns 0 rows on first run (post-bootstrap) to make signal loss detectable |
| SR-06 | Promotion tick position (after orphaned-edge compaction, before TypedGraphState::rebuild) is the established pattern per entry #3821. Any future tick step added between these points by a concurrent feature branch (e.g. col-023, crt-029 follow-ups) could push the promotion after rebuild, silently deferring freshly promoted edges by one tick cycle | Low | Low | Architect should document the insertion point as a named constant or comment anchor in background.rs so the position is not inadvertently displaced |

## Assumptions

- **SCOPE.md §Non-Goals**: Assumes no new schema migration is needed. Valid only if GRAPH_EDGES table schema is unchanged since v13 and the `source = 'co_access'` / `relation_type = 'CoAccess'` values do not conflict with any pending schema work (e.g. col-023 GRAPH_EDGES extensions).
- **SCOPE.md §Background Research — Weight Normalization**: Assumes the co_access table is small enough (~0.34 MB cited) that a full-table MAX(count) query is cheap each tick. If the table grows significantly (high-volume deployment), this assumption degrades.
- **SCOPE.md §Tracking**: Assumes GH #409 has not already shipped or been merged. The sequencing risk (SR-05) is only valid if this is confirmed before crt-034 implementation begins.

## Design Recommendations

- **SR-05 (High)**: Before the architect commits to an unconditional tick, add a first-run detectability mechanism — log at `warn!` if qualifying pairs = 0 on the first N ticks after schema v13 promotion. This surfaces silent signal loss from a race with #409.
- **SR-02 (Med)**: Evaluate collapsing the per-pair INSERT + conditional UPDATE into a single SQL statement (e.g. `INSERT OR REPLACE` with weight recompute, or a CTE with an explicit conflict check). The current two-step design is correct but doubles write pool round-trips per pair.
- **SR-01 (Med)**: Combine MAX(count) into the batch query as a subquery (`SELECT ..., (SELECT MAX(count) FROM co_access WHERE count >= ?) AS max_count FROM co_access WHERE count >= ? ORDER BY count DESC LIMIT ?`) to avoid a separate read-pool query racing ahead of the write sequence.
