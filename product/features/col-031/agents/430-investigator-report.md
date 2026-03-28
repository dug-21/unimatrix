# Bug Investigation Report: 430-investigator

## Bug Summary

`write_auto_outcome_entry()` in `crates/unimatrix-server/src/uds/listener.rs` calls `store.insert(entry)` directly, writing session telemetry rows into the ENTRIES table (the main knowledge store) on every session close that had injections. These rows have UUID-derived titles, one-line content, no embeddings, and land in ENTRIES permanently. Additionally, OUTCOME_INDEX is never populated from this path — contradicting the col-010 design intent.

## Root Cause Analysis

The col-010 pseudocode (`product/features/col-010/pseudocode/auto-outcomes.md`, §4) made a false claim:

> "When `insert_entry` is called with a non-empty `feature_cycle` and `category = 'outcome'`, the existing code in `write.rs` already populates OUTCOME_INDEX."

This was wrong. `write.rs::SqlxStore::insert()` has never called `insert_outcome_index_if_applicable()`. That call exists only in:
- `crates/unimatrix-server/src/services/store_ops.rs` (StoreService) — line 256
- `crates/unimatrix-server/src/server.rs` (`insert_with_audit`) — line 480

The col-010 implementer followed the pseudocode and used `store.insert()` directly in a fire-and-forget `tokio::spawn`. This path writes to ENTRIES, never populates OUTCOME_INDEX, never inserts into VECTOR_MAP, and computes no embedding.

### Code Path Trace

```
UDS SessionClose hook event
  → process_session_close()                    listener.rs:~1650
    → drain_and_signal_session()               listener.rs:1688
    → [if !is_abandoned && injection_count > 0]
      → write_auto_outcome_entry()             listener.rs:1762, 1880
        → tokio::spawn(async move {
            store_clone.insert(entry).await    write.rs:18  ← WRONG
          })
        // insert_outcome_index_if_applicable() NEVER CALLED
        // put_vector_mapping() NEVER CALLED
```

### Why It Fails

`store.insert()` is a thin SQLite write to ENTRIES only. The full "store a knowledge entry" pipeline lives in `StoreService::store_entry()` (store_ops.rs) or `insert_with_audit()` (server.rs). `write_auto_outcome_entry()` uses neither.

Three compounding failures:
1. Wrong table: session telemetry written as knowledge entries
2. OUTCOME_INDEX skipped: the cross-referencing index is never populated
3. VECTOR_MAP skipped: `embed_reconstruct::read_entries()` will attempt to re-embed these rows on future import rebuilds

## Affected Files and Functions

| File | Function | Role in Bug |
|------|----------|-------------|
| `crates/unimatrix-server/src/uds/listener.rs` | `write_auto_outcome_entry()` line 1880 | Writes directly to ENTRIES via `store.insert()`, bypasses entire pipeline |
| `crates/unimatrix-server/src/uds/listener.rs` | `process_session_close()` line ~1762 | Only caller of `write_auto_outcome_entry()` |
| `crates/unimatrix-store/src/write.rs` | `SqlxStore::insert()` line 18 | Target of incorrect write — only writes ENTRIES + tags + counters |
| `crates/unimatrix-store/src/write_ext.rs` | `insert_outcome_index_if_applicable()` line 577 | Function that should be called but isn't |
| `product/features/col-010/pseudocode/auto-outcomes.md` | §4 | Source of the false assumption that write.rs auto-populates OUTCOME_INDEX |

## Proposed Fix Approach

**Option B: Drop the ENTRIES write entirely.**

1. Delete `write_auto_outcome_entry()` from `listener.rs`
2. Remove the call site at line 1762 in `process_session_close()`
3. Add a cleanup migration: `UPDATE entries SET status = 2 WHERE source = 'hook' AND created_by = 'cortical-implant' AND topic LIKE 'session/%' AND category = 'outcome'` — then quarantine. The status counter must be decremented correspondingly; use `store.update_status()` per entry to keep COUNTERS consistent.

Option A (redirect through StoreService) still writes useless rows into ENTRIES. The content has no retrieval value — it would never surface a meaningful result. Session outcome data already lives in the SESSIONS table.

### Why This Fix

The session record (SESSIONS table, col-010) is the correct store for session telemetry. ENTRIES is for queryable knowledge with semantic content. The col-010 ARCHITECTURE.md §4 described writing to ENTRIES, but that design decision produces UUID-titled rows that are never meaningfully queryable.

## Risk Assessment

- **Blast radius**: `write_auto_outcome_entry()` has exactly one caller. Deleting it affects no other code path.
- **Regression risk**: Low. The function never worked correctly (OUTCOME_INDEX never populated; entries never retrievable by intended `context_lookup` path). No currently-working behavior degrades.
- **Status counter impact**: Each existing bogus row increments `active_count`. Cleanup must use `update_status()` not raw SQL to keep counters consistent.
- **Confidence**: High. Full call chain traced, false pseudocode assumption identified, gap between `store.insert()` and the full pipeline confirmed by reading `write.rs` and `write_ext.rs`.

## Missing Test

The col-010 test plan specified `test_auto_outcome_indexed_by_feature_cycle` but it was never implemented. The test that should have caught this:

```rust
// Integration test: tmpdir store + UDS listener
// 1. Register session, feature_cycle = "col-010-test", inject 1 entry
// 2. Call process_session_close(Success)
// 3. Sleep ~200ms for fire-and-forget task
// 4. Assert: ENTRIES row count unchanged (no new session/* rows)
// 5. Assert: outcome_index has no row for feature_cycle="col-010-test"
//    (documents current broken state; after fix, assert presence if
//     an alternative OUTCOME_INDEX write mechanism is added)
```

No Rust test exercises the full `process_session_close → write_auto_outcome_entry` path end-to-end.

## Reproduction Scenario

Deterministic: any session where `injection_count > 0` and outcome is `Success` or `Rework`. The bogus ENTRIES row is created on the next tokio task poll after `process_session_close()` returns. Identifiable by: `source = 'hook'`, `created_by = 'cortical-implant'`, `topic LIKE 'session/%'`, `category = 'outcome'`.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned UDS listener patterns (#3402, #3374, #838, #322); no prior entry covered this specific write-pipeline bypass pattern
- Stored: entry #3707 "store.insert() does not populate OUTCOME_INDEX — only insert_with_audit and StoreService do" via `/uni-store-lesson`
