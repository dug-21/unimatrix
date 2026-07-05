# Test Plan — deprecate-handler (`tools.rs:1413`, step 6.5)

**Unit under test:** `context_deprecate` orchestration — after step-5 idempotency return and step-6 flip, invoke `delete_agent_edges_for_entry`, thread `Some(count)`/`None`, fire the `edge_cleanup` audit. Order: flip → delete → count → audit → format (steps 6 → 6.5 → 7 → 8).

**Placement:** extend `background.rs` `mod tests` (`:1911`) for the AC-10 subset test (it owns `insert_graph_edge_with_source`, `deprecate_entry_with_successor`, `run_orphaned_edge_compaction`, `total_graph_edges`, `count_graph_edges`). Handler orchestration tests use the in-process server fixture (`make_server()` pattern, `server.rs` tests). Audit read-back via `SELECT ... metadata FROM audit_log WHERE operation = 'context_deprecate.edge_cleanup'`.

## Test Expectations

### AC-10 — eager ⊆ tick subset invariant (BEHAVIORAL, both real functions) — **Critical / R-01, R-02**
- `test_deprecate_eager_subset_of_tick_and_exactly_agent_edges`
  - Arrange: TWO parallel fixtures A and B seeded from ONE shared helper — entry E with one edge per (direction × source): inbound/outbound × {`agent`, `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`} (14 edges). Assert pre-deprecation edge sets of A and B are IDENTICAL before divergence (R-02 fixture-identity).
  - Act: fixture A — bare `context_deprecate(E)` then the REAL `delete_agent_edges_for_entry` → capture removed set `R`. Fixture B — bare-deprecate E, then REAL `run_orphaned_edge_compaction(&store)` → capture removed set `T` (diff of `graph_edges` before/after).
  - Assert: `R ⊆ T` AND `R` == exactly the TWO `agent` edges (inbound + outbound). Widening the eager SQL to a machine source breaks the exact-set assertion; narrowing the tick so it keeps agent edges breaks `R ⊆ T`.
- `test_eager_predicate_string_pinned` (R-02)
  - Snapshot the LOCKED predicate text (`(source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING source_id, target_id, relation_type`); a casual edit to `WHERE`/`RETURNING` (e.g. a relation-type blocklist creeping in — F2 discipline) fails the pin.

### AC-10 chokepoint-exclusion — the missing half (**R-01 closure, R-06**)
- `test_correct_successor_bearing_entry_never_invokes_eager_helper`
  - Arrange: build a successor-bearing correction — drive `context_correct` (sets `superseded_by`), seed an INBOUND `source='agent'` edge that Phase 1 (`repoint_deprecated_target_edges`) would repoint.
  - Act: run the correction path (production handler).
  - Assert (against the REAL handler, not prose): NO `"context_deprecate.edge_cleanup"` audit event is written for the corrected entry, AND the inbound agent edge SURVIVES (Phase 1 would repoint it). Proves the successor-bearing entry never reaches the eager helper.
- `test_negative_mutation_helper_would_destroy_repointable_edge` (R-01 scenario 3, documentary)
  - In a unit harness, point the eager helper at a successor-bearing entry and assert it WOULD delete the repointable inbound edge — documents why the chokepoint (not the helper) is the guarantor. Ensures the author modeled the hazard.

### R-06 — single production caller (delivery-time closure)
- `test_delete_agent_edges_for_entry_has_single_production_caller`
  - Callgraph/grep assertion: `delete_agent_edges_for_entry` is called from exactly ONE non-test site — `context_deprecate` step 6.5. Any second caller must be raised as a design change, not merged silently.

### AC-01 / AC-09 — synchronous removal on return — **R-11**
- `test_deprecate_removes_agent_edges_synchronously_on_return`
  - Arrange: Active E with ≥1 inbound + ≥1 outbound agent edge.
  - Act: `context_deprecate(E)`; IMMEDIATELY (no sleep, no tick) query `graph_edges`.
  - Assert: agent edges already absent (both directions); `edges_removed = Some(2)`; entry `Deprecated`.

### AC-07 — idempotency / ordering — **R-11**
- `test_redeprecate_performs_no_delete_no_cleanup_audit`
  - Arrange: deprecate E (first call); seed a FRESH `source='agent'` edge touching E; deprecate E again.
  - Assert: second call returns via the step-5 early-return; the fresh edge is UNTOUCHED; NO new `"context_deprecate.edge_cleanup"` audit record for the second call.
- `test_flip_precedes_delete` — assert the delete matches because E is non-Active at 6.5 (if 6.5 ran before the flip on an Active id, the subset reasoning collapses — order is load-bearing).

### AC-06 — non-fatal on failure — **SR-05**
- `test_deprecate_eager_failure_returns_success_warn_none_advisory`
  - Arrange: fault-inject an eager-delete error (forced error path / poisoned pool).
  - Assert: `context_deprecate` returns SUCCESS; entry is `Deprecated`; a `warn` log carrying the entry id is emitted (NOT `debug` — #3448 / NFR-05); `edges_removed = None` → advisory OMITTED in all three formats (distinct from AC-05 `Some(0)`); agent edges STILL PRESENT.
  - Then: run `run_orphaned_edge_compaction`; assert the backstop removes them (AC-06 tail).

### AC-08 — no tick/schema change (grep)
- `test_no_new_migration_and_compaction_unchanged` — schema version + migration list unchanged; `run_orphaned_edge_compaction` (`background.rs:805`) body unchanged; helper uses `write_pool_server()` on `graph_edges`.

### Edge case — shared edge across two deprecations
- `test_two_entries_sharing_edge_deprecated_in_sequence` — first deprecation removes the shared agent edge (count attributes to it); second's RETURNING omits it (count 0 for that edge); state consistent.

## Notes for delivery
- The subset test is the feature's keystone (SR-02) and the re-check trigger if the tick ever changes (C-07) — if the tick predicate is later modified, RE-DERIVE the test, never delete it.
- Audit assertions MUST filter on `operation == "context_deprecate.edge_cleanup"` (R-08) — the flip emits a separate `"context_deprecate"` record.
