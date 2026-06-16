# Test Plan — store-split-count (`count_neighbors_split`)

A new `COUNT(*)` over the **canonicalized** neighbor set, split into **THREE buckets**
inbound/outbound/both, **uncapped**, returning `EdgeCountSplit { inbound, outbound, both }` plus a
**fourth digest-only aggregate** `authored` (`SUM(source='agent')` over the same `deduped` CTE).
Owns **Critical R-03** and the totals half of **R-01**. The defining hazard: this query and the
ranked query are **two queries that must agree** on canonicalization (RISK-TEST-STRATEGY
Integration Risks) — a divergence double-counts on one surface only. Store unit tests against
seeded `graph_edges`.

> **TOTALS BUCKET CONTRACT (ADR-005, locked 2026-06-16).** THREE buckets, each edge counted
> **exactly once**, all UNCAPPED: `outbound` = `direction='outbound'`; `inbound` =
> `direction='inbound'` **ONLY** (the old `IN ('inbound','both')` fold is RETIRED — `↔` no longer
> counts as inbound); `both` = `SUM(direction='both')` (canonicalized symmetric, counted once).
> The digest-only `authored` = `SUM(source='agent')` over the full canonicalized set (NOT a
> JSON/markdown key). The markdown `…N more` grand total = `inbound + outbound + both`.

> **Invariant under test:** "symmetric counted once" holds on **totals independently of the
> displayed set** (now counted once in `both`). A fix that dedups the rendered list but misses
> count-dedup MUST fail here. **#744 inbound-degree integrity:** a `↔` edge MUST increment `both`
> and **NEVER** `inbound` — folding `↔` into `inbound` corrupts the asymmetric inbound-degree
> observability signal the split exists to serve.

## Unit Test Expectations

### R-03 — Split COUNT divergence / canon mismatch (Critical)

**`test_count_uncapped_exact`** (FR-10/SR-14)
Seed **8** mixed-direction edges on anchor A (> cap). Assert `inbound + outbound + both == 8` (the
true **uncapped** total across all three buckets) — the count is NOT computed off the capped ≤3
Vec. Pair with a rank-query assertion (in store-ranked-query) that the displayed set is ≤3, so the
cap/totals decoupling is proven from both sides.

**`test_count_direction_split_load_bearing`** (#744 observability)
Seed an entry with high inbound + **zero** outbound (e.g. 5 incoming `Prerequisite`, 0 outgoing,
0 symmetric). Assert `inbound == 5, outbound == 0, both == 0`. Proves inbound-degree observability
survives — the totals are genuinely split, not a single scalar.

**`test_count_symmetric_increments_both_never_inbound`** (#744 inbound-degree-integrity regression — REPLACES the old "↔ folds into inbound" assertion)
Seed an anchor with N reciprocal symmetric pairs (`Contradicts`/`CoAccess`/`Informs`) and **zero**
true inbound asymmetric edges. Assert each `↔` increments **`both`** (`both == N`) and that
`inbound == 0` — the `↔` edges NEVER land in `inbound`. This is the exact #744 regression: a node
with N `CoAccess` + 0 true inbound must read `inbound:0, both:N`, NOT a false `inbound:N`. Assert
`both` is **distinct from** `inbound` (the deciding three-bucket guard). The retired convention was
`↔` bucketed into `inbound` (`IN ('inbound','both')` fold) — that test is removed; this replaces it.

**`test_count_authored_aggregate_over_full_set`** (digest `(K authored)` source — TOTALS BUCKET CONTRACT §3)
Seed a >cap mixed set with a known number of `source='agent'` edges and inferred edges spanning all
three buckets. Assert the query returns `authored ==` the count of `source='agent'` rows over the
**entire canonicalized `deduped` set** (NOT the displayed ≤3, NOT only the `both` bucket). E.g.
9 symmetric edges where 7 are agent-asserted ⇒ `authored == 7`. This fourth aggregate is
digest-only: it is **NOT** a JSON `edge_totals` key and **NOT** a markdown field; assert it is
carried out of the store (field of `EdgeCountSplit` or sibling return) for the summary renderer.

**`test_count_symmetric_counted_once`** (R-01 totals side — SEPARATE assertion)
For each of `Contradicts`, `CoAccess`, `Informs`: seed the pair as **both reciprocal rows**.
Assert the relationship contributes **once** to the split — exactly one increment to the **`both`**
bucket (canonical `direction='both'` row per ADR-007 #4), **not** once inbound + once outbound and
**not** once inbound (the retired fold). Assert this **directly on the `EdgeCountSplit` output**,
independent of the ranked query.

**`test_count_canon_parity_with_rank_query`** (the two-queries-must-agree guard)
Seed a mixed set including a symmetric pair. Assert the count query's canonicalization yields the
**same** symmetric-once result the ranked query produces (cross-check: the symmetric pair occupies
**one** slot in the ranked output AND contributes **one** to the count). A divergence (count
dedups differently than rank) fails this test.

**`test_count_before_canon_would_double`** (order-of-ops negative framing)
Document/assert that counting happens **post-canonicalization**: a symmetric pair seeded as two
rows yields a total of 1 for that relationship, not 2 — i.e. canon precedes `COUNT(*)`.

### R-04 — COUNT in SQL, scalar return (Critical)

**`test_count_returns_scalars_not_materialized`** (SR-14)
On the ≥50-edge high-degree fixture, assert `count_neighbors_split` returns **scalar aggregates**
(the three bucket counts + the digest-only `authored` aggregate) and **never materializes** the
neighbor rows (the function "never materializes rows" per signature — it is a `COUNT(*)`/`SUM`
aggregate query). Assert via store boundary: no `Vec<RawEdgeRow>` allocation path.

### R-03 — Nested shape (carried to render)

**`test_count_nested_shape_three_buckets`** (OQ-01/ADR-005)
Assert the returned struct exposes `inbound`, `outbound`, and `both` as the
`{inbound, outbound, both}` object the JSON `edge_totals` renders (three keys, not a flat or capped
scalar, not the old two-key shape). (Render shape itself is in serializer-seam; this asserts the
store value feeding it.) Assert the digest-only `authored` aggregate is carried alongside but is
NOT part of the nested `edge_totals` object.

### R-12 — Supersedes excluded from totals (Medium)

**`test_count_supersedes_not_counted`**
Seed a `Supersedes` edge among typed edges; assert it is **absent from all three** `inbound`,
`outbound`, and `both` counts, and excluded from the `authored` aggregate (filter inheritance
survives the count-query rewrite).

## Integration Expectations (through MCP)
- `test_get_edge_totals_symmetric_once` — `edge_totals` (3-key `{inbound,outbound,both}`) in the
  JSON response counts a symmetric pair once in `both`, with `inbound` unchanged (separate from the
  displayed-set assertion).
- `test_get_capped_pointer_when_more_than_cap` — on a >cap entry, totals report the true uncapped
  count and the `…N more` pointer appears (totals ≠ displayed count).
- `test_get_high_degree_node_caps_at_three` — honest uncapped totals on a hub.

## Edge Cases
- Zero edges → `{inbound: 0, outbound: 0, both: 0}`, `authored == 0` (feeds AC-06 empty state /
  `edges: none` digest sentinel).
- High inbound + zero outbound + zero symmetric (#744): `inbound:5, outbound:0, both:0`.
- Symmetric `↔` edge increments `both`, NEVER `inbound` (#744 inbound-integrity).
- Symmetric pair contributes once to `both` (all three types).
- `authored` aggregate over the FULL uncapped set, not the displayed ≤3 (digest `(K authored)`).
- >cap entry: totals (`inbound+outbound+both`) exceed displayed count.

## Security
**`test_count_uses_positional_binds`** — `id` positional-bound; canonicalization `CASE` and the
direction predicates are static SQL.
