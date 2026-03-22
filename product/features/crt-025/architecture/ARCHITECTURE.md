# crt-025: WA-1 Phase Signal + FEATURE_ENTRIES Tagging — Architecture

## System Overview

crt-025 adds phase awareness to the Unimatrix engine. The system currently knows which feature a session is working on (`SessionState.feature`) but not which workflow phase — scope, design, implementation, testing — is active. This feature supplies that missing signal in three layers:

1. **Wire layer** — `context_cycle` gains a `phase-end` event type and `phase`/`outcome`/`next_phase` parameters.
2. **State layer** — `SessionState` carries `current_phase: Option<String>` maintained by the UDS listener.
3. **Storage layer** — A new `CYCLE_EVENTS` append-only table records every lifecycle event; `FEATURE_ENTRIES` gains a `phase` column populated at store time.

The result is training data for W3-1 (GNN), an explicit phase narrative in `context_cycle_review`, and a foundation for WA-2's phase-conditioned category affinity boosting.

## Component Breakdown

### Component 1: Validation Layer (`unimatrix-server/infra/validation.rs`)

**Responsibility**: Accept, normalize, and validate `CycleParams` inputs from both the MCP tool path and the hook path.

**Changes**:
- Add `PhaseEnd` variant to `CycleType` enum.
- Remove `keywords` from `ValidatedCycleParams`; add `phase: Option<String>`, `outcome: Option<String>`, `next_phase: Option<String>`.
- Extend `validate_cycle_params` signature: accept `phase`, `outcome`, `next_phase` as `Option<&str>` parameters.
- Validate phase string: trim, lowercase-normalize, reject if contains space or exceeds 64 chars.
- Return `Err(String)` (unchanged contract) on invalid input — hook path requires plain string errors.
- Add constant `CYCLE_PHASE_END_EVENT = "cycle_phase_end"`.

### Component 2: MCP Tool Handler (`unimatrix-server/mcp/tools.rs`)

**Responsibility**: Parse `CycleParams` from MCP JSON, call validation, emit `HookRequest::RecordEvent`.

**Changes**:
- Replace `keywords: Option<Vec<String>>` with `phase: Option<String>`, `outcome: Option<String>`, `next_phase: Option<String>` on `CycleParams`.
- Pass new fields to `validate_cycle_params`.
- For `phase-end` events: emit `HookRequest::RecordEvent` with `event_type = CYCLE_PHASE_END_EVENT`, payload carrying `feature_cycle`, `phase`, `outcome`, `next_phase`.
- Remove keywords persistence call.

### Component 3: Hook Path (`unimatrix-server/uds/hook.rs`)

**Responsibility**: Intercept `context_cycle` pre-tool-use events, build `HookRequest::RecordEvent`.

**Changes**:
- Extract `phase`, `outcome`, `next_phase` from `tool_input` payload alongside existing `type` and `topic` extraction.
- Pass new fields through `validate_cycle_params`.
- Map `phase-end` to `CYCLE_PHASE_END_EVENT` constant.
- Remove keywords extraction and JSON serialization.

### Component 4: SessionState (`unimatrix-server/infra/session.rs`)

**Responsibility**: Maintain per-session in-memory state including the new `current_phase` field.

**Changes**:
- Add `current_phase: Option<String>` to `SessionState`.
- Initialize to `None` in `register_session`.
- Expose a new `set_current_phase(session_id: &str, phase: Option<String>)` method on `SessionRegistry`.

### Component 5: UDS Listener (`unimatrix-server/uds/listener.rs`)

**Responsibility**: Dispatch `HookRequest::RecordEvent` events; handle cycle lifecycle variants specially.

**Changes**:
- Add `CYCLE_PHASE_END_EVENT` to the imports from `validation`.
- Extend `handle_cycle_start` (or split into `handle_cycle_event`) to also handle `cycle_phase_end` and `cycle_stop` cases.
- **Synchronous in-memory mutation** (SR-01): `SessionState.current_phase` is set **before** the DB write task is spawned. The mutation is a direct `SessionRegistry.set_current_phase()` call inside the handler's synchronous code path, not queued.
- Fire-and-forget DB write: INSERT into `CYCLE_EVENTS` via `spawn_blocking_fire_and_forget`.

**Phase transition logic (in the handler's synchronous section)**:

| Event | current_phase mutation |
|-------|----------------------|
| `cycle_start` with `next_phase` | `set_current_phase(session_id, Some(next_phase))` |
| `cycle_start` without `next_phase` | no change to `current_phase` |
| `cycle_phase_end` with `next_phase` | `set_current_phase(session_id, Some(next_phase))` |
| `cycle_phase_end` without `next_phase` | no change to `current_phase` |
| `cycle_stop` | `set_current_phase(session_id, None)` |

**`seq` computation** (SR-02 resolution — see ADR-002): Computed inside the spawned DB task via `SELECT COALESCE(MAX(seq), -1) + 1 FROM cycle_events WHERE cycle_id = ?`. This is safe because UDS listener events for a given session are serialized per-session, and cross-session conflicts on the same `cycle_id` produce advisory (non-fatal) seq gaps at worst. See ADR-002.

### Component 6: Store Layer (`unimatrix-store`)

**Responsibility**: Persist CYCLE_EVENTS rows; persist feature_entries rows with phase.

**Changes**:

**`analytics.rs`**:
- `AnalyticsWrite::FeatureEntry` gains `phase: Option<String>` field.
- `FeatureEntry` drain handler: `INSERT OR IGNORE INTO feature_entries (feature_id, entry_id, phase) VALUES (?1, ?2, ?3)`.

**`write_ext.rs`**:
- `record_feature_entries` signature changes to: `record_feature_entries(feature_cycle: &str, entry_ids: &[u64], phase: Option<&str>) -> Result<()>`.
- INSERT statement updated to write `phase` column.

**`db.rs`** (new method):
- `insert_cycle_event(cycle_id, seq, event_type, phase, outcome, next_phase, timestamp) -> Result<()>` — direct write pool call (not analytics drain, since CYCLE_EVENTS is a structural table, not observational telemetry).

### Component 7: Schema Migration (`unimatrix-store/migration.rs` + `db.rs`)

**Responsibility**: Advance schema v14 → v15.

**Changes**:
- `CURRENT_SCHEMA_VERSION` bumped to 15.
- `run_main_migrations`: add `v14 → v15` block.
- `create_tables_if_needed` in `db.rs`: include `CYCLE_EVENTS` DDL and `phase` column in `feature_entries` DDL.

### Component 8: Context Store Phase Capture (`unimatrix-server/server.rs` + `services/usage.rs`)

**Responsibility**: Snapshot `SessionState.current_phase` at the moment `context_store` is called, propagate it to feature entry writes.

**Changes** (SR-07 resolution — see ADR-001):
- In `context_store` handler: read `session_state.current_phase` into a local `Option<String>` **at the point session state is read**, before any async dispatch.
- Pass the snapshotted phase to both write paths:
  - Direct path: `store.record_feature_entries(feature_cycle, &ids, phase.as_deref()).await`
  - Analytics drain path: `AnalyticsWrite::FeatureEntry { feature_id, entry_id, phase }` — the phase value is baked into the queued event, not read from live session state at drain time.
- `UsageContext` gains `current_phase: Option<String>` field.

### Component 9: `context_cycle_review` Phase Narrative (`unimatrix-server/mcp/tools.rs` + `unimatrix-observe`)

**Responsibility**: Enrich the retrospective report with explicit phase narrative and cross-cycle comparison.

**New queries (in `context_cycle_review` handler)**:

1. `SELECT seq, event_type, phase, outcome, next_phase, timestamp FROM cycle_events WHERE cycle_id = ? ORDER BY seq ASC` — raw event log for the feature.
2. `SELECT fe.phase, e.category, COUNT(*) as cnt FROM feature_entries fe JOIN entries e ON e.id = fe.entry_id WHERE fe.feature_id = ? AND fe.phase IS NOT NULL GROUP BY fe.phase, e.category` — current feature's phase/category distribution.
3. `SELECT fe.phase, e.category, COUNT(*) as cnt FROM feature_entries fe JOIN entries e ON e.id = fe.entry_id WHERE fe.feature_id IN (SELECT DISTINCT feature_id FROM feature_entries WHERE phase IS NOT NULL) AND fe.feature_id != ? AND fe.phase IS NOT NULL GROUP BY fe.phase, e.category` — cross-feature distribution for baseline.

**New type** (`unimatrix-observe/types.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleEventRecord {
    pub seq: i64,
    pub event_type: String,
    pub phase: Option<String>,
    pub outcome: Option<String>,
    pub next_phase: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseNarrative {
    pub phase_sequence: Vec<String>,      // ordered, may repeat (rework)
    pub rework_phases: Vec<String>,       // phases appearing more than once
    pub per_phase_categories: HashMap<String, HashMap<String, u64>>,
    pub cross_cycle_comparison: Option<Vec<PhaseCategoryComparison>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseCategoryComparison {
    pub phase: String,
    pub category: String,
    pub this_feature_count: u64,
    pub cross_cycle_mean: f64,
    pub sample_features: usize,
}
```

**`RetrospectiveReport`** gains:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub phase_narrative: Option<PhaseNarrative>,
```

**Phase narrative construction** (`unimatrix-observe/src/phase_narrative.rs`, new module):
- `build_phase_narrative(events: &[CycleEventRecord], current_dist: &PhaseCategoryDist, cross_dist: &PhaseCategoryDist) -> PhaseNarrative`
- Phase sequence: walk `cycle_events` ordered by `seq`, extract `phase` from `cycle_phase_end` and `cycle_start` events. A phase is "entered" when a `cycle_start` carries `next_phase` or when a `cycle_phase_end` specifies `phase`.
- Rework detection: phase name appearing more than once in the ordered sequence.
- Cross-cycle comparison: for each (phase, category) pair in the current feature, compute mean count across prior features.

**Backward compatibility**: If `CYCLE_EVENTS` has no rows for the feature, `phase_narrative` field is omitted (AC-12). No placeholder, no error.

### Component 10: CategoryAllowlist (`unimatrix-server/infra/categories.rs`)

**Responsibility**: Remove `outcome` from the set of valid store categories.

**Changes**:
- Remove `"outcome"` from `INITIAL_CATEGORIES` constant.
- Existing entries in the store with category `outcome` are not touched (no DELETE migration).
- Tests that assert `al.validate("outcome").is_ok()` must be updated to assert `is_err()`.

## Component Interactions

```
context_cycle (MCP)
      │ CycleParams {type, topic, phase, outcome, next_phase}
      ▼
validate_cycle_params() ─────────────────────────────────┐
      │ ValidatedCycleParams                              │
      ▼                                                   │ (same fn, hook path)
MCP tool handler                              hook.rs (pre-tool-use)
      │ HookRequest::RecordEvent                          │
      └──────────────────────┬────────────────────────────┘
                             ▼
                    UDS listener (dispatch_request)
                             │
               ┌─────────────┴──────────────┐
               │ SYNCHRONOUS (in-task)       │ FIRE-AND-FORGET (spawned)
               │ session_registry             │ insert_cycle_event(CYCLE_EVENTS)
               │   .set_current_phase(...)   │
               └─────────────────────────────┘

context_store (MCP)
      │
      ▼
server.rs / tools.rs
      │ read session_state.current_phase  ← snapshot at call time
      │ (local variable, not re-read at flush)
      ▼
record_feature_entries(feature_cycle, ids, phase)   ← direct write path
      OR
AnalyticsWrite::FeatureEntry { feature_id, entry_id, phase }  ← drain path
      (phase baked into event, not read from live state)

context_cycle_review (MCP)
      │
      ├── existing telemetry pipeline (untouched)
      ├── SELECT cycle_events WHERE cycle_id = ?
      ├── SELECT feature_entries JOIN entries GROUP BY phase, category (current)
      ├── SELECT feature_entries JOIN entries GROUP BY phase, category (cross-cycle)
      └── build_phase_narrative() → PhaseNarrative → RetrospectiveReport.phase_narrative
```

## Data Flow

### Phase Signal Path (write)
```
Protocol calls context_cycle(type="phase-end", topic="crt-025", phase="design", next_phase="implementation")
  → validate_cycle_params: normalize "implementation" (already lowercase, no spaces)
  → hook.rs: emits RecordEvent { event_type: "cycle_phase_end", payload: {feature_cycle, phase, next_phase} }
  → UDS listener dispatches RecordEvent
      SYNC:  session_registry.set_current_phase("s1", Some("implementation"))
      ASYNC: store.insert_cycle_event("crt-025", seq, "cycle_phase_end", Some("design"), None, Some("implementation"), ts)
```

### Phase Tag Path (write)
```
Agent calls context_store(category="decision", content="...")
  → context_store handler reads session_state.current_phase = Some("implementation") → snapshotted
  → record_feature_entries("crt-025", [entry_id], Some("implementation"))
      → INSERT INTO feature_entries(feature_id, entry_id, phase) VALUES("crt-025", N, "implementation")
```

### Phase Narrative Path (read)
```
context_cycle_review(topic="crt-025")
  → SELECT cycle_events → [start/phase-end/stop rows ordered by seq]
  → SELECT feature_entries join entries GROUP BY phase, category (current feature)
  → SELECT feature_entries join entries GROUP BY phase, category (all other features with phase data)
  → build_phase_narrative(events, current_dist, cross_dist) → PhaseNarrative
  → report.phase_narrative = Some(narrative)
```

## Technology Decisions

See ADR files for detailed rationale.

| Decision | Choice | ADR |
|----------|--------|-----|
| Phase snapshot timing for FeatureEntry | Snapshot at enqueue time, baked into event | ADR-001 |
| seq monotonicity enforcement | Advisory seq via MAX()+1; true ordering at query time is timestamp+seq | ADR-002 |
| CYCLE_EVENTS write path | Direct write pool (not analytics drain) | ADR-003 |
| Phase narrative data model | New `PhaseNarrative` type on `RetrospectiveReport` | ADR-004 |
| `outcome` category retirement | Remove from INITIAL_CATEGORIES; block ingest only | ADR-005 |

## Integration Points

| Integration | Direction | Notes |
|------------|-----------|-------|
| `context_cycle` MCP tool | Protocol → Server | New fields pass through; `keywords` silently dropped |
| `hook.rs` → `validate_cycle_params` | In-process | Shared function, string-error contract preserved |
| `SessionRegistry.set_current_phase` | Listener → SessionState | Synchronous; called before any DB spawn |
| `context_store` → `record_feature_entries` | MCP handler → Store | Phase snapshotted at call site, not at flush |
| `AnalyticsWrite::FeatureEntry` | Store queue | Phase field baked in at enqueue |
| `context_cycle_review` → `CYCLE_EVENTS` | MCP handler → Store | New SQL query |
| `context_cycle_review` → `FEATURE_ENTRIES` | MCP handler → Store | Extended GROUP BY query |
| WA-2 consumer | Server state → future feature | `SessionState.current_phase` is the interface |
| W3-1 consumer | DB → future feature | `feature_entries.phase` is the interface |

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `CycleParams.phase` | `Option<String>` | `mcp/tools.rs` |
| `CycleParams.outcome` | `Option<String>` | `mcp/tools.rs` |
| `CycleParams.next_phase` | `Option<String>` | `mcp/tools.rs` |
| `CycleType::PhaseEnd` | new enum variant | `infra/validation.rs` |
| `ValidatedCycleParams.phase` | `Option<String>` | `infra/validation.rs` |
| `ValidatedCycleParams.outcome` | `Option<String>` | `infra/validation.rs` |
| `ValidatedCycleParams.next_phase` | `Option<String>` | `infra/validation.rs` |
| `CYCLE_PHASE_END_EVENT` | `&str = "cycle_phase_end"` | `infra/validation.rs` |
| `SessionState.current_phase` | `Option<String>` | `infra/session.rs` |
| `SessionRegistry::set_current_phase` | `fn(&self, session_id: &str, phase: Option<String>)` | `infra/session.rs` |
| `record_feature_entries` | `async fn(feature_cycle: &str, entry_ids: &[u64], phase: Option<&str>) -> Result<()>` | `unimatrix-store/write_ext.rs` |
| `AnalyticsWrite::FeatureEntry` | `{ feature_id: String, entry_id: u64, phase: Option<String> }` | `unimatrix-store/analytics.rs` |
| `SqlxStore::insert_cycle_event` | `async fn(cycle_id: &str, seq: i64, event_type: &str, phase: Option<&str>, outcome: Option<&str>, next_phase: Option<&str>, timestamp: i64) -> Result<()>` | `unimatrix-store/db.rs` |
| `UsageContext.current_phase` | `Option<String>` | `services/usage.rs` |
| `RetrospectiveReport.phase_narrative` | `Option<PhaseNarrative>` | `unimatrix-observe/types.rs` |
| `PhaseNarrative` | see type definition above | `unimatrix-observe/types.rs` |
| `CycleEventRecord` | see type definition above | `unimatrix-observe/types.rs` |
| `PhaseCategoryComparison` | see type definition above | `unimatrix-observe/types.rs` |
| `CYCLE_EVENTS` DDL | `(id AUTOINCREMENT, cycle_id TEXT, seq INTEGER, event_type TEXT, phase TEXT NULL, outcome TEXT NULL, next_phase TEXT NULL, timestamp INTEGER)` | `unimatrix-store/db.rs` |
| `feature_entries.phase` | `TEXT NULL` (new column) | `unimatrix-store/db.rs` |
| Schema version | `CURRENT_SCHEMA_VERSION = 15` | `unimatrix-store/migration.rs` |

## Crate Touch Map

| Crate | Files Changed |
|-------|--------------|
| `unimatrix-store` | `analytics.rs`, `write_ext.rs`, `db.rs`, `migration.rs` |
| `unimatrix-server` | `infra/validation.rs`, `infra/session.rs`, `infra/categories.rs`, `mcp/tools.rs`, `uds/hook.rs`, `uds/listener.rs`, `services/usage.rs`, `server.rs`, `format.rs` |
| `unimatrix-observe` | `types.rs`, `lib.rs`, new `phase_narrative.rs` |

## Open Questions

None — all questions were resolved in SCOPE.md Decisions section and SCOPE-RISK-ASSESSMENT.md recommendations. See ADRs for the resolution of SR-01, SR-02, and SR-07.
