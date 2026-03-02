# Architecture: col-009 Closed-Loop Confidence

## System Overview

col-009 closes the confidence feedback loop for hook-injected knowledge. When col-007 injects entries into an agent's context via `UserPromptSubmit`, those entries shape agent behaviour — but until col-009, no feedback signal returns to the confidence pipeline. Explicit agent votes (`helpful=true/false` on MCP tools) are rare. col-009 derives implicit signals from session outcomes automatically.

The feature sits at the intersection of four existing subsystems:
- **Session layer** (col-008): `SessionRegistry` and `SessionState` — the source of injection history
- **Store layer** (crt-001/crt-002): `helpful_count` increment and confidence recompute — the sink for Helpful signals
- **Hook layer** (col-006): `PostToolUse` and `Stop` hooks — the event sources for rework and session-end detection
- **Observe layer** (col-002): `RetrospectiveReport` — the sink for Flagged signals

col-009 adds one new redb table (SIGNAL_QUEUE), three extensions to existing in-memory structs, one new module for signal records, and two new wire protocol variants. No existing MCP tools change behaviour.

## Component Breakdown

### Component 1: SIGNAL_QUEUE Table and SignalRecord (store layer)

**Location**: `crates/unimatrix-store/src/` — new file `signal.rs`, modifications to `schema.rs` and `migration.rs`

**Responsibility**: Persist confidence signals between generation (SessionClose) and consumption (confidence pipeline / retrospective pipeline). Acts as a transient work queue — records are deleted after drain.

**New types**:
```rust
// crates/unimatrix-store/src/signal.rs
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignalRecord {
    pub signal_id: u64,
    pub session_id: String,
    pub created_at: u64,          // Unix seconds
    pub entry_ids: Vec<u64>,      // deduplicated, from injection_history
    pub signal_type: SignalType,
    pub signal_source: SignalSource,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum SignalType { Helpful, Flagged }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum SignalSource { ImplicitOutcome, ImplicitRework }
```

**Schema change** (migration.rs):
- Bump `CURRENT_SCHEMA_VERSION` from 3 to 4
- Add `migrate_v3_to_v4()`: opens SIGNAL_QUEUE table (triggers redb creation), writes `next_signal_id = 0` to COUNTERS
- No entry scan-and-rewrite (SIGNAL_QUEUE is new; no existing data to migrate)

**Store methods** (added to `Store` in `db.rs` or `write.rs`/`read.rs`):
```rust
pub fn insert_signal(&self, record: &SignalRecord) -> Result<u64>; // returns signal_id
pub fn drain_signals(&self, signal_type: SignalType) -> Result<Vec<SignalRecord>>;
// drain_signals reads ALL matching records and deletes them in one write transaction
pub fn signal_queue_len(&self) -> Result<u64>;  // for cap enforcement
```

**Queue cap**: Before `insert_signal`, if `signal_queue_len() >= 10_000`, delete the oldest `entry_count - 9_999` records (ordered by signal_id ascending) in the same write transaction.

### Component 2: SessionState Extensions (session layer)

**Location**: `crates/unimatrix-server/src/session.rs` — modifications to existing `SessionState` and `SessionRegistry`

**Responsibility**: Track which entries have already generated a signal (dedup), accumulate rework events for threshold evaluation, and record agent MCP actions that intercept implicit signal generation.

**New fields on `SessionState`**:
```rust
pub signaled_entries: HashSet<u64>,           // entries that already got an implicit Helpful signal
pub rework_events: Vec<ReworkEvent>,          // PostToolUse observations for rework detection
pub agent_actions: Vec<SessionAction>,        // explicit MCP actions observed during session
pub last_activity_at: u64,                    // max(last_injection_ts, last_rework_ts, registration_ts)
```

**New types**:
```rust
pub struct ReworkEvent {
    pub tool_name: String,       // "Bash", "Edit", "Write", "MultiEdit"
    pub file_path: Option<String>, // for Edit/Write
    pub had_failure: bool,       // true if Bash exit_code != 0
    pub timestamp: u64,
}

pub struct SessionAction {
    pub entry_id: u64,
    pub action: AgentActionType,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AgentActionType {
    ExplicitUnhelpful,
    ExplicitHelpful,
    Correction,      // context_correct was called on this entry
    Deprecation,     // context_deprecate was called on this entry
}
```

**New `SessionRegistry` methods**:
```rust
// Record a rework observation; updates last_activity_at
pub fn record_rework_event(&self, session_id: &str, event: ReworkEvent);

// Record an agent MCP action on an entry in this session
pub fn record_agent_action(&self, session_id: &str, action: SessionAction);

// Generate signals: returns (helpful_entry_ids, flagged_entry_ids).
// Applies dedup (signaled_entries), excludes ExplicitUnhelpful entries from Helpful set.
// Evaluates rework threshold and assigns outcome. Marks session as signaled.
// Atomic: modifies signaled_entries in one lock acquisition.
pub fn generate_signals(&self, session_id: &str, hook_outcome: &str) -> Option<SignalOutput>;

// Sweep: find sessions where last_activity_at < now - STALE_THRESHOLD_SECS.
// Returns Vec<(session_id, SignalOutput)> for each stale session with entries,
// and removes stale sessions from registry.
pub fn sweep_stale_sessions(&self) -> Vec<(String, SignalOutput)>;

pub struct SignalOutput {
    pub session_id: String,
    pub helpful_entry_ids: Vec<u64>,
    pub flagged_entry_ids: Vec<u64>,
    pub final_outcome: SessionOutcome,  // Success | Rework | Abandoned
}

pub enum SessionOutcome { Success, Rework, Abandoned }
```

**Rework threshold logic** (inside `generate_signals`): evaluate `rework_events` against the threshold defined in ADR-002. The hook sets `outcome = "success"` for `Stop`; the server overrides to `"rework"` if the threshold is crossed.

**Stale threshold constant**: `const STALE_SESSION_THRESHOLD_SECS: u64 = 4 * 3600;` (4 hours)

### Component 3: Signal Generation and Processing (UDS listener)

**Location**: `crates/unimatrix-server/src/uds_listener.rs` — extensions to `dispatch_request()`

**Responsibility**: On `SessionClose`, call stale sweep + generate signals for the closing session, write to SIGNAL_QUEUE, then run dual consumers. Also handles `RecordEvent(event_type="post_tool_use_rework_candidate")` to update `SessionState.rework_events`.

**Signal generation flow** (new `process_session_close()` helper):
```
1. sweep_stale_sessions() → Vec<(session_id, SignalOutput)>
   For each stale SignalOutput: write_signals_to_queue(output, store)
2. generate_signals(closing_session_id, hook_outcome) → Option<SignalOutput>
   If Some: write_signals_to_queue(output, store)
3. clear_session(closing_session_id)  — only after signals are written
4. run_confidence_consumer(store)     — drain Helpful signals
5. run_retrospective_consumer(pending_entries_analysis, store)  — drain Flagged signals
```

**Confidence consumer** (`run_confidence_consumer`):
```rust
// drain_signals(Helpful) → Vec<SignalRecord>
// group entry_ids by entry_id (multiple signals may reference same entry)
// for each unique entry_id: entry_store.record_helpfulness(entry_id, true)
//   → increments helpful_count, recomputes confidence (existing crt-002 path)
// Source tag: "hook" (distinguishes from "mcp" explicit votes in audit log)
```

**Retrospective consumer** (`run_retrospective_consumer`):
```rust
// drain_signals(Flagged) → Vec<SignalRecord>
// for each SignalRecord: update PendingEntriesAnalysis for each entry_id
//   → increment rework_flag_count, rework_session_count
// If entry not yet in PendingEntriesAnalysis: fetch EntryRecord title + category,
//   create new EntryAnalysis entry
```

**RecordEvent dispatch for rework** (new arm in dispatch_request):
```
HookRequest::RecordEvent { event } where event.event_type == "post_tool_use_rework_candidate":
  → parse event.payload: tool_name, file_path, had_failure
  → session_registry.record_rework_event(event.session_id, ReworkEvent {...})
  → return HookResponse::Ack
```

### Component 4: PostToolUse Rework Detection (hook layer)

**Location**: `crates/unimatrix-server/src/hook.rs` — new arm in `build_request()`

**Responsibility**: Parse PostToolUse stdin JSON and send rework-candidate events to the server for rework-eligible tool calls.

**New `"PostToolUse"` arm in `build_request()`**:
```rust
"PostToolUse" => {
    let tool_name = input.extra["tool_name"].as_str().unwrap_or("").to_string();
    // Rework-eligible tools: Bash, Edit, Write, MultiEdit
    if is_rework_eligible_tool(&tool_name) {
        let had_failure = is_failure(&input.extra, &tool_name);
        let file_path = extract_file_path(&input.extra, &tool_name);
        HookRequest::RecordEvent {
            event: ImplantEvent {
                event_type: "post_tool_use_rework_candidate".to_string(),
                session_id,
                timestamp: now_secs(),
                payload: serde_json::json!({
                    "tool_name": tool_name,
                    "file_path": file_path,
                    "had_failure": had_failure,
                }),
            },
        }
    } else {
        // Non-rework tool: generic record (unchanged behaviour)
        HookRequest::RecordEvent { event: generic_record_event(event, &session_id, &input) }
    }
}
```

**Failure detection** (per ADR-002):
- Bash: `input.extra["exit_code"]` is non-zero integer, OR `input.extra["interrupted"]` is true
- Edit/Write/MultiEdit: no failure concept — `had_failure = false`

**File path extraction**:
- Edit/MultiEdit: `input.extra["tool_input"]["path"]` as str
- Write: `input.extra["tool_input"]["file_path"]` as str
- Bash: `None` (command, not path)

**Stop outcome** (new in `build_request()`): Change `"Stop"` arm to set `outcome: Some("success".to_string())`. Server's `generate_signals` overrides to `"rework"` if threshold crossed.

**`is_fire_and_forget` update**: PostToolUse rework-candidate RecordEvent remains fire-and-forget (no change to sync/async classification).

### Component 5: PendingEntriesAnalysis (server state)

**Location**: `crates/unimatrix-server/src/server.rs` (or wherever `McpServer` struct is defined) — new field

**Responsibility**: Accumulate `EntryAnalysis` from Flagged signal drains between `context_retrospective` calls.

```rust
// On McpServer:
pub pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,

pub struct PendingEntriesAnalysis {
    pub entries: HashMap<u64, EntryAnalysis>,  // entry_id -> analysis
    pub created_at: u64,
    // Cap: if entries.len() >= 1000, drop the entry with lowest rework_flag_count
}
```

**Drain on `context_retrospective`**: Before building the report, drain `pending_entries_analysis.entries.values().cloned().collect()`, clear the map, pass to `build_report()` as `entries_analysis`.

### Component 6: RetrospectiveReport Extension (observe layer)

**Location**: `crates/unimatrix-observe/src/types.rs` — additive change to `RetrospectiveReport`

**New field** (backward-compatible, `#[serde(default)]`):
```rust
pub entries_analysis: Option<Vec<EntryAnalysis>>,
```

**New type**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntryAnalysis {
    pub entry_id: u64,
    pub title: String,
    pub category: String,
    pub rework_flag_count: u32,
    pub injection_count: u32,
    pub success_session_count: u32,
    pub rework_session_count: u32,
}
```

**`build_report()` signature update**:
```rust
pub fn build_report(
    feature_cycle: &str,
    records: &[ObservationRecord],
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
    baseline: Option<Vec<BaselineComparison>>,
    entries_analysis: Option<Vec<EntryAnalysis>>,  // NEW
) -> RetrospectiveReport
```

## Component Interactions

```
Claude Code (PostToolUse hook)
    │  stdin: { tool_name, exit_code, tool_input, session_id }
    ▼
hook.rs build_request("PostToolUse")
    │  → HookRequest::RecordEvent { event_type: "post_tool_use_rework_candidate" }
    ▼
UDS listener dispatch_request()
    │  → session_registry.record_rework_event(session_id, ReworkEvent)
    ▼
SessionState.rework_events: Vec<ReworkEvent>   [accumulated]

Claude Code (Stop hook)
    │  stdin: { session_id }
    ▼
hook.rs build_request("Stop")
    │  → HookRequest::SessionClose { outcome: "success" }
    ▼
UDS listener dispatch_request() → process_session_close()
    │  1. sweep_stale_sessions() → stale signal outputs
    │  2. generate_signals(session_id, "success") → SignalOutput
    │     └─ evaluates rework_events → may override outcome to "rework"
    │     └─ filters ExplicitUnhelpful entries from helpful set (Session Intent Registry)
    │  3. Store::insert_signal() → SIGNAL_QUEUE (redb)
    │  4. clear_session(session_id)
    │  5. run_confidence_consumer()
    │     └─ Store::drain_signals(Helpful)
    │     └─ entry_store.record_helpfulness(entry_id, true) per entry
    │        └─ helpful_count += 1, confidence recomputed (crt-002)
    │  6. run_retrospective_consumer(pending_entries_analysis)
    │     └─ Store::drain_signals(Flagged)
    │     └─ PendingEntriesAnalysis.entries updated

context_retrospective MCP tool call
    ▼
Server handler
    │  → drain pending_entries_analysis
    │  → build_report(..., entries_analysis: Some(drained))
    ▼
RetrospectiveReport { entries_analysis: Some(Vec<EntryAnalysis>) }
```

## Technology Decisions

See ADRs:
- ADR-001: SignalRecord field order locked at shipping (bincode v2 positional encoding)
- ADR-002: Rework threshold definition (edit-fail-edit × 3, server-side evaluation)
- ADR-003: `drain_and_signal` atomicity — generate_signals + clear_session in locked scope

## Integration Points

### Existing Components Consumed

| Component | Interface Used | What col-009 Does |
|-----------|---------------|-------------------|
| `SessionState.injection_history` | `Vec<InjectionRecord>` | Read to generate helpful entry set |
| `SessionRegistry.clear_session()` | `fn clear_session(&self, session_id: &str)` | Must run AFTER signal write |
| `entry_store.record_helpfulness()` | crt-001 pathway | Increments helpful_count, triggers confidence recompute |
| `Store` (redb) | `db: Arc<redb::Database>` | Writes/drains SIGNAL_QUEUE |
| `RetrospectiveReport` | `build_report()` | Accepts new entries_analysis param |

### New Interfaces Introduced

| Interface | Type | Location |
|-----------|------|----------|
| `Store::insert_signal` | `fn(&SignalRecord) -> Result<u64>` | `unimatrix-store` |
| `Store::drain_signals` | `fn(SignalType) -> Result<Vec<SignalRecord>>` | `unimatrix-store` |
| `Store::signal_queue_len` | `fn() -> Result<u64>` | `unimatrix-store` |
| `SessionRegistry::generate_signals` | `fn(&str, &str) -> Option<SignalOutput>` | `unimatrix-server` |
| `SessionRegistry::sweep_stale_sessions` | `fn() -> Vec<(String, SignalOutput)>` | `unimatrix-server` |
| `SessionRegistry::record_rework_event` | `fn(&str, ReworkEvent)` | `unimatrix-server` |
| `SessionRegistry::record_agent_action` | `fn(&str, SessionAction)` | `unimatrix-server` |
| `EntryAnalysis` struct | `pub struct EntryAnalysis { ... }` | `unimatrix-observe` |
| `PendingEntriesAnalysis` | `pub struct PendingEntriesAnalysis { entries: HashMap<u64, EntryAnalysis> }` | `unimatrix-server` |

## Integration Surface

| Integration Point | Type/Signature | Source | Notes |
|-------------------|---------------|--------|-------|
| `SIGNAL_QUEUE` table | `TableDefinition<u64, &[u8]>` | `schema.rs` | Key = next_signal_id |
| `SignalRecord` bincode layout | See Component 1 struct | `signal.rs` | Field order FROZEN per ADR-001 |
| `SessionState.signaled_entries` | `HashSet<u64>` | `session.rs` | Dedup set, never persisted |
| `SessionState.rework_events` | `Vec<ReworkEvent>` | `session.rs` | Server-side accumulation |
| `SessionState.agent_actions` | `Vec<SessionAction>` | `session.rs` | Session Intent Registry |
| `SessionState.last_activity_at` | `u64` (Unix seconds) | `session.rs` | Updated on inject + rework |
| `PendingEntriesAnalysis` on McpServer | `Arc<Mutex<PendingEntriesAnalysis>>` | `server.rs` | Drained by context_retrospective |
| `build_report(..., entries_analysis)` | `Option<Vec<EntryAnalysis>>` param | `report.rs` | New 6th param, callers updated |
| PostToolUse stdin fields | `extra["tool_name"]`, `extra["exit_code"]`, `extra["tool_input"]["path"]` | hook.rs | Claude Code hook JSON |

## Data Flow: Signal Queue Lifecycle

```
SessionClose (Stop hook) fires
    │
    ├─ [sweep] stale sessions → SignalRecord(s) → SIGNAL_QUEUE
    │
    ├─ [generate] current session → SignalRecord → SIGNAL_QUEUE
    │     └─ signal_type: Helpful  (if outcome == success AND rework threshold NOT crossed)
    │     └─ signal_type: Flagged  (if rework threshold crossed)
    │
    ├─ [consume] drain Helpful → helpful_count increments (redb ENTRIES table)
    │
    ├─ [consume] drain Flagged → PendingEntriesAnalysis (in-memory)
    │
    └─ [clear] SessionState removed from SessionRegistry

context_retrospective called
    └─ drain PendingEntriesAnalysis → entries_analysis in RetrospectiveReport
```

## Error Handling and Graceful Degradation

- If `drain_signals` fails (redb error): log warning, skip consumer — do not crash. Unprocessed signals remain in SIGNAL_QUEUE for next drain attempt.
- If `record_helpfulness` fails for a specific entry (entry deleted since injection): log warning, skip that entry, continue with remaining entries.
- If `sweep_stale_sessions` yields no results: no-op, proceed to current session processing.
- If hook process loses connection before Stop fires (crash): stale sweep handles orphaned session on next SessionClose from any session.
- `PendingEntriesAnalysis` cap (1000 entries): drop the entry with lowest `rework_flag_count` when cap reached.

## Files Modified or Created

| File | Change Type | Description |
|------|-------------|-------------|
| `crates/unimatrix-store/src/signal.rs` | NEW | `SignalRecord`, `SignalType`, `SignalSource` |
| `crates/unimatrix-store/src/schema.rs` | MODIFY | Add `SIGNAL_QUEUE` table definition |
| `crates/unimatrix-store/src/migration.rs` | MODIFY | `CURRENT_SCHEMA_VERSION = 4`, `migrate_v3_to_v4()` |
| `crates/unimatrix-store/src/db.rs` | MODIFY | Add `insert_signal`, `drain_signals`, `signal_queue_len` |
| `crates/unimatrix-store/src/lib.rs` | MODIFY | Re-export `signal` module types |
| `crates/unimatrix-server/src/session.rs` | MODIFY | New fields on `SessionState`, new `SessionRegistry` methods |
| `crates/unimatrix-server/src/uds_listener.rs` | MODIFY | Signal generation dispatch, rework RecordEvent arm |
| `crates/unimatrix-server/src/hook.rs` | MODIFY | PostToolUse arm, Stop outcome field |
| `crates/unimatrix-server/src/server.rs` | MODIFY | `pending_entries_analysis` field on McpServer |
| `crates/unimatrix-observe/src/types.rs` | MODIFY | `entries_analysis` on `RetrospectiveReport`, new `EntryAnalysis` |
| `crates/unimatrix-observe/src/report.rs` | MODIFY | `build_report()` new parameter |
| `.claude/settings.json` | MODIFY | Add PostToolUse hook registration |
