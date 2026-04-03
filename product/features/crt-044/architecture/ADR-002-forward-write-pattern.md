## ADR-002: Forward-Write Bidirectionality — Two write_graph_edge Calls Per Pair

### Context

The three tick functions (run_s1_tick, run_s2_tick, run_s8_tick) currently write one edge per pair. Two approaches to making them bidirectional were considered:

1. **Modify the SQL query** to return each pair twice (once per direction) by removing the `t2.entry_id > t1.entry_id` / `e2.id > e1.id` constraint. Rejected: the constraint also deduplicates pairs at the query level — removing it would double the candidate set without the dedup, requiring more complex SQL or application-level dedup logic. For S8, the pair construction from audit rows (`min/max`) would also need restructuring.

2. **Two write_graph_edge calls per pair** with swapped source_id/target_id. Selected. This is the identical pattern used by `co_access_promotion_tick.rs` (`promote_one_direction(a, b)` then `promote_one_direction(b, a)`). The existing SQL query shapes are unchanged.

The `write_graph_edge` function is already idempotent (`INSERT OR IGNORE`). After the v19→v20 migration, the reverse edge already exists for most pairs, so the second call returns `false` (UNIQUE conflict) — this is correct behavior per entry #4041.

Entry #4041 establishes the `write_graph_edge` three-case return contract:
- `true` (rows_affected = 1): new row inserted — increment budget counter.
- `false` (rows_affected = 0, Ok): UNIQUE conflict — expected and silent. Do NOT warn.
- `false` (Err): SQL error — warned inside write_graph_edge. Do NOT double-log.

SR-02 risk: the second direction call frequently returns `false` after migration. This is NOT a bug. Budget counters (edges_written, pairs_written) must be incremented only on `true` return from each call independently.

SR-01 risk: run_s8_tick's `pairs_written` counter now counts per-edge, not per logical pair. A new pair `(a, b)` where neither direction exists causes `pairs_written += 2`. This is a documented semantic change from crt-041. The log field name `pairs_written` is retained for backward compatibility but its semantics shift to match `edges_written` in run_s1_tick and run_s2_tick.

### Decision

In each of the three tick functions, add a second `write_graph_edge` call immediately after the existing call, with `source_id` and `target_id` swapped. All other arguments (relation_type, weight, now_ts, EDGE_SOURCE_*, metadata) are identical to the first call.

The budget counter is incremented independently for each call's return value. No special handling for `false` returns.

For run_s8_tick specifically: the `valid_ids` set already contains both `*a` and `*b` (validated in Phase 4). The quarantine filter applies symmetrically — no additional guard needed before the second call.

The `pairs_written` counter in run_s8_tick log output reflects actual DB writes (both directions). Downstream monitoring or alerting that keys off this counter should expect 2× values for new pairs relative to crt-041 behavior.

### Consequences

- All three tick functions are symmetric at the write site going forward.
- `edges_written` (S1, S2) and `pairs_written` (S8) count actual DB inserts. For steady-state post-migration DBs, the second call mostly returns false (no counter increment) — minimal overhead.
- `write_graph_edge` is called twice per pair in each tick loop iteration. Performance impact is negligible: `INSERT OR IGNORE` on an existing UNIQUE row is a single B-tree lookup.
- The pattern is consistent with `co_access_promotion_tick.rs`, making the codebase uniform across all bidirectional edge sources.
- SR-06 (tick regression risk) is mitigated by the per-source integration tests specified in ARCHITECTURE.md §Test Requirements.
