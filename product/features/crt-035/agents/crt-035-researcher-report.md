# crt-035 Researcher Report

## Task

Research the problem space for bidirectional CoAccess edges + bootstrap-era back-fill (crt-035).
Explore codebase, ADR-006, migration framework, promotion tick implementation, test coverage,
and produce SCOPE.md.

## SCOPE.md

Written to: `product/features/crt-035/SCOPE.md`

## Key Findings

### ADR-006 (Unimatrix #3830) — explicit follow-up contract

ADR-006 documents three explicit requirements for crt-035:
1. Write `(entry_id_b, entry_id_a, 'CoAccess')` reverse edge — distinct under
   `UNIQUE(source_id, target_id, relation_type)`, no collision risk.
2. Back-fill ALL bootstrap-era pairs where `source = 'co_access'` in `GRAPH_EDGES`.
3. Reference ADR-006 to confirm forward-edge layout was intentional.
ADR-006 explicitly confirms cycle detection is unaffected (CoAccess edges excluded from
Supersedes-only cycle subgraph, Pattern #2429).

### Back-fill discriminator: `source = 'co_access'`, not `created_by = 'bootstrap'`

ADR-006 says "identifiable by `source = 'co_access'` AND `created_by = 'bootstrap'`" but
the correct discriminator for the back-fill SQL is `source = 'co_access'` alone. Reason:
crt-034 tick-era forward-only edges use `created_by = 'tick'`, not `'bootstrap'`. Filtering
by `created_by = 'bootstrap'` would leave all tick-era forward-only edges un-back-filled.
Filtering by `source = 'co_access'` covers both bootstrap-era and tick-era in one pass.
This is the key correctness finding for the back-fill SQL design. Stored as Pattern #3889.

### Promotion tick: one-direction INSERT is the only structural change needed

The tick at `crates/unimatrix-server/src/services/co_access_promotion_tick.rs` writes
`(entry_id_a, entry_id_b)` once per pair (line 172–183). The change is to write
`(entry_id_b, entry_id_a)` in the same pass with the same `new_weight`. The weight update
path (Steps B/C) must be duplicated for the reverse direction. A helper function is needed
to keep the file under 500 lines.

### Test impact: one explicit test must be inverted

`test_inserted_edge_is_one_directional` (Group A, `co_access_promotion_tick_tests.rs`
line 151) asserts `reverse edge must not be created` — this test must be replaced with a
bidirectional assertion. `test_double_tick_idempotent` and `test_basic_promotion_new_qualifying_pair`
also need count updates (1 edge → 2 edges per pair).

### Migration framework: v18→v19, pure data back-fill

Current schema version: 18 (set by crt-033). The migration is a pure INSERT OR IGNORE
data back-fill — no new DDL, no table creation, no ALTER TABLE. The `run_main_migrations`
function uses `if current_version < N` guards. The final `INSERT OR REPLACE INTO counters`
unconditionally bumps to `CURRENT_SCHEMA_VERSION`. Pattern from v17→v18 is the template.
Integration test file `tests/migration_v18_to_v19.rs` must be created following the
established pattern.

### GRAPH_EDGES UNIQUE constraint is idempotency

`UNIQUE(source_id, target_id, relation_type)` treats `(a, b, type)` and `(b, a, type)`
as distinct rows. `INSERT OR IGNORE` in both the tick and the migration handles all
idempotency cases without additional code.

### No PPR traversal or TypedGraphState code changes needed

PPR already traverses `Direction::Outgoing`. Adding `(b→a)` edges means seeding `b` now
reaches `a` via Outgoing — correct semantic. No changes to PPR, TypedRelationGraph, or
downstream search scoring.

## Open Questions for Human

1. **Back-fill `created_by` field**: copy from forward edge (preserves `'bootstrap'` vs
   `'tick'` provenance) or use a new value `'back-fill'`? SCOPE proposes copying; confirm.

2. **Tick log semantics**: report `inserted`/`updated` as edge writes (up to 2× pairs) or
   as pair promotions? SCOPE proposes edge writes; confirm.

3. **PPR behavioral regression test (AC-12)**: extend existing TypedGraphState tests or
   write a new graph traversal test? Needs investigation of `typed_graph*.rs` test coverage
   before design phase.

4. **Weight symmetry on diverged edges**: if forward and reverse edges have different
   weights (e.g., from a partial previous tick), both should be updated to `new_weight`
   independently. SCOPE confirms this is the correct semantic — verify during design.

5. **`db.rs` `create_tables_if_needed` fresh-DB path**: no changes expected (back-fill is
   data-only), but verify during design before closing.

## Risks Identified

- **crt-034 tests assert one-directional behavior**: at least `test_inserted_edge_is_one_directional`
  and `test_double_tick_idempotent` must be updated before crt-035 tests can pass. This is
  expected breakage, not a regression.
- **File size**: `co_access_promotion_tick.rs` is currently ~289 lines. Doubling the
  per-direction logic inline would approach the 500-line limit. A helper function is
  required.
- **ADR-006 Unimatrix entry (#3830) must be updated** as part of crt-035 to confirm the
  follow-up contract is fulfilled (AC-14). This is a knowledge stewardship task, not just
  a code task.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 12 entries; entries #3830, #3827,
  #3882, #3883, #3754 were directly relevant and read in full.
- Stored: entry #3889 "Back-filling reverse GRAPH_EDGES for symmetric relations: filter by
  source, not created_by" via `/uni-store-pattern` — generalizes beyond crt-035 to any
  future symmetric relation type back-fill migration.
