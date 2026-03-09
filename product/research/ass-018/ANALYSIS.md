# ASS-018: Observation Pipeline Structural Analysis

## Status: IN PROGRESS

## Problem Statement

The observation/retrospective pipeline has been non-functional since col-012. `context_retrospective` returns no data for any feature. Bug-162 added a content-based attribution fallback, but the underlying structural issues remain.

## Findings To Date

### 1. Session Identity Model

**Key finding: `session_id` is shared across parent + all subagents within a single Claude Code session.** Subagents inherit the parent's session UUID. This means observations from parent and subagent tool calls are already linked by `session_id` — no parent→child linking is needed at that level.

Source: `crates/unimatrix-engine/src/wire.rs` — `HookInput.session_id` is passed through to all events.

### 2. Current SQL Schema

**sessions table:**
```sql
CREATE TABLE sessions (
    session_id TEXT PRIMARY KEY,
    feature_cycle TEXT,          -- ALWAYS NULL (never populated)
    agent_role TEXT,
    started_at INTEGER,
    ended_at INTEGER,
    status INTEGER,              -- Active(0), Completed(1), TimedOut(2), Abandoned(3)
    compaction_count INTEGER,
    outcome TEXT,                -- "success" | "rework" | "abandoned"
    total_injections INTEGER
);
-- Indexes: idx_sessions_feature_cycle, idx_sessions_started_at
```

**observations table:**
```sql
CREATE TABLE observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    ts_millis INTEGER NOT NULL,
    hook TEXT NOT NULL,           -- PreToolUse, PostToolUse, SubagentStart, SubagentStop
    tool TEXT,                   -- tool name or agent type
    input TEXT,                  -- serialized tool_input JSON
    response_size INTEGER,
    response_snippet TEXT
);
-- Indexes: idx_observations_session, idx_observations_ts
```

### 3. Hook Events Captured

Seven events configured in `.claude/settings.json`:
- `SessionStart` → creates `sessions` row (feature_cycle always NULL)
- `Stop` / `TaskCompleted` → updates sessions row (ended_at, status, outcome)
- `PreToolUse` → observation row (tool_name, tool_input)
- `PostToolUse` → observation row (tool_name, tool_input, response_size, response_snippet)
- `SubagentStart` → observation row (agent_type, prompt_snippet)
- `SubagentStop` → observation row (empty payload)
- `UserPromptSubmit` → routed to ContextSearch, NOT stored as observation

### 4. Data Flow (Write Path)

```
Claude Code hook event
  → stdin JSON → parse_hook_input() → HookInput struct
  → build_request() → HookRequest enum
  → dispatch_request() routes by variant:
      SessionRegister → sessions INSERT (feature_cycle from input.extra, always None)
      RecordEvent → extract_observation_fields() → observations INSERT
      RecordEvents → batch observations INSERT
      SessionClose → sessions UPDATE
```

`extract_observation_fields()` at `listener.rs:1590` maps ImplantEvent → ObservationRow:
- PreToolUse: extracts tool_name, tool_input
- PostToolUse: extracts tool_name, tool_input, response_size, response_snippet
- SubagentStart: extracts agent_type as tool, prompt_snippet as input
- SubagentStop/other: no fields extracted (NULL tool, NULL input)

### 5. Data Flow (Read Path) — The Break Point

`load_feature_observations(feature_cycle)`:
1. Query `sessions WHERE feature_cycle = ?` → get session_ids
2. Query `observations WHERE session_id IN (...)` → get records
3. **Always returns empty** because feature_cycle is always NULL

Bug-162 fallback (`load_unattributed_sessions()`):
1. Query `sessions WHERE feature_cycle IS NULL` → get session_ids
2. Query observations for those session_ids
3. Run `attribute_sessions()` content-based attribution
4. **Works but limited** — only finds sessions that have a `sessions` row

### 6. Structural Gaps Identified

#### Gap 1: `feature_cycle` is never populated
Claude Code doesn't send `feature_cycle` in hook input. The field in `HookInput.extra` is never present. No mechanism exists to populate it.

#### Gap 2: No "feature delivery" concept
A feature delivery (e.g., col-015) spans 5+ independent Claude Code sessions (research, design, implementation stages, testing, PR review). Each session gets its own `session_id`. There is no grouping layer above `session` that ties them together.

#### Gap 3: Orphaned observations possible
If observations exist in `observations` table but their `session_id` has no matching row in `sessions`, they are invisible to all current queries. Both `load_feature_observations()` and `load_unattributed_sessions()` start from the `sessions` table.

#### Gap 4: SubagentStart/SubagentStop lack span structure
SubagentStart records `agent_type` and `prompt_snippet` but there's no span ID linking a SubagentStart to its corresponding SubagentStop. Within a shared `session_id`, multiple concurrent subagents can't be distinguished. The agent spawn tree is not reconstructable from current data.

#### Gap 5: UserPromptSubmit not stored as observation
`UserPromptSubmit` is routed to `ContextSearch` and never written to the `observations` table. User prompts — which contain the most direct feature-identifying content — are lost from the observation record.

#### Gap 6: Content-based attribution is best-effort
`attribution.rs` scans tool inputs for file paths (`product/features/{id}/...`) and text patterns. This works when agents touch feature files but fails for:
- Sessions doing general refactoring
- Sessions where feature context is only in prompts (not captured, see Gap 5)
- Early research sessions that don't touch feature directories

### 7. What Worked Before (JSONL Era)

Pre-col-012, JSONL files captured all hook data as flat records. Attribution happened at read time via `attribution.rs` scanning content. This worked because:
- All data in one place (no session→observation join)
- Attribution at query time, not write time
- No session registration dependency
- Content-based attribution had access to everything

### 8. Existing Attribution System

`crates/unimatrix-observe/src/attribution.rs` (15 tests, working):
- Scans `ObservationRecord.input` for three signal types (priority order):
  1. File paths: `product/features/{id}/...` → extract `{id}`
  2. Text content: words matching `alpha-digits` pattern
  3. Git checkout: `feature/{id}` branch references
- Groups records into `ParsedSession` structs
- Partitions sessions at "feature switch points" (handles multi-feature sessions)
- Attributes pre-feature records to the first feature detected

### 9. Related Tables (Context)

18 total SQLite tables. Observation pipeline uses:
- `sessions` — session lifecycle
- `observations` — hook events (normalized)
- `observation_metrics` — aggregated metrics per feature (universal)
- `observation_phase_metrics` — phase-specific metrics
- `injection_log` — historical injections per session
- `signal_queue` — implicit signals (helpful, flagged)

### 10. Key Code Locations

| File | Role |
|------|------|
| `crates/unimatrix-engine/src/wire.rs` | HookInput, ImplantEvent structs |
| `crates/unimatrix-server/src/uds/hook.rs` | build_request(), HookRequest routing |
| `crates/unimatrix-server/src/uds/listener.rs` | dispatch_request(), extract_observation_fields(), insert_observation() |
| `crates/unimatrix-server/src/services/observation.rs` | SqlObservationSource (read path) |
| `crates/unimatrix-observe/src/source.rs` | ObservationSource trait |
| `crates/unimatrix-observe/src/attribution.rs` | Content-based feature attribution (15 tests) |
| `crates/unimatrix-observe/src/metrics.rs` | Metric computation from observations |
| `crates/unimatrix-core/src/observation.rs` | ObservationRecord, ParsedSession, HookType |
| `crates/unimatrix-store/src/sqlite_store.rs` | Schema migrations, table creation |
| `.claude/settings.json` | Hook event configuration |

## Open Questions — ANSWERED

1. **Should feature attribution happen at write time or read time?**
   → **Both.** Write-time: on SessionClose, scan observations and backfill feature_cycle. Read-time: content-based fallback remains for sessions that weren't attributed at close.
   → See: SCHEMA-PROPOSAL.md §1B

2. **Is a `feature_deliveries` grouping table the right abstraction?**
   → **Yes.** Features span 5+ sessions. Need an anchor table with aggregate counters. Auto-created on first attribution, updated on session close.
   → See: SCHEMA-PROPOSAL.md §3A

3. **Can we reconstruct subagent spans from timestamp ordering?**
   → **Heuristic only.** Same session_id + timestamp ordering gives approximate pairing. True span linking requires platform support (span_id). Accept the limitation for now.
   → See: SCHEMA-PROPOSAL.md §2C

4. **Should `UserPromptSubmit` be dual-routed?**
   → **Yes.** User prompts are the richest signal for feature identification and intent. Store as observation AND dispatch to ContextSearch.
   → See: SCHEMA-PROPOSAL.md §2A

5. **What's the migration path?**
   → Additive schema changes (new tables, no column modifications). Backfill via Rust migration code that runs attribution on all existing unattributed sessions.
   → See: SCHEMA-PROPOSAL.md §3B

## Additional Findings

### 6. Query Text Not Captured
audit_log stores "returned N results" for search operations — NOT the query text. injection_log stores (entry, confidence) — NOT the query. New query_log table proposed.
→ See: DATA-INVENTORY.md §3, SCHEMA-PROPOSAL.md §2B

### 7. Test/Query Data Status
Test execution captured indirectly via Bash observations (cargo test commands). Not structured — pass/fail counts in response_snippet only. MCP tool calls captured as observations when invoked as Claude Code tools, but UDS searches (from hooks) are not stored. Query text never captured anywhere.
→ See: USE-CASES.md §"What We Have Today"

### 8. 16 Use Cases Identified
Connected data enables far more than retrospective restoration. Knowledge effectiveness, agent profiling, workflow optimization, embedding tuning data export, predictive signals.
→ See: USE-CASES.md (full catalog)

## Research Artifacts

| Document | Purpose |
|----------|---------|
| ANALYSIS.md | Structural analysis of current pipeline (this file) |
| DATA-INVENTORY.md | Complete inventory of what we have, what's missing, connection gaps, and prioritized implementation plan |
| SCHEMA-PROPOSAL.md | Concrete schema changes, migration plan, and data flow diagrams |
| USE-CASES.md | 16 use cases across 5 tiers, from basic retrospective to predictive intelligence |
