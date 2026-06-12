# Test Plan — `query_outgoing_edges` + `OutgoingEdgeRow` (store query)

> Component: new `unimatrix-store` query mirroring `query_incoming_edges` (read.rs:1694).
> Risks: **R-03** (predicate drift — High), **R-09** (missing index — Low).
> Eligibility predicate is the **single source of truth** and the safety basis for AC-09's
> "no ceiling" — its correctness is security-adjacent (a regression admitting
> `CoAccess`/`Informs` turns no-ceiling into unbounded fan-out).

## Signature under test

```rust
pub async fn query_outgoing_edges(&self, source_id: u64) -> Result<Vec<OutgoingEdgeRow>>;
// SQL: SELECT target_id, relation_type, created_at FROM graph_edges
//      WHERE source_id = ?1 AND relation_type NOT IN ('Supersedes','CoAccess','Informs')
// bind source_id as i64; read_pool(); inline superset-vs-incoming rationale comment.
```

`OutgoingEdgeRow { target_id: u64, relation_type: String, created_at: u64 }`.

## Unit Tests (extend `read.rs`-style store tests; mirror `query_incoming_edges` tests)

### `test_query_outgoing_excludes_derived_classes` (R-03, AC-04 unit half) — REQUIRED
- **Arrange**: seed entry A; insert four outgoing edges from A: `Supports` (eligible),
  `Supersedes`, `CoAccess`, `Informs`.
- **Act**: `store.query_outgoing_edges(A.id).await`.
- **Assert**: returns exactly one row, `relation_type == "Supports"`. The three derived/
  tick-generated classes are absent. Pins the exclusion set `('Supersedes','CoAccess','Informs')`.

### `test_query_outgoing_returns_eligible_with_fields` (AC-01 store half) — REQUIRED
- **Arrange**: seed A with two eligible outgoing edges (`Supports → X`, `Advances → Y`)
  inserted at known `created_at` timestamps.
- **Act**: `query_outgoing_edges(A.id)`.
- **Assert**: two rows; `target_id` and `relation_type` match the seeded triples;
  `created_at` reflects the stored value (read-only, for ordering/observability — NOT written
  onto B per ADR-004).

### `test_query_outgoing_empty_when_no_edges` (R-02 zero-carry support) — REQUIRED
- **Arrange**: seed A with no outgoing edges (optionally one **incoming** edge `E→A` to prove
  directionality).
- **Act**: `query_outgoing_edges(A.id)`.
- **Assert**: returns `Ok(vec![])`. The incoming edge does not leak into the outgoing result
  (confirms `WHERE source_id = ?1`, not `target_id`).

### `test_query_outgoing_only_ineligible_returns_empty` (edge case) — REQUIRED
- **Arrange**: seed A with only `Supersedes` + `CoAccess` rows.
- **Act**: `query_outgoing_edges(A.id)`.
- **Assert**: `Ok(vec![])` (`found > 0` raw rows but all excluded → empty eligible set).
  Supports the handler-level "ineligible-only → `carried == 0`, ack omitted" edge case.

## Single-Source Guard (R-03, NFR-02) — grep, not a runtime test

`test-plan`-driven Gate 3a/3c check (operationalized as a grep, AC design-mandated row):
- The exclusion list `('Supersedes','CoAccess','Informs')` appears in **exactly one** SQL
  clause. No parallel Rust-side `.filter(...)` re-implements eligibility.
- An **inline rationale comment** is present stating the outgoing predicate is a deliberate
  **superset** of the incoming predicate (`Supersedes` only) — not drift — so a future reader
  does not "align" them and re-admit `CoAccess`/`Informs`.

Stage 3c records the grep result (the clause count and presence of the comment) in
RISK-COVERAGE-REPORT.md as evidence for R-03 / the eligibility single-source check.

## Index Finding (R-09 / O-1) — resolved at plan time

`idx_graph_edges_source_id` **already exists** (`db.rs:969`, `migration.rs:367`). The
`WHERE source_id = ?1` filter is index-covered — no full-table scan, no latency concern for
high-out-degree hubs. **No functional test required.** Stage 3c notes "index present,
confirmed" in the report; correctness tests pass regardless of the index.

## Injection / Safety (Security Risks)

- `query_outgoing_edges` uses a parameterized `WHERE source_id = ?1` bind; the `NOT IN`
  predicate is a static literal list — no string interpolation, no injection surface. No
  dedicated security-suite test needed (confirmed in OVERVIEW.md harness plan).

## Out of scope for this component
- Count semantics, warn-and-continue, Contradicts → **run_carry_forward_loop.md**.
- Ack envelope, pipeline order, composition → **context_correct_handler.md**.
