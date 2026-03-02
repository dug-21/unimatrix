# Implementation Brief: col-009 Closed-Loop Confidence

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-009/SCOPE.md |
| Scope Risk Assessment | product/features/col-009/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-009/architecture/ARCHITECTURE.md |
| ADR-001: SignalRecord Field Order | product/features/col-009/architecture/ADR-001-signal-record-field-order.md |
| ADR-002: Rework Threshold | product/features/col-009/architecture/ADR-002-rework-threshold.md |
| ADR-003: Atomicity | product/features/col-009/architecture/ADR-003-generate-and-clear-atomicity.md |
| Specification | product/features/col-009/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-009/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-009/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| signal-store | pseudocode/signal-store.md | test-plan/signal-store.md |
| session-signals | pseudocode/session-signals.md | test-plan/session-signals.md |
| signal-dispatch | pseudocode/signal-dispatch.md | test-plan/signal-dispatch.md |
| hook-posttooluse | pseudocode/hook-posttooluse.md | test-plan/hook-posttooluse.md |
| entries-analysis | pseudocode/entries-analysis.md | test-plan/entries-analysis.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/col-009/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/col-009/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

col-009 closes the confidence feedback loop for hook-injected knowledge by deriving implicit helpfulness signals from session outcomes — without any agent cooperation. A successful session applies bulk `helpful=true` to all injected entries via the existing crt-002 confidence pipeline. Rework sessions flag entries for human review in the retrospective pipeline, never auto-downweighting. Schema v4 adds the SIGNAL_QUEUE table (15th) as a transient work queue owned and validated by this feature.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| SignalRecord field order | Fields frozen: signal_id, session_id, created_at, entry_ids, signal_type, signal_source. Explicit enum discriminants. `// LAYOUT FROZEN` comment in source. | ADR-001 | architecture/ADR-001-signal-record-field-order.md |
| Rework threshold definition | Edit-fail-edit × 3 on the same file path per session. Intervening Bash failure (exit_code != 0 or interrupted=true) required between consecutive edits. 3 is the cycle count threshold. | ADR-002 | architecture/ADR-002-rework-threshold.md |
| generate_signals + clear_session atomicity | Single `drain_and_signal_session()` method holds Mutex for generate + clear. No separate lock acquisitions. Eliminates race with stale session sweep. | ADR-003 | architecture/ADR-003-generate-and-clear-atomicity.md |
| Rework detection state location | Server-side in `SessionState.rework_events: Vec<ReworkEvent>`. Hook process is stateless across invocations. | SCOPE Resolved Decision #1 | — |
| PostToolUse rework threshold | Edit-fail-edit × 3. Rapid multi-edits with no intervening failure are NOT rework. | SCOPE Resolved Decision #2 | — |
| Outcome field authority | Hook is sole authority. `Stop` → `outcome = "success"`. Server overrides to `"rework"` if rework threshold crossed. | SCOPE Resolved Decision #3 | — |
| PendingEntriesAnalysis location | In-memory field on McpServer: `pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>`. No schema change. | SCOPE Resolved Decision #4 | — |
| Signal queue overflow | Drop oldest at 10,000 records. Lost signals = smaller Wilson sample. Acceptable. | SCOPE Resolved Decision #5 | — |
| Session Intent Registry | `agent_actions: Vec<SessionAction>` on `SessionState`. Entries with ExplicitUnhelpful excluded from Helpful signals. | SCOPE Resolved Decision #6 | — |
| No auto-downweighting | Flagged signals never touch `unhelpful_count`. Product invariant. | SCOPE Non-Goals | — |
| Schema migration pattern | v3→v4: open SIGNAL_QUEUE table + write `next_signal_id=0`. No entry scan-and-rewrite. | ARCHITECTURE.md Component 1 | — |

## Files to Create or Modify

| File | Change | Description |
|------|--------|-------------|
| `crates/unimatrix-store/src/signal.rs` | CREATE | `SignalRecord`, `SignalType`, `SignalSource` structs and enums with bincode layout |
| `crates/unimatrix-store/src/schema.rs` | MODIFY | Add `SIGNAL_QUEUE: TableDefinition<u64, &[u8]>` |
| `crates/unimatrix-store/src/migration.rs` | MODIFY | Bump `CURRENT_SCHEMA_VERSION` to 4; add `migrate_v3_to_v4()` |
| `crates/unimatrix-store/src/db.rs` | MODIFY | Add `insert_signal()`, `drain_signals()`, `signal_queue_len()` methods to `Store` |
| `crates/unimatrix-store/src/lib.rs` | MODIFY | Re-export `signal` module public types |
| `crates/unimatrix-server/src/session.rs` | MODIFY | New fields on `SessionState`; new `SessionRegistry` methods: `drain_and_signal_session`, `sweep_stale_sessions`, `record_rework_event`, `record_agent_action` |
| `crates/unimatrix-server/src/uds_listener.rs` | MODIFY | Add PostToolUse rework dispatch; add `process_session_close()` helper; call sweep + signal generation + consumers |
| `crates/unimatrix-server/src/hook.rs` | MODIFY | Add `"PostToolUse"` arm in `build_request()`; update `"Stop"` arm to set `outcome = "success"` |
| `crates/unimatrix-server/src/server.rs` | MODIFY | Add `pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>` field; drain in `context_retrospective` handler |
| `crates/unimatrix-observe/src/types.rs` | MODIFY | Add `EntryAnalysis` struct; add `entries_analysis: Option<Vec<EntryAnalysis>>` to `RetrospectiveReport` |
| `crates/unimatrix-observe/src/report.rs` | MODIFY | Add `entries_analysis: Option<Vec<EntryAnalysis>>` param to `build_report()`; update all callers |
| `.claude/settings.json` | MODIFY | Add `PostToolUse` hook registration |

## Data Structures

### SignalRecord (new — `crates/unimatrix-store/src/signal.rs`)
```rust
// LAYOUT FROZEN: bincode v2 positional. Fields append-only. See ADR-001.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignalRecord {
    pub signal_id: u64,
    pub session_id: String,
    pub created_at: u64,
    pub entry_ids: Vec<u64>,
    pub signal_type: SignalType,
    pub signal_source: SignalSource,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalType { Helpful = 0, Flagged = 1 }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalSource { ImplicitOutcome = 0, ImplicitRework = 1 }
```

### SessionState new fields (`crates/unimatrix-server/src/session.rs`)
```rust
pub signaled_entries: HashSet<u64>,
pub rework_events: Vec<ReworkEvent>,
pub agent_actions: Vec<SessionAction>,
pub last_activity_at: u64,
```

### ReworkEvent (new)
```rust
pub struct ReworkEvent {
    pub tool_name: String,
    pub file_path: Option<String>,
    pub had_failure: bool,
    pub timestamp: u64,
}
```

### SessionAction (new)
```rust
pub struct SessionAction {
    pub entry_id: u64,
    pub action: AgentActionType,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AgentActionType {
    ExplicitUnhelpful,
    ExplicitHelpful,
    Correction,
    Deprecation,
}
```

### SignalOutput (new)
```rust
pub struct SignalOutput {
    pub session_id: String,
    pub helpful_entry_ids: Vec<u64>,
    pub flagged_entry_ids: Vec<u64>,
    pub final_outcome: SessionOutcome,
}

pub enum SessionOutcome { Success, Rework, Abandoned }
```

### PendingEntriesAnalysis (new — `crates/unimatrix-server/src/server.rs`)
```rust
pub struct PendingEntriesAnalysis {
    pub entries: HashMap<u64, EntryAnalysis>,  // entry_id -> analysis
    pub created_at: u64,
}
// Cap: 1,000 entries. Evict lowest rework_flag_count when exceeded.
```

### EntryAnalysis (new — `crates/unimatrix-observe/src/types.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntryAnalysis {
    pub entry_id: u64,
    pub title: String,
    pub category: String,
    pub rework_flag_count: u32,
    pub injection_count: u32,       // populated as 0 in col-009; col-010 provides data
    pub success_session_count: u32,
    pub rework_session_count: u32,
}
```

## Function Signatures

### Store methods (new)
```rust
// crates/unimatrix-store/src/db.rs
impl Store {
    pub fn insert_signal(&self, record: &SignalRecord) -> Result<u64>;
    pub fn drain_signals(&self, signal_type: SignalType) -> Result<Vec<SignalRecord>>;
    pub fn signal_queue_len(&self) -> Result<u64>;
}
```

### SessionRegistry methods (new)
```rust
// crates/unimatrix-server/src/session.rs
impl SessionRegistry {
    pub fn drain_and_signal_session(
        &self,
        session_id: &str,
        hook_outcome: &str,
    ) -> Option<SignalOutput>;

    pub fn sweep_stale_sessions(&self) -> Vec<(String, SignalOutput)>;

    pub fn record_rework_event(&self, session_id: &str, event: ReworkEvent);

    pub fn record_agent_action(&self, session_id: &str, action: SessionAction);

    // internal, called within drain_and_signal_session
    fn has_crossed_rework_threshold(state: &SessionState) -> bool;
}
```

### UDS listener helper (new)
```rust
// crates/unimatrix-server/src/uds_listener.rs
async fn process_session_close(
    session_id: &str,
    hook_outcome: &str,
    store: &Store,
    session_registry: &SessionRegistry,
    entry_store: &AsyncEntryStore<StoreAdapter>,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
) -> HookResponse;

async fn run_confidence_consumer(
    store: &Store,
    entry_store: &AsyncEntryStore<StoreAdapter>,
);

async fn run_retrospective_consumer(
    store: &Store,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
    entry_store: &AsyncEntryStore<StoreAdapter>,
);
```

### Hook field extraction helpers (new)
```rust
// crates/unimatrix-server/src/hook.rs
fn is_rework_eligible_tool(tool_name: &str) -> bool;
fn is_bash_failure(extra: &serde_json::Value) -> bool;
fn extract_file_path(extra: &serde_json::Value, tool_name: &str) -> Option<String>;
fn extract_rework_events_for_multiedit(extra: &serde_json::Value) -> Vec<(Option<String>, bool)>;
```

### Schema migration (new)
```rust
// crates/unimatrix-store/src/migration.rs
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 4;  // was 3
fn migrate_v3_to_v4(txn: &redb::WriteTransaction) -> Result<()>;
```

### build_report signature change
```rust
// crates/unimatrix-observe/src/report.rs
pub fn build_report(
    feature_cycle: &str,
    records: &[ObservationRecord],
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
    baseline: Option<Vec<BaselineComparison>>,
    entries_analysis: Option<Vec<EntryAnalysis>>,  // NEW 6th param — pass None for existing callers
) -> RetrospectiveReport
```

## Constraints

- **No auto-downweighting**: `unhelpful_count` is NEVER modified by col-009. Product invariant.
- **Schema migration pattern**: Must follow exactly the 3-step pattern (schema.rs constant + migrate function + migrate_if_needed call). SIGNAL_QUEUE migration has no entry scan-and-rewrite.
- **In-memory only for injection history**: col-009 reads `SessionState.injection_history`. No redb reads of injection history.
- **Backward compatibility**: `RetrospectiveReport.entries_analysis` uses `#[serde(default, skip_serializing_if = "Option::is_none")]`. Absent in JSON when None.
- **Zero regression**: All 1,025 unit + 174 integration tests must pass.
- **Session-scoped dedup is idempotent**: `drain_and_signal_session` returns None if session already cleared.
- **Edition 2024, MSRV 1.89**.
- **SIGNAL_QUEUE cap**: 10,000 records max. Drop oldest (lowest signal_id) when exceeded.
- **PendingEntriesAnalysis cap**: 1,000 entries. Drop lowest rework_flag_count when exceeded.
- **Rework threshold constant**: `REWORK_EDIT_CYCLE_THRESHOLD: usize = 3`
- **Stale threshold constant**: `STALE_SESSION_THRESHOLD_SECS: u64 = 4 * 3600`
- **ADR-001 LAYOUT FROZEN comment**: Must be present on `SignalRecord` struct definition in source.

## Dependencies

| Dependency | Version | What col-009 Uses |
|------------|---------|-------------------|
| `unimatrix-store` (internal) | current | SIGNAL_QUEUE table, drain/insert/len Store methods |
| `unimatrix-server` (internal) | current | SessionRegistry extensions, UDS listener dispatch, hook.rs |
| `unimatrix-observe` (internal) | current | EntryAnalysis, build_report extension |
| `unimatrix-engine` (internal) | current | wire.rs HookRequest/HookResponse (no changes needed) |
| `redb` | v3.1.x | SIGNAL_QUEUE TableDefinition and write transactions |
| `bincode` | v2 serde path | SignalRecord serialization (positional, layout-frozen) |
| `serde` | existing | Derive macros on SignalRecord, EntryAnalysis |
| col-008 | COMPLETE | SessionRegistry with InjectionRecord, SessionState |
| col-007 | COMPLETE | injection_history population via record_injection() |

## NOT in Scope

- `unhelpful_count` modification from implicit signals
- Persistent session storage (col-010: SESSIONS, INJECTION_LOG, session_id on EntryRecord)
- `injection_count` field population (col-010 provides INJECTION_LOG data)
- `context_retrospective` JSONL pipeline changes (`from_structured_events()` — col-010)
- Signal replay on server restart (soft durability tradeoff — documented)
- Anti-stuffing defenses beyond Wilson 5-vote minimum and session-scoped dedup
- Modification of the confidence formula (crt-002 unchanged)
- col-011 (Semantic Agent Routing)

## Alignment Status

**Overall**: PASS with one WARN

**WARN-01**: `success_session_count` population path not specified in SPECIFICATION.md FR-06.2.

- `EntryAnalysis.success_session_count` is defined but the increment trigger for Helpful-signal sessions is not in an FR.
- **Resolution before Session 2**: Add FR-06.2b to specification: "When `run_confidence_consumer` drains a Helpful SignalRecord, for each entry_id in the record, also increment `EntryAnalysis.success_session_count` in `PendingEntriesAnalysis`."
- **Impact on implementation**: Session 2 agents must populate `success_session_count` in `run_confidence_consumer` alongside `helpful_count` increment. This must be included in pseudocode for the `signal-dispatch` and `entries-analysis` components.

No VARIANCEs. No FAILs.
