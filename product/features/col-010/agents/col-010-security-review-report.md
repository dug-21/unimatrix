# Security Review: col-010-security-review

## Risk Level: low

## Summary

The col-010 implementation is well-structured and follows the design decisions closely. Two gaps exist worth noting: `sanitize_session_id` is not called on the `SessionClose` and `ContextSearch` paths (only `SessionRegister` validates the incoming session_id), and `sanitize_session_id` explicitly accepts the empty string as valid, which may be by design but is undocumented. Neither is blocking. No injection, path traversal, credential, or deserialization risks were found.

---

## Findings

### Finding 1: session_id not validated on SessionClose and ContextSearch paths

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds_listener.rs:442-464` (SessionClose), `512-533` (ContextSearch)
- **Description**: `sanitize_session_id` is called only in the `SessionRegister` arm (line 388). `SessionClose` and `ContextSearch` receive a `session_id` from the hook caller and forward it unsanitized to `process_session_close`, `insert_injection_log_batch`, and `session_registry` calls. Because authentication (UID check) already runs at the connection level before any request is dispatched, an attacker reaching these arms is already the server owner. The practical risk is low. However, a session_id arriving in `SessionClose` or `ContextSearch` with invalid characters (e.g., from a buggy or future hook) would be stored verbatim in SESSIONS or INJECTION_LOG.
- **Recommendation**: Add `sanitize_session_id` calls at the start of the `SessionClose` and `ContextSearch` dispatch arms, mirroring the `SessionRegister` pattern. This is a defence-in-depth measure rather than a blocking security gap given the UID auth boundary.
- **Blocking**: no

### Finding 2: Empty session_id accepted by sanitize_session_id

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds_listener.rs:47-56`, test at line 2217
- **Description**: `sanitize_session_id("")` returns `Ok(())`. The test explicitly documents this as intended ("Empty string: no chars to fail"). However, downstream code in `handle_context_search` already guards against empty session_id before writing injection log records (lines 707-708, 718-719: `if !sid.is_empty()`). The `SessionRegister` path does not have such a guard and would persist a `SessionRecord` with `session_id: ""` as the SESSIONS table key. If a hook sends `SessionRegister` with an empty session_id, it will be stored. A second `SessionRegister` with an empty session_id would silently overwrite it (redb upsert semantics).
- **Recommendation**: Explicitly reject empty session_id in `sanitize_session_id`, or document the accepted behavior in the function's doc comment and add a guard in `SessionRegister` before persist. The fix in `sanitize_session_id` is one line: `if session_id.is_empty() { return Err("session_id must not be empty".to_string()); }`.
- **Blocking**: no

### Finding 3: Auto-outcome entry title and topic embed unsanitized session_id

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds_listener.rs:1261-1263`
- **Description**: `write_auto_outcome_entry` constructs `title: format!("Session outcome: {}", session_id)` and `topic: format!("session/{}", session_id)` where `session_id` comes from process_session_close, which receives it from the `SessionClose` dispatch arm without sanitization (see Finding 1). If a session_id with unexpected characters reaches this path, those characters are embedded in a persisted entry title and topic. The risk is limited to data integrity (odd-looking entries in the knowledge base) not code execution, because the values pass through `Store::insert` and are stored as bincode-serialized strings, not interpreted as queries.
- **Recommendation**: Addressed by resolving Finding 1. No additional action needed once SessionClose sanitizes at entry.
- **Blocking**: no

---

## Design Decision Verification

| Design Decision | Status |
|---|---|
| SEC-01: session_id `[a-zA-Z0-9-_]`, max 128 chars | Implemented at SessionRegister; missing on SessionClose and ContextSearch |
| ADR-002: GC cascade in same write transaction as SESSIONS delete | PASS — gc_sessions uses a single WriteTransaction for all 5 phases |
| ADR-003: One write transaction per ContextSearch response for INJECTION_LOG | PASS — insert_injection_log_batch is called once per ContextSearch response |
| SR-05: next_log_id = 0 only if key doesn't exist (idempotent) | PASS — migration.rs line 78: `if counters.get("next_log_id")?.is_none()` |
| FR-08: auto-outcome entry: embedding_dim=0, trust_source="system", category="outcome" | PASS — write.rs sets embedding_dim=0 for all NewEntry inserts; trust_source="system" and category="outcome" set in write_auto_outcome_entry |
| FR-06: No auto-outcome for Abandoned sessions | PASS — line 1213: `if !is_abandoned && injection_count > 0` |
| Fire-and-forget spawn_blocking for all UDS store writes | PASS |
| SESSIONS table key is session_id (string) | PASS |
| Migration single transaction covering all steps | PASS |

---

## Blast Radius Assessment

The worst-case failure mode for a subtle bug in this PR is silent data loss in INJECTION_LOG during GC. However, `gc_sessions` runs all five phases (collect-to-delete, scan injection log, delete log entries, delete sessions, mark timed-out) inside a single `WriteTransaction` with a final `txn.commit()`. If any phase returns an error, the transaction is dropped without commit. This is the correct atomicity guarantee for the cascade. The fire-and-forget pattern for `insert_session`, `update_session`, and `insert_injection_log_batch` means failures are logged as warnings and swallowed — this is an accepted trade-off documented as OQ-01 in the design. The SESSIONS record is the authority; in-memory `SessionRegistry` state is the primary path. Discrepancies between them are non-fatal.

If the migration fails partway through on a production database, the `txn.commit()` at the end of `migrate_if_needed` is never reached, so no partial state is written. The idempotent `next_log_id` counter guard (SR-05) prevents a re-run from resetting a counter that advanced. This is sound.

## Regression Risk

Low. The two new tables (SESSIONS, INJECTION_LOG) are additive. Existing code paths in the MCP tools are not modified except for the GC call added to the `maintain=true` branch of `context_status` in `tools.rs`. That GC path is opt-in and gated; it does not affect any existing tool behaviour for callers that do not pass `maintain: true`. The `outcome_tags.rs` change adds `"session"` to the allowed type list and is additive.

The `FEATURE_ENTRIES` multimap table is correctly opened in `db.rs` (line 54). All 17 tables are accounted for in `Store::open`.

---

## PR Comments

- Posted 1 comment on PR #77
- Blocking findings: no
