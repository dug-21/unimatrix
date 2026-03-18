# Gate 3b Security Review: nxs-011 (sqlx migration)

**Date**: 2026-03-18
**Reviewer**: uni-security-reviewer (fresh context, cold read)
**Scope**: `crates/unimatrix-store/`, `crates/unimatrix-server/src/uds/listener.rs`, `crates/unimatrix-core/`

---

## Re-Review Addendum: 2026-03-18

**Verdict updated to: PASS**

Both blocking findings (F-01, F-02) verified as correctly resolved. See addendum section below for verification details.

---

## Overall Verdict: ~~REWORKABLE FAIL~~ PASS

Both Medium blocking findings are resolved. Remaining findings (F-03 through F-06) are Low/Info and non-blocking. Branch is clear for merge.

---

## Findings Table

| # | Severity | Location | Title | Blocking |
|---|----------|----------|-------|----------|
| F-01 | Medium | `migration.rs:581,599` | Format-interpolated table names in SQL DDL | Yes |
| F-02 | Medium | `listener.rs:2316,2343,2348` | Manual BEGIN/COMMIT/ROLLBACK via raw SQL in `spawn_blocking` context bypasses sqlx pool transaction semantics | Yes |
| F-03 | Low | `write_ext.rs:88` | Timestamp interpolated directly into SQL SET clause (not injected, but inconsistent pattern) | No |
| F-04 | Low | `db.rs:234` | `write_pool_server()` exposes the write pool broadly to the server layer — no access control | No |
| F-05 | Info | `pool_config.rs:118-120` | Zero-duration acquire timeouts silently allowed — can cause immediate pool exhaustion failures in tests | No |
| F-06 | Info | `analytics.rs:308` | Analytics batch failure discards entire batch silently — no dead-letter queue | No |

---

## Detailed Findings

### F-01: Format-interpolated table names in SQL DDL (Medium — Blocking)

**Location**: `crates/unimatrix-store/src/migration.rs`, lines 581 and 599

**Code**:
```rust
// Line 581
sqlx::query(&format!("DROP TABLE IF EXISTS {table}"))

// Line 599
sqlx::query(&format!("ALTER TABLE {old} RENAME TO {new}"))
```

**Description**: The table names `table`, `old`, and `new` are hard-coded `&str` literals in adjacent array literals (lines 567-580 and 590-598 respectively), so they do not originate from external input. This means there is no actual SQL injection vector at runtime.

However, this is a violation of the codebase's own parameterized-query convention and creates a dangerous pattern: if a future developer extends these loops (e.g., by adding a user-supplied table name from a schema migration config), the injection risk becomes real. SQLite DDL statements (`DROP TABLE`, `ALTER TABLE ... RENAME`) cannot use parameterized values for identifiers — there is no `:table_name` bind in DDL — so the correct mitigation is an explicit allowlist check before the format, not a bind.

The rule in this review: all SQL containing external data must use `.bind()`. DDL operating on identifier strings must verify against a compile-time allowlist before interpolation.

**Risk if shipped as-is**: Low (data is hardcoded), but the pattern is flagged because it trains reviewers to accept string interpolation in SQL contexts, and because migration code paths are visited at database upgrade time — a sensitive, unrepeatable operation.

**Recommendation**: Add an explicit allowlist assert before both loops, or (preferred) use a `match`/`if` guard pattern rather than dynamic format interpolation for DDL identifiers. Example:

```rust
// Acceptable guard pattern:
const ALLOWED_DROP: &[&str] = &["entries", "co_access", ...];
assert!(ALLOWED_DROP.contains(&table), "unexpected table name in migration DDL: {table}");
sqlx::query(&format!("DROP TABLE IF EXISTS {table}")).execute(...);
```

---

### F-02: Manual BEGIN/COMMIT/ROLLBACK bypasses sqlx pool transaction semantics (Medium — Blocking)

**Location**: `crates/unimatrix-server/src/uds/listener.rs`, function `insert_observations_batch`, lines 2316, 2343, 2348

**Code**:
```rust
fn insert_observations_batch(store: &Store, batch: &[ObservationRow]) -> ... {
    let pool = store.write_pool_server();
    let handle = tokio::runtime::Handle::current();
    handle.block_on(sqlx::query("BEGIN").execute(pool))...
    // loop: individual INSERTs against pool (may use DIFFERENT connections)
    handle.block_on(sqlx::query("COMMIT").execute(pool))...
    // on err:
    handle.block_on(sqlx::query("ROLLBACK").execute(pool))...
}
```

**Description**: `write_pool_server()` returns `&SqlitePool`. Calling `.execute(pool)` on successive raw `BEGIN`/INSERT/`COMMIT` statements acquires a pool connection for each call independently. SQLite connection pools do not guarantee the same connection across sequential `execute()` calls. If `write_max_connections = 2`, a second writer could be holding the WAL lock when `BEGIN` runs, and the subsequent INSERT could be dispatched to a different connection (one that does not have an active transaction).

The correct pattern for transactional work against a sqlx pool is `pool.begin().await` which returns a `Transaction<'_, Sqlite>` — a RAII guard that holds one connection for the entire transaction lifetime.

The `insert_observation` (singular) function does not use manual transaction management and is safe. The batch variant is the problem.

**Risk**: In production with `write_max_connections=2`, a race between the observation batch writer and the analytics drain task could produce:
1. `BEGIN` runs on connection A
2. An analytics drain task acquires connection B
3. The subsequent INSERT in the batch runs against connection B (no open transaction on B)
4. `COMMIT` on connection A commits an empty transaction; the INSERT data is either auto-committed or lost, depending on SQLite autocommit state

This is a correctness issue (silently lost observation batch records) and a protocol violation (raw COMMIT/ROLLBACK against a pooled connection can corrupt pool state in sqlx's connection lifecycle).

**Recommendation**: Replace the manual BEGIN/COMMIT/ROLLBACK with `pool.begin().await?` transaction semantics, mirroring the pattern used throughout `analytics.rs::commit_batch()` and `sessions.rs::gc_sessions()`. Since this function is called from `spawn_blocking`, bridge using `Handle::current().block_on(pool.begin())`.

---

### F-03: Timestamp interpolated directly into SQL SET clause (Low — Not Blocking)

**Location**: `crates/unimatrix-store/src/write_ext.rs`, line 88

**Code**:
```rust
let mut sets: Vec<String> = vec![format!("last_accessed_at = {}", now)];
let sql = format!("UPDATE entries SET {} WHERE id = ?1", sets.join(", "));
```

Where `now` is `u64` from `current_unix_timestamp_secs()`.

**Description**: `now` is an internally generated `u64` timestamp, not external input. The integer cannot contain SQL metacharacters. There is no injection vector here.

However, the pattern is inconsistent with the rest of the codebase where all values (including timestamps) are passed via `.bind()`. The correct approach would be `.bind(now as i64)` as an additional parameter or to refactor the entire SET clause construction to use a fixed-form query.

**Risk**: None for SQL injection specifically. The risk is developer confusion: a future contributor may copy this pattern and apply it to a field that does accept external input.

**Recommendation**: Change to a bind parameter. The `now` value can be passed as `?N` with `param_idx` and bound conventionally.

---

### F-04: `write_pool_server()` has no access control (Low — Not Blocking)

**Location**: `crates/unimatrix-store/src/db.rs`, line 234

**Code**:
```rust
pub fn write_pool_server(&self) -> &SqlitePool {
    &self.write_pool
}
```

**Description**: This accessor is `pub` (not `pub(crate)`) and is documented as the escape hatch for the server layer. It is used in 40+ call sites across the server crate. While this is intentional by design (noted in the docstring: "callers MUST NOT use this for analytics-path writes"), there is no enforcement mechanism.

A caller that accidentally routes an analytics write through `write_pool_server()` instead of `enqueue_analytics()` would bypass the shed counter, the bounded queue backpressure, and the batched commit logic. Under sustained write load, this could exhaust the write pool (max 2 connections), blocking integrity writes.

**Risk**: Misuse blast radius — analytics writes incorrectly routed through `write_pool_server()` compete directly with integrity table writes for the 2 write pool slots. Given that the pool cap is the primary SQLite WAL concurrency guard (AC-09), pool saturation caused by misdirected analytics writes would cause `PoolTimeout` errors on integrity paths (INSERT entry, UPDATE status, etc.).

**Recommendation**: The "MUST NOT" comment is insufficient enforcement for a widely-used public API. Consider renaming to `write_pool_integrity_bypass()` to signal intent, or adding a doc-level `# Panics` note in debug builds when called from analytics-path callsites. Long-term: track callers via a capability enum at the type level.

---

### F-05: Zero-duration acquire timeout silently allowed (Info)

**Location**: `crates/unimatrix-store/src/pool_config.rs`, lines 118-120

**Code**:
```rust
// Zero-duration timeouts are technically valid (immediate fail on any
// saturation). Allowed — tests may use them for controlled failure scenarios.
Ok(())
```

**Description**: This is documented behavior with a stated rationale. The concern is that a misconfigured production deployment passing `Duration::ZERO` for timeouts would produce opaque `PoolTimeout` errors on the first pool saturation event. There is no warning log when `validate()` accepts a zero-duration timeout.

**Recommendation**: Emit a `tracing::warn!` in `validate()` when either timeout is zero, so zero-duration deployments are visible in logs.

---

### F-06: Silent analytics batch discard with no dead-letter mechanism (Info)

**Location**: `crates/unimatrix-store/src/analytics.rs`, function `commit_batch`, line 307

**Code**:
```rust
// On failure: logs at `ERROR` level and discards the batch. Analytics loss is
// acceptable (FR-05). Does NOT retry — retrying risks double-writes.
```

**Description**: This is an explicitly documented design decision (FR-05). Analytics durability is intentionally traded for availability. The concern is that under SQLite WAL write contention (both drain task and `write_pool_server()` callers competing), batch failures could be more frequent than expected given the write_max=2 constraint.

**Risk**: Analytics gaps (co-access, query logs, session records) degrade the confidence evolution pipeline and retrospective detection. In normal operation this is low risk. Under pool saturation — which becomes more likely given F-04 — it becomes a compounding failure mode.

**Recommendation**: No change required. Informational observation only.

---

## Blast Radius Assessment

**If F-02 is not fixed**: Under `write_max_connections=2`, the manual transaction pattern in `insert_observations_batch` can silently drop observation batch writes when the drain task holds a write pool connection during `BEGIN`/INSERT dispatch. The failure mode is silent data loss in the `observations` table — not a crash, not an error returned to the caller, not a logged error. This is non-obvious and hard to reproduce in tests.

**Worst case for the write_pool=2 constraint being bypassed**: If `PoolConfig::validate()` were not called (or if `write_pool_server()` callers contend heavily), both write pool slots are consumed by analytics reads and no slots remain for integrity writes. This blocks `INSERT entry`, `UPDATE status`, and `audit_log` writes with `PoolTimeout` errors. The MCP server returns `StoreError::PoolTimeout` to callers — visible, but operationally severe.

**Migration path risk**: The format-string interpolation in `migration.rs` runs during database upgrade. A migration failure leaves the database in a partially migrated state. The existing backup mechanism (`create_backup_file`) mitigates data loss but not the operational impact of a failed migration on a production instance. This is low risk given hardcoded values, but high impact if triggered.

---

## Regression Risk

The primary regression risk from this migration is write-pool exhaustion in scenarios not covered by `PoolConfig::test_default()` (which uses `write_max=1`). Specifically:

1. The drain task holds one write pool slot continuously during high-throughput analytics.
2. `write_pool_server()` callers (40+ sites) compete for the remaining slot.
3. Under concurrent MCP requests + observation batch writes + drain task flush, `write_max=2` may be insufficient for throughput without appropriate `write_acquire_timeout` tuning.

The existing tests using `test_default()` (write_max=1) do not exercise the two-slot contention scenario.

---

## Required Fixes Before Merge

1. **F-02** (Medium): Replace manual `BEGIN`/`COMMIT`/`ROLLBACK` in `insert_observations_batch` with `pool.begin().await` sqlx transaction API.
2. **F-01** (Medium): Add explicit allowlist guards before DDL format-interpolation loops in `migration.rs`. The guard is a one-line assert per loop.

Both fixes are targeted, self-contained, and do not require design changes.

---

## PR Comments

No PR exists for this branch at review time. Comments will be posted when a PR is opened. The two blocking findings (F-01, F-02) should be resolved before PR creation or addressed in the PR description as explicit acceptance decisions.

---

## Knowledge Stewardship

Nothing novel to store — the `spawn_blocking` + manual transaction anti-pattern is a known sqlx migration hazard (async pool bridged into sync context without preserving connection identity). This is specific to this PR's implementation choice rather than a recurring codebase-wide pattern.

---

## Re-Review Addendum: F-01 and F-02 Verification

**Reviewer**: uni-security-reviewer (fresh context re-read)
**Trigger**: Blocking fixes applied after initial REWORKABLE FAIL verdict.

### F-01 Verification — RESOLVED

Examined `migration.rs` lines 566–605.

Step 10 (DROP TABLE loop, lines 567–587) now uses an array of inline string literals:
```
"DROP TABLE IF EXISTS entries",
"DROP TABLE IF EXISTS topic_index",
... (12 entries)
```
No `format!()` call. No variable interpolation. `sqlx::query(sql)` receives a fully static string in each iteration.

Step 11 (ALTER TABLE RENAME loop, lines 590–605) likewise uses inline string literals:
```
"ALTER TABLE entries_v6 RENAME TO entries",
... (7 entries)
```
No `format!()` call. No variable interpolation.

The comment on line 566 ("inline literals — no format! interpolation") is accurate and present in both loop headers. F-01 is fully resolved.

### F-02 Verification — RESOLVED

Examined `listener.rs` lines 2308–2343 (`insert_observations_batch`).

The function now uses a single `handle.block_on(async { ... })` block containing:
1. `pool.begin().await` — acquires one connection and opens a `Transaction<'_, Sqlite>` RAII guard (line 2318–2321).
2. All INSERT statements execute against `&mut *txn` (line 2335) — the same connection for all rows in the batch.
3. `txn.commit().await` on line 2339 — sqlx RAII commit.
4. No `ROLLBACK` statement: rollback is implicit via `Transaction::drop()` if the block returns early via `?`.

There are no raw `BEGIN`, `COMMIT`, or `ROLLBACK` strings anywhere in the function. The pattern correctly mirrors `analytics.rs::commit_batch()` as recommended.

F-02 is fully resolved. The fix is correct and complete.
