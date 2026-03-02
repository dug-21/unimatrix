# D14-6: Architecture Synthesis — Cortical Implant

**Spike:** ASS-014
**Date:** 2026-03-01
**Status:** Complete
**Synthesizes:** RQ-1 (Data Model), RQ-2 (Access Pattern), RQ-3 (Impact Assessment), RQ-4 (Transport & Security), RQ-5 (Distribution)

---

## 1. Executive Summary

The cortical implant is a hook-driven delivery system that connects Unimatrix's knowledge engine to Claude Code agents automatically. It is not a new binary but a subcommand of the existing `unimatrix-server` binary (`unimatrix-server hook <EVENT>`), configured once in `.claude/settings.json`, dispatching all Claude Code lifecycle hook events to the running Unimatrix MCP server via a Unix domain socket.

The architecture is forced by a single hard constraint: redb v3.1.x takes an exclusive file lock. No second process can open the database while the MCP server is running. Every read and every write from the hook process must go through IPC to the server. This constraint, discovered in RQ-2, shapes the entire design: the server becomes the sole data owner, the hook process becomes a thin IPC client, and session state lives server-side.

The data model adds a telemetry tier (3 new redb tables: SESSIONS, INJECTION_LOG, SIGNAL_QUEUE) alongside the existing 14 knowledge tables, with strict isolation -- telemetry is never embedded, never appears in search results, and has its own time-based garbage collection. Compaction defense state lives in server process memory, updated on every injection, read instantly on PreCompact hook.

The transport uses a synchronous `Transport` trait with length-prefixed JSON over Unix domain socket. Authentication is layered: filesystem permissions (socket mode 0o600), kernel peer credentials (SO_PEERCRED on Linux, getpeereid on macOS), and process lineage verification (/proc/{pid}/cmdline). No tokens, no passwords, no configuration.

The system degrades gracefully. If the server is unavailable, synchronous hooks (UserPromptSubmit, PreCompact) produce no output -- agents work as before, just without enrichment. Fire-and-forget hooks (PostToolUse, SessionEnd) queue events to a local WAL file for replay when the server returns. This is the zero-regression guarantee: the implant can only add value, never subtract it.

No existing features require changes to ship col-006 (the transport layer). Changes to crt-001, crt-002, crt-004, col-001, col-002, and vnc-003 cascade from features that USE the transport (col-007 through col-010), not from the transport itself. All can proceed incrementally.

---

## 2. Architecture Overview

### System Diagram

```
                    Claude Code Process
                           |
          spawns hook process per lifecycle event
                           |
                           v
                ┌─────────────────────────┐
                │  unimatrix-server hook   │  Ephemeral process (~10-50ms)
                │  <EVENT>                 │  Reads JSON from stdin
                │                          │  Writes injection to stdout
                │  (same binary as MCP     │  Writes diagnostics to stderr
                │   server, hook subcmd)   │
                └────────────┬────────────┘
                             |
              Unix domain socket (unimatrix.sock)
              Length-prefixed JSON, sync req/resp
                             |
                             v
                ┌─────────────────────────┐
                │  unimatrix-server       │  Long-running MCP server
                │                          │
                │  ┌──────────┐ ┌────────┐│
                │  │ stdio    │ │  UDS   ││  Two listeners:
                │  │ (MCP)    │ │ (hooks)││  - stdio for MCP tools
                │  └────┬─────┘ └───┬────┘│  - UDS for hook requests
                │       │           │      │
                │       v           v      │
                │  ┌──────────────────────┐│
                │  │ unimatrix-engine     ││  Shared business logic:
                │  │ (search, briefing,   ││  search pipeline, confidence
                │  │  confidence, co-acc) ││  re-ranking, co-access boost
                │  └──────────┬───────────┘│
                │             │            │
                │  ┌──────────v───────────┐│
                │  │  redb (17 tables)    ││  Single-writer, exclusive lock
                │  │  ┌────────────────┐  ││
                │  │  │ Knowledge tier │  ││  14 existing tables (unchanged)
                │  │  │ (entries, idx, │  ││
                │  │  │  vectors, etc) │  ││
                │  │  ├────────────────┤  ││
                │  │  │ Telemetry tier │  ││  3 new tables (SESSIONS,
                │  │  │ (sessions, inj │  ││  INJECTION_LOG, SIGNAL_QUEUE)
                │  │  │  log, signals) │  ││
                │  │  └────────────────┘  ││
                │  └──────────────────────┘│
                └─────────────────────────┘
```

### Component Inventory

| Component | Status | Change |
|-----------|--------|--------|
| `unimatrix-store` | Existing | Add 3 telemetry tables, 1 EntryRecord field (session_id), schema v3->v4 migration |
| `unimatrix-vector` | Existing | No changes (telemetry never embedded) |
| `unimatrix-embed` | Existing | No changes (server-side only, hook process never loads ONNX) |
| `unimatrix-core` | Existing | No changes to traits (telemetry uses direct redb access) |
| `unimatrix-engine` | **NEW** | Shared business logic extracted from server: search pipeline, confidence re-ranking, co-access boost, project discovery, briefing logic |
| `unimatrix-server` | Existing | Add UDS listener (tokio task), hook subcommand dispatch, session state management |
| `unimatrix-observe` | Existing | Later: add structured event ingestion alongside JSONL parser |
| Hook subcommand | **NEW** | `unimatrix-server hook <EVENT>` -- thin IPC client in same binary |

### Data Flow by Hook Event Type

**UserPromptSubmit (context injection):**
```
stdin JSON → parse prompt → IPC: ContextSearch(query, role, feature, k=5, max_tokens=500)
  → server embeds query (ONNX, ~3ms hot) → HNSW search → confidence re-rank → co-access boost
  → Response::Entries{items, total_tokens} → format as markdown → stdout
  → IPC (fire-and-forget): RecordEvent(injection, entry_ids, scores)
  → server writes INJECTION_LOG, updates session state
```

**PreCompact (compaction defense):**
```
stdin JSON → parse session_id → IPC: CompactPayload(session_id, injected_ids, role, feature, token_limit=2000)
  → server reads session state (in-memory map of injected entry IDs)
  → fetches entries by ID (fast, <1ms each) → sorts by confidence → truncates to budget
  → Response::Briefing{content, token_count} → stdout
```

**PostToolUse (observation):**
```
stdin JSON → parse tool event → IPC (fire-and-forget): RecordEvent(tool_use, tool_name, duration)
  → exit immediately (does not wait for server)
  → server writes to INJECTION_LOG or processes as observation
```

**SessionStart:**
```
stdin JSON → parse session_id, cwd → IPC: SessionRegister(session_id, role, feature)
  → server creates SESSIONS record, initializes in-memory session state
```

**SessionEnd:**
```
stdin JSON → parse session_id → IPC: SessionClose(session_id, outcome, duration)
  → server closes SESSIONS record, applies accumulated SIGNAL_QUEUE entries
  → optionally creates session summary entry via context_store
  → clears in-memory session state
```

---

## 3. Key Architectural Decisions

### D1: IPC Over Unix Domain Socket

**Decision:** The cortical implant communicates with the Unimatrix MCP server exclusively via Unix domain socket. No direct database access.

**Alternatives considered:**
- *Direct redb access (both processes open database)* -- Impossible. redb v3.1.x acquires an exclusive file lock (`Database::create()`). A second process attempting to open the same file gets `DatabaseAlreadyOpen`. Confirmed by existing test in `crates/unimatrix-store/src/db.rs`.
- *ReadOnlyDatabase for reads, queue writes* -- Impossible. `ReadOnlyDatabase` cannot coexist with `Database` on the same file. The MCP server's exclusive lock blocks both read-write and read-only opens from other processes.
- *Staging area with merge* -- Partially feasible but the read path still requires IPC. Net effect: more complex than pure IPC for no benefit.
- *HTTP on localhost* -- ~1-5ms TCP handshake overhead versus ~0.1ms UDS connect. Unnecessary for same-machine communication.

**Evidence:** RQ-2 (access-pattern.md Section 3) confirmed the redb exclusive lock via direct code analysis. RQ-4 (transport-security.md Section 2) validated UDS latency at ~12-36ms round-trip for search operations, well within the 50ms budget.

**Trade-offs:** IPC introduces server dependency -- if the MCP server is not running, the implant cannot read or write. This is acceptable because: (a) the MCP server is already running during hook events (Claude Code starts it), and (b) graceful degradation handles the server-unavailable case.

**Confidence:** HIGH -- based on direct redb code analysis and existing test confirmation.

### D2: Bundled Subcommand

**Decision:** The cortical implant ships as `unimatrix-server hook <EVENT>`, a subcommand of the existing MCP server binary. Not a separate binary.

**Alternatives considered:**
- *Separate `unimatrix-hook` binary* -- Requires separate distribution, version sync, and binary size overhead. If it links the full engine (~31MB + 87MB model), it doubles the installation footprint. If it's IPC-only (~3-5MB), there's no reason it can't be part of the server binary.
- *npm-distributed standalone package* -- Good for external adoption (M9) but premature for M5. Creates version drift risk.
- *Shell script router* -- Fragile, not cross-platform, hard to test.

**Evidence:** RQ-5 (distribution.md) scored bundled subcommand 25/25 across installation friction, update friction, platform coverage, binary size, and team consistency. All other options scored 12-23. The key insight: the IPC client code adds ~50-100KB to the server binary. The marginal cost is negligible.

**Trade-offs:** Couples implant release cadence to server releases. Cannot upgrade the implant without upgrading the server. This is actually a feature -- version mismatch is the #1 distribution risk, and same-binary eliminates it entirely.

**Confidence:** HIGH -- bundled subcommand is strictly dominant across all evaluation criteria.

### D3: Two-Tier Data Model

**Decision:** Knowledge and telemetry live in the same redb file but in separate table namespaces. Knowledge tier (14 existing tables) is unchanged. Telemetry tier (3 new tables: SESSIONS, INJECTION_LOG, SIGNAL_QUEUE) has its own lifecycle.

**Alternatives considered:**
- *Entries-only extension (everything is an EntryRecord)* -- Rejected. Causes embedding space pollution (every injection event gets a 384d vector), volume explosion (telemetry outnumbers knowledge 100:1 within a week), search result pollution, and confidence contamination.
- *Sidecar file per session (SQLite or temp file)* -- Partial. Good for compaction defense state but insufficient for telemetry that must survive across sessions for retrospective analysis. Data loss on crash is a real risk.
- *Separate redb file for telemetry* -- Possible but adds database lifecycle management complexity. Same-file with table isolation is simpler.

**Evidence:** RQ-1 (data-model.md Section 3) evaluated three candidate models against write volume, search quality, migration complexity, and alignment with the "it starts with the data model" principle. The parallel table tier preserves search quality (zero HNSW impact) while accommodating ~5-6MB steady-state telemetry (after 30-day GC).

**Trade-offs:** 17 tables is more to manage than 14. Telemetry queries need separate code paths. The signal queue introduces a new async pattern (accumulate-then-apply). These are manageable costs for complete isolation.

**Confidence:** HIGH -- the entries-only model's failure modes (embedding pollution, search degradation) are catastrophic and well-understood.

### D4: Server-Side Session State

**Decision:** The MCP server maintains in-memory session state: a map of `session_id -> SessionState` containing injection history, active context, and a pre-computed compaction payload. The hook process is ephemeral and stateless.

**Alternatives considered:**
- *Hook process maintains state in sidecar file* -- Works but adds file I/O on every hook invocation (~1ms write, ~2ms read). The hook process must discover and open the file on cold start.
- *Daemon architecture (long-running hook process)* -- Best latency (sub-microsecond memory access) but high implementation complexity. Inverts the ownership model. Deferred to Phase 2.
- *Database-persisted session state* -- Adds redb write on every injection for state that has zero value after the session ends. Wrong tier.

**Evidence:** RQ-2 (access-pattern.md Section 6) showed that server-side session state is the natural fit: the server already holds the database and the ONNX runtime. The hook process registers injections via fire-and-forget IPC, and the server accumulates them. On PreCompact, the server already has the injection history -- no additional query needed.

**Trade-offs:** If the MCP server restarts mid-session, session state is lost. The compaction defense falls back to a briefing-based query (Strategy 1) or a cached payload on disk. This is acceptable because server restarts are infrequent during active sessions.

**Confidence:** HIGH for the architecture. MEDIUM for the specific data structure -- the exact fields of `SessionState` will be refined during col-006/col-008 implementation.

### D5: Synchronous Transport Trait

**Decision:** The `Transport` trait has a synchronous public interface (`fn request(&self, req, timeout) -> Result<Response, TransportError>`). Implementations may use async internals, but callers are sync.

**Alternatives considered:**
- *Async trait (async fn)* -- Requires tokio runtime in the hook process. Tokio cold start is ~1-3ms -- acceptable but unnecessary for a single UDS round-trip.
- *Callback-based* -- More complex API for no benefit in the ephemeral process model.

**Evidence:** RQ-4 (transport-security.md Section 1) analyzed the constraint: hook processes live for <100ms, execute one request, and exit. `std::os::unix::net::UnixStream` with `SO_RCVTIMEO`/`SO_SNDTIMEO` provides timeout behavior without async. The sync trait with async-capable internals is the pragmatic choice.

**Trade-offs:** If the implant evolves into a daemon, the sync public interface may bottleneck. The daemon can wrap the trait in its own async event loop. The trait does not prevent async usage -- it avoids forcing it on ephemeral callers.

**Confidence:** HIGH -- sync is strictly simpler for the Phase 1 ephemeral model. The trait can be extended (not replaced) for Phase 2 daemon.

### D6: Layered Local Authentication

**Decision:** Three-layer zero-ceremony authentication for local UDS connections: (1) filesystem permissions (socket mode 0o600), (2) kernel peer credentials (SO_PEERCRED/getpeereid), (3) process lineage verification (/proc/{pid}/cmdline). Shared secret file as fallback.

**Alternatives considered:**
- *Token-based auth* -- Overhead for local same-user IPC. Adds configuration burden.
- *No authentication (filesystem permissions only)* -- Layer 1 alone is sufficient for most threat models. Layers 2 and 3 defend against same-user compromise scenarios.
- *Challenge-response on connect* -- Proposed in RQ-4 (M3.3) for MITM defense. Adds latency (~1ms). Recommended as optional hardening, not default.

**Evidence:** RQ-4 (transport-security.md Sections 4 and 6) designed the layered model. All three layers are zero-ceremony -- no tokens, passwords, or configuration. The bundled subcommand simplifies this further: the hook process IS the server binary running a different code path. Process lineage verification via `/proc/{pid}/cmdline` can recognize "unimatrix-server" in the hook process's command line.

**Trade-offs:** macOS `getpeereid()` does not return PID, so Layer 3 (process lineage) is unavailable on macOS via peer credentials. The shared secret fallback covers this gap. Layer 1 (filesystem permissions) provides the primary defense on all platforms.

**Confidence:** HIGH for the design. MEDIUM for macOS coverage (no PID from getpeereid requires fallback path).

### D7: Zero-Regression Degradation Guarantee

**Decision:** The implant can only add value, never subtract it. If any component fails (server unavailable, timeout, auth failure), the hook produces no output and the agent operates exactly as it did before the implant existed.

**Alternatives considered:**
- *Error injection (print errors to stdout)* -- Dangerous. Stdout content is injected into the agent's context. Errors would confuse agents.
- *Blocking retry* -- Hook processes cannot afford retry loops within the 50ms budget. Natural retry occurs on the next hook event.

**Evidence:** RQ-4 (transport-security.md Section 7) defined the degradation matrix. Synchronous hooks (UserPromptSubmit, PreCompact) skip silently on failure. Fire-and-forget hooks queue to a local WAL for replay. The compaction defense cache provides partial protection when the server is unavailable. RQ-3 (impact-assessment.md) confirmed that no existing features are degraded by the implant's presence or absence.

**Trade-offs:** Silent failure means agents don't know they're missing enrichment. This is deliberate -- an agent that doesn't know about the implant cannot react to its absence, which is the correct behavior.

**Confidence:** HIGH -- this is a design principle, not a technical hypothesis.

### D8: Pre-Computed Compaction Payload

**Decision:** The server maintains a rolling compaction payload in session state, updated after every injection. When PreCompact fires, the payload is served from memory -- no database query needed.

**Alternatives considered:**
- *Query briefing on demand* -- context_briefing takes ~15-20ms (embed + HNSW + fetch + format). Feasible within 50ms budget but leaves no margin. Also, briefing is not session-aware -- it doesn't know what was injected.
- *Replay injection history only* -- Fetches entries by ID (fast, <1ms each). Good primary strategy, but requires an IPC round-trip to the server for the fetch.
- *Full pre-compute in hook process memory* -- Requires daemon architecture (hook process is ephemeral, no persistent memory).

**Evidence:** RQ-2 (access-pattern.md Section 6) evaluated three strategies and recommended a hybrid: Strategy 2 (injection history replay via ID-based fetch) as primary, with briefing as fallback. RQ-1 (data-model.md Section 9) designed the CompactionState struct for daemon memory. The synthesis merges these: the server holds the compaction state and pre-computes the payload, serving it as a simple memory read on PreCompact.

**Trade-offs:** Server memory usage increases by ~1-2KB per active session. Negligible. The main risk is session state loss on server restart -- mitigated by the disk-based compaction cache and briefing fallback.

**Confidence:** HIGH for the strategy. MEDIUM for the specific token budget allocation (400 decisions + 200 session context + 600 high-confidence + 400 conventions + 100 buffer = 1700 out of 2000 tokens) -- will need tuning during implementation.

---

## 4. Data Architecture

### Storage Tiers

| Tier | Storage | Contents | Lifecycle | Gets Embedded? |
|------|---------|----------|-----------|----------------|
| **Knowledge** | redb (14 existing tables) | Entries, indexes, vectors, confidence, agents, audit | Permanent (standard entry lifecycle) | Yes (384d HNSW) |
| **Telemetry** | redb (3 new tables) | Sessions, injection logs, signal queue | Time-bounded (30-day GC) | Never |
| **Session State** | Server process memory | Compaction defense, injection cache, active context | Session lifetime | Never |

### Schema v4 Changes

**New tables (3):**

| # | Table | Key | Value | Purpose |
|---|-------|-----|-------|---------|
| 15 | SESSIONS | `&str` (session_id) | bincode SessionRecord | Session lifecycle tracking |
| 16 | INJECTION_LOG | `(u64, u64)` (timestamp, sequence) | bincode InjectionRecord | Per-injection event records |
| 17 | SIGNAL_QUEUE | `u64` (monotonic_id) | bincode SignalRecord | Pending confidence signals |

**EntryRecord evolution (1 new field):**
- `session_id: String` (appended, `#[serde(default)]`, empty for pre-implant entries)

**New records:** SessionRecord (14 fields), InjectionRecord (6 fields), SignalRecord (5 fields)

**Migration:** v3->v4 scan-and-rewrite of all entries to add `session_id` field. New tables created in `Store::open()`. Backward compatible -- MCP tools never touch telemetry tables.

### Data Flow: Hook Event to Storage

```
Hook Event
    │
    v
Hook Process (ephemeral)
    │
    │ IPC via UDS
    v
Server UDS Handler
    │
    ├── Synchronous queries ──────────> Knowledge Tier (READ)
    │   (ContextSearch, Briefing)        ENTRIES, VECTOR_MAP, indexes
    │                                    CO_ACCESS, AGENT_REGISTRY
    │
    ├── Fire-and-forget writes ───────> Telemetry Tier (WRITE)
    │   (RecordEvent, SessionRegister)   SESSIONS, INJECTION_LOG
    │
    ├── Session state updates ────────> Server Memory (UPDATE)
    │   (injection tracking)             SessionState map
    │
    └── Signal generation ────────────> Telemetry Tier (WRITE)
        (session end, rework detect)     SIGNAL_QUEUE
                                              │
                                         [batch apply]
                                              │
                                              v
                                         Knowledge Tier (WRITE)
                                         EntryRecord.helpful_count
                                         EntryRecord.unhelpful_count
```

### Lifecycle: Event to Knowledge

1. **Raw event** (PostToolUse, UserPromptSubmit) -- recorded in INJECTION_LOG or processed inline. Ephemeral telemetry.
2. **Session accumulation** -- events grouped by session_id. Session state updated in server memory.
3. **Signal generation** (SessionEnd) -- asymmetric signals written to SIGNAL_QUEUE based on session outcome:
   - Successful session → Helpful signals for all injected entries (auto-applied to confidence pipeline)
   - Rework session → Flagged signals for injected entries (routed to retrospective pipeline for human review, NOT auto-applied as unhelpful)
   - Abandoned session → no signals (inconclusive)
4. **Signal routing** (two consumers):
   - Confidence pipeline (crt-002): consumes Helpful signals only → increments EntryRecord.helpful_count → confidence recomputed. Only explicit MCP votes can increment unhelpful_count.
   - Retrospective pipeline (col-002): consumes Flagged signals → surfaces in RetrospectiveReport.entries_analysis as "correlated with rework" for human review, alongside hotspot findings and baseline comparisons.
5. **Retrospective analysis** (context_retrospective or periodic) -- expanded report now includes entry-level analysis: which entries were injected, which sessions they appeared in, correlation with outcomes, agent-specific effectiveness. All 21 detection rules + baseline comparison + entry analysis = comprehensive feature-level intelligence.
6. **Session summary** (agent-authored, NOT auto-generated) -- agents create outcome entries via `context_store(category: "outcome")` for meaningful session knowledge. Auto-promotion from telemetry to knowledge is NOT done — this prevents flooding the knowledge tier with low-quality templated entries that pollute search results.
6. **GC** (maintain=true) -- sessions older than 30 days are deleted along with their INJECTION_LOG records. Knowledge entries created from summaries persist independently.

---

## 5. Hook Event Flow

### UserPromptSubmit (Context Injection)

**Purpose:** Inject relevant knowledge into every agent prompt automatically.

```
Claude Code fires UserPromptSubmit hook
  → spawns: unimatrix-server hook UserPromptSubmit
  → stdin: {"session_id":"abc", "cwd":"/project", "hook_event_name":"UserPromptSubmit", "prompt":"..."}
  → hook process:
      1. Parse stdin JSON, extract prompt text and session context
      2. Discover instance: compute project hash from cwd, find unimatrix.sock
      3. Connect UDS, send ContextSearch{query: prompt, role, feature, k: 5, max_tokens: 500}
      4. Server: embed prompt (ONNX, ~3ms hot), HNSW search, confidence re-rank, co-access boost
      5. Server returns Response::Entries{items: [...], total_tokens: 450}
      6. Hook formats entries as structured markdown, writes to stdout
      7. Hook sends fire-and-forget RecordEvent(injection, entry_ids, scores, session_id)
      8. Server writes INJECTION_LOG, updates SessionState.injection_history
      9. Server updates pre-computed compaction payload
  → stdout content injected into Claude's context window
  → total latency: ~15-35ms
```

### PreCompact (Compaction Defense)

**Purpose:** Preserve critical context when Claude Code compresses conversation history.

```
Claude Code fires PreCompact hook
  → spawns: unimatrix-server hook PreCompact
  → stdin: {"session_id":"abc", ...}
  → hook process:
      1. Parse stdin, extract session_id
      2. Connect UDS, send CompactPayload{session_id, injected_entry_ids, role, feature, token_limit: 2000}
      3. Server reads SessionState from memory:
         a. Active decisions for current feature (~400 tokens)
         b. Session context metadata (~200 tokens)
         c. Top-N injected entries by confidence (~600 tokens)
         d. Relevant conventions (~400 tokens)
         e. Correction chains (~200 tokens)
      4. If no session state (server restarted): fall back to context_briefing(role, task)
      5. Server returns Response::Briefing{content, token_count}
      6. Hook writes to stdout
  → stdout content preserved in compacted window
  → total latency: ~5-15ms (pre-computed), ~15-25ms (fallback)
```

**Fallback chain:**
1. Server-side pre-computed payload (fastest, ~5ms)
2. Server-side ID-based fetch from injection history (~10ms)
3. Server-side briefing query (~20ms)
4. Disk-based compaction cache (`compaction-cache.json`) (~2ms)
5. No output (graceful skip)

### PostToolUse (Observation)

**Purpose:** Record tool usage for retrospective pipeline and rework detection.

```
Claude Code fires PostToolUse hook
  → spawns: unimatrix-server hook PostToolUse
  → stdin: {"session_id":"abc", "tool_name":"Edit", "tool_input":{...}, "tool_response_size":1234, ...}
  → hook process:
      1. Parse stdin, extract tool event data
      2. Connect UDS, send fire-and-forget RecordEvent(tool_use, tool_name, duration, session_id)
      3. Exit immediately (do not wait for server response)
  → server writes to INJECTION_LOG or processes as observation telemetry
  → total latency: ~3-5ms (connect + send + exit)

  On server unavailable:
  → write event to local WAL file (~/.unimatrix/{hash}/event-queue/pending-{ts}.jsonl)
  → replay on next successful connection
```

### SessionStart

**Purpose:** Register a session with the server for correlation and lifecycle tracking.

```
Claude Code fires SessionStart hook (or first hook of a session)
  → spawns: unimatrix-server hook SessionStart
  → stdin: {"session_id":"abc", "cwd":"/project", ...}
  → hook process:
      1. Parse stdin, extract session_id (from Claude Code's session_id field)
      2. Connect UDS, send SessionRegister{session_id, agent_role, feature}
      3. Server creates SESSIONS record (status: Active), initializes SessionState in memory
  → total latency: ~5-8ms
```

### SessionEnd

**Purpose:** Close session, generate asymmetric confidence signals, feed retrospective pipeline.

```
Claude Code fires SessionEnd (or Stop/TaskCompleted) hook
  → spawns: unimatrix-server hook SessionEnd
  → stdin: {"session_id":"abc", ...}
  → hook process:
      1. Parse stdin, extract session_id and any outcome data
      2. Connect UDS, send SessionClose{session_id, outcome, duration_secs}
      3. Exit (fire-and-forget)
  → server:
      a. Closes SESSIONS record (status: Completed, ended_at, outcome_summary)
      b. Generates ASYMMETRIC confidence signals (auto-positive, flag-negative):
         - Session succeeded → Helpful signals for all injected entries
           (auto-applied to EntryRecord.helpful_count via confidence pipeline)
         - Rework detected → Flagged signals for injected entries
           (routed to retrospective pipeline for human review, NOT auto-downweighted)
         - Abandoned → no signals (inconclusive)
      c. Writes signals to SIGNAL_QUEUE with appropriate SignalSource discriminator
      d. Drains Helpful signals → applies to EntryRecord.helpful_count, confidence recomputed
      e. Flagged signals retained for next retrospective analysis (col-002)
      f. Clears SessionState from memory
```

**Design decision: auto-positive, flag-negative, never auto-downweight.**
Implicit signals cannot attribute negative outcomes to specific entries (guilt-by-association problem).
Only explicit MCP votes (`helpful=false`) can increment `unhelpful_count`. The retrospective pipeline
surfaces rework-correlated entries for human judgment alongside its existing 21-rule hotspot detection
and baseline comparison analysis.

### PreToolUse[Bash] (Safety Check -- Optional, Low Priority)

**Purpose:** Optional safety gate for dangerous shell commands. Advisory only.

```
Claude Code fires PreToolUse hook (matcher: "Bash")
  → spawns: unimatrix-server hook PreToolUse
  → hook process evaluates command against safety patterns
  → stdout: {"decision":"allow"} or {"decision":"block","reason":"..."}
  → low priority, not part of initial col-006 scope
```

---

## 6. Compaction Defense Architecture

### The Problem

When Claude Code compacts conversation history, earlier messages are compressed. Knowledge injected via hooks in those messages is lost. The PreCompact hook has one synchronous opportunity to re-inject critical context into the compacted window.

### Constraints

- **Latency:** <50ms total (process start + IPC + format + print)
- **Token budget:** <2000 tokens (from PRODUCT-VISION.md)
- **Content:** Must reconstruct enough context for the agent to continue working

### Server-Side Session State Model

The server maintains a `SessionState` per active session:

```rust
struct SessionState {
    session_id: String,
    role: String,
    task: String,
    feature_cycle: String,

    // Ordered injection history (most recent first)
    // (entry_id, injection_timestamp, confidence_at_injection)
    injection_history: Vec<(u64, u64, f64)>,

    // Pre-computed: active decisions for current feature
    feature_decisions: Vec<u64>,

    // Pre-computed: the compaction payload, ready to serve
    compaction_payload: Option<String>,
    compaction_payload_tokens: u32,

    // Tracking
    compaction_count: u32,
    total_injections: u32,
}
```

**Updated on every injection:** After each UserPromptSubmit, the server re-sorts injection history by confidence, recomputes token budget allocation, and materializes the compaction payload. Cost: O(N log N) sort of ~30-60 entries. Sub-millisecond.

**Memory footprint:** ~1-2KB per active session. Negligible for any realistic number of concurrent sessions.

### Pre-Computed Payload Strategy

The compaction payload is a materialized view of "what I would re-inject right now":

| Section | Token Budget | Priority | Source |
|---------|-------------|----------|--------|
| Session context | ~100 | 1 | SessionState.role, task, feature_cycle |
| Active decisions | ~600 | 2 | feature_decisions (pre-computed ADR entry IDs) |
| Re-injected entries | ~600 | 3 | Top-5 from injection_history by confidence |
| Cross-cutting conventions | ~400 | 4 | Conventions for active role (cached or queried) |
| Correction chains | ~200 | 5 | Entries corrected during this session |

**Update trigger:** After every UserPromptSubmit injection. The payload is always fresh.

### Token Budget Management

- Total budget: 2000 tokens
- Usable budget: ~1700 tokens (100 reserved for formatting/headers)
- Adaptive: on repeated compactions (compaction_count > 1), reduce re-injected entries to make room for session context (the agent needs "where am I" more than "what I already know" after multiple compactions)
- Compaction frequency as signal: if compaction_count > 3 in a session, reduce injection volume on subsequent UserPromptSubmit hooks (feedback loop to prevent injection-caused compaction)

### Fallback Chain

When the primary path (server pre-computed payload) is unavailable:

1. **Server ID-based fetch:** Send injected_entry_ids to server, server fetches and returns sorted by confidence. Requires IPC but no embedding. ~10ms.
2. **Briefing query:** Call context_briefing(role, task, feature) on the server. Requires embedding. ~20ms. Not session-aware but better than nothing.
3. **Disk cache:** Read `~/.unimatrix/{hash}/compaction-cache.json`, written on every successful injection. Stale but available without server. ~2ms.
4. **Skip:** Print nothing. Agent loses context but continues working.

### Whose Memory?

**Synthesis decision:** The server owns session state, not the implant.

RQ-1 proposed daemon memory or sidecar checkpoint. RQ-2 proposed server-side `session_id -> Vec<injected_entry_id>` map. RQ-4 proposed compaction defense cache on disk. These are compatible, not conflicting:

- **Primary state holder:** Server process memory (fastest access, natural lifecycle)
- **Disk checkpoint:** compaction-cache.json (crash recovery, server restart fallback)
- **No daemon needed:** The hook process is ephemeral; it sends injection data to the server. The server accumulates it. PreCompact reads from server memory via IPC.

This is architecturally cleaner than the daemon model because it maintains a single database owner (the MCP server) and avoids the complexity of daemon lifecycle management.

---

## 7. Security Architecture

### Trust Boundary Diagram

```
┌──────────────────────────────────────────────────┐
│  Developer Machine                                │
│                                                   │
│  ┌─────────────────┐   ┌──────────────────────┐  │
│  │  Claude Code     │   │  Other processes      │  │
│  │  (trusted host)  │   │  (untrusted)          │  │
│  │                  │   │                       │  │
│  │  spawns hooks ──>│   │  ─── x ──> blocked   │  │
│  └────────┬─────────┘   │  by socket perms      │  │
│           │              └──────────────────────┘  │
│           v                                        │
│  ┌─────────────────────────────────────────────┐  │
│  │  Trust Boundary: Filesystem (0o600/0o700)    │  │
│  │                                              │  │
│  │  ┌─────────────────┐  ┌──────────────────┐  │  │
│  │  │ Hook Process     │  │ MCP Server       │  │  │
│  │  │ (unimatrix-server│  │ (unimatrix-server│  │  │
│  │  │  hook ...)       │  │  serve)          │  │  │
│  │  │                  │  │                  │  │  │
│  │  │  Trust: Internal │  │  Trust: System   │  │  │
│  │  │  Auth: SO_PEERCRED│  │  Auth: N/A      │  │  │
│  │  │  + /proc check   │  │  (is the server) │  │  │
│  │  └───────┬──────────┘  └───────┬──────────┘  │  │
│  │          │                     │              │  │
│  │          └──── UDS ────────────┘              │  │
│  │              (unimatrix.sock)                 │  │
│  │              mode 0o600                       │  │
│  └──────────────────────────────────────────────┘  │
│                                                   │
│  ~/.unimatrix/{hash}/  (mode 0o700)               │
│    unimatrix.redb                                 │
│    unimatrix.pid                                  │
│    unimatrix.sock                                 │
│    auth.token  (fallback, mode 0o600)             │
│    vector/                                        │
└──────────────────────────────────────────────────┘
```

### How Bundled Subcommand Affects Security

The bundled subcommand simplifies the threat model in two ways:

1. **Binary identity is trivial.** The hook process is the same binary as the server. Process lineage verification (`/proc/{pid}/cmdline`) sees "unimatrix-server" in both cases. No risk of binary name collision with unknown binaries.

2. **Supply chain is unified.** One binary to audit, sign, and verify. No second distribution channel to compromise. The hook configuration in `.claude/settings.json` references the same binary path used by the MCP server configuration.

However, bundled subcommand does NOT eliminate all threat vectors:
- A compromised `.claude/settings.json` can still point hooks at a rogue binary
- A same-user process can still connect to the UDS
- Environment variable injection can still redirect the implant

### Implant Trust Level

The implant connects as `cortical-implant` (Internal trust level). It can:
- Read: query entries, search, briefing
- Write: record events, create session entries, store session outcomes
- Search: semantic search for injection

It cannot:
- Admin: modify AGENT_REGISTRY, change trust levels, quarantine entries
- Escalate: promote itself or other agents

If compromised, damage is bounded by Write capability scope. Content scanning (vnc-002), contradiction detection (crt-003), and audit logging provide detection.

---

## 8. Evolution Path

### Phase 1: Bundled Subcommand + UDS + Local redb (What We Build Now)

**Timeline:** M5, col-006 through col-011

- Hook subcommand added to `unimatrix-server` binary
- UDS listener added alongside stdio MCP transport
- 3 telemetry tables added to redb (schema v4)
- Server-side session state in process memory
- Sync Transport trait with local UDS implementation
- Layered local authentication (filesystem + SO_PEERCRED + process lineage)
- Graceful degradation with WAL and compaction cache

**What works:** Context injection, compaction defense, observation recording, session lifecycle, confidence feedback, agent routing -- all via IPC to the running server.

**What doesn't work yet:** Standalone mode (hooks when no server), remote transport, warm ONNX in hook process, npm distribution.

### Phase 2: Daemon Architecture + Warm ONNX (Future)

**Timeline:** Post-M5, when hook throughput or latency is insufficient

- Daemon process owns the database, provides both MCP (stdio) and hook (UDS) interfaces
- MCP server wrapper becomes a thin stdio-to-daemon bridge
- ONNX stays warm in the daemon for sub-3ms embedding at hook time
- Hook processes connect directly to daemon (same UDS protocol)
- Session state in daemon memory (no IPC needed for compaction payload)

**What changes:**
- Database ownership moves from MCP server to daemon
- MCP server becomes a proxy
- Latency drops from ~15-35ms to ~3-10ms for search operations

**What stays:**
- Transport trait (same interface, same wire protocol)
- Hook subcommand (still connects via UDS)
- Data model (same tables, same records)
- Security model (same authentication)

### Phase 3: Centralized Unimatrix — Dockerized Server + WASM Client (Future)

**Timeline:** M8-M9, when multi-project and team deployment matter

**Server side:**
- `unimatrix-server` runs in its own Docker container (per-org or per-team deployment)
- Owns redb, ONNX runtime, HNSW — all heavy infrastructure centralized
- Exposes HTTPS endpoint for remote cortical implant connections
- Project isolation via project-hash namespacing (same hash algorithm as local mode)
- Multi-repo: multiple projects connect to the same server instance, isolated by hash

**Client side (cortical implant):**
- Thin WASM client compiled from Rust to `wasm32-wasip2`
- Distributed via npm: `npm install -D @unimatrix/cortical`
- Single `.wasm` artifact — **eliminates the entire cross-compilation matrix** (1 file replaces 5 platform-specific native binaries)
- ~1-2 MB (transport + JSON serialization only — no ONNX, no redb, no HNSW)
- Runs via Node.js WASI support — Node.js is already a Claude Code prerequisite, so this is **not an additive runtime dependency**
- Per-repo initialization via project hash: `unimatrix init` computes hash, registers with centralized server, writes hook config

**Why WASM for Phase 3 (not Phase 1):**
1. Phase 1 uses UDS (WASI UDS support immature) → bundled subcommand is correct for local
2. Phase 3 uses HTTPS → WASI Preview 2 `wasi:http` is mature for outbound HTTPS
3. Distribution: 1 `.wasm` replaces 5 native binaries — trivial npm packaging
4. Sandboxing: WASI capabilities are explicitly granted (network + stdio only)
5. Non-additive: Claude Code requires Node.js → WASI support comes free

**What changes:**
- New Transport implementation (HTTPS via `wasi:http`)
- Client is WASM, not native binary
- Server runs in Docker container, not local process
- Authentication: OAuth 2.1 / mTLS for remote connections
- Discovery: endpoint URL from config (not local socket discovery)
- Latency budget relaxes to 100-500ms for remote
- Event queue becomes more critical (network less reliable than local socket)

**What stays:**
- Transport trait interface (same operations, same request/response types)
- Hook event schema (same JSON from Claude Code)
- Data model (same tables, same records — just centralized)
- Graceful degradation patterns (same zero-regression guarantee)
- Project-hash isolation (same algorithm, server-side instead of filesystem)

### Phase Transition Contract

The architecture is designed so each phase transition is additive:
- Phase 1 → 2: Add daemon, update database ownership. Nothing removed.
- Phase 2 → 3: Add WASM client + dockerized server. Local mode still works — teams choose local or centralized per project.
- Phase 1 → 3 directly: Also valid — daemon is an optimization, not a prerequisite. A team could go straight from bundled subcommand (local) to WASM client (centralized).

```
Phase 1: unimatrix-server hook <event>  →  UDS   →  local unimatrix-server     →  local redb
Phase 2: unimatrix-server hook <event>  →  UDS   →  daemon (warm ONNX)          →  local redb
Phase 3: @unimatrix/cortical (.wasm)    →  HTTPS →  dockerized unimatrix-server →  centralized redb
```

---

## 9. Risk Register

### Consolidated from All RQ Findings

| ID | Risk | Impact | Likelihood | Source | Mitigation | Status |
|----|------|--------|------------|--------|------------|--------|
| R1 | redb exclusive lock forces IPC for all database access | Architectural | Certain | RQ-2 | Accepted. IPC latency (~10-35ms) is within budget. This IS the architecture. | **Resolved by architecture** |
| R2 | ONNX cold start (~200ms) blocks standalone semantic search | High | High | RQ-2, RQ-4 | Server-side embedding. Hook process never loads ONNX. Standalone mode uses index-based lookup only. | **Resolved by architecture** |
| R3 | Hook injection inflates access_count, distorting confidence | High | High | RQ-3 | Session-scoped dedup: max 1 access count per entry per session. Log-transform caps at ~50. | **Deferred to col-009** |
| R4 | Bulk implicit helpful votes dilute Wilson score quality | Medium | Medium | RQ-3 | Wilson 5-vote minimum guard. Entries need 5+ sessions before deviation from neutral prior. | **Deferred to col-009** |
| R5 | JSONL path and structured events produce different MetricVectors | Medium | Medium | RQ-3 | Cross-path equivalence tests during migration. Both paths produce ObservationRecord. | **Deferred to col-010** |
| R6 | Compaction defense latency exceeds 50ms on fallback | High | Low | RQ-2, RQ-3 | Pre-computed payload is primary (5-15ms). Briefing fallback only on server restart. Disk cache as final fallback. | **Mitigated by architecture** |
| R7 | Session identity unreliable without Claude Code session_id | Medium | Low | RQ-1, RQ-2 | Claude Code provides `session_id` in hook stdin JSON (confirmed in RQ-4 Section 5). Use parent PID as fallback. | **Resolved by evidence** |
| R8 | Socket unavailable on Windows | Medium | Low | RQ-4 | AF_UNIX available on Windows 10 1803+. Named pipe as fallback. Windows is P2 priority. | **Deferred to Windows support** |
| R9 | Server restart loses session state | Medium | Low | RQ-1, RQ-2 | Disk-based compaction cache. Briefing fallback. WAL for queued events. Session state is ephemeral by design. | **Mitigated by architecture** |
| R10 | Supply chain compromise of binary | Critical | Low | RQ-4 | SHA-256 checksums, signed releases, reproducible builds. Single binary simplifies auditing. | **Needs prototyping (M9)** |
| R11 | Co-access pairs from injections overwhelm MCP pairs | Low | Low | RQ-3 | Session dedup. Log-transform saturation at count=20. 30-day staleness cleanup. | **Deferred to col-007** |
| R12 | Schema v3->v4 migration at scale | Low | Low | RQ-1 | Established migration pattern (3 prior migrations). ~100ms for current ~170 entries. | **Accepted** |
| R13 | Signal queue unbounded growth | Low | Low | RQ-1 | Cap at 10,000 entries. Drop oldest on overflow. Consumer runs on session end + periodic. | **Design constraint** |
| R14 | Concurrent UDS connections during swarm runs | Low | Medium | RQ-4 | Tokio-based server handles concurrent connections via task spawning. One request/response per connection. | **Resolved by architecture** |
| R15 | Compaction frequency increases due to injection volume | Medium | Medium | SCOPE Q9 | Adaptive injection volume: if compaction_count > 3, reduce entries per injection. Feed compaction frequency to retrospective. | **Deferred to col-008** |
| R16 | macOS lacks PID in peer credentials | Low | Certain (macOS) | RQ-4 | Shared secret fallback. Filesystem permissions (Layer 1) sufficient for most threat models. | **Accepted** |

---

## 10. Open Questions Resolved

### From SCOPE.md

**Q1: redb concurrent access model**
**Status: RESOLVED** by RQ-2.
redb v3.1.x acquires an exclusive file lock. No concurrent access across processes. The implant MUST use IPC. This is confirmed by the existing test `test_open_already_open_returns_database_error()`.

**Q2: Hook process lifetime**
**Status: RESOLVED** by architecture decision.
Phase 1: ephemeral process per hook event. Cold start is ~3ms (compiled binary, no ONNX). IPC round-trip fits within 50ms budget. Phase 2 (future): daemon architecture for warm ONNX and sub-millisecond latency.

**Q3: Embedding at hook time**
**Status: RESOLVED** by architecture decision.
The hook process never loads ONNX. Embedding is server-side. The MCP server has a warm ONNX runtime (~3ms per embed). For standalone mode (no server), skip semantic search entirely -- use index-based lookups only.

**Q4: JSONL telemetry transition**
**Status: PARTIALLY RESOLVED** by RQ-3.
Migration path: Phase 1 (col-006) adds structured event ingestion alongside JSONL. Phase 2 (col-010) provides explicit session boundaries. Phase 3 (stabilization): JSONL deprecated. Detection rules and metric computation are source-agnostic. The transition is incremental, not a big-bang replacement.

**Q5: npm binary packaging precedent**
**Status: RESOLVED** by RQ-5.
esbuild, Turborepo, Biome, SWC, and Sentry CLI all use the optionalDependencies + postinstall pattern. Well-documented by Sentry's engineering blog. Deferred to M9 (nan-001/004) -- not needed for Phase 1 (bundled subcommand).

**Q6: Claude Code hook event schema stability**
**Status: PARTIALLY RESOLVED** by RQ-4.
Hooks receive rich JSON via stdin including `session_id`, `cwd`, `transcript_path`, `hook_event_name`. Schema appears stable but Anthropic has not made explicit stability guarantees. Mitigation: parse JSON loosely with `#[serde(default)]`, monitor Claude Code release notes.

**Q7: Session identity**
**Status: RESOLVED** by RQ-4.
Claude Code provides `session_id` in hook stdin JSON. This was confirmed in the hook environment context analysis (RQ-4 Section 5). Parent PID available as fallback via process tree inspection.

**Q8: Compaction defense depth**
**Status: RESOLVED** by synthesis.
Knowledge-level re-injection (entries, decisions, conventions) is sufficient for the common case. The 2000-token budget accommodates active decisions + session context + top injected entries + conventions. For complex mid-task state (partial implementation, debugging context), the implant cannot reconstruct working state -- that lives in the agent's context window and is partly preserved by Claude Code's own compaction. The implant supplements, not replaces, Claude Code's native compaction.

**Q9: Compaction frequency as signal**
**Status: PARTIALLY RESOLVED.**
Architecture supports compaction_count tracking in SessionState. Adaptive injection volume (reduce entries when compaction_count > 3) is a design constraint for col-008. Feeding compaction frequency to the retrospective pipeline (col-002) requires a new metric in MetricVector -- deferred to col-010.

---

## 11. Success Criteria Evaluation

### From SCOPE.md

| Criterion | Met? | Evidence |
|-----------|------|---------|
| Data model covers both access paths | **YES** | Two-tier model (knowledge + telemetry) supports MCP and hook paths with strict isolation. Sessions, injections, events modeled. Compaction defense state modeled (server memory + disk cache). Clear durable/ephemeral boundary. |
| Transport abstraction supports local and remote | **YES** | Sync Transport trait defined in Rust pseudocode (RQ-4). Local variant designed with UDS + length-prefixed JSON. Remote variant sketched with HTTPS + JSON. Latency estimates: 12-36ms local search, 16-46ms local briefing -- within 50ms target. |
| Existing feature impact quantified | **YES** | Seven features assessed (RQ-3): col-002, col-002b, crt-001, crt-002, crt-004, col-001, vnc-003. All rework items classified as incremental (none blocking col-006). ~400 LOC of dead/removable code identified. Cross-feature interaction graph documented. |
| Security model extends to implant | **YES** | Threat model documents 6 attack vectors (RQ-4). Layered local auth defined. AuthContext interface unifies local and remote. Graceful degradation matrix covers 5 failure scenarios. Zero-regression guarantee. |
| Distribution mechanism identified | **YES** | Bundled subcommand (primary, score 25/25). npm with native binaries (secondary, M9). GitHub Releases + cargo-binstall (tertiary). Platform matrix: 5 targets (3 P0, 1 P1, 1 P2). |
| col-006-011 can be scoped against architecture | **YES** | See D14-7 (feature-scoping.md). Each feature has revised description, specific components, dependencies, complexity estimate, and key risks derived from architectural decisions. |

### Gaps

- **Prototype validation**: No actual IPC prototype was built (SCOPE constraint: no production code). Latency estimates are analytical, not measured. col-006 implementation should prototype early and validate the 50ms budget.
- **Windows support**: Transport trait design accounts for Windows but no implementation sketch was produced. Windows is P2 priority.
- **Daemon architecture**: Phase 2 daemon is designed at high level but transitions (database ownership inversion, MCP proxy) need detailed design when the time comes.
