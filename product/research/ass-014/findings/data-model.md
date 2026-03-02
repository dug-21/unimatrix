# RQ-1: Unified Data Model for the Cortical Implant

**Deliverable:** D14-1
**Date:** 2026-03-01
**Research Question:** What data model supports both MCP tools (agent-initiated, explicit) and cortical implant (system-initiated, automatic) without degrading knowledge quality?

---

## 1. Current State: The 14-Table Schema (v3)

The Unimatrix storage engine (`crates/unimatrix-store/`) uses redb v3.1.x with bincode v2 serde serialization. The schema is at version 3 (migrated through v0->v1 in nxs-004, v1->v2 in crt-001, v2->v3 in crt-005 for f32->f64 confidence). The PRODUCT-VISION.md counts 13 tables; the codebase actually has **14** (OBSERVATION_METRICS was added in col-002).

### 14 Tables

| # | Table | Type | Key | Value | Purpose | Feature |
|---|-------|------|-----|-------|---------|---------|
| 1 | ENTRIES | Table | `u64` | `&[u8]` (bincode EntryRecord) | Primary entry storage | nxs-001 |
| 2 | TOPIC_INDEX | Table | `(&str, u64)` | `()` | Topic prefix scan | nxs-001 |
| 3 | CATEGORY_INDEX | Table | `(&str, u64)` | `()` | Category prefix scan | nxs-001 |
| 4 | TAG_INDEX | MultimapTable | `&str` | `u64` | Tag set intersection | nxs-001 |
| 5 | TIME_INDEX | Table | `(u64, u64)` | `()` | Temporal range queries | nxs-001 |
| 6 | STATUS_INDEX | Table | `(u8, u64)` | `()` | Status filtering | nxs-001 |
| 7 | VECTOR_MAP | Table | `u64` | `u64` | entry_id -> hnsw_data_id bridge | nxs-002 |
| 8 | COUNTERS | Table | `&str` | `u64` | ID generation, schema_version, status counts | nxs-001 |
| 9 | AGENT_REGISTRY | Table | `&str` | `&[u8]` (bincode AgentRecord) | Agent identity and trust | vnc-001 |
| 10 | AUDIT_LOG | Table | `u64` | `&[u8]` (bincode AuditEntry) | Append-only request trail | vnc-001 |
| 11 | FEATURE_ENTRIES | MultimapTable | `&str` | `u64` | Feature-to-entry linking | crt-001 |
| 12 | CO_ACCESS | Table | `(u64, u64)` | `&[u8]` (bincode CoAccessRecord) | Entry co-retrieval pairs | crt-004 |
| 13 | OUTCOME_INDEX | Table | `(&str, u64)` | `()` | Feature-cycle outcome entries | col-001 |
| 14 | OBSERVATION_METRICS | Table | `&str` | `&[u8]` (bincode MetricVector) | Retrospective metric storage | col-002 |

### EntryRecord (27 fields)

```rust
pub struct EntryRecord {
    // Core fields (nxs-001)
    pub id: u64,
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source: String,
    pub status: Status,             // Active | Deprecated | Proposed | Quarantined
    pub confidence: f64,            // [0.0, 1.0] composite score (crt-005: f64)
    pub created_at: u64,            // Unix seconds
    pub updated_at: u64,            // Unix seconds
    pub last_accessed_at: u64,
    pub access_count: u32,
    pub supersedes: Option<u64>,
    pub superseded_by: Option<u64>,
    pub correction_count: u32,
    pub embedding_dim: u16,

    // Security fields (nxs-004)
    pub created_by: String,
    pub modified_by: String,
    pub content_hash: String,       // SHA-256
    pub previous_hash: String,
    pub version: u32,
    pub feature_cycle: String,
    pub trust_source: String,       // "agent" | "human" | "system"

    // Usage tracking (crt-001)
    pub helpful_count: u32,
    pub unhelpful_count: u32,
}
```

### In-Memory State (Not Persisted to redb)

- **UsageDedup** (`usage_dedup.rs`): Per-session dedup of access counts and votes. Tracks `(agent_id, entry_id)` pairs for access, `(agent_id, entry_id) -> bool` for votes, and `(min_id, max_id)` for co-access pairs. Cleared on server restart.
- **HNSW Index** (`unimatrix-vector`): In-memory hnsw_rs graph, persisted/loaded via `.hnsw` file. 384d DistDot. VECTOR_MAP is the crash-safe source of truth for id mappings.
- **Embedder** (`unimatrix-embed`): ONNX runtime session. ~200ms cold start.
- **CategoryAllowlist** (`categories.rs`): RwLock-protected runtime-extensible set. Poison recovery.

### Core Traits (unimatrix-core)

```rust
pub trait EntryStore: Send + Sync {
    fn insert(&self, entry: NewEntry) -> Result<u64, CoreError>;
    fn update(&self, entry: EntryRecord) -> Result<(), CoreError>;
    fn get(&self, id: u64) -> Result<EntryRecord, CoreError>;
    fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError>;
    fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>;
    // ... 11 more methods
}

pub trait VectorStore: Send + Sync {
    fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<(), CoreError>;
    fn search(&self, query: &[f32], top_k: usize, ef_search: usize) -> Result<Vec<SearchResult>, CoreError>;
    fn search_filtered(&self, ...) -> Result<Vec<SearchResult>, CoreError>;
    fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<(), CoreError>;
    // ...
}

pub trait EmbedService: Send + Sync {
    fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError>;
    fn dimension(&self) -> usize;
}
```

### Schema Evolution Pattern

Established in nxs-004, exercised 3 times (v0->v1, v1->v2, v2->v3):
1. Append new fields to EntryRecord with `#[serde(default)]`
2. Bump `CURRENT_SCHEMA_VERSION` constant
3. Add `migrate_vN_to_vN+1()` function that scan-and-rewrites all entries
4. `migrate_if_needed()` runs on `Store::open()`, single write transaction

**Key constraint:** bincode v2 with serde path uses positional encoding. Fields can only be appended, never removed or reordered. All inserts write the full current schema. Migration rewrites all existing entries.

---

## 2. New Concepts Taxonomy

The cortical implant introduces six categories of new data. Each is classified on the durable/ephemeral spectrum.

### 2.1 Session Records

**What:** A bounded period of agent-Unimatrix interaction, delimited by SessionStart and SessionEnd hook events.
**Fields:** session_id, parent_pid (Claude Code process), agent_role, agent_task, feature_cycle, started_at, ended_at, outcome (pass/fail/abandoned), compaction_count, injection_count, entry_ids_injected, entry_ids_retrieved_mcp.
**Lifecycle:** Created on SessionStart hook, updated incrementally, closed on SessionEnd.
**Classification:** **Hybrid.** The raw session record is ephemeral telemetry (useful for hours to days). A session *summary* (outcome, key decisions encountered, entries that helped) is durable knowledge.

### 2.2 Injection Records

**What:** A record of each knowledge injection into a prompt via hooks.
**Fields:** session_id, timestamp, hook_type (UserPromptSubmit / PreCompact / SubagentStart), entry_ids injected, prompt_context (truncated), token_count, injection_reason (semantic match / compaction defense / routing).
**Lifecycle:** Created per hook event. Accumulated during session. Summarized at session end. Raw records garbage collected after summary.
**Classification:** **Ephemeral telemetry.** Individual injection events are only useful for debugging and session-end summarization. They must NOT become entries or get embedded.

### 2.3 Confidence Signals (Implicit)

**What:** Inferred helpfulness signals from session outcomes (col-009). When a task succeeds, entries injected during the session receive a bulk helpful signal. When rework is detected, entries may receive unhelpful signals.
**Fields:** session_id, entry_ids, signal_type (helpful/unhelpful), signal_source (implicit_outcome / explicit_mcp), strength (full / partial).
**Lifecycle:** Generated at session end (or on rework detection mid-session). Applied immediately to EntryRecord.helpful_count / unhelpful_count. Raw signal discarded after application.
**Classification:** **Ephemeral event** that produces a **durable side effect** (mutates existing EntryRecord confidence inputs). The signal itself need not persist — its effect is captured in the entry's counters.

### 2.4 Compaction Defense State

**What:** The state needed to reconstruct critical context after Claude Code compresses conversation history.
**Fields:** session_id, active_role, active_task, active_feature, injection_history (ordered list of entry_ids with timestamps and scores), active_decisions (entry_ids of high-confidence decisions for current feature), working_context (current files, recent tool outputs — NOT stored by Unimatrix).
**Lifecycle:** Built incrementally as injections occur. Consumed on PreCompact hook. Evicted on SessionEnd.
**Classification:** **Session-scoped ephemeral state.** Must survive across hook invocations within a single session but has zero value after the session ends. This is process memory, not knowledge.

### 2.5 Routing Decisions

**What:** Records of how the implant decided which entries to inject (col-011 semantic agent routing, col-007 context injection).
**Fields:** session_id, timestamp, prompt_embedding (or hash), candidate_entry_ids, selected_entry_ids, scores, selection_reason, token_budget_used.
**Lifecycle:** Per-injection event. Useful for prompt debugger (mtx-005) and retrospective. Summarized at session end.
**Classification:** **Ephemeral telemetry.** Useful for debugging and retrospective analysis. Must not pollute the entry/embedding space.

### 2.6 Session Summary Entries

**What:** Distilled knowledge from a completed session: what worked, what didn't, patterns observed.
**Fields:** Standard EntryRecord fields with category "observation" or "outcome", plus structured tags linking back to the session.
**Lifecycle:** Created at SessionEnd from session telemetry. Becomes durable knowledge subject to normal entry lifecycle (confidence, corrections, deprecation).
**Classification:** **Durable knowledge.** This is the bridge between ephemeral telemetry and the knowledge base. Session summaries are entries — they get embedded, searched, and participate in confidence evolution.

### Classification Summary

| Concept | Classification | Gets Embedded? | Stored Where? | Lifetime |
|---------|---------------|----------------|---------------|----------|
| Session Record | Hybrid | Summary only | Dedicated table + entry | Hours to days (raw), permanent (summary) |
| Injection Record | Ephemeral | No | Dedicated table | Session + GC window |
| Confidence Signal | Ephemeral event | No | Applied to EntryRecord | Instant (consumed on arrival) |
| Compaction Defense | Session-scoped | No | Daemon memory or sidecar | Session lifetime |
| Routing Decision | Ephemeral | No | Dedicated table | Session + GC window |
| Session Summary | Durable | Yes | ENTRIES table | Permanent (standard lifecycle) |

---

## 3. Candidate Data Models

### 3.1 Model A: Entries-Only Extension

**Approach:** Everything becomes an EntryRecord with special categories.

- Sessions -> entries with `category: "session"`
- Injections -> entries with `category: "injection-log"`
- Routing decisions -> entries with `category: "routing-log"`
- New categories added to allowlist

**Schema changes:**
- Add `session_id: String` field to EntryRecord (for linking)
- New categories in allowlist
- No new tables

**Pros:**
- Minimal schema change. Single table, single query path.
- All data participates in existing search, confidence, correction chain infrastructure.
- Simplest migration (one field addition).

**Cons:**
- **Embedding space pollution (CRITICAL).** Every injection event gets a 384d vector. A session with 50 injections creates 50 embedded events in HNSW. These are structurally different from knowledge entries — their embeddings will create noise clusters in the vector space, degrading semantic search for real knowledge.
- **Volume explosion.** A single session generates ~100-200 events. At 10 sessions/day, that is 1000-2000 entries/day. The current knowledge base has ~53 active entries. Telemetry would outnumber knowledge 100:1 within a week.
- **Confidence contamination.** Ephemeral events don't have meaningful confidence — they aren't "helpful" or "unhelpful" knowledge. Mixing them into the confidence pipeline adds noise.
- **Search result pollution.** `context_search("how do I write tests?")` would return injection log entries alongside actual test conventions.
- **GC complexity.** Entries don't currently support TTL or automatic cleanup. Deleting old events requires scanning by category + time, updating all indexes.

**Verdict:** Rejected. Violates the "it starts with the data model" principle. Mixing ephemeral telemetry with durable knowledge degrades the core value proposition.

### 3.2 Model B: Parallel Table Tier

**Approach:** Two tiers of storage — knowledge tier (existing 14 tables, untouched) and telemetry tier (new tables, separate lifecycle).

**New tables:**

| Table | Type | Key | Value | Purpose |
|-------|------|-----|-------|---------|
| SESSIONS | Table | `&str` (session_id) | `&[u8]` (bincode SessionRecord) | Session lifecycle |
| INJECTION_LOG | Table | `(u64, &str)` (timestamp, session_id) | `&[u8]` (bincode InjectionRecord) | Per-injection events |
| SIGNAL_QUEUE | Table | `u64` (monotonic_id) | `&[u8]` (bincode SignalRecord) | Pending confidence signals |

**SessionRecord:**
```rust
pub struct SessionRecord {
    pub session_id: String,
    pub parent_pid: u32,
    pub agent_role: String,
    pub agent_task: String,
    pub feature_cycle: String,
    pub started_at: u64,
    pub ended_at: u64,            // 0 until closed
    pub status: SessionStatus,    // Active | Completed | Abandoned
    pub compaction_count: u32,
    pub injection_count: u32,
    pub injected_entry_ids: Vec<u64>,
    pub mcp_retrieved_entry_ids: Vec<u64>,
}
```

**InjectionRecord:**
```rust
pub struct InjectionRecord {
    pub session_id: String,
    pub timestamp: u64,
    pub hook_type: HookEventType,  // UserPromptSubmit | PreCompact | SubagentStart
    pub entry_ids: Vec<u64>,
    pub scores: Vec<f64>,         // Parallel to entry_ids
    pub token_count: u32,
    pub reason: InjectionReason,  // SemanticMatch | CompactionDefense | Routing
}
```

**Schema changes:**
- Add `session_id` field to EntryRecord (optional, for session-created entries)
- 3 new tables
- Knowledge tier completely isolated — no embedding, no HNSW, no confidence

**Pros:**
- **Complete isolation.** Telemetry never enters HNSW. Search quality unaffected.
- **Independent lifecycle.** Telemetry has its own GC (time-based) independent of entry status lifecycle.
- **Volume-safe.** Telemetry tables can grow freely without impacting knowledge query performance (redb scans are table-scoped).
- **Clean API boundary.** Knowledge operations (`context_search`, `context_store`) never touch telemetry tables. Telemetry operations (`session_start`, `record_injection`) never touch knowledge tables. The only bridge is confidence signal application (telemetry -> EntryRecord mutation).

**Cons:**
- More tables to manage (17 total). Migration adds 3 table definitions to `Store::open()`.
- Telemetry queries require separate code paths (not reusable via QueryFilter).
- Signal queue adds a new async pattern: accumulate signals, apply in batch at session end or periodically.

**Verdict:** Strong contender. Clean separation, no knowledge quality impact. Additional complexity is manageable.

### 3.3 Model C: Hybrid with Session-Scoped Sidecar

**Approach:** Session-scoped ephemeral data lives outside redb entirely (in the implant's process memory or a sidecar SQLite/file per session). Only durable outcomes flow into redb.

**redb changes:**
- Add 1 new table: SESSIONS (summary only, written at SessionEnd)
- Add `session_id` field to EntryRecord

**Sidecar (per-session temporary file or in-memory):**
- Injection log (full detail)
- Routing decisions (full detail)
- Compaction defense state
- Accumulated confidence signals

**Flow:** During session, all ephemeral data stays in the sidecar. On SessionEnd, the implant: (1) writes a SessionRecord summary to redb SESSIONS table, (2) creates session summary entries if warranted, (3) applies accumulated confidence signals to EntryRecords, (4) deletes the sidecar.

**Pros:**
- **Minimal redb footprint.** Only 1 new table. Ephemeral data never touches the knowledge database.
- **Fastest write path for hooks.** Writing to local memory or a temp file is faster than redb write transactions.
- **Natural GC.** Sidecar is deleted when session ends. No scan-and-cleanup needed.
- **Compaction defense state naturally lives in the sidecar.** The implant daemon's process memory is the simplest and fastest storage for injection history.

**Cons:**
- **Data loss on crash.** If the implant process dies mid-session, all sidecar data is lost. Session outcome, accumulated signals, injection history — all gone. For a daemon, this is a real risk.
- **No cross-session telemetry queries.** Cannot query "show me all injections across the last 10 sessions" because raw data doesn't persist. Only summaries survive.
- **Retrospective pipeline (col-002) loses raw data.** The existing JSONL observation files serve this role today. Removing them without a replacement leaves a gap.
- **Two storage systems.** redb for knowledge, sidecar for telemetry. More moving parts. Sidecar format needs its own serialization.

**Verdict:** Partial solution. Good for compaction defense state (which is inherently process-scoped) but insufficient for telemetry that needs to survive across sessions for retrospective analysis.

---

## 4. Recommended Model: Model B (Parallel Table Tier) with Model C Elements

### 4.1 Architecture

Combine Model B's durable telemetry tables with Model C's insight that compaction defense state belongs in process memory.

**Storage tiers:**

| Tier | Storage | Contents | Lifecycle | Embedded? |
|------|---------|----------|-----------|-----------|
| **Knowledge** | redb (existing 14 tables) | Entries, indexes, vectors, confidence, agents | Permanent (standard lifecycle) | Yes (384d HNSW) |
| **Telemetry** | redb (3 new tables) | Sessions, injection logs, signal queue | Time-bounded (GC after configurable window, default 30 days) | No |
| **Session State** | Implant daemon memory | Compaction defense state, active injection cache | Session lifetime (process-scoped) | No |

### 4.2 New Tables (Telemetry Tier)

#### Table 15: SESSIONS

```rust
pub const SESSIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("sessions");

pub struct SessionRecord {
    pub session_id: String,
    pub parent_pid: u32,
    pub agent_role: String,
    pub agent_task: String,
    pub feature_cycle: String,
    pub started_at: u64,
    pub ended_at: u64,             // 0 = still active
    pub status: SessionStatus,     // Active | Completed | Abandoned | TimedOut
    pub compaction_count: u32,
    pub total_injections: u32,
    pub total_mcp_retrievals: u32,
    pub injected_entry_ids: Vec<u64>,
    pub mcp_retrieved_entry_ids: Vec<u64>,
    pub outcome_summary: String,   // Brief text summary, empty until SessionEnd
}

pub enum SessionStatus {
    Active = 0,
    Completed = 1,
    Abandoned = 2,
    TimedOut = 3,
}
```

**Key:** session_id (string, format: `"{parent_pid}-{start_timestamp}"` or Claude Code's session ID if exposed).
**GC:** Sessions older than 30 days (configurable) are deleted during maintenance (`maintain=true` on `context_status`). Active sessions are never GC'd.

#### Table 16: INJECTION_LOG

```rust
pub const INJECTION_LOG: TableDefinition<(u64, u64), &[u8]> =
    TableDefinition::new("injection_log");

pub struct InjectionRecord {
    pub session_id: String,
    pub hook_type: HookEventType,
    pub entry_ids: Vec<u64>,
    pub scores: Vec<f64>,
    pub token_count: u32,
    pub reason: InjectionReason,
}

pub enum HookEventType {
    UserPromptSubmit = 0,
    PreCompact = 1,
    SubagentStart = 2,
    PostToolUse = 3,
}

pub enum InjectionReason {
    SemanticMatch = 0,
    CompactionDefense = 1,
    AgentRouting = 2,
    FeatureContext = 3,
}
```

**Key:** `(timestamp_millis: u64, sequence: u64)` — compound key for temporal ordering with sub-millisecond resolution. The sequence counter handles same-millisecond events.
**GC:** Same window as SESSIONS (30 days). Cleaned up by session — when a session is GC'd, its injection records are also removed.

#### Table 17: SIGNAL_QUEUE

```rust
pub const SIGNAL_QUEUE: TableDefinition<u64, &[u8]> =
    TableDefinition::new("signal_queue");

pub struct SignalRecord {
    pub session_id: String,
    pub timestamp: u64,
    pub entry_ids: Vec<u64>,
    pub signal_type: SignalType,
    pub source: SignalSource,
}

pub enum SignalType {
    Helpful = 0,
    Flagged = 1,              // Correlated with rework — surfaced in retrospective, NOT auto-applied
}

pub enum SignalSource {
    ImplicitOutcome = 0,      // Session completed successfully -> auto-apply Helpful
    ImplicitRework = 1,       // Rework detected -> Flagged for human review (never auto-downweight)
    ExplicitMcp = 2,          // Agent explicitly voted via MCP tool -> auto-apply (helpful or unhelpful)
}

// DESIGN DECISION (2026-03-01): Auto-positive, flag-negative, never auto-downweight.
//
// Implicit signals from session outcomes are ASYMMETRIC:
// - Successful session → Helpful signals auto-applied to all injected entries (safe: rising tide)
// - Rework session → Flagged entries surfaced in retrospective report for human review
//   NOT auto-applied as unhelpful. Rationale: guilt-by-association problem — if 5 entries
//   were injected and 1 was bad, all 5 would be penalized. Only explicit MCP votes (where
//   an agent deliberately says "this was unhelpful") trigger unhelpful_count increments.
//
// This means SIGNAL_QUEUE entries with source=ImplicitRework are consumed by the
// retrospective pipeline (col-002), NOT by the confidence pipeline (crt-002).
// They appear in the RetrospectiveReport.entries_analysis section as "correlated with rework"
// for human review, alongside the hotspot findings and baseline comparisons.
```

**Key:** Monotonic u64 (auto-increment from COUNTERS, key `"next_signal_id"`).
**Lifecycle:** Signals are consumed and deleted after being applied to EntryRecords. The queue is a write-ahead buffer — the implant writes signals, the server (or implant itself) applies them in batch. This decouples signal generation from signal application and prevents lost updates if the process dies between inference and write.

### 4.3 EntryRecord Field Addition

Add one new field to EntryRecord:

```rust
// -- cortical implant fields (appended after unhelpful_count) --
/// Session that created this entry (empty for pre-implant entries).
#[serde(default)]
pub session_id: String,
```

This enables linking an entry back to the session that created it. Optional — empty for all existing entries and entries created via MCP without session context.

### 4.4 Compaction Defense State (Daemon Memory)

The compaction defense state does NOT go into redb. It lives in the implant daemon's process memory:

```rust
pub struct CompactionState {
    /// Session this state belongs to.
    pub session_id: String,

    /// Active agent context.
    pub role: String,
    pub task: String,
    pub feature_cycle: String,

    /// Ordered injection history (most recent first).
    /// Each entry: (entry_id, injection_timestamp, confidence_at_injection)
    pub injection_history: Vec<(u64, u64, f64)>,

    /// High-confidence entries for current feature (pre-computed).
    /// Refreshed when feature_cycle changes or every N injections.
    pub feature_decisions: Vec<u64>,

    /// Token budget tracking.
    pub total_tokens_injected: u32,
    pub last_compaction_at: u64,
    pub compaction_count: u32,
}
```

**Why daemon memory, not redb:**
1. **Latency.** PreCompact hook must respond in <50ms. Reading compaction state from redb adds a read transaction + deserialization (~5-10ms). Reading from memory is sub-microsecond.
2. **Write frequency.** Updated on every injection (every prompt cycle). Writing to redb on every prompt creates write contention with the MCP server.
3. **Session-scoped lifetime.** This state has zero value after the session ends. Persisting it wastes storage.
4. **Crash recovery.** If the daemon dies, the session is effectively dead anyway. Compaction defense for a dead session is meaningless. The next session starts fresh.

**Risk mitigation:** The daemon can optionally write a checkpoint to a temp file (`~/.unimatrix/{project_hash}/session-{id}.state`) every N minutes as a crash recovery aid. This is a sidecar file, not a redb table. It is deleted on SessionEnd.

---

## 5. Schema Migration Plan: v3 to v4

### 5.1 Changes

**EntryRecord:**
- Append `session_id: String` with `#[serde(default)]` (defaults to empty string)

**New tables (3):**
- SESSIONS
- INJECTION_LOG
- SIGNAL_QUEUE

**New counter keys:**
- `"next_signal_id"`: Monotonic signal queue ID

**New enums:**
- SessionStatus, HookEventType, InjectionReason, SignalType, SignalSource

### 5.2 Migration Function

```rust
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 4;

fn migrate_v3_to_v4(txn: &WriteTransaction) -> Result<()> {
    // 1. Scan-and-rewrite all entries to add session_id field
    let entry_ids: Vec<u64> = {
        let table = txn.open_table(ENTRIES)?;
        table.iter()?.map(|r| r.map(|(k, _)| k.value())).collect::<Result<Vec<_>>>()?
    };

    for id in entry_ids {
        let table = txn.open_table(ENTRIES)?;
        if let Some(guard) = table.get(id)? {
            let bytes = guard.value().to_vec();
            drop(guard);
            drop(table);

            // Deserialize with v3 schema (no session_id)
            let v3_record: EntryRecordV3 = deserialize_v3(&bytes)?;

            // Convert to v4 (session_id = "")
            let v4_record = EntryRecord {
                session_id: String::new(),
                ..v3_record.into()
            };

            let new_bytes = serialize_entry(&v4_record)?;
            let mut table = txn.open_table(ENTRIES)?;
            table.insert(id, new_bytes.as_slice())?;
        }
    }

    // 2. New tables are created in Store::open() (already handles all table creation)
    // No data migration needed for SESSIONS, INJECTION_LOG, SIGNAL_QUEUE

    Ok(())
}
```

### 5.3 Backward Compatibility

- **Existing MCP tools:** Unaffected. The 3 new tables are only accessed by implant code paths. MCP tool handlers never touch SESSIONS, INJECTION_LOG, or SIGNAL_QUEUE.
- **Existing entries:** Gain an empty `session_id` field. No functional change.
- **QueryFilter:** Unchanged. No new filter dimensions for telemetry tables (they have their own query patterns).
- **Core traits:** Unchanged. `EntryStore`, `VectorStore`, `EmbedService` operate only on the knowledge tier. New telemetry operations use direct redb access (same pattern as AUDIT_LOG, CO_ACCESS).
- **Agents without the implant:** Continue using MCP tools as before. Telemetry tables stay empty. No degradation.

### 5.4 Table Creation Order

`Store::open()` will create all 17 tables:

```rust
// Existing 14 tables...
txn.open_table(SESSIONS).map_err(StoreError::Table)?;
txn.open_table(INJECTION_LOG).map_err(StoreError::Table)?;
txn.open_table(SIGNAL_QUEUE).map_err(StoreError::Table)?;
```

---

## 6. Volume Projections

### 6.1 Assumptions

- A typical Claude Code session: 30-60 prompt cycles, lasting 20-60 minutes
- Hooks fire on: every prompt (UserPromptSubmit), every compaction (PreCompact), every tool use (PostToolUse for observation)
- Context injection (col-007): 1 injection per prompt cycle, injecting 3-5 entries
- Compaction events: 1-3 per session (depends on session length and injection volume)
- MCP tool calls: 5-20 per session (explicit agent calls)
- Active developer: 5-10 sessions per day

### 6.2 Per-Session Write Volumes

| Data Type | Events/Session | Bytes/Event | Total/Session |
|-----------|---------------|-------------|---------------|
| SessionRecord (1 create + N updates) | ~35 writes | ~500 bytes | ~17.5 KB |
| InjectionRecords | 30-60 | ~200 bytes | 6-12 KB |
| SignalRecords | 3-5 (batch at session end) | ~150 bytes | ~750 bytes |
| Session summary entry (if warranted) | 0-1 | ~1 KB (entry + embedding) | 0-1 KB |

**Total per session:** ~25-30 KB of telemetry data.

### 6.3 Daily and Monthly Projections

| Metric | Per Day (10 sessions) | Per Month (200 sessions) | Per Year (2400 sessions) |
|--------|----------------------|-------------------------|------------------------|
| SESSIONS rows | 10 | 200 | 2,400 |
| INJECTION_LOG rows | 300-600 | 6,000-12,000 | 72,000-144,000 |
| SIGNAL_QUEUE rows | 30-50 (consumed) | 0 (consumed) | 0 (consumed) |
| Raw telemetry storage | 250-300 KB | 5-6 MB | 60-72 MB |
| After 30-day GC | 250-300 KB | 5-6 MB | 5-6 MB (steady state) |

### 6.4 Storage Overhead Assessment

**redb file size impact:** The current knowledge base is small (~53 active entries, ~170 total entries). With all tables, indexes, and HNSW data, the redb file is likely 5-15 MB. Adding 5-6 MB of telemetry (at steady state after GC) roughly doubles the file size. This is acceptable for a local-first embedded database.

**HNSW impact:** Zero. Telemetry tier data is never embedded. The HNSW graph only grows with knowledge entries.

**Write contention:** With the recommended model, telemetry writes happen through the implant's write path. If the implant shares the redb database with the MCP server, they must coordinate writes (redb allows only one writer at a time). Volume: ~1 write per prompt cycle (injection log) + session updates. At 1 prompt per 10-30 seconds, this is 2-6 writes/minute. redb write transactions complete in <1ms for small records. Contention risk is low but must be tested (this is an RQ-2 concern).

### 6.5 Comparison with Current JSONL Telemetry

col-002 currently writes JSONL files to `~/.unimatrix/observation/`. These files are per-session, average ~50-200 KB each, and are retained for 60 days. The cortical implant's injection logs are smaller per event (structured, no response snippets) but higher frequency (every prompt vs. every tool call). Total volume is comparable. The JSONL files can be deprecated once the implant absorbs the observer role, consolidating two storage locations into one.

---

## 7. Isolation Boundaries

### 7.1 Embedding Space Isolation

**Rule:** Telemetry tier data (SESSIONS, INJECTION_LOG, SIGNAL_QUEUE) is NEVER embedded. No vectors are created, no HNSW insertions, no VECTOR_MAP entries.

**Enforcement:** The EmbedService is called only by the `context_store` and `context_correct` code paths (knowledge tier). The implant's telemetry write paths bypass embedding entirely.

**Session summary entries** that are promoted to the knowledge tier (written via `context_store`) DO get embedded. This is correct — they are durable knowledge. The promotion step is explicit: the implant calls `context_store` with a summary, and the normal store pipeline handles embedding.

### 7.2 Search Isolation

**Rule:** `context_search` and `context_lookup` only query the knowledge tier. Telemetry data is invisible to these tools.

**Enforcement:** The existing query paths use ENTRIES + index tables (knowledge tier). The telemetry tables have different names and are not included in any QueryFilter code path.

**New query paths** for telemetry are separate methods on Store (or a new TelemetryStore trait):
- `get_session(session_id)` -> SessionRecord
- `list_sessions(time_range, status_filter)` -> Vec<SessionRecord>
- `get_injection_history(session_id)` -> Vec<InjectionRecord>
- `drain_signals(batch_size)` -> Vec<SignalRecord>

### 7.3 Confidence Isolation

**Rule:** Telemetry events do not have confidence scores. Confidence applies only to knowledge entries.

**Bridge:** Implicit confidence signals flow asymmetrically:
- **ImplicitOutcome (Helpful):** Auto-applied to `EntryRecord.helpful_count` via the confidence pipeline (crt-002). Same path as explicit MCP votes.
- **ImplicitRework (Flagged):** NOT applied to `unhelpful_count`. Consumed by the retrospective pipeline (col-002) and surfaced in `RetrospectiveReport.entries_analysis` for human review. This prevents guilt-by-association — entries co-injected with a bad entry are not automatically penalized.
- **ExplicitMcp:** Applied directly as before (helpful or unhelpful). Only path that can increment `unhelpful_count`.

**Design decision:** Auto-positive, flag-negative, never auto-downweight. The SIGNAL_QUEUE serves two consumers: the confidence pipeline (Helpful signals only) and the retrospective pipeline (Flagged signals for human review). The `SignalSource` discriminator routes signals to the correct consumer.

### 7.4 Maintenance Isolation

**Rule:** Coherence gate (crt-005) maintenance operates only on the knowledge tier. Telemetry tier has its own, simpler GC.

**Knowledge tier maintenance** (triggered by `maintain=true` on `context_status`):
- Confidence refresh (recompute stale entries)
- Graph compaction (HNSW rebuild)
- Co-access cleanup (stale pair removal)

**Telemetry tier GC** (triggered by the same `maintain=true` flag, but separate logic):
- Delete SESSIONS where `ended_at > 0 AND ended_at < now - retention_window`
- Delete INJECTION_LOG records for GC'd sessions
- Drain and apply any remaining SIGNAL_QUEUE entries
- Report telemetry stats in StatusReport (new section)

---

## 8. Session Data Lifecycle

### 8.1 Creation (SessionStart hook)

1. Implant daemon receives SessionStart event
2. Creates SessionRecord with `status: Active`, timestamps, agent context
3. Writes to SESSIONS table
4. Initializes CompactionState in daemon memory

### 8.2 Accumulation (During session)

On each prompt cycle:
1. **UserPromptSubmit hook** fires
2. Implant queries Unimatrix for relevant entries (semantic search)
3. Formats injection payload, prints to stdout
4. Writes InjectionRecord to INJECTION_LOG
5. Updates CompactionState in memory (adds injected entry_ids)
6. Updates SessionRecord.injection_count (batch update, not per-prompt)

On each compaction:
1. **PreCompact hook** fires
2. Implant reads CompactionState from memory
3. Constructs compaction defense payload (prioritized entries)
4. Prints to stdout
5. Increments CompactionState.compaction_count
6. Writes InjectionRecord with `reason: CompactionDefense`

### 8.3 Signal Generation (Mid-session and session end)

Mid-session (rework detection):
1. PostToolUse hook detects rework pattern (repeated edits, compile failures)
2. Writes SignalRecord with `signal_type: Unhelpful, source: ImplicitRework` for recently injected entries

Session end:
1. SessionEnd hook fires (or timeout detected)
2. If session outcome is positive: write SignalRecord with `signal_type: Helpful` for all injected entries
3. Write final SessionRecord update with `status: Completed`, `ended_at`, `outcome_summary`
4. Optionally: create session summary entry via `context_store` if session produced notable outcomes
5. Clear CompactionState from daemon memory

### 8.4 Signal Application (Batch)

Signals are applied to EntryRecords in batch, not per-signal:

1. Drain up to N signals from SIGNAL_QUEUE
2. Group by entry_id
3. For each entry: read current helpful_count/unhelpful_count, apply deltas, write back
4. Delete consumed signal records

**When:** Triggered by:
- Session end processing
- Periodic timer in the daemon (every 5 minutes)
- `maintain=true` on `context_status` (MCP-triggered)

### 8.5 Garbage Collection

Time-based with configurable retention (default 30 days):

1. Scan SESSIONS for records where `ended_at > 0 AND ended_at < now - 30d`
2. For each expired session:
   a. Delete all INJECTION_LOG records for that session_id
   b. Delete the SessionRecord
3. Report: "GC'd N sessions, M injection records, freed ~X bytes"

**Never GC:**
- Active sessions (ended_at == 0)
- Sessions < 30 days old
- Session summary entries in the knowledge tier (they are normal entries with normal lifecycle)

---

## 9. Compaction Defense State: Detailed Design

### 9.1 The Problem

When Claude Code compacts conversation history, it compresses earlier messages. Knowledge that was injected via hooks in those earlier messages is lost. The PreCompact hook has one chance to re-inject critical context before the compacted window replaces the originals.

### 9.2 What Must Be Re-Injected

Priority order (highest first, total budget <2000 tokens per PRODUCT-VISION.md):

1. **Active decisions** (~400 tokens): ADRs and decisions for the current feature cycle. These are entries with `category: "decision"` and `feature_cycle` matching the active feature.
2. **Session context** (~200 tokens): Current role, task, feature, in-progress work summary. Not from entries — this is session metadata.
3. **High-confidence injections** (~600 tokens): The top-N entries by confidence score from this session's injection history. These are entries the implant already determined were relevant.
4. **Cross-cutting conventions** (~400 tokens): Active conventions relevant to the current work. Retrieved via `context_briefing` for the active role/task.
5. **Correction chains** (~200 tokens): Any entry that was corrected during this session (supersedes chain). Ensures the agent doesn't revert to a corrected decision.

### 9.3 State Required

The CompactionState struct (Section 4.4) tracks:
- `injection_history`: Which entries were injected and when. Enables "re-inject the top-N" strategy.
- `feature_decisions`: Pre-computed list of active decisions for the current feature. Avoids a redb query at PreCompact time.
- `role`/`task`/`feature_cycle`: Context for `context_briefing` call if needed.

### 9.4 Computation Strategy

**Pre-computed vs. on-demand:** Pre-compute the compaction payload and update it incrementally on each injection. When PreCompact fires, the payload is ready — no redb queries needed, just format and emit.

**Update frequency:** On each injection, re-sort the injection history by confidence and recalculate the token budget allocation. The payload is a materialized view of "what I would re-inject right now." Cost: O(N log N) sort of injection history, where N is typically 30-60 entries. Sub-millisecond.

### 9.5 Daemon Memory vs. Sidecar File

For the daemon architecture: **daemon memory is sufficient and preferred.**
- The daemon holds one CompactionState per active session.
- Memory footprint per session: ~50 entry_ids * (8+8+8) bytes = ~1.2 KB plus strings. Negligible.
- Access latency: sub-microsecond (struct field read).

**If the implant is ephemeral (no daemon):** Sidecar file at `~/.unimatrix/{project_hash}/session-{id}.state`. Written on each injection (~30 bytes update to an mmap'd file or write-and-rename). Read on PreCompact. Deleted on SessionEnd. This adds ~1ms per injection write and ~2ms PreCompact read — within the 50ms budget.

**Recommendation:** Design for daemon architecture (memory-resident). Implement sidecar as fallback for environments where a daemon is impractical (CI, containers, cold-start-only mode).

---

## 10. Open Risks

### 10.1 redb Write Contention (HIGH)

If both the MCP server and the cortical implant write to the same redb database, they compete for the single-writer lock. redb serializes writes — one writer blocks the other. At hook frequency (every 10-30 seconds), this may cause the implant to block waiting for the MCP server's write transaction to complete, or vice versa.

**Mitigation options:**
- (a) Implant queues writes for the MCP server via IPC (no direct redb access for writes)
- (b) Implant opens its own redb file for telemetry tier, MCP server owns the knowledge tier
- (c) Accept serialized writes — at typical frequencies, blocking time is <1ms per transaction

**Required validation:** Prototype concurrent access pattern and measure latency under realistic hook frequency. This is RQ-2's domain.

### 10.2 Session Identity (MEDIUM)

Claude Code does not expose a session ID to hooks (as of ASS-011 research). The implant must infer session identity from:
- Parent PID (Claude Code process ID)
- Start time
- Or a UUID generated on SessionStart

If session identity is unreliable, injection logs cannot be grouped and signals cannot be attributed correctly.

**Mitigation:** Generate a UUID at SessionStart and pass it through all subsequent hook invocations via environment variable or state file. The implant daemon can track sessions by parent PID and map to UUID.

### 10.3 Signal Queue Unbounded Growth (LOW)

If signals are generated faster than they are consumed (broken consumer, long-running session with no maintenance), SIGNAL_QUEUE grows unboundedly.

**Mitigation:** Cap SIGNAL_QUEUE at 10,000 entries. If full, drop oldest signals (they are least valuable). Log a warning. Consumer runs on session end and every 5 minutes during active sessions.

### 10.4 Injection Log Volume in Long Sessions (LOW)

A very long session (8+ hours, 500+ prompts) generates 500+ injection records. At ~200 bytes each, that is ~100 KB — manageable. But the SessionRecord's `injected_entry_ids: Vec<u64>` could hold 500+ IDs, making the record large.

**Mitigation:** Cap `injected_entry_ids` at 200 entries (most recent). For compaction defense, only the last N injections matter. Older injection data is preserved in INJECTION_LOG for retrospective.

### 10.5 Schema Migration at Scale (LOW)

The v3->v4 migration rewrites all entries to add `session_id`. With the current ~170 entries, this completes in <100ms. For future knowledge bases with thousands of entries, this scan-and-rewrite could take seconds.

**Mitigation:** The established pattern (nxs-004) handles this well. Migration runs once on `Store::open()`, within a single transaction. For very large databases (10K+ entries), consider batched migration with progress logging. Not a concern for the near term.

### 10.6 Implicit vs. Explicit Signal Weighting (MEDIUM)

Implicit helpful signals (from session success) and explicit helpful signals (from agent MCP votes) have different reliability. Session success correlates with all injected entries being helpful, but many of those entries may have been neutral or even unhelpful — the session succeeded despite them, not because of them.

**Mitigation:** Track `SignalSource` on each signal. Allow the confidence formula (crt-002) to weight implicit and explicit signals differently. Default: implicit weight = 0.5 * explicit weight. This is a tunable parameter, not a data model issue, but the data model must support it — which it does via the `source` field on SignalRecord.

### 10.7 Backward Compatibility of JSONL Observation Path (MEDIUM)

col-002's retrospective pipeline currently reads JSONL files. The implant's INJECTION_LOG and SESSIONS tables are a different format and location. During the transition, both paths must coexist.

**Mitigation:** The retrospective pipeline should be extended to read from both JSONL files (legacy) and redb telemetry tables (new). The JSONL path can be deprecated after one release cycle where both coexist. The MetricVector computation in `unimatrix-observe` is format-independent — it operates on parsed `ObservationRecord` structs, which can be produced from either source.

---

## 11. Summary: Recommended Schema v4

### Tables (17 total)

| # | Table | Tier | Change |
|---|-------|------|--------|
| 1-14 | (existing) | Knowledge | Unchanged |
| 15 | SESSIONS | Telemetry | **NEW** |
| 16 | INJECTION_LOG | Telemetry | **NEW** |
| 17 | SIGNAL_QUEUE | Telemetry | **NEW** |

### EntryRecord (28 fields)

1-27: unchanged from v3
28: `session_id: String` (#[serde(default)]) **NEW**

### New Records

- SessionRecord (14 fields)
- InjectionRecord (6 fields)
- SignalRecord (5 fields)

### New Enums

- SessionStatus (4 variants)
- HookEventType (4 variants)
- InjectionReason (4 variants)
- SignalType (2 variants)
- SignalSource (3 variants)

### CompactionState (daemon memory, not persisted to redb)

7 fields tracking injection history and session context for compaction defense.

### Key Design Decisions

1. **Two-tier isolation:** Knowledge and telemetry are separate table namespaces in the same redb file. Telemetry never gets embedded or appears in search results.
2. **Ephemeral data has its own lifecycle:** Time-based GC independent of entry status lifecycle. 30-day default retention.
3. **Compaction defense state lives in process memory:** Fastest access, natural lifecycle, no persistence needed.
4. **Signal queue as write-ahead buffer:** Decouples signal generation (hook time) from signal application (batch time). Survives process restart.
5. **One new EntryRecord field:** `session_id` links entries to the session that created them. Minimal schema footprint.
6. **Backward compatible:** All existing MCP tools, agents, and workflows continue unchanged. The implant is purely additive.
