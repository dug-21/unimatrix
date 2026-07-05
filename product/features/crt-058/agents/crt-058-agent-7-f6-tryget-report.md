# Agent Report: crt-058-agent-7-f6-tryget

## Task
F-6 (non-blocking robustness): replace panicking `row.get(...)` marshaling in
`delete_agent_edges_for_entry` with fallible `try_get(...)` so a future nullable
RETURNING column routes through the function's non-fatal `Err` path instead of a
post-commit panic.

## Change
File: `crates/unimatrix-server/src/mcp/edge_write.rs` — marshaling loop only.

- `source_id`, `target_id`, `relation_type` now read via `row.try_get(...)`.
- Each `try_get` error maps to the function's existing error type using the SAME
  mapping already used for the `fetch_all` query error:
  `.map_err(|e| EdgeDeleteError::StoreError(StoreError::Database(e.into())))?`.
  `try_get` yields `sqlx::Error`, `e.into()` lifts it into the store's boxed DB
  error exactly as the query path does. No new error variant was added — the
  existing `StoreError` variant fits, so the failure surfaces through the caller's
  non-fatal `edges_removed = None` + `warn!` path.
- No panic, no `unwrap`/`expect` on row values.

## Invariants preserved (unchanged)
- LOCKED predicate `(source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING
  source_id, target_id, relation_type`.
- Single `fetch_all`; count = `removed.len()` (tuples), not `rows_affected()`.
- Pool `write_pool_server()`; bindings `?1 = entry_id`, `?2 = EDGE_SOURCE_AGENT`.

`use sqlx::Row;` (already in scope) provides `try_get`; no new import.

## Verification (foreground)
- `cargo build -p unimatrix-server`: PASS (Finished dev profile).
- `cargo clippy -p unimatrix-server --all-targets`: PASS (no warnings).
- `cargo test -p unimatrix-server edge_write`: PASS — 20/20 (0 failed), including
  per-source (`test_delete_agent_edges_only_removes_agent_source`), self-loop
  (`test_self_loop_agent_edge_removed_and_counted_once`), tuple-capture
  (`test_count_source_of_truth_is_tuples_len_not_rows_affected`,
  `test_delete_returning_is_single_statement_capture`), and predicate/pool-lock tests.

Note: the nullable-column fault is not unit-injectable against the NOT-NULL schema,
so no new test was added; happy-path marshaling behavior is unchanged and covered by
the existing passing tests.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- not invoked; this is a single-file,
  single-line marshaling-robustness fold-in on an already-delivered/gated PR with a
  fully specified spawn brief (exact edit, exact error mapping, LOCKED predicate). No
  discovery task; the change reuses an existing in-file mapping pattern.
- Stored: nothing novel to store -- the `try_get`-over-`get` non-fatal-contract
  practice is already the established convention this change conforms to (the panic
  was the outlier), and no new runtime trap was discovered.
