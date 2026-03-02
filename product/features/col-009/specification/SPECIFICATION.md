# Specification: col-009 Closed-Loop Confidence

## Objective

col-009 closes the confidence feedback loop for hook-injected knowledge by deriving implicit helpfulness signals from session outcomes. When a session completes successfully, entries that were injected during the session receive `helpful=true` signals applied via the existing crt-002 confidence pipeline. When rework is detected, injected entries are flagged for human review in the retrospective pipeline — never auto-downweighted. This delivers the "learning" part of the knowledge lifecycle without requiring any agent cooperation.

## Functional Requirements

### FR-01: Schema v4 Migration

FR-01.1: On `Store::open()`, if `schema_version < 4`, run `migrate_v3_to_v4()` in a single write transaction.

FR-01.2: `migrate_v3_to_v4()` MUST: open the SIGNAL_QUEUE table (triggering redb table creation), write `next_signal_id = 0` to COUNTERS (only if the key does not already exist), update `schema_version` to 4.

FR-01.3: Migration MUST leave all existing entries, indexes, and tables unchanged.

FR-01.4: If `schema_version >= 4`, `migrate_if_needed()` MUST return immediately without performing any writes.

FR-01.5: `CURRENT_SCHEMA_VERSION` constant MUST be 4 after this feature ships.

### FR-02: SignalRecord Persistence

FR-02.1: `SignalRecord` MUST be bincode v2 serialized with the field order defined in ADR-001. The `signal_id` is the SIGNAL_QUEUE table key AND a field in the value.

FR-02.2: `Store::insert_signal(record)` MUST allocate a new `signal_id` by reading and incrementing `next_signal_id` in COUNTERS within the same write transaction as the SIGNAL_QUEUE write.

FR-02.3: `Store::insert_signal` MUST enforce the 10,000-record cap: before inserting, if `signal_queue_len() >= 10_000`, delete the record with the lowest signal_id (oldest) before inserting the new record.

FR-02.4: `Store::drain_signals(signal_type: SignalType)` MUST read all SignalRecords with matching `signal_type` from SIGNAL_QUEUE and delete them within a single write transaction. Returns the drained records.

FR-02.5: `Store::drain_signals` MUST be idempotent on an empty queue (returns empty Vec, no error).

FR-02.6: `Store::signal_queue_len()` MUST return the count of all records in SIGNAL_QUEUE regardless of `signal_type`.

### FR-03: Session State Extensions

FR-03.1: `SessionState` MUST have the following new fields initialized at `register_session()`:
- `signaled_entries: HashSet<u64>` — empty
- `rework_events: Vec<ReworkEvent>` — empty
- `agent_actions: Vec<SessionAction>` — empty
- `last_activity_at: u64` — current Unix timestamp

FR-03.2: `SessionRegistry::record_rework_event(session_id, event)` MUST append the `ReworkEvent` to `SessionState.rework_events` and update `last_activity_at = max(last_activity_at, event.timestamp)`. If session is not registered, this is a silent no-op.

FR-03.3: `SessionRegistry::record_agent_action(session_id, action)` MUST append the `SessionAction` to `SessionState.agent_actions`. If session is not registered, this is a silent no-op.

FR-03.4: `SessionRegistry::record_injection()` MUST update `last_activity_at` to the current timestamp whenever new injection records are appended.

FR-03.5: `SessionRegistry::register_session()` MUST set `last_activity_at` to the current Unix timestamp.

### FR-04: Signal Generation

FR-04.1: `SessionRegistry::drain_and_signal_session(session_id, hook_outcome)` MUST be atomic — a single Mutex acquisition that generates signals AND removes the session from the registry.

FR-04.2: If the session is not found (already cleared), `drain_and_signal_session` MUST return `None`.

FR-04.3: The `helpful_entry_ids` in `SignalOutput` MUST be the deduplicated union of entry_ids from `injection_history`, with these exclusions applied:
- Any entry_id present in `agent_actions` with `action == ExplicitUnhelpful` MUST be excluded.
- Any entry_id already in `signaled_entries` MUST be excluded (dedup guard against double-processing).

FR-04.4: The `final_outcome` in `SignalOutput` is determined as follows:
- If `hook_outcome == "success"` AND `has_crossed_rework_threshold() == false`: `SessionOutcome::Success` → `signal_type = Helpful`, `signal_source = ImplicitOutcome`
- If `has_crossed_rework_threshold() == true` (regardless of hook_outcome): `SessionOutcome::Rework` → `signal_type = Flagged`, `signal_source = ImplicitRework`
- If `hook_outcome == "abandoned"` OR `hook_outcome == ""` OR `hook_outcome == None`: `SessionOutcome::Abandoned` → no SignalOutput returned (None)

FR-04.5: `has_crossed_rework_threshold()` MUST evaluate `rework_events` per ADR-002: returns `true` if any single `file_path` appears in 3+ Edit/Write/MultiEdit events, each pair of consecutive edits separated by at least one `ReworkEvent` with `had_failure == true`.

FR-04.6: When `helpful_entry_ids` is empty (session had no injections, or all were excluded), NO SignalRecord is written to SIGNAL_QUEUE. Returns `Some(SignalOutput { helpful_entry_ids: [], flagged_entry_ids: [] })`.

FR-04.7: `SessionRegistry::sweep_stale_sessions()` MUST, in a single lock acquisition: find all sessions where `last_activity_at < now - STALE_SESSION_THRESHOLD_SECS`, generate `SignalOutput` for each (applying same rules as FR-04.3/FR-04.4 with `hook_outcome = "success"`), remove them from the registry, and return the outputs.

FR-04.8: `STALE_SESSION_THRESHOLD_SECS` MUST be a named constant set to `4 * 3600` (14,400 seconds).

### FR-05: Signal Processing — Confidence Consumer

FR-05.1: After `drain_and_signal_session` returns a non-empty `SignalOutput` with `final_outcome == Success`, the caller MUST write a `SignalRecord(signal_type=Helpful)` to SIGNAL_QUEUE, then immediately call `run_confidence_consumer()`.

FR-05.2: `run_confidence_consumer()` MUST call `Store::drain_signals(SignalType::Helpful)`, then for each unique `entry_id` across all drained records, call the existing `helpful_count` increment + confidence recompute pathway (crt-002) with `source = "hook"`.

FR-05.3: If `drain_signals` returns an error, `run_confidence_consumer()` MUST log a warning and return without crashing. Unprocessed signals remain in SIGNAL_QUEUE.

FR-05.4: If `record_helpfulness` fails for a specific entry_id (e.g., entry was deleted), that entry_id MUST be skipped with a warning log. Processing continues for remaining entry_ids.

FR-05.5: `run_confidence_consumer()` MUST process all Helpful signals in a single drain (not one at a time), to minimize redb write transaction count.

### FR-06: Signal Processing — Retrospective Consumer

FR-06.1: After `drain_and_signal_session` returns a non-empty `SignalOutput` with `final_outcome == Rework`, the caller MUST write a `SignalRecord(signal_type=Flagged)` to SIGNAL_QUEUE, then call `run_retrospective_consumer(pending_entries_analysis)`.

FR-06.2: `run_retrospective_consumer()` MUST call `Store::drain_signals(SignalType::Flagged)`, then for each `entry_id` in each drained record, update `PendingEntriesAnalysis`:
- If entry_id not yet present: fetch `EntryRecord` (title, category) from store, create new `EntryAnalysis` with `rework_flag_count = 1`, `rework_session_count = 1`.
- If entry_id present: increment `rework_flag_count` and `rework_session_count`.

FR-06.2b: When `run_confidence_consumer()` drains a `Helpful` SignalRecord, for each `entry_id` in the record, the caller MUST also increment `EntryAnalysis.success_session_count` in `PendingEntriesAnalysis`:
- If entry_id not yet present: fetch `EntryRecord` (title, category) from store, create new `EntryAnalysis` with `success_session_count = 1` (all rework counters = 0).
- If entry_id present: increment `success_session_count`.

FR-06.3: `PendingEntriesAnalysis` MUST enforce a cap of 1,000 entries: when adding a new entry would exceed 1,000, drop the existing entry with the lowest `rework_flag_count`. If tied, drop the oldest (any stable tiebreak).

FR-06.4: `PendingEntriesAnalysis` MUST be protected by `Arc<Mutex<...>>` for thread-safe access from both the UDS listener and the MCP tool handler.

FR-06.5: The `context_retrospective` MCP tool MUST drain `PendingEntriesAnalysis` before building the report and pass the drained entries as `entries_analysis` to `build_report()`.

FR-06.6: After draining `PendingEntriesAnalysis`, the map MUST be cleared (empty for the next accumulation period).

### FR-07: PostToolUse Rework Detection

FR-07.1: `hook.rs build_request()` MUST have a `"PostToolUse"` arm that extracts `tool_name` from `input.extra["tool_name"]`.

FR-07.2: Rework-eligible tools MUST be: `"Bash"`, `"Edit"`, `"Write"`, `"MultiEdit"`. All other tools fall through to the generic `RecordEvent` path.

FR-07.3: For Bash: `had_failure = true` if `input.extra["exit_code"]` is a non-zero integer OR `input.extra["interrupted"]` is boolean true. `had_failure = false` otherwise (including missing fields).

FR-07.4: For Edit/Write/MultiEdit: `had_failure = false` always.

FR-07.5: `file_path` extraction:
- Edit: `input.extra["tool_input"]["path"]` as string, or `None`
- Write: `input.extra["tool_input"]["file_path"]` as string, or `None`
- MultiEdit: `input.extra["tool_input"]["edits"][*]["path"]` — generate one `ReworkEvent` per distinct path in the edits array
- Bash: `file_path = None`

FR-07.6: For rework-eligible tools, `build_request()` MUST produce `HookRequest::RecordEvent { event_type: "post_tool_use_rework_candidate", payload: { tool_name, file_path, had_failure } }`.

FR-07.7: `dispatch_request()` MUST handle `RecordEvent` with `event_type == "post_tool_use_rework_candidate"` by calling `session_registry.record_rework_event()`. Response: `HookResponse::Ack`.

FR-07.8: PostToolUse RecordEvent MUST remain fire-and-forget (is_fire_and_forget returns true).

### FR-08: Stop Hook Outcome

FR-08.1: `build_request()` for `"Stop"` MUST set `outcome: Some("success".to_string())`. The server overrides to `"rework"` via threshold evaluation in `drain_and_signal_session`.

FR-08.2: `build_request()` for `"TaskCompleted"` MUST behave identically to `"Stop"` (same outcome field).

### FR-09: Stale Session Sweep

FR-09.1: `sweep_stale_sessions()` MUST be called at the start of every `process_session_close()` invocation, before processing the current session.

FR-09.2: `sweep_stale_sessions()` MUST also be callable from the `maintain=true` path in `context_status`.

FR-09.3: A session is stale if `current_time - last_activity_at >= STALE_SESSION_THRESHOLD_SECS`.

FR-09.4: Stale sessions with no injection history (`injection_history.is_empty()`) MUST be evicted silently (no signals generated, no SIGNAL_QUEUE writes).

### FR-10: RetrospectiveReport Extension

FR-10.1: `RetrospectiveReport` MUST have a new field `entries_analysis: Option<Vec<EntryAnalysis>>` with `#[serde(default)]`.

FR-10.2: `EntryAnalysis` MUST have fields: `entry_id: u64`, `title: String`, `category: String`, `rework_flag_count: u32`, `injection_count: u32`, `success_session_count: u32`, `rework_session_count: u32`.

FR-10.3: `build_report()` MUST accept `entries_analysis: Option<Vec<EntryAnalysis>>` as a new parameter and assign it to `RetrospectiveReport.entries_analysis`.

FR-10.4: All existing callers of `build_report()` MUST be updated to pass `None` for `entries_analysis` (backward-compatible).

FR-10.5: When `PendingEntriesAnalysis` is empty at retrospective time, `entries_analysis` in the report MUST be `None` (not `Some([])`).

### FR-11: Session Intent Registry Integration

FR-11.1: When the UDS listener processes an MCP tool result that constitutes an agent action on an injected entry (explicit `helpful=false` vote, `context_correct`, `context_deprecate`), it MUST call `session_registry.record_agent_action()` with the appropriate `AgentActionType`.

FR-11.2: Entries with `ExplicitUnhelpful` in `agent_actions` for the current session MUST be excluded from the `helpful_entry_ids` set in `drain_and_signal_session` (FR-04.3).

FR-11.3: Entries with `ExplicitHelpful`, `Correction`, or `Deprecation` in `agent_actions` MUST NOT be excluded from `helpful_entry_ids` in col-009 (reserved for future intercept criteria in col-010+).

## Non-Functional Requirements

### NFR-01: Performance

NFR-01.1: `drain_and_signal_session()` (Mutex hold + threshold evaluation + dedup construction) MUST complete in < 5ms for a session with up to 1,000 `rework_events` and 500 injection_history entries.

NFR-01.2: `run_confidence_consumer()` MUST complete in < 100ms for a batch of 50 entry_ids (including redb write transaction).

NFR-01.3: Signal generation does NOT add latency to the hook process response path. The hook receives `HookResponse::Ack` from `SessionClose` after signals are queued but consumers may run asynchronously via `tokio::spawn` if the synchronous path exceeds 50ms.

NFR-01.4: SIGNAL_QUEUE table operations (insert, drain, len) MUST complete in < 10ms for queue sizes up to 10,000 records.

### NFR-02: Reliability

NFR-02.1: If the server crashes after writing to SIGNAL_QUEUE but before draining, the records are lost (not reprocessed). This is a documented soft-durability tradeoff (SCOPE Non-Goals).

NFR-02.2: If `drain_signals` fails, the system MUST NOT crash. Unprocessed signals are retried on the next SessionClose.

NFR-02.3: The dedup guarantee (`signaled_entries`) is in-memory and does not survive server restarts. A server restart resets dedup state. This is acceptable — the Wilson 5-vote minimum prevents isolated double-signals from mattering.

### NFR-03: Compatibility

NFR-03.1: All existing 1,025 unit + 174 integration tests MUST pass after schema v4 migration and all new code.

NFR-03.2: `RetrospectiveReport` JSON serialization MUST be backward compatible — `entries_analysis` MUST be absent (not null) in JSON when None, using `#[serde(default, skip_serializing_if = "Option::is_none")]`.

NFR-03.3: The schema v4 migration MUST complete in < 1 second on a database with 1,000 active entries (no entry scan-and-rewrite required).

## Acceptance Criteria

Tracing from SCOPE.md:

- **AC-01**: Schema v4 migration runs on `Store::open()` when schema version is 3 — creates SIGNAL_QUEUE table, writes `next_signal_id = 0`, increments schema_version to 4. Verified by: open a fresh test database, confirm SIGNAL_QUEUE table exists, schema_version == 4, next_signal_id == 0.

- **AC-02**: For a session with 3 distinct injected entries and `"success"` outcome, exactly one `Helpful` SignalRecord with `entry_ids = [id1, id2, id3]` is written. Verified by: unit test with mock session, assert `signal_queue_len() == 1` after SessionClose.

- **AC-03**: Double-processing dedup: if `drain_and_signal_session` is called twice for the same session_id, the second call returns `None` (session already cleared). Verified by: unit test calling method twice, assert second call returns None.

- **AC-04**: Confidence consumer increments `helpful_count` exactly once per entry_id in a Helpful SignalRecord. Verified by: integration test, inspect `EntryRecord.helpful_count` after consumer runs.

- **AC-05**: Abandoned sessions (empty or None outcome) produce no signals. Verified by: unit test with `outcome = ""`, assert SIGNAL_QUEUE empty after SessionClose.

- **AC-06**: Sessions with rework threshold crossed produce `Flagged` SignalRecords, not Helpful. `EntryRecord.helpful_count` and `unhelpful_count` are NOT modified. Verified by: integration test with 3 edit-fail-edit cycles, assert signal_type == Flagged and no helpful_count change.

- **AC-07**: Flagged signals accumulate in `entries_analysis` and appear in next `context_retrospective` response. Verified by: integration test with rework session followed by context_retrospective call, assert non-empty entries_analysis.

- **AC-08**: After 3 consecutive failed `cargo test` Bash calls in a session (each with exit_code != 0, separated by Edit events to the same file), `has_crossed_rework_threshold() == true`. Verified by: unit test populating rework_events matching the pattern.

- **AC-09**: Stale session sweep evicts sessions with `last_activity_at < now - 4h`. Verified by: unit test creating a session with backdated last_activity_at, calling sweep, asserting session removed.

- **AC-10**: SIGNAL_QUEUE cap: inserting 10,001 records drops the oldest. Verified by: store unit test inserting 10,001 records, asserting len == 10,000 and oldest record absent.

- **AC-11**: All 1,025 unit + 174 integration tests pass. Verified by: `cargo test --workspace`.

- **AC-12**: Signal consumer processes 50 entries in < 100ms. Verified by: timing integration test.

- **AC-13**: `entries_analysis` is absent from JSON when None (not `"entries_analysis": null`). Verified by: serialize `RetrospectiveReport` with no flagged signals, assert key absent in JSON.

## Domain Models

### SignalRecord

The unit of work in the signal pipeline. Created at session end, consumed by dual consumers, then deleted.

```
SignalRecord {
  signal_id:    u64        — monotonic key (from next_signal_id counter)
  session_id:   String     — which session generated this signal
  created_at:   u64        — Unix seconds at generation time
  entry_ids:    Vec<u64>   — deduplicated entries receiving this signal
  signal_type:  Helpful | Flagged
  signal_source: ImplicitOutcome | ImplicitRework
}
```

### SessionAction (Session Intent Registry)

Records a purposeful agent MCP action during a session that intercepts implicit signal generation.

```
SessionAction {
  entry_id:  u64               — which entry the agent acted on
  action:    AgentActionType   — ExplicitUnhelpful | ExplicitHelpful | Correction | Deprecation
  timestamp: u64
}
```

### ReworkEvent

A single PostToolUse observation that may contribute to the rework threshold.

```
ReworkEvent {
  tool_name:   String          — "Bash" | "Edit" | "Write" | "MultiEdit"
  file_path:   Option<String>  — for file-mutating tools
  had_failure: bool            — true if Bash returned non-zero exit
  timestamp:   u64
}
```

### EntryAnalysis

Aggregated entry-level performance data accumulated across sessions and surfaced in the retrospective.

```
EntryAnalysis {
  entry_id:            u64     — references EntryRecord
  title:               String
  category:            String
  rework_flag_count:   u32     — how many Flagged signal batches included this entry
  injection_count:     u32     — total times injected across sessions (not col-009, future)
  success_session_count: u32   — sessions that completed successfully with this entry injected
  rework_session_count: u32    — sessions that crossed rework threshold with this entry injected
}
```

### SessionOutcome

The resolved outcome classification for signal generation.

```
Success   — clean Stop, rework threshold not crossed → Helpful signals
Rework    — rework threshold crossed → Flagged signals
Abandoned — no Stop hook / empty outcome → no signals
```

## User Workflows

### Workflow 1: Successful Session (Happy Path)

1. Agent session starts. `SessionStart` hook fires → server registers session.
2. Agent prompts Claude Code. `UserPromptSubmit` fires → col-007 injects entries → `SessionState.injection_history` populated.
3. Agent works through the feature. PostToolUse fires for each tool call → `record_rework_event()` called for rework-eligible tools. Rework threshold not crossed.
4. Agent completes task. `Stop` hook fires → `build_request()` sets `outcome = "success"`.
5. Server processes `SessionClose`: sweeps stale sessions, generates `SignalOutput(Success)`, writes `Helpful` SignalRecord to SIGNAL_QUEUE, clears session.
6. Confidence consumer drains SIGNAL_QUEUE, increments `helpful_count` for each injected entry, recomputes confidence.
7. Over 5+ sessions, high-quality entries strengthen in confidence score.

### Workflow 2: Rework Detected

1. Same session start.
2. Agent edits `foo.rs` → cargo test fails → agent edits `foo.rs` again → cargo test fails → agent edits `foo.rs` again → cargo test fails → agent edits `foo.rs` a 4th time. Rework threshold crossed (3 edit-fail-edit cycles on `foo.rs`).
3. `Stop` hook fires → `outcome = "success"` in wire request.
4. Server evaluates `has_crossed_rework_threshold()` → true → overrides to `Rework`.
5. Writes `Flagged` SignalRecord. Retrospective consumer updates `PendingEntriesAnalysis` for injected entries.
6. Next `context_retrospective` call returns `entries_analysis` with flagged entries. Human reviews which entries correlated with the rework session.

### Workflow 3: Orphaned Session Recovery

1. Claude Code crashes mid-session. `Stop` hook never fires. SessionState persists in-memory for 4+ hours.
2. Next Claude Code session starts. Its `Stop` hook fires, triggering `process_session_close()`.
3. `sweep_stale_sessions()` runs at the top of `process_session_close()`, finds the orphaned session, generates `SignalOutput(Success)` (orphaned sessions default to success outcome — no rework events observed).
4. Helpful signals processed for orphaned session's injected entries.
5. Orphaned session removed from registry.

### Workflow 4: Agent Explicit Unhelpful Vote Intercepts Implicit

1. Session active. Agent receives injected entry_id=42.
2. Agent explicitly calls `context_search` with `helpful=false` for entry_id=42.
3. Server: `record_agent_action(session_id, SessionAction { entry_id: 42, action: ExplicitUnhelpful })`.
4. Session ends with `Stop`.
5. `drain_and_signal_session`: entry_id=42 is in `agent_actions` with `ExplicitUnhelpful` → excluded from `helpful_entry_ids`. Only remaining injected entries receive Helpful signals. Entry_id=42 gets no implicit Helpful signal. The explicit unhelpful vote stands unmodified.

## Constraints

- **No auto-downweighting**: `unhelpful_count` is never modified by col-009. Only explicit `helpful=false` MCP votes do this.
- **In-memory injection history only**: col-009 reads from `SessionState.injection_history`. No redb reads of injection history.
- **Schema migration append-only**: SIGNAL_QUEUE is new; no existing data rewrite. Future field additions require a schema v5 migration.
- **Edition 2024, MSRV 1.89**.
- **Single binary**: all col-009 code is in existing crates. No new binary or crate.
- **Synchronous confidence consumer**: runs inline during SessionClose dispatch. No background task or timer.

## Dependencies

| Dependency | Version / Feature | What col-009 Uses |
|------------|-------------------|-------------------|
| `unimatrix-store` (internal) | schema v3 → v4 | SIGNAL_QUEUE table, migration |
| `unimatrix-server` (internal) | session.rs, uds_listener.rs, hook.rs | SessionRegistry extensions, dispatch, hook handler |
| `unimatrix-observe` (internal) | types.rs, report.rs | EntryAnalysis, build_report extension |
| `redb` | v3.1.x (existing) | SIGNAL_QUEUE table operations |
| `bincode` | v2 serde path (existing) | SignalRecord serialization |
| `serde` | existing | SignalRecord/EntryAnalysis derive |
| col-008 | COMPLETE | SessionRegistry, InjectionRecord, SessionState |
| col-007 | COMPLETE | injection_history population via record_injection() |

## NOT in Scope

- Persistent session storage (col-010: SESSIONS table, INJECTION_LOG)
- `unhelpful_count` modification from implicit signals (asymmetric design — never)
- Modification of the confidence formula (crt-002 unchanged)
- Entry-level injection count tracking (`injection_count` field on `EntryAnalysis` is present in the struct but populated as 0 in col-009; col-010 provides INJECTION_LOG data to populate it)
- `context_retrospective` JSONL pipeline changes (col-009 extension is additive)
- Signal replay on server restart (SIGNAL_QUEUE is ephemeral — soft durability tradeoff)
- Anti-stuffing defenses beyond Wilson 5-vote minimum and session-scoped dedup
- col-011 (Semantic Agent Routing) — independent feature
