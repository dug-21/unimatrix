# Agent Report — vnc-035-agent-3-query_outgoing_edges

## Task
Implement `query_outgoing_edges(source_id) -> Result<Vec<OutgoingEdgeRow>>` + the
`OutgoingEdgeRow` DTO in `unimatrix-store`, mirroring `query_incoming_edges`.

## Files Modified
- `crates/unimatrix-store/src/read_outgoing.rs` (NEW) — `OutgoingEdgeRow` DTO,
  `impl SqlxStore::query_outgoing_edges`, 4 unit tests.
- `crates/unimatrix-store/src/lib.rs` — `pub mod read_outgoing;` + `pub use read_outgoing::OutgoingEdgeRow;`

Committed: `impl(store): query_outgoing_edges + OutgoingEdgeRow for carry-forward (#730)`

## Key Decisions
- **New module (O-2):** `read.rs` is 3765 lines — far over the 500-line limit — so the
  new query lives in `read_outgoing.rs` with its own `impl SqlxStore` block. The method
  stays a method on `Store`; the DTO is re-exported from lib.rs.
- **Single-source eligibility predicate (NFR-02/SR-03/R-03):** `NOT IN ('Supersedes',
  'CoAccess','Informs')` expressed ONCE at SQL level with an inline rationale comment
  documenting the deliberate SUPERSET-vs-incoming difference so a future reader does not
  "align" them into false symmetry. No parallel Rust-side filter exists.
- `source_id` bound as i64; `read_pool()` used; `Err` propagated (caller warns-and-continues).
- `created_at` read into the DTO for observability/ordering only (ADR-004 — not written onto B).

## Tests
4 unit tests, all passing (component run + full `-p unimatrix-store --lib`):
- `test_query_outgoing_excludes_derived_classes` (R-03, AC-04) — only `Supports` returns.
- `test_query_outgoing_returns_eligible_with_fields` (AC-01) — fields + created_at match.
- `test_query_outgoing_empty_when_no_edges` (R-02) — incoming edge does not leak (directionality).
- `test_query_outgoing_only_ineligible_returns_empty` — raw rows present, eligible set empty.

`cargo test -p unimatrix-store --lib`: **344 passed; 0 failed** (340 prior + 4 new).
`cargo clippy -p unimatrix-store --lib`: no warnings in `read_outgoing.rs`. `cargo fmt` applied.

## Out of Scope (handled by other agents)
- `run_carry_forward_loop` / `CarrySummary` / count semantics → tools.rs agent.
- The single-source grep guard + index-present note → Stage 3c (RISK-COVERAGE-REPORT.md).
  Index `idx_graph_edges_source_id` already exists (db.rs:969 / migration.rs:367) — no index work.

## Issues / Blockers
None.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern) — surfaced #4417 (agent-declared edge
  writes), #3884 (INSERT OR IGNORE graph edges), #2451 (dual GraphEdgeRow mapping). Confirmed
  the i64-bind / `try_get` mapping convention. ADR-002 (#4984) read from file.
- Stored: entry #4993 "Splitting a Store query into a new module: extra impl SqlxStore block +
  pub(crate) write_pool in tests" via `context_store` (pattern). Novel trap: the
  `#[cfg(test)] write_pool_test()` accessor does NOT resolve from a sibling test module —
  must use the `pub(crate) write_pool` field directly.
