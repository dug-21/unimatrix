# Risk-Based Test Strategy: col-009

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `drain_and_signal_session` race with stale sweep produces double Helpful signals for the same session | High | Low | High |
| R-02 | Schema v4 migration corrupts or loses existing entries when opening a v3 database | High | Low | High |
| R-03 | Rework detection false positive flags a normal development iteration as rework, causing useful entries to be flagged incorrectly | High | Med | High |
| R-04 | `ExplicitUnhelpful` intercept logic fails: entries explicitly voted unhelpful still receive implicit Helpful signals | High | Low | High |
| R-05 | `drain_signals` deletes records before consumers process them — crash between drain read and delete loses signals | Med | Low | Med |
| R-06 | SIGNAL_QUEUE cap logic drops newest records instead of oldest, losing recent signal data | Med | Low | Med |
| R-07 | `PendingEntriesAnalysis` grows unbounded (context_retrospective never called in long-running server) | Med | Med | Med |
| R-08 | Stale session sweep uses wall-clock time incorrectly — sessions swept too early or too late | Med | Med | Med |
| R-09 | PostToolUse JSON field extraction fails silently — rework events not recorded, threshold never crossed even in genuine rework | Med | High | High |
| R-10 | bincode v2 field order violation: SignalRecord fields reordered in a future change, causing deserialization failures in the drain window | Med | Low | Med |
| R-11 | `run_confidence_consumer` applies `helpful_count` increment to a deleted entry — entry lookup fails, crashes consumer | Low | Med | Low |
| R-12 | `entries_analysis` JSON includes `"entries_analysis": null` instead of omitting the field, breaking MCP clients that check field absence | Low | Low | Low |
| R-13 | Stale session with no injection history generates an empty Helpful SignalRecord (zero entry_ids) that passes through consumers without effect | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Race Between drain_and_signal_session and Stale Sweep

**Severity**: High
**Likelihood**: Low
**Impact**: Entry `helpful_count` incremented twice in the same session — Wilson score integrity violated for affected entries.

**Test Scenarios**:
1. Unit test: call `sweep_stale_sessions()` and `drain_and_signal_session(same_session_id)` on separate threads, assert the session appears in exactly one output (not both).
2. Unit test: populate a session with 3 injected entries; call `drain_and_signal_session` twice in sequence; assert second call returns `None` and SIGNAL_QUEUE contains exactly one record.
3. Integration test: verify that the `signaled_entries` HashSet is fully populated before session removal — no partial-signal state observable.

**Coverage Requirement**: `drain_and_signal_session` must be provably atomic (single lock scope). The test must confirm the session cannot be observed by `sweep_stale_sessions` after `drain_and_signal_session` starts.

---

### R-02: Schema Migration Corruption

**Severity**: High
**Likelihood**: Low
**Impact**: Loss of existing entries or indexes after migration — catastrophic data loss.

**Test Scenarios**:
1. Integration test: open a test database at schema v3 with 10 pre-populated entries; call `Store::open()` (which runs `migrate_if_needed()`); assert all 10 entries are intact, all 14 existing tables accessible, schema_version == 4, SIGNAL_QUEUE table exists, `next_signal_id == 0`.
2. Integration test: open an already-v4 database; call `Store::open()` again; assert `migrate_if_needed()` is a no-op (no writes performed, schema_version still 4).
3. Integration test: simulate a partially-migrated state (schema_version == 3 but SIGNAL_QUEUE already exists) — migration should be idempotent (write `next_signal_id = 0` only if key absent).
4. Unit test: assert `CURRENT_SCHEMA_VERSION == 4`.

**Coverage Requirement**: Migration tested against a real redb v3 database snapshot with populated entries. No test may use an empty database only.

---

### R-03: Rework Detection False Positive

**Severity**: High
**Likelihood**: Med
**Impact**: Good entries flagged incorrectly. Human review burden increases. Trust in the system erodes.

**Test Scenarios**:
1. Unit test: 2 edit-fail-edit cycles on the same file (threshold is 3) → `has_crossed_rework_threshold() == false`.
2. Unit test: 3 edit-fail-edit cycles on the same file → `has_crossed_rework_threshold() == true`.
3. Unit test: 5 rapid Edit events on the same file with NO intervening Bash failure → threshold NOT crossed (rapid multi-edit is not rework per ADR-002).
4. Unit test: 3 edit-fail-edit cycles but on DIFFERENT files (one cycle per file) → threshold NOT crossed (per-file tracking).
5. Unit test: Edit-Bash(fail)-Edit-Bash(fail)-Edit pattern (3 edits, 2 failures) → threshold NOT crossed until the 3rd failure-separated edit pair.
6. Integration test: realistic hook JSON with MultiEdit containing 3 paths → each path tracked independently.

**Coverage Requirement**: Both the positive (rework) and negative (normal iteration) cases must be covered. The threshold boundary (exactly 2 cycles = pass, exactly 3 cycles = fail) must be tested.

---

### R-04: ExplicitUnhelpful Intercept Failure

**Severity**: High
**Likelihood**: Low
**Impact**: Entries explicitly voted unhelpful by the agent receive an implicit Helpful signal, partially counteracting the agent's explicit feedback. Confidence pipeline integrity violated.

**Test Scenarios**:
1. Unit test: session with 3 injected entries (ids 1, 2, 3); record_agent_action(entry_id=2, ExplicitUnhelpful); drain_and_signal_session(outcome="success") → assert SignalOutput.helpful_entry_ids == [1, 3] (id 2 excluded).
2. Unit test: session with all 3 entries having ExplicitUnhelpful → helpful_entry_ids is empty → no SignalRecord written.
3. Unit test: session with ExplicitHelpful (not ExplicitUnhelpful) → entry NOT excluded from helpful set.
4. Integration test: simulate agent explicit `helpful=false` vote triggering record_agent_action, then Stop hook → verify helpful_count for that entry not incremented.

**Coverage Requirement**: The exclusion logic in `drain_and_signal_session` must be tested against all AgentActionType variants. Only ExplicitUnhelpful excludes.

---

### R-05: drain_signals Crash Window (Soft Durability)

**Severity**: Med
**Likelihood**: Low
**Impact**: Signals written to SIGNAL_QUEUE but not applied to helpful_count — lost signals. Wilson sample smaller than expected.

**Test Scenarios**:
1. Unit test: `drain_signals` on an empty queue returns empty Vec without error.
2. Unit test: `drain_signals` called twice in sequence — second call returns empty Vec (records deleted after first drain).
3. Integration test: insert 5 Helpful signals, crash simulate (stop before drain), re-open store, call drain → records still present (redb durability). This validates that signals survive server restart within the 50ms crash window.

**Coverage Requirement**: Document the soft-durability tradeoff in test. Verify redb's write-ahead-log ensures records written before drain are not lost on crash (redb's own guarantee).

---

### R-06: SIGNAL_QUEUE Cap — Oldest vs Newest Deletion

**Severity**: Med
**Likelihood**: Low
**Impact**: Recent signals (higher signal_id) deleted instead of oldest, causing newest session outcomes to be discarded.

**Test Scenarios**:
1. Unit test: insert 10,001 signals with distinct signal_ids; call `drain_signals`; assert exactly 10,000 records returned and the record with the LOWEST signal_id is absent (oldest dropped).
2. Unit test: signal_ids are monotonically increasing — verify `insert_signal` uses COUNTERS-based allocation (not random keys).
3. Unit test: `signal_queue_len()` returns correct count after cap enforcement.

**Coverage Requirement**: Cap logic must be tested at exactly 10,000 (no-op), 10,001 (one drop), and 11,000 (1,000 drops) to verify boundary behavior.

---

### R-07: PendingEntriesAnalysis Unbounded Growth

**Severity**: Med
**Likelihood**: Med
**Impact**: Server memory grows without bound if `context_retrospective` is never called. In a session with many rework events, this could exhaust heap.

**Test Scenarios**:
1. Unit test: insert 1,001 entries into PendingEntriesAnalysis; assert len == 1,000 after cap enforcement.
2. Unit test: when cap drops an entry, the entry with the LOWEST rework_flag_count is dropped (not a random entry).
3. Integration test: long-running server simulation — generate Flagged signals for 2,000 distinct entries; drain PendingEntriesAnalysis; assert exactly 1,000 entries with the highest rework_flag_counts retained.

**Coverage Requirement**: Cap enforcement tested at boundary (1,000, 1,001, 2,000).

---

### R-08: Stale Session Sweep Timing

**Severity**: Med
**Likelihood**: Med
**Impact**: Sessions swept too early (active sessions prematurely evicted, losing injection history) or too late (orphaned sessions accumulate, memory grows).

**Test Scenarios**:
1. Unit test: session with `last_activity_at = now - 3h` (below 4h threshold) → NOT swept.
2. Unit test: session with `last_activity_at = now - 4h - 1s` (above 4h threshold) → swept.
3. Unit test: `last_activity_at` is updated correctly by `record_injection()` and `record_rework_event()`.
4. Unit test: `last_activity_at` initialized to registration time in `register_session()`.
5. Integration test: session with injections 3h ago but rework event 1h ago → NOT swept (rework event updates last_activity_at).

**Coverage Requirement**: `last_activity_at = max(registration, last_injection, last_rework)` invariant must be tested for each update path.

---

### R-09: PostToolUse JSON Field Extraction Failures

**Severity**: Med
**Likelihood**: High (Claude Code hook JSON format may vary)
**Impact**: Rework events not recorded. Genuine rework sessions classified as success. Useful entries receive incorrect confidence boosts.

**Test Scenarios**:
1. Unit test: Bash PostToolUse JSON with `exit_code: 1` → `had_failure = true`.
2. Unit test: Bash PostToolUse JSON with `exit_code: 0` → `had_failure = false`.
3. Unit test: Bash PostToolUse JSON with missing `exit_code` → `had_failure = false` (default).
4. Unit test: Bash PostToolUse JSON with `interrupted: true` → `had_failure = true`.
5. Unit test: Edit PostToolUse JSON with `tool_input.path: "src/foo.rs"` → `file_path = Some("src/foo.rs")`.
6. Unit test: Write PostToolUse JSON with `tool_input.file_path: "src/bar.rs"` → `file_path = Some("src/bar.rs")`.
7. Unit test: MultiEdit PostToolUse JSON with 2 edits → 2 ReworkEvents with distinct file_paths.
8. Unit test: non-rework tool (Read) → generic RecordEvent (not rework-candidate).
9. Unit test: missing `tool_name` field → treated as non-rework tool (no panic).
10. Unit test: `hook_input.extra` is `serde_json::Value::Null` → all field extractions return default/None without panic.

**Coverage Requirement**: All rework-eligible tool types and all failure indicator fields must be tested. Missing-field cases must produce safe defaults.

---

### R-10: bincode Field Order Violation

**Severity**: Med
**Likelihood**: Low
**Impact**: Deserialization panic when draining SIGNAL_QUEUE after a binary upgrade that reordered SignalRecord fields.

**Test Scenarios**:
1. Unit test: serialize a `SignalRecord` with known field values, then deserialize with the same struct → round-trip produces identical fields.
2. Integration test: write a SignalRecord at v4.0, upgrade code, open database, drain → records still deserialized correctly (field order preserved).
3. Documentation test: CI check that verifies `// LAYOUT FROZEN` comment is present on `SignalRecord`.

**Coverage Requirement**: Field order is enforced by the struct definition and ADR-001. No runtime check needed; the layout-frozen comment and this test serve as the guard.

---

### R-11: Consumer Skips Deleted Entry Without Crash

**Severity**: Low
**Likelihood**: Med
**Impact**: Consumer encounters an entry_id that no longer exists (deprecated, deleted). Panicking would crash the consumer; skipping is correct.

**Test Scenarios**:
1. Integration test: write a Helpful signal for entry_id=999 (does not exist), run confidence consumer → no panic, warning logged, remaining entries processed.

**Coverage Requirement**: `record_helpfulness` error path (entry not found) must be handled with warning log and continue.

---

### R-12: RetrospectiveReport JSON Null vs Absent

**Severity**: Low
**Likelihood**: Low
**Impact**: MCP clients checking `"entries_analysis" in response` receive `null` instead of absent key — subtle breaking change.

**Test Scenarios**:
1. Unit test: serialize `RetrospectiveReport` with `entries_analysis: None` → JSON string does not contain `"entries_analysis"` key.
2. Unit test: serialize with `entries_analysis: Some(vec![...])` → JSON contains `"entries_analysis"` key with array value.
3. Verify `#[serde(skip_serializing_if = "Option::is_none")]` is applied.

**Coverage Requirement**: Both None (absent) and Some (present) cases serialized and verified.

---

### R-13: Empty Entry_ids SignalRecord

**Severity**: Low
**Likelihood**: Low
**Impact**: A SignalRecord with zero entry_ids is written, consuming a signal_id and a SIGNAL_QUEUE slot without effect.

**Test Scenarios**:
1. Unit test: session with empty `injection_history` → `FR-04.6` specifies no write to SIGNAL_QUEUE. Assert `signal_queue_len() == 0`.
2. Unit test: session with all injected entries excluded by ExplicitUnhelpful → no SignalRecord written.

**Coverage Requirement**: The "no signal written" path when `helpful_entry_ids` is empty must be explicitly tested.

---

## Integration Risks

**IR-01**: `Session Intent Registry` intercept logic runs within the Mutex-locked `drain_and_signal_session`. If `agent_actions` lookup is O(n) for large action lists, lock hold time increases. Mitigation: agent_actions list is bounded by session length; max realistic size is ~50 actions. O(50) lookup is still sub-microsecond.

**IR-02**: `run_confidence_consumer` calls `entry_store.record_helpfulness()` which opens a redb write transaction. If this runs concurrently with an MCP tool that also opens a write transaction, redb serializes them (no deadlock). Latency may increase. Mitigation: run confidence consumer after SIGNAL_QUEUE drain, not concurrently.

**IR-03**: `build_report()` signature change (new 6th parameter `entries_analysis`) breaks all existing callers. Mitigation: all callers must be updated to pass `None`. Test: `cargo test --workspace` confirms no compilation errors.

---

## Edge Cases

**EC-01**: Session where `last_activity_at == registration_at` (no injections, no rework events). `sweep_stale_sessions` sweeps it after 4h; `injection_history.is_empty()` → no signals generated, silent eviction.

**EC-02**: Session_id collision: two sessions with the same session_id (Claude Code reuses IDs across restarts). `register_session` overwrites the prior session's state (col-008 behavior, documented in col-008 ADR). For col-009, this means the prior session's signals are lost. Acceptable — the prior session was already orphaned.

**EC-03**: PostToolUse for `MultiEdit` with 0 edits array entries → 0 ReworkEvents generated. No panic.

**EC-04**: SIGNAL_QUEUE has 9,999 Helpful + 1 Flagged records when a new insert would trigger cap. The oldest Helpful record (lowest signal_id) is dropped regardless of type. This preserves the most recent signals but may drop older Helpful signals. Mitigation: consumers drain frequently (every SessionClose).

**EC-05**: `run_confidence_consumer` and `run_retrospective_consumer` run sequentially after `drain_and_signal_session`. If a session has both Helpful AND Flagged signals in queue (from different sessions), both consumers drain their respective types correctly.

---

## Security Risks

**SEC-01**: PostToolUse stdin JSON arrives from Claude Code (same-process, UID-verified). `input.extra` is serde_json::Value — arbitrary JSON. Malformed/malicious values for `exit_code` (non-integer type), `tool_input.path` (path traversal strings), or `tool_name` (injection attempt) must not cause:
- Panics in field extraction helpers
- Path traversal through `file_path` storage in `ReworkEvent.file_path`
- Memory allocation attacks via very long strings

Mitigation: all field extractions use `.as_str()` / `.as_i64()` / `.get()` — type coercions that return `None` on unexpected types, not panics. `file_path` is stored as-is in memory (no filesystem access). Maximum string length enforcement is NOT required in col-009 (rework detection is internal-only state, not user-facing output).

**SEC-02**: `SignalRecord.entry_ids` contains entry_ids from `injection_history`. An attacker who can inject entries (requiring Write MCP trust level) could potentially manipulate which entries receive `helpful_count` increments. Blast radius: `helpful_count` inflated for attacker-controlled entries → higher confidence → more frequent injection. Mitigation: content scanning on `context_store` (vnc-002) is the primary defense. col-009 does not add new write paths.

**SEC-03**: `PendingEntriesAnalysis` is drained and cleared on every `context_retrospective` call. An attacker repeatedly calling `context_retrospective` could prevent accumulation of `entries_analysis` data. Mitigation: `context_retrospective` requires `Admin` trust level (existing capability check). Admin agents are enrolled by humans.

---

## Failure Modes

**FM-01**: Server crashes after `insert_signal` but before `drain_signals`. Recovery: signals remain in SIGNAL_QUEUE and are processed on the next `SessionClose`. Acceptable — soft durability.

**FM-02**: `record_helpfulness` returns `Err` for all entries (e.g., redb I/O error). Consumer logs warning, does not retry. Signals are lost. Recovery: next batch of signals will re-attempt when the I/O condition clears.

**FM-03**: `sweep_stale_sessions` runs but the registry lock is contended (another SessionClose in progress). `sweep_stale_sessions` will block briefly until the lock is acquired. Lock hold times are microseconds — no significant latency impact.

**FM-04**: `PendingEntriesAnalysis.entries` exceeds 1,000 entries with no `context_retrospective` call. Cap enforcement drops lowest-rework entries. Human visibility into those entries is reduced. Recovery: call `context_retrospective` more frequently.

**FM-05**: `drain_signals` is called but SIGNAL_QUEUE table doesn't exist (migration failed). Returns `StoreError::TableNotFound`. Consumer logs error and skips. Server remains functional; confidence pipeline is degraded. Recovery: schema migration issue must be investigated and resolved manually.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (bincode field order) | R-10 | ADR-001 locks field order; `// LAYOUT FROZEN` comment; explicit enum discriminants |
| SR-02 (drain crash window) | R-05 | Documented soft-durability tradeoff; Wilson 5-vote guard bounds impact |
| SR-03 (Wilson 5-vote delay) | — | Not an architecture risk; product expectation management |
| SR-04 (Session Intent Registry scope creep) | — | ADR-003 constrains scope; `SessionAction` closed enum; col-010 is the correct extension point |
| SR-05 (PendingEntriesAnalysis unbounded) | R-07 | 1,000-entry cap with lowest-rework eviction (FR-06.3) |
| SR-06 (PostToolUse JSON fields) | R-09 | All fields extracted with safe `.as_*()` coercions; missing-field defaults specified in FR-07.3–FR-07.5 |
| SR-07 (SessionState race) | R-01 | ADR-003: `drain_and_signal_session` is atomic; single Mutex acquisition for generate + clear |
| SR-08 (confidence pipeline contention) | R-02 (migration) | Migration is write-once for new table; no scan-and-rewrite |
| SR-09 (entries_analysis conflict with col-010) | — | Resolved Design Decision #4: col-009 additive; col-010 alignment deferred |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 4 (R-01, R-02, R-03, R-04, R-09) | 19 scenarios across 5 risks |
| Med | 6 (R-05, R-06, R-07, R-08, R-10, R-11) | 14 scenarios across 6 risks |
| Low | 3 (R-11 elevated, R-12, R-13) | 5 scenarios across 3 risks |
| Security | 3 (SEC-01, SEC-02, SEC-03) | Covered by access control tests + field extraction tests |
| **Total** | **13 risks** | **~38 test scenarios** |
