# crt-025 Pseudocode Overview
## WA-1: Phase Signal + FEATURE_ENTRIES Tagging

GH #330 | Schema v14 → v15

---

## Components Involved

| # | Component | File(s) | Why |
|---|-----------|---------|-----|
| 1 | validation-layer | `infra/validation.rs` | Adds `CycleType::PhaseEnd`, new params, phase format rules |
| 2 | mcp-tool-handler | `mcp/tools.rs` | Updates `CycleParams` struct, `context_cycle` handler, `context_cycle_review` phase queries |
| 3 | hook-path | `uds/hook.rs` | Extracts phase/outcome/next_phase from `tool_input`, maps `phase-end` event |
| 4 | session-state | `infra/session.rs` | Adds `current_phase` field to `SessionState` and `set_current_phase` to `SessionRegistry` |
| 5 | uds-listener | `uds/listener.rs` | Dispatches `cycle_phase_end` + `cycle_stop` with synchronous `current_phase` mutation |
| 6 | store-layer | `store/analytics.rs`, `store/write_ext.rs`, `store/db.rs` | `FeatureEntry.phase` field, `record_feature_entries` sig change, `insert_cycle_event` |
| 7 | schema-migration | `store/migration.rs`, `store/db.rs` | v14→v15 migration: `CYCLE_EVENTS` table, `feature_entries.phase` column |
| 8 | context-store-phase-capture | `server/server.rs`, `services/usage.rs` | Snapshots `current_phase` at call time, propagates to both write paths |
| 9 | phase-narrative | `observe/types.rs`, `observe/phase_narrative.rs`, `observe/lib.rs` | New types, `build_phase_narrative` pure function, `RetrospectiveReport` extension |
| 10 | category-allowlist | `infra/categories.rs` | Remove `"outcome"` from `INITIAL_CATEGORIES` |

---

## Data Flow

### Phase Signal Write Path

```
Protocol → context_cycle(type="phase-end", topic="crt-025", phase="design", next_phase="implementation")
  │
  ├── MCP path: CycleParams deserialized → validate_cycle_params() → ValidatedCycleParams
  │     → context_cycle handler → (no direct DB write; hook already fired)
  │
  └── Hook path: PreToolUse event → hook.rs intercepts → validate_cycle_params()
        → RecordEvent { event_type="cycle_phase_end", payload={feature_cycle, phase, next_phase, outcome} }
        → UDS listener dispatch_request()
              SYNC:  session_registry.set_current_phase(session_id, Some("implementation"))
              ASYNC: store.insert_cycle_event("crt-025", seq, "cycle_phase_end", "design", None, "implementation", ts)
```

### Phase Tag Write Path (context_store)

```
Agent → context_store(category="decision", topic="crt-025", content="...")
  → context_store handler: snapshot phase = session_registry.get_state(session_id)?.current_phase
  → record_feature_entries("crt-025", [entry_id], phase.as_deref())   [direct write path]
       OR
     AnalyticsWrite::FeatureEntry { feature_id, entry_id, phase }      [analytics drain path]
       phase baked in at enqueue — NOT re-read from SessionState at drain time
```

### Phase Narrative Read Path (context_cycle_review)

```
context_cycle_review(feature_cycle="crt-025")
  → existing telemetry pipeline (UNCHANGED)
  → store query: SELECT seq, event_type, phase, outcome, next_phase, timestamp
                   FROM cycle_events WHERE cycle_id = ? ORDER BY timestamp ASC, seq ASC
  → store query: SELECT fe.phase, e.category, COUNT(*) cnt
                   FROM feature_entries fe JOIN entries e ON e.id = fe.entry_id
                  WHERE fe.feature_id = ? AND fe.phase IS NOT NULL GROUP BY fe.phase, e.category
  → store query: SELECT fe.phase, e.category, COUNT(*) cnt  [cross-cycle baseline]
                   FROM feature_entries fe JOIN entries e ON e.id = fe.entry_id
                  WHERE fe.feature_id != ? AND fe.phase IS NOT NULL GROUP BY fe.phase, e.category
  → build_phase_narrative(events, current_dist, cross_dist)  [pure function, observe crate]
  → report.phase_narrative = Some(narrative)   [or None if no cycle_events rows]
```

---

## Shared Types (New and Modified)

### Modified in `infra/validation.rs`

```
enum CycleType { Start, PhaseEnd, Stop }   // PhaseEnd is new

struct ValidatedCycleParams {
    cycle_type:  CycleType,
    topic:       String,
    phase:       Option<String>,     // new; normalized lowercase, trimmed
    outcome:     Option<String>,     // new; max 512 chars
    next_phase:  Option<String>,     // new; normalized lowercase, trimmed
    // keywords field REMOVED
}

const CYCLE_PHASE_END_EVENT: &str = "cycle_phase_end";   // new
// CYCLE_START_EVENT and CYCLE_STOP_EVENT already exist
```

### Modified in `infra/session.rs`

```
struct SessionState {
    // ... existing fields unchanged ...
    current_phase: Option<String>,   // new; None until first phase signal
}
```

### Modified in `mcp/tools.rs`

```
struct CycleParams {
    r#type:     String,
    topic:      String,
    phase:      Option<String>,      // new
    outcome:    Option<String>,      // new
    next_phase: Option<String>,      // new
    agent_id:   Option<String>,
    format:     Option<String>,
    // keywords REMOVED
}
```

### Modified in `store/analytics.rs`

```
AnalyticsWrite::FeatureEntry {
    feature_id: String,
    entry_id:   u64,
    phase:      Option<String>,   // new; snapshot at enqueue time
}
```

### Modified in `services/usage.rs`

```
struct UsageContext {
    // ... existing fields unchanged ...
    current_phase: Option<String>,   // new
}
```

### New in `unimatrix-observe/types.rs`

```
struct CycleEventRecord {
    seq:        i64,
    event_type: String,
    phase:      Option<String>,
    outcome:    Option<String>,
    next_phase: Option<String>,
    timestamp:  i64,
}

struct PhaseNarrative {
    phase_sequence:         Vec<String>,
    rework_phases:          Vec<String>,
    per_phase_categories:   HashMap<String, HashMap<String, u64>>,
    cross_cycle_comparison: Option<Vec<PhaseCategoryComparison>>,
}

struct PhaseCategoryComparison {
    phase:              String,
    category:           String,
    this_feature_count: u64,
    cross_cycle_mean:   f64,
    sample_features:    usize,
}
```

### Modified in `unimatrix-observe/types.rs`

```
struct RetrospectiveReport {
    // ... all existing fields unchanged ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    phase_narrative: Option<PhaseNarrative>,   // new
}
```

### New DDL (schema v15)

```sql
CREATE TABLE IF NOT EXISTS cycle_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    cycle_id   TEXT    NOT NULL,
    seq        INTEGER NOT NULL,
    event_type TEXT    NOT NULL,
    phase      TEXT,
    outcome    TEXT,
    next_phase TEXT,
    timestamp  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cycle_events_cycle_id ON cycle_events (cycle_id);

-- Added to feature_entries:
ALTER TABLE feature_entries ADD COLUMN phase TEXT;
```

---

## Recommended Wave Grouping

Components within a wave are independent of each other and can be implemented in parallel. Dependencies flow downward across waves.

### Wave 1 — Foundation (no intra-feature dependencies)

| Component | Rationale |
|-----------|-----------|
| **10: category-allowlist** | Self-contained removal; no dependencies on other crt-025 components |
| **4: session-state** | Adds field + method to `SessionState`/`SessionRegistry`; nothing else in crt-025 depends on it being done first, but everything that uses `set_current_phase` must wait for it |
| **7: schema-migration** | DDL and migration are prerequisite for any store writes to new tables/columns |
| **9: phase-narrative** | Pure types + pure function in `unimatrix-observe`; no server or store dependency |

### Wave 2 — Core Store and Validation

| Component | Depends on |
|-----------|------------|
| **1: validation-layer** | None from Wave 1 (pure validation logic; can write in parallel with Wave 1) |
| **6: store-layer** | Wave 1 (schema-migration must exist for `insert_cycle_event` DDL to be defined; `FeatureEntry.phase` variant update requires compile-compatible schema) |

### Wave 3 — Server Wiring (depends on Wave 1+2)

| Component | Depends on |
|-----------|------------|
| **8: context-store-phase-capture** | Wave 1 (`session-state`), Wave 2 (`store-layer` for `record_feature_entries` new sig) |
| **3: hook-path** | Wave 2 (`validation-layer` for `CYCLE_PHASE_END_EVENT` and new `validate_cycle_params` sig) |
| **5: uds-listener** | Wave 1 (`session-state` for `set_current_phase`), Wave 2 (`validation-layer` for `CYCLE_PHASE_END_EVENT`), Wave 2 (`store-layer` for `insert_cycle_event`) |

### Wave 4 — MCP Tool Handler (depends on Wave 1+2+3)

| Component | Depends on |
|-----------|------------|
| **2: mcp-tool-handler** | Wave 2 (`validation-layer` new sig), Wave 2 (`store-layer` for `insert_cycle_event`), Wave 1 (`phase-narrative` types for `context_cycle_review` assembly) |

---

## Sequencing Constraints

1. `schema-migration` (component 7) must be complete before any store integration tests run against the new schema.
2. `session-state` (component 4) must be complete before `uds-listener` (5) or `context-store-phase-capture` (8) can compile against `set_current_phase`.
3. `validation-layer` (component 1) must be complete before `mcp-tool-handler` (2), `hook-path` (3), and `uds-listener` (5) can use the new signature and `CYCLE_PHASE_END_EVENT`.
4. `store-layer` (component 6) must be complete before `context-store-phase-capture` (8) — the `record_feature_entries` signature change is a compile-time breaking change at all call sites.
5. `phase-narrative` (component 9) types must be complete before `mcp-tool-handler` (2) can assemble `PhaseNarrative` and attach it to `RetrospectiveReport`.
