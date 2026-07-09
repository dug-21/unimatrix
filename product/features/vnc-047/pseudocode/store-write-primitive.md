# C2 — Store write primitive `insert_cycle_start_with_tags`

**File:** `crates/unimatrix-store/src/db.rs` (NEW method; `insert_cycle_event` UNCHANGED)
**ADR:** ADR-002 (+ ADR-003 durability envelope, ADR-007 trace). **Risks:** R-05, R-15, R-08, R-11, R-06.
**AC:** AC-02, AC-02a, AC-01, AC-07. **Security:** parameterized binds are the ONLY SQLi defense (load-bearing).

## Purpose

Persist a `cycle_start` event row **and** the whole submitted tag set in ONE atomic transaction,
enforcing WHOLE-SET-ONCE via a row-existence guard that is race-safe under concurrent same-cycle
starts. This is the single persistence route for cycle tags (SR-03). `insert_cycle_event` (db.rs:320,
15 call sites) is left completely untouched.

## Signature (fixed by ARCHITECTURE Integration Surface — do not change)

```rust
#[allow(clippy::too_many_arguments)]
pub async fn insert_cycle_start_with_tags(
    &self,
    cycle_id: &str,          // == feature_cycle
    seq: i64,
    phase: Option<&str>,
    outcome: Option<&str>,
    next_phase: Option<&str>,
    timestamp: i64,
    goal: Option<&str>,      // col-025: rides the same start row (may be None)
    tags: &[String],         // non-empty-filtered by the caller (C4); still defensively skip empties
) -> Result<()>;
```

### HEAD verification — `insert_cycle_event` parity (Stage 3a reconciliation)

Verified directly at HEAD (`crates/unimatrix-store/src/db.rs:320`, single definition — grep-confirmed):

```rust
// ACTUAL HEAD signature — 8 args, WITH next_phase, NO goal_embedding:
pub async fn insert_cycle_event(
    &self, cycle_id: &str, seq: i64, event_type: &str,
    phase: Option<&str>, outcome: Option<&str>, next_phase: Option<&str>,
    timestamp: i64, goal: Option<&str>,
) -> Result<()>;
// INSERT columns (db.rs:338): (cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal)
```

A Stage 3a tester report claimed the signature was `(cycle_id, seq, event_type, phase, outcome,
goal, timestamp, goal_embedding)` with NO `next_phase`. **That claim is incorrect against HEAD** —
`next_phase` IS present and `goal_embedding` is NOT written by `insert_cycle_event`. The
`insert_cycle_start_with_tags` signature above (which carries `next_phase` and NO `goal_embedding`
arg) is therefore correct as written; the cycle_start INSERT below matches HEAD's 8 columns exactly.

**`goal_embedding` handling (do NOT add it to this INSERT).** `cycle_events.goal_embedding` is a
nullable BLOB that `insert_cycle_event` leaves NULL at insert time. It is populated LATER by a
separate fire-and-forget UPDATE — `update_cycle_start_goal_embedding(cycle_id, bytes)` (db.rs:438,
`UPDATE cycle_events SET goal_embedding=?1 WHERE cycle_id=?2 AND event_type='cycle_start'`) — spawned
in listener Step 6 (listener.rs:3081) for every `Start`. To keep the cycle_start row **byte-identical**
to the plain path, `insert_cycle_start_with_tags` must likewise NOT write `goal_embedding`: it inserts
the same 8 columns (goal_embedding stays NULL), and the EXISTING Step-6 UPDATE finds and populates the
row this method inserted (it keys on `event_type='cycle_start'`). No change to Step 6 is needed (C5).

## Transaction mechanism — BEGIN IMMEDIATE, NOT `pool.begin()` (R-15, load-bearing)

`sqlx`'s `write_pool.begin()` opens a **DEFERRED** transaction — the write lock is not taken until
the first write, so two concurrent same-`feature_cycle` starts can both pass the EXISTS guard and
both write, merging the set. Use the **dedicated-connection + `BEGIN IMMEDIATE`** pattern already
proven at `unimatrix-server/src/import/mod.rs:196-197`: acquire ONE connection, issue
`BEGIN IMMEDIATE` (takes the write lock up front), run every statement on that SAME connection, then
`COMMIT` (or `ROLLBACK` on error). Using the pool for the inner statements would dispatch them to a
different connection that cannot see the open txn (SQLITE_BUSY, code 5).

## Pseudocode body

```
FUNCTION insert_cycle_start_with_tags(cycle_id, seq, phase, outcome, next_phase, timestamp, goal, tags):

    # --- open a race-safe write transaction on a single dedicated connection ---
    conn = write_pool.acquire()                 map_err → StoreError::Database
    execute(conn, "BEGIN IMMEDIATE")            map_err → StoreError::Database
        # write lock is now held for the whole txn; the EXISTS guard below is TOCTOU-safe

    # helper: on ANY error below, best-effort ROLLBACK then return the error
    #   (parity import/mod.rs:203 — `let _ = execute(conn, "ROLLBACK")`)

    # (a) cycle_start event row — byte-identical INSERT to insert_cycle_event, event_type fixed
    result = execute(conn,
        "INSERT INTO cycle_events
            (cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal)
         VALUES (?1, ?2, 'cycle_start', ?3, ?4, ?5, ?6, ?7)",
        binds: [cycle_id, seq, phase, outcome, next_phase, timestamp, goal])
    on Err(e): rollback; return Err(StoreError::Database(e))

    # (b) WHOLE-SET-ONCE guard — existence only, never reads tag VALUES (value-opacity)
    exists: bool = query_scalar(conn,
        "SELECT EXISTS(SELECT 1 FROM cycle_tags WHERE feature_cycle = ?1)",
        binds: [cycle_id])                       # returns 0/1
    on Err(e): rollback; return Err(StoreError::Database(e))

    wrote_set: bool
    if NOT exists:
        # first tag-bearing start for this feature_cycle → freeze the whole submitted set
        for tag in tags:
            if tag.is_empty(): continue          # defensive; C4 already filtered whitespace-only
            execute(conn,
                "INSERT INTO cycle_tags (feature_cycle, tag) VALUES (?1, ?2)
                 ON CONFLICT(feature_cycle, tag) DO NOTHING",   # dup WITHIN one set → no abort
                binds: [cycle_id, tag])          # PARAMETERIZED — no interpolation (SQLi defense)
            on Err(e): rollback; return Err(StoreError::Database(e))
        wrote_set = true
    else:
        # set already frozen → skip the ENTIRE tag write (no merge, no accumulate, no per-row logic)
        wrote_set = false

    execute(conn, "COMMIT")                       map_err → StoreError::Database (best-effort rollback first)

    # (C13) best-effort freeze-outcome trace — see freeze-trace.md; NON-GATING
    if wrote_set:
        tracing::info!(feature_cycle = cycle_id, n = tags.len(),
            "cycle_tags: recorded N labels for feature_cycle")
    else:
        tracing::info!(feature_cycle = cycle_id, n = tags.len(),
            "cycle_tags: set already frozen for feature_cycle, N submitted labels ignored")

    return Ok(())
```

### Why each piece

- **The cycle_start row is inserted on EVERY start** (parity with goal, seq++). Only the *tag* write
  is frozen. A later start still appends its own `cycle_start` event row via this method (or via
  `insert_cycle_event` when it carries no tags — routed by C5).
- **The freeze unit is the whole set**, decided by `EXISTS(rows for feature_cycle)` — never by
  inspecting tag values or `ns:` prefixes (value-opacity, ADR-002). Per-key/per-namespace write-once
  is REJECTED.
- **`ON CONFLICT(feature_cycle, tag) DO NOTHING`** handles a *duplicate tag within one submitted set*
  gracefully (no txn abort, R-05 scenario 3). It is NOT the whole-set freeze — that's the EXISTS
  guard. Both coexist: EXISTS gates the set; ON CONFLICT dedups within it.
- **Guard + start row + tag rows are ONE `BEGIN IMMEDIATE` txn** → atomic (R-05) and serialized
  against concurrent starts (R-15). A mid-op failure rolls back the whole unit — never a start row
  without tags or vice versa.
- **No `LIKE`/`like_escape`** anywhere here (no namespace query ships; R-08 metacharacter surface is
  absent by design). If a future reviewer adds prefix querying, `like_escape` becomes mandatory —
  flag to the deferred mutation home (C11 / ADR-006).
- **No length/count cap** on tags (value-opacity; DoS accepted under the Write gate, vnc-045 SD-8).
- **`goal_embedding` is NOT written here** — the INSERT lists exactly the 8 columns
  `insert_cycle_event` writes (goal_embedding stays NULL); the existing listener Step-6 UPDATE
  populates it afterward on the row this method inserted. Adding it to this INSERT would DIVERGE from
  the plain cycle_start path (see HEAD verification above).

## Error handling (fire-and-forget contract, ADR-003)

- Any DB error → `StoreError::Database`, after a best-effort `ROLLBACK`. The method's `Result` is
  consumed by C5's spawn, which only `tracing::warn`s on `Err` — no caller-visible signal.
- Returns `Ok(())` on both wrote-set and frozen-skip; the distinction surfaces ONLY as the C13 trace
  line (frozen-skip is NOT caller-returnable — ADR-007).

## Key test scenarios (hints)

1. **Atomicity (R-05):** after one call, both the `cycle_start` row and all `cycle_tags` rows are
   visible at the same commit boundary. Fault-injection: a failing tag insert rolls back the start row.
2. **Whole-set-once exact equality (R-08, AC-02a):** start `{A,B}` then re-start `{C}` → stored set
   EXACTLY `{A,B}`; start `{A}` then `{B}` → EXACTLY `{A}`; subset/superset both no-op wholesale; no error.
3. **Tagless-does-not-lock:** (exercised via C5 routing) a tagless start writes NO `cycle_tags` rows;
   a later `{A}` start still locks `{A}`.
4. **BEGIN IMMEDIATE verified (R-15):** code/SQL review confirms `BEGIN IMMEDIATE` on a dedicated
   connection, not `pool.begin()`. Concurrency test: two same-FC starts `{A,B}` and `{C,D}` fired
   concurrently → stored set is EXACTLY one intact whole set, never a merge `{A,B,C,D}`; loser errors/panics nowhere.
5. **Duplicate-within-set (R-05.3):** one call with `["A","A","B"]` → rows `{A,B}`, no abort.
6. **Opacity (R-11, AC-01/07):** `["workflow:v1.3", "foo"]` stored verbatim; colon-prefixed and bare
   stored identically (no prefix branching). Empty string skipped, others stored.
7. **Assembled-path coverage of the write is AC-02 (see C5) — a store-only test cannot satisfy AC-02.**
