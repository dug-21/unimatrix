# Agent Report — vnc-037-agent-3-store-ranked-query

## Scope
Ranked read path for the `context_get` next-hop edge affordance: one new file
`graph_queries_ranked.rs` co-locating `query_ranked_neighbors` and
`count_neighbors_split` over ONE shared canonicalization CTE (ADR-007 parity).

## Files Modified
- `crates/unimatrix-store/src/graph_queries_ranked.rs` (new) — both functions +
  `EdgeCountSplit` + `RankedEdge` + shared `CANON_CTE` const.
- `crates/unimatrix-store/src/graph_queries_ranked_tests.rs` (new) — 24 unit tests.
- `crates/unimatrix-store/src/lib.rs` — `pub mod graph_queries_ranked;` + re-export of
  `EdgeCountSplit, RankedEdge, count_neighbors_split, query_ranked_neighbors`.

## Tests
- Ranked suite: 26 test fns pass / 0 fail (symmetric-type loops iterate all three types).
- Full store lib: 389 passed / 0 failed.
- Existing `context_graph` neighbors + chain suite: green, UNEDITED (`graph_queries::tests`).
- `cargo build --workspace`: clean (server warnings pre-existing, unrelated).
- clippy on the new files: zero warnings. `cargo fmt` applied.

Coverage includes the load-bearing discriminators:
- `#3886` proof-outside-cap: high-confidence/low-weight target wins; weight does NOT decide.
- Canonicalization on BOTH ranked set and split count (Contradicts/CoAccess/Informs).
- `#744` integrity: `↔` increments `both`, never `inbound`; mixed N CoAccess + M inbound.
- authored aggregate over the full deduped set; high-degree (≥50) hits SQL LIMIT;
  dangling target retained + NULLS LAST; Supersedes excluded; positional binds; no literal 3.

## Design Note (authorized deviation, not silent)
The pseudocode signature shows `Vec<RawEdgeRow>`; the "carrying direction" note
explicitly authorizes option (a) — widen the row mapping to carry the SQL-computed
`direction` alongside `RawEdgeRow`. Implemented as `RankedEdge { row: RawEdgeRow,
direction: String }` (the clean form of (a)), so the get-edge projection reads the
canonicalization decision directly and never re-derives a `↔` as `→`/`←`. The symmetric
type list stays in exactly one place (the `CANON_CTE` `CASE`).

## Issues / Blockers
None. Confined to the authorized files; did not run git; did not touch integration tests
or server-crate files.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- SKIPPED, Unimatrix MCP disconnected per
  spawn prompt; read ADR files (ADR-001/006/007 + amended ADR-005 totals-bucket contract)
  and pseudocode directly instead.
- Stored: nothing -- MCP disconnected, cannot store. Candidate pattern worth recording when
  reconnected: "SQLite `GROUP BY` on a 4-tuple including `CASE WHEN direction='both' THEN 1
  ELSE other_id END` collapses reciprocal symmetric pairs to one row while keeping distinct
  asymmetric neighbors separate — the load-bearing canonicalization mechanism; both the
  ranked select and split count MUST embed the byte-identical CTE (extract to one const) or
  they double-count on one surface only." Topic: unimatrix-store.
