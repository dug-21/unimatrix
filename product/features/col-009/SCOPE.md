# col-009: Closed-Loop Confidence — Asymmetric Implicit Feedback

## Problem Statement

Unimatrix's confidence evolution pipeline (crt-002) is powered by `helpful_count` and `unhelpful_count` on `EntryRecord`. Today, these counters are updated only when agents explicitly call `context_search` or `context_lookup` with the `helpful=true/false` parameter. Agents rarely do this: they forget, they don't know which entries were injected, and they have no view into the hook-level injection history.

The result is that the confidence pipeline is effectively inert for hook-injected knowledge. Entries are injected by col-007 into every prompt; they shape agent behaviour; but the feedback loop is never closed. High-quality entries don't strengthen. Low-quality entries don't surface for review. The knowledge base is a write-once system masquerading as a learning one.

col-009 closes this loop automatically — without requiring any agent cooperation — by deriving confidence signals from session outcomes:

- A successful session (clean Stop/TaskCompleted with no rework) → bulk `helpful=true` for every entry the session's injection history records, applied atomically via the existing crt-002 pipeline.
- Rework detected mid-session (repeated edits to the same file with intervening failures, undo patterns via PostToolUse) → the affected entries are **flagged for human review** in the retrospective pipeline, NOT auto-downweighted.

This "auto-positive, flag-negative, never auto-downweight" asymmetry is a deliberate product safety decision: guilt-by-association would cause good entries that happened to be co-injected with a troubled session to lose confidence incorrectly. Only explicit human MCP votes may increment `unhelpful_count`. The asymmetry bounds the worst-case error: false positive confidence signals are diluted over time by Wilson score's sample-size requirement (minimum 5 votes before deviation from the neutral prior); false negatives are surfaced to humans rather than acted on automatically.

col-009 depends on col-008's in-memory `SessionRegistry` (already built) for injection history. It does NOT require session persistence — col-010 will add that later. Server restarts lose the current session's injection history, but this is an accepted tradeoff for the current wave.

## Goals

1. Implement SIGNAL_QUEUE table (15th table, schema v4) and `next_signal_id` counter in COUNTERS — owned by col-009 and validated by the feature that writes to it
2. Implement session-end Helpful signal generation: on `SessionClose`, drain `SessionState.injection_history` and write `Helpful` SignalRecords to SIGNAL_QUEUE for all injected entry_ids
3. Implement mid-session Flagged signal generation: on PostToolUse events, detect rework patterns and write `Flagged` SignalRecords for recently injected entries
4. Implement dual-consumer signal processing: confidence consumer (drains Helpful signals → increments `helpful_count` → recomputes confidence via crt-002); retrospective consumer (drains Flagged signals → appends to `entries_analysis` in RetrospectiveReport)
5. Implement session-scoped dedup guarantee: at most one `Helpful` implicit signal per (session_id, entry_id) pair — prevents repeated SessionClose attempts from double-counting
6. Implement stale session sweep for orphaned in-memory sessions: periodic scan clears sessions older than a configurable threshold (default 4 hours) and generates signals for them before eviction
7. Extend `unimatrix-observe` `RetrospectiveReport` with an `entries_analysis` field that surfaces Flagged entries correlated with rework events
8. Update `HookRequest` and `HookResponse` wire types to support signal-related IPC (new `RecordSignal` request variant and `SignalQueuedAck` response)
9. Update `.claude/settings.json` hook wiring to register `PostToolUse` events for rework detection (Stop is already registered from col-008)

## Non-Goals

- **Auto-downweighting** — never. Only explicit `helpful=false` MCP votes increment `unhelpful_count`. Flagged signals go to human review only.
- **Persistent session recovery** — col-010 implements SESSIONS table, INJECTION_LOG, and `session_id` on EntryRecord. col-009 operates on col-008's in-memory SessionState exclusively. A server restart mid-session loses that session's injection history; signals are not generated for it.
- **Per-entry injection log in redb** — col-010 implements INJECTION_LOG. col-009 reads from in-memory `SessionState.injection_history` only.
- **Signal persistence beyond processing** — SignalRecords are ephemeral: after the dual consumers drain them, the entries in SIGNAL_QUEUE are deleted. The SIGNAL_QUEUE is a transient work queue, not an audit log.
- **Rework score weighting** — The confidence formula (crt-002) is unchanged. Flagged signals do not modify any numeric score; they only appear in the retrospective pipeline.
- **Adaptive injection volume** — col-008 deferred this (reduce col-007 injection volume on repeated compaction). col-009 does not address it either.
- **Signal authentication / anti-stuffing** — The Wilson 5-vote minimum and session-scoped dedup are the only guards. Sophisticated manipulation is a future concern (crt-002 gaming resistance note in PRODUCT-VISION.md).
- **Session lifecycle persistence** — col-010's SESSIONS table and structured event ingestion for col-002. col-009 does not add these.
- **col-010's structured event ingestion for col-002** — col-009 extends RetrospectiveReport but does NOT replace the JSONL parser. The `entries_analysis` extension is additive.

## Background Research

### col-008 SessionRegistry (Already Built)

`crates/unimatrix-server/src/session.rs` implements:
- `InjectionRecord { entry_id: u64, confidence: f64, timestamp: u64 }`
- `SessionState { session_id, role, feature, injection_history: Vec<InjectionRecord>, coaccess_seen, compaction_count }`
- `SessionRegistry { sessions: Mutex<HashMap<String, SessionState>> }` with `register_session`, `record_injection`, `get_state`, `clear_session` methods

col-009 reads `injection_history` from the registry to generate signals. No structural changes to `SessionRegistry` are needed — col-009 adds a method to drain signal-eligible entries with dedup.

### Existing Wire Protocol (col-006/col-007/col-008)

`crates/unimatrix-engine/src/wire.rs` defines `HookRequest` and `HookResponse` as serde-tagged enums. `SessionClose` already carries `outcome: Option<String>`. col-009 needs to:
1. Use the `outcome` field to differentiate `"success"` from `"rework"` / `""` (abandoned)
2. Add PostToolUse rework-detection logic in `hook.rs` `build_request()` — currently PostToolUse falls through to `RecordEvent`. col-009 will add a new `PostToolUse` arm that generates a `RecordSignal` or enriched `RecordEvent` when rework patterns are detected.

### Existing Store (14 Tables — schema v3)

`crates/unimatrix-store/src/schema.rs` defines 14 tables. COUNTERS uses `&str -> u64`. Adding `next_signal_id` to COUNTERS follows the established counter pattern (`next_entry_id`, `next_event_id`). No new counter infrastructure needed.

SIGNAL_QUEUE will be defined in `schema.rs` as:
```rust
pub const SIGNAL_QUEUE: TableDefinition<u64, &[u8]> = TableDefinition::new("signal_queue");
```
Key: `next_signal_id` (monotonic), Value: bincode-serialized `SignalRecord`.

Migration pattern: same 3-step process used in v0→v1 (nxs-004), v1→v2 (crt-001), v2→v3 (crt-005). For schema v4, the migration only needs to open the SIGNAL_QUEUE table definition and write the initial `next_signal_id = 0` counter — no entry scan-and-rewrite needed since SIGNAL_QUEUE is new. Schema version bumps from 3 to 4 in `CURRENT_SCHEMA_VERSION`.

### Existing Confidence Pipeline (crt-001, crt-002)

`helpful_count: u32` on EntryRecord is incremented by `record_usage_with_confidence()` in the server's tools layer. The Wilson score helpfulness factor is computed at confidence-recompute time using `helpful_count` and `unhelpful_count`. col-009's confidence consumer calls the same pathway with `source="hook"` to distinguish implicit from explicit votes. The `UsageDedup` in `crates/unimatrix-server/src/usage_dedup.rs` tracks `(agent_id, entry_id)` pairs — col-009 needs a parallel session-scoped dedup for implicit signals: `(session_id, entry_id) -> bool`, distinct from agent-scoped dedup.

### Existing Retrospective Pipeline (col-002)

`crates/unimatrix-observe/src/types.rs` defines `RetrospectiveReport { feature_cycle, session_count, total_records, metrics, hotspots, is_cached, baseline_comparison }`. col-009 adds an `entries_analysis: Option<Vec<EntryAnalysis>>` field using `#[serde(default)]` — additive, backward compatible. `build_report()` in `report.rs` will accept the new field.

`EntryAnalysis` holds: `entry_id: u64`, `title: String`, `category: String`, `rework_flag_count: u32`, `injection_count: u32`, `success_session_count: u32`, `rework_session_count: u32`. This lets the retrospective surface patterns like "entry #42 was present in 6 sessions where rework occurred but only 1 success session."

### SessionStop Unreliability

Stop/TaskCompleted hooks may not fire if Claude Code crashes, is OOM-killed, or the user force-quits. col-009 cannot treat SessionClose as a guaranteed trigger. The stale session sweep (Goal 6) handles orphaned sessions: a background timer or maintenance trigger scans `SessionRegistry` for sessions where `last_activity_time > threshold` (4h default), generates signals for them, and evicts them. The sweep runs:
- On every `SessionClose` (as a side-effect before processing the closed session)
- On the `maintain=true` path in `context_status`

The dedup guarantee (Goal 5) must hold even if the sweep and a real SessionClose both trigger for the same session — the `(session_id, entry_id)` dedup set prevents double-counting.

### Rework Detection via PostToolUse

PostToolUse hook events carry `tool_name`, `tool_input`, and `tool_response` in their stdin JSON. The current `build_request()` falls through to `RecordEvent` for PostToolUse. col-009 adds rework pattern detection:
- **Pattern 1 (Edit-fail-edit)**: Same file edited more than once with a failed Bash (non-zero exit) between the edits → rework signal for that session
- **Pattern 2 (Compile loop)**: `cargo build` or `cargo test` fails 3+ consecutive times in the same session → rework signal
- **Pattern 3 (Undo pattern)**: A file is written and then overwritten within 60 seconds → rework signal

Rework detection state is maintained in a lightweight per-session `ReworkDetector` in the hook process (ephemeral, stateless across invocations) — or, since the hook process is stateless, the server-side SessionState can be extended with a `rework_events: Vec<ReworkEvent>` to track cross-invocation state. The server-side approach is preferred since the hook process has no persistent state.

## Proposed Approach

### 5 Build Components

**1. SIGNAL_QUEUE Table and SignalRecord (store layer)**

Add to `crates/unimatrix-store/src/schema.rs`:
- `SIGNAL_QUEUE: TableDefinition<u64, &[u8]>` (key = monotonic signal_id)
- Schema v3→v4 migration: open SIGNAL_QUEUE table (triggers creation), write `next_signal_id = 0` to COUNTERS
- Bump `CURRENT_SCHEMA_VERSION` to 4

Add `crates/unimatrix-store/src/signal.rs`:
```rust
pub struct SignalRecord {
    pub signal_id: u64,
    pub session_id: String,
    pub created_at: u64,
    pub entry_ids: Vec<u64>,
    pub signal_type: SignalType,       // Helpful | Flagged
    pub signal_source: SignalSource,   // ImplicitOutcome | ImplicitRework
}
pub enum SignalType { Helpful, Flagged }
pub enum SignalSource { ImplicitOutcome, ImplicitRework }
```

Store write: `insert_signal(record)` — allocates next_signal_id from COUNTERS, bincode-serializes, writes to SIGNAL_QUEUE.
Store drain: `drain_signals(type_filter) -> Vec<SignalRecord>` — reads all matching records, deletes them in the same transaction (at-most-once processing).

**2. Session-Scoped Signal Dedup (server layer)**

Extend `SessionState` in `session.rs` with:
- `signaled_entries: HashSet<u64>` — tracks entry_ids that have already generated a Helpful implicit signal for this session
- `rework_context: ReworkContext` — tracks edit sequences for rework detection

Add `SessionRegistry::generate_signals(session_id)` → returns `(Vec<u64> // helpful_entries, Vec<u64> // flagged_entries)` with dedup applied. Called from both the SessionClose handler and the stale session sweep.

**3. Signal Generation and Processing (UDS listener / server layer)**

In `uds_listener.rs`, extend `dispatch_request()`:
- `HookRequest::SessionClose { outcome: "success", .. }` → call `generate_signals` → write `Helpful` SignalRecords → trigger confidence consumer
- `HookRequest::SessionClose { outcome: "rework", .. }` → call `generate_signals` → write `Flagged` SignalRecords → trigger retrospective consumer
- `HookRequest::SessionClose { outcome: "" | "abandoned" | None, .. }` → no signals, just clear session
- New: stale session sweep before each SessionClose (scan sessions, evict expired, generate signals for them)

Dual-consumer processing (called synchronously after signal write):
- **Confidence consumer**: `drain_signals(SignalType::Helpful)` → for each SignalRecord, for each entry_id in record.entry_ids, call existing `helpful_count` increment + confidence recompute. Batch by entry_id (multiple sessions may signal the same entry).
- **Retrospective consumer**: `drain_signals(SignalType::Flagged)` → accumulate into `Vec<EntryAnalysis>` stored in a server-side `PendingEntriesAnalysis` struct. The next `context_retrospective` call picks this up and includes it in `entries_analysis`.

**4. PostToolUse Rework Detection (hook.rs + server-side state)**

In `hook.rs` `build_request()`, add `"PostToolUse"` arm:
- Extract `tool_name`, `exit_code` / `error` from `input.extra`
- Build a `RecordEvent` with `event_type = "post_tool_use_rework_candidate"` if rework indicators present
- Server-side: `dispatch_request()` handles `RecordEvent(event_type="post_tool_use_rework_candidate")` by calling `SessionRegistry::record_rework_event(session_id, tool_name, had_error)`
- Rework detection logic in `SessionState::check_rework_threshold()` — returns `Some(Vec<u64>)` (flagged entry IDs) when threshold crossed, `None` otherwise

**5. RetrospectiveReport entries_analysis Extension (unimatrix-observe)**

Add to `crates/unimatrix-observe/src/types.rs`:
```rust
pub struct EntryAnalysis {
    pub entry_id: u64,
    pub title: String,
    pub category: String,
    pub rework_flag_count: u32,
    pub injection_count: u32,
    pub success_session_count: u32,
    pub rework_session_count: u32,
}
// On RetrospectiveReport:
pub entries_analysis: Option<Vec<EntryAnalysis>>,
```

Add `#[serde(default)]` — additive, backward compatible. Update `build_report()` signature to accept `entries_analysis: Option<Vec<EntryAnalysis>>`. The server passes the accumulated `PendingEntriesAnalysis` when calling into the observe crate.

## Acceptance Criteria

- AC-01: Schema v4 migration runs on `Store::open()` when schema version is 3 — creates SIGNAL_QUEUE table and writes `next_signal_id = 0` to COUNTERS. Schema version increments to 4. All existing entries and indexes survive migration intact.
- AC-02: `SignalRecord` is written to SIGNAL_QUEUE on session end: for a session with 3 distinct injected entries and a `"success"` outcome, exactly one `Helpful` SignalRecord with `entry_ids = [id1, id2, id3]` is written.
- AC-03: Session-scoped dedup: if `SessionClose` is called twice for the same session (e.g., once from stale sweep and once from real hook), only one set of Helpful signals is generated per (session_id, entry_id) pair.
- AC-04: Confidence consumer: for each entry_id in a `Helpful` SignalRecord, `EntryRecord.helpful_count` is incremented exactly once, and `confidence` is recomputed using the existing crt-002 pipeline.
- AC-05: Abandoned sessions (empty or missing outcome) produce no signals.
- AC-06: Sessions with `"rework"` outcome produce `Flagged` SignalRecords (not `Helpful`). No `EntryRecord.helpful_count` or `unhelpful_count` is modified.
- AC-07: `Flagged` signals from the retrospective consumer accumulate in `entries_analysis` and are returned in the next `context_retrospective` response as `EntryAnalysis` entries with non-zero `rework_flag_count`.
- AC-08: PostToolUse rework detection: after 3 consecutive `cargo build`/`cargo test` failures in a session, a `Flagged` signal is generated for the session's injected entries.
- AC-09: Stale session sweep: sessions with no activity for >= 4 hours (measured by last injection timestamp) are processed and evicted during the next `SessionClose` or `maintain=true` call.
- AC-10: SIGNAL_QUEUE is capped at 10,000 records. When the cap is reached, the oldest records are dropped before inserting new ones.
- AC-11: All existing 1,025 unit + 174 integration tests pass without modification after schema v4 migration and the new signal processing code.
- AC-12: Signal consumer processing completes in < 100ms for a batch of 50 entries (measured in integration tests using a test store).
- AC-13: `RetrospectiveReport.entries_analysis` is `None` when no Flagged signals have been generated, preserving backward compatibility with existing `context_retrospective` callers.

## Constraints

### Hard Constraints

- **No auto-downweighting**: Flagged signals never touch `unhelpful_count`. This is a product invariant, not a tuning parameter.
- **Schema migration pattern**: Follow exactly the 3-step pattern from prior migrations (schema.rs constant bump + `migrate_vN_to_vN+1()` function + `migrate_if_needed()` call on open). SIGNAL_QUEUE migration is simpler than prior ones — no entry scan-and-rewrite.
- **In-memory only for injection history**: col-009 reads from `SessionState.injection_history`. It does NOT add redb reads of injection history.
- **Backward compatibility**: `RetrospectiveReport.entries_analysis` uses `#[serde(default)]`. No breaking change to the MCP tool response.
- **Zero regression**: All 1,025 unit + 174 integration tests must pass. Existing MCP tools and hook handlers work identically.
- **Session-scoped dedup is idempotent**: The `(session_id, entry_id)` dedup set must prevent double-signaling even if signal generation runs twice.
- **Edition 2024, MSRV 1.89**: Workspace constraints inherited.

### Soft Constraints

- **Rework detection thresholds are conservative**: Start with high thresholds (3+ consecutive failures, same-file edit+fail+edit). Tune empirically after delivery.
- **Confidence consumer is synchronous**: Signal processing happens inline during SessionClose dispatch (not in a background task). This keeps the architecture simple — sessions close infrequently and batch sizes are small.
- **SIGNAL_QUEUE is ephemeral**: Records are deleted after drain. If the server crashes between write and drain, the signals are lost (not reprocessed). This is acceptable — the Wilson minimum-sample guard prevents a single lost batch from mattering.
- **Stale sweep threshold (4h)**: Tunable via named constant. Not user-configurable in v1.

### Dependencies

- **col-008** (hard, COMPLETE): `SessionRegistry` with `InjectionRecord`, `SessionState.injection_history`, `record_injection()`, `get_state()`, `clear_session()` — all required
- **col-007** (hard, COMPLETE): Populates `injection_history` via `record_injection()` on every `ContextSearch` dispatch — required for signals to have entries to reference
- **crt-001** (existing): `helpful_count` increment pathway, `UsageDedup` pattern — extended with session-scoped dedup
- **crt-002** (existing): Confidence recompute formula — unchanged, called after `helpful_count` increment
- **col-002** (existing): `RetrospectiveReport` — extended with `entries_analysis` field

### Downstream Dependents

| Feature | What It Needs from col-009 |
|---------|---------------------------|
| col-010 | Builds on SIGNAL_QUEUE (schema v4) and SignalRecord types; adds SESSIONS + INJECTION_LOG (schema v5) |

## Resolved Design Decisions (Human-Approved)

1. **Rework detection state location** — Server-side in `SessionState`. Add `rework_events: Vec<ReworkEvent>` to `SessionState`. Hook process is ephemeral and stateless across invocations; server-side tracking is the only viable approach.

2. **PostToolUse rework detection threshold** — Conservative. Rework signal fires only when: Edit/Write to a file → failed Bash (non-zero exit) → Edit/Write to the same file again, repeated **3+ times**. Rapid succession edits to the same file with no intervening failure are normal multi-section update behavior and must NOT be flagged. The intervening failure is required. Single multi-edit passes are never rework.

3. **Outcome field authority** — **Hook is sole authority.** Session outcome for signal purposes is entirely hook-derived: `Stop`/`TaskCompleted` → `"success"` in wire request; server checks `rework_events` and overrides to `"rework"` if threshold crossed; no activity / forced quit → `"abandoned"`. Agent MCP writes (e.g., `context_store(category: "outcome")`) are a separate, orthogonal system and do not influence session signals. Agent explicit voice for signal purposes is via `helpful=false` MCP votes only. Revisit if operational friction emerges.

4. **PendingEntriesAnalysis location** — In-memory field on `McpServer`: `pending_entries_analysis: Mutex<Vec<EntryAnalysis>>`. No schema change. Consistent with server-side ephemeral state pattern. The next `context_retrospective` call drains and includes it. **col-010 alignment note**: col-010's `from_structured_events()` is the right place to unify entry-level signal data with the structured retrospective pipeline. col-009 stays additive (`entries_analysis` field) and does not block that alignment.

5. **Signal queue overflow** — Drop oldest at 10,000 records. Lost signals are informationally equivalent to a smaller Wilson sample. The 5-vote minimum guard means isolated losses are non-damaging. Simplicity over guaranteed delivery.

6. **Session Intent Registry** — Generic in-memory mechanism for intercepting purposeful agent MCP actions during a session. Lives as `agent_actions: Vec<SessionAction>` on `SessionState` — not a redb table. Populated whenever the UDS listener observes an agent MCP call (correct, deprecate, explicit helpful/unhelpful vote) that touches an entry in the session's injection history. Consulted at signal-generation time before emitting any implicit signal. `SessionAction { entry_id: u64, action: AgentActionType, timestamp: u64 }` where `AgentActionType` covers `ExplicitUnhelpful | ExplicitHelpful | Correction | Deprecation`. The immediate use case: entries with `ExplicitUnhelpful` in the session are excluded from implicit Helpful signal generation. The generic design accommodates future intercept criteria (e.g., corrected entries get higher compaction weight, deprecated entries excluded from re-injection) without structural changes.

## Tracking

- GH Issue: https://github.com/dug-21/unimatrix/issues/73
