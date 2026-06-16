# Component 5 — compaction_reread reckoning + compaction_events read accessor

**Crate**: `unimatrix-observe` (reckoning) + `unimatrix-store` (read accessor)
**Files**: `unimatrix-store` new read accessor (sibling to `write_ext.rs:195` `insert_compaction_event`); `unimatrix-observe/src/cycle_aggregates.rs` reckoning
**ADRs**: ADR-006 (#5048), ADR-005 (#5047) | **Risks**: R-05 (attribution), R-08 (clock/unit — Critical), R-11 boundary | **Wave**: 3

## Purpose

Compute `compaction_count` (COUNT of attributed `compaction_events` rows) and `compaction_reread_count` (within-cycle post-compaction overlap reads), gating on the **earliest** `compacted_at` per session with **binding seconds-normalization** of the read `ts`. crt-054 owns the table + writer; crt-055 adds the READ accessor only.

## Constraints honored

- **Seconds-normalization (Constraint 9, binding)**: gate is `(read_ts_millis ÷ 1000) > compacted_at_secs`. Normalize the read side only (floor `÷ 1000`); the boundary stays seconds (producer contract untouched).
- **Earliest boundary per session (ADR-006)**: gate on `MIN(compacted_at)`; each re-read counted at most once.
- **Declaration-chain attribution (R-05)**: only the cycle's DECLARED sessions' compaction rows count; undeclared/evicted sessions do not mis-attribute and do not fabricate a zero.
- `compaction_count` reports ALL boundaries even though the reread gate uses one (ADR-005/006).

## 5a. Read accessor (`unimatrix-store`, read_pool)

crt-054 supplies `insert_compaction_event` + the table + `idx_compaction_events_session`. crt-055 adds three reads (one accessor, parameterized, or three small accessors):

```
// MIN(compacted_at) per session — the gate boundary (ADR-006).
async fn min_compacted_at(&self, session_id: &str) -> Result<Option<i64>>:
    SELECT MIN(compacted_at) FROM compaction_events WHERE session_id = ?1
    // None when the session has no rows.

// All compacted_at for a session, ascending (matches ARCHITECTURE §6 read shape).
async fn compaction_boundaries(&self, session_id: &str) -> Result<Vec<i64>>:
    SELECT compacted_at FROM compaction_events WHERE session_id = ?1 ORDER BY compacted_at ASC

// Count of rows for a set of declared session ids (compaction_count source).
async fn compaction_count_for_sessions(&self, session_ids: &[String]) -> Result<i64>:
    if session_ids empty: return 0
    SELECT COUNT(*) FROM compaction_events WHERE session_id IN (<bound list>)
```

All `read_pool()`, parameterized binds (session_id is data — no injection surface; the sqlite_parity test already proves injection-safety). `compacted_at` is read verbatim as Unix seconds — NO conversion here.

## 5b. compaction_count reckoning (attributed)

```
fn reckon_compaction_count(store, declared_session_ids) -> i64:
    return store.compaction_count_for_sessions(declared_session_ids)
```

`declared_session_ids` = the cycle's sessions resolved via the session→`feature_cycle` declaration chain at review (the same set the handler already attributes for aggregation). Undeclared/evicted sessions (the #4140 silent-no-op condition) are NOT in this list → their compaction rows do not count (R-05). Zero rows → `compaction_count = 0` and `compaction_available = false` (Component 7), distinct from a measured zero.

## 5c. compaction_reread reckoning (the gate — Critical, R-08)

For each declared session: find its earliest boundary, then count within-session reads that re-read a pre-boundary file after the boundary, normalizing the read ts to seconds first.

```
fn reckon_compaction_reread(records: &[ObservationRecord], store, declared_session_ids) -> i64:
    total_reread = 0
    for sid in declared_session_ids:
        boundary_secs = store.min_compacted_at(sid)?    // Option<i64>
        if boundary_secs is None: continue              // no boundary → no gate → contributes 0
        // Partition this session's PostToolUse file reads by the seconds-normalized ts.
        prior_files: set<path> = {}                     // files read at-or-before the boundary
        reread_files: set<path> = {}                    // distinct files re-read after the boundary
        // process this session's records in ts order:
        for r in records where r.session_id == sid AND r.event_type == "PostToolUse":
            path = extract_file_path(r.tool, r.input)   // existing helper; skip if None
            read_ts_secs = r.ts / 1000                  // BINDING: epoch millis → seconds, integer floor
                                                        //   (ObservationRecord.ts: u64 millis; session_metrics.rs:115 convention)
            if read_ts_secs > boundary_secs:
                if path in prior_files: reread_files.insert(path)   // re-read after compaction → counts once
            else:
                prior_files.insert(path)                // read at-or-before boundary → prior context
        total_reread += reread_files.len()              // each re-read file counted once per session
    return total_reread as i64
```

Gate details:

**CANONICAL GATE (binding — do NOT weaken to `>=`, do NOT substitute rounding for floor):**

```
counts as a reread  IFF  (read.ts_millis ÷ 1000) > compacted_at        // integer FLOOR, STRICT >
```

`compacted_at` is `T` in Unix seconds; the read side is floored to whole seconds via integer division (`÷ 1000`) and compared with strict `>`. The boundary is NEVER touched (producer contract, seconds). The comparison is STRICT `>` — a read whose floored second equals `T` does NOT count.

**Worked example — THE canonical AC-22 case. Let `compacted_at = T` (Unix seconds). One distinct pre-boundary file is re-read three times after compaction at the following offsets:**

| Read offset | `ts_millis` | `ts_millis ÷ 1000` (floor) | `floor > T` ? | Counts? |
|-------------|-------------|----------------------------|---------------|---------|
| **exact boundary** | `T*1000`        | `T`   | `T > T` = false   | **NO** (strict `>`, not `>=`) |
| **−500ms before**  | `T*1000 − 500`  | `T−1` | `T−1 > T` = false | **NO** (floor-catching guard) |
| **+1s after**      | `T*1000 + 1000` | `T+1` | `T+1 > T` = true  | **YES** |

**Expected `compaction_reread_count = 1`** (only the +1s read clears the floored-strict gate; the file is counted once per session via the `reread_files` SET).

Why these three offsets (do not "fix" the example by making −500ms or exact-boundary count):
- The **+1s** case is the positive: a genuine post-compaction re-read floors to `T+1` and counts.
- The **−500ms** case is the **floor-catching guard** (R-08 / #4236 intent): it MUST NOT count. A gate that forgot to normalize and compared raw millis (`T*1000 − 500`) against seconds `T` would see `T*1000 − 500 > T` = true and wrongly count it — the `÷ 1000` floor is exactly what makes this NOT count. This is the case that fails if the floor is absent or the gate is unnormalized.
- The **exact-boundary** case pins STRICT `>` (not `>=`): a read AT second `T` is the compaction instant's own context, not a reread.

Note: SPECIFICATION AC-22, ACCEPTANCE-MAP AC-22, and the compaction_reckoning test plan are aligned to this same floor + strict-`>` semantics with **expected count = 1** (reconciled in this rework iteration).

The load-bearing assertion is that a **millis value entering unnormalized** (≈1000× larger) would make every read pass — and the `÷1000` floor prevents it; the −500ms-floors-to-`T−1` row is the case that catches an absent/broken floor.
- "counted once" (R-08): use a per-session `reread_files` SET so a file re-read multiple times after the boundary counts once; earliest-boundary-only avoids per-boundary double-counting across multiple compactions (ADR-006).
- `high_water` is NOT read (reserved, ADR-006).

## Data flow

- IN: cycle `ObservationRecord`s, store handle, declared session id list.
- OUT: `compaction_count: i64`, `compaction_reread_count: i64` on `CycleAggregates`.

## Error handling

- Read accessor Err → log + treat as no boundary (contributes 0) rather than aborting; Component 7 marks `compaction_available` from whether any boundary existed. Never fabricate a count on a read failure.
- Session with compaction but no post-boundary re-read → `compaction_count > 0`, `compaction_reread_count == 0` (a genuine measured zero, distinct from unavailable).

## Key test scenarios

- **AC-22 (must-have INTEGRATION test, R-08)**: seed `compaction_events.compacted_at = T` (seconds) + PostToolUse reads of ONE distinct pre-boundary file at three offsets — exact boundary (`T*1000` → floor `T` → NOT counted), `−500ms` (`T*1000−500` → floor `T−1` → NOT counted, the floor-catching guard), `+1s` (`T*1000+1000` → floor `T+1` → counted). **Assert `compaction_reread_count == 1`** (only the +1s read clears floor + strict-`>`). The `−500ms`-floors-to-`T−1` case is load-bearing: an unnormalized gate comparing raw millis (`T*1000−500`) against seconds `T` would wrongly count it ~1000× over — the `÷1000` floor prevents it (seconds-vs-seconds). Gate is STRICT `>` (exact-boundary read does NOT count); do NOT relax to `>=` and do NOT substitute rounding for floor.
- Multi-compaction session (N>1 rows) → `MIN(compacted_at)` is the gate; each re-read counted once; `compaction_count` reports all N (AC-12, R-08).
- Declared vs undeclared sessions seeded → only declared-session rows count toward `compaction_count`; evicted/undeclared session surfaces unavailable/honest-partial, never a fabricated complete-looking zero (AC-11, R-05, #4140).
- Session compacts but never re-reads → `compaction_count > 0`, `compaction_reread_count == 0`.
- Read accessor SQL-injection guard holds for a malicious `session_id` (parameterized).
