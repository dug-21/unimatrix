# D14-7: Feature Scoping Recommendations — col-006 through col-011

**Spike:** ASS-014
**Date:** 2026-03-01
**Status:** Complete
**Inputs:** Architecture synthesis (D14-6), all five RQ findings (D14-1 through D14-5)

---

## col-006: Hook Transport Layer (Cortical Implant)

### Revised Scope

col-006 establishes the cortical implant as a subcommand of the `unimatrix-server` binary. It adds a Unix domain socket listener to the MCP server, implements the sync Transport trait for local communication, and provides the hook dispatch infrastructure that all subsequent features build on.

### What to Build

**1. UDS Listener in MCP Server**
- Add a tokio task that binds `UnixListener` on `~/.unimatrix/{project_hash}/unimatrix.sock`
- Socket mode 0o600 (owner-only)
- Accept connections, spawn handler per connection
- Length-prefixed JSON wire protocol (4-byte big-endian length + JSON payload)
- Socket lifecycle: create on startup (after PidGuard), remove on shutdown (before compaction)
- Stale socket detection and cleanup (same pattern as stale PID handling)

**2. Hook Subcommand (`unimatrix-server hook <EVENT>`)**
- Clap subcommand: `hook` with positional `EVENT` argument
- Read JSON from stdin (Claude Code hook format)
- Parse `hook_event_name`, `session_id`, `cwd`, and event-specific fields
- Dispatch to event-specific handlers (initially: SessionStart, SessionEnd, Ping)
- Instance discovery: compute project hash from `cwd`, find `unimatrix.sock`
- Connect to UDS, send request, receive response (or fire-and-forget)
- Write response to stdout (when synchronous) or exit immediately (fire-and-forget)
- Exit codes: 0 = success, 1 = error (logged to stderr, non-blocking)

**3. Transport Trait (`unimatrix-engine` or `unimatrix-core`)**
- `Transport` trait: `request()`, `fire_and_forget()`, `is_connected()`, `connect()`, `disconnect()`
- `LocalTransport` implementation: UDS connect, length-prefixed JSON serialization, timeout via `SO_RCVTIMEO`/`SO_SNDTIMEO`
- Request/Response enums: `ContextSearch`, `Briefing`, `CompactPayload`, `RecordEvent`, `RecordEvents`, `SessionRegister`, `SessionClose`, `Ping`/`Pong`
- `TransportError` enum: `Unavailable`, `Timeout`, `Rejected`, `Codec`, `Transport`

**4. `unimatrix-engine` Crate (Shared Business Logic)**
- Extract from `unimatrix-server`: `confidence.rs`, `coaccess.rs`, `project.rs`
- New: `search.rs` (embed + HNSW + re-rank + co-access boost pipeline)
- New: `query.rs` (index-based lookup with filtering)
- Both MCP server and hook UDS handler call into `unimatrix-engine`

**5. Graceful Degradation**
- Local event queue (WAL): `~/.unimatrix/{hash}/event-queue/pending-{ts}.jsonl`
- Queue replay on successful connection
- Size limits: 1000 events/file, 10 files max, 7-day pruning
- Compaction cache: `~/.unimatrix/{hash}/compaction-cache.json` (updated on every injection, read as fallback on PreCompact)

**6. Authentication**
- SO_PEERCRED extraction (Linux) / getpeereid (macOS)
- UID verification (same user)
- Process lineage check (`/proc/{pid}/cmdline` for "unimatrix-server")
- Shared secret fallback (`~/.unimatrix/{hash}/auth.token`, 32 random bytes, mode 0o600)
- Pre-enroll `cortical-implant` as Internal trust agent in `bootstrap_defaults()`

**7. `.claude/settings.json` Hook Configuration**
- Document manual configuration (Method 1)
- Replace existing bash observation hooks with `unimatrix-server hook <EVENT>` commands
- Initially register: SessionStart, PostToolUse, Stop (minimal set for transport validation)

### Dependencies

- Existing: `unimatrix-server` (vnc-001 through vnc-004), all core crates
- New: `unimatrix-engine` crate must be created by extracting from server
- No dependency on col-007 through col-011 (col-006 is the foundation)

### Estimated Complexity

**Large.** Multiple subsystems: UDS listener, hook subcommand, transport trait, engine extraction, auth, graceful degradation. This is the most architecturally significant feature in M5 -- it adds a second communication channel to the server and a new binary entry point.

Estimated effort:
- UDS listener + wire protocol: ~400 LOC
- Hook subcommand + dispatch: ~300 LOC
- Transport trait + LocalTransport: ~350 LOC
- unimatrix-engine extraction: ~500 LOC moved, ~200 LOC new
- Authentication: ~250 LOC
- Graceful degradation (WAL + cache): ~300 LOC
- Tests: ~600 LOC
- **Total: ~2,900 LOC** (including ~500 moved from server)

### Key Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| UDS latency exceeds 50ms budget | High | Prototype early. Measure end-to-end latency before building features on top. Latency estimates (12-36ms for search) are analytical -- validate empirically. |
| Engine extraction breaks existing MCP tools | High | Comprehensive integration test run after extraction. Extract incrementally: move one module at a time, verify all 174 integration tests pass after each move. |
| Socket lifecycle conflicts with PidGuard | Medium | Socket creation/cleanup follows same pattern as PidGuard. Add socket cleanup to `LifecycleHandles` shutdown sequence. Test: server crash leaves stale socket; next startup cleans it. |
| Claude Code hook stdin format changes | Medium | Parse JSON loosely with `#[serde(default)]` and `#[serde(flatten)]`. Test against documented hook schema. |

---

## col-007: Automatic Context Injection

### Revised Scope

col-007 implements the UserPromptSubmit hook handler that queries Unimatrix for knowledge relevant to the current prompt and injects it into the agent's context via stdout. This is the core value proposition: every prompt gets enriched with relevant knowledge automatically.

### What to Build

**1. UserPromptSubmit Hook Handler**
- Extract prompt text from stdin JSON
- Send `ContextSearch` request to server via Transport
- Parameters: query=prompt (or summarized prompt), role (from session), feature (from session), k=5, max_tokens=500
- Receive `Response::Entries` with matched entries and confidence scores

**2. Server-Side Search Endpoint**
- Route `ContextSearch` to the search pipeline in `unimatrix-engine`
- Embed query text (ONNX, ~3ms hot)
- HNSW search (k=5, ef_search=32)
- Confidence re-ranking: 0.85*similarity + 0.15*confidence + co_access_boost
- Token budget enforcement: truncate entries to fit within max_tokens
- Return formatted entries with ID, title, content, confidence, similarity, category

**3. Injection Formatting**
- Format matched entries as structured markdown for stdout injection
- Include entry IDs (for compaction defense tracking)
- Include confidence scores (for agent trust calibration)
- Token count tracking (ensure <500 tokens per injection)

**4. Injection Recording**
- After printing to stdout, fire-and-forget `RecordEvent(injection, entry_ids, scores, session_id)` to server
- Server writes to INJECTION_LOG
- Server updates SessionState.injection_history

**5. Co-Access Recording (Integration with crt-004)**
- Generate co-access pairs from injected entry sets
- Record via existing `generate_pairs()` infrastructure with session-scoped dedup
- Same CO_ACCESS table, same boost computation

### Dependencies

- **col-006** (transport layer, UDS listener, hook subcommand) -- hard dependency
- **crt-004** (co-access recording) -- existing, extend with injection pairs
- Existing: search pipeline, embedding, HNSW index

### Estimated Complexity

**Medium.** The search pipeline exists. The main work is the hook handler, injection formatting, and injection recording.

Estimated effort:
- UserPromptSubmit handler: ~150 LOC
- Server-side ContextSearch endpoint: ~200 LOC (mostly wiring to existing search)
- Injection formatting: ~100 LOC
- Injection recording: ~150 LOC
- Co-access integration: ~50 LOC
- Tests: ~300 LOC
- **Total: ~950 LOC**

### Key Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Injection quality is poor (irrelevant entries) | High | Start with k=3, conservative. Use confidence threshold (skip entries below 0.3). Evaluate injection quality through manual review before increasing volume. |
| Token budget exceeded causes context bloat | Medium | Hard limit in formatting. Count tokens before printing. Truncate last entry if budget exceeded. |
| Prompt embedding latency spike (cold ONNX) | Medium | Server-side embedding only. ONNX should be warm after first MCP tool call. If cold, the ~200ms embed time blows the budget -- add a fast-path: skip injection on first hook if ONNX not ready (return EmbedNotReady). |

---

## col-008: Compaction Resilience

### Revised Scope

col-008 implements the PreCompact hook handler that constructs a token-budgeted knowledge payload from server-side session state and injects it into the compacted window. This preserves critical context when Claude Code compresses conversation history.

### What to Build

**1. PreCompact Hook Handler**
- Parse stdin JSON, extract session_id
- Send `CompactPayload` request to server via Transport
- Parameters: session_id, injected_entry_ids (from session context or passed by Claude Code), role, feature, token_limit=2000
- Receive `Response::Briefing{content, token_count}`
- Print to stdout

**2. Server-Side CompactPayload Endpoint**
- Read SessionState from in-memory map
- If session state available (normal path):
  1. Format session context (role, task, feature) -- ~100 tokens
  2. Fetch active decisions for current feature -- ~600 tokens
  3. Fetch top-N entries from injection_history by confidence -- ~600 tokens
  4. Fetch relevant conventions for active role -- ~400 tokens
  5. Include correction chains from session -- ~200 tokens
  6. Truncate to token budget
- If no session state (fallback):
  1. Call context_briefing(role, task, feature) internally
  2. Return briefing result as compaction payload

**3. Server-Side Session State Management**
- In-memory `HashMap<String, SessionState>` (session_id -> state)
- Created on SessionRegister, updated on every injection recording
- Cleared on SessionClose
- Pre-computed compaction payload updated after every injection
- Adaptive token allocation: reduce re-injected entries on repeated compaction

**4. Disk-Based Compaction Cache**
- Write `compaction-cache.json` on every successful UserPromptSubmit injection
- Read as final fallback when server unavailable
- Cache content: session_id, timestamp, entry summaries, role, feature
- Stale check: only use if same session_id and <30 minutes old

**5. Token Budget Management**
- Priority-based allocation within 2000-token budget
- Adaptive: reduce injection replays on compaction_count > 1
- Compaction frequency feedback: if compaction_count > 3, reduce col-007 injection volume

### Dependencies

- **col-006** (transport layer) -- hard dependency
- **col-007** (context injection) -- the injection recording that populates SessionState.injection_history
- **vnc-003** (context_briefing) -- fallback path uses existing briefing logic

### Estimated Complexity

**Medium-High.** The compaction payload construction logic is new. Session state management adds stateful behavior to the server. Token budgeting requires careful design.

Estimated effort:
- PreCompact handler: ~100 LOC
- CompactPayload endpoint: ~300 LOC
- Session state management: ~250 LOC
- Compaction cache (disk): ~150 LOC
- Token budget management: ~200 LOC
- Tests: ~400 LOC
- **Total: ~1,400 LOC**

### Key Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Token budget allocation sub-optimal (too much of one category, too little of another) | High | Tune empirically. Start with equal allocation, measure what agents actually need post-compaction. Make allocations configurable. |
| Server restart mid-session loses session state | Medium | Disk-based compaction cache provides partial recovery. Briefing fallback provides role/task context without session history. |
| Compaction payload latency exceeds 50ms on fallback path | Medium | Pre-computed payload is primary (~5ms). Briefing fallback is ~20ms. Only briefing + cold ONNX (~200ms) could blow the budget -- but ONNX is warm if any prior hook has run. |

---

## col-009: Closed-Loop Confidence

### Revised Scope

col-009 implements asymmetric implicit confidence signals from session outcomes. When a session completes successfully, entries injected during that session receive auto-applied helpful signals (closing the confidence feedback loop without agent cooperation). When rework is detected, entries are **flagged for human review** in the retrospective pipeline — NOT auto-downweighted. This "auto-positive, flag-negative, never auto-downweight" design prevents guilt-by-association (where good entries co-injected with a bad one get incorrectly penalized). Only explicit MCP votes can increment unhelpful_count.

### What to Build

**1. Session-End Signal Generation (asymmetric)**
- On SessionClose, determine session outcome (success/rework/abandoned)
- For success: generate `Helpful` signals for all entries in SessionState.injection_history → auto-applied to EntryRecord.helpful_count via confidence pipeline
- For rework: generate `Flagged` signals for injected entries → routed to retrospective pipeline (col-002) for human review. NOT applied to unhelpful_count.
- For abandoned: no signals (inconclusive)
- Write SignalRecords to SIGNAL_QUEUE with appropriate SignalSource discriminator

**2. Mid-Session Rework Detection**
- Monitor PostToolUse events for rework patterns:
  - Repeated edits to same file
  - Compile/test failures followed by edits
  - Undo patterns (revert tool calls)
- On rework detection: generate `Flagged` signals for recently injected entries (surfaced in retrospective, not auto-applied)

**3. Signal Queue Processing (two consumers)**
- Confidence consumer: drain Helpful signals from SIGNAL_QUEUE, group by entry_id, apply to EntryRecord.helpful_count, recompute confidence
- Retrospective consumer: drain Flagged signals, store as entries_analysis data for next retrospective report
- Triggers: session end, periodic timer (every 5 minutes), maintain=true
- Session-scoped dedup: max 1 helpful vote per entry per session from implicit signaling
- Apply via existing `record_usage_with_confidence()` with source="hook"

**4. Usage Recording Extension (crt-001 Impact)**
- Add `source` discriminator to usage recording: "mcp" vs "hook"
- Implement `filter_injection_access()` on UsageDedup: dedup per (session_id, entry_id)
- Batch session-end usage recording: increment access_count once per entry per session

**5. SIGNAL_QUEUE Table and Records**
- SignalRecord: session_id, timestamp, entry_ids, signal_type (Helpful/Flagged), source (ImplicitOutcome/ImplicitRework/ExplicitMcp)
- Monotonic key from COUNTERS (next_signal_id)
- Cap at 10,000 entries; drop oldest on overflow

**6. Retrospective Integration (col-002 extension)**
- Expand `RetrospectiveReport` with `entries_analysis` field
- Correlate INJECTION_LOG + SIGNAL_QUEUE(Flagged) + session outcomes per feature_cycle
- Surface entry-level performance: which entries correlated with success vs. rework, injection frequency, agent-specific effectiveness
- Present alongside existing 21-rule hotspot detection and baseline comparison

### Dependencies

- **col-006** (transport layer) -- hard dependency (hook events flow via IPC)
- **col-007** (context injection) -- provides injection_history that signals reference
- **col-010** (session lifecycle) -- provides clean session boundaries for signal generation
- **crt-001** (usage tracking) -- extended with source discriminator and session-scoped dedup
- **crt-002** (confidence evolution) -- no formula changes; Helpful signals feed helpful_count. Flagged signals bypass confidence entirely.
- **col-002** (retrospective pipeline) -- extended with entries_analysis for Flagged signal consumption

### Estimated Complexity

**Medium.** The signal generation logic is straightforward. The main complexity is in the dual-consumer routing (confidence vs. retrospective) and the entries_analysis integration with col-002.

Estimated effort:
- Signal generation (session-end + rework, asymmetric): ~250 LOC
- Signal queue table + records: ~150 LOC
- Signal consumer (dual-path: confidence + retrospective): ~250 LOC
- UsageDedup extension: ~100 LOC
- RetrospectiveReport entries_analysis: ~200 LOC
- Tests: ~400 LOC
- **Total: ~1,350 LOC** (up from ~1,050 due to dual-consumer and entries_analysis)

### Key Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Helpful signals inflate Wilson scores for tangentially-injected entries | Low | Wilson 5-vote minimum guard means 5+ sessions before deviation from neutral prior. Session-scoped dedup (max 1 vote per entry per session). Over time, frequently helpful entries rise naturally; rarely helpful ones stagnate near 0.5. |
| Rework detection false positives | Medium | Start conservative: only flag rework on 3+ consecutive edits to same file with intervening failures. Tunable thresholds. Flagged entries go to human review, not auto-downweight, so false positives are informational rather than damaging. |
| Signal queue grows unbounded | Low | Hard cap at 10,000. Drop oldest. Consumer runs on session end and every 5 minutes. |
| entries_analysis adds complexity to retrospective | Low | Builds on existing RetrospectiveReport structure. Entry-level analysis is additive — existing hotspot/baseline analysis unchanged. |

---

## col-010: Session Lifecycle & Observation

### Revised Scope

col-010 implements explicit session lifecycle tracking via SessionStart and SessionEnd hooks, plus observation event recording that replaces the JSONL telemetry pipeline. It provides clean session boundaries for col-002 retrospective analysis and session-scoped dedup for crt-001/crt-004.

### What to Build

**1. SessionStart Hook Handler**
- Parse session_id, cwd, agent context from stdin JSON
- Send `SessionRegister` to server
- Server creates SESSIONS record (status: Active), initializes SessionState

**2. SessionEnd/Stop/TaskCompleted Hook Handlers**
- Parse session_id, outcome data
- Send `SessionClose` to server
- Server updates SESSIONS record (status: Completed, ended_at)
- Trigger signal generation (col-009)
- Clear SessionState from memory

**3. SESSIONS Table**
- SessionRecord: session_id, parent_pid, agent_role, agent_task, feature_cycle, started_at, ended_at, status, compaction_count, total_injections, total_mcp_retrievals, injected_entry_ids, mcp_retrieved_entry_ids, outcome_summary
- Time-based GC: sessions older than 30 days deleted during maintenance
- Active sessions never GC'd

**4. Schema v3->v4 Migration**
- Add session_id field to EntryRecord (`#[serde(default)]`)
- Create SESSIONS, INJECTION_LOG, SIGNAL_QUEUE tables in Store::open()
- Add next_signal_id to COUNTERS
- Scan-and-rewrite existing entries for session_id field

**5. Structured Event Ingestion for col-002**
- New `from_structured_events()` entry point in `unimatrix-observe`
- Converts SessionRecord + InjectionRecord data into ObservationRecord format
- Feature attribution uses session's feature_cycle directly (no content-based inference needed)
- JSONL parser retained as legacy/fallback

**6. Auto-Generated Session Outcomes (col-001 Integration)**
- On SessionEnd, optionally create outcome entry via context_store
- Category: "outcome", type: "session"
- Add "session" to VALID_TYPES in outcome_tags.rs
- Structured tags: type:session, phase:implementation, result:pass/fail, source:auto

### Dependencies

- **col-006** (transport layer) -- hard dependency
- **col-001** (outcome tracking) -- extend with "session" type
- **col-002** (retrospective pipeline) -- add structured event ingestion
- Schema v4 migration must ship with or before col-010

### Estimated Complexity

**Medium-High.** Multiple integration points: schema migration, session lifecycle, observation pipeline, outcome tracking.

Estimated effort:
- Session handlers (start + end): ~200 LOC
- SESSIONS table + SessionRecord: ~200 LOC
- Schema v3->v4 migration: ~150 LOC
- Structured event ingestion for col-002: ~250 LOC
- Auto-generated session outcomes: ~100 LOC
- Telemetry GC (maintenance extension): ~150 LOC
- Tests: ~400 LOC
- **Total: ~1,450 LOC**

### Key Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Schema migration breaks existing data | High | Follow established migration pattern (3 prior successful migrations). Test with real database snapshot. Migration is append-only (new field with serde(default)). |
| Session boundaries unreliable (hooks missed) | Medium | Timeout detection: sessions with ended_at=0 and started_at > 24h ago are marked TimedOut during maintenance. Gap analysis recovers from missed SessionEnd hooks. |
| Structured events and JSONL produce different retrospective results | Medium | Cross-path equivalence tests. Both paths produce ObservationRecord structs. Run both paths on same session data and compare MetricVectors. |

---

## col-011: Semantic Agent Routing

### Revised Scope

col-011 implements a UserPromptSubmit hook handler that matches the current prompt against stored agent duties, patterns, and historical outcomes to recommend the best-fit agent for the task. Advisory output only -- prints recommendation, does not spawn.

### What to Build

**1. UserPromptSubmit Routing Handler**
- Triggered alongside (or instead of) col-007 context injection on specific events (SubagentStart, or a dedicated routing request)
- Extract prompt/task description from stdin
- Send routing query to server

**2. Server-Side Semantic Routing Query**
- Embed prompt text (server-side ONNX)
- Search against entries with category "duties" (agent role definitions)
- Search against entries with category "outcome" (historical success/failure patterns)
- Score: combination of semantic similarity, confidence, and outcome history
- Return ranked list of agent recommendations

**3. Advisory Output**
- Format routing recommendation as structured text to stdout
- Include: recommended agent(s), match scores, reasoning (which duties/outcomes matched)
- Does NOT spawn agents or modify workflow -- purely informational

**4. Connection to col-001 Outcomes**
- Query OUTCOME_INDEX for entries related to the task's feature cycle
- Boost agents with successful outcome history for similar tasks
- Penalize agents with rework/failure outcomes for similar tasks

### Dependencies

- **col-006** (transport layer) -- hard dependency
- **col-001** (outcome tracking) -- outcome entries inform routing quality
- Knowledge base must contain "duties" category entries (populated via alc-001 knowledge bootstrap or manual entry)

### Estimated Complexity

**Low-Medium.** Routing is conceptually a filtered semantic search with a different output format. The search pipeline exists; the main work is the query formulation and scoring logic.

Estimated effort:
- Routing handler: ~100 LOC
- Server-side routing query: ~250 LOC
- Scoring (similarity + confidence + outcome): ~150 LOC
- Advisory formatting: ~100 LOC
- Tests: ~250 LOC
- **Total: ~850 LOC**

### Key Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| No "duties" entries in knowledge base | High | Requires knowledge bootstrap (alc-001 populated duties entries, though they were deprecated). New duties entries must be created before col-011 provides value. |
| Routing quality is poor (wrong agent recommendations) | Medium | Start advisory-only. Log routing decisions for retrospective analysis. Tune scoring weights based on outcome correlation. |
| Overlap with col-007 context injection | Low | Clear separation: col-007 injects knowledge entries. col-011 routes to agents. Different hook events or different output sections. |

---

## Dependency Graph

```
col-006: Hook Transport Layer
   │
   │  FOUNDATION — all other features depend on this
   │
   ├──> col-007: Context Injection
   │       │
   │       └──> col-008: Compaction Resilience
   │               (requires injection_history from col-007)
   │
   ├──> col-010: Session Lifecycle
   │       │
   │       ├──> col-009: Closed-Loop Confidence
   │       │       (requires clean session boundaries)
   │       │
   │       └──> [col-002 integration: structured event ingestion]
   │
   └──> col-011: Semantic Agent Routing
           (independent, needs only transport + search)
```

### External Dependencies (from existing features)

```
col-009 ──depends──> crt-001 (usage tracking extension)
col-009 ──depends──> crt-002 (confidence formula, no changes needed)
col-010 ──depends──> col-001 (outcome tracking, 1-line type addition)
col-010 ──depends──> col-002 (structured event ingestion path)
col-007 ──depends──> crt-004 (co-access recording, extension)
```

### Schema v4 Migration Dependency

The schema migration (v3->v4: adding session_id to EntryRecord, creating 3 new tables) must ship with the first feature that writes to the telemetry tables. This is either:
- **col-006** if it includes basic session registration (recommended -- establish the tables early)
- **col-010** if col-006 is transport-only without telemetry writes

**Recommendation:** Include schema v4 migration in col-006. Even if the initial hook handlers don't write session data, having the tables present prevents a second migration later.

---

## Recommended Implementation Order

### Wave 1: col-006 (Foundation)

**What:** Transport layer, UDS listener, hook subcommand, engine extraction, auth, graceful degradation, schema v4 migration.

**Why first:** Every other feature depends on it. Validates the core architectural hypothesis (IPC latency, UDS reliability, hook integration).

**Validation gate:** End-to-end test: hook fires, connects to server via UDS, server responds, hook prints to stdout. Measured latency < 50ms.

### Wave 2: col-007 + col-010 (in parallel)

**col-007** (Context Injection): Uses the transport from Wave 1 to inject knowledge. Validates the core value proposition: agents get enriched prompts without tool calls.

**col-010** (Session Lifecycle): Uses the transport from Wave 1 to register and close sessions. Provides session boundaries needed by col-008 and col-009. Includes the col-002 structured event integration.

**Why parallel:** col-007 and col-010 have no dependency on each other. col-007 needs search; col-010 needs session tables. Both need col-006 transport.

**Validation gate:**
- col-007: Agent receives injected knowledge on prompt. Injection recorded in INJECTION_LOG.
- col-010: Session created and closed in SESSIONS table. Structured events flow to col-002 pipeline.

### Wave 3: col-008 + col-009 (sequential or parallel)

**col-008** (Compaction Resilience): Requires injection_history from col-007 to build the compaction payload. Depends on SessionState populated by col-007's injection recording.

**col-009** (Closed-Loop Confidence): Requires clean session boundaries from col-010 to generate bulk signals. Requires injection_history to know which entries to signal.

**Sequencing:** col-008 and col-009 can proceed in parallel IF both col-007 and col-010 are complete. If only one of col-007/col-010 is complete:
- col-008 can start after col-007 alone (session boundaries optional for compaction defense)
- col-009 should wait for col-010 (session boundaries are essential for clean signal generation)

**Validation gate:**
- col-008: PreCompact hook returns knowledge payload. Agent retains context after compaction.
- col-009: Session-end hook generates signals. EntryRecord.helpful_count updated after session.

### Wave 4: col-011 (Independent)

**col-011** (Semantic Agent Routing): Independent of col-007/008/009/010. Only needs col-006 transport and populated knowledge base (duties, outcomes entries).

**Why last:** Lowest business value of the six features. Context injection and compaction defense deliver more impact. Routing is advisory and requires knowledge base content that may not exist yet.

**Can be parallelized with Wave 2 or Wave 3** if engineering capacity allows. It has no dependencies beyond col-006.

### Summary Timeline

```
Wave 1:  col-006 ─────────────────────────────────────
                                                       │
Wave 2:                col-007 ───────────  ┐          │
                       col-010 ───────────  ┤ parallel │
                                            │          │
Wave 3:                         col-008 ──  ┤          │
                                col-009 ──  ┘          │
                                                       │
Wave 4:  col-011 ──────────────────────────────────────┘
         (can overlap with Wave 2/3)
```

### What Can Be Parallelized

| Pair | Parallel? | Rationale |
|------|-----------|-----------|
| col-007 + col-010 | Yes | Independent concerns (injection vs. session lifecycle) |
| col-007 + col-011 | Yes | Independent concerns (injection vs. routing) |
| col-008 + col-009 | Yes (if col-007 + col-010 done) | Independent concerns (compaction vs. confidence) |
| col-008 + col-011 | Yes | No shared state |
| col-009 + col-011 | Yes | No shared state |
| col-007 + col-008 | No | col-008 needs col-007's injection_history |
| col-010 + col-009 | No | col-009 needs col-010's session boundaries |

### Risk-Ordered Alternative

If the goal is to reduce risk early (validate the hardest hypotheses first):

1. **col-006** -- validates IPC latency (the hardest technical hypothesis)
2. **col-008** -- validates compaction defense (the most latency-critical hook)
3. **col-007** -- validates injection quality (the core value proposition)
4. **col-010** -- validates session lifecycle (integration with col-002)
5. **col-009** -- validates implicit confidence (integration with crt-001/crt-002)
6. **col-011** -- validates agent routing (lowest risk, lowest dependency)

This order builds compaction defense before full injection, which is backwards for user value but forward for risk reduction. col-008 is the tightest latency constraint (50ms with pre-computed payload serving) -- validating it early de-risks the entire hook architecture.

---

## Sizing Summary

| Feature | Estimated LOC | Complexity | Wave |
|---------|:------------:|:----------:|:----:|
| col-006 | ~2,900 | Large | 1 |
| col-007 | ~950 | Medium | 2 |
| col-008 | ~1,400 | Medium-High | 3 |
| col-009 | ~1,050 | Medium | 3 |
| col-010 | ~1,450 | Medium-High | 2 |
| col-011 | ~850 | Low-Medium | 4 |
| **Total** | **~8,600** | | |

For context: existing codebase is ~1,199 unit + integration tests across 5 crates. The delivery features add roughly 8,600 LOC of production code + ~2,300 LOC of tests, representing a significant expansion (~40-50% growth in server-side code).

---

## Scoping Revision (2026-03-02)

**Decision: col-010a eliminated. col-009 owns its schema migration. col-010 scope unchanged.**

During col-009 scoping, the original Wave 3 plan (col-010a infrastructure → col-009 consumer) was revised:

**Problem with col-010a:** An infrastructure-only feature that creates tables (SIGNAL_QUEUE, SESSIONS, INJECTION_LOG) without a writer means tests can only validate that migrations run and tables exist — not that the schema is fit for purpose. The first real validation happens when col-009 tries to use the tables, by which point col-010a is complete and context is split between features. Infrastructure-without-consumer is a testing anti-pattern.

**Revised approach:**
- **col-009** owns schema v4: adds SIGNAL_QUEUE table + `next_signal_id` counter — the only table col-009 writes to. Consistent with prior convention (crt-001 owned USAGE_LOG, crt-005 owned confidence f64 migration). Every table tested by its actual writer.
- **col-010** owns schema v5: adds SESSIONS table, INJECTION_LOG table, and `session_id: Option<String>` on EntryRecord. Ships independently as the full session lifecycle + col-002 integration feature. No scope change from "col-010b".
- **No col-010a.** The thin schema-migration-only feature is eliminated entirely.

**Revised sizing:**

| Feature | Estimated LOC | Complexity | Wave | Schema |
|---------|:------------:|:----------:|:----:|--------|
| col-006 | ~2,900 | Large | 1 | — |
| col-007 | ~950 | Medium | 2 | — |
| col-008 | ~1,400 | Medium-High | 3 | — |
| col-009 | ~1,350 | Medium | 4 | v4: SIGNAL_QUEUE |
| col-010 | ~1,450 | Medium-High | 5 | v5: SESSIONS, INJECTION_LOG, session_id |
| col-011 | ~850 | Low-Medium | independent | — |

col-009 LOC estimate increases from ~1,050 to ~1,350 to account for the schema migration it now owns.

**Revised dependency chain:**
```
col-006 → col-007 → col-008 → col-009 (schema v4) → col-010 (schema v5)
col-006 → col-011 (independent)
```

**SessionStop reliability constraint (surfaced during col-009 scoping):**
SessionEnd/Stop hooks cannot be guaranteed to always fire (crashes, OOM, force-quit). col-009 must not treat SessionEnd as a hard trigger. Required design elements regardless of ordering:
1. Optimistic path: SessionClose fires → generate signals immediately
2. Recovery path: periodic sweep processes orphaned in-memory sessions
3. Dedup guarantee: at-most-once per (session, entry) even if signal generation runs twice

This recovery logic is col-009's responsibility (it owns signal generation). col-010's stale session sweep (marking SESSIONS records TimedOut) is a separate concern with the same trigger condition.
