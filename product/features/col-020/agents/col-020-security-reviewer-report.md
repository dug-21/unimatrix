# Security Review: col-020-security-reviewer

## Risk Level: low

## Summary

col-020 is an additive, read-heavy analytics feature that computes session metrics and knowledge reuse from existing observation and log data. No new external inputs, no new MCP tool parameters, no new dependencies. All SQL queries use parameterized queries via rusqlite. New computation steps are wrapped in best-effort error handling that degrades gracefully without aborting the existing retrospective pipeline. No blocking findings.

## Findings

### Finding 1: SQL Injection — Parameterized Queries Verified
- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/query_log.rs:121-161`, `crates/unimatrix-store/src/injection_log.rs:93-143`
- **Description**: Both `scan_query_log_by_sessions` and `scan_injection_log_by_sessions` build dynamic SQL with `format!` for the placeholder list, but the placeholders are positional (`?1`, `?2`, etc.) and actual session ID values are passed via `rusqlite::params_from_iter`. Session IDs never enter the SQL string directly. This is safe.
- **Recommendation**: None required.
- **Blocking**: no

### Finding 2: Deserialization of Untrusted JSON — result_entry_ids
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/knowledge_reuse.rs:20-28`
- **Description**: `parse_result_entry_ids` deserializes `result_entry_ids` (a JSON string stored in SQLite) via `serde_json::from_str::<Vec<u64>>()`. Malformed JSON returns an empty Vec with a debug log. The input originates from the system's own `QueryLogRecord::new()` constructor which serializes via `serde_json::to_string`. Even if the stored value were tampered with (direct DB edit), the worst case is zero reuse count — no panic, no error propagation. serde_json has built-in depth limits preventing stack overflow from deeply nested input.
- **Recommendation**: None required. Defense-in-depth is adequate.
- **Blocking**: no

### Finding 3: File Path Data Exposure in Session Summaries
- **Severity**: low
- **Location**: `crates/unimatrix-observe/src/session_metrics.rs:123-138`, `crates/unimatrix-observe/src/types.rs:180`
- **Description**: `top_file_zones` in `SessionSummary` exposes directory zone names (e.g., `crates/unimatrix-store/src`) derived from file paths in tool inputs. These flow through MCP to the consuming agent. Since the consuming agent already has filesystem access (it ran the original session), this does not constitute privilege escalation or information disclosure beyond the existing trust boundary.
- **Recommendation**: None required. The RISK-TEST-STRATEGY already documents this assessment.
- **Blocking**: no

### Finding 4: Division by Zero Guards
- **Severity**: low
- **Location**: `crates/unimatrix-observe/src/session_metrics.rs:94-97`
- **Description**: `compute_context_reload_pct` guards against division by zero when `total_files_in_subsequent == 0`, returning 0.0. Single-session and empty inputs return 0.0 via early return at line 47-49. The function is pure computation with no side effects. Verified by tests.
- **Recommendation**: None required.
- **Blocking**: no

### Finding 5: Idempotent Counter Updates — Race Condition Analysis
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` (lines 1340-1383 in the handler)
- **Description**: Step 17 performs a get-then-upsert-then-set pattern for `topic_deliveries`. The get and upsert are in a single `spawn_blocking` closure, so they execute atomically within the same thread. However, `Store::lock_conn()` acquires the connection mutex, and `upsert_topic_delivery` and `set_topic_delivery_counters` are separate calls releasing and re-acquiring the lock between them. In theory, a concurrent retrospective for the same topic could interleave. In practice: (a) retrospectives are user-initiated and sequential, (b) the absolute-set semantics mean the last writer wins with correct values, not corrupted values. The failure mode is benign — the final state reflects whichever retrospective ran last with its computed totals.
- **Recommendation**: Document the last-writer-wins behavior if concurrent retrospectives become possible.
- **Blocking**: no

### Finding 6: No New Dependencies
- **Severity**: informational
- **Location**: All `Cargo.toml` files
- **Description**: No new crate dependencies were added. The change uses only existing dependencies (serde_json, rusqlite, tracing, tokio, rmcp, unimatrix-core, unimatrix-store, unimatrix-observe).
- **Recommendation**: None required.
- **Blocking**: no

### Finding 7: No Hardcoded Secrets
- **Severity**: informational
- **Location**: All changed files
- **Description**: Reviewed the full diff (6,687 insertions). No API keys, tokens, passwords, or credentials found. No `.env` files modified. No sensitive data in test fixtures.
- **Recommendation**: None required.
- **Blocking**: no

### Finding 8: Error Handling — Graceful Degradation Verified
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` (lines 1238-1387)
- **Description**: All new col-020 computation steps (11-17) are wrapped in best-effort error handling. Failures produce `tracing::warn!` logs and leave the corresponding report fields as `None`. The existing retrospective pipeline output (hotspots, metrics, baselines, narratives, recommendations) is preserved regardless of new step failures. This matches the architecture specification (section "Error Propagation").
- **Recommendation**: None required.
- **Blocking**: no

## Blast Radius Assessment

**Worst case**: If the new code has a subtle bug (e.g., incorrect session grouping, wrong reuse count), the impact is limited to inaccurate analytics in the retrospective report's new optional fields (`session_summaries`, `knowledge_reuse`, `rework_session_count`, `context_reload_pct`, `attribution`). These fields are informational — no downstream automation, access control, or data mutation depends on their values. The `set_topic_delivery_counters` call could write incorrect counter values to `topic_deliveries`, but these are overwritten on the next retrospective run (idempotent absolute-set).

**Existing functionality is protected**: All new code executes AFTER the existing pipeline (steps 1-10). Errors in steps 11-17 cannot affect the existing report fields. The `session_summaries`, `knowledge_reuse`, etc. fields use `Option` types with `serde(default, skip_serializing_if)`, so pre-col-020 consumers deserializing reports will not break.

**No panic paths in production**: All new computation is pure or wrapped in error handling. Division by zero is guarded. JSON parsing failures return empty defaults. Store query failures are caught and logged.

## Regression Risk

- **Backward compatibility**: New `Option` fields on `RetrospectiveReport` with `serde(default)` and `skip_serializing_if` are backward-compatible. Pre-col-020 JSON deserializes correctly (verified by test `test_retrospective_report_deserialize_pre_col020`).
- **Existing report formatting**: `format_retrospective_report` serializes the full struct. New `None` fields are omitted from output. vnc-011 ReportFormatter will not render new fields until updated — this is a known completeness gap, not a regression.
- **Performance**: New steps involve additional Store reads (`scan_query_log_by_sessions`, `scan_injection_log_by_sessions`, `count_active_entries_by_category`, `scan_sessions_by_feature`, `discover_sessions_for_feature`). All are `spawn_blocking` and bounded by topic size (typically < 100 sessions). No unbounded loops or O(n^2) algorithms.
- **Re-export changes in `unimatrix-observe/src/lib.rs`**: Import reordering is cosmetic (alphabetization). New re-exports (`SessionSummary`, `KnowledgeReuse`, `AttributionMetadata`, `compute_session_summaries`, `compute_context_reload_pct`) are additive.

## PR Comments
- Posted 1 review comment on PR #191
- Blocking findings: no
