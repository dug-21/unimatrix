# D14-2: Access Pattern Architecture

**Deliverable:** ASS-014 RQ-2 — Two-Door Access Pattern
**Date:** 2026-03-01
**Status:** Research Complete

---

## 1. Operation Inventory (RQ-2a)

### MCP Server Path (Existing)

All MCP tools execute through `UnimatrixServer` (rmcp handler) over stdio transport. Each follows the execution order: identity -> capability -> validation -> category -> scanning -> business logic -> format -> audit.

| Tool | Operation | Read/Write/Mixed |
|------|-----------|-----------------|
| `context_search` | Embed query, HNSW search, fetch entries, confidence re-rank, co-access boost, record usage | Mixed (read-heavy, write: usage recording, co-access pairs) |
| `context_lookup` | Index scan (topic/category/tag/status), fetch entries, record usage | Mixed (read-heavy, write: usage recording) |
| `context_get` | Fetch single entry, record usage | Mixed (read-heavy, write: usage recording) |
| `context_store` | Embed content, duplicate check, insert entry + all indexes + vector + audit in single txn | Write |
| `context_correct` | Fetch original, deprecate, create correction, embed, insert, audit — single txn | Write |
| `context_deprecate` | Update status, update audit | Write |
| `context_quarantine` | Update status, update audit | Write |
| `context_status` | Full table scans (entries, co-access, vectors), optional maintenance (confidence refresh, graph compaction) | Mixed (read default, write when maintain=true) |
| `context_briefing` | Lookup duties + conventions by role, embed task, semantic search, co-access boost, record usage | Mixed (read-heavy, write: usage recording) |
| `context_enroll` | Registry read/write, audit | Write |
| `context_retrospective` | Parse JSONL, compute metrics, store MetricVector, compare baselines | Write |

**Write hotspots:** `context_store`, `context_correct`, and the fire-and-forget usage recording on every retrieval tool.

### Cortical Implant Path (Proposed)

| Operation | Hook Source | Read/Write | Latency Class |
|-----------|-----------|------------|---------------|
| **Prompt-scoped search** | UserPromptSubmit | Read | Synchronous, <50ms target |
| **Session-aware briefing** | UserPromptSubmit (first), PreCompact | Read | Synchronous, <50ms target |
| **Category-filtered match** | UserPromptSubmit (agent routing) | Read | Synchronous, <50ms target |
| **Compaction payload** | PreCompact | Read | **Critical** — synchronous, <50ms hard limit |
| **Injection recording** | UserPromptSubmit (after injection) | Write | Fire-and-forget |
| **Event recording** | PostToolUse, SubagentStart/Stop | Write | Fire-and-forget |
| **Session start** | SessionStart | Write | Fire-and-forget |
| **Session end** | SessionEnd, TaskCompleted | Write | Fire-and-forget |
| **Confidence signal** | PostToolUse, TaskCompleted | Write | Fire-and-forget |
| **Observation telemetry** | All hooks | Write | Fire-and-forget |

**Critical distinction:** The implant has TWO latency classes:
1. **Synchronous reads** — must return content to stdout before Claude Code proceeds. The PreCompact hook is the hardest deadline.
2. **Fire-and-forget writes** — record events, queue for later processing, no response needed.

### Operation Overlap Analysis

| Capability | MCP | Implant | Shared? |
|-----------|-----|---------|---------|
| Semantic search (embed + HNSW) | Yes | Yes | Yes — same HNSW index, same embeddings |
| Entry fetch by ID | Yes | Yes | Yes — same ENTRIES table |
| Index scan (topic/category/tag) | Yes | Yes | Yes — same index tables |
| Entry insert + indexing | Yes | No (queues writes) | Server only |
| Confidence re-ranking | Yes | Yes | Yes — same formula |
| Co-access boost | Yes | Yes | Yes — same CO_ACCESS table |
| Usage recording | Yes | Implant records injections | Similar but different signal source |
| Audit logging | Yes | No (own telemetry) | Separate |
| Embedding generation | Yes | Questionable (ONNX cold start) | Shared if daemon; skipped if ephemeral |

---

## 2. Shared Code Architecture (RQ-2b)

### Current Crate Dependency Graph

```
unimatrix-embed  (ONNX pipeline, EmbeddingProvider trait)
       │
       ▼
unimatrix-store  (redb Store, EntryRecord, schema, tables)
       │
       ▼
unimatrix-vector (HNSW index, persistence, VectorIndex)
       │
       ▼
unimatrix-core   (traits: EntryStore, VectorStore, EmbedService)
       │          (adapters: StoreAdapter, VectorAdapter, EmbedAdapter)
       │          (async wrappers: AsyncEntryStore, AsyncVectorStore)
       │
       ▼
unimatrix-server (MCP handler, tools, confidence, scanning, etc.)
```

### Logic Currently in unimatrix-server That Would Need Sharing

The following modules in `unimatrix-server` contain business logic that the implant path also needs:

| Module | What It Does | Implant Needs It? |
|--------|-------------|-------------------|
| `confidence.rs` | Confidence formula (compute_confidence, rerank_score, co_access_affinity) | **Yes** — for search re-ranking |
| `coaccess.rs` | Co-access pair generation, boost computation | **Yes** — for search/briefing boost |
| `scanning.rs` | Content scanning (injection + PII) | No — implant doesn't accept writes |
| `response.rs` | Format entries for MCP output | **Partial** — implant needs its own formatting for stdout injection |
| `categories.rs` | Category allowlist | No — implant doesn't validate categories |
| `validation.rs` | Input validation | No — implant has its own validation |
| `audit.rs` | Audit logging | No — implant has own telemetry |
| `registry.rs` | Agent registry, trust levels | Maybe — implant needs to read agent trust for routing |
| `identity.rs` | Agent identification | Maybe — simplified |
| `embed_handle.rs` | Lazy ONNX loading state machine | Maybe — if implant links embed directly |
| `project.rs` | Project root detection, hash, data paths | **Yes** — implant needs to find the database |

### Proposed Crate Factoring

```
unimatrix-embed     (unchanged — ONNX pipeline)
unimatrix-store     (unchanged — redb Store)
unimatrix-vector    (unchanged — HNSW index)
unimatrix-core      (unchanged — traits, adapters)

unimatrix-engine    (NEW — shared business logic)
   ├── confidence.rs      (from server)
   ├── coaccess.rs         (from server)
   ├── project.rs          (from server)
   ├── registry_reader.rs  (read-only agent registry access)
   ├── search.rs           (embed + HNSW + re-rank + co-access boost pipeline)
   ├── briefing.rs         (briefing query logic, token budgeting)
   └── query.rs            (index-based lookup with filtering)

unimatrix-server    (MCP handler — uses unimatrix-engine)
   ├── tools.rs            (MCP tool implementations, validation, audit)
   ├── scanning.rs         (injection detection — server only)
   ├── audit.rs            (audit logging — server only)
   └── ...

unimatrix-hook      (NEW — cortical implant binary)
   ├── router.rs           (hook event dispatch)
   ├── inject.rs           (stdout injection formatting)
   ├── session.rs          (session state tracking)
   ├── queue.rs            (fire-and-forget write queue)
   └── main.rs             (CLI, daemon, hook entry point)
```

**The `unimatrix-engine` crate is the key architectural addition.** It extracts the reusable query pipeline (search with re-ranking, briefing with token budgeting, co-access boosting) from the server into a library that both the MCP server and the cortical implant can link. The server adds MCP-specific concerns (validation, scanning, audit). The implant adds hook-specific concerns (routing, injection formatting, session tracking).

### Trait Reuse Assessment

Both paths can use the same `EntryStore`, `VectorStore`, and `EmbedService` traits from `unimatrix-core`. The traits are object-safe, `Send + Sync`, and designed for this exact scenario. The `StoreAdapter` and `VectorAdapter` bridge concrete types to the trait interface.

The `async_wrappers` module is server-specific (tokio dependency). The implant, if synchronous (ephemeral process), would call the synchronous trait methods directly. If a daemon, it would use its own async wrappers or reuse the existing ones.

---

## 3. Concurrency Analysis (RQ-2c)

### redb v3.1.x Concurrent Access Model

Based on research of redb documentation and source code:

**Within a single process:**
- Multiple concurrent read transactions via MVCC (each sees a snapshot)
- Single write transaction at a time (write lock via `Mutex<()>`)
- Readers and writers do NOT block each other (copy-on-write B-trees)
- Read transactions see a consistent snapshot from when `begin_read()` was called

**Across processes — the critical constraint:**
- `Database::create()` / `Database::open()` acquires an **exclusive file lock**
- Only ONE process can hold a `Database` (read-write) handle to a given file
- Attempting to open the same file from a second process returns `DatabaseError::DatabaseAlreadyOpen`
- This is confirmed by the existing test in `crates/unimatrix-store/src/db.rs`:

```rust
#[test]
fn test_open_already_open_returns_database_error() {
    let _store1 = Store::open(&path).unwrap();
    let result = Store::open(&path);
    match result {
        Err(StoreError::Database(redb::DatabaseError::DatabaseAlreadyOpen)) => {}
        // ...
    }
}
```

**redb 3.0.0 introduced `ReadOnlyDatabase`:**
- Multiple `ReadOnlyDatabase` instances CAN open the same file concurrently (shared lock)
- `ReadOnlyDatabase` **cannot coexist with a `Database`** on the same file
- Only `begin_read()` is available — no write transactions
- Uses platform file locking (shared lock via `flock(LOCK_SH)`)

**Implication:** The implant CANNOT open the database with `Database::create()` while the MCP server holds it. The implant CAN open a `ReadOnlyDatabase` — but only if the MCP server does NOT hold the file (since `Database` takes an exclusive lock).

### Comparison with SQLite WAL Mode

SQLite in WAL mode allows:
- Unlimited concurrent readers + one writer, across processes
- Writers don't block readers (readers see snapshot before write)
- All via file-level locking with shared memory for coordination
- Multiple processes can use the same database file simultaneously

redb is more restrictive: only one `Database` handle per file, period. The `ReadOnlyDatabase` type only works when no `Database` exists. This is a fundamental architectural constraint for the two-door pattern.

### Evaluation of Three Options

#### Option 1: Implant Read-Only + Write Queue to Server

The implant opens redb only for reads and queues all writes for the MCP server.

**Problem:** redb does not allow `ReadOnlyDatabase` concurrent with `Database`. The MCP server holds an exclusive lock via `Database::create()`. The implant cannot open `ReadOnlyDatabase` while the server is running.

**Verdict: Not feasible with redb's locking model.** The exclusive file lock held by `Database` blocks both `Database::create()` and `ReadOnlyDatabase::open()` from other processes.

#### Option 2: Both Open redb With Locking

Both the MCP server and implant open their own `Database` handle.

**Problem:** redb returns `DatabaseAlreadyOpen` when a second process tries to open the same file. There is no "shared write" mode. The file lock is exclusive.

**Verdict: Impossible.** redb is a single-process database.

#### Option 3: Staging Area + Merge

The implant writes to a separate staging file. The server periodically merges.

**Problem:** The implant still needs to READ the main database for search and briefing. But it can't open it while the server holds it.

**Feasibility only if:** The implant doesn't read the main database directly — instead querying the server via IPC. The staging area is for write events only.

**Verdict: Partially feasible, but the read path still requires IPC to the server.**

### The Real Constraint

redb's single-process ownership model means the cortical implant **cannot directly open the database file** while the MCP server is running. This eliminates all "direct linking for reads" options. The implant must communicate with the server process for ALL database access — reads and writes.

This is architecturally significant. It means the access pattern decision is NOT between "direct vs IPC" — it's between "IPC to running server" vs "implant IS the server" vs "implant takes ownership when server isn't running."

---

## 4. Transport Options Comparison (RQ-2d)

Given the redb constraint above, the options reshape:

### Option A: Implant Links Everything Directly (Two Separate Processes)

**Architecture:** Implant links `unimatrix-core`, `unimatrix-store`, `unimatrix-embed`, `unimatrix-vector`. Opens its own `Database` handle.

**Fatal flaw:** Cannot coexist with running MCP server. `DatabaseAlreadyOpen` error.

**Could work if:** The MCP server is NOT running (hooks-only mode). The implant becomes the sole database owner. MCP tools become unavailable.

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Latency | Excellent (<5ms reads) | Direct redb + HNSW access |
| Complexity | Low | Single process, no IPC |
| Reliability | Good (standalone) | Works offline, no server dependency |
| Server coexistence | **Impossible** | Mutual exclusion with MCP server |
| Binary size | Large (~30-50MB) | Includes ONNX runtime |
| Cold start | ~200-500ms | ONNX init + redb open + HNSW load |

**Verdict: Rejected for normal operation. Viable as degraded standalone mode.**

### Option B: Implant Talks to MCP Server via IPC

**Architecture:** Implant is a thin client. MCP server adds a secondary transport (Unix domain socket) alongside its existing stdio MCP transport. Implant sends structured requests over the socket and receives responses.

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Latency | Good (5-15ms local) | UDS round-trip on same machine |
| Complexity | Medium | Server needs secondary listener, implant needs client |
| Reliability | Server-dependent | Fails when server isn't running |
| Server coexistence | Perfect | Server owns database exclusively |
| Binary size | Small (~2-5MB) | No ONNX, no redb, no HNSW |
| Cold start | Fast (~1-5ms) | Just socket connect |

**Latency estimate for Unix domain socket:**
- Socket connect: ~0.1ms (persistent connection) or ~0.5ms (per-request)
- Request serialization: ~0.05ms (compact binary format)
- Server-side processing: 2-10ms (redb read + HNSW search)
- Response deserialization: ~0.05ms
- **Total: ~3-15ms** — well under 50ms target

**Protocol options:**
1. **Custom binary protocol over UDS** — minimal overhead, purpose-built
2. **JSON-RPC over UDS** — familiar, debuggable, slightly more overhead
3. **gRPC over UDS** — future-proof, but heavy dependency for local IPC
4. **HTTP over UDS** — standard, debuggable, moderate overhead

Recommendation: JSON-RPC over Unix domain socket. Matches MCP's existing JSON-RPC pattern, minimal learning curve, good debugging story.

### Option C: Hybrid — Implant Opens redb for Reads, Queues Writes for Server

**Ruled out by redb's locking model.** `ReadOnlyDatabase` cannot coexist with `Database` on the same file. See Section 3.

### Option D: Implant as Subcommand of Unified Binary

**Architecture:** Single `unimatrix` binary with subcommands: `unimatrix serve` (MCP server), `unimatrix hook <event>` (hook handler). The hook subcommand connects to the running server via IPC.

This is variant of Option B but with a shared binary. Benefits:
- Single binary to distribute (solves versioning)
- Shared code at compile time
- `unimatrix hook` knows the exact IPC protocol (same codebase)

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Latency | Good (5-15ms) | Same as Option B |
| Complexity | Medium | Unified binary, IPC client built-in |
| Reliability | Server-dependent | Same as Option B |
| Server coexistence | Perfect | Server owns database |
| Binary size | Moderate (~30-50MB) | One binary includes everything |
| Cold start | Fast (~1-5ms) | Hook subcommand is thin |
| Version sync | Perfect | Same binary, always in sync |

### Option E: Daemon Implant Takes Database Ownership

**Architecture:** The cortical implant runs as a long-lived daemon that owns the database. The MCP server becomes a thin proxy that forwards tool calls to the daemon. Inverts the current architecture.

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Latency | Excellent for hooks (<5ms) | Daemon has hot database + HNSW + ONNX |
| Complexity | High | Requires rewriting MCP server as proxy |
| Reliability | Good | Daemon lifecycle management needed |
| Server coexistence | Perfect | Daemon is single owner |
| Cold start | Amortized (daemon) | First hook starts daemon, subsequent are fast |
| ONNX embedding | Available | Daemon keeps ONNX warm |

This option has the best latency profile but the highest implementation complexity. It inverts the ownership model: the daemon becomes the engine, the MCP server becomes a pass-through.

### Transport Comparison Matrix

| Criterion | A: Direct | B: IPC to Server | D: Unified Binary | E: Daemon Owner |
|-----------|-----------|------------------|-------------------|-----------------|
| Meets <50ms | Yes (<5ms) | Yes (~10ms) | Yes (~10ms) | Yes (<5ms) |
| Works with MCP server | **No** | Yes | Yes | Yes (inverted) |
| Works without MCP server | Yes | **No** | **No** | Yes |
| Binary size | Large | Small | Moderate | Large (daemon) |
| Cold start per hook | ~300ms | ~3ms | ~3ms | Amortized |
| ONNX available | Yes (cold) | Via server | Via server | Yes (hot) |
| Implementation complexity | Low | Medium | Medium | High |
| Version sync risk | High | Medium | None | Low |

---

## 5. Recommended Access Pattern

### Primary: Option D — Unified Binary with IPC (phased toward Option E)

**Phase 1 (col-006): Unified binary + IPC to running MCP server**

The cortical implant is a subcommand of the `unimatrix` binary: `unimatrix hook <event>`. When a hook fires, it:

1. Connects to the MCP server's secondary Unix domain socket listener
2. Sends a structured request (JSON-RPC) with the hook event data
3. For synchronous hooks (UserPromptSubmit, PreCompact): waits for response, prints to stdout
4. For fire-and-forget hooks (PostToolUse, SessionEnd): sends and exits immediately

The MCP server adds a UDS listener alongside its existing stdio transport:
- Listens on `~/.unimatrix/{project_hash}/unimatrix.sock`
- Accepts connections from the hook process
- Routes requests to the same internal logic (via `unimatrix-engine`)
- Returns results in a compact format

**Why this first:**
- redb's locking model forces IPC — no way around it
- Lowest implementation risk — server already has all the logic
- Unified binary solves version sync
- <50ms latency is achievable (UDS round-trip ~10ms)
- The MCP server process is already running during hook events (Claude Code starts it)

**Phase 2 (future): Daemon architecture**

Once the hook system is proven, evolve toward the daemon model (Option E):
- The daemon owns the database and provides both MCP (stdio) and hook (UDS) interfaces
- The MCP server wrapper becomes a thin stdio-to-daemon bridge
- Hook processes connect directly to the daemon
- ONNX stays warm in the daemon for embedding at hook time

This is the right end state but the wrong starting point. Phase 1 validates the hook system, the injection format, the query patterns. Phase 2 optimizes the transport.

### Fallback: Graceful Degradation When Server Isn't Running

When `unimatrix hook` cannot connect to the socket:
1. **Skip injection** — print nothing to stdout (safe, no-op)
2. **Queue events** — append to a local file (`~/.unimatrix/{hash}/hook-queue.jsonl`)
3. **Log** — write to stderr for debugging
4. The server processes the queue on next startup

This is acceptable because:
- Hooks that inject context (UserPromptSubmit, PreCompact) fail silently — agents still work, just without enrichment
- Fire-and-forget events queue for later — no data loss
- The queue file is small and bounded (rotate at 10MB)

---

## 6. PreCompact Architecture (RQ-2f)

### The Problem

The PreCompact hook fires when Claude Code is about to compress the conversation history. Content printed to stdout by the hook is injected into the compacted window, preserving it for the agent. This is the most latency-critical operation — Claude Code waits synchronously for the hook to complete.

**Constraints:**
- **Latency:** <50ms total (hook process start + query + format + print)
- **Token budget:** <2000 tokens (from PRODUCT-VISION.md)
- **Content:** Must reconstruct enough context for the agent to continue working after compaction

### What the Implant Queries

The compaction payload needs three categories of information, in priority order:

1. **Active decisions** — Architectural decisions (ADRs) relevant to the current feature/task. These are the most critical because they constrain what the agent can do.

2. **Session injection history** — Which entries were injected during this session? The agent has already seen these; re-injecting the most important ones ensures continuity. Prioritize by confidence score.

3. **Feature context** — What feature is being worked on? What phase? What gate? This is the "where am I?" information.

4. **Relevant conventions** — Coding patterns, testing conventions, workflow rules that apply to the current work.

### Three Strategies Evaluated

#### Strategy 1: Call briefing with session context

The implant calls the server's briefing-equivalent with the session's role, task, and feature parameters.

```
Query: briefing(role="developer", task="implementing col-006", feature="col-006")
```

**Pros:**
- Reuses existing briefing logic
- Semantic search finds relevant entries
- Token budgeting already implemented in briefing

**Cons:**
- Briefing returns an unordered bag of entries — not session-aware
- No memory of what was injected earlier in the session
- Requires embedding the task query (ONNX needed — via server)
- Briefing latency: embed (2-5ms in hot server) + HNSW search (1-3ms) + fetch (2-5ms) + format (1ms) = ~10-15ms server-side, plus IPC overhead (~5ms) = ~15-20ms total

**Feasibility:** Meets <50ms. Content quality depends on how well the briefing query captures session context.

#### Strategy 2: Replay injection history

The implant tracks which entries were injected during this session (via injection recording). On PreCompact, it replays the top-N entries by confidence.

```
Query: get_entries(ids=[42, 17, 93, 5, 28], sort=confidence_desc, token_limit=2000)
```

**Pros:**
- Session-aware — re-injects what the agent has already seen
- No embedding needed — just fetch by ID
- Very fast — redb read by ID is <1ms each
- Deterministic — same entries, same order

**Cons:**
- Requires session state (injection history)
- If session tracking fails, falls back to no data
- Doesn't discover NEW relevant entries added since session start

**Feasibility:** Meets <50ms easily (<5ms server-side for ID-based fetch). Requires the implant to maintain an injection history (in-memory for daemon; sidecar file for ephemeral).

#### Strategy 3: Session state snapshot

The implant maintains a pre-computed "compaction payload" that updates after every injection cycle. On PreCompact, it returns the cached payload immediately.

```
Response: cached_payload (updated on last UserPromptSubmit)
```

**Pros:**
- Fastest possible — no query at all, just print cached content
- Guaranteed <50ms (just file read or memory access)
- Payload is always fresh (updated every prompt cycle)

**Cons:**
- Requires daemon architecture (for in-memory cache) or sidecar file (for ephemeral)
- Sidecar file adds write on every prompt — but fire-and-forget, so latency is amortized
- Increases complexity of session state management

**Feasibility:** Trivially meets <50ms. Implementation complexity depends on architecture.

### Recommended PreCompact Approach: Strategy 2 + 1 Hybrid

**Primary (Phase 1):** Strategy 2 — replay injection history via ID-based fetch.

The implant sends a `compact_payload` request to the server with:
```json
{
  "method": "compact_payload",
  "params": {
    "session_id": "abc123",
    "injected_entry_ids": [42, 17, 93, 5, 28, 14, 7],
    "role": "developer",
    "feature": "col-006",
    "token_limit": 2000
  }
}
```

The server:
1. Fetches entries by ID (fast — <1ms each)
2. Sorts by confidence (descending)
3. Truncates to token budget
4. Returns formatted payload

**Fallback:** If no injection history is available (first compaction, session tracking failure), fall back to Strategy 1 — call briefing with role/task context.

**Enhancement (Phase 2):** Strategy 3 — pre-computed payload. Once the daemon architecture is in place, maintain a rolling compaction payload in memory, updated after each injection cycle. PreCompact becomes a simple memory read.

### Token Budget Allocation

Within the 2000-token budget:

| Section | Tokens | Priority | Content |
|---------|--------|----------|---------|
| Session context | ~100 | 1 | Feature, phase, role, task summary |
| Active decisions | ~600 | 2 | Top 3 ADRs by confidence |
| Re-injected entries | ~800 | 3 | Top 5 previously-injected entries |
| Conventions | ~400 | 4 | Top 3 relevant conventions |
| Buffer | ~100 | — | Formatting overhead |

### Latency Budget (Phase 1)

| Step | Time | Notes |
|------|------|-------|
| Hook process start | ~3ms | Pre-compiled binary, no ONNX |
| Socket connect | ~0.5ms | UDS to running server |
| Request serialization | ~0.1ms | JSON-RPC |
| Server processing | ~5-10ms | ID fetch + sort + format |
| Response deserialization | ~0.1ms | |
| Stdout write | ~0.1ms | |
| **Total** | **~10-15ms** | Well under 50ms |

### Embedding at PreCompact Time

**Not needed.** The recommended Strategy 2 uses ID-based fetch, not semantic search. No embedding required. This is a deliberate design choice — embedding at hook time would require either:
- A hot ONNX runtime (daemon architecture only, ~2-5ms per embed)
- Cold ONNX initialization (~200-500ms — blows the latency budget)

If Strategy 1 fallback triggers, the server has a hot ONNX runtime and can embed in ~2-5ms. The implant never needs to embed directly.

---

## 7. Failure Mode Analysis (RQ-2e)

### Failure Modes and Degradation

| Failure | Impact | Degradation Strategy |
|---------|--------|---------------------|
| MCP server not running | Socket connect fails | Skip injection, queue writes, log to stderr |
| Socket timeout (>40ms) | Risk missing 50ms deadline | Abort with partial/no injection, queue event |
| Server busy (slow response) | Latency spike | Configurable timeout (default 40ms), abort on timeout |
| Database locked (server restarting) | Server can't serve queries | Socket connect fails — same as "not running" |
| ONNX not loaded yet | Embedding-dependent queries fail | Server returns EmbedNotReady; implant uses index-only fallback |
| Session state lost | No injection history | Fall back to briefing-based compaction defense |
| Queue file corruption | Queued events lost | Truncate and continue — events are telemetry, not critical |
| Version mismatch (binary) | Protocol incompatibility | Version handshake on first connect; abort on mismatch |

### Standalone Capability Assessment

**Should the implant work standalone (without MCP server)?**

Yes, but only in a degraded read-only mode — and only for specific scenarios:

1. **CI/CD environments** — hooks fire but no MCP server. Implant should skip injection silently.
2. **Offline development** — developer working without MCP server configured. No value from hooks.
3. **Initial setup** — hooks configured before server is started. First prompt gets no injection.

For standalone read capability, the implant would need to open the database directly. This is only possible when the MCP server is NOT running (redb exclusive lock). The implant would:
1. Try socket connect (preferred)
2. If socket fails, try opening `ReadOnlyDatabase` directly
3. If that fails (server has lock), skip injection

This provides a degradation ladder:
- **Best:** Server running, full capability via IPC
- **Good:** Server not running, implant opens ReadOnlyDatabase for reads (no writes, no HNSW search without loading index, limited to index-based lookups)
- **Minimal:** Neither works, skip injection, queue events

**Important caveat for standalone reads:** Opening `ReadOnlyDatabase` provides access to redb tables but NOT the HNSW index (which is an in-memory data structure loaded from disk by `VectorIndex`). Standalone reads would be limited to:
- Index-based lookups (topic, category, tag, status)
- Direct entry fetch by ID
- NO semantic search (requires HNSW + embeddings)

This is still useful for compaction defense (Strategy 2 — fetch by ID) but not for context injection (Strategy 1 — needs semantic search).

### Event Queue Design

For fire-and-forget writes when the server is unavailable:

```
~/.unimatrix/{hash}/hook-queue.jsonl
```

Format: One JSON object per line with timestamp, event type, and payload.

```json
{"ts":1709280000,"event":"injection","ids":[42,17],"session":"abc123"}
{"ts":1709280001,"event":"tool_use","tool":"Edit","duration_ms":150,"session":"abc123"}
```

Queue behavior:
- Append-only (no locking needed — single writer per hook invocation)
- Server drains queue on startup and periodically
- Rotate at 10MB (delete oldest, keep last 5 files)
- Best-effort — loss is acceptable (telemetry, not critical data)

---

## 8. Interaction with PidGuard (vnc-004)

### Current PidGuard Assumptions

The PidGuard system (`crates/unimatrix-server/src/pidfile.rs`) assumes single-process ownership:
- `PidGuard::acquire()` takes an exclusive flock on `unimatrix.pid`
- `handle_stale_pid_file()` terminates stale unimatrix-server processes
- `is_unimatrix_process()` checks `/proc/{pid}/cmdline` for "unimatrix-server"

### Impact of Cortical Implant

The implant is a **separate binary** (or subcommand). It does NOT compete with the PidGuard:
- The implant does not open the database directly (IPC model)
- The implant does not need its own PID file (ephemeral process, or daemon with separate PID)
- The implant connects to the server via UDS — the server already owns the database

**If daemon architecture (Phase 2):**
- The daemon would need its own PidGuard (separate PID file: `unimatrix-hook.pid`)
- Or the daemon replaces the server as the sole database owner, inheriting the existing PidGuard
- `is_unimatrix_process()` would need to recognize both "unimatrix-server" and "unimatrix" binary names

### Socket File Lifecycle

The UDS socket file (`unimatrix.sock`) needs lifecycle management analogous to PidGuard:
- Created by server on startup
- Removed on clean shutdown
- Stale socket detection: try connecting, if refused, remove and recreate
- Socket path: `~/.unimatrix/{hash}/unimatrix.sock`

---

## 9. Open Risks

### R1: redb Exclusive Lock Is Non-Negotiable

The biggest finding of this research: redb v3.1.x's exclusive file lock means the implant CANNOT directly read the database while the MCP server is running. This forces IPC for all database access. If redb adds a mode for concurrent read+write across processes in a future version, the architecture could simplify. But we cannot depend on this.

**Mitigation:** Design the IPC protocol to be fast enough (<50ms target) and fall back gracefully when the server is unavailable. The unified binary (Option D) minimizes the IPC overhead by keeping serialization compact and protocols aligned.

### R2: ONNX Cold Start Blocks Standalone Semantic Search

If the implant needs to do semantic search without the server (standalone mode), it must load ONNX. `OnnxProvider::new()` downloads the model (~90MB first time), loads the tokenizer, and builds the ONNX session. This takes ~200-500ms — far beyond the 50ms hook latency budget.

**Mitigation:** In standalone mode, skip semantic search entirely. Use only index-based lookups (topic, category, tag). For compaction defense, use injection history replay (Strategy 2). Only enable semantic search when the server is running (hot ONNX via IPC).

### R3: Socket Availability on All Platforms

Unix domain sockets are available on Linux and macOS but have limited support on Windows (added in Windows 10 1803+). Since Claude Code's supported platforms include Windows, the transport layer must have a Windows fallback.

**Mitigation:** Abstract the transport behind a trait. Primary: UDS (Linux, macOS). Fallback: named pipe (Windows) or TCP localhost (universal but slightly more overhead).

### R4: Hook Process Startup Overhead

Each hook invocation starts a new process (unless daemon architecture). Process startup time on Linux is ~1-3ms for a compiled Rust binary. This is acceptable for the 50ms budget but leaves less room for the actual query.

**Mitigation:** Minimize binary size for the hook subcommand (no ONNX linked). Use the unified binary approach where the hook is a thin subcommand. Consider daemon architecture in Phase 2 to eliminate process startup entirely.

### R5: Session State Persistence for Ephemeral Hooks

The compaction defense Strategy 2 requires knowing which entries were injected during the session. If the hook process is ephemeral (new process per event), this state must be persisted between invocations.

**Options:**
- Sidecar file: `~/.unimatrix/{hash}/sessions/{session_id}.json` — written on each injection, read on PreCompact
- Server-side: Server maintains session state, implant queries it
- Environment variable: Claude Code could pass session context (but current hook interface is limited)

**Recommendation:** Server-side session state. The server maintains an in-memory map of `session_id -> Vec<injected_entry_id>`. The implant registers injections via fire-and-forget IPC. On PreCompact, the server already has the injection history.

### R6: Session Identity Without Claude Code Support

Claude Code does not expose a session ID to hook processes. The implant needs to correlate events across a session.

**Mitigation options:**
- Use parent PID (Claude Code process PID) as session proxy — available in all Unix environments
- Use the `CLAUDE_SESSION_ID` or equivalent env var if Claude Code exposes one (check at runtime)
- Generate a session ID on first hook invocation and pass it via a file or env var

### R7: Server Socket Listener Adds Complexity

Adding a UDS listener to the MCP server is a non-trivial change:
- The server currently uses only stdio (rmcp SDK)
- Adding a second listener requires tokio task management
- The listener must handle concurrent connections from multiple hook processes
- Must not interfere with the existing MCP protocol on stdio

**Mitigation:** The UDS listener is a separate tokio task that accepts connections and routes to the same internal API (via `unimatrix-engine`). It does NOT share the rmcp router — it has its own JSON-RPC handler. This keeps the MCP stdio path untouched.

---

## 10. Summary of Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Primary transport | IPC to MCP server via Unix domain socket | redb exclusive lock prevents direct database access from implant |
| IPC protocol | JSON-RPC over UDS | Matches MCP's existing protocol style, debuggable |
| Binary architecture | Unified `unimatrix` binary with `hook` subcommand | Solves version sync, single distribution artifact |
| Code sharing | New `unimatrix-engine` crate with shared business logic | Extracted from server, used by both server and implant |
| PreCompact strategy | Injection history replay (ID-based fetch) with briefing fallback | Fast (<15ms), session-aware, no ONNX needed |
| Standalone mode | Index-based lookups via ReadOnlyDatabase when server is down | No semantic search without server (ONNX cold start too slow) |
| Write handling | Fire-and-forget to server; queue to file when server unavailable | Events are telemetry — loss is acceptable, queue provides durability |
| Session tracking | Server-side in-memory map, keyed by parent PID | Implant registers injections; server serves compaction payload |
| Phase 2 evolution | Daemon architecture with database ownership | Best latency, hot ONNX, but high complexity — defer until hooks proven |
