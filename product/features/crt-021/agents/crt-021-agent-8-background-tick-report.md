# Agent Report: crt-021-agent-8-background-tick

**Agent ID**: crt-021-agent-8-background-tick
**Component**: background-tick
**File modified**: `crates/unimatrix-server/src/background.rs`
**Feature**: crt-021 (Typed Relationship Graph W1-1)

---

## Summary

Inserted GRAPH_EDGES orphaned-edge compaction as Step 2 in the `run_single_tick` maintenance
sequence — positioned after `maintenance_tick()` (which includes VECTOR_MAP compaction internally)
and before the existing `TypedGraphState::rebuild` block. The sequence is strictly sequential with
no concurrent dispatch across any of the three steps (C-07, FR-24 satisfied).

---

## Changes Made

### `crates/unimatrix-server/src/background.rs`

**Production code** (lines ~432-478 in the modified file):

Added a scoped block immediately after the `maintenance_tick` timeout block and before the
`TypedGraphState::rebuild` block. The block runs:

```sql
DELETE FROM graph_edges
WHERE source_id NOT IN (SELECT id FROM entries)
   OR target_id NOT IN (SELECT id FROM entries)
```

Via `sqlx::query(...).execute(store.write_pool_server()).await` — direct write pool, no analytics
queue involvement (per ADR-001 write-path contract).

Error handling:
- `Ok(result)`: logs `rows_deleted` at INFO level if > 0 (silent on zero-row run)
- `Err(e)`: logs ERROR, continues tick — compaction failure is non-fatal (rebuild proceeds on
  pre-compaction state; orphaned edges are silently skipped by `build_typed_relation_graph`)

**TypedGraphState rebuild** (lines ~480-530): already correctly wired from Wave 3
(`TypedGraphState::rebuild` call, cycle-detected arm sets `use_fallback=true`, store error arm
retains old state). No changes needed to this block.

**Tests added** (7 new tests in `background::tests`):

| Test | Plan Coverage |
|------|---------------|
| `test_background_tick_compacts_orphaned_graph_edges` | AC-14, R-11 |
| `test_background_tick_compaction_handles_empty_graph_edges` | empty-table edge case |
| `test_background_tick_compaction_removes_multiple_orphaned_edges` | AC-14 (3-orphan batch) |
| `test_background_tick_compaction_completes_within_budget` | R-11 perf guard (1000 rows, <1s) |
| `test_background_tick_compaction_uses_write_pool_not_analytics_queue` | structural gate |
| `test_typed_graph_state_handle_swap_in_tick_pattern` | AC-13 rebuild swap |
| `test_typed_graph_state_handle_cycle_sets_fallback_without_swap` | cycle-detected arm |

Test helpers added (not registered as tests): `insert_graph_edge`, `insert_test_entry`,
`count_graph_edges`, `run_graph_edges_compaction`.

---

## Verification

- `cargo build --workspace`: PASS (zero errors, pre-existing warnings only)
- `cargo test -p unimatrix-server --lib`: PASS — 1462 tests (53 in background module)
- `cargo fmt --all`: applied
- `cargo clippy -p unimatrix-server`: no new errors

Doctest failure in `unimatrix-server` (`config.rs` `~/.unimatrix/config.toml` path line) is
pre-existing and unrelated to this change.

---

## Sequencing Invariant Confirmed

Per pseudocode C-07 / FR-24: steps 2, 3, 4 are sequential `await` calls — no `tokio::join!` or
concurrent `tokio::spawn` wrapping any of the three steps. Confirmed by code inspection.

---

## Issues / Deviations

None. Wave 3 had already wired `TypedGraphState::rebuild` correctly into the tick. This agent's
only task was to insert the compaction step and add tests — both complete.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` (via `context_search` on "background tick maintenance sequence compaction") — found entries #1542, #1366, #1560 confirming the `Arc<RwLock<T>>` sole-writer pattern and extract-and-catch tick error recovery conventions. Applied both.
- Stored: nothing novel to store — the `write_pool_server()` direct-pool pattern for maintenance writes and the tick sequencing convention are already captured in existing entries (#1560, #732). No new gotchas discovered beyond what's in the codebase and existing knowledge base.
