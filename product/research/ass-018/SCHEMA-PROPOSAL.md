# ASS-018: Schema & Pipeline Changes — Connecting the Data

## Overview

Three categories of changes, ordered by dependency:
1. **Fix what's broken** — make retrospective work again
2. **Capture what's missing** — store data we're losing today
3. **Connect what's disconnected** — link sessions to topics to knowledge

### Naming Decision: `topic` as Universal Grouping

Knowledge entries use `topic` as their grouping attribute. Activity tracking uses `feature_cycle`. These are the same concept with different names. `feature_cycle` is SDLC-specific; `topic` is domain-agnostic.

**Going forward**: New code and new tables use `topic`. Existing `feature_cycle` columns remain (backward compat) but are understood as synonymous with `topic`. API/tool parameters accept both during transition. See DATA-INVENTORY.md §3 for full rationale.

---

## Category 1: Fix What's Broken

### 1A. Hook-Side Topic Signal Extraction

**Problem**: The hook binary runs per-event as a separate process, parses the full stdin JSON, but discards topic-identifying information. Attribution only happens in the retrospective pipeline (too late).

**Change**: The hook extracts topic signals from event payloads before sending to the UDS server. This pushes work to the edge — each hook invocation is cheap and fast.

**Per-event extraction in `build_request()`**:
```rust
// For file-path-bearing tools (Read, Edit, Write, Glob, Grep):
let topic_signal = extract_topic_from_tool_input(&input.extra);

// For UserPromptSubmit:
let topic_signal = input.prompt.as_deref().and_then(extract_feature_id_pattern);

// Attach to event:
HookRequest::RecordEvent {
    event: ImplantEvent { ..., },
    topic_signal,  // NEW: Option<String>
}
```

The extraction functions already exist in `attribution.rs` (`extract_from_path`, `extract_feature_id_pattern`). They're lightweight (string scanning, no I/O).

**Where**: `crates/unimatrix-server/src/uds/hook.rs` — `build_request()` and `generic_record_event()`.

### 1B. Server-Side Topic Accumulation + Attribution on SessionClose

**Problem**: Attribution only runs when `context_retrospective` is called. Sessions sit unattributed until someone explicitly requests a retrospective.

**Change**: Two-layer approach:

**Layer 1 — Accumulate topic signals per session** (in SessionRegistry or new accumulator):
- Each RecordEvent arrives with optional `topic_signal`
- Server tallies signals per session_id: `HashMap<String, Vec<String>>`
- Lightweight, no I/O

**Layer 2 — Resolve on SessionClose**:
- On SessionClose, resolve dominant topic from accumulated signals (majority vote)
- If clear winner: UPDATE sessions SET feature_cycle = ?
- If ambiguous: Fall back to full content-based attribution (load observations, run `attribute_sessions()`)
- Create/update topic_deliveries row

**Where**: `dispatch_request()` → SessionClose handler, after existing status/outcome update.

**Impact**: Sessions get attributed within seconds of completion. No retrospective needed.

### 1C. Persist Content-Based Attribution Results (Fallback)

**Problem**: `load_feature_observations(feature_cycle)` queries sessions WHERE feature_cycle = ?. feature_cycle is always NULL. Content-based attribution via `attribute_sessions()` works but never persists its results.

**Change**: After successful content-based attribution (in retrospective or maintenance), UPDATE the sessions table:
```sql
UPDATE sessions SET feature_cycle = ?1 WHERE session_id = ?2
```

**Where**: In `context_retrospective` handler and as a standalone maintenance operation.

**Impact**: Next retrospective call for the same topic hits the fast path.

---

## Category 2: Capture What's Missing

### 2A. Dual-Route UserPromptSubmit

**Problem**: UserPromptSubmit is converted to ContextSearch in `build_request()`. The prompt text is never stored as an observation. This is the richest signal for feature identification, intent classification, and task tracking.

**Current flow**:
```
UserPromptSubmit → build_request() → HookRequest::ContextSearch
```

**Proposed flow**:
```
UserPromptSubmit → build_request() → HookRequest::PromptAndSearch {
    event: ImplantEvent,        // stored as observation
    query: String,              // sent to ContextSearch
    session_id: Option<String>,
}
```

**Alternative** (simpler, no new variant): In `build_request()`, when UserPromptSubmit has a prompt, return `RecordEvent` with event_type = "UserPromptSubmit" and the prompt as input. Then in `dispatch_request()`, intercept RecordEvent where event_type == "UserPromptSubmit" to also trigger ContextSearch.

**Simplest approach**: Add a new HookRequest variant `PromptAndSearch` that dispatch_request handles as: store observation + run search + return search results.

**Observation row for UserPromptSubmit**:
```
hook: "UserPromptSubmit"
tool: NULL (or "UserPrompt")
input: JSON { "prompt": "implement col-016..." }
response_size: NULL
response_snippet: NULL
```

### 2B. Query Log Table

**Problem**: We record which entries were served (injection_log) but not what query triggered the retrieval. Can't evaluate search quality, identify gaps, or export training data.

**Schema**:
```sql
CREATE TABLE query_log (
    query_id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    query_text TEXT NOT NULL,
    ts INTEGER NOT NULL,             -- unix epoch seconds
    result_count INTEGER NOT NULL,
    result_entry_ids TEXT,           -- JSON array of u64
    similarity_scores TEXT,          -- JSON array of f64 (parallel to entry_ids)
    retrieval_mode TEXT,             -- 'strict' | 'relaxed'
    source TEXT NOT NULL             -- 'uds' | 'mcp' | 'hook'
);
CREATE INDEX idx_query_log_session ON query_log(session_id);
CREATE INDEX idx_query_log_ts ON query_log(ts);
```

**Write path**: In `handle_context_search()` and `SearchService::search()`, after computing results, fire-and-forget insert to query_log.

**Relationship to injection_log**: query_log captures the request side (what was asked), injection_log captures the response side (what was served with reranked scores). They can be joined by (session_id, timestamp proximity) or query_log.result_entry_ids ∩ injection_log.entry_id.

### 2C. Enrich SubagentStop

**Problem**: SubagentStop observations have NULL tool and NULL input. Can't pair start/stop or measure subagent duration.

**Minimal fix**: Extract available fields from SubagentStop payload:
```rust
"SubagentStop" => {
    let tool = payload.get("agent_type").and_then(|v| v.as_str()).map(|s| s.to_string());
    // If Claude Code provides agent_type on stop, we get pairing for free
}
```

**If Claude Code doesn't send agent_type on SubagentStop**: Use timestamp heuristic at read time to pair start/stop events within a session.

---

## Category 3: Connect What's Disconnected

### 3A. Topic Deliveries Table

**Purpose**: Group multiple sessions into a single topic delivery. The anchor for cross-session analysis. Named `topic_deliveries` to align with knowledge-side `topic` field.

```sql
CREATE TABLE topic_deliveries (
    topic TEXT PRIMARY KEY,              -- "col-016", "air-quality-sensors", etc.
    created_at INTEGER NOT NULL,         -- first session attributed
    completed_at INTEGER,                -- set on final session or explicit close
    status TEXT NOT NULL DEFAULT 'active', -- 'active' | 'completed' | 'abandoned'
    github_issue INTEGER,                -- optional GH issue number
    total_sessions INTEGER DEFAULT 0,
    total_tool_calls INTEGER DEFAULT 0,
    total_duration_secs INTEGER DEFAULT 0,
    phases_completed TEXT                 -- JSON array of phase names
);
```

**Lifecycle**:
1. Created automatically when first session is attributed to a topic (on SessionClose)
2. Aggregate counters updated on each session attribution or closure
3. Completed when `record-outcome` is called for the topic, or explicitly
4. Can be queried by `context_status` for topic-level overview

**Relationship to existing tables**:
```
topic_deliveries (1) ←── (N) sessions (via feature_cycle = topic)
                 (1) ←── (N) observation_metrics (via feature_cycle, already exists)
                 (1) ←── (N) feature_entries (via feature_id, already exists)
                 (1) ←── (N) outcome_index (via feature_cycle, already exists)
                 (1) ←── (N) entries (via topic field — knowledge entries)
```

Note: The last relationship is new — knowledge entries and activity sessions for the same topic are now queryable from the same anchor.

### 3B. Topic Signal on Observations

Add `topic_signal` column to observations table for hook-extracted signals:

```sql
ALTER TABLE observations ADD COLUMN topic_signal TEXT;
```

This stores the per-event topic signal extracted by the hook. Not every observation will have one — only those where a topic was detectable from the tool input (file paths, prompt text, git branches).

### 3C. Session-to-Topic Backfill Migration

For existing data: run attribution across all unattributed sessions and persist results.

```sql
-- Find sessions needing attribution
SELECT session_id FROM sessions WHERE feature_cycle IS NULL AND status != 3;

-- For each: load observations, run extract_feature_signal, update if found
UPDATE sessions SET feature_cycle = ?1 WHERE session_id = ?2;

-- Create topic_deliveries rows for all distinct topics
INSERT OR IGNORE INTO topic_deliveries (topic, created_at, status)
SELECT DISTINCT feature_cycle, MIN(started_at), 'completed'
FROM sessions
WHERE feature_cycle IS NOT NULL
GROUP BY feature_cycle;
```

### 3D. Explicit Topic Registration

**Already wired**: SessionRegister has `feature: Option<String>` — never populated.
```rust
SessionRegister {
    session_id: String,
    cwd: String,
    agent_role: Option<String>,
    feature: Option<String>,       // ← already in the schema!
}
```

**Problem**: Claude Code doesn't send `feature` in hook input. The `build_request()` function extracts it from `input.extra` but it's never there.

**Solution (no platform change needed)**: Coordinator agents can set feature context. When a coordinator spawns, it knows the feature (from its task prompt). The coordinator's agent definition can include instructions to call a registration tool:

```
# At session start, register the feature you're working on
context_register_session(feature_cycle: "col-016")
```

This could be a new MCP tool or a parameter on an existing tool. Implementation: UPDATE sessions SET feature_cycle = ? WHERE session_id = current_session_id.

---

## Schema Migration Plan

### Migration 10 (v9 → v10)

```sql
-- New tables
CREATE TABLE IF NOT EXISTS query_log (
    query_id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    query_text TEXT NOT NULL,
    ts INTEGER NOT NULL,
    result_count INTEGER NOT NULL,
    result_entry_ids TEXT,
    similarity_scores TEXT,
    retrieval_mode TEXT,
    source TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_query_log_session ON query_log(session_id);
CREATE INDEX IF NOT EXISTS idx_query_log_ts ON query_log(ts);

CREATE TABLE IF NOT EXISTS topic_deliveries (
    topic TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    status TEXT NOT NULL DEFAULT 'active',
    github_issue INTEGER,
    total_sessions INTEGER DEFAULT 0,
    total_tool_calls INTEGER DEFAULT 0,
    total_duration_secs INTEGER DEFAULT 0,
    phases_completed TEXT
);

-- Add topic_signal column to observations
ALTER TABLE observations ADD COLUMN topic_signal TEXT;

-- Backfill: handled by Rust migration code, not raw SQL
-- (attribution requires loading observations and running content analysis)
```

### Existing Schema Compatibility
- sessions.feature_cycle: Already exists as nullable TEXT. No schema change — just start populating it. Semantically = topic.
- observation_metrics: Already keyed by feature_cycle (= topic). Works once sessions are attributed.
- feature_entries: Already exists. Can be leveraged alongside topic_deliveries.
- injection_log: No changes needed. Linked by session_id.
- entries.topic: Already exists. Now aligned with topic_deliveries.topic — same namespace.

---

## Data Flow After Changes

```
                    ┌─────────────────────┐
                    │   Claude Code Hook   │
                    │     (7 events)       │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │   build_request()    │
                    │  + topic extraction  │ ◄── NEW: extract_from_path / extract_feature_id_pattern
                    └──────────┬──────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
     ┌────────▼───────┐  ┌────▼────────┐  ┌────▼───────────┐
     │ SessionRegister │  │ RecordEvent │  │ PromptAndSearch │
     │ (+ topic from   │  │ (+ topic_  │  │ (new)          │
     │  first prompt)  │  │  signal)   │  │                │
     └────────┬───────┘  └────┬────────┘  └───┬────────┬───┘
              │               │               │        │
              ▼               ▼               ▼        ▼
         sessions        observations     observations  ContextSearch
         (topic via      (+ topic_signal  (prompt as     │
          accumulation)   column)          observation)   │
              │                                          │
              │                                  ┌───────▼───────┐
              │                                  │  query_log    │
              │                                  │  (query text) │
              │                                  └───────┬───────┘
              │                                          │
              │                                  ┌───────▼───────┐
              │                                  │ injection_log │
              │                                  │ (results)     │
              │                                  └───────────────┘
              │
On SessionClose:
  ├─► Resolve topic from accumulated signals (majority vote)
  ├─► UPDATE sessions SET feature_cycle = topic
  ├─► INSERT OR UPDATE topic_deliveries (aggregate counters)
  └─► Fallback: full content-based attribution if no signals
```

---

## Backward Compatibility

- All new tables are additive (no existing table modifications except ALTER ADD COLUMN)
- observations.topic_signal is nullable — existing rows unaffected
- sessions.feature_cycle backfill is UPDATE on existing NULL column
- Existing load_feature_observations() starts working once feature_cycle is populated
- load_unattributed_sessions() fallback remains for sessions that can't be attributed
- observation_metrics continues to work as-is (keyed by feature_cycle = topic)
- 60-day GC on observations unaffected (query_log should have its own GC policy)
- `topic` and `feature_cycle` coexist during transition — new code uses `topic`, old code uses `feature_cycle`, both refer to same value
