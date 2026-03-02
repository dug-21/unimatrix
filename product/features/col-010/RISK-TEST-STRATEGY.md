# Risk-Based Test Strategy: col-010

Feature: Session Lifecycle Persistence & Structured Retrospective
Author: col-010-agent-3-risk
Date: 2026-03-02
Schema: v4 → v5

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Schema v5 migration — `next_log_id` counter write races a concurrent restart after partial table creation | High | Med | Critical |
| R-02 | GC cascade atomicity — SESSIONS deleted but INJECTION_LOG orphans survive if transaction aborts mid-phase | High | Low | High |
| R-03 | `total_injections` accuracy — INJECTION_LOG batch writes are fire-and-forget; SessionClose reads in-memory count before in-flight writes commit | High | Med | High |
| R-04 | Abandoned session status variant — retrospective metrics contaminated if `Abandoned` filter is missing or incorrectly applied | High | Med | High |
| R-05 | Batch INJECTION_LOG write under concurrent ContextSearch — single-writer redb serialization causes latency spike if batch size grows beyond expected bounds | Med | Low | Medium |
| R-06 | Fire-and-forget ONNX embedding failure — lesson-learned entry written without vector embedding; invisible to `context_search` until next supersede | Med | Med | Medium |
| R-07 | Provenance boost applied at two callsites — divergence between `uds_listener.rs` and `tools.rs` search paths if one site is missed | Med | Med | Medium |
| R-08 | Concurrent supersede race (SR-09) — two simultaneous `context_retrospective` calls produce two active lesson-learned entries for the same feature_cycle | Med | Low | Medium |
| R-09 | `evidence_limit = 3` default truncates evidence arrays; tests asserting exact array lengths fail unless updated | Low | Low | Low |
| R-10 | P0/P1 delivery split — col-011 blocked if P0 ACs fail; P1 merged before P0 creates integration surface risk | Med | Low | Medium |
| R-11 | `session_id` input validation bypass — unsanitized session_id written to SESSIONS or propagated into INJECTION_LOG content | Med | Low | Medium |
| R-12 | Auto-outcome entry bypasses MCP validation — category allowlist or tag validation not applied, corrupt entry reaches ENTRIES table | Med | Low | Medium |
| R-13 | `trust_source = "system"` missing on auto-written entries — entries fall into `_ => 0.3` trust arm; underscored in re-ranking | Low | Low | Low |
| R-14 | lesson-learned category allowlist absent at runtime — write silently skipped, AC-20 undetected failure | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Schema v5 Migration — `next_log_id` Idempotency Under Restart

**Severity**: High
**Likelihood**: Med
**Impact**: If `next_log_id = 0` is written unconditionally after injection records already exist, the log_id counter is reset. All subsequent batch writes allocate IDs from 0, overwriting existing records. Retrospective scans return corrupt or missing injection history.

**Test Scenarios**:
1. Open a v4 store, run migration to v5. Verify `next_log_id = 0` is present in COUNTERS, SESSIONS and INJECTION_LOG tables exist, `schema_version = 5`. Insert injection records. Simulate partial restart by closing and reopening the store without bumping the schema version (test harness). Verify migration does NOT overwrite `next_log_id`.
2. Call `migrate_v4_to_v5` twice on the same open transaction (simulate repeated call). Verify `next_log_id` remains at its current value (not reset to 0) and no error is returned.
3. Open a store that is already at schema v5. Verify `migrate_if_needed()` returns immediately with no writes.
4. Insert 10 injection log batches, read back `next_log_id`. Close and reopen the store. Verify `next_log_id` is unchanged and records are intact (AC-14 regression under migration).

**Coverage Requirement**: Both the check-then-write guard (`if counters.get("next_log_id").is_none()`) and the schema version gate must be exercised. Atomicity of the full migration within one transaction must be verified by confirming all three writes (SESSIONS table open, INJECTION_LOG table open, counter write) either all succeed or all roll back.

---

### R-02: GC Cascade Atomicity — INJECTION_LOG Orphan Survival

**Severity**: High
**Likelihood**: Low
**Impact**: If `gc_sessions` deletes SESSIONS records but the INJECTION_LOG cascade deletion does not complete in the same transaction (e.g., panic or early return after Phase 4), orphaned INJECTION_LOG records accumulate. `from_structured_events()` scans these orphans on every retrospective call, degrading performance and potentially leaking stale injection data into metrics.

**Test Scenarios**:
1. Insert 3 sessions (feature_cycle "test") each with 5 injection log records. Age them past `DELETE_THRESHOLD_SECS`. Call `gc_sessions()`. Verify: SESSIONS has 0 records for those session_ids; INJECTION_LOG has 0 records for those session_ids; `GcStats.deleted_injection_log_count = 15`.
2. Insert 1 session aged past 30 days (to be deleted) and 1 session aged to 25 hours (to be timed out only). Call `gc_sessions()`. Verify: the 25-hour session is `TimedOut` but NOT deleted; its INJECTION_LOG records survive. The 30-day session and its log records are deleted.
3. Inject records for session-A (to be deleted) and session-B (to be retained). After GC, verify `scan_injection_log_by_session("session-B")` still returns all session-B records intact.
4. Verify `from_structured_events()` returns an empty result (not error) for a feature_cycle after all its sessions are GC'd.

**Coverage Requirement**: The single-transaction guarantee must be tested by verifying no partial state is possible. `GcStats` fields must be verified numerically, not just for non-zero values.

---

### R-03: `total_injections` Accuracy Under Fire-and-Forget Write Patterns

**Severity**: High
**Likelihood**: Med
**Impact**: The SESSIONS record `total_injections` field is populated from the in-memory `signal_output.injection_count` at SessionClose. INJECTION_LOG batch writes are still in-flight (fire-and-forget) at that moment. If the in-memory count and the INJECTION_LOG record count diverge (e.g., due to a write failure or ordering race), `SessionRecord.total_injections` misreports the actual injection history. col-011 consumes this field for routing quality scoring — a consistent under-count silently degrades routing decisions.

**Test Scenarios**:
1. Register a session. Simulate 5 ContextSearch injections (3 entries each = 15 total). Issue SessionClose with Success. Wait for all spawn_blocking tasks to complete. Verify: `SessionRecord.total_injections = 15`; `scan_injection_log_by_session` returns 15 records.
2. Simulate a failed INJECTION_LOG batch write (inject an error in the batch write path). Issue SessionClose. Verify: SessionClose still completes successfully; `total_injections` reflects the in-memory count; the write failure is logged at `warn` level.
3. Verify OQ-01 resolution: confirm the implementation uses `signal_output.injection_count` (in-memory) not a INJECTION_LOG database count for `total_injections` (per specification recommendation). The test documents the accepted discrepancy explicitly.

**Coverage Requirement**: The divergence between in-memory count and INJECTION_LOG record count under write failure must be explicitly tested and the accepted behavior documented. AC-03/AC-04 alone are insufficient — they verify the field is written, not that it is accurate under failure.

---

### R-04: Abandoned Session Status — Retrospective Metric Contamination

**Severity**: High
**Likelihood**: Med
**Impact**: The `Abandoned` variant (ADR-001) was added specifically to prevent abandoned sessions from inflating retrospective metrics. If `from_structured_events()` fails to filter `status == Abandoned` sessions, hotspot counts are overstated (injection events from cancelled work inflate frequency counts). The narrative synthesis layer then produces misleading recommendations based on corrupted metrics.

**Test Scenarios**:
1. Insert 3 sessions for feature_cycle "fc-test": 2 `Completed` (1 success, 1 rework) and 1 `Abandoned`. Write injection log records for all 3. Call `from_structured_events("fc-test")`. Verify: `report.session_count = 2` (abandoned excluded); injection records from the abandoned session are NOT in the computation.
2. Verify that `scan_sessions_by_feature("fc-test")` returns all 3 sessions (scan is not pre-filtered), but `from_structured_events()` applies the Abandoned filter internally.
3. Verify that a session written with `status = Abandoned` does NOT produce an auto-outcome entry in ENTRIES (AC-05 — no `type:session` entry for abandoned sessions).
4. Insert a session with `status = TimedOut`. Verify it IS excluded from `from_structured_events()` metric computation (same filter as Abandoned — timed-out sessions are not completed meaningful work).

**Coverage Requirement**: The enum match in `from_structured_events()` must be verified to exclude both `Abandoned` and `TimedOut` variants from the sessions fed into metric computation. A test with only abandoned/timed-out sessions must return an empty report, not a report with zero sessions that still runs the pipeline.

---

### R-05: Batch INJECTION_LOG Write Latency Under Concurrent Sessions

**Severity**: Med
**Likelihood**: Low
**Impact**: redb's single-writer model serializes all write transactions. If multiple ContextSearch requests arrive concurrently, each spawning a batch write task, the tasks queue behind each other. At current volumes (expected <5 entries per response, <5 concurrent sessions), this is acceptable. The risk is that unexpected batch sizes (e.g., a search returning 20+ results) or unexpected concurrency (e.g., a coordinator spawning 10 subagents simultaneously) causes measurable ContextSearch response latency despite the fire-and-forget pattern.

**Test Scenarios**:
1. Simulate 10 concurrent ContextSearch responses each injecting 5 entries (50 total records). Measure time for all `spawn_blocking` tasks to complete. Verify all 50 records are written correctly with no duplicate log_ids and no counter corruption.
2. Verify `insert_injection_log_batch([])` returns immediately without opening a write transaction (early return on empty slice — prevents unnecessary lock acquisition on zero-injection responses).
3. Verify log_ids within a batch are contiguous (start_id to start_id + n - 1) and do not overlap with IDs from a concurrent batch.

**Coverage Requirement**: Log_id uniqueness under concurrent writes. Zero-injection fast-path. Counter value after N concurrent batches equals sum of all batch sizes.

---

### R-06: Fire-and-Forget ONNX Embedding Failure — Degraded Search Visibility

**Severity**: Med
**Likelihood**: Med
**Impact**: If the ONNX embedding task fails (model load failure, OOM, timeout), the lesson-learned entry is written to ENTRIES with `embedding_dim = 0` and no VECTOR_MAP entry. The entry is invisible to `context_search` (vector similarity queries). `PROVENANCE_BOOST` still applies at re-ranking, but the entry is never in the candidate set to begin with. The caller of `context_retrospective` receives a successful response with no indication that the knowledge entry is unsearchable.

**Test Scenarios**:
1. Inject an ONNX failure (mock embed service to return error). Run `context_retrospective` with ≥1 hotspot. Verify: tool returns a valid `RetrospectiveReport`; an error is logged at `warn` level; a lesson-learned entry EXISTS in ENTRIES with `embedding_dim = 0`; the entry is retrievable via `context_lookup(category: "lesson-learned")`.
2. Run `context_retrospective` again for the same feature_cycle with a healthy embed service. Verify: the failed entry (embedding_dim=0) is superseded; the new entry has `embedding_dim > 0`; `context_search` now returns the entry.
3. Verify that `context_retrospective` response latency does not include ONNX embedding time (fire-and-forget). The response must return before the embed task completes. Test via a mock that introduces a 300ms delay in the embed path.

**Coverage Requirement**: Both the failure path (entry written without embedding) and the recovery path (second retrospective supersedes and re-embeds) must be tested. The caller-facing guarantee (retrospective always returns a report) must be verified under embed failures.

---

### R-07: Provenance Boost Applied at Two Callsites — Divergence Risk

**Severity**: Med
**Likelihood**: Med
**Impact**: `PROVENANCE_BOOST` must be applied consistently at both `uds_listener.rs` (ContextSearch hook path) and `tools.rs` (MCP context_search tool path). If one callsite is missed, lesson-learned entries rank with the boost in one code path but not the other. Agents using the hook path see different ranking behavior than agents using the MCP tool directly. AC-23 tests one path; the other may be silently inconsistent.

**Test Scenarios**:
1. Unit test: given two entries (one `lesson-learned`, one `convention`) with identical `similarity = 0.8` and `confidence = 0.6`, verify the reranking function returns `lesson-learned` score = `convention` score + 0.02` (exactly `PROVENANCE_BOOST`).
2. Integration test via MCP `context_search`: insert a lesson-learned entry and a convention entry with equal stored confidence. Run `context_search` via the MCP tool. Verify lesson-learned appears first.
3. Integration test via ContextSearch hook: same two entries, trigger a ContextSearch hook request. Verify lesson-learned appears first in injected results.
4. Verify that `PROVENANCE_BOOST` is applied at both callsites by checking the constant is referenced from both files (not duplicated as a magic number). Any `0.02` literal in the re-ranking logic should fail a code review.

**Coverage Requirement**: Both application sites must be exercised by integration tests. The constant value must be referenced from a single definition point (`confidence.rs`).

---

### R-08: Concurrent Supersede Race — Duplicate Active Lesson-Learned Entries

**Severity**: Med
**Likelihood**: Low
**Impact**: Two simultaneous `context_retrospective` calls for the same feature_cycle both read no active lesson-learned entry, both proceed to write one, producing two active entries with the same topic. On the next retrospective call, one is superseded, but until then, `context_search` may return both, providing duplicate/inconsistent retrospective findings. The crt-003 contradiction detector may flag them as contradictions.

**Test Scenarios**:
1. Simulate two concurrent retrospective calls for the same feature_cycle using `tokio::join!`. After both complete, query ENTRIES for lesson-learned entries with the given topic. Verify: at most 2 active entries exist (the known tolerated race), and subsequent retrospective call reduces to exactly 1. Document this as the accepted behavior in test comments.
2. Verify the supersede check-then-write runs synchronously (before spawning the fire-and-forget embed task) per ADR-004. This ensures the de-duplication check is not itself deferred.
3. Single-call path: verify that exactly one lesson-learned entry is active after a single `context_retrospective` call on a clean store (no pre-existing entries). No race condition possible in this case.

**Coverage Requirement**: The race is documented as a known limitation (ADR accepted). Tests must verify the tolerated upper bound (2 active entries briefly), not assert on exactly 1, to avoid flaky tests. A deterministic single-threaded test must verify the happy-path produces exactly 1 entry.

---

### R-09: `evidence_limit = 3` Default Truncates Evidence Arrays

**Severity**: Low
**Likelihood**: Low
**Impact**: The `hotspots: Vec<HotspotFinding>` type is unchanged. The only callers at risk are integration tests that assert exact evidence array lengths. Those tests fail only if they assert a count greater than 3 without passing `evidence_limit = 0`. The fix is mechanical: pass `evidence_limit = 0` in those tests or update expected count to ≤ 3. Agent logic that iterates evidence arrays continues to work — it receives fewer items by default, not absent or differently-typed fields.

**Test Scenarios**:
1. Audit existing integration tests for `context_retrospective` that assert on `hotspots[n].evidence.len()`. For each such test: either pass `evidence_limit = 0` to restore full arrays, or update expected count to ≤ 3. Complete this audit before implementing Component 6.
2. Verify `evidence_limit = 0` output is structurally identical to pre-col-010 output. Take a snapshot of a pre-col-010 `context_retrospective` response and assert field-by-field parity when `evidence_limit = 0` is passed.
3. Verify the default (`evidence_limit = 3`) response payload is ≤10KB for a synthetic feature cycle with 13 hotspots (AC-15). Serialize JSON and assert byte length.
4. Verify `evidence_limit = 3` with the structured-events path returns non-None `narratives` alongside capped evidence (AC-17).

**Coverage Requirement**: FR-10.8 mandates auditing existing tests before implementing evidence_limit. The audit is a blocking prerequisite for P1 Component 6. Both `evidence_limit = 0` (full backward compat) and `evidence_limit = 3` (default) must be exercised in integration tests. The byte-size constraint (AC-15) must be a hard assertion.

---

### R-10: P0/P1 Delivery Split — col-011 Blocking and Integration Surface

**Severity**: Med
**Likelihood**: Low
**Impact**: If P1 is merged before P0 is fully stable, the structured retrospective path (`from_structured_events()`) is live but SESSIONS/INJECTION_LOG tables may have incomplete or corrupt data (if P0 has bugs). col-011 has a hard dependency on P0 only — if P1 ships first, col-011 integration tests run against a schema that may have P1 code paths that reference P0 tables before P0 acceptance criteria pass. Additionally, the `context_retrospective` fallback logic (try structured path first, fall back to JSONL) could mask P0 data quality bugs if the fallback silently hides empty SESSIONS results.

**Test Scenarios**:
1. Gate test: verify that P0 acceptance criteria (AC-01 through AC-11 + AC-24) all pass in CI before any P1 component is merged. This is a process verification, not a code test — document in implementation brief as a merge gate.
2. Verify `context_retrospective` logs which path was used (`tracing::debug!` call). Integration test: with SESSIONS data populated, verify the log message indicates `"structured"` path. With no SESSIONS data, verify it indicates `"jsonl"` path.
3. Verify the JSONL fallback does not mask SESSIONS data quality bugs: if SESSIONS has data but `from_structured_events()` returns an error, the tool should return an error (not silently fall back to JSONL). The fallback triggers only on empty SESSIONS result, not on errors.

**Coverage Requirement**: Path selection logic must be tested for all three states: SESSIONS data present (structured path), SESSIONS empty (JSONL fallback), and `from_structured_events()` error (propagate error, do not fall back).

---

### R-11: `session_id` Input Validation Bypass

**Severity**: Med
**Likelihood**: Low
**Impact**: `session_id` values from hook callers are untrusted input from an external process (the Claude hooks). If `session_id` contains path traversal sequences, null bytes, Unicode control characters, or SQL-injection-like patterns, and these are written directly to SESSIONS as keys or interpolated into auto-outcome entry content, the database key space could be corrupted or content could contain unexpected characters that break downstream parsers.

**Test Scenarios**:
1. Send a `SessionRegister` request with `session_id = "../etc/passwd"`. Verify: request returns an error; no record is written to SESSIONS.
2. Send `session_id = "valid-session-1"` (alphanumeric + `-`). Verify: accepted and written.
3. Send `session_id = "session with spaces"`. Verify: rejected with a logged warning.
4. Send `session_id = ""` (empty string). Verify: rejected.
5. Send `session_id` of length 129 characters. Verify: rejected (exceeds SEC-01.1 128-char limit).
6. Verify that the sanitized `session_id` in SESSIONS propagates to INJECTION_LOG records without re-validation (SEC-01.3). The `session_id` in INJECTION_LOG must always be the already-sanitized version from the in-memory registry.

**Coverage Requirement**: All rejection cases must produce a logged warning and an error response, not a panic. Valid `session_id` patterns must be verified to succeed. The allowed character set (`[a-zA-Z0-9-_]`) must be tested explicitly with boundary characters.

---

### R-12: Auto-Outcome Entry Bypasses MCP Validation

**Severity**: Med
**Likelihood**: Low
**Impact**: Auto-outcome entries are written directly via `store.insert_entry()` without the MCP validation layer. If the category allowlist check is omitted or the tag validation (`validate_outcome_tags`) is not applied, entries with invalid categories, malformed tags, or unexpected content may reach the ENTRIES table. This could corrupt the category distribution seen by `context_status` or produce entries that break downstream consumers expecting structured outcome data.

**Test Scenarios**:
1. Temporarily remove `"outcome"` from the CategoryAllowlist. Trigger a SessionClose with Success + injections. Verify: no auto-outcome entry is written; a `warn!` is logged; SessionClose response is still successful.
2. Verify `validate_outcome_tags(&["type:session", "result:pass"])` returns `Ok(())` (AC-10). Verify `validate_outcome_tags(&["type:invalid"])` returns an error.
3. Verify the auto-outcome entry has `embedding_dim = 0` (no vector entry added to VECTOR_MAP). Confirm via `context_lookup(category: "outcome", tags: ["type:session"])` that the entry is returned and its `embedding_dim` field is 0.
4. Verify an auto-outcome entry is NOT written when `total_injections = 0` (FR-08.3 guard).

**Coverage Requirement**: Pre-write validation gate (category + tags) must be tested both positively (allowed) and negatively (disallowed). The `embedding_dim = 0` constraint is a hard AC requirement (AC-11) and must be asserted, not just implied.

---

### R-13: `trust_source = "system"` Missing on System-Generated Entries

**Severity**: Low
**Likelihood**: Low
**Impact**: Entries written without `trust_source = "system"` fall into the `_ => 0.3` arm of the `trust_score()` function, receiving a score of 0.3 instead of the correct 0.7. For lesson-learned entries, this reduces their stored confidence contribution from the TRUST component, partially offsetting the `PROVENANCE_BOOST`. For auto-outcome entries, it affects lookup-based scoring.

**Test Scenarios**:
1. Write a lesson-learned entry via the auto-persist path. Read it back from ENTRIES. Verify `entry.trust_source == "system"`.
2. Write an auto-outcome entry via SessionClose. Read it back. Verify `entry.trust_source == "system"`.
3. Verify the `trust_score("system")` function returns 0.7 (not the wildcard 0.3). This is a unit test against `confidence.rs`.

**Coverage Requirement**: Both auto-written entry types must be verified. The `trust_source` field must be verified on the persisted record, not just asserted at the write call site.

---

### R-14: lesson-learned Category Allowlist Absent at Runtime

**Severity**: Low
**Likelihood**: Med
**Impact**: If `"lesson-learned"` is not in the active CategoryAllowlist at runtime (e.g., allowlist state diverged from the expected initial set), FR-11.7 specifies that the lesson-learned write is silently skipped with a logged error. No retrospective knowledge is persisted. `context_search` never returns retrospective findings. AC-20 through AC-22 fail silently with no user-visible error from `context_retrospective`.

**Test Scenarios**:
1. Verify the CategoryAllowlist initial set in `allowlist.rs` contains `"lesson-learned"` (static code check). Add an integration test that checks `context_status` output lists `"lesson-learned"` as a valid category.
2. Simulate a poisoned allowlist (remove `"lesson-learned"` in test setup). Run `context_retrospective`. Verify: a `tracing::error!` is logged mentioning the missing category; the retrospective report is still returned successfully; no lesson-learned entry exists in ENTRIES.
3. Restore allowlist and verify lesson-learned writes resume on next retrospective call.

**Coverage Requirement**: Both the allowlist-present happy path (entry written) and allowlist-absent degraded path (write skipped, error logged, retrospective still succeeds) must be tested.

---

## Integration Risks

### IR-01: SessionRegister → ContextSearch → SessionClose Ordering

The UDS listener dispatches hook events sequentially, but hook callers (the Claude hooks) fire in order: `SessionStart` → `UserPromptSubmit` (ContextSearch) → potentially many more → `Stop` (SessionClose). The INJECTION_LOG write references `session_id` which must already exist in the in-memory registry for the fire-and-forget write to associate correctly. FR-07.5 specifies: if session_id is absent, write the INJECTION_LOG record anyway but log a warning. The risk is that a ContextSearch arrives before SessionRegister completes (e.g., due to `spawn_blocking` scheduling) and injection records are written for an unregistered session. These records will be orphans until the session is registered, and `from_structured_events()` may miss them.

**Test Scenario**: Send ContextSearch before SessionRegister for the same session_id. Verify the injection records are written. Then send SessionRegister. Verify `scan_injection_log_by_session` returns the pre-registration records. Run `from_structured_events()`. Verify the records are included.

### IR-02: `maintain=true` Path — GC Runs During Active Sessions

Session GC is triggered by `context_status(maintain=true)`. If called during an active development session, it may time-out sessions that are active at the 24-hour boundary. A session that spans a maintenance call (e.g., a long-running coordinator session started 25 hours ago) is marked `TimedOut` while still receiving ContextSearch events. Subsequent INJECTION_LOG writes proceed normally (no guard on TimedOut status in the injection path), but the session appears `TimedOut` in the SESSIONS table while still injecting.

**Test Scenario**: Insert an Active session with `started_at = now - 25h`. Call `gc_sessions()`. Verify status = TimedOut. Then write an INJECTION_LOG batch for that session. Verify the write succeeds. Verify `from_structured_events()` excludes the TimedOut session from metrics (consistent with Abandoned filter).

### IR-03: col-011 Dependency on P0 Tables

col-011 reads SESSIONS and INJECTION_LOG. If col-010 P0 ships with a bug in `insert_session` or `insert_injection_log_batch` that causes silent failures (fire-and-forget with suppressed errors), col-011 will observe empty or sparse SESSIONS/INJECTION_LOG and produce incorrect routing quality scores without any visible error. The fire-and-forget pattern in P0 trades error visibility for response latency.

**Test Scenario**: Verify that SESSIONS and INJECTION_LOG write errors are logged at `warn` level with sufficient context for diagnosis (session_id, operation type, error message). Verify via a test that injects a store error, that the warning is emitted and the caller-facing response succeeds.

---

## Edge Cases

### EC-01: Session with Zero Injections at Close

A session registers, never triggers a ContextSearch (or all ContextSearch calls return zero results), and then closes. `total_injections = 0`. FR-06.4 and FR-08.3 specify: no auto-outcome entry is written. SESSIONS record should still be written with `total_injections = 0`.

**Test**: Register session, close with Success, zero injections. Verify SESSIONS record written; no auto-outcome entry in ENTRIES; `from_structured_events()` includes the session in session_count but contributes zero ObservationRecords.

### EC-02: `next_log_id` Counter Overflow

`next_log_id` is `u64`. At <5,000 records/day, overflow requires ~3.7 × 10^15 years. Not a practical risk. No test required, but implementer must ensure the counter increment in `insert_injection_log_batch` does not use checked arithmetic that would panic on (theoretical) overflow — use wrapping or saturating if defensive programming is desired.

### EC-03: feature_cycle = None Sessions in Retrospective Scan

`scan_sessions_by_feature("col-010")` filters on `session.feature_cycle == Some("col-010")`. Sessions registered without a feature_cycle have `feature_cycle = None` and are correctly excluded. Verify: insert one session with `feature_cycle = None` and one with `feature_cycle = Some("col-010")`. Call `scan_sessions_by_feature("col-010")`. Verify only 1 result returned.

### EC-04: Empty `hotspots` and `recommendations` in Retrospective Output

A feature cycle with sessions and injection records but no hotspots detected (all metrics below thresholds). FR-11.1 specifies lesson-learned write requires `hotspots.len() > 0 OR recommendations.len() > 0`. With zero hotspots, no lesson-learned entry is written. Verify: `context_retrospective` returns a valid report with empty `hotspots` and `recommendations`; no lesson-learned entry is created; `context_lookup(category: "lesson-learned")` returns empty for that feature_cycle.

### EC-05: Supersede Chain with Embedding Failure on Prior Entry

If the previous lesson-learned entry was written with `embedding_dim = 0` (due to embedding failure), the supersede path writes a new entry and deprecates the old one. The new entry should have `embedding_dim > 0`. Verify: supersede completes correctly even when the prior entry has `embedding_dim = 0`. The `supersedes` field on the new entry and `superseded_by` on the old entry must be set correctly regardless of the old entry's embedding status.

### EC-06: GC Boundary Conditions — Sessions at Exactly Threshold Age

A session with `started_at = now - TIMED_OUT_THRESHOLD_SECS` (exactly 24h old): is it timed out or not? The spec says `started_at < (now - TIMED_OUT_THRESHOLD_SECS)` (strict less-than). Verify the boundary: `started_at = now - threshold` is NOT timed out; `started_at = now - threshold - 1` IS timed out. Same boundary verification for delete threshold.

---

## Security Risks

### SR-SEC-01: Untrusted Hook Input — session_id as Database Key

**Untrusted input**: `session_id` from `HookRequest::SessionRegister` (external process, hook caller).
**What damage malformed input could cause**: `session_id` is used as the redb SESSIONS table key (`&str`). redb stores keys in B-tree order. A maliciously crafted `session_id` containing null bytes or very long strings could corrupt the B-tree key ordering or cause unexpected scan behavior. The string is also interpolated into auto-outcome entry content.
**Blast radius if compromised**: Corrupt SESSIONS table keys could cause `scan_sessions_by_feature` to return incorrect results or miss records. Content injection into auto-outcome entries could produce misleading knowledge entries in the store.
**Mitigations in place**: SEC-01 specifies `session_id` validation to `[a-zA-Z0-9-_]`, max 128 chars, before any write. Verification in R-11 test scenarios.

### SR-SEC-02: Auto-Outcome Entry Content Injection via agent_role/feature_cycle

**Untrusted input**: `agent_role` and `feature_cycle` from `SessionRegister` hook request.
**What damage**: These values are interpolated into auto-outcome entry `content` string. If not sanitized, a hook caller could inject arbitrary text into knowledge entries that are then queried by future agents.
**Blast radius**: Injected content appears in `context_lookup(category: "outcome")` results. Lesson-learned entries synthesized from session context could contain the injected text. The entries are marked `trust_source = "system"` (0.7 trust score) which amplifies the impact.
**Mitigations in place**: `session_id` is sanitized (SEC-01). `feature_cycle` and `agent_role` are `Option<String>` — specification should apply the same sanitization. FR-05.2 scopes sanitization to `session_id` only; this is a gap. Test: send `agent_role = "<script>alert(1)</script>"` in SessionRegister; verify the auto-outcome entry content is sanitized or the field is treated as untrusted.

### SR-SEC-03: lesson-learned Content as Aggregated Hook-Derived Data

**Untrusted input**: `HotspotNarrative.summary` content derived from observation records which originate from hook events.
**What damage**: The lesson-learned entry content is embedded via ONNX and stored as semantic knowledge. If an adversarial agent deliberately triggers specific hotspot patterns (e.g., 1,000 permission retries in a session), the synthesized narrative ("1000 permission retries detected") pollutes the lesson-learned knowledge base and may surface as a high-confidence recommendation in future sessions.
**Blast radius**: Lesson-learned entries have `trust_source = "system"` (0.7) and `PROVENANCE_BOOST` applied at search time. Adversarial pollution of the lesson-learned category would systematically surface in `context_search` and `context_briefing` results for future agents.
**Mitigations**: Threshold-based detection (hotspots require counts above statistical thresholds) limits trivial injection. The `helpful_count` / `unhelpful_count` feedback mechanism can surface low-quality entries over time. No specific mitigation in col-010 v1 — note as a future hardening concern.

### SR-SEC-04: fire-and-forget Write Tasks — Unbound Task Queue Growth

**Untrusted input**: Rapid-fire hook events (e.g., many ContextSearch calls in quick succession).
**What damage**: Each ContextSearch spawns a `tokio::spawn` fire-and-forget task. Under a flood of hook calls (adversarial or accidental), the tokio task queue grows unbounded. This is a resource exhaustion vector — not a data corruption risk.
**Blast radius**: Server memory exhaustion under extreme hook event rates. In practice, the UDS listener processes requests sequentially, so the concurrency is bounded by the request queue depth.
**Mitigations**: `spawn_blocking` tasks are bounded by the tokio blocking thread pool. Monitor `spawn_blocking` task backlog in production. No specific mitigation needed in col-010 v1.

---

## Failure Modes

### FM-01: Store Write Failure During SessionRegister

**Expected behavior**: The `insert_session` `spawn_blocking` task logs a `warn!` with session_id and error context. `SessionRegister` response returns success to the hook caller (fire-and-forget). The session proceeds in-memory normally (registry is populated). The SESSIONS record will not exist for this session. At SessionClose, `update_session` will return `StoreError::NotFound` — log warn, proceed. No auto-outcome entry is written (no SESSIONS record to read injection_count from). `from_structured_events()` will not find this session.

### FM-02: Store Write Failure During ContextSearch Injection Log

**Expected behavior**: `insert_injection_log_batch` `spawn_blocking` task logs `warn!`. ContextSearch response is already returned to the caller (fire-and-forget). The in-memory `record_injection()` has already been called, so col-009 signal generation is unaffected. The INJECTION_LOG has a gap for this ContextSearch event. `from_structured_events()` will under-report injections for this session.

### FM-03: ONNX Embed Service Unavailable During lesson-learned Write

**Expected behavior**: `write_lesson_learned_entry` logs `warn!` and writes the entry with `embedding_dim = 0`. The entry is not added to VECTOR_MAP. `context_retrospective` has already returned its report. On the next retrospective call for the same feature_cycle, the supersede path replaces the unembedded entry with a newly embedded one.

### FM-04: `context_retrospective` Invoked Before P0 Tables Exist (Migration Not Run)

**Expected behavior**: `from_structured_events()` calls `store.scan_sessions_by_feature()`. If the SESSIONS table does not exist (pre-migration store), this returns an error (redb table-not-found). The tool should not panic — it should catch the error, log it, and fall back to the JSONL path. After schema v5 migration, the table exists and the structured path becomes available. This scenario should not occur in production (migration runs on `Store::open()`), but is possible in tests that open a raw v4 store without migration.

### FM-05: GC Transaction Abort (Disk Full / Crash)

**Expected behavior**: The 5-phase GC runs in a single write transaction. If the transaction aborts (disk full, power loss, redb internal error), all phases roll back atomically. The store returns to its pre-GC state — no partial deletions. SESSIONS and INJECTION_LOG are consistent. On the next `maintain=true` call, GC retries from the beginning.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — col-009 hard dependency | — | Gate check only: not an architecture-level risk. col-009 must be merged before col-010 implementation begins. Architectural design assumes col-009 is complete. |
| SR-02 — Feature bundle delivery risk | R-10 | ADR-006: explicit P0/P1 split. P0 (AC-01–AC-11) gates col-011. P1 (AC-12–AC-23) independent delivery path. |
| SR-03 — `context_retrospective` default behavior change | R-09 | `hotspots` type unchanged; only tests asserting exact evidence array lengths need updating (pass `evidence_limit=0` or expect ≤3). Low-impact fix. FR-10.8 mandates test audit before P1 implementation. |
| SR-04 — INJECTION_LOG orphan records on GC | R-02 | ADR-002: GC cascade in single transaction (5-phase). Atomicity verified in R-02 test scenarios. |
| SR-05 — Schema migration idempotency under restart | R-01 | Architecture §1.4: check-then-write guard on `next_log_id`. Idempotency tests in R-01 scenarios. |
| SR-06 — Abandoned session status modeling ambiguity | R-04 | ADR-001: distinct `Abandoned` variant added. `from_structured_events()` filters Abandoned + TimedOut. |
| SR-07 — ONNX embedding latency in context_retrospective | R-06 | ADR-004: fire-and-forget `tokio::spawn`. Response returns before embedding. Graceful degradation on failure. |
| SR-08 — Evidence synthesis heuristic fragility | — | Architecture: `CLUSTER_WINDOW_SECS` named constant; `sequence_pattern = None` on no match. Best-effort synthesis. No new architecture-level risks beyond the specification guardrails. |
| SR-09 — Concurrent supersede race | R-08 | ADR architecture §7.2: accepted known limitation. Tests verify tolerated upper bound (≤2 active entries briefly). |
| SR-10 — Vision doc discrepancy (session_id on EntryRecord) | — | Non-Goal confirmed in SCOPE.md, ARCHITECTURE.md open questions. Documentation correction out of scope for risk strategy. |
| SR-11 — Auto-outcome validation bypass | R-11, R-12 | SEC-01/SEC-02: session_id sanitization + category allowlist + tag validation. Both risks covered by specific test scenarios. |
| SR-12 — Counter contention on INJECTION_LOG | R-05 | ADR-003: batch writes (1 transaction per ContextSearch response). Concurrency tests in R-05. |
| SR-13 — `trust_source = "system"` scoring inconsistency | R-13 | SEC-03: all system-generated entries set `trust_source = "system"`. Test verifies field on persisted records. |
| SR-14 — lesson-learned category allowlist readiness | R-14 | FR-11.7: verify allowlist before write, log error and skip if absent. Test exercises both allowlist-present and allowlist-absent paths. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 4 scenarios |
| High | 4 (R-02, R-03, R-04, R-07) | 14 scenarios across the 4 risks |
| Medium | 6 (R-05, R-06, R-07, R-08, R-10, R-11, R-12) | 18 scenarios |
| Low | 3 (R-09, R-13, R-14) | 10 scenarios |
| **Total** | **14** | **46+ scenarios** |

**Integration test focus areas** (ordered by risk):
1. Schema migration idempotency under partial restart (R-01)
2. GC cascade atomicity with exact `GcStats` counts (R-02)
3. `total_injections` accuracy under write failure (R-03)
4. Abandoned/TimedOut session filter in `from_structured_events()` (R-04)
5. Provenance boost consistency across both search callsites (R-07)
6. `evidence_limit=0` backward compatibility snapshot test (R-09) — blocking gate for P1 Component 6

**Open questions for risk assessment**:
1. OQ-01 (`total_injections` source of truth) directly creates R-03. The specification recommends in-memory count. The test should explicitly document the accepted discrepancy under write failure.
2. OQ-03 (empty session set vs. no SESSIONS data) affects R-10 path selection logic. The specification's recommendation (JSONL fallback only when JSONL has data too) is stricter than the AC-13 wording — confirm which behavior the tester should assert.
3. The `agent_role` and `feature_cycle` sanitization gap (SR-SEC-02) is not fully addressed by SEC-01. The implementer should decide whether to sanitize or omit these fields in auto-outcome content; the tester should verify the decision is implemented.
