# Security Review: crt-043-security-reviewer

## Risk Level: Low

## Summary

crt-043 adds two nullable columns (`goal_embedding BLOB`, `phase TEXT`) to existing SQLite tables, a composite index, and a fire-and-forget goal-embedding spawn in the UDS listener. No new external input surfaces, no new network calls, no new crate dependencies. All new SQL uses SQLx bound parameters. The highest-risk finding (F-04 below) is a minor logic gap — the embedding spawn fires even when `feature_cycle` is empty, producing a wasted UPDATE against `cycle_id = ''` that safely matches zero rows. No blocking findings.

---

## Findings

### F-01: Architecture document uses stale column name in example SQL

- **Severity**: Informational
- **Location**: `product/features/crt-043/architecture/ARCHITECTURE.md` lines 48 and 144
- **Description**: The document refers to `WHERE topic = ?2` in the example UPDATE SQL. The actual implementation at `crates/unimatrix-store/src/db.rs:426-427` correctly uses `WHERE cycle_id = ?2 AND event_type = 'cycle_start'`. The `cycle_events` table schema confirms the column is `cycle_id`, not `topic`. This is a documentation-only discrepancy; the code is correct.
- **Recommendation**: Update the architecture doc before it is archived. Not blocking.
- **Blocking**: No

### F-02: `phase` column stores `next_phase` payload value without allowlist validation

- **Severity**: Low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:707-710, 824-827, 934-937, 1081-1083`
- **Description**: `obs.phase` is set from `session_registry.get_state(...).and_then(|s| s.current_phase.clone())`. The `current_phase` value originates from `event.payload.get("next_phase")` in Step 3 of `handle_cycle_event`, stored verbatim with no sanitization (comment at line 2376 notes no normalization is applied). A UDS peer with `SessionWrite` capability can write arbitrary text into `observations.phase`. Constraints: (1) UDS peers must pass UID credential check before dispatch is reached; (2) the value is bound as a SQL parameter — SQL injection is not possible; (3) this is an analytics-only column with no security enforcement semantics. The risk is noisy/incorrect phase labels corrupting Group 6 phase-stratification queries, not a security breach.
- **Recommendation**: If `phase` values are ever surfaced in a UI or compared against an enum at read time, add allowlist validation at the capture sites using a pattern analogous to `sanitize_observation_source`. Not required for this PR — document as a known data-quality gap for Group 6.
- **Blocking**: No

### F-03: Double schema_version write in migration — redundant but correct

- **Severity**: Informational
- **Location**: `crates/unimatrix-store/src/migration.rs:848-853, 857-863`
- **Description**: The v21 migration block issues `UPDATE counters SET value = 21` inside the block (line 848), then after all migration blocks there is an unconditional `INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)` with `CURRENT_SCHEMA_VERSION = 21` (line 857). This writes the schema version twice within the same transaction. The `INSERT OR REPLACE` is the pattern used by all previous migration steps and is correct. The in-block `UPDATE` is correctly documented as enabling future chained blocks to observe the intermediate version. Both writes are inside the same outer transaction. There is no functional risk — the final committed value is always `CURRENT_SCHEMA_VERSION`. The only concern would be if `INSERT OR REPLACE` and `UPDATE` conflicted, which they cannot in SQLite for a single-row keyed table.
- **Recommendation**: None. The pattern is intentional and matches all prior migration blocks.
- **Blocking**: No

### F-04: Embedding spawn fires when `feature_cycle` is empty — wasted CPU, no corruption

- **Severity**: Low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2523-2587`
- **Description**: Step 5 (INSERT spawn) is guarded by `if !feature_cycle.is_empty()` (line 2466). Step 6 (embedding spawn) is guarded only by `if lifecycle == CycleLifecycle::Start` (line 2523) with no corresponding `feature_cycle` empty-check. If a `CycleStart` event arrives with `feature_cycle = ""` (after sanitization) and a non-empty `goal`, Step 5 will not INSERT any row, but Step 6 will spawn an embed task. That task performs ONNX inference (CPU on the rayon pool) and then issues `UPDATE cycle_events SET goal_embedding = ?1 WHERE cycle_id = '' AND event_type = 'cycle_start'`, which will match zero rows. Result: CPU wasted on an embedding that cannot be stored. No data corruption. The logging path for this scenario (`update_cycle_start_goal_embedding` returning Ok with zero rows affected) is a silent no-op per ADR-002 design.
- **Recommendation**: Add `&& !feature_cycle.is_empty()` to the Step 6 outer guard, or ensure the `feature_cycle` empty path is covered in test coverage (the warn already fires for empty `feature_cycle` at lines 2321-2334). Not blocking — the worst case is wasted rayon pool work, not data corruption or incorrect state.
- **Blocking**: No

### F-05: `decode_goal_embedding` has no production call site in this branch

- **Severity**: Informational
- **Location**: `crates/unimatrix-store/src/embedding.rs:46`, `crates/unimatrix-store/src/lib.rs:32`
- **Description**: `decode_goal_embedding` is exported `pub` from the crate root but is called only in tests within this PR. The OVERVIEW.md and source comment acknowledge this explicitly — Group 6 will consume it via a future store query method. There is no attacker-reachable decode path in this branch. The malformed-bytes test (EMBED-U-02) confirms `DecodeError` is returned, not a panic, for truncated input. Risk: when Group 6 ships a read path, if it calls `decode_goal_embedding` directly (bypassing the intended store method wrapper), untrusted bytes from a compromised SQLite file could trigger decode errors. The only way attacker-controlled bytes reach this function is with direct SQLite file write access (outside the threat model).
- **Recommendation**: When Group 6 ships, verify decode is called only through a store query method, not directly in MCP tool handlers. OVERVIEW.md WARN-2 documents this constraint. No action required for this PR.
- **Blocking**: No

---

## OWASP Checklist

| OWASP Category | Status | Notes |
|----------------|--------|-------|
| A01 Broken Access Control | PASS | UDS UID credential check unchanged; embed_service passed by reference only |
| A02 Cryptographic Failures | PASS | No cryptography; bincode is serialization, not encryption |
| A03 Injection | PASS | All new SQL uses SQLx bound parameters; no string interpolation; DDL uses string literals only |
| A04 Insecure Design | PASS | INSERT/UPDATE race explicitly designed and accepted (ADR-002); NULL degradation is safe |
| A05 Security Misconfiguration | PASS | No new config surface; nullable columns are correct by design |
| A06 Vulnerable Components | PASS | No new dependencies; bincode 2 is a pre-existing workspace dependency |
| A07 Auth Failures | PASS | No auth changes; cycle event path requires existing UDS connection with UID check |
| A08 Data Integrity Failures | PASS | bincode standard config is deterministic; round-trip tests confirm integrity |
| A09 Logging Failures | PASS | All four error branches in embedding spawn emit tracing::warn!; no silent swallowing |
| A10 SSRF | N/A | No outbound HTTP; embedding is local ONNX inference on rayon pool |

---

## Blast Radius Assessment

**Worst case — embedding spawn has a subtle bug**: The `let _ = tokio::spawn(...)` assignment discards the `JoinHandle`. A panic inside the spawn task would terminate only that task; Tokio catches task panics and does not propagate them to the spawner. Cycle start processing, UDS response, and session state mutation are all complete before the spawn fires. No user-visible functionality is blocked. The worst reachable outcome is `goal_embedding` remaining NULL for the affected cycle.

**Worst case — migration v20→v21 partially applies**: Both `pragma_table_info` pre-checks run before either `ALTER TABLE`. If one column was added in a previous partial run, the pre-check skips the `ALTER` for that column. `CREATE INDEX IF NOT EXISTS` is inherently idempotent. `schema_version` is bumped inside the same outer transaction — if the transaction rolls back (e.g., disk full between the two `ALTER TABLE`s), the version stays at 20 and the next open retries the full v21 block. No partial-apply corruption is possible.

**Worst case — `phase` binding is dropped from `insert_observation`**: The observation write would fail at the SQL bind count mismatch (`?9` unbound). The error is caught by the fire-and-forget error handler and emitted as `tracing::error!`. The UDS connection returns `HookResponse::Ack` regardless. Phase data is lost for that observation. This is the pre-existing error-handling contract for all observation writes.

**Worst case — `update_cycle_start_goal_embedding` receives an empty `cycle_id`** (F-04 path): The UPDATE matches zero rows. SQLite returns Ok with 0 rows affected. No panic. No data corruption. The wasted rayon-pool work completes and is discarded.

---

## Regression Risk

| Area | Risk | Rationale |
|------|------|-----------|
| Existing observation writes | Low | `phase` column is nullable; existing rows get NULL. INSERT SQL adds `?9` binding — any bypassed call site would fail at SQL level, but no such site exists. |
| Existing `cycle_events` writes | Low | `goal_embedding` column is nullable; `insert_cycle_event` does not bind it (existing columns only). All existing rows and writes are unaffected. |
| Migration chain | Low | v19→v20 tests updated to expect version 21 (chain runs in full); this is correct. Idempotency tests pass on second open. The test name `test_current_schema_version_is_20` in `migration_v19_v20.rs` now asserts 21 — the name is stale but the assertion is correct. |
| `handle_cycle_event` signature change | Low | `embed_service` added as last parameter. All three call sites in `dispatch_request` were updated in the same commit. Rust compilation enforces completeness; any missed call site would be a compile error. |
| Session state cross-request leakage | None | `get_state` returns a clone (`SessionState` by value). Mutating `obs.phase` after the clone does not affect the registry's internal state or any concurrent request. |

---

## Input Validation Check

| Input | Validation | Assessment |
|-------|-----------|------------|
| `goal` text (UDS payload) | Byte-truncated at `MAX_GOAL_BYTES` (1024) via `truncate_at_utf8_boundary`; whitespace trimmed before embed spawn | Adequate for the embedding use case. Goal text is not used in SQL. |
| `feature_cycle` (cycle_id for UPDATE) | `sanitize_metadata_field`: strips non-printable ASCII, truncates to 128 chars | Adequate; passed as SQLx bound parameter. |
| `next_phase` → `current_phase` → `obs.phase` | None — stored verbatim from UDS payload | Low severity (F-02); stored as TEXT, queried via bound parameter. No injection surface. |
| Embedding bytes from ONNX | None — values are trusted output from internal rayon pool | Acceptable; no external boundary. |
| `decode_goal_embedding` input | Bincode DecodeError returned for malformed bytes; no panic | Adequate for future read sites. |

---

## Dependency Safety

No new crate dependencies. `bincode = { version = "2", features = ["serde"] }` is a pre-existing workspace dependency. No new entries to `Cargo.toml` or `Cargo.lock`. No CVE surface introduced.

---

## Hardcoded Secrets Check

No hardcoded secrets, API keys, tokens, credentials, or passwords in any modified file. PASS.

---

## Scope Check

Changed files are all in-scope for crt-043:
- `crates/unimatrix-store/src/embedding.rs` (NEW) — in scope
- `crates/unimatrix-store/src/migration.rs` — in scope
- `crates/unimatrix-store/src/db.rs` — in scope
- `crates/unimatrix-store/src/lib.rs` — re-export only, in scope
- `crates/unimatrix-store/tests/migration_v20_v21.rs` (NEW) — in scope
- `crates/unimatrix-store/tests/migration_v19_v20.rs` — version constant updates only, in scope
- `crates/unimatrix-store/tests/sqlite_parity.rs` — version assertion update only, in scope
- `crates/unimatrix-server/src/uds/listener.rs` — in scope
- `crates/unimatrix-server/src/server.rs` — schema version assertion update only, in scope
- `product/features/crt-043/**` — design artifacts, in scope
- `product/research/ass-040/ROADMAP.md` — minor update, not a code change

No out-of-scope code changes detected.

---

## PR Comments

- 1 comment posted on PR #506 via `gh pr review 506 --comment`
- Blocking findings: No

---

## Knowledge Stewardship

Nothing novel to store. The `feature_cycle` empty-guard asymmetry between Step 5 and Step 6 (F-04) is a local implementation detail, not a generalizable anti-pattern. The bincode encode/decode pairing discipline is already captured in existing knowledge entries. The fire-and-forget warn-not-panic convention is already represented in the codebase. The UDS trusted-boundary pattern (UID check before dispatch) is pre-existing and unchanged.

---

*Report authored by crt-043-security-reviewer (claude-sonnet-4-6, fresh context). Date: 2026-04-03.*
