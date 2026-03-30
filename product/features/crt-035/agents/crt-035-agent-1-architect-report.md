# crt-035 Architect Agent Report

**Agent:** crt-035-agent-1-architect

## Outputs

### ARCHITECTURE.md
`product/features/crt-035/architecture/ARCHITECTURE.md`

### ADR Files
`product/features/crt-035/architecture/ADR-001-bidirectional-tick-eventual-consistency.md`

### Unimatrix IDs
- ADR-001 stored as entry **#3890**
- ADR-006 (#3830) correction: `context_correct` failed — anonymous agent lacks Write
  capability. The Design Leader must run this correction directly or via a credentialed
  agent. Correction content is prepared in this report.

## ADR-006 Correction (for Design Leader to apply)

Call `context_correct` with `original_id: 3830`:

- **reason:** "crt-035 fulfills the ADR-006 follow-up contract: bidirectional writes are
  now the default in run_co_access_promotion_tick, back-fill is scoped to
  source='co_access' in GRAPH_EDGES (covers both bootstrap and tick-era edges), and cycle
  detection is confirmed unaffected."

Content is the updated ADR-006 text recorded in ARCHITECTURE.md §Integration Points and
in the context_correct call attempt above.

## Key Decisions

1. **Atomicity (SR-01):** Eventually consistent, not atomic per pair. `promote_one_direction`
   helper called twice per pair; failures log `warn!` and continue. Both directions use the
   same `new_weight`; divergence corrects on the next tick.

2. **Helper refactor:** Module-private `promote_one_direction(store, source, target, weight)
   -> (bool, bool)` extracts the three-step INSERT/fetch/UPDATE per direction. Required to
   keep the file under the 500-line limit and avoid duplicate inline code.

3. **Migration (v18→v19):** Pure data migration inside the main transaction. No DDL.
   `INSERT OR IGNORE` + `NOT EXISTS` guard (D4). `UNIQUE(source_id, target_id, relation_type)`
   B-tree index covers the NOT EXISTS self-join — no additional index needed (SR-04 resolved).

4. **`db.rs` fresh path:** No changes needed. Fresh DB has zero CoAccess rows; back-fill
   is a no-op (SCOPE OQ-2 resolved).

5. **AC-12 test placement (SR-06):** Add `test_reverse_coaccess_high_id_to_low_id_ppr_regression`
   in `graph_ppr_tests.rs` using `make_graph_with_edges` with a `(b→a)` CoAccess edge, seeded
   at `b`. Uses the real in-memory `TypedRelationGraph` + `personalized_pagerank` path — not a
   mock. The existing `test_cycle_detection_on_supersedes_subgraph_only` already covers AC-13.

6. **Log format (D2):** `promoted_pairs: N, edges_inserted: M, edges_updated: K`

## SR-05 Test Blast Radius

Complete enumeration in ARCHITECTURE.md §SR-05. Ten tests require updates. Three new Group I
tests required. Full list in ARCHITECTURE.md.

## Open Questions

None. Both SCOPE.md open questions are resolved.
