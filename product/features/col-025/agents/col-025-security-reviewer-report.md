# Security Review: col-025-security-reviewer

## Risk Level: medium

## Summary

col-025 adds a `goal` field to the feature cycle lifecycle — stored in `cycle_events` (schema v16), cached in `SessionState`, and used as the retrieval query for `IndexBriefingService`. The diff is well-structured: parameterized SQL binds prevent injection, input validation is layered correctly across MCP and UDS paths, and the test suite directly addresses the risk register items. One medium-severity finding requires a non-blocking fix before next merge: a raw byte slice on the `goal_text` value in a `tracing::debug!` call (`listener.rs:936`) is not char-boundary-safe and will panic on any non-ASCII goal string whose 50th byte falls in the middle of a multi-byte UTF-8 sequence.

---

## Findings

### Finding 1: Unsafe byte-index slice on `goal_text` in debug log (listener.rs:936)

- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:936`
- **Description**: The debug log line reads:
  ```
  goal_preview = %&goal_text[..goal_text.len().min(50)],
  ```
  This slices `goal_text` at byte offset `50`. `goal_text` is a `String` that may contain multi-byte UTF-8 characters — it is not constrained to ASCII. The `truncate_at_utf8_boundary` helper exists and is used for the `MAX_GOAL_BYTES` guard, but this separate `min(50)` truncation for log preview does not use it. If `goal_text` contains a non-ASCII character whose codepoint spans bytes 49–51 (or any similar straddle scenario), Rust will panic with "byte index X is not a char boundary." This code path is exercised every time a goal-present `SubagentStart` fires, making this a production-reachable panic. The `tracing::debug!` macro does not suppress panics in its argument evaluation — the slice `&goal_text[..N]` is eagerly evaluated.
- **Recommendation**: Replace the raw byte slice with a char-boundary-safe truncation:
  ```rust
  let preview = truncate_at_utf8_boundary(goal_text, 50);
  goal_preview = %preview,
  ```
  The `truncate_at_utf8_boundary` function is already in scope in `listener.rs`. Alternatively, use `goal_text.chars().take(50).collect::<String>()` for character-count-based truncation.
- **Blocking**: no — the bug is in a `tracing::debug!` call (not a `warn!` or `error!`), which means it only fires in debug-level deployments or test runs with debug logging enabled. However, it is a panic-class defect and should be fixed before next merge.

---

### Finding 2: Goal text reflected verbatim in MCP response string (tools.rs:1864–1870)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:1864`
- **Description**: The `response_text` string echoes the full `validated_goal` content back to the caller: `"Acknowledged: ... with goal: '{g}'."` The goal text has already been validated (trimmed, empty-normalized, byte-bounded at 1024), so this is not a storage or injection risk. However, goal text is caller-controlled and is reflected in the response. This is intentional acknowledgment behavior, but it means any text up to 1024 bytes (including HTML, special characters, or misleading content) appears verbatim in the tool response. This is acceptable given the MCP protocol's closed audience (the calling agent), but is noted for awareness.
- **Recommendation**: No change required. The echo is intentional and the input has been sanitized. Document in a comment that the echo is deliberate acknowledgment.
- **Blocking**: no

---

### Finding 3: Audit log correctly omits goal content — only "goal=present" logged

- **Severity**: informational (positive finding)
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:1889–1898`
- **Description**: The audit log `detail` field records `"goal=present"` rather than the goal text content. This is correct security hygiene — audit logs should not capture arbitrary user-provided text. Confirmed as intended.
- **Recommendation**: None. This is correct.
- **Blocking**: no

---

### Finding 4: SQL injection — parameterized binds confirmed safe

- **Severity**: informational (positive finding)
- **Location**: `crates/unimatrix-store/src/db.rs:327–337`
- **Description**: The `insert_cycle_event` SQL insert and the new `get_cycle_start_goal` SELECT both use positional binds (`?1`, `?2`, ...) via sqlx, not string interpolation. The `cycle_id` parameter in `get_cycle_start_goal` is bound as `?1` — no injection risk. The `goal` column bind at position `?8` is correct and confirmed by the full column-value round-trip test (T-V16-07).
- **Recommendation**: None.
- **Blocking**: no

---

### Finding 5: Migration idempotency guard uses `pragma_table_info` pre-check

- **Severity**: informational (positive finding)
- **Location**: `crates/unimatrix-store/src/migration.rs:2761–2780`
- **Description**: The v15→v16 migration pre-checks for the existence of the `goal` column via `pragma_table_info('cycle_events')` before issuing `ALTER TABLE`. This is the correct idempotency pattern for SQLite (which lacks `ADD COLUMN IF NOT EXISTS`). Test T-V16-05 covers the scenario where the column already exists.
- **Recommendation**: None. Pattern is sound.
- **Blocking**: no

---

### Finding 6: UDS path does not normalize whitespace-only goal to `None`

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:1793–1822` (handle_cycle_event)
- **Description**: The architecture explicitly documents that whitespace normalization is scoped to the MCP handler only (ADR-005 FR-11 scope = MCP only). The UDS path does NOT normalize whitespace-only goals to `None` — it stores them verbatim. This is a documented, accepted design decision. A whitespace-only goal arriving via the UDS hook path will be stored in `SessionState.current_goal` and passed as the query to `IndexBriefingService`, which will produce a low-quality retrieval result. The `SubagentStart` branch filters with `.filter(|g| !g.trim().is_empty())`, so a whitespace-only goal stored via UDS is effectively treated as absent at injection time. This is internally consistent.
- **Recommendation**: The design is intentional. No change required. The `.filter(|g| !g.trim().is_empty())` filter at the SubagentStart branch is the correct behavioral guard.
- **Blocking**: no

---

### Finding 7: `get_cycle_start_goal` ORDER BY clause and LIMIT 1 semantics

- **Severity**: low
- **Location**: `crates/unimatrix-store/src/db.rs:2706–2712`
- **Description**: The query `ORDER BY timestamp DESC, seq DESC LIMIT 1` returns the most recent `cycle_start` row for a given `cycle_id` when duplicates exist. The test (T-V16-14) verifies last-writer-wins semantics for the retry-overwrite scenario (R-13). However, the test comment at line 3785 says "LIMIT 1 must return the first row by insertion order" but the actual assertion accepts either goal. In practice the ORDER BY ensures the higher-timestamp row is returned, which is the correct behavior for the retry scenario. This is not a security risk but is a minor documentation inconsistency in the test comment.
- **Recommendation**: No code change needed. The ORDER BY clause is correct for the intended semantics.
- **Blocking**: no

---

### Finding 8: Goal text reaches embedding pipeline — adversarial input surface

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/index_briefing.rs` (derive_briefing_query, IndexBriefingService::index)
- **Description**: The goal string (max 1024 bytes, caller-controlled) becomes the text input to the ONNX embedding model via `IndexBriefingService`. A crafted adversarial goal string could skew vector similarity rankings for all retrieval queries in a feature cycle. Blast radius is limited to retrieval quality within that session's cycle — no external data exfiltration exists, and the 1024-byte cap constrains the attack surface. This matches the threat model described in RISK-TEST-STRATEGY.md §Security Risks.
- **Recommendation**: No additional mitigation needed at this time. `MAX_GOAL_BYTES = 1024` is the accepted bound per ADR-005.
- **Blocking**: no

---

### Finding 9: No hardcoded secrets in diff

- **Severity**: informational (positive finding)
- **Description**: The diff contains no hardcoded API keys, tokens, passwords, or credentials. All new constants are non-sensitive (`MAX_GOAL_BYTES = 1024`, `CONTEXT_GET_INSTRUCTION` text).
- **Blocking**: no

---

## Blast Radius Assessment

**Worst-case scenario if the fix at listener.rs:936 has a subtle production panic:** Any session with a goal string containing non-ASCII characters in the first 50 bytes will cause the `SubagentStart` hook handler to panic at debug-log evaluation time. Depending on the build profile and log level, this could crash the UDS listener for that session. The UDS listener runs requests in `tokio::spawn` tasks; a panic in a spawned task causes that task to terminate but does not crash the entire Unimatrix server process. However, the SubagentStart hook response would be lost, and the spawning agent would receive no injection context.

**Worst-case for the broader feature:** If `synthesize_from_session` (now returning `current_goal.clone()`) is called with an unexpected `SessionState`, the `O(1)` clone is safe — it cannot fail. The only async/fallible path (session resume DB lookup) degrades gracefully per ADR-004. The migration idempotency guard prevents duplicate column errors. The parameterized SQL binds cannot produce injection.

---

## Regression Risk

**High-confidence areas (well-tested):**
- Schema migration v15→v16: 6-scenario integration test file (`migration_v15_to_v16.rs`) with idempotency, column binding assertion, and last-writer-wins coverage.
- `derive_briefing_query` step 2 replacement: Old topic-signal synthesis tests updated; new goal-driven tests added. The no-goal path (step 3 fallback) is explicitly tested to be identical to pre-col-025 behavior (R-09).
- `format_index_table` header change: `strip_briefing_header` helper introduced; all existing assertions updated via the helper.
- `insert_cycle_event` signature change: All call sites in the diff are updated. No unmodified call sites remain (confirmed by grep results showing only listener.rs and test files).

**Lower-confidence areas:**
- R-04 / SubagentStart goal-present branch: The test (`test_subagent_start_goal_present_routes_to_index_briefing`) verifies the log line is emitted but cannot verify the full round-trip response because the test environment has no embedding model. The test degrades gracefully (empty entries → falls through to ContextSearch). This is a known test environment limitation, not a regression risk per se.
- The `sqlite_parity_specialized.rs` file was audited and contains no schema version assertions referencing 15 — confirmed clean.

---

## PR Comments

Posted 1 comment on PR #375 with the key finding (Finding 1, unsafe byte slice in debug log).

---

## Knowledge Stewardship

- Stored: nothing novel to store — the unsafe byte-index slice pattern on user-provided strings before log emission is a specific instance of the "truncate at char boundary" requirement already present in pattern #R-07 of this feature's own risk register. It is not a cross-feature generalizable anti-pattern beyond what is already documented. The `truncate_at_utf8_boundary` helper was introduced in this same PR to address the UDS truncation path; it should have been used here too.
