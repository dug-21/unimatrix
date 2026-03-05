# ASS-015: Data Path Unification — Can We Eliminate JSONL Observation Files?

## The Question

Unimatrix has two independent data ingestion paths from Claude Code hooks:
1. **JSONL files** — Shell scripts write per-session observation files
2. **UDS socket** — Hook CLI sends structured requests to the server process

Can path 1 (JSONL) be eliminated while retaining or improving retrospective analysis capability?

## Answer: YES

The hook stream already receives everything the JSONL path captures. The gap is not in the data arriving — it's in what the UDS handler **discards vs stores**. One table addition closes the gap entirely, and the unified path is strictly more powerful than either path alone.

---

## 1. The Current Dual-Path Architecture

```
Claude Code hook fires (PreToolUse, PostToolUse, SubagentStart, etc.)
       |
       v
  .claude/hooks/{event}.sh
       |
       +──→ Writes to ~/.unimatrix/observation/{session_id}.jsonl   [Path 1: JSONL]
       |    Fields: ts, hook, session_id, tool, input,
       |            response_size, response_snippet
       |
       +──→ Calls: unimatrix-server hook < stdin                    [Path 2: UDS]
            |
            v
         UDS Listener
            |
            +──→ SessionRegister → SESSIONS table
            +──→ RecordEvent → **DISCARDED** (except rework candidates)
            +──→ ContextSearch → INJECTION_LOG + CO_ACCESS
            +──→ SessionClose → SIGNAL_QUEUE + outcome resolution
            +──→ CompactPayload → compaction_count
```

**The problem is at `RecordEvent → DISCARDED`.** The hook CLI sends full tool event data through the UDS socket, but the handler only extracts rework-candidate events and throws away everything else.

---

## 2. Field-by-Field Gap Analysis

### What JSONL Captures That UDS Currently Discards

| Field | JSONL | UDS Handler | Gap |
|-------|-------|-------------|-----|
| Tool name per call | `tool: "Edit"` | Received in RecordEvent payload, **not stored** | Store it |
| Tool input (full JSON) | `input: {path, command, ...}` | Received in payload, **not stored** | Store it |
| Response size (bytes) | `response_size: 4532` | Received in payload, **not stored** | Store it |
| Response snippet (500 chars) | `response_snippet: "..."` | Received in payload, **not stored** | Store it |
| SubagentStart events | `hook: "SubagentStart"` | Received as RecordEvent, **not stored** | Store it |
| SubagentStop events | `hook: "SubagentStop"` | Received as RecordEvent, **not stored** | Store it |
| Millisecond timestamps | ISO-8601 → epoch millis | Received as seconds | Upgrade to millis |

### What UDS Captures That JSONL Doesn't

| Field | UDS Tables | JSONL | Advantage |
|-------|-----------|-------|-----------|
| Injection records (entry_id + confidence) | INJECTION_LOG | Not captured | Know which entries were served |
| Session outcome (success/rework/abandoned) | SESSIONS + SIGNAL_QUEUE | Not captured | Know if session succeeded |
| Helpful/Flagged signals | SIGNAL_QUEUE | Not captured | Quality feedback on entries |
| Co-access pairs | CO_ACCESS | Not captured | Entry relationship data |
| Explicit user actions (votes, corrections) | SessionRegistry | Not captured | Ground truth labels |
| Compaction count | SessionRegistry → SESSIONS | Not captured | Context pressure signal |
| Feature cycle (explicit) | SESSIONS | Inferred from content scanning | More reliable attribution |

### What Neither Path Correlates Today

**This is the biggest win of unification:** Neither path currently connects tool execution with knowledge injection. Example: "Agent called context_search, got entry #42, then spent 30 minutes editing 5 files and the feature succeeded" — this correlation doesn't exist in either path alone. A unified stream makes it trivial.

---

## 3. The Fix: One Table Addition

Add an `observations` table to SQLite that stores what RecordEvent currently discards:

```sql
CREATE TABLE IF NOT EXISTS observations (
    session_hash INTEGER NOT NULL,
    ts_millis    INTEGER NOT NULL,
    hook         TEXT NOT NULL,        -- PreToolUse|PostToolUse|SubagentStart|SubagentStop
    session_id   TEXT NOT NULL,
    tool         TEXT,                 -- Tool name or agent type
    input        TEXT,                 -- Full tool input (JSON)
    response_size INTEGER,             -- PostToolUse only
    response_snippet TEXT,             -- First 500 chars, PostToolUse only
    PRIMARY KEY (session_hash, ts_millis)
);
CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(session_id);
CREATE INDEX IF NOT EXISTS idx_observations_ts ON observations(ts_millis);
```

**Implementation:** In `handle_record_event` / `handle_record_events`, instead of discarding generic events, persist them to the observations table via `spawn_blocking` (fire-and-forget, same pattern as injection log writes).

**Cost:** One additional `spawn_blocking` per hook event. The RecordEvent handler already dispatches to spawn_blocking for rework events — extending this to all events is ~20 lines of code. SQLite's WAL mode handles concurrent reads during writes naturally.

---

## 4. What Each Retrospective Detection Rule Needs

All 21 detection rules were analyzed against data availability:

| Rule Category | Rules | Fields Used | Available in Hook Stream? |
|---------------|-------|-------------|--------------------------|
| Friction (4) | permission_retries, sleep_workarounds, search_via_bash, output_parsing_struggle | hook_type, tool, input (command text) | YES — all in RecordEvent payload |
| Session (5) | session_timeout, cold_restart, coordinator_respawns, post_completion_work, rework_events | hook_type, tool, input, session_id, timestamps | YES — all in RecordEvent payload |
| Agent (7) | context_load, lifespan, file_breadth, reread_rate, mutation_spread, compile_cycles, edit_bloat | hook_type, tool, input (file_path), response_size | YES — all in RecordEvent payload |
| Scope (5) | source_file_count, design_artifact_count, adr_count, post_delivery_issues, phase_duration_outlier | hook_type, tool, input (file_path, command) | YES — all in RecordEvent payload |

**Result: 0 rules require data that is JSONL-only.** Every field comes through the hook stream.

### One Semi-Gap: Phase Attribution

Phase attribution currently parses TaskCreate/TaskUpdate `subject` fields containing patterns like `"crt-006: description"`. This parsing works on the `input` field of the observation record — which IS in the RecordEvent payload. The only nuance: it could be pre-computed and stored as a column, or parsed at query time (as done today). Either way, data is present.

---

## 5. What Changes in the Retrospective Pipeline

### Current Flow (JSONL-based)

```
JSONL files on disk
    → discover_sessions() scans directory
    → parse_session_file() reads line-by-line
    → attribute_sessions() infers feature from content
    → detection rules run on ObservationRecord vec
    → compute_metric_vector()
    → compare_to_baseline()
    → synthesize report
```

### New Flow (SQLite-based)

```
observations table in SQLite
    → query by session_id or feature_cycle (indexed)
    → ObservationRow vec (same schema as ObservationRecord)
    → detection rules run unchanged (same input type)
    → compute_metric_vector() unchanged
    → compare_to_baseline() unchanged
    → synthesize report
```

**The detection rules don't change.** They consume `Vec<ObservationRecord>` today. With the new table, they consume `Vec<ObservationRow>` which has identical fields. This is a data source swap, not a logic change.

### Advantages of SQLite Over JSONL

| Dimension | JSONL | SQLite observations |
|-----------|-------|---------------------|
| Query by session | Scan directory + read file | Direct index lookup |
| Query by feature | Scan all files, parse, attribute | Index on feature_cycle |
| Query by time range | Scan all files, filter | Range scan on ts_millis |
| Data integrity | Partial writes possible | ACID transactions |
| Retention management | Manual cleanup scripts | DELETE WHERE ts_millis < ? |
| Correlation with injections | Separate path, no join key | Same database, JOIN on session_id + ts |
| Concurrent access | File locking fragile | WAL mode: concurrent readers + single writer |

---

## 6. The Unified Data Model

After unification, one session produces data in these tables:

```
Session Lifecycle:
  SESSIONS          — session_id, feature, role, outcome, timestamps

Tool Execution:
  OBSERVATIONS      — every PreToolUse/PostToolUse/SubagentStart/SubagentStop  [NEW]

Knowledge Interaction:
  INJECTION_LOG     — which entries were served, with confidence scores
  CO_ACCESS         — entry pairs accessed together

Quality Signals:
  SIGNAL_QUEUE      — helpful/flagged signals at session end

Rework Detection:
  (in SessionRegistry, persisted via OBSERVATIONS)  — edit-fail-edit cycles
```

**For passive knowledge acquisition, this unified model is transformative:**

- **Gap detection:** OBSERVATIONS shows context_search with zero results → gap signal
- **Outcome correlation:** OBSERVATIONS (tool calls) + INJECTION_LOG (entries served) + SESSIONS (outcome) → "entry X was injected in sessions that succeeded/failed"
- **Convention detection:** OBSERVATIONS across features → "agents always read A before editing B"
- **Friction-to-knowledge:** OBSERVATIONS (friction patterns) + feature attribution → "this friction appears in every feature → lesson"

---

## 7. Migration Path

### Phase 1: Add observations Table (~50 lines)
- Define SQL table schema (matches ObservationRecord)
- Extend RecordEvent handler to persist all events (not just rework candidates)
- Fire-and-forget via spawn_blocking (existing pattern)
- SQLite migration (add table via existing migration infrastructure)

### Phase 2: Dual-Write Period (~0 code, just time)
- JSONL files continue writing (existing hooks unchanged)
- observations table also writes (new handler)
- Validate data parity by running retrospective from both sources

### Phase 3: Migrate Retrospective Pipeline (~100 lines)
- New data source module: read from observations table instead of JSONL files
- Same `Vec<ObservationRecord>` output type → detection rules unchanged
- Session discovery: query sessions table instead of scanning directory
- Feature attribution: use sessions.feature_cycle (explicit) instead of content scanning

### Phase 4: Remove JSONL Path (~50 lines removed)
- Remove observation file writes from shell hooks
- Remove JSONL discovery/parsing code from unimatrix-observe
- Remove observation directory management
- Shell hooks become pure UDS forwarders

### Total Scope: ~200 lines changed, net reduction in code

---

## 8. Bonus: What Unification Enables for ASS-015

The unified data path is not just a cleanup — it's a prerequisite for the passive knowledge acquisition architecture:

| Capability | JSONL-only | Unified |
|------------|-----------|---------|
| Tool execution → knowledge gap correlation | Manual cross-reference | Single query: search miss → no store → gap |
| Injection → outcome correlation | Impossible | Join OBSERVATIONS × INJECTION_LOG × SESSIONS |
| Per-entry effectiveness | Impossible | Which entries appeared in successful vs failed sessions |
| Convention detection | Possible (from tool patterns) | Enhanced: tool patterns + knowledge patterns in one stream |
| Neural model training labels | Require file parsing + table queries | Single stream with all signals aligned |
| Real-time signal capture | Batch (file read) | Streaming (table write triggers) |

**The passive knowledge acquisition pipeline from the self-learning neural design assumes a unified event stream.** This analysis confirms that assumption is achievable with minimal work.

---

## 9. Recommendation

1. **Eliminate JSONL observation files** — they are 100% redundant with the hook stream data
2. **Add OBSERVATIONS table** — persist what RecordEvent currently discards (~50 lines)
3. **Migrate retrospective pipeline** — swap data source from files to table (~100 lines)
4. **Gain unified correlation** — tool execution + knowledge injection + outcomes in one queryable store

This is a net simplification of the architecture. Less code, better data, faster queries, and the foundation for passive knowledge acquisition.

---

## Appendix: Data That Would Be LOST If We Just Deleted JSONL Today

Without adding the OBSERVATIONS table first, eliminating JSONL would lose:

| Data | Used By | Impact |
|------|---------|--------|
| Tool name per call | All 21 detection rules | **All retrospective analysis breaks** |
| Tool input (commands, file paths) | 18 of 21 detection rules | **Most detection rules break** |
| Response size | 3 detection rules (context_load, edit_bloat, search_miss_rate) | **Key metrics break** |
| Response snippet | 1 detection rule (search_miss_rate via empty result detection) | **Minor degradation** |
| SubagentStart/Stop | 2 detection rules (lifespan, coordinator_respawns) | **Agent metrics break** |

**The OBSERVATIONS table is not optional — it must be added BEFORE JSONL can be removed.**
