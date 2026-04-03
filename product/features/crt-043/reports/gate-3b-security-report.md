# Gate 3b Security Review: crt-043 Behavioral Signal Infrastructure

> Gate: 3b (Security Review)
> Date: 2026-04-03
> Branch: feature/crt-043
> Reviewer: crt-043-gate-3b (claude-sonnet-4-6, fresh context)
> Result: **PASS**

---

## Risk Level: Low

## Summary

Two implementation waves add nullable columns (`goal_embedding BLOB` on `cycle_events`, `phase TEXT` on `observations`) plus a composite index, and a fire-and-forget goal-embedding spawn. No new external input surfaces are introduced. All SQL uses SQLx bound parameters. The deserialization path (`decode_goal_embedding`) has zero production call sites in this branch — it exists as a pub API for future Group 6 consumers. All error paths in the embedding spawn use `tracing::warn!` without panicking. Phase capture from session state is an in-memory clone of an `Option<String>` gated behind an `and_then` — no cross-request state leakage is possible.

---

## Findings

### Finding 1: `decode_goal_embedding` — no production call site in this branch

- **Severity**: Informational
- **Location**: `crates/unimatrix-store/src/embedding.rs:46`, `crates/unimatrix-store/src/lib.rs:32`
- **Description**: `decode_goal_embedding` is promoted to `pub` and re-exported from the crate root. No production code on this branch calls it — only tests. The OVERVIEW.md comment acknowledges Group 6 will consume it via a future store query method. The function itself returns `Result<Vec<f32>, DecodeError>` and has no panic path. The malformed-bytes test (EMBED-U-02) confirms `DecodeError` is returned, not a panic, for truncated input. **There is no current attacker-reachable decode path** — the column is write-only in this PR.
- **Recommendation**: When Group 6 ships the read path, ensure decode is called only from within a store query method (as the OVERVIEW.md WARN-2 instructs), never from a direct MCP tool handler. No action required for this PR.
- **Blocking**: No

### Finding 2: INSERT/UPDATE race on goal_embedding — race window acknowledged

- **Severity**: Low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2509-2577` (Step 5 spawn, Step 6 spawn)
- **Description**: The goal-embedding `tokio::spawn` (Step 6) fires after the cycle_start INSERT spawn (Step 5). The UPDATE will silently no-op (zero rows affected, no error) if the INSERT has not yet committed. The code comment explicitly acknowledges this race (ADR-002). The outcome — `goal_embedding` remaining NULL — is identical to the embed-service-unavailable degradation path. There is no data corruption: the worst case is a missing embedding, not corrupted data or incorrect state.
- **Recommendation**: Accepted design. No action required.
- **Blocking**: No

### Finding 3: `cycle_id_embed` originates from `feature_cycle` — sanitization verified

- **Severity**: Informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2311-2320, 2533`
- **Description**: `cycle_id_embed` is cloned from `feature_cycle`, which is the output of `sanitize_metadata_field()` applied to the UDS payload's `feature_cycle` key. `sanitize_metadata_field` strips non-printable ASCII and truncates to 128 chars. The value is then used as a bound parameter (`?2`) in the UPDATE SQL — no string interpolation. SQL injection via `cycle_id_embed` is not possible.
- **Recommendation**: None.
- **Blocking**: No

### Finding 4: `phase` column — no validation on stored value

- **Severity**: Informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:708-710, 825-827, 934-937, 1082-1083`
- **Description**: `obs.phase` is set from `session_registry.get_state(&event.session_id).and_then(|s| s.current_phase.clone())`. The `current_phase` value originates from `event.payload.get("next_phase")` in `handle_cycle_event` (Step 3), stored verbatim (no sanitization). An attacker who can send a UDS cycle event with a crafted `next_phase` value can write arbitrary text into `observations.phase`. However: (1) UDS peer credentials are UID-verified before dispatch; (2) the column is TEXT with no constraints — there is no injection risk from the stored value itself, as all reads will use bound parameters; (3) the worst case is a noisy or incorrect phase label in the analytics column. This is not a security risk in the current system, but worth noting for future query consumers.
- **Recommendation**: If `phase` values are ever displayed in a UI or compared against an enum, add allowlist validation at capture time using a pattern analogous to `sanitize_observation_source`. Not blocking for this PR.
- **Blocking**: No

### Finding 5: `goal_text` — byte-bounded before embedding, but not sanitized

- **Severity**: Informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2524-2531`
- **Description**: `goal_for_event` is the verbatim UDS goal string (byte-truncated at `MAX_GOAL_BYTES` = 1024 by `truncate_at_utf8_boundary`). The embedding spawn trims whitespace before the `Some` check, so whitespace-only goals produce no spawn. The goal text is passed to `adapter.embed_entry("", &goal_text)` — this is the ONNX inference path, which processes text, not SQL. ONNX does not interpret the string as a command or query; it tokenizes it. No injection risk applies here.
- **Recommendation**: None.
- **Blocking**: No

---

## OWASP Checklist

| OWASP Category | Status | Notes |
|----------------|--------|-------|
| A01 Broken Access Control | PASS | UDS peer-credential UID check unchanged; `embed_service` passed by reference, no new capability gate needed |
| A02 Cryptographic Failures | PASS | No cryptography involved; bincode is serialization, not encryption |
| A03 Injection | PASS | All SQL uses SQLx bound parameters (`?1`, `?2`); no string interpolation in any new SQL |
| A04 Insecure Design | PASS | INSERT/UPDATE race explicitly designed and accepted (ADR-002); degradation to NULL is safe |
| A05 Security Misconfiguration | PASS | No new configuration surface; `goal_embedding BLOB` column is nullable by design (NFR-04) |
| A06 Vulnerable Components | PASS | No new dependencies introduced; `bincode = "2"` pre-existing workspace dependency |
| A07 Auth Failures | PASS | No auth changes; cycle event path requires existing UDS connection |
| A08 Data Integrity Failures | PASS | bincode standard config is deterministic and self-describing; round-trip tests confirm integrity |
| A09 Logging Failures | PASS | All error paths emit `tracing::warn!`; no silent swallowing of unexpected errors |
| A10 SSRF | N/A | No outbound HTTP; embedding is local ONNX inference |

---

## Blast Radius Assessment

**Worst case if the embedding spawn has a subtle bug**: The `tokio::spawn` is fire-and-forget with `let _ = tokio::spawn(...)`. A panic inside the spawn would terminate only that task (Tokio catches task panics). The cycle start processing, UDS response, and session state mutation are all complete before the spawn fires. No user-visible functionality is blocked.

**Worst case if migration v20→v21 partially applies**: Both `pragma_table_info` pre-checks run before either `ALTER TABLE`. If one column was added in a previous partial run, the pre-check skips the `ALTER` for that column. The `CREATE INDEX IF NOT EXISTS` is inherently idempotent. `schema_version` is bumped inside the same outer transaction — if the transaction rolls back (e.g., disk full), the version stays at 20 and the next open will retry. The idempotency design eliminates the partial-apply corruption risk.

**Worst case if `insert_observation` fails with the new `phase` binding**: The error is caught by the fire-and-forget closure and emitted as `tracing::error!`. The UDS connection returns `HookResponse::Ack` regardless (existing behavior). Phase data is lost for that observation; no user-visible regression.

**Worst case if `update_cycle_start_goal_embedding` receives corrupt `cycle_id`**: The UPDATE matches on both `cycle_id = ?2 AND event_type = 'cycle_start'`. A corrupt or empty cycle_id would match zero rows — silent no-op, `goal_embedding` stays NULL. No other row is affected.

---

## Regression Risk

| Area | Risk | Rationale |
|------|------|-----------|
| Existing observation writes | Low | `phase` column is nullable; existing rows get NULL. `INSERT` SQL adds `?9` binding — any existing call site that bypassed `insert_observation` would fail, but no such site exists. |
| Existing `cycle_events` writes | Low | `goal_embedding` column is nullable; `insert_cycle_event` does not bind it (pre-existing columns only). All existing rows and writes are unaffected. |
| Migration chain | Low | v19→v20 migration tests were updated to expect version 21 (chain runs in full); this is correct. Idempotency tests pass on second open. |
| Session state cross-request leakage | None | `get_state` returns a **clone** of `SessionState` (value type). Mutating `obs.phase` after the clone does not affect registry state. No aliasing. |
| `handle_cycle_event` signature change | Low | `embed_service` added as last parameter. All three call sites in `dispatch_request` were updated in the same commit. Compilation enforces completeness. |

---

## Specific Checks (per review brief)

### 1. Bincode deserialization safety

`decode_goal_embedding` is not called in any production code path on this branch. The write path (`encode_goal_embedding`) is called from the embedding spawn and is infallible for valid `Vec<f32>`. When Group 6 adds a read path, bytes will come from the SQLite `goal_embedding` column — data written exclusively by `encode_goal_embedding`. The only way attacker-controlled bytes could reach `decode_goal_embedding` is if an attacker could write directly to the SQLite file (which implies full local access, outside the threat model). No panic path exists: `bincode::serde::decode_from_slice` returns `DecodeError`, which is propagated as `Result`. **PASS.**

### 2. SQL injection

All new SQL statements use SQLx bound parameters:
- `UPDATE cycle_events SET goal_embedding = ?1 WHERE cycle_id = ?2 AND event_type = 'cycle_start'` — two bound params, no interpolation.
- `INSERT INTO observations ... VALUES (?1..?9)` — nine bound params, no interpolation.
- `ALTER TABLE cycle_events ADD COLUMN goal_embedding BLOB` — no parameters needed; literal DDL.
- `ALTER TABLE observations ADD COLUMN phase TEXT` — literal DDL.
- `SELECT COUNT(*) FROM pragma_table_info(...)` — table name is a string literal, not a parameter. **PASS.**

### 3. Fire-and-forget spawn error handling

The embedding spawn (`let _ = tokio::spawn(async move { ... })`) handles all four error branches:
- `embed_svc.get_adapter().await` failure: `tracing::warn!`, no panic.
- `adapter.embed_entry(...).Err`: `tracing::warn!`, no panic.
- `encode_goal_embedding(...).Err`: `tracing::warn!`, no panic.
- `update_cycle_start_goal_embedding(...).Err`: `tracing::warn!` with `cycle_id`, no panic.

No `.unwrap()` in the spawn body. **PASS.**

### 4. Phase capture — cross-request state leakage

`get_state` returns `Option<SessionState>` where `SessionState` is a **clone** (line 225 of session.rs: `sessions.get(session_id).cloned()`). The Mutex is acquired and released within `get_state`. The returned `SessionState` is owned by the calling code and has no shared reference to the registry's internal state. Writing to `obs.phase` from the clone does not affect any other request's view of `current_phase`. **PASS.**

### 5. Migration atomicity

The v20→v21 block runs inside the `txn` passed from `migrate_if_needed`'s outer transaction (`conn.begin()` / `txn.commit()` / `txn.rollback()`). Both `ALTER TABLE` statements execute inside that transaction. The schema_version bump (`UPDATE counters SET value = 21`) is also inside the transaction. If either `ALTER TABLE` fails, the entire transaction rolls back, `schema_version` stays at 20, and the next open retries. The `CREATE INDEX IF NOT EXISTS` is also inside the transaction and is idempotent. **PASS.**

### 6. Blast radius — additive nullable columns

Both new columns (`goal_embedding BLOB`, `phase TEXT`) are nullable with no default constraint and no backfill. Pre-v21 rows get NULL for both, which is the documented baseline (NFR-04). All existing `INSERT INTO observations` and `INSERT INTO cycle_events` call sites do not bind the new columns — SQLite assigns NULL automatically. No existing read query is affected (no `SELECT *` without explicit column list in production code that would break on schema change). **PASS.**

---

## Hardcoded Secrets Check

No hardcoded secrets, API keys, tokens, credentials, or passwords in any modified file. **PASS.**

---

## Scope Check

The diff contains exactly:
- `crates/unimatrix-store/src/embedding.rs` (NEW) — in scope
- `crates/unimatrix-store/src/migration.rs` — in scope
- `crates/unimatrix-store/src/db.rs` — in scope
- `crates/unimatrix-store/src/lib.rs` — in scope (re-export only)
- `crates/unimatrix-store/tests/migration_v20_v21.rs` (NEW) — in scope
- `crates/unimatrix-store/tests/migration_v19_v20.rs` — version constant updates only, in scope
- `crates/unimatrix-server/src/uds/listener.rs` — in scope
- `crates/unimatrix-server/src/server.rs` — schema version assertion update only, in scope

No out-of-scope changes detected.

---

## PR Comments

No blocking findings. No PR comments required.

---

## Knowledge Stewardship

Nothing novel to store — the bincode-encode-in-spawn / decode-in-store-method pattern and the fire-and-forget warn-not-panic error convention are already represented in the codebase. The observation that `decode_goal_embedding` has no production call site in Wave 2 (write-path-only PR) is an expected design property, not an anti-pattern requiring a lesson.

---

*Report authored by crt-043-gate-3b (claude-sonnet-4-6). Date: 2026-04-03.*
