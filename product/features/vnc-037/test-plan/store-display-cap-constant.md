# Test Plan — store-display-cap-constant (`GET_EDGE_DISPLAY_LIMIT`)

`pub const GET_EDGE_DISPLAY_LIMIT: i64 = 3;` in `read.rs`, re-exported via `lib.rs`. The single
source of truth for the display cap — bound by the SQL `LIMIT`, the `…N more` render threshold,
and **referenced by tests**. Owns **AC-13 / FR-18 / C-12 / R-? (maintainability)**. The cap is
**decoupled** from the uncapped totals and from canonicalization.

## Unit / File-Check Expectations

### AC-13a — Single source, no magic literal `3`

**`grep_no_literal_3_at_cap_sites`** (grep/file-check)
Assert **no literal `3`** appears at:
- the SQL `LIMIT` site in `graph_queries_ranked.rs` (must be `LIMIT ?` bound to the constant),
- the `…N more` threshold/arithmetic in `response/edges.rs` (must be `total >
  GET_EDGE_DISPLAY_LIMIT` and `N = total − GET_EDGE_DISPLAY_LIMIT`).
Grep the cap-application sites; assert each references `GET_EDGE_DISPLAY_LIMIT`, not `3`.

**`const_is_reexported_from_lib`** (file-check)
Assert `GET_EDGE_DISPLAY_LIMIT` is defined once in `read.rs` (below `CO_ACCESS_GRAPH_MIN_COUNT`)
and re-exported in the `pub use read::{…}` block of `lib.rs`.

**`tests_reference_the_constant`** (discipline, per ADR-006 #5054 / #3886)
The ranking/cap tests in store-ranked-query and serializer-seam seed `GET_EDGE_DISPLAY_LIMIT + N`
edges and assert result length `== GET_EDGE_DISPLAY_LIMIT` — **never** a literal 3 or 5. Verified
by inspection of those tests (so changing the constant cannot break the suite spuriously).

### AC-13b — Cap-isolation (override changes ONLY rendered count)

**`test_cap_override_shrinks_only_rendered_set`** (the load-bearing isolation test)
Override / parametrize the cap to **2** (e.g. via a test seam or a parametrized query helper).
Seed a fixed edge set of, say, 5 edges including a symmetric pair. Assert:
- the **rendered set shrinks to 2** edges,
- the inbound/outbound **totals are byte-unchanged** (still report the uncapped 4-after-canon
  count),
- the `↔`-once **canonicalization is byte-unchanged** (the symmetric pair still collapses once).
Restore the constant. Confirms a one-line value edit retunes **only** the rendered count — never
totals (FR-10), never canon (FR-8).

> If overriding a `const` is impractical in-test, the implementer threads the cap as a parameter
> bound to the constant at the call site; the isolation test exercises that parameter. The plan
> requires the **behavior** (override → only rendered set changes), not a specific seam.

## Cross-Component Dependency
- This constant is consumed by `store-ranked-query` (SQL `LIMIT ?`) and `serializer-seam`
  (`…N more`). The cap-isolation test spans all three: override here, observe rendered-set
  shrink in serializer, observe totals/canon unchanged in store-split-count.

## Edge Cases
- cap override to a value **larger** than the available edge count → render shows all (no
  pointer); totals unchanged.
- cap value `3` (default) is exercised implicitly by every other cap test.
