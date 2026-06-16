## ADR-004 (vnc-037): Read query extends additively — `source` on the shared SELECT, plus a confidence JOIN and canonicalization on the get-only ranked variant; no migration; `context_graph` neighbors unaffected

> **EXTENDED under the next-hop reframe.** The get read path now needs two more things
> beyond `source`: the **target-entry confidence** as the inferred rank key (ADR-006) and
> the **symmetric canonicalization** (ADR-007). Both are still **additive and read-only —
> no DDL, no migration** — but they live on a **separate ranked variant**, not on the
> shared plain SELECT, so the `context_graph` neighbors contract stays byte-stable.

### Context

`authored` (D-03) is `source == "agent"`: a human/agent declared the edge, vs. a
statistical inference (co-access / cosine; NLI is dark per ASS-037, so the inferred bucket
today is entirely statistical — a boolean is the honest trust split). But
`query_direct_neighbors` and its `RawEdgeRow` currently carry only
`{source_id, target_id, relation_type}` — the `graph_edges.source` column
(`db.rs:960`) is **not selected**. So `source` must be added to the read path.

The reframe adds two read-path needs: (a) the ranked select orders inferred edges by the
**target entry's** `entries.confidence` (`db.rs:549`) — a column on `entries`, joinable via
the other endpoint (ADR-006); and (b) symmetric two-row edges must be **canonicalized** to
one logical row before ranking and counting (ADR-007). Neither requires a schema change —
`confidence` already exists, canonicalization is pure query logic — but both must be kept
**off the shared plain SELECT** that `context_graph` neighbors consumes (SR-02 / SR-06).

SR-02 (High): `query_direct_neighbors` / `RawEdgeRow` are **shared** with `context_graph`
neighbors mode (`neighbors_sql`, `graph_read_neighbors.rs:187`). A struct or SELECT change
that is incompatible — or that `neighbors_sql` consumes in a way that shifts behavior —
breaks the existing neighbors contract (ADR #4478/#4479). The additive-field blast-radius
lesson (#4831) and the "structural reasoning is insufficient — build + run the existing
tests" lesson (#4876) both apply.

### Decision

Extend the read path **additively, read-only, no DDL**:

1. **`RawEdgeRow` gains `pub source: String`** (`graph_queries.rs:73`). New field appended;
   the three existing fields are untouched.
2. **`map_edge_row` reads it** (`graph_queries_neighbors.rs:95`):
   `source: row.try_get("source").map_err(…)?` — same `try_get` + `StoreError` mapping as
   the existing fields, no `.unwrap()`.
3. **All four neighbor SELECTs add `source`** — both the empty-`edge_types` branch and the
   `IN (…)` branch, in both `run_outgoing_query` and `run_incoming_query`:
   `SELECT source_id, target_id, relation_type, source FROM graph_edges WHERE …`. No WHERE,
   index, or filter change; `source` is an existing column, so no migration.
4. **`context_graph` neighbors is verified unaffected empirically.** `neighbors_sql`
   constructs `EdgeRecord` from `row.source_id`/`row.target_id`/`row.relation_type` by
   name and never matches `RawEdgeRow` exhaustively — adding a field is compatible. The
   requirement (not just the reasoning): build, then run the existing neighbors / graph
   tests and confirm green, per lesson #4876. `EdgeRecord` itself is **not** changed (it
   keeps no `source`/`authored`); only the get-path projection (ADR-002) reads
   `RawEdgeRow.source`.
5. **`source` stays a `String` underneath; only the get path collapses it to `authored`
   (ADR-002).** The raw string is retained on `RawEdgeRow` so a future non-statistical
   inferred source (e.g., NLI revival, SR-05) can revisit the boolean without another
   read-path change — the boolean's NLI-dark precondition is the documented trigger to
   revisit.

6. **The get-only ranked variant additionally JOINs confidence and canonicalizes — on its
   own SQL, never the shared plain SELECT.** The ranked variant (ADR-001):
   - `LEFT JOIN entries t ON t.id = <the OTHER endpoint>` and selects `t.confidence` into a
     new `RawEdgeRow.target_confidence: Option<f64>` (populated by the ranked variant only;
     `None` for dangling targets, ranked last via `NULLS LAST`). `LEFT` (not inner) retains
     dangling targets per D-02 / SR-11. The rank key is never *surfaced* — it is consumed
     by `ORDER BY` and dropped at projection (ADR-002 forbids enrichment).
   - Applies the symmetric canonicalization (ADR-007) before `ORDER BY…LIMIT` and the split
     `COUNT(*)`.
   `entries.confidence` is read-only here — no write, no DDL. The shared plain SELECT used
   by `context_graph` neighbors gains **only** the `source` column (point 3); it does **not**
   gain the JOIN or canonicalization. This is the SR-02/SR-06 firewall: get-only ranking
   logic must not leak into the neighbors contract.

### Consequences

- **Easier:** `authored` becomes computable with one additive column; the shared plain
  consumers keep one row type, preventing shape drift; future provenance revival needs no
  new SELECT (the string is already fetched); the confidence rank key rides an existing
  column + the `entries` PK, so no migration and an indexed JOIN.
- **Harder:** every shared `RawEdgeRow` construction site and the four plain SELECT strings
  must be updated together for `source` (front-load per #4831 rather than discover one
  compile error at a time); the shared-row change demands an *empirical* re-verification of
  `context_graph` neighbors, not a structural argument; the ranked variant carries an extra
  optional field (`target_confidence`) populated only on its path, so projection code must
  ignore it for the shared consumers; the extra `source` column is fetched on every neighbor
  query including `context_graph`'s, which ignores it — negligible but non-zero.
- **Cross-ref:** ADR-001 (the ranked query + split count that issue this), ADR-002 (the
  projection that consumes `source` as `authored` and drops `target_confidence`), ADR-006
  (the ranking rule the confidence JOIN serves), ADR-007 (the canonicalization).
