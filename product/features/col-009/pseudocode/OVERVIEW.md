# Pseudocode Overview: col-009 Closed-Loop Confidence

## Components Involved

| Component | File | Crates Touched |
|-----------|------|----------------|
| signal-store | `pseudocode/signal-store.md` | `unimatrix-store` |
| session-signals | `pseudocode/session-signals.md` | `unimatrix-server` |
| signal-dispatch | `pseudocode/signal-dispatch.md` | `unimatrix-server` |
| hook-posttooluse | `pseudocode/hook-posttooluse.md` | `unimatrix-server` |
| entries-analysis | `pseudocode/entries-analysis.md` | `unimatrix-server`, `unimatrix-observe` |

## Data Flow

```
Claude Code (PostToolUse hook)
  → hook.rs build_request("PostToolUse")
  → HookRequest::RecordEvent { event_type: "post_tool_use_rework_candidate" }
  → UDS listener: session_registry.record_rework_event()
  → SessionState.rework_events accumulated

Claude Code (Stop hook)
  → hook.rs build_request("Stop")
  → HookRequest::SessionClose { outcome: "success" }
  → UDS listener: process_session_close()
    1. sweep_stale_sessions() → stale signal outputs → SIGNAL_QUEUE
    2. drain_and_signal_session(session_id, "success") → SignalOutput
       └─ evaluates rework_events → may override to Rework
       └─ excludes ExplicitUnhelpful entries from helpful set
    3. write_signals_to_queue(SignalOutput) → Store::insert_signal()
    4. clear handled by drain_and_signal_session (atomic)
    5. run_confidence_consumer() → drain Helpful → helpful_count increments
                                  → also update success_session_count in PendingEntriesAnalysis
    6. run_retrospective_consumer() → drain Flagged → PendingEntriesAnalysis updated

context_retrospective tool call
  → drain pending_entries_analysis
  → build_report(..., entries_analysis: Some(drained))
  → RetrospectiveReport { entries_analysis }
```

## Shared Types (new or modified)

### New in `unimatrix-store/src/signal.rs`

```rust
// LAYOUT FROZEN: bincode v2 positional encoding. Fields may only be APPENDED.
// See ADR-001 (col-009). Do not reorder or remove fields.
pub struct SignalRecord {
    pub signal_id: u64,
    pub session_id: String,
    pub created_at: u64,
    pub entry_ids: Vec<u64>,
    pub signal_type: SignalType,
    pub signal_source: SignalSource,
}

#[repr(u8)]
pub enum SignalType { Helpful = 0, Flagged = 1 }

#[repr(u8)]
pub enum SignalSource { ImplicitOutcome = 0, ImplicitRework = 1 }
```

### New in `unimatrix-server/src/session.rs`

```rust
// New fields on SessionState
pub signaled_entries: HashSet<u64>
pub rework_events: Vec<ReworkEvent>
pub agent_actions: Vec<SessionAction>
pub last_activity_at: u64

pub struct ReworkEvent { tool_name, file_path, had_failure, timestamp }
pub struct SessionAction { entry_id, action: AgentActionType, timestamp }
pub enum AgentActionType { ExplicitUnhelpful, ExplicitHelpful, Correction, Deprecation }
pub struct SignalOutput { session_id, helpful_entry_ids, flagged_entry_ids, final_outcome }
pub enum SessionOutcome { Success, Rework, Abandoned }
```

### New in `unimatrix-server/src/server.rs`

```rust
pub struct PendingEntriesAnalysis {
    pub entries: HashMap<u64, EntryAnalysis>,  // entry_id -> analysis
    pub created_at: u64,
}
// Field on McpServer: pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>
```

### New in `unimatrix-observe/src/types.rs`

```rust
pub struct EntryAnalysis {
    pub entry_id: u64, pub title: String, pub category: String,
    pub rework_flag_count: u32, pub injection_count: u32,
    pub success_session_count: u32, pub rework_session_count: u32,
}
// Added to RetrospectiveReport:
pub entries_analysis: Option<Vec<EntryAnalysis>>
```

## Sequencing Constraints

1. **signal-store** must be built first — defines `SignalRecord`, `SIGNAL_QUEUE`, `insert_signal`, `drain_signals`
2. **session-signals** depends on `SignalRecord` types from signal-store
3. **entries-analysis** (observe layer) can be built in parallel with signal-store and session-signals
4. **signal-dispatch** depends on session-signals and entries-analysis being complete (calls both)
5. **hook-posttooluse** can be built in parallel with all others (no dependencies on new types except `ReworkEvent` which lives in session-signals)

## Build Order for Implementation Agents

```
Wave 1 (parallel): signal-store, entries-analysis, hook-posttooluse
Wave 2 (parallel): session-signals (uses SignalRecord)
Wave 3: signal-dispatch (uses all of the above)
```

However, since all types are defined in the IMPLEMENTATION-BRIEF and pseudocode, a single-wave parallel build is achievable if agents define types locally and trust the brief's signatures.
