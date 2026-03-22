# Security Review: crt-025-security-reviewer

## Risk Level: low

## Summary

crt-025 introduces three new untrusted string fields (`phase`, `outcome`, `next_phase`) on the `context_cycle` MCP tool and an append-only `CYCLE_EVENTS` table. All SQL writes use parameterized queries throughout (no string interpolation). Input validation is centralized in `validate_cycle_params` and enforces the required constraints — length, no-spaces, lowercase normalization — before data reaches the storage layer. No injection vectors, privilege escalation paths, secrets, or access control gaps were found. One low-severity informational finding is noted regarding byte-vs-char length enforcement on the `outcome` field.

## Findings

### Finding 1: outcome length check uses byte count, not character count

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/validation.rs:431`
- **Description**: The `outcome` 512-character limit is enforced with `s.len() > MAX_OUTCOME_LEN`, which counts UTF-8 bytes, not Unicode scalar values. A caller supplying 200 four-byte emoji characters (800 bytes) would be correctly rejected. However, a caller supplying 512 four-byte characters (2048 bytes) would also be rejected, while a caller supplying 512 ASCII characters (512 bytes) would pass. The practical effect is that the effective character limit is lower than 512 for non-ASCII input. This is not an injection vector — the field is stored via a parameterized bind — but the documented 512-character limit is misleading for callers using multibyte text. The `phase`/`next_phase` fields correctly use `.len()` after `.to_lowercase()` (which is safe for ASCII-only phase tokens), so only `outcome` is affected.
- **Recommendation**: Replace `s.len() > MAX_OUTCOME_LEN` with `s.chars().count() > MAX_OUTCOME_LEN` to enforce a consistent character limit. This aligns with how `validate_cycle_params` already counts topic characters after sanitization. Not blocking — the current behavior errs on the side of rejecting more input, not accepting more, so there is no security regression.
- **Blocking**: no

### Finding 2: server.rs record_feature_entries call passes phase: None unconditionally

- **Severity**: low (informational, intentional)
- **Location**: `crates/unimatrix-server/src/server.rs:688-691` (`record_usage_for_entries`)
- **Description**: The `record_usage_for_entries` helper in `server.rs` always passes `phase: None` to `record_feature_entries`, with a comment stating this will be addressed in Wave 3 (context-store-phase-capture). This is intentional and documented. The `context_store` MCP tool handler in `tools.rs` correctly snapshots `current_phase` and passes it via `UsageContext` on the hot path. The `server.rs` path is a separate code path used by non-store operations (search, lookup, etc.) where phase tagging is not expected. No data quality or security issue — entries written via this path will have `phase = NULL`, consistent with entries written before crt-025.
- **Recommendation**: No action needed for this PR. The architectural comment is clear. Ensure Wave 3 tracks this remaining call site.
- **Blocking**: no

### Finding 3: outcome field is stored as free-form text with no sanitization beyond length

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/infra/validation.rs:427-435`
- **Description**: `outcome` is stored verbatim (after length check) with no normalization, HTML encoding, or format restriction. This is intentional per the specification (free-form field). All writes go through a parameterized SQL bind (`?5` in `insert_cycle_event`), so SQL injection is not possible. The field is never reflected back to callers as executable code. The only risk is a data-quality attack (callers stuffing arbitrary content into the audit trail), which is bounded by the 512-character limit.
- **Recommendation**: No code change needed. Document that `outcome` is user-supplied free text and consumers rendering it in a UI context should HTML-escape the value. No blocking concern for this MCP-only server.
- **Blocking**: no

## Blast Radius Assessment

The worst-case failure mode for this feature is data quality degradation rather than a security breach. If `validate_phase_field` has a subtle bug, malformed phase strings could reach `CYCLE_EVENTS` and `feature_entries.phase`, silently corrupting W3-1 GNN training labels. The `CYCLE_EVENTS` table is append-only with no foreign-key constraint on `feature_id`, so a corrupt row cannot cascade to damage other tables.

The `CategoryAllowlist` change (removing `"outcome"`) is irreversible at the API level — callers using `category = "outcome"` will receive `InvalidCategory` after this merge. Existing rows in the store with `category = "outcome"` are preserved (no DELETE migration). The blast radius here is limited to callers that have `outcome` hardcoded in their `context_store` calls. The RISK-TEST-STRATEGY.md identifies this as R-03 and all relevant tests are updated.

The `current_phase` synchronous mutation path in `handle_cycle_event` is the highest-stakes piece of this change. A race condition between the phase mutation and a concurrent `context_store` call would silently mislabel entries. The design (synchronous write inside the handler, before any `spawn`) is correct by construction. The tests `test_listener_phase_mutation_before_db_spawn` and `test_listener_cycle_phase_end_with_next_phase_updates_phase` directly verify this invariant.

If `insert_cycle_event` fails (pool timeout, disk full), the failure is logged and discarded (fire-and-forget). This is safe: the in-memory `SessionState.current_phase` has already been updated synchronously, so `context_store` calls continue to receive the correct phase. The `CYCLE_EVENTS` table may miss a row, resulting in an incomplete phase narrative — a data quality gap, not a security or correctness breach.

## Regression Risk

**CategoryAllowlist change** is the highest-regression-risk item. All test assertions previously expecting `al.validate("outcome").is_ok()` have been updated to `is_err()`, confirmed in the diff. The poison recovery path is also updated. Regression guard test `test_all_remaining_seven_categories_valid` verifies no other category was accidentally removed.

**`validate_cycle_params` signature change** (3 new parameters replacing `keywords`) affects all call sites. The diff shows all call sites in `tools.rs`, `hook.rs`, and all tests have been updated. Old callers passing `keywords` in JSON are silently discarded (no `deny_unknown_fields`), which is the intended backward compatibility behavior per C-04.

**`record_feature_entries` signature change** (new `phase: Option<&str>` parameter) affects `server.rs`, `services/usage.rs`, and integration tests. All call sites are updated in the diff. The `sqlite_parity.rs` test is updated to pass `None`.

**Schema migration v14 → v15** uses `CREATE TABLE IF NOT EXISTS` (idempotent) for `CYCLE_EVENTS` and `pragma_table_info` guard for the `ALTER TABLE ADD COLUMN phase` (idempotent). Both paths match the established codebase pattern for prior migrations. Migration integration test suite `migration_v14_to_v15.rs` covers both migration and fresh-DB paths.

**`RetrospectiveReport` struct change** (`phase_narrative` field) uses `#[serde(default, skip_serializing_if = "Option::is_none")]`, so pre-existing JSON without the field deserializes without error, and serialized JSON without phase data omits the field entirely. Backward compatibility test `test_retrospective_report_phase_narrative_backward_compat` directly verifies this.

## OWASP Assessment

| Check | Status | Notes |
|-------|--------|-------|
| Injection (SQL) | Clean | All new queries use parameterized binds. `cycle_id`, `phase`, `outcome`, `next_phase`, `feature_cycle` are never interpolated into SQL strings. |
| Injection (command/path) | N/A | No new file path or shell command operations. |
| Input validation | Clean | `phase`/`next_phase`: trimmed, lowercased, empty rejected, space rejected, max 64 chars enforced. `outcome`: max 512 bytes enforced. `topic`: existing feature-ID format validation preserved. |
| Length limits | Minor gap | `outcome` uses byte count vs character count (Finding 1, low). All other limits are adequate. |
| Broken access control | Clean | `context_cycle` requires `Capability::Write`. `context_cycle_review` requires no new capabilities. No trust boundary changes. |
| Deserialization | Clean | New `CycleParams` fields (`phase`, `outcome`, `next_phase`) are `Option<String>` with `#[serde(default)]`-compatible defaults. No deserialization of untrusted binary blobs. |
| Security misconfiguration | Clean | No new config surface. `CategoryAllowlist` change is intentional and tested. |
| Secrets | Clean | No hardcoded credentials, tokens, or API keys in the diff. |
| Error handling | Clean | Errors are logged via `tracing::warn!` and not propagated to callers as internal details. Fire-and-forget failures are silent to the caller. |
| New dependencies | Clean | No new crate dependencies introduced. |

## PR Comments

- Posted findings summary as a review comment on PR #339.
- No blocking findings: the PR is clear to merge.

## Knowledge Stewardship

- Stored: nothing novel to store — the `outcome` byte-vs-char length check pattern is a one-off inconsistency, not a recurring anti-pattern across features. The SQL parameterization and validation patterns in this PR are consistent with existing codebase conventions.
