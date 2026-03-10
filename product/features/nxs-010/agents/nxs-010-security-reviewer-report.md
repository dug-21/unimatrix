# Security Review: nxs-010-security-reviewer

## Risk Level: low

## Summary

nxs-010 is a purely additive schema evolution adding two SQLite tables (`topic_deliveries`, `query_log`) and wiring fire-and-forget query_log writes into existing search paths. All SQL uses parameterized queries. No new dependencies are introduced. No secrets, no file path operations, no deserialization of untrusted binary data. The change is well-scoped with minimal blast radius.

## Findings

### Finding 1: Parameterized queries consistently used -- no injection risk
- **Severity**: info (no issue found)
- **Location**: `crates/unimatrix-store/src/query_log.rs:97-113`, `crates/unimatrix-store/src/topic_deliveries.rs:67-84`
- **Description**: All SQL operations use rusqlite `params![]` with positional placeholders. No string interpolation of user-controlled input into SQL. The `TOPIC_DELIVERY_COLUMNS` constant used in `format!()` for SELECT statements contains only hardcoded column names, not user input.
- **Blocking**: no

### Finding 2: User-controlled query_text stored without size limit
- **Severity**: low
- **Location**: `crates/unimatrix-store/src/query_log.rs:98-100`, `crates/unimatrix-server/src/uds/listener.rs:916`, `crates/unimatrix-server/src/mcp/tools.rs:337`
- **Description**: The `query_text` field is stored as-is from user search queries with no size cap. SQLite TEXT columns have no practical limit (~2GB). A malicious or buggy caller could submit very large query strings that consume disk space in the query_log table. However, the UDS path has `MAX_PAYLOAD_SIZE` enforcement upstream, and the MCP path has rmcp message size limits. The fire-and-forget pattern means disk pressure would only affect the query_log table, not search responses.
- **Recommendation**: Accept as-is. Upstream size limits provide adequate protection. If query_log volume becomes a concern, the existing GC deferral (SR-06, NFR-03) addresses it.
- **Blocking**: no

### Finding 3: Fire-and-forget error handling is correct
- **Severity**: info (no issue found)
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:920-933`, `crates/unimatrix-server/src/mcp/tools.rs:345-353`
- **Description**: Both paths wrap `insert_query_log` in `if let Err(e)` with `tracing::warn!`. The `spawn_blocking` return value is correctly discarded with `let _`. No `unwrap()` or `expect()` on the insert path. Panics inside `spawn_blocking` are isolated to the spawned task and do not propagate to the caller.
- **Blocking**: no

### Finding 4: UDS path correctly guards on session_id presence
- **Severity**: info (no issue found)
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:908-909`
- **Description**: The UDS path checks `if let Some(ref sid) = session_id` and `if !sid.is_empty()` before writing to query_log. This matches the injection_log guard pattern and prevents writing analytically useless rows.
- **Blocking**: no

### Finding 5: MCP path uses unwrap_or_default for session_id
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:334`
- **Description**: The MCP path uses `ctx.audit_ctx.session_id.clone().unwrap_or_default()` which writes an empty string if session_id is None. This is intentional per the architecture document (Open Question 2) -- MCP queries are always analytically interesting. The empty string is a valid TEXT value and does not cause index or query issues.
- **Recommendation**: No change needed. Behavior is documented and intentional.
- **Blocking**: no

### Finding 6: Migration backfill SQL is safe
- **Severity**: info (no issue found)
- **Location**: `crates/unimatrix-store/src/migration.rs:148-199`
- **Description**: The backfill uses `INSERT OR IGNORE` for idempotency on re-run. The WHERE clause `feature_cycle IS NOT NULL AND feature_cycle != ''` correctly excludes unattributed sessions. The COALESCE(SUM(...), 0) handles all-NULL ended_at safely. The migration runs inside the existing `BEGIN IMMEDIATE` transaction, so partial failure rolls back atomically.
- **Blocking**: no

### Finding 7: No new dependencies introduced
- **Severity**: info (no issue found)
- **Location**: No changes to Cargo.toml or Cargo.lock
- **Description**: The feature uses only existing dependencies (rusqlite, serde_json, tokio). No new supply chain risk.
- **Blocking**: no

### Finding 8: StoreError::Deserialization used for non-deserialization error
- **Severity**: low
- **Location**: `crates/unimatrix-store/src/topic_deliveries.rs:130-133`
- **Description**: `update_topic_delivery_counters` returns `StoreError::Deserialization("topic_delivery not found: {topic}")` when no rows are affected. This is a semantic mismatch -- the error is a "not found" condition, not a deserialization failure. It does not create a security issue, but could confuse error handling in downstream consumers that match on error variants.
- **Recommendation**: Consider introducing a `StoreError::NotFound` variant in a future change. Not blocking for this PR.
- **Blocking**: no

## Blast Radius Assessment

**Worst case if the fix has a subtle bug**: The query_log or topic_deliveries table has incorrect data. Since both tables are new and have no existing consumers in this PR (consumers are in future features col-020, crt-018, crt-019), incorrect data would affect only future analysis -- not current server operation. The fire-and-forget pattern ensures search responses are never blocked or corrupted by query_log write failures.

**Migration failure**: If the v10->v11 migration fails, the transaction rolls back and the database stays at v10. The next `Store::open()` retries. No data loss. The `create_tables()` call after migration uses `IF NOT EXISTS`, so it safely handles both fresh and migrated databases.

**Silent data corruption risk**: Low. The only mutation path is INSERT (query_log) and INSERT OR REPLACE (topic_deliveries). No UPDATE of existing data in the search hot path. The backfill is a one-time operation during migration.

## Regression Risk

- **Schema version bump (10 -> 11)**: The `server.rs` test assertions were updated to expect version 11 instead of 10. Existing migration tests continue to work because all migration steps are additive and guarded by version checks.
- **Import change in UDS listener**: Only adds `QueryLogRecord` to the existing import. No removed imports.
- **No existing API signatures changed**: All new code is additive (new modules, new `impl Store` methods, new code blocks in search handlers).
- **Regression risk is low**: No existing behavior is modified. The only changes to existing files are (1) appending new code blocks after existing search logic, (2) updating schema version expectations in tests, and (3) adding imports.

## PR Comments
- Posted 1 comment on PR #186
- Blocking findings: no
