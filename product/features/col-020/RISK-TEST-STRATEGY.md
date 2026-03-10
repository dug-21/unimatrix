# Risk-Based Test Strategy: col-020

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | JSON parsing of `result_entry_ids` in query_log produces incorrect reuse counts on malformed data | Med | High | High |
| R-02 | Knowledge reuse undercounts when query_log or injection_log has gaps for pre-nxs-010 or hook-failure sessions | Med | High | High |
| R-03 | Low attribution coverage (SR-07) silently degrades all cross-session metrics without consumer awareness | High | Med | High |
| R-04 | Server-side knowledge reuse computation (C3) bypasses ObservationSource abstraction, making it untestable without full Store setup | Med | High | High |
| R-05 | Repeated retrospective runs produce incorrect topic_deliveries counters if set_topic_delivery_counters is not truly idempotent | High | Med | High |
| R-06 | File path extraction mapping misses Grep tool or future file-touching tools, producing incomplete file zone and reload metrics | Med | Med | Med |
| R-07 | Session ordering by earliest observation timestamp breaks for concurrent sessions with identical timestamps | Low | Med | Med |
| R-08 | Rework outcome detection via substring match produces false positives on free-form outcome text containing "result:rework" or "result:failed" in unexpected contexts | Low | High | Med |
| R-09 | New optional fields on RetrospectiveReport break backward-compatible deserialization if serde attributes are incorrect | High | Low | Med |
| R-10 | Empty topic (zero sessions/observations) causes panic or error in new computation paths that assume non-empty data | High | Med | High |
| R-11 | Batch SQL queries with large IN clauses (>100 session IDs) fail or degrade on SQLite | Med | Low | Low |
| R-12 | Knowledge reuse double-counts entries that appear in both query_log and injection_log for the same cross-session pair | Med | Med | Med |
| R-13 | context_reload_pct division by zero when no files are read in sessions after the first | Med | Med | Med |
| R-14 | New computation steps fail and abort the entire retrospective instead of degrading gracefully | High | Med | High |
| R-15 | Directory prefix extraction produces inconsistent zone names across different path formats (relative vs absolute, trailing slash) | Low | Med | Low |

## Risk-to-Scenario Mapping

### R-01: JSON Parsing of result_entry_ids

**Severity**: Med | **Likelihood**: High
**Impact**: Malformed JSON in query_log rows silently inflates or deflates tier1_reuse_count. Consumers make incorrect assessments of Unimatrix's cross-session value.

**Test Scenarios**:
1. query_log row with `result_entry_ids = "not json"` -- parser returns empty set, row contributes zero entries, no panic
2. query_log row with `result_entry_ids = ""` (empty string) -- returns empty set
3. query_log row with `result_entry_ids = "null"` -- returns empty set
4. query_log row with `result_entry_ids = "[1, \"not_a_number\", 3]"` -- mixed types handled gracefully (skip non-integers or fail entire row)
5. query_log row with valid `result_entry_ids = "[1,2,3]"` -- returns {1, 2, 3}

**Coverage Requirement**: Unit tests for JSON parsing edge cases. Integration test confirming malformed rows do not abort knowledge reuse computation.

### R-02: Knowledge Reuse Undercounting from Data Gaps

**Severity**: Med | **Likelihood**: High
**Impact**: Topics spanning the nxs-010 migration boundary report artificially low reuse. Pre-nxs-010 sessions have no query_log data, so search-based reuse reports 0 for those sessions even if entries were reused.

**Test Scenarios**:
1. Topic with 3 sessions: session A (pre-nxs-010, no query_log), session B (has query_log referencing entry from A), session C (has injection_log referencing entry from A) -- reuse count includes B and C signals, not blocked by A's missing data
2. Topic where all sessions lack query_log data -- tier1_reuse_count is 0, not an error
3. Topic where injection_log is empty but query_log shows cross-session retrieval -- reuse counts from query_log alone

**Coverage Requirement**: Integration test with mixed data availability across sessions.

### R-03: Low Attribution Coverage Degrades Metrics

**Severity**: High | **Likelihood**: Med
**Impact**: If only 3 of 10 sessions are attributed, session summaries cover 30% of activity, knowledge reuse undercounts, and reload rate is computed over partial data. Without AttributionCoverage metadata, consumers cannot assess this.

**Test Scenarios**:
1. Topic with 10 sessions, only 3 with direct feature_cycle match -- AttributionCoverage reports attributed_sessions=3, total_sessions=10
2. Topic with 100% attribution -- attributed_sessions equals total_sessions
3. Topic with 0 attributed sessions (all fallback) -- attributed_sessions=0, total_sessions=N

**Coverage Requirement**: Integration test verifying AttributionCoverage values. Unit test verifying the struct is present and correctly populated.

### R-04: Server-Side Knowledge Reuse Testability

**Severity**: Med | **Likelihood**: High
**Impact**: Knowledge reuse logic lives in the handler (C3), not in a testable module. Bugs in cross-table join logic require full integration test setup to reproduce.

**Test Scenarios**:
1. Integration test: seed Store with entries in session A, query_log returning those entries in session B -- verify tier1_reuse_count
2. Integration test: seed entries across 3 categories, verify by_category breakdown matches
3. Integration test: seed active entries in categories never referenced in any session -- verify category_gaps includes those categories
4. Entry stored and retrieved in the SAME session -- should NOT count as reuse (same-session exclusion)

**Coverage Requirement**: Integration tests covering the core reuse logic. The server-side location per ADR-001 (#383 precedent) means unit tests are not possible for this component.

### R-05: Non-Idempotent Counter Updates

**Severity**: High | **Likelihood**: Med
**Impact**: Running retrospective twice on the same topic doubles the counter values, producing incorrect topic_deliveries data consumed by dashboards and other features.

**Test Scenarios**:
1. Run retrospective on topic X -- verify topic_deliveries counters match computed values
2. Run retrospective on topic X a second time with identical data -- verify counters are unchanged (idempotent)
3. Run retrospective on topic X after adding new session data -- verify counters reflect updated totals, not accumulated sums
4. Run retrospective on topic with no pre-existing topic_deliveries record -- verify record is created via upsert before setting counters

**Coverage Requirement**: Integration test with double-run verification. This directly validates ADR-002.

### R-06: File Path Extraction Mapping Gaps

**Severity**: Med | **Likelihood**: Med
**Impact**: Missing tools in the mapping produce incomplete top_file_zones and deflated context_reload_pct. The spec FR-01.4 lists Read/Edit/Write/Glob but the architecture ADR-004 also includes Grep. If Grep is omitted, search-heavy sessions underreport file zones.

**Test Scenarios**:
1. ObservationRecord with tool="Grep", input=`{"path": "/foo/bar"}` -- file path extracted (if Grep is in mapping) or silently skipped (if not)
2. ObservationRecord with unknown tool "NewTool", input=`{"file_path": "/foo"}` -- silently skipped, no panic
3. ObservationRecord with tool="Read", input missing `file_path` key -- returns None, no error
4. ObservationRecord with tool="Read", input `file_path` is a number not a string -- returns None

**Coverage Requirement**: Unit tests for extract_file_path covering all mapped tools plus unknown tool fallback.

### R-07: Concurrent Session Ordering

**Severity**: Low | **Likelihood**: Med
**Impact**: Two sessions with identical earliest timestamps are ordered non-deterministically, causing context_reload_pct to vary between runs.

**Test Scenarios**:
1. Two sessions with identical started_at -- ordered by session_id lexicographically, reload computed consistently
2. Three sessions with distinct started_at -- ordered chronologically regardless of insertion order

**Coverage Requirement**: Unit test for session ordering with identical timestamps.

### R-08: Rework Outcome False Positives

**Severity**: Low | **Likelihood**: High
**Impact**: Sessions with outcome text like "investigation of result:failed pattern" would count as rework even if the session itself was successful.

**Test Scenarios**:
1. Session with outcome "result:rework" -- counted as rework
2. Session with outcome "result:failed" -- counted as rework
3. Session with outcome "result:pass" -- NOT counted as rework
4. Session with outcome containing both "result:pass" and "result:rework" -- counted as rework (substring match)
5. Session with NULL outcome -- NOT counted as rework
6. Session with empty string outcome -- NOT counted as rework

**Coverage Requirement**: Unit test with outcome string variations.

### R-09: Backward-Compatible Deserialization

**Severity**: High | **Likelihood**: Low
**Impact**: Existing consumers deserializing RetrospectiveReport JSON without new fields panic or error, breaking vnc-011 ReportFormatter and any cached reports.

**Test Scenarios**:
1. Deserialize pre-col-020 JSON (without session_summaries, knowledge_reuse, etc.) into updated struct -- all new fields are None
2. Serialize report with new fields present, then deserialize -- round-trip preserves all data
3. Serialize report with new fields as None -- JSON output omits those fields entirely (skip_serializing_if)

**Coverage Requirement**: Unit test with pre-col-020 JSON fixture. Validates Unimatrix pattern #646 (backward-compatible config extension via serde(default)).

### R-10: Empty Topic Handling

**Severity**: High | **Likelihood**: Med
**Impact**: Zero sessions or zero observation records cause index-out-of-bounds, division by zero, or unwrap-on-None in new computation paths.

**Test Scenarios**:
1. Topic with zero sessions discovered -- all new fields are None, existing cached behavior preserved
2. Topic with sessions but zero observation records -- session_summaries is empty vec (not None), knowledge_reuse computes from injection_log/query_log only
3. Single session with one observation record -- session_summaries has 1 entry, reload_pct is 0.0, knowledge_reuse has tier1=0

**Coverage Requirement**: Unit tests for empty/minimal input to every computation function.

### R-11: Large IN Clause SQL Performance

**Severity**: Med | **Likelihood**: Low
**Impact**: Topics with >100 sessions produce SQL `WHERE session_id IN (...)` with 100+ parameters. SQLite handles this but may degrade.

**Test Scenarios**:
1. Batch query with 50 session IDs -- returns correct results
2. Batch query with 0 session IDs -- returns empty result, no SQL error

**Coverage Requirement**: Unit test for boundary (0 sessions). Architecture notes chunking at 50 for degenerate case.

### R-12: Double-Counting Reuse Across Data Sources

**Severity**: Med | **Likelihood**: Med
**Impact**: An entry stored in session A, returned by search in session B, AND injected in session B would count as 1 reuse event. If the implementation counts query_log and injection_log separately without deduplication, it counts as 2.

**Test Scenarios**:
1. Entry X stored in session A, appears in both query_log result_entry_ids and injection_log for session B -- tier1_reuse_count is 1 (deduplicated by entry ID)
2. Entry X stored in session A, appears in query_log for session B and injection_log for session C -- tier1_reuse_count is 1 (distinct entry, not distinct retrieval events)

**Coverage Requirement**: Integration test with overlapping query_log and injection_log references to same entry.

### R-13: Division by Zero in context_reload_pct

**Severity**: Med | **Likelihood**: Med
**Impact**: If sessions after the first session read zero files, the denominator for reload percentage is zero, causing NaN or panic.

**Test Scenarios**:
1. Multi-session topic where sessions after the first have zero file reads -- reload_pct is 0.0, not NaN
2. Single-session topic -- reload_pct is 0.0 (no "prior session" exists)
3. Multi-session topic where all sessions read the same files -- reload_pct approaches 1.0

**Coverage Requirement**: Unit test for zero-file-read sessions and single-session topic.

### R-14: New Steps Abort Existing Pipeline

**Severity**: High | **Likelihood**: Med
**Impact**: An error in session summary computation, knowledge reuse, or counter update propagates and aborts the entire retrospective, losing the valuable existing pipeline output (hotspots, metrics, narratives).

**Test Scenarios**:
1. Knowledge reuse computation fails (e.g., Store query error) -- existing report fields are intact, knowledge_reuse is None, warning logged
2. Session summary computation panics on malformed input -- caught by spawn_blocking boundary, report returns without session data
3. Counter update fails (topic_deliveries row missing) -- report still returns, counter failure logged

**Coverage Requirement**: Integration test simulating failure in each new step, verifying existing report fields survive.

### R-15: Inconsistent Directory Prefix Extraction

**Severity**: Low | **Likelihood**: Med
**Impact**: Paths like `/workspaces/unimatrix/crates/foo/bar.rs` and `crates/foo/bar.rs` produce different zone names, splitting what should be one zone into two.

**Test Scenarios**:
1. Absolute path `/workspaces/unimatrix/crates/store/src/lib.rs` -- zone is `crates/store/src` (3 components from workspace root)
2. Relative path `crates/store/src/lib.rs` -- zone matches absolute path zone
3. Path with trailing slash `/workspaces/unimatrix/crates/store/src/` -- same zone as file in that directory

**Coverage Requirement**: Unit test for directory prefix normalization.

## Integration Risks

- **C1 (session_metrics) <-> C6 (handler)**: Session summaries are computed from ObservationRecord arrays. If the handler passes records that include PostToolUse events, tool_distribution counts will double (PreToolUse + PostToolUse). The specification says PreToolUse events only (FR-01.2) but the handler loads all observation records. Filtering must happen in C1 or the caller.
- **C3 (knowledge reuse) <-> C4 (Store batch APIs)**: The knowledge reuse logic joins data from scan_query_log_by_sessions, scan_injection_log_by_sessions, and entry metadata. If scan_query_log_by_sessions returns QueryLogRecords with different field semantics than expected (e.g., result_entry_ids already parsed vs raw JSON string), the join fails silently.
- **C4 (set_topic_delivery_counters) <-> existing update_topic_delivery_counters**: Two Store methods modify the same table row. If other code paths still use the additive method concurrently with retrospective's absolute-set method, counters become inconsistent.
- **C2 (types) <-> vnc-011 (ReportFormatter)**: ReportFormatter renders RetrospectiveReport as markdown. New optional fields (session_summaries, knowledge_reuse) will be absent from formatted output until vnc-011 is updated. Not a bug, but a completeness gap.

## Edge Cases

- **Single observation record in a session**: duration_secs = 0 (max ts == min ts). Valid but potentially confusing.
- **Session with only SubagentStart events**: tool_distribution has only "spawn" category. knowledge_in = 0, knowledge_out = 0. No file zones. Valid edge case.
- **Entry stored and retrieved in same session**: Must NOT count as cross-session reuse. Session A != Session B check is critical.
- **Entry created outside the topic but retrieved within it**: Should this count as reuse? The spec says entries "stored in session A within the topic" -- entries from other topics are excluded. The implementation must filter by origin topic.
- **All sessions have the same set of files read**: context_reload_pct approaches 1.0 but should equal 1.0 exactly for full overlap.
- **Topic with one session containing thousands of observation records**: Performance bound -- compute_session_summaries iterates all records. Should be O(n) not O(n^2).
- **query_log row with result_entry_ids containing duplicate entry IDs**: `"[1,1,1,2]"` -- should deduplicate to {1, 2} for reuse counting.

## Security Risks

- **File path data in session summaries**: top_file_zones expose directory structures from the user's workspace. This data flows through MCP to the consuming agent. Since the consuming agent already has filesystem access (it ran the session), this is not an escalation. No new attack surface.
- **JSON parsing of tool inputs**: extract_file_path parses the `input` serde_json::Value. Malicious or deeply nested JSON could cause stack overflow in serde_json. Mitigation: serde_json handles this gracefully with depth limits. Risk is negligible.
- **SQL injection via session_id strings**: Session IDs are passed to SQL IN clauses. If using parameterized queries (rusqlite named_params!), injection is not possible. If string-interpolated, session IDs containing SQL metacharacters could corrupt queries. The architecture uses rusqlite parameterized queries (pattern #372).
- **Blast radius**: All new computation runs within the existing spawn_blocking boundary. A panic in new code kills that task but does not crash the MCP server process.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| query_log table missing (pre-nxs-010 schema) | Knowledge reuse computation should catch the SQLite error and return None for knowledge_reuse field. Existing report is unaffected. |
| injection_log table empty | Knowledge reuse computes from query_log alone. tier1_reuse_count reflects search-based reuse only. |
| No sessions have outcome data | rework_session_count = 0. Not an error -- the metric is simply zero. |
| topic_deliveries record does not exist for topic | Handler creates record via upsert_topic_delivery before calling set_topic_delivery_counters. If upsert fails, counter update is skipped with warning. |
| ObservationRecord with missing/null session_id | Record should be excluded from session grouping. If included, it creates a phantom session with empty session_id. |
| Store read transaction fails mid-computation | New steps fail gracefully (None fields). Existing pipeline output from earlier steps is preserved. |
| serde deserialization of old report format | New Option fields default to None via serde(default). No breakage. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (JSON parsing fragility for result_entry_ids) | R-01 | Architecture specifies serde_json::from_str with fallback to empty set on parse failure. Logged at debug level. |
| SR-02 (Cross-table join combinatorial blowup) | R-11 | Architecture bounds batch queries to chunked IN clauses (batches of 50). Rust-side join is O(sessions * entries) bounded by typical topic size (<100 sessions). |
| SR-03 (Rework outcome substring matching brittleness) | R-08 | Specification enumerates exact patterns: `result:rework` and `result:failed` (case-sensitive). False-positive trade-off documented. |
| SR-04 (File path extraction from heterogeneous tool inputs) | R-06 | ADR-004 defines explicit tool-to-field mapping. Unknown tools return None silently. Fail-safe: undercounting, not false data. |
| SR-05 (injection_log gaps causing reuse undercounting) | R-02 | Architecture specifies graceful degradation: missing data produces conservative (lower) counts. NFR-04 codifies this. |
| SR-06 (Concurrent sessions breaking N/N+1 model) | R-07 | Architecture defines: concurrent sessions (identical started_at) ordered by session_id lexicographically. Neither is "prior" to the other when identical. |
| SR-07 (Attribution quality bounding metric accuracy) | R-03 | ADR-003 adds AttributionCoverage metadata to report. Consumers compare attributed_sessions to total_sessions. |
| SR-08 (Server-side computation precedent) | R-04 | ADR-001 documents the exception. Rule of thumb: ObservationRecord-only computation in observe; Store joins in server. |
| SR-09 (Non-idempotent counter updates) | R-05 | ADR-002 specifies absolute-set via set_topic_delivery_counters. No additive increment. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 6 (R-01, R-02, R-03, R-04, R-05, R-10, R-14) | 22 scenarios |
| Medium | 5 (R-06, R-07, R-08, R-12, R-13) | 15 scenarios |
| Low | 2 (R-11, R-15) | 5 scenarios |
| **Total** | **15** | **42 scenarios** |
