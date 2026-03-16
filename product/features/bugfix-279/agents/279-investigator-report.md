# Bug Investigation Report: 279-investigator

## Bug Summary

The extraction tick in `background.rs` queries up to 10,000 observation rows in a single `spawn_blocking` call that holds `Mutex<Connection>` for the entire result set iteration. At high hook event volume, this blocks all concurrent MCP request handlers for potentially several seconds. A second `spawn_blocking` for `run_extraction_rules()` acquires the same mutex a second time within the same tick phase, adding a second contended window.

---

## Root Cause Analysis

The root cause is a magic literal `LIMIT 10000` with no named constant, combined with the architectural property that `lock_conn()` holds `Mutex<Connection>` for the full lifetime of the `MutexGuard` returned. The guard is held for the duration of the `stmt.prepare()` + `query_map()` + full row iteration loop — all inside one `spawn_blocking` closure. There is no intermediate mutex release between the SQL prepare and the last row being deserialized.

### Code Path Trace

```
extraction_tick()  [background.rs:859]
  └── tokio::task::spawn_blocking [background.rs:871]
        └── store_clone.lock_conn()  [background.rs:872]  ← mutex acquired
              └── conn.prepare("... LIMIT 10000")         [background.rs:873-879]
              └── stmt.query_map(...)                     [background.rs:883-906]
              └── for row in rows { ... }                 [background.rs:908-931] ← mutex held for all 10,000 rows
        └── MutexGuard dropped here (end of closure)     [background.rs:932]  ← mutex released
  └── [first spawn_blocking awaited at background.rs:934-935]

  └── tokio::task::spawn_blocking [background.rs:945]  ← second window
        └── run_extraction_rules(&obs_for_rules, &store_for_rules, &rules)  [background.rs:947]
              └── DeadKnowledgeRule::evaluate()
                    └── query_accessed_active_entries(store)  [dead_knowledge.rs:68]
                          └── store.lock_conn()              [dead_knowledge.rs:141]  ← mutex acquired
                          └── stmt.query_map(...)            [dead_knowledge.rs:148-158] ← held for full entries scan
                          └── MutexGuard dropped at end of function
  └── [second spawn_blocking awaited at background.rs:949-950]
```

### Why It Fails

`lock_conn()` returns a `MutexGuard<'_, Connection>` that holds the `Mutex<Connection>` for as long as the guard is live. In the first `spawn_blocking` closure, the guard is live from line 872 to the end of the closure at line 932. For 10,000 rows, the iteration loop deserialization is the bottleneck: each row involves 8 column reads, string allocations for `session_id`, `tool`, `input_str`, `snippet`, and a JSON parse (`serde_json::from_str`) for the `input` field. At active development session volume (continuous hook events), 10,000 rows can accumulate between 15-minute ticks, making this hold potentially 2-5 seconds depending on row size and I/O.

Every concurrent MCP handler that needs the store (`context_search`, `context_lookup`, `context_get`, `context_store`, etc.) blocks on `Mutex::lock()` during this window, creating the observable availability degradation.

The second `spawn_blocking` (Step 2: run extraction rules) acquires the same mutex inside `DeadKnowledgeRule::evaluate()` via `query_accessed_active_entries()`. This is a separate, independent hold after the first guard is dropped — but it adds a second contended window within the same tick cycle.

The watermark mechanism (`ctx.last_watermark`) is correct and safe: it is updated to `new_watermark` (the maximum `id` seen in the batch) at line 1111. If fewer rows are returned due to a smaller LIMIT, the remaining rows have higher IDs and will be picked up on the next tick with `WHERE id > ?1` starting from the new watermark. Reducing the LIMIT does not cause data loss or missed observations.

---

## Affected Files and Functions

| File | Function | Role in Bug |
|------|----------|-------------|
| `crates/unimatrix-server/src/background.rs` | `extraction_tick()` | Contains the `LIMIT 10000` query and the first mutex hold |
| `crates/unimatrix-server/src/background.rs` | `extraction_tick()` (Step 2) | Second `spawn_blocking` invoking `run_extraction_rules` which re-acquires the mutex |
| `crates/unimatrix-observe/src/extraction/dead_knowledge.rs` | `query_accessed_active_entries()` | Acquires `lock_conn()` inside the second spawn_blocking during rule evaluation |
| `crates/unimatrix-store/src/db.rs` | `Store::lock_conn()` | Returns a `MutexGuard` that is held for the entire closure lifetime |

---

## Proposed Fix Approach

FIX-3 from ASS-020 is confirmed correct. The fix is:

1. Add a named constant at the top of `background.rs` (alongside `DEFAULT_TICK_INTERVAL_SECS` and `AUTO_QUARANTINE_CYCLES_MAX`):
   ```rust
   /// Maximum observations loaded per extraction tick.
   /// The watermark advances by exactly this many rows; any remainder is
   /// processed on the next tick. Smaller values reduce mutex hold time.
   const EXTRACTION_BATCH_SIZE: i64 = 500;
   ```

2. Replace the literal `LIMIT 10000` in the SQL string at `background.rs:875` with the named constant. Since `rusqlite::params!` is used for bind parameters and the LIMIT value must be a constant (not a bind parameter in this existing pattern), the constant must be interpolated into the query string at compile time. Use `format!` or a `const` string concatenation:
   ```rust
   let sql = format!(
       "SELECT id, ts_millis, hook, session_id, tool, input, response_size, response_snippet \
        FROM observations WHERE id > ?1 ORDER BY id ASC LIMIT {}",
       EXTRACTION_BATCH_SIZE
   );
   let mut stmt = conn.prepare(&sql)...
   ```
   Alternatively, use a bind parameter for the LIMIT (rusqlite supports `LIMIT ?2`) and pass `EXTRACTION_BATCH_SIZE` via `rusqlite::params![watermark as i64, EXTRACTION_BATCH_SIZE]`. This is the cleaner approach and avoids format-string SQL construction.

3. No changes required to Step 2 (`run_extraction_rules`): the second mutex acquisition is a separate, short-lived hold for a targeted index query (`WHERE status = ?1 AND access_count > 0` on `entries`). With a smaller observation batch, the `dead_knowledge` rule will have fewer sessions to analyze, making its store query faster as a side effect.

### Why This Fix

The watermark pagination is already correct and designed to handle batched processing. The 10,000 limit was never a correctness requirement — it was an implicit capacity ceiling. Reducing it to 500 bounds the mutex hold time to a duration proportional to 500 rows (estimated 50-200ms at worst vs. 2-5s for 10,000 rows). The constant makes the bound visible in code review and testable.

---

## Risk Assessment

- **Blast radius**: Only `extraction_tick()` in `background.rs` is changed. No store layer changes. No changes to extraction rules. The watermark update path (`ctx.last_watermark = new_watermark` at line 1111) is unaffected.

- **Regression risk (behavioral)**: Low. The only behavioral change is that if more than 500 observations accumulate between ticks, the excess is deferred to the next tick. Extraction rules that require a minimum number of sessions (e.g., `DeadKnowledgeRule` requires 5 sessions, `RecurringFrictionRule` needs repeated occurrences) may undercount signal if a logical cluster of related observations straddles a batch boundary. This is an acceptable eventual-consistency trade-off given the 15-minute tick interval. In practice, active development sessions produce dozens to a few hundred observations per tick, well within a 500-row batch.

- **Regression risk (correctness)**: None. Watermark advancement is based on `max_id` of returned rows, so no row is skipped or double-processed across ticks.

- **Regression risk (performance)**: Positive. Shorter mutex holds reduce contention. More ticks may be needed to process a backlog, but this is strictly better for MCP request latency.

- **Confidence**: High. The code path is clear, the watermark mechanism is verified correct, and the fix is a one-line change plus a constant declaration.

---

## Missing Test

**What test should have caught this?**

An integration test for `extraction_tick()` that verifies the observation query respects a configurable batch size:

```
Test scenario: "extraction_tick respects EXTRACTION_BATCH_SIZE and advances watermark correctly"

Setup:
- Insert 1,200 observations into the store (IDs 1–1200)
- Set ctx.last_watermark = 0

Run:
- Call extraction_tick() once

Assert:
- Exactly EXTRACTION_BATCH_SIZE (500) ObservationRecords were processed (can be verified by tracking
  how many records the rules received, or by checking ctx.last_watermark == 500)
- ctx.last_watermark == 500 (not 0, not 1200)

Run again:
- Call extraction_tick() a second time with same store

Assert:
- ctx.last_watermark == 1000 (next 500 rows consumed)
- No observation with id <= 500 was re-processed

Run a third time:
Assert:
- ctx.last_watermark == 1200 (remaining 200 rows consumed)
```

This test would have caught both the absence of the constant (making the limit visible) and any regression in watermark advancement logic when reducing the batch size.

Additionally, a unit test asserting that the SQL string used in `extraction_tick` contains `EXTRACTION_BATCH_SIZE` (not the literal 10000) would enforce the constant is used at the call site — though this is more of a lint/audit than a behavioral test.

---

## Reproduction Scenario

The contention is deterministic under the following condition:

1. Run a long Claude session with many tool calls (generates high hook event volume — hundreds of observations per 15-minute period).
2. Wait for the background tick to fire (15-minute interval, or set `UNIMATRIX_TICK_INTERVAL_SECS=60` to trigger faster).
3. Issue an MCP request (`context_search`, `context_lookup`, etc.) within the same window as the extraction tick.
4. Observable symptom: MCP request latency spikes to 2-5+ seconds (normally <100ms) during the extraction tick's first spawn_blocking window.

At lower observation volumes (<500 per tick interval), the bug exists but is not noticeable (hold time proportional to row count).

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` via `context_search` for "spawn_blocking mutex hold extraction tick batch size contention" — found #735 (spawn_blocking pool saturation from unbatched writes), #731 (batched fire-and-forget pattern), #1367 (spawn_blocking_with_timeout for MCP handlers), #770 (non-reentrant mutex deadlock). None directly covered the extraction tick batch size pattern specifically.
- Queried: `/uni-knowledge-lookup` for entries #735, #1367, #1688 — confirmed these are the relevant prior lessons. #1688 covers the "apply timeout to ALL handlers at introduction time" lesson from bugfix-277.
- Stored: entry #1736 "Extraction tick batch size controls mutex hold duration: EXTRACTION_BATCH_SIZE constant pattern" via `/uni-store-lesson` — covers the generalizable pattern that named batch size constants must be extracted for any spawn_blocking loop that iterates a variable-length result set from a mutex-protected connection. Tagged `caused_by_feature:col-013`.
