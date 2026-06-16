# Test Plan — store-ranked-query (`graph_queries_ranked.rs`)

The ranked variant: **canonicalize symmetric → LEFT JOIN `entries.confidence` → `ORDER BY
(source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?`** with `?` bound to
`GET_EDGE_DISPLAY_LIMIT`. This component owns the **two Critical risks R-02 (ranking) and R-04
(SQL-not-Rust)** and half of R-01 (canon before rank) and R-06 (LEFT JOIN). All tests are store
unit tests (`#[sqlx::test]` against a seeded `graph_edges` + `entries`), asserting at the
**store boundary** — the returned `Vec<RawEdgeRow>`, before any projection.

> **Trace discipline (#3645/#3621).** Every ranking/canon scenario below carries a per-edge
> trace: `(source, target_confidence, weight) → expected slot`. The expected top-3 is **derived
> from the rule**, never intuited.

## Unit Test Expectations

### R-02 — Ranking ORDER BY (Critical, discriminating per #3886)

**`test_query_ranked_by_target_confidence_proof_outside_cap`** (the load-bearing #3886 test)
Seed `GET_EDGE_DISPLAY_LIMIT + 1` (=4) **inferred** edges (`source='co_access'`) from anchor A,
confidences straddling the cap boundary, and crucially seed the **high-confidence target with a
LOWER `graph_edges.weight`** so weight-ordering and confidence-ordering disagree:

| edge | target | source | t.confidence | weight | correct slot |
|------|--------|--------|--------------|--------|--------------|
| A→T1 | T1 | co_access | 0.90 | **0.1** | 1 (in top-3) |
| A→T2 | T2 | co_access | 0.70 | 1.0 | 2 |
| A→T3 | T3 | co_access | 0.50 | 1.0 | 3 |
| A→T4 | T4 | co_access | 0.30 | 1.0 | **excluded** |

- Assert returned rows (len == `GET_EDGE_DISPLAY_LIMIT`) are exactly `[T1, T2, T3]` ordered by
  descending confidence.
- Assert **T1 is present** despite its lowest weight, and **T4 is absent**.
- A weight-ordering bug ranks `T2,T3,T4` first (T1's weight 0.1 sinks it) → produces a **visibly
  different** top-3 → test fails correctly. A batch-local / insertion-order bug likewise diverges.
- **Assert weight does NOT decide:** the high-conf/low-weight target wins. This is the
  proof-outside-cap discriminator (#3886).

**`test_query_ranked_authored_priority_under_cap`** (R-02, `(source='agent') DESC` term)
Seed >3 edges with **≥3 authored** (`source='agent'`, e.g. 3 `Prerequisite` + 2 `co_access`).
Trace: all 3 authored sort first (term 1) → `LIMIT 3` consumed by authored → no inferred row.
Assert: returned set is **exactly the 3 authored**, **no** inferred target appears.

**`test_query_ranked_inferred_fill_only_when_authored_lt_3`** (R-02)
Seed 1 authored + 3 inferred (confidences 0.8, 0.6, 0.4). Trace: authored slot 1; inferred fill
slots 2–3 by descending confidence (0.8, 0.6); 0.4 excluded. Assert order `[authored, T(0.8),
T(0.6)]`, len == cap.

**`test_query_ranked_deterministic_tiebreak`** (R-02/R-06)
Seed ≥4 inferred with **equal** confidence (e.g. all 0.5) targets T9,T7,T5,T3. Trace: tier
collapses to `target_id ASC` → `[T3, T5, T7]`. Assert stable order across **repeated runs** (run
the query twice, assert identical). Cold-start variant: all `entries.confidence = 0.0` (default)
→ same `target_id ASC` resolution, **not** arbitrary row order.

### R-04 — Rank-and-limit in SQL, not Rust (Critical, store boundary)

**`test_query_ranked_high_degree_returns_exactly_cap_rows`** (SR-14/FR-11)
Seed a node with **≥50** edges. Assert `query_ranked_neighbors` returns **exactly
`GET_EDGE_DISPLAY_LIMIT` rows** — the full neighbor set is **never** materialized. Prove at the
store boundary (returned `Vec` len == cap), not via rendered output (a Rust-slice bug would
satisfy the rendered output too).

**`test_query_ranked_sql_carries_limit_not_select_star_slice`** (C-7, structural)
Assert the executed statement carries `LIMIT ?` (bound to the constant) — not a `SELECT *`
followed by Rust truncation. Verifiable via the static SQL string and/or a query-log assertion;
at minimum, the ≥50-edge fixture allocates ≤ `GET_EDGE_DISPLAY_LIMIT` `RawEdgeRow`.

### R-01 (this side) — Canonicalize BEFORE rank

**`test_query_ranked_canon_before_cap_authored_wins`** (order-of-operations)
Seed **>3 symmetric pairs** (each as both reciprocal rows) **plus one authored asymmetric edge**.
Trace: canonicalization collapses each symmetric pair to one row *before* the cap → the authored
edge still ranks first and **wins a slot**. Assert the authored edge is in the returned set
(proves symmetric rows were collapsed before the cap consumed slots — a no-canon impl would let
duplicate symmetric rows crowd the authored edge out under `LIMIT 3`).

**`test_query_ranked_symmetric_collapses_to_one_row`** (R-01 display side)
For each of `Contradicts`, `CoAccess`, `Informs`: seed the pair as both rows; assert the ranked
query returns **one** row for that relationship (not two), carrying the canonical anchor.

**`test_query_ranked_asymmetric_untouched`** (R-01/R-10)
A `Prerequisite` (single row) and a `Supports` (single row): assert each passes through as its
own row, **not** collapsed, anchor/direction preserved.

### R-06 — Confidence LEFT JOIN (High)

**`test_query_ranked_dangling_target_retained_nulls_last`** (DNB-1/SR-11)
Inferred edge whose `target_id` has **no `entries` row**. Assert: edge is **retained** (LEFT
JOIN, not dropped), `target_confidence` is `None`, and it ranks **last** among inferred
(`NULLS LAST`) — seed a resolved inferred edge above it and assert ordering.

**`test_query_ranked_join_is_left_not_inner`**
Seed ONLY a dangling inferred edge; assert it appears in the result (an INNER JOIN would drop
it → empty result → test fails).

**`test_query_ranked_null_confidence_deterministic`**
Mix resolved (0.6) + two NULL-confidence inferred targets; assert resolved ranks first, the two
NULLs resolve by `target_id ASC`, stable across runs.

### R-12 — Supersedes excluded (Medium)

**`test_query_ranked_supersedes_absent`**
Seed a `Supersedes` edge alongside typed edges; assert it is **absent** from the ranked result
(the `!= 'Supersedes'` filter inherited via the empty-`edge_types` branch survives the
canonicalization/SELECT rewrite — re-verified against the new SQL).

## Integration Expectations (through MCP)

- `test_get_rank_by_target_confidence` (tools suite) — end-to-end echo of the discriminating
  case; the displayed edges reflect the confidence ranking. The proof-outside-cap discrimination
  itself lives in the store unit test above.
- `test_get_authored_priority_under_cap`, `test_get_inferred_fill_when_authored_lt_3`,
  `test_get_supersedes_absent`, `test_get_dangling_title_null_retained` — MCP-visible confirmation.
- `test_get_high_degree_node_caps_at_three` (volume/tools) — hub get surfaces ≤3.

## Edge Cases (from Risk Strategy)
- Proof value **outside the cap** with disagreeing weight (#3886) — mandatory.
- Cold-start uniform 0.0 confidence → tiebreak, not arbitrary (A4).
- Symmetric pair + authored edge under cap (order-of-ops).
- All-dangling inferred set (LEFT JOIN retains all, NULLS-ordered).

## Security
**`test_query_ranked_uses_positional_binds`** — assert the `id` and `LIMIT` are positional binds
(`?`), never string-interpolated; the canonicalization `CASE`, `ORDER BY`, and `LIMIT` keyword
are **static SQL**, not assembled from input.
