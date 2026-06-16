# Test Plan — store-split-count (`count_neighbors_split`)

A new `COUNT(*)` over the **canonicalized** neighbor set, split inbound/outbound, **uncapped**,
returning `EdgeCountSplit { inbound, outbound }`. Owns **Critical R-03** and the totals half of
**R-01**. The defining hazard: this query and the ranked query are **two queries that must agree**
on canonicalization (RISK-TEST-STRATEGY Integration Risks) — a divergence double-counts on one
surface only. Store unit tests against seeded `graph_edges`.

> **Invariant under test:** "symmetric counted once" holds on **totals independently of the
> displayed set**. A fix that dedups the rendered list but misses count-dedup MUST fail here.

## Unit Test Expectations

### R-03 — Split COUNT divergence / canon mismatch (Critical)

**`test_count_uncapped_exact`** (FR-10/SR-14)
Seed **8** mixed-direction edges on anchor A (> cap). Assert `inbound + outbound == 8` (the true
**uncapped** total) — the count is NOT computed off the capped ≤3 Vec. Pair with a rank-query
assertion (in store-ranked-query) that the displayed set is ≤3, so the cap/totals decoupling is
proven from both sides.

**`test_count_direction_split_load_bearing`** (#744 observability)
Seed an entry with high inbound + **zero** outbound (e.g. 5 incoming `Prerequisite`, 0 outgoing).
Assert `inbound == 5, outbound == 0`. Proves inbound-degree observability survives — the totals
are genuinely split, not a single scalar.

**`test_count_symmetric_counted_once`** (R-01 totals side — SEPARATE assertion)
For each of `Contradicts`, `CoAccess`, `Informs`: seed the pair as **both reciprocal rows**.
Assert the relationship contributes **once** to the split (total contribution == 1, attributed to
the canonical row's direction bucket per ADR-007 #4), **not** once inbound + once outbound.
Assert this **directly on the `EdgeCountSplit` output**, independent of the ranked query.

**`test_count_canon_parity_with_rank_query`** (the two-queries-must-agree guard)
Seed a mixed set including a symmetric pair. Assert the count query's canonicalization yields the
**same** symmetric-once result the ranked query produces (cross-check: the symmetric pair occupies
**one** slot in the ranked output AND contributes **one** to the count). A divergence (count
dedups differently than rank) fails this test.

**`test_count_before_canon_would_double`** (order-of-ops negative framing)
Document/assert that counting happens **post-canonicalization**: a symmetric pair seeded as two
rows yields a total of 1 for that relationship, not 2 — i.e. canon precedes `COUNT(*)`.

### R-04 — COUNT in SQL, scalar return (Critical)

**`test_count_returns_two_scalars_not_materialized`** (SR-14)
On the ≥50-edge high-degree fixture, assert `count_neighbors_split` returns **two scalars** and
**never materializes** the neighbor rows (the function "never materializes rows" per signature —
it is a `COUNT(*)` aggregate). Assert via store boundary: no `Vec<RawEdgeRow>` allocation path.

### R-03 — Nested shape (carried to render)

**`test_count_nested_shape_inbound_outbound`** (OQ-01/ADR-005)
Assert the returned struct exposes `inbound` and `outbound` as the `{inbound, outbound}` object
the JSON `edge_totals` renders (not a flat or capped scalar). (Render shape itself is in
serializer-seam; this asserts the store value feeding it.)

### R-12 — Supersedes excluded from totals (Medium)

**`test_count_supersedes_not_counted`**
Seed a `Supersedes` edge among typed edges; assert it is **absent from both** `inbound` and
`outbound` counts (filter inheritance survives the count-query rewrite).

## Integration Expectations (through MCP)
- `test_get_edge_totals_symmetric_once` — `edge_totals` in the JSON response counts a symmetric
  pair once (separate from the displayed-set assertion #4).
- `test_get_capped_pointer_when_more_than_cap` — on a >cap entry, totals report the true uncapped
  count and the `…N more` pointer appears (totals ≠ displayed count).
- `test_get_high_degree_node_caps_at_three` — honest uncapped totals on a hub.

## Edge Cases
- Zero edges → `{inbound: 0, outbound: 0}` (feeds AC-06 empty state).
- High inbound + zero outbound (#744).
- Symmetric pair contributes once (all three types).
- >cap entry: totals exceed displayed count.

## Security
**`test_count_uses_positional_binds`** — `id` positional-bound; canonicalization `CASE` and the
direction predicates are static SQL.
