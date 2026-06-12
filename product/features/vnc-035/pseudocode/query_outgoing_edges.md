# Component: `query_outgoing_edges` + `OutgoingEdgeRow`

**Crate / location:** `unimatrix-store/src/read.rs` (mirror of `query_incoming_edges` at
`read.rs:1694`), OR a new `read_outgoing.rs` module if `read.rs` breaches the 500-line rule
(O-2; `read.rs` is already >1570 lines per the `read.rs:1800` note — a new module is the likely
choice; developer decides on live line count). If split, re-export so `Store::query_outgoing_edges`
remains a method on `Store` (same impl block pattern as `query_incoming_edges`).

## Purpose

Read entry A's **eligible outgoing** graph edges (`source_id = A`) for carry-forward. This query
is the **single source of truth** for the outgoing eligibility predicate (NFR-02 / SR-03): the
agent-declared-only filter is expressed once, at the SQL level, mirroring `query_incoming_edges`'
`Supersedes`-at-SQL precedent (ADR-002 vnc-017). No parallel Rust-side filter may exist.

## Data structures

```rust
/// One eligible outgoing `graph_edges` row returned by `query_outgoing_edges` (vnc-035).
///
/// `source_id` is the query parameter and is implicit; it is not included in the struct
/// (mirrors `IncomingEdgeRow` which omits the implicit `target_id`).
///
/// `created_at` is read for ordering / observability ONLY. The carry-forward RE-STAMPS
/// `created_at = now` (the correction timestamp); the source row's value is NOT written
/// onto the new entry B (ADR-004 / FR-11). Reading it keeps the DTO symmetric with
/// `IncomingEdgeRow` and available for deterministic loop ordering if a test needs it.
#[derive(Debug, Clone)]
pub struct OutgoingEdgeRow {
    /// Entry ID of the target (the entry A declared an edge toward).
    pub target_id: u64,
    /// Relation type string as stored (e.g. "Supports", "Advances", "Contradicts").
    pub relation_type: String,
    /// Unix timestamp (seconds) when the original edge was created.
    /// NOT written onto B — see struct doc (ADR-004).
    pub created_at: u64,
}
```

## Function signature

```rust
pub async fn query_outgoing_edges(&self, source_id: u64) -> Result<Vec<OutgoingEdgeRow>>
```

`Result` is the store's `Result` alias (`StoreError`), exactly as `query_incoming_edges`.

## Pseudocode body (mirror of `query_incoming_edges`, on `source_id`)

```
query_outgoing_edges(self, source_id: u64) -> Result<Vec<OutgoingEdgeRow>>:

    SQL = "
        SELECT target_id, relation_type, created_at
        FROM graph_edges
        WHERE source_id = ?1
          AND relation_type NOT IN ('Supersedes', 'CoAccess', 'Informs')
        -- ELIGIBILITY PREDICATE — SINGLE SOURCE OF TRUTH (SR-03, ADR-002 vnc-035).
        -- Agent-declared edges carry forward on correction; derived/tick-generated classes
        -- do NOT (they re-materialize on their own):
        --   'Supersedes' — derived from entries.supersedes; rebuilt by the graph tick.
        --   'CoAccess'   — tick-generated co-access affinity; re-promoted by its own tick.
        --   'Informs'    — tick-generated affinity class; re-materializes.
        -- SUPERSET vs query_incoming_edges (which excludes ONLY 'Supersedes'):
        -- this is INTENTIONAL, NOT drift. CoAccess/Informs are OUTGOING-relevant from a hub
        -- entry but NOT incoming-relevant to a correction target, so the incoming query has
        -- no reason to exclude them. Do NOT 'align' the two predicates into false symmetry —
        -- doing so would silently carry tick-generated classes (R-03). See ADR-002.
    "

    rows = sqlx::query(SQL)
        .bind(source_id as i64)        // SQLite stores IDs as i64; cast u64 → i64 (read.rs convention)
        .fetch_all(self.read_pool())   // read_pool() — canonical accessor (db.rs:294 aliasing, C-07)
        .await
        .map_err(|e| StoreError::Database(e.into()))?

    // Map each row to OutgoingEdgeRow, mirroring query_incoming_edges' try_get + i64→u64 casts.
    return rows.into_iter().map(|row| {
        Ok(OutgoingEdgeRow {
            target_id:     row.try_get::<i64,_>("target_id").map_err(db_err)? as u64,
            relation_type: row.try_get("relation_type").map_err(db_err)?,
            created_at:    row.try_get::<i64,_>("created_at").map_err(db_err)? as u64,
        })
    }).collect::<Result<Vec<_>>>()
    // where db_err = |e| StoreError::Database(e.into())
```

## Data flow

- **Input:** `source_id: u64` — the original entry A's id, passed by `run_carry_forward_loop`.
- **Output:** `Vec<OutgoingEdgeRow>` — eligible outgoing edges, possibly empty. Empty is a
  normal result (entry with no eligible outgoing edges), NOT an error.
- **Consumer:** `run_carry_forward_loop` (`tools.rs`) iterates the rows and writes each onto B.

## Error handling

- SQL / pool failure → `Err(StoreError::Database(...))`. The query does NOT warn or swallow;
  it propagates `Err` to its caller. The caller (`run_carry_forward_loop`) is responsible for
  the warn-and-continue posture (it logs `warn!` and returns `CarrySummary{0,0,0}` — ADR-002).
  This matches `query_incoming_edges`, which also propagates `Err` to `run_redirect_loop`.
- Per-row `try_get` decode failure → `Err(StoreError::Database(...))` (same propagation). A
  partial decode does not return a partial vec — the whole query fails, caller warns-and-continues.

## Eligibility invariant (documented, NFR-02 / SR-03 / SR-04)

The exclusion list is the **only** definition of outgoing eligibility. "No outgoing ceiling"
(FR-09, AC-09) is safe **only** while eligibility = agent-declared-only — this predicate is what
bounds agent-declared out-degree. A future agent-declarable type added to the engine taxonomy
(`graph.rs:139`) carries automatically (accepted, SCOPE Assumptions). Any future defense against
fan-out is an observability warning that STILL carries every edge, never a truncating cap.

## Key test scenarios (hints for the tester — not the test plan)

- **Exclusion set unit test (AC-04 / R-03):** seed A with `Supports` (eligible) + `Supersedes`
  + `CoAccess` + `Informs` rows; assert `query_outgoing_edges(A)` returns ONLY the `Supports`
  row; all three derived/tick classes excluded.
- **Empty result:** entry with zero outgoing edges → `Ok(vec![])`, not `Err`.
- **Only-ineligible:** entry with only `Supersedes`/`CoAccess`/`Informs` → `Ok(vec![])`.
- **`created_at` carried in DTO:** assert the row's `created_at` is populated (used for
  observability; the *non-preservation onto B* is asserted in the carry-loop tests, not here).
- **Single-source guard (R-03 #3):** a grep/structure test asserting the `NOT IN (...)`
  exclusion list appears in exactly one SQL clause and no parallel Rust filter exists.
- **`source_id` parameterized bind (security):** the predicate is a static `NOT IN` literal;
  `source_id` is a `?1` bind — no string interpolation, no injection surface.

## Notes / open questions

- **O-1 (index):** `query_incoming_edges` relies on `idx_graph_edges_target_id`. This query
  filters on `source_id`; confirm whether `idx_graph_edges_source_id` exists (mirror of the
  target index). If absent it is a latency concern only (R-09), one query per correction —
  likely fine; developer verifies and notes the finding inline. Not a correctness blocker.
