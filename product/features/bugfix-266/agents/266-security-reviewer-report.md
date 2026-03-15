# Security Review: 266-security-reviewer

## Risk Level: low

## Summary

The fix addresses GH #266 (background tick mutex starvation) by wrapping the supersession rebuild
`spawn_blocking` call in `tokio::time::timeout(TICK_TIMEOUT, ...)` and replacing a 4x
`query_by_status` loop with a single `query_all_entries` SQL SELECT. No user-supplied input
reaches any new code path. No new dependencies introduced. All findings are low-severity or
informational; none are blocking.

---

## Findings

### Finding 1 — SQL query is fully static; no injection risk
- **Severity**: informational (no risk)
- **Location**: `crates/unimatrix-store/src/read.rs:288`
- **Description**: `query_all_entries` uses `format!("SELECT {} FROM entries", ENTRY_COLUMNS)`
  where `ENTRY_COLUMNS` is a `const &str` defined at compile time. No parameters, no user input,
  no dynamic values. `query_map([], entry_from_row)` passes an empty parameter list. This is
  equivalent in safety to a hard-coded SQL literal.
- **Recommendation**: No action needed.
- **Blocking**: no

### Finding 2 — Abandoned blocking thread on timeout does not corrupt shared state
- **Severity**: low (by design, acceptable)
- **Location**: `crates/unimatrix-server/src/background.rs:348-376`
- **Description**: When `tokio::time::timeout` fires, the `JoinHandle` is dropped but the
  blocking thread spawned by `spawn_blocking` continues running in the thread pool until it
  completes (Tokio does not cancel blocking threads). The thread holds `MutexGuard<Connection>`
  for the duration of its DB read. If the timeout fires mid-query:
  - The `guard` write to `SupersessionState` is **not performed** (the `Ok(Ok(Ok(new_state)))`
    arm is never reached). This is the correct, safe path — the existing cached state is retained.
  - The `Arc<Store>` clone kept alive by the blocking closure extends the mutex hold past the
    timeout, which is the original problem the fix addresses. However this is bounded: the thread
    will eventually complete (SQLite SELECT with no mutation), release the mutex, and return — the
    thread-pool slot is reclaimed. The two-minute `TICK_TIMEOUT` is generous relative to a full
    table scan even under high contention.
  - No shared mutable state is written by two concurrent threads: `rebuild()` constructs a new
    `SupersessionState` on the stack and returns it; the `RwLock` write guard is only acquired
    inside the `Ok(Ok(Ok(...)))` match arm in the async context, which is only reachable when the
    timeout has NOT fired.
- **Recommendation**: No action required. The "abandoned thread holds mutex" scenario is the
  pre-existing bug this fix is designed to bound. The fix correctly prevents the server from
  waiting indefinitely and does not introduce a new race. Documenting this behavior in the code
  comment (which the PR already does) is sufficient.
- **Blocking**: no

### Finding 3 — Regression risk: all statuses included in SELECT
- **Severity**: low (informational — verified clean)
- **Location**: `crates/unimatrix-store/src/read.rs:288` and
  `crates/unimatrix-server/src/services/supersession.rs:89-97` (original loop on main)
- **Description**: The old loop iterated explicitly over `[Active, Deprecated, Proposed,
  Quarantined]` — all four Status variants. The new SELECT has no WHERE clause, which is
  semantically equivalent and will include any future Status variants added to the schema
  automatically. This is strictly more correct than the explicit list, not less. Verified by
  reading `query_by_status` (uses `WHERE status = ?1`) and confirming `ENTRY_COLUMNS` does not
  filter by status.
- **Recommendation**: No action required. The new approach is more future-proof than the original.
- **Blocking**: no

### Finding 4 — Partial result / mid-error behavior of query_all_entries
- **Severity**: low (safe failure mode)
- **Location**: `crates/unimatrix-store/src/read.rs:291-295`
- **Description**: `query_map` + `collect::<rusqlite::Result<Vec<_>>>()` means any single row
  deserialization error aborts the entire collect and returns `Err`. The result is propagated via
  `?` to the caller (`rebuild()`), which returns `Err(StoreError::Sqlite(_))`. The background tick
  catches this as `Ok(Ok(Err(e)))` and logs it without updating the guard — the existing cached
  state is retained. There is no partial-result window where the guard is updated with an
  incomplete entry set.
- **Recommendation**: No action required. Error handling is correct and conservative.
- **Blocking**: no

### Finding 5 — Doc comment in supersession.rs still refers to "all four entry statuses" after refactor
- **Severity**: informational (documentation drift)
- **Location**: `crates/unimatrix-server/src/services/supersession.rs:75`
- **Description**: The `rebuild()` doc comment header reads "Rebuild SupersessionState from the
  store by querying all four entry statuses." The implementation no longer queries by status at all
  — it calls `query_all_entries`. The comment is factually correct in outcome but misleading about
  mechanism. This is cosmetic and not a security concern.
- **Recommendation**: Update the doc comment to reflect the new implementation ("by fetching all
  entries in a single query"). Non-blocking.
- **Blocking**: no

### Finding 6 — No hardcoded secrets or credentials
- **Severity**: informational (no risk)
- **Location**: All changed files
- **Description**: Reviewed all diff hunks. No API keys, tokens, passwords, or connection strings
  present.
- **Blocking**: no

### Finding 7 — No new dependencies
- **Severity**: informational (no risk)
- **Location**: Cargo.toml files (unchanged in diff)
- **Description**: The fix uses only `tokio::time::timeout`, which is already a dependency via the
  `time` feature of `tokio`. No new crates introduced.
- **Blocking**: no

---

## Blast Radius Assessment

Worst case if the fix has a subtle regression:

1. **`query_all_entries` returns empty vec silently** — not possible given the implementation:
   an empty table returns `Ok(vec![])`, which the rebuild path accepts and stores as an empty
   `all_entries`. The search path would then fall back to `FALLBACK_PENALTY` (the `use_fallback`
   flag is set to `false` by `rebuild()`, but an empty `all_entries` means the supersession graph
   has no nodes, so penalty application is a no-op). Degraded but not incorrect behavior.

2. **Timeout fires every tick** — if DB is pathologically slow, every supersession rebuild times
   out. The server continues serving MCP requests using the stale (or cold-start empty) cached
   state. Supersession penalty is skipped or degraded. No crash, no data loss.

3. **Both the timeout AND a concurrent search hold `RwLock` simultaneously** — this is explicitly
   safe. `tokio::time::timeout` fires in the async context; the `RwLock` write guard is never
   acquired (the `Ok(Ok(Ok(...)))` arm is skipped). Concurrent readers hold the read lock
   normally. No deadlock possible from this change.

4. **`query_all_entries` is exposed as a public API and called by other code paths** — currently
   it has exactly one call site: `SupersessionState::rebuild`. Public visibility is appropriate
   for the crate architecture but could be tightened to `pub(crate)` for defense in depth. Not
   a security concern given all callers are internal Rust code with no user input reaching the
   function.

---

## Regression Risk

Low. The only behavioral change externally observable is:

- The background tick no longer hangs indefinitely when the supersession rebuild blocks. Existing
  server behavior under non-blocking conditions is unchanged: the state is rebuilt and the guard
  is updated identically.
- `SupersessionState::rebuild` now holds the DB mutex for the duration of one SELECT instead of
  four sequential acquires. This is a reduction in lock hold time and number, not an increase.
- All 2335+ existing tests pass. The `test_concurrent_search_stability` test specifically exercises
  the concurrent search path that was broken by the original bug.

---

## PR Comments

- Posted 1 comment on PR #269 (approval with findings).
- Blocking findings: no

---

## Knowledge Stewardship

- Stored: nothing novel to store — the "abandoned spawn_blocking thread holds sync mutex past
  timeout" pattern is a known Tokio characteristic, not a new discovery for this codebase. The
  documentation gap (doc comment drift in supersession.rs) is specific to this PR and does not
  warrant a generalizable lesson entry.
