# ASS-018: Data Inventory — What We Have, What's Missing, What's Possible

## 1. Current Data Assets

### 1a. observations table — Raw Hook Events
Every tool call in every Claude Code session is captured:
- **PreToolUse**: tool_name + full tool_input JSON (file paths, commands, queries)
- **PostToolUse**: tool_name + tool_input + response_size (bytes) + response_snippet (first 500 chars)
- **SubagentStart**: agent_type + prompt_snippet
- **SubagentStop**: empty (no span linking)
- Indexed by: session_id, ts_millis

**What this gives us**: Complete tool-level activity trace per session. Every Read, Write, Edit, Bash, Grep, Glob, Agent spawn, context_store, context_search (as MCP tool calls) — all captured with full input payloads.

**What's missing**: UserPromptSubmit (routed to ContextSearch, never stored as observation).

### 1b. sessions table — Session Lifecycle
- session_id (shared across parent + all subagents in a Claude Code session)
- agent_role, started_at, ended_at, status, outcome
- compaction_count, total_injections
- **feature_cycle: ALWAYS NULL** — never populated

### 1c. injection_log table — Knowledge Retrieval Record
Every entry served to an agent via ContextSearch:
- session_id, entry_id, confidence (reranked score), timestamp
- Indexed by session_id and entry_id

**What this gives us**: Which knowledge was served to which session, at what confidence. Can compute: hit rates, entry utility, confidence calibration.

**What's missing**: The **query text** that triggered the retrieval. Only the results are recorded, not what was asked.

### 1d. audit_log table — MCP Operation Audit
Every MCP tool invocation (search, store, correct, deprecate, etc.):
- session_id, agent_id, operation, target_ids, outcome, detail
- detail for search: "returned N results" (query text NOT captured)

### 1e. signal_queue table — Implicit Feedback Signals
- Helpful/Flagged signals tied to entry_ids
- Source: ImplicitOutcome or ImplicitRework
- Used by confidence evolution pipeline

### 1f. observation_metrics + observation_phase_metrics — Computed Aggregates
22 universal metrics + per-phase (duration, tool_calls) stored per feature_cycle.
Persisted after retrospective computation.

### 1g. shadow_evaluations table — Detection Rule Evaluation
Rule name, category, neural_category, confidence, convention_score, accepted flag.
Used for detection rule calibration.

### 1h. Knowledge Tables (entries, entry_tags, co_access, feature_entries, outcome_index)
Full knowledge graph with: confidence scores, access patterns, co-retrieval pairs, feature associations, correction chains.

---

## 2. The Connection Gap

### Current State: Islands of Data

```
sessions ──(feature_cycle=NULL)──✗── observations
    │                                      │
    │                                      │ (session_id)
    │                                      │
    ✗── feature_cycle                      ✓ linked by session_id
    │                                      │
    │                                 injection_log
    │                                      │
    ✗── no grouping above session     audit_log
```

### What's Broken
1. **sessions.feature_cycle is always NULL** → load_feature_observations() always returns empty
2. **No feature delivery layer** → 5+ sessions per feature, no way to group them
3. **Content-based attribution is the only bridge** → works when agents touch feature files, fails otherwise
4. **UserPromptSubmit not stored** → richest feature-identifying content (user's actual instructions) is lost
5. **No query text in audit_log or injection_log** → can't analyze what agents were looking for
6. **SubagentStop has no span ID** → can't pair start/stop or reconstruct agent trees

### What Used to Work (JSONL Era)
- All data in flat files, no joins needed
- Attribution happened at query time by scanning all content
- No session registration dependency
- Content-based attribution had access to everything in one pass

---

## 3. Naming Alignment: `topic` as the Universal Grouping Concept

### The Problem
Knowledge uses `topic` as its grouping attribute. Activity tracking uses `feature_cycle`. These are the same concept — a body of work that knowledge and activity both belong to — but with different names.

| System | Field | Example values |
|--------|-------|---------------|
| Knowledge (EntryRecord) | `topic` | "confidence-scoring", "mcp-server" |
| Knowledge (EntryRecord) | `feature_cycle` | "col-002", "crt-005" |
| Activity (sessions) | `feature_cycle` | NULL (always) |
| Activity (observations) | (none) | — |
| Activity (observation_metrics) | `feature_cycle` | "col-002" |
| Hooks (HookRequest) | `feature` | None (never populated) |

`topic` is the knowledge-side grouping. `feature_cycle` is the activity-side grouping. But `feature_cycle` is SDLC-specific (`col-002`, `crt-005`). If Unimatrix serves non-SDLC domains, "feature_cycle" makes no sense.

### Proposal: Align on `topic`

`topic` is domain-agnostic. A topic could be:
- SDLC: "col-016" (a feature)
- Research: "air-quality-sensors" (a research area)
- Operations: "prod-deploy-2026-03" (a deployment)
- Learning: "rust-async-patterns" (a skill area)

`feature_cycle` becomes an alias or legacy name for `topic` in the activity domain. The concept is: **a topic groups both knowledge entries and activity sessions that relate to the same body of work.**

### Migration Path
- New code uses `topic` as the canonical name
- `feature_cycle` remains in existing schemas (backward compat) but is understood as = topic
- New tables (feature_deliveries → topic_deliveries, query_log, etc.) use `topic`
- API/tool parameters accept both `topic` and `feature_cycle` during transition
- EntryRecord already has both `topic` and `feature_cycle` — `topic` is the semantic grouping, `feature_cycle` tracks which delivery created the entry

### What This Enables
- `context_retrospective(topic: "col-016")` — analyze all activity for a topic
- `context_search(topic: "air-quality")` — knowledge + activity in one namespace
- `context_status(topic: "col-016")` — unified view: knowledge entries + session activity + metrics
- Cross-domain: same Unimatrix instance serves SDLC features AND research topics AND operational runbooks

---

## 4. What We Need to Connect

### Priority 1: Session → Topic Linking
The fundamental break. The hook process should do as much attribution as possible — every millisecond of server-side processing saved is a win, and the hook has access to the full Claude Code stdin payload.

**A. Hook-side attribution (preferred — push work to the edge)**

The hook binary (`unimatrix-server hook <event>`) already parses the full stdin JSON. It can extract topic signals before sending to the UDS server:

1. **SessionStart**: Scan `input.extra` for any topic/feature hints. Not much there today, but the hook could parse the CWD for project structure hints.

2. **UserPromptSubmit**: The prompt text is available as `input.prompt`. The hook can run `extract_feature_signal()` (or a lightweight version) on the prompt text before sending. If a topic is found, include it in the request:
   ```rust
   HookRequest::SessionRegister { feature: detected_topic, .. }
   // or
   HookRequest::PromptAndSearch { topic: detected_topic, .. }
   ```

3. **PreToolUse/PostToolUse**: The hook already parses tool_name and tool_input. For file-path-bearing tools (Read, Edit, Write), the hook can scan paths for `product/features/{topic}/` patterns and attach a `topic` field to the RecordEvent.

4. **SessionClose (Stop/TaskCompleted)**: The hook can do a final attribution pass over accumulated signals before closing.

**What the hook can realistically do**:
- Parse file paths from tool_input → `extract_from_path()` (cheap, O(1) per event)
- Parse prompt text for topic patterns → `extract_feature_id_pattern()` (cheap)
- Accumulate topic signals across events within the session (needs state — either in-process or via a lightweight side-channel)

**What the hook can't easily do**:
- Stateful accumulation across events (each hook invocation is a separate process)
- Content-based attribution requiring full session history

**Solution for statelessness**: The hook sends its per-event topic signal to the server. The server accumulates signals per session and resolves the dominant topic. This keeps the hook fast (extract + send) while the server handles aggregation.

```
Hook (per-event):
  1. Parse stdin
  2. Extract topic signal from tool_input/prompt (lightweight)
  3. Send to server: RecordEvent { ..., topic_signal: Option<String> }

Server (per-session accumulation):
  1. Receive event with optional topic_signal
  2. Tally signals per session_id (HashMap<session_id, Vec<topic_signal>>)
  3. On SessionClose: majority-vote → UPDATE sessions SET feature_cycle = dominant_topic
```

**B. Server-side attribution on SessionClose** (fallback)
- On SessionClose, load all observations for the session
- Run full `attribute_sessions()` content scan
- UPDATE sessions SET feature_cycle
- Create/update topic_deliveries row

**C. Explicit registration** (supplement, not primary)
- Coordinator agents know their topic — they can call a registration tool
- Works for structured workflows; doesn't help ad-hoc sessions
- Nice-to-have, not the primary mechanism

**Recommendation: A+B** — Hook extracts per-event topic signals (cheap, at the edge), server accumulates and resolves on SessionClose. Explicit registration as bonus when available.

### Priority 2: Topic Delivery Grouping
A topic's work spans multiple sessions. We need a layer above session:

```sql
CREATE TABLE topic_deliveries (
    topic TEXT PRIMARY KEY,          -- "col-016", "air-quality-sensors", etc.
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    status TEXT,                    -- 'active', 'completed', 'abandoned'
    total_sessions INTEGER DEFAULT 0,
    total_tool_calls INTEGER DEFAULT 0,
    total_duration_secs INTEGER DEFAULT 0
);
-- sessions.feature_cycle (= topic) becomes a FK to topic_deliveries
```

Auto-created on first session attribution to a topic. Updated as sessions complete. This becomes the anchor for retrospectives and cross-session analysis. Name aligns with knowledge-side `topic` field.

### Priority 3: UserPromptSubmit Storage
Currently: UserPromptSubmit → ContextSearch (search only, not stored)
Needed: UserPromptSubmit → ContextSearch AND observation record

The user prompt is the single richest signal for:
- Feature identification ("implement col-016...")
- Intent classification (bug fix, new feature, research)
- Task scope assessment
- Session purpose tracking

Implementation: Dual-route in build_request(). When prompt is non-empty, return both a ContextSearch AND a RecordEvent. Or: store the prompt as an observation before dispatching to ContextSearch.

### Priority 4: Query Text Capture
Currently: injection_log records (session, entry, confidence) but NOT the query.
Currently: audit_log records "returned N results" but NOT the query.

Needed: Store the query text alongside retrieval results.

Options:
- Add `query_text` column to injection_log (denormalized but simple)
- New `query_log` table: (query_id, session_id, query_text, ts, result_count, result_entry_ids)
- Enrich audit_log.detail to include query text

**Recommendation: New query_log table** — cleaner than denormalizing, and enables query-specific analysis independent of results.

```sql
CREATE TABLE query_log (
    query_id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    query_text TEXT NOT NULL,
    ts INTEGER NOT NULL,
    result_count INTEGER NOT NULL,
    result_entry_ids TEXT,           -- JSON array
    retrieval_mode TEXT,             -- 'strict', 'relaxed'
    source TEXT                      -- 'mcp', 'uds', 'hook'
);
```

### Priority 5: Subagent Span Reconstruction
Currently: SubagentStart records agent_type + prompt_snippet. SubagentStop records nothing.
No span_id linking start to stop.

Since session_id is shared across parent + subagents, and multiple concurrent subagents exist, we can't distinguish them by session_id alone.

Options:
- Add `span_id` field to SubagentStart/SubagentStop (requires Claude Code platform change)
- Heuristic: Match SubagentStart to nearest subsequent SubagentStop with same session_id by timestamp ordering (fragile with concurrency)
- Accept limitation: Use SubagentStart count as proxy for subagent activity

**Recommendation: Timestamp-based heuristic for now**, platform span_id as future enhancement.

---

## 4. Use Cases Enabled by Connected Data

### UC-1: Full Feature Retrospective (Restore + Enhance)
**What we had**: Content-scanned observations → metrics + hotspots + narratives.
**What we'd gain**: Multi-session view across the full feature lifecycle.

With feature_deliveries + session attribution:
- Cross-session patterns (e.g., design session had 80% Read, implementation had 50% Edit — normal)
- Phase-over-phase comparison within a single feature
- Session continuity analysis (did knowledge persist across sessions or was it re-discovered?)
- Time between sessions (indicates blocked, context-switched, or incremental delivery)

### UC-2: Knowledge Effectiveness Analysis
With query_log + injection_log + session outcome:
- **Entry utility scoring**: Which entries were injected → led to successful outcomes?
- **Knowledge gaps**: What queries returned 0 results? What are agents searching for that doesn't exist?
- **Confidence calibration**: Are high-confidence entries actually more useful than low-confidence ones?
- **Stale knowledge detection**: Entries that get injected but never lead to successful outcomes
- **Query clustering**: Group similar queries to identify recurring information needs

### UC-3: Agent Performance Profiling
With connected session → feature → observations:
- **Agent efficiency**: Tool calls per agent type, duration per agent type
- **Agent failure patterns**: Which agents trigger rework events? Which get respawned?
- **Subagent utilization**: Are the right agents being spawned for the right tasks?
- **Cross-feature comparison**: Is `uni-rust-dev` faster on nexus features than collective features?

### UC-4: Workflow Optimization
With multi-session feature data:
- **Phase duration benchmarks**: How long should design vs implementation vs testing take?
- **Session count norms**: How many sessions does a typical feature need? Outliers?
- **Context reload cost**: How much Read activity in session N repeats session N-1?
- **Coordinator overhead**: What % of tool calls are coordination vs actual work?

### UC-5: Query Data Export for Tuning
With query_log:
- **Search quality evaluation**: (query, results, outcome) triples for offline evaluation
- **Embedding model tuning**: query→entry pairs where entry was helpful = positive training signal
- **Retrieval calibration data**: similarity scores vs actual utility (from implicit feedback)
- **Miss analysis**: queries with 0 results → candidates for new knowledge entries

### UC-6: Predictive Signals
With enough connected historical data:
- **Rework prediction**: Session-level signals (high Read/Write ratio, many compile cycles) that predict rework
- **Feature complexity estimation**: Early-session metrics that correlate with total delivery effort
- **Knowledge demand forecasting**: Query patterns in design sessions predict what implementation sessions will need

### UC-7: Cross-Feature Learning
With feature_deliveries spanning multiple features:
- **Pattern reuse detection**: Same files/patterns accessed across features → shared convention candidate
- **Knowledge lifecycle**: When are entries created, how long until first use, peak usage period, decay
- **Feature dependency graph**: Features that access overlapping file sets or knowledge entries

### UC-8: Developer Experience Metrics
With UserPromptSubmit stored:
- **Task clarity**: Prompt length/complexity vs session efficiency
- **Friction taxonomy**: Categorize what users ask for help with most
- **Self-service rate**: How often do agents resolve without follow-up prompts?

---

## 5. Data We're NOT Capturing (and Should Consider)

| Data Point | Source | Value |
|-----------|--------|-------|
| **User prompt text** | UserPromptSubmit | Feature ID, intent, task scope |
| **Search query text** | ContextSearch / SearchService | Retrieval evaluation, gap analysis |
| **MCP tool parameters** (full) | MCP tool calls | What agents ask Unimatrix for |
| **Git operations** | Bash observations (partial) | Commit, branch, PR activity |
| **Test results** | Bash observations (partial) | Pass/fail, test count changes |
| **Compilation results** | Bash observations (partial) | Build success/failure patterns |
| **File diff sizes** | Edit/Write observations | Change magnitude per file |

Note: Some of these are partially available in Bash observation inputs/snippets. They could be extracted with specialized parsers at read time without schema changes.

---

## 6. Implementation Priority Matrix

| Change | Effort | Impact | Dependency |
|--------|--------|--------|------------|
| Persist attribution results (backfill feature_cycle) | Low | **Critical** | None |
| Dual-route UserPromptSubmit (store + search) | Low | High | None |
| feature_deliveries table | Medium | High | Attribution |
| query_log table | Medium | High | None |
| Explicit feature registration tool/parameter | Medium | Medium | feature_deliveries |
| SubagentStart span heuristic | Low | Medium | None |
| Specialized parsers for Bash observations | Medium | Medium | None |
| Query data export pipeline | Medium | High | query_log |

**Phase 1** (unblock retrospective): Persist attribution + dual-route UserPromptSubmit
**Phase 2** (connect the graph): feature_deliveries + query_log
**Phase 3** (advanced analytics): Export pipeline + specialized parsers + predictive signals
