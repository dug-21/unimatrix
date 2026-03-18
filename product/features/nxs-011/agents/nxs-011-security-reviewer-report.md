# Security Review: nxs-011-security-reviewer

**Feature**: nxs-011 — rusqlite → sqlx 0.8 dual-pool migration
**PR**: #299
**Branch**: feature/nxs-011
**Reviewer**: nxs-011-security-reviewer (fresh context, no prior pipeline knowledge)
**Date**: 2026-03-18

---

## Risk Level: medium

---

## Summary

The nxs-011 branch replaces `rusqlite 0.34` + `Mutex<Connection>` with a `sqlx 0.8` dual-pool architecture. No new external trust surface is introduced; all inputs continue to originate from MCP tool callers already in the threat model. The SQL execution path is parameterized throughout — no injection vectors were found. The primary findings are pool routing misuse (read operations going through the write pool) and an undocumented write pool connection-holding pattern during export, both of which degrade the connection pool availability guarantees that the architecture explicitly treats as a security boundary (AC-09, write_pool cap of 2). A secondary observation concerns a `block_on` pattern in a `spawn_blocking` context — which is documented as intentional and technically safe, but sits one refactor away from a panic path if the calling context changes.

No blocking security findings. No hardcoded secrets. No deserialization of untrusted data from new paths.

---

## Findings

### Finding 1: Read operations routed through write pool in production code paths

- **Severity**: medium
- **Location**:
  - `crates/unimatrix-server/src/background.rs:940` — `fetch_observation_batch()` uses `write_pool_server()` for a SELECT on `observations`
  - `crates/unimatrix-server/src/services/observation.rs:7` — entire file comment: "All store access uses async sqlx via `write_pool_server()`"; all SELECT queries (lines 35, 91, 142, 159, 228) use write pool
- **Description**: The write pool is capped at 2 connections (AC-09, ADR-001). Using it for read-only SELECT operations (observation batch reads, observation count queries, session queries) consumes write pool connections that should be reserved for integrity writes (entries, audit_log, etc.) and the drain task's batch commits. Each call to `fetch_observation_batch` or any `ObservationService` query acquires a write pool connection for the duration of the fetch. Under concurrent load, this leaves fewer connections for integrity writes, increasing the probability of `StoreError::PoolTimeout { pool: Write }` on core MCP tool operations.

  The architecture explicitly separates read_pool (6–8 connections) from write_pool (max 2) for this reason. These SELECT operations have no write semantics — they are safe to run on the read pool.

- **Recommendation**: Route all SELECT-only observation and session queries through `store.read_pool()` (via `store.read_pool_test()` in test code, or a public `read_pool()` accessor or individual store methods). The `write_pool_server()` accessor should be reserved for tables that require write-pool connection semantics (observations INSERT, audit_log INSERT, agent_registry, etc.).
- **Blocking**: No — the write pool timeout (5s) is the backstop and these reads are bounded in scope. However, this is a correctness debt that compounds as more call sites follow the same pattern.

---

### Finding 2: Export holds write pool connection for entire export duration

- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/export.rs:38–55`
- **Description**: The export function opens the store, acquires `write_pool_server()`, issues `BEGIN DEFERRED`, and then runs all export SELECT operations through that same write pool connection for the full export duration. With `write_pool max_connections=2`, the export snapshot transaction holds one of the two write pool connections for the entire export. During export, only one write connection remains for both integrity writes and the drain task's batch commits.

  The comment "ADR-001" is referenced but ADR-001 covers timeouts, not the read-vs-write pool routing decision. The architecture document states that `write_pool` serves "integrity writes and drain task"; it does not document export as an intended write_pool use case.

  An export of a large database (many entries, co_access rows, audit events) could hold the write connection for seconds, forcing integrity write callers to wait on the 5s write acquire timeout.

- **Recommendation**: Evaluate whether `BEGIN DEFERRED` (a read-only snapshot) on the read pool satisfies the export consistency requirement. SQLite WAL read pools at the DEFERRED isolation level see a consistent snapshot — no writes can interrupt the read. If the export's snapshot semantics require write pool access (e.g., to prevent WAL checkpoint during export), document this explicitly in the code and consider increasing `write_max_connections` for export-only pools.
- **Blocking**: No — the 5s timeout provides a structured error backstop. But this is an undocumented hold that the risk strategy document did not anticipate.

---

### Finding 3: `Handle::current().block_on()` in `persist_shadow_evaluations` — pattern risk

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/background.rs:196`
- **Description**: `persist_shadow_evaluations` is a sync `fn` that calls `tokio::runtime::Handle::current().block_on(store.insert_shadow_evaluations(...))`. It is called from within `tokio::task::spawn_blocking(move || { persist_shadow_evaluations(...) })` at line 1071. This is technically safe: `spawn_blocking` runs on a dedicated thread pool outside tokio worker threads, so `block_on` on an existing handle does not panic.

  However, `fn persist_shadow_evaluations` has no visible marker that it must only be called from `spawn_blocking`. If a future refactor calls this function from an async context (e.g., `tokio::spawn` instead of `spawn_blocking`), the `block_on` call will panic with "cannot call block_on from within a runtime". The risk is latent, not current.

  The architecture ADR-006 states "remove `spawn_blocking` wrappers" as a design goal. If this function is later converted to `async fn` and the `spawn_blocking` at the call site is removed without converting the `block_on` inside, the panic path is activated.

- **Recommendation**: Add a code comment explicitly stating `persist_shadow_evaluations` must only be called from `spawn_blocking` context, referencing R-08 from RISK-TEST-STRATEGY.md. Consider using `tokio::task::block_in_place` instead of `Handle::current().block_on()` if the function is ever called outside `spawn_blocking` — `block_in_place` panics at compile time in single-threaded contexts, which surfaces the issue earlier. Alternatively, convert to `async fn` now, which is consistent with the broader migration goal.
- **Blocking**: No.

---

### Finding 4: `format!` SQL interpolation in store crate — hardcoded values only, pattern risk

- **Severity**: low
- **Location**:
  - `crates/unimatrix-store/src/read.rs:123–131, 164, 192, 212, 239–252, 281–284, 305, 325, 395–440` — `format!("SELECT {} FROM entries ...", ENTRY_COLUMNS)` and dynamic WHERE clause construction
  - `crates/unimatrix-store/src/write_ext.rs:88–105` — `format!("UPDATE entries SET {} WHERE id = ?1", sets.join(", "))`
  - Test helpers: `format!("SELECT COUNT(*) FROM pragma_table_info('{table}')")` and `format!(...WHERE name = '{column}')` in test-only functions
- **Description**: Several query strings are assembled with `format!()` rather than `sqlx::query!()` macros. The values interpolated are:
  - `ENTRY_COLUMNS`: a `pub const &str` containing only column names (no user input)
  - `sets.join(", ")`: built from hardcoded string literals (`"last_accessed_at = {now}"`, `"access_count = access_count + 1"`) where `now` is an integer from `current_unix_timestamp_secs()` — a system clock value, not user input
  - `pragma_table_info` helpers: `table` and `column` parameters are always hardcoded literal strings in test helpers gated behind `#[cfg(test)]`

  None of these format! paths accept external or user-controlled input. However, the RISK-TEST-STRATEGY.md security section (SR-sec-01) explicitly recommends a CI grep check to reject `format!("SELECT...{}")` patterns in store SQL. That check does not exist in the diff. The pattern is non-exploitable today because all interpolated values are internal constants, but the absence of a CI guard means the pattern could be replicated with user-controlled input in the future without triggering a review gate.

- **Recommendation**: Add the CI grep check from RISK-TEST-STRATEGY.md: `grep -rn 'format!.*".*SELECT\|INSERT\|UPDATE\|DELETE.*{' crates/unimatrix-store/src/` returning zero matches. This does not need to block this PR but should be tracked as a follow-on CI improvement.
- **Blocking**: No.

---

### Finding 5: `write_pool_server()` public accessor — no external trust boundary, but access is unrestricted across entire server crate

- **Severity**: low
- **Location**: `crates/unimatrix-store/src/db.rs:162–170`
- **Description**: `write_pool_server()` returns `&SqlitePool` — a reference to the raw write pool — to any caller in the server crate. The docstring states "Callers MUST NOT use this for analytics-path writes." This constraint is enforced by convention only; there is no type-level or visibility barrier. Any function in the server crate can call `write_pool_server()` and route any query — including analytics writes — through the write pool without going through `enqueue_analytics`. If analytics writes are misrouted this way they are never shed under queue pressure (risk R-06 in RISK-TEST-STRATEGY.md), which means an analytics DoS scenario uses write pool capacity instead of the shedding path.

  The diff shows 131 new `write_pool_server()` call sites. While all current call sites were reviewed and none route analytics writes through the write pool, the broad exposure of this accessor is a regression risk: any future feature could add an analytics write via `write_pool_server()` and bypass the shed mechanism.

  Gate 3b was noted to have reviewed `migration.rs` DDL and `listener.rs` transactions, but the write_pool_server accessor exposure across 131 call sites in production code was not a focus of that review.

- **Recommendation**: Consider narrowing `write_pool_server()` visibility. If the tables accessed via this method (`audit_log`, `agent_registry`, `observations`, `shadow_evaluations`) were each exposed as typed store methods on `SqlxStore`, callers would not need the raw pool reference. This is a medium-term refactor goal, not a blocker.
- **Blocking**: No.

---

## OWASP Checklist

| Concern | Assessment |
|---------|-----------|
| SQL Injection | Not found. All user-originated data (query_text, session_id, content, etc.) is bound via sqlx parameterized queries. `format!()` SQL uses only internal constants. |
| Broken Access Control | Not found. Trust level enforcement remains in MCP tool dispatch (unchanged by this PR). No new MCP tools. |
| Security Misconfiguration | Not found. PRAGMAs (`foreign_keys ON`, `WAL`, `synchronous NORMAL`) are applied per-connection via `SqliteConnectOptions::pragma()`. `read_only(true)` on read pool is defense-in-depth. |
| Vulnerable Dependencies | sqlx 0.8 introduced. No known CVEs in sqlx 0.8 at time of review. rusqlite 0.34 (bundled SQLite) removed — net security improvement. New transitive dependencies: `atoi`, `concurrent-queue`, `crossbeam-queue`, `dotenvy`, `event-listener`, `flume`. All are well-maintained crates used by sqlx. |
| Data Integrity Failures | Integrity writes (entries, audit_log, entry_tags, vector_map, counters) go through write_pool directly or via `pool.begin().await?`. sqlx Transaction Drop rolls back on early exit. No integrity leak found. |
| Deserialization | `bincode` deserialization unchanged. No new deserialization of external data. `serde_json::from_str` on `target_ids` JSON uses `unwrap_or_default()` — safe for malformed JSON. |
| Input Validation | Analytics queue acts as rate-limit on analytics writes (capacity 1000, shed on full). Pool acquire timeouts (2s read, 5s write) prevent indefinite blocking from malicious callers. No new external input surface introduced. |
| Secrets | No hardcoded secrets, API keys, tokens, or passwords found in the diff. |

---

## Blast Radius Assessment

**Worst case if subtle bug in pool_config.rs:**
`PoolConfig::validate()` fails to reject `write_max_connections > 2`. A deployment with `write_max=3` opens 3 concurrent write connections. Under concurrent integrity writes, SQLite WAL begins serializing writers through its journal lock, causing SQLITE_BUSY on the third connection (5s busy_timeout). Callers receive `StoreError::PoolTimeout` after 5s — visible but operationally severe. Data integrity is preserved (no corruption).

**Worst case if subtle bug in analytics.rs drain task:**
The drain task does not call `drain_remaining_and_commit` on shutdown signal. Co_access, outcome_index, and session writes accumulated since the last batch commit (up to 50 events) are lost silently. No integrity table data is lost (drain handles only analytics tables). The `shed_counter` will not reflect this loss. This is analytics data loss, not integrity data loss — acceptable per the architecture's explicit tradeoff (FR-05).

**Worst case if write_pool_server() read operations accumulate:**
Five simultaneous callers each call a `write_pool_server()` SELECT (observation count, audit log read, session query, export snapshot, fetch_observation_batch). With `write_pool max_connections=2`, the first two succeed; the remaining three block until 5s write acquire timeout. MCP tool callers that need to do entry writes (`context_store`, `context_correct`) also block. The server appears hung for 5s, then returns structured timeout errors. No data corruption, but operational impact is severe.

---

## Regression Risk

1. **Drain task omission from tests**: If any of the ~1,445 converted async tests omits `store.close().await`, the drain task is left alive holding a write pool connection. Tests that share process state can observe SQLITE_BUSY on the next test's pool construction. Gate 3b identified this as R-02 (Critical) but was focused on migration.rs and listener.rs. This reviewer confirms the `close()` pattern is correctly implemented in `db.rs` (both `close()` and `Drop`), but test enforcement is outside scope of this code review.

2. **PRAGMA per-connection guarantee**: `build_connect_options()` applies all 6 PRAGMAs via `SqliteConnectOptions::pragma()`. This is correct — sqlx applies these options on each new connection, including lazily-created pool connections. The migration connection uses `apply_pragmas_to_connection()` separately. Both paths were reviewed and are consistent.

3. **Migration connection dropped before pool construction (ADR-003)**: The `drop(migration_conn)` at `db.rs:69` (explicit, not relying on end-of-scope) is present and correct. No regression from ADR-003 found.

4. **`#[non_exhaustive]` on AnalyticsWrite**: New variants `ObservationPhaseMetric` and `DeleteObservationPhases` are in the diff. The drain task's `match event` in `execute_analytics_write()` handles all variants exhaustively within the crate. External crates using `match AnalyticsWrite` must have catch-all arms — this is documented in the enum's doc comment.

---

## PR Comments

- Posted findings on PR #299 (see gh pr review below).
- Blocking findings: No.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the read-via-write-pool anti-pattern is a feature-specific manifestation of R-01 (pool starvation) already documented in RISK-TEST-STRATEGY.md entry #2057 and ARCHITECTURE.md. No new cross-feature lesson emerged beyond what the risk strategy already captured. The `block_on` in `spawn_blocking` pattern is already documented in multiple Unimatrix entries.
