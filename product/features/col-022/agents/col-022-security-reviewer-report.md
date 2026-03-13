# Security Review: col-022-security-reviewer

## Risk Level: low

## Summary

The col-022 changeset introduces an explicit feature cycle lifecycle mechanism (`context_cycle` MCP tool) with hook-based attribution and a schema migration (v11->v12). The code demonstrates strong security practices: shared validation between MCP and hook paths (ADR-004), parameterized SQL throughout, proper capability checks, sanitization of all untrusted inputs, and defensive error handling. No blocking findings. Two informational observations noted below.

## Findings

### Finding 1: Duplicate `update_session_keywords` implementations
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2069` and `crates/unimatrix-store/src/sessions.rs:275`
- **Description**: Two `update_session_keywords` functions exist. The listener's version uses `store.update_session()` (read-modify-write pattern), while `Store::update_session_keywords()` uses a direct `UPDATE` SQL statement. The listener calls its own local version, not the Store method. The Store method (`sessions.rs:275`) appears unused in production code. This is not a vulnerability, but dead code in the store layer could cause future confusion if someone calls the wrong version.
- **Recommendation**: Remove `Store::update_session_keywords` if unused, or consolidate to one implementation. The read-modify-write pattern in the listener is safer for data integrity.
- **Blocking**: no

### Finding 2: `set_feature_force` returns `Set` for unregistered sessions
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/session.rs:257-261`
- **Description**: When `set_feature_force` is called with a `session_id` not in the registry, it returns `SetFeatureResult::Set` even though no state was actually modified. The caller (`handle_cycle_start`) then proceeds to spawn a fire-and-forget `update_session_feature_cycle` which will attempt to persist to SQLite. If the session row also does not exist in SQLite, this is silently swallowed. The return value `Set` is misleading -- it implies the feature was set when in fact nothing happened. The logging at `debug` level helps, but the caller's control flow is based on the return value.
- **Recommendation**: Consider returning a new variant like `SessionNotFound` or at minimum changing the log level to `warn`. This is not a security issue but could mask attribution failures during debugging.
- **Blocking**: no

### Finding 3: Keywords stored without JSON validation
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:2069-2076`
- **Description**: The `update_session_keywords` function stores the keywords string as-is without verifying it is valid JSON. The test `test_update_session_keywords_malformed_json` confirms that `"not-json"` is accepted and stored. While `validate_cycle_params` in the hook handler produces a proper `Vec<String>` which is then serialized to JSON by `serde_json::to_string`, the listener's `handle_cycle_start` calls `keywords_val.to_string()` on the `serde_json::Value`, which will produce valid JSON. So in practice, malformed JSON cannot reach this path through normal code flow. The concern is only if future code paths bypass validation.
- **Recommendation**: No immediate action needed. The defense-in-depth is adequate -- validation happens upstream and the JSON value is serialized by serde_json.
- **Blocking**: no

### Finding 4: Input validation is thorough and correctly shared
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/infra/validation.rs:321-424`
- **Description**: Positive finding. The `validate_cycle_params` function is correctly shared between the MCP tool handler and the hook handler (ADR-004). The `CYCLE_START_EVENT`/`CYCLE_STOP_EVENT` constants eliminate the magic string divergence risk (R-04). Topic validation includes control character stripping, length limits, and structural validation via `is_valid_feature_id`. Keywords are truncated to 5 items and 64 chars each. This is well-designed input validation.
- **Recommendation**: None.
- **Blocking**: no

## OWASP Assessment

| Check | Result |
|-------|--------|
| Input validation | Strong. All three parameters (type, topic, keywords) validated. Control chars stripped, length enforced, structural check on topic. |
| Injection (SQL) | Not applicable. All SQL uses parameterized queries (`rusqlite::params!`, `named_params!`). No string interpolation in SQL. |
| Injection (command) | Not applicable. No shell commands executed. |
| Path traversal | Not applicable. No file path operations in the new code. |
| Deserialization | Safe. `serde_json::from_str` with typed structs. Hook handler uses defensive `.get()/.as_str()` chains with fallback to generic event on parse failure. |
| Error handling | Good. Hook always exits 0 (FR-03.7). MCP tool returns validation errors to caller. No panics in production paths. Mutex poisoning handled via `unwrap_or_else(\|e\| e.into_inner())`. |
| Access control | Correct. `context_cycle` requires `Capability::Write`. Hook connections have SessionWrite. MCP-connected agents must have Write capability. |
| Secrets | None. No hardcoded credentials, tokens, or keys in the diff. |
| Dependencies | No new dependencies introduced. |

## Blast Radius Assessment

**Worst case if the fix has a subtle bug**: A race condition in `set_feature_force` could attribute a session to the wrong feature cycle. This would cause retrospective data to be associated with the wrong feature -- incorrect but not dangerous. No data corruption, no information disclosure, no privilege escalation. The failure mode is incorrect attribution metadata, which is observable via `context_retrospective`.

**Schema migration failure**: If v11->v12 migration fails, the server refuses to start (standard migration error path). `ALTER TABLE ADD COLUMN` is atomic in SQLite, so no partial corruption. The migration has an idempotency guard (`pragma_table_info` check), handling re-run after partial failure.

## Regression Risk

- **SESSION_COLUMNS / session_from_row alignment**: The `keywords` column is appended to both `SESSION_COLUMNS` and `session_from_row`, using named column access (`row.get("keywords")`), which is immune to column ordering bugs. This is a positive design choice that eliminates the R-03 risk.
- **Existing session operations**: `insert_session`, `update_session`, `get_session` all updated consistently. The `#[serde(default)]` on `keywords` ensures backward compatibility with any serialized `SessionRecord` that lacks the field.
- **Formatter changes**: The majority of the diff (~60%) is `rustfmt` reformatting (import ordering, line wrapping). These are cosmetic and carry zero regression risk.

## PR Comments
- Posted 1 comment on PR #225
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store -- the validation patterns (shared validation function, parameterized SQL, defensive deserialization in hook handlers) are already well-established in this codebase. The `is_valid_feature_id` duplication pattern (choosing to duplicate a small private function rather than promote visibility across crates) is a reasonable trade-off documented in the code comment.
