# ASS-015: UDS Architecture Patterns for Passive Knowledge Extraction

Research spike: architecture patterns for a Unimatrix Data Service (UDS) that
passively extracts knowledge from agent signals in a multi-agent development
orchestration system.

---

## Context: What Exists Today

The current Unimatrix system already has significant signal infrastructure:

| Component | Location | Role |
|-----------|----------|------|
| Hook process | `hook.rs` | Sync CLI, reads Claude Code stdin JSON, sends to UDS listener via UDS |
| UDS listener | `uds_listener.rs` | Tokio task, accepts hook connections, dispatches to handlers |
| SessionRegistry | `session.rs` | In-memory per-session state: injections, rework events, agent actions |
| SignalRecord | `signal.rs` | Confidence signals in signal_queue table (SQLite) |
| INJECTION_LOG | `injection_log.rs` | Persistent log of which entries were served to which sessions |
| SESSIONS | `sessions.rs` | Persistent session lifecycle records with GC |
| Signal consumers | `uds_listener.rs` | `run_confidence_consumer` and `run_retrospective_consumer` |
| unimatrix-observe | `crates/unimatrix-observe/` | 21 detection rules, metric vectors, retrospective reports |
| EventQueue | `event_queue.rs` | JSONL graceful-degradation queue for offline hook events |
| unimatrix-adapt | `crates/unimatrix-adapt/` | MicroLoRA adaptation of embeddings from co-access signals |

The system captures signals (tool use events, session outcomes, rework cycles,
co-access pairs) and uses them for:
1. **Confidence scoring** (helpful/flagged signals -> entry confidence)
2. **Retrospective analysis** (observation records -> hotspot detection)
3. **Embedding adaptation** (co-access pairs -> MicroLoRA fine-tuning)

What it does NOT do: **automatically extract new knowledge entries from signals**.

A session might reveal that "agents always struggle with the authentication
module" or "this particular test pattern works reliably" -- but today, only
explicit `context_store` calls capture knowledge. The UDS would close this gap.

---

## 1. In-Process Observer Pattern

### Description

Add signal capture hooks directly within the existing MCP request pipeline and
UDS listener. Knowledge extraction runs as background Tokio tasks within the
same process.

### Architecture

```
Claude Code hooks
       |
       v
  UDS Listener (tokio)
       |
       +-- SessionRegistry (in-memory state)
       |
       +-- SignalCapture (new)
       |       |
       |       v
       |   SignalBuffer (bounded channel or VecDeque behind Mutex)
       |       |
       |       v
       |   ExtractionWorker (tokio::spawn, runs periodically)
       |       |
       |       +-- Rule-based fast path
       |       +-- LLM slow path (batched)
       |       |
       |       v
       |   Store::insert (via spawn_blocking)
       |
       +-- existing confidence/retrospective consumers
```

### Signal Capture Points

1. **PostToolUse handler** (`uds_listener.rs`): After processing each tool
   event, clone the relevant fields into the signal buffer. Already implemented
   as `RecordEvent` / `RecordEvents` dispatch.

2. **SessionClose handler**: When `drain_and_signal_session` produces a
   `SignalOutput`, the complete session history (injection records, rework
   events, agent actions) becomes available. This is the richest signal source.

3. **context_search response** (`tools.rs`): The search query + returned
   results + helpful/unhelpful feedback form a pattern signal. The query itself
   reveals what agents are looking for but not finding.

4. **context_store calls**: What agents explicitly store reveals their
   understanding of what matters. Cross-referencing store content against
   session context can surface implicit patterns.

### Implementation Approach

```rust
// New in-process signal buffer
struct SignalBuffer {
    events: Mutex<VecDeque<ExtractableSignal>>,
    notify: tokio::sync::Notify,
}

enum ExtractableSignal {
    SessionCompleted {
        session_id: String,
        outcome: SessionOutcome,
        injected_entries: Vec<u64>,
        rework_events: Vec<ReworkEvent>,
        search_queries: Vec<String>,  // new: capture queries
        feature_cycle: Option<String>,
    },
    SearchMiss {
        query: String,
        result_count: usize,
        session_id: String,
    },
    RepeatedPattern {
        pattern_type: String,
        evidence: Vec<String>,
        frequency: u32,
    },
}
```

### How to Avoid Impacting MCP Response Latency

- Signal capture is a shallow clone + push to bounded VecDeque: <1us
- ExtractionWorker runs on a separate Tokio task, never blocks request handlers
- SQLite writes happen via `spawn_blocking` (same pattern as existing confidence)
- Bounded buffer (e.g., 10,000 entries) with oldest-drop on overflow (same
  pattern as SIGNAL_QUEUE cap)

### Assessment

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Feasibility | HIGH | Extends existing patterns (spawn_blocking, signal queue) |
| Complexity | LOW-MED | ~500-800 lines of new code |
| Quality potential | MEDIUM | Rule-based extraction is limited without LLM understanding |
| Latency impact | NEGLIGIBLE | Shallow clone + channel send in hot path |
| Risk | LOW | Single-process, no IPC, no concurrency hazards beyond existing Mutex |

### Pros
- No new processes, no IPC, no deployment changes
- Reuses existing Store/VectorIndex/EmbedService from the process
- Signal buffer is naturally co-located with signal sources
- Existing `spawn_blocking` pattern for SQLite writes is proven

### Cons
- Knowledge extraction rules compete for CPU with MCP request handling
- Cannot survive server restart (in-memory buffer is lost)
- LLM calls from within the server process could block the tokio runtime
  (must use spawn_blocking or dedicated thread pool)
- Testing extraction logic requires full server setup

---

## 2. Sidecar Process Pattern

### Description

A separate `unimatrix-uds` binary runs alongside the MCP server, reading
signals from a shared log and writing extracted knowledge to the store.

### Architecture

```
Claude Code hooks
       |
       v
  MCP Server (unimatrix-server)
       |
       +-- writes signals to shared log (file / UDS / shared memory)
       |
       v
  Shared Signal Log
       ^
       |
  UDS Sidecar (unimatrix-uds)
       |
       +-- reads signals
       +-- runs extraction rules + LLM batches
       +-- writes knowledge to ... where?
```

### Storage Constraint: SQLite Concurrency

**SQLite (with WAL mode) supports concurrent readers alongside a single writer.** This is a significant improvement over the previous redb backend which enforced exclusive database access (only one process could open the file). With SQLite:

- Multiple readers can operate concurrently with one writer
- WAL mode enables non-blocking reads during writes
- A sidecar process CAN open the same `.db` file for reads (though writes still serialize)

### IPC Options for the Sidecar

| Mechanism | Signal Transport | Knowledge Write-Back | Complexity |
|-----------|-----------------|---------------------|------------|
| Unix domain socket | Sidecar connects to MCP server's UDS | Request via existing HookRequest protocol | MEDIUM |
| File-based JSONL | Server appends to signal.jsonl, sidecar tails it | Sidecar sends store requests via UDS | LOW |
| Shared memory (mmap) | Lock-free ring buffer | Same UDS write-back | HIGH |
| Direct SQLite access | Sidecar reads DB directly (WAL mode) | Sidecar writes directly (serialized with server) | LOW-MEDIUM |

With SQLite, a **sidecar can read the database directly** (no UDS needed for reads). For writes, the sidecar can either write directly (SQLite serializes automatically) or route through UDS for consistency.

### Process Lifecycle

- Sidecar starts as a systemd service or launched by the MCP server on startup
- Discovers the server's UDS socket path from the project data directory
- Server must be running for the sidecar to write knowledge
- Graceful degradation: sidecar buffers extractions when server is down

### Assessment

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Feasibility | HIGH | SQLite WAL mode allows direct DB reads; writes serialize naturally |
| Complexity | MEDIUM | New binary, process lifecycle, but direct DB access simplifies IPC |
| Quality potential | MEDIUM | Same extraction logic, but decoupled from server state |
| Latency impact | NONE | Completely separate process |
| Risk | LOW-MEDIUM | Process coordination, startup ordering |

### Pros
- Complete isolation from MCP request path
- Can run heavy LLM extraction without any server impact
- Survives server restart (reads from persistent signal log)
- Can be upgraded independently
- SQLite WAL mode allows direct database reads without contention

### Cons
- Cannot access in-memory SessionRegistry state (only persisted signals)
- Two processes to deploy, monitor, and keep alive
- Signal log becomes a new persistence surface to manage
- Testing requires multi-process integration tests
- Write serialization with server process (SQLite handles this, but contention possible under load)

---

## 3. Event Sourcing Pattern

### Description

Treat all agent interactions as an append-only event log. The UDS becomes an
event processor that projects knowledge from the event stream.

### Architecture

```
All Agent Interactions
       |
       v
  Event Log (append-only)
       |
       +------+------+------+
       |      |      |      |
       v      v      v      v
  Confidence  Co-Access  Retrospective  Knowledge Extraction
  Projector   Projector  Projector      Projector (UDS)
```

### Mapping to Existing Infrastructure

The system already has elements of event sourcing:
- **SIGNAL_QUEUE**: append-only queue of confidence signals (capped at 10K)
- **INJECTION_LOG**: append-only log of entry injections per session
- **SESSIONS**: session lifecycle records
- **EventQueue (JSONL)**: offline event queue for hook degradation
- **Observation files**: JSONL session recordings used by retrospective

What's missing: a **unified event log** that captures ALL interactions in a
single, replayable stream.

### Proposed Event Log Schema

```rust
/// Unified event record for the event log.
#[derive(Serialize, Deserialize)]
pub struct UdsEvent {
    pub event_id: u64,          // monotonic
    pub timestamp: u64,          // unix millis
    pub session_id: String,
    pub event_type: UdsEventType,
}

#[derive(Serialize, Deserialize)]
pub enum UdsEventType {
    // Session lifecycle
    SessionStarted { role: Option<String>, feature: Option<String> },
    SessionEnded { outcome: String, duration_secs: u64 },

    // Tool use (from hooks)
    ToolUsePre { tool: String, input_summary: String },
    ToolUsePost { tool: String, response_size: u64, snippet: String },

    // Knowledge interactions (from MCP tools)
    SearchExecuted { query: String, result_count: usize, result_ids: Vec<u64> },
    EntryAccessed { entry_id: u64, helpful: Option<bool> },
    EntryStored { entry_id: u64, category: String, topic: String },
    EntryCorrected { original_id: u64, new_id: u64, reason: String },

    // Derived signals
    ReworkDetected { file_path: String, cycle_count: usize },
    ConfidenceSignal { entry_ids: Vec<u64>, signal_type: String },
}
```

### Storage Options for the Event Log

| Option | Pros | Cons |
|--------|------|------|
| New SQLite table (event_log) | Same database, same transaction model, indexed | Table grows unbounded; needs retention policy |
| Separate SQLite file | Isolation from knowledge store | Two databases to manage |
| JSONL files (like EventQueue) | Simple, human-readable, easy to replay | No indexing, sequential scan for replay, manual rotation |

**Recommended: SQLite table with retention policy** (same database, indexed, queryable).
With SQLite as the storage backend, an event_log table is natural — WAL mode handles concurrent reads during append-heavy writes efficiently. Replay happens
only during extraction or retrospective, not on the hot path.

### Signal Correlation Across Events

Event sourcing enables temporal correlation that isolated signals cannot:

```
SearchExecuted("how to handle auth errors") -> 0 results
  ... 15 minutes later ...
EntryStored(category="pattern", topic="auth", content="Error handling approach...")
```

This sequence reveals: the agent searched, found nothing, figured it out, and
stored the answer. The UDS could detect search-miss -> store sequences and
elevate the stored entry's confidence.

More patterns:
- **Repeated search misses**: Same query across sessions -> knowledge gap
- **Rework after injection**: Entry was served but session had rework -> entry quality issue
- **Store after correction**: Agent corrected an entry then stored a new one -> emerging convention
- **Cross-session convergence**: Multiple agents independently store similar content -> validated pattern

### Replay Capability

Event sourcing's killer feature: when extraction rules improve, replay the
entire event log to re-extract knowledge with better logic. No signal data is
lost.

### Assessment

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Feasibility | HIGH | Extends existing JSONL/EventQueue patterns |
| Complexity | MEDIUM | Unified event schema + projector framework |
| Quality potential | HIGH | Temporal correlation enables deep pattern detection |
| Latency impact | LOW | Append-only writes are fast; extraction is async |
| Risk | LOW-MED | Disk space management, event schema evolution |

### Pros
- Replay capability means no signal data is ever lost
- Temporal correlation reveals patterns invisible to isolated signals
- Clean separation between capture (append) and processing (project)
- Natural fit for batch LLM extraction (process event windows)
- Existing JSONL infrastructure (EventQueue, observation files) provides a template

### Cons
- Event log grows without bound (needs rotation/retention policy)
- Replay of large logs can be slow (mitigated by checkpointing)
- Schema evolution for events requires backward-compatible serialization
- Adds a third persistence surface (SQLite, vector files, event log)

---

## 4. LLM-in-the-Loop Extraction

### Description

Use Claude or another LLM to analyze accumulated signals and extract knowledge
entries that a rule-based system would miss. This is the differentiator that
could make the system GREAT rather than merely useful.

### Why LLM Extraction is the Key Differentiator

Rule-based extraction can detect patterns like "search query X returned 0
results" or "entry Y was flagged in 3 sessions." But only an LLM can:

1. **Synthesize meaning from context**: "The agent searched for 'auth error
   handling', found nothing, then spent 45 minutes reading 12 files and writing
   a try-catch pattern. The pattern it wrote is a reusable convention."

2. **Generalize from instances**: "Across 5 sessions, agents edited the same 3
   files in sequence. This suggests an implicit dependency that should be
   documented."

3. **Assess quality**: "This auto-extracted pattern contradicts entry #47, but
   the new pattern was used successfully in 3 sessions while #47 was flagged
   twice."

4. **Write well**: Rule-based extraction produces structured data. LLM
   extraction produces coherent, contextual knowledge entries that agents can
   immediately use.

### Batch Processing Architecture

```
Event Log / Signal Buffer
       |
       v
  Signal Accumulator (batches by time window or session)
       |
       v
  Batch Selector (picks highest-signal batches)
       |
       v
  Prompt Builder
       |  Constructs extraction prompt with:
       |  - Signal batch (events, outcomes, patterns)
       |  - Existing relevant knowledge (context_search)
       |  - Extraction instructions (what to look for)
       |
       v
  LLM API Call (Claude)
       |
       v
  Response Parser
       |  Extracts structured knowledge entries from LLM response:
       |  - title, content, topic, category, tags
       |  - confidence assessment
       |  - relationship to existing entries
       |
       v
  Quality Gate
       |  - Duplicate check (cosine similarity against existing)
       |  - Contradiction check (existing mechanism from crt-003)
       |  - Minimum confidence threshold
       |
       v
  Store as "proposed" status entries
```

### Prompt Design (Sketch)

```
You are analyzing agent interaction signals from a multi-agent development
system. Your task is to extract reusable knowledge entries.

## Signal Batch
{formatted_events}

## Session Context
- Feature: {feature_cycle}
- Outcome: {outcome}
- Duration: {duration}
- Rework detected: {rework_count} cycles

## Existing Knowledge (potentially related)
{existing_entries}

## Instructions
Identify any of the following:
1. Patterns: Recurring approaches that worked (or failed)
2. Conventions: Implicit rules that agents followed
3. Lessons: What went wrong and how it was resolved
4. Gaps: Knowledge that was searched for but not found

For each finding, provide:
- title: Concise name
- content: Detailed description with context
- category: pattern | convention | lesson-learned
- topic: Relevant system area
- tags: Relevant labels
- confidence: 0.0-1.0 (how certain is this finding?)
- rationale: Why this is worth recording

Return findings as a JSON array. Return an empty array if no significant
knowledge can be extracted from these signals.
```

### Cost Analysis

| Scenario | Signals/day | Batches/day | Tokens/batch | Cost/day (Haiku) |
|----------|-------------|-------------|--------------|------------------|
| Light use | 50-100 | 2-5 | ~2000 | ~$0.01 |
| Medium use | 500-1000 | 10-20 | ~3000 | ~$0.15 |
| Heavy use | 5000+ | 50-100 | ~4000 | ~$2.00 |

Using Haiku for extraction keeps costs negligible. Sonnet could be used for
high-signal batches where quality matters most.

### LLM Call Integration

```rust
// Async LLM extraction (runs in background task)
async fn extract_knowledge_batch(
    signals: Vec<ExtractableSignal>,
    existing_context: Vec<EntryRecord>,
    llm_client: &dyn LlmClient,
) -> Vec<ProposedEntry> {
    let prompt = build_extraction_prompt(&signals, &existing_context);

    let response = llm_client
        .complete(&prompt, LlmConfig {
            model: "claude-haiku",
            max_tokens: 4096,
            temperature: 0.3,  // Low temperature for factual extraction
        })
        .await?;

    parse_extraction_response(&response)
}

trait LlmClient: Send + Sync {
    async fn complete(&self, prompt: &str, config: LlmConfig) -> Result<String>;
}
```

### Assessment

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Feasibility | HIGH | HTTP API calls; no special infrastructure |
| Complexity | MEDIUM | Prompt engineering, response parsing, quality gates |
| Quality potential | VERY HIGH | LLM understands semantics, can synthesize and generalize |
| Cost | LOW | Haiku-level models at batch frequency = pennies/day |
| Risk | MEDIUM | LLM reliability, hallucination, response format stability |

### Pros
- Can extract knowledge that rules cannot (semantic understanding)
- Produces human-readable, contextual entries
- Can assess quality and relevance
- Cost is negligible with batch processing and Haiku

### Cons
- Requires API key management (but .env is the standard)
- LLM can hallucinate (mitigated by quality gates and "proposed" status)
- Response format may vary (mitigated by structured output / JSON mode)
- Network dependency for extraction (but degradation is graceful: buffer signals)
- Latency for individual extractions is 1-5 seconds (but this is background)

---

## 5. Hybrid Architecture (Recommended)

### Description

Combine rule-based fast path for high-confidence, well-defined patterns with
LLM slow path for ambiguous signals requiring semantic understanding.
Include human-in-the-loop for low-confidence extractions.

### Three-Tier Extraction Pipeline

```
Signal Buffer
       |
       v
  +-----------+
  | Tier 1:   |  Rule-based, immediate
  | Auto      |  Examples: search miss counting, rework flagging,
  | Extract   |  co-access pattern detection, duplicate store detection
  |           |  Confidence: >= 0.8
  |           |  -> Store as Active (source: "uds:auto")
  +-----------+
       |
       | (signals not handled by Tier 1)
       v
  +-----------+
  | Tier 2:   |  LLM batch, periodic (every N sessions or T minutes)
  | LLM       |  Examples: session narrative analysis, cross-session
  | Extract   |  pattern synthesis, knowledge gap identification
  |           |  Confidence: 0.5-0.8
  |           |  -> Store as Proposed (source: "uds:llm")
  +-----------+
       |
       | (low-confidence extractions)
       v
  +-----------+
  | Tier 3:   |  Human review
  | Propose   |  Surfaced in context_status or dedicated review tool
  |           |  Confidence: < 0.5
  |           |  -> Store as Proposed with review_needed tag
  +-----------+
```

### Tier 1: Rule-Based Auto-Extraction

These patterns can be detected with high confidence using rules alone:

| Pattern | Signal | Extraction |
|---------|--------|------------|
| Knowledge gap | Same search query, 0 results, >= 3 sessions | "Agents frequently search for '{query}' but no knowledge exists. Consider documenting this area." |
| Implicit convention | >= 5 sessions store entries with overlapping content | Merge/summarize into a single authoritative entry |
| Entry quality issue | Entry flagged in >= 3 rework sessions | Lower confidence + tag "needs-review" |
| Dead knowledge | Entry not accessed in 90+ days, confidence < 0.3 | Propose for deprecation |
| Emerging dependency | Same file set edited together in >= 5 sessions | "Files X, Y, Z form an implicit module" |

Implementation: extend existing detection rules (unimatrix-observe pattern).

```rust
pub trait ExtractionRule: Send {
    fn name(&self) -> &str;
    fn extract(&self, signals: &[ExtractableSignal]) -> Vec<ProposedEntry>;
}

fn default_extraction_rules() -> Vec<Box<dyn ExtractionRule>> {
    vec![
        Box::new(KnowledgeGapRule),
        Box::new(ImplicitConventionRule),
        Box::new(EntryQualityRule),
        Box::new(DeadKnowledgeRule),
        Box::new(EmergingDependencyRule),
    ]
}
```

### Tier 2: LLM Batch Extraction

Triggered by:
- Session close with "interesting" signals (high rework, many searches, long duration)
- Accumulation threshold (e.g., 10 unprocessed sessions)
- Periodic timer (e.g., every 30 minutes if signals exist)
- Manual trigger via new MCP tool `context_extract`

The LLM receives:
1. Accumulated signals since last extraction
2. Context from existing knowledge (via vector search on signal content)
3. Extraction prompt with structured output format

### Tier 3: Human-in-the-Loop

Proposed entries with low confidence or from novel domains get the `Proposed`
status and a `review_needed` tag. They surface in:
- `context_status` output (count of pending proposals)
- A new `context_review` tool that presents proposals for accept/reject
- Agent briefings that mention pending proposals for the relevant topic

### Confidence Tiers for Auto-Extracted Knowledge

```
Source              Initial Confidence    Status
─────────────────   ──────────────────    ──────
uds:auto (Tier 1)  0.6                   Active
uds:llm (Tier 2)   0.4                   Proposed
uds:propose (T3)   0.2                   Proposed
agent:explicit      0.5 (existing)        Active
```

Auto-extracted entries start with lower confidence than explicit stores,
allowing the existing confidence evolution system (crt-002) to naturally
promote or demote them based on usage signals.

### Provenance Metadata

```rust
// New fields on EntryRecord (appended per bincode positional contract)
pub trust_source: String,  // Already exists! Values: "agent", "uds:auto", "uds:llm", "uds:propose"
```

The `trust_source` field on EntryRecord already exists and is the natural
place to tag provenance. The `source` field can carry additional context
(e.g., "uds:auto:knowledge_gap_rule" or "uds:llm:batch-42").

### Quality Gates Before Auto-Stored Knowledge Becomes Searchable

1. **Near-duplicate check**: Cosine similarity >= 0.92 against existing entries
   (existing `DUPLICATE_THRESHOLD` constant). Reject if duplicate found.

2. **Contradiction check**: Run existing contradiction detection (crt-003)
   against the proposed entry. Flag if contradiction detected.

3. **Minimum content quality**: Title non-empty, content >= 50 chars,
   category in allowlist. (Existing validation in `validate_store_params`.)

4. **Confidence floor**: Auto-extracted entries below 0.2 confidence are
   discarded rather than stored.

5. **Rate limiting**: Maximum 10 auto-extractions per hour to prevent
   knowledge base pollution.

6. **Proposed status quarantine**: Tier 2 and Tier 3 entries are `Proposed`
   status, which is excluded from default search results (only `Active`
   entries appear). They must be explicitly promoted.

### Assessment

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Feasibility | HIGH | Combines proven patterns from existing codebase |
| Complexity | MEDIUM-HIGH | Three tiers + quality gates + provenance tracking |
| Quality potential | VERY HIGH | Rules for clear patterns, LLM for ambiguous, human for novel |
| Risk | LOW-MED | Graceful degradation at each tier |

---

## 6. SQLite Concurrency Analysis

### WAL Mode and Write Serialization

SQLite (now the default backend after nxs-006 migration from redb) uses WAL mode, which provides:

- Multiple concurrent readers alongside a single writer
- Non-blocking reads during writes (readers see pre-write state until commit)
- No exclusive file lock — multiple processes CAN open the same database
- Write transactions serialize automatically (SQLite handles the locking)

**Implications for UDS:**

1. **Sidecar with direct DB reads is now feasible** (Pattern 2 is less constrained)
2. **Write transactions should still be short** to minimize serialization delays
3. **Batch writes remain preferred** over individual writes (fewer transactions, less contention)

### Current Write Transaction Usage

The codebase already handles this well:

```
MCP request -> spawn_blocking -> SQL transaction -> commit
                                        |
                                (microseconds to low milliseconds)
```

Key write operations and their patterns:
- `insert_signal`: Single-record INSERT, fast
- `drain_signals`: Batch SELECT + DELETE, moderate
- `insert_injection_log_batch`: Batch INSERT with counter increment, fast
- `insert_session` / `update_session`: Single-record, fast
- Knowledge store (`context_store`): Entry + indexes + embeddings, moderate
- Confidence refresh (`maintain=true`): Batch of 100 entries, slow but opt-in

### Can the Observe Crate's Event Log Serve as the Signal Buffer?

Currently, observation files are JSONL files in `~/.unimatrix/{hash}/observations/`.
They are written by hook processes and read by the retrospective pipeline.

**Yes, with modifications:**

1. The observation JSONL files already capture tool-use events with timestamps,
   session IDs, and payloads. They are the natural signal buffer.

2. The UDS extraction pipeline could read these files the same way
   `discover_sessions` + `parse_session_file` already do.

3. New event types (search queries, knowledge interactions) would need to be
   appended to the observation files from within the MCP server process.

4. A watermark file (`last_extracted_ts`) would track how far extraction has
   progressed, enabling incremental processing.

### Write Pattern for Signals and Knowledge

Proposed write pattern:

```
Signals arrive (high frequency)
       |
       v
  INSERT into observations table (lightweight SQLite write)
       |
       | (background timer or threshold trigger)
       v
  Extraction pipeline runs (reads from observations)
       |
       v
  Batch of ProposedEntries
       |
       v
  SQLite transaction:
    - Insert all entries
    - Update all indexes
    - Insert vector embeddings
    - Commit
```

This pattern:
- Signal capture is a lightweight INSERT (SQLite WAL handles concurrent reads)
- Batches knowledge writes for efficiency
- All data in one database — enables JOIN-based correlation queries

### Transaction Contention Risk

With the recommended hybrid architecture, the UDS adds at most ~10 write
transactions per hour for auto-extracted knowledge. The existing system already
handles ~50-100 write transactions per MCP session (stores, signals,
injections, sessions). The marginal increase is negligible.

---

## 7. Integration Points

### Signal Capture in the MCP Request Pipeline

| Capture Point | What's Captured | Where in Code |
|---------------|----------------|---------------|
| `handle_session_register` | Session start, role, feature | `uds_listener.rs` |
| `handle_record_event` / `handle_record_events` | Tool use events (Pre/Post) | `uds_listener.rs` |
| `handle_context_search` | Search query, results, session | `uds_listener.rs` (ContextSearch handler) |
| `context_search` MCP tool | Search query, results, helpful flag | `tools.rs` (ContextSearch impl) |
| `context_store` MCP tool | Stored entry metadata | `tools.rs` (ContextStore impl) |
| `handle_session_close` | Session outcome, signal output | `uds_listener.rs` |
| `drain_and_signal_session` | Complete session history | `session.rs` |

**New capture needed:**

The MCP tool handlers in `tools.rs` currently do NOT write to the observations
table — they only interact with the knowledge tables. To capture knowledge interaction signals,
add observations INSERTs at:

1. After `context_search` returns results (query + result IDs + count)
2. After `context_store` completes (entry ID + category + topic)
3. After `context_correct` completes (original ID + new ID + reason)
4. When `helpful` parameter is provided on any retrieval tool

These INSERTs are cheap (SQLite WAL write, non-blocking for readers) and provide
the richest signal for knowledge extraction.

### Distinguishing Extracted vs Explicit Knowledge

```
EntryRecord.trust_source:
  "agent"       -> explicit context_store by an agent (existing)
  "human"       -> explicit store by human operator
  "uds:auto"    -> rule-based extraction (Tier 1)
  "uds:llm"     -> LLM extraction (Tier 2)
  "uds:propose" -> low-confidence proposal (Tier 3)

EntryRecord.source:
  "uds:auto:knowledge_gap_rule"    -> specific rule that produced it
  "uds:llm:batch-20260303-001"     -> specific extraction batch
  "agent:architect"                -> existing agent source (unchanged)
```

### Metadata Tagging for Provenance

New tags automatically added to UDS-extracted entries:

- `uds-extracted` -- all auto-extracted entries
- `extraction-rule:{rule_name}` -- which rule produced it (Tier 1)
- `extraction-batch:{batch_id}` -- which batch produced it (Tier 2)
- `review-needed` -- needs human review (Tier 3)
- `derived-from:session:{session_id}` -- source session(s)
- `derived-from:entry:{entry_id}` -- if extracted by observing an existing entry

### Quality Gates

```
  ProposedEntry
       |
       v
  [1] Content validation (min length, valid category, no injection)
       |
       v
  [2] Near-duplicate check (cosine sim >= 0.92 -> reject)
       |
       v
  [3] Contradiction check (crt-003 mechanism -> flag + lower confidence)
       |
       v
  [4] Rate limit check (max 10/hour auto-extractions)
       |
       v
  [5] Confidence floor (< 0.2 -> discard)
       |
       v
  Store with appropriate status:
    Tier 1 (>= 0.6 confidence): Active
    Tier 2 (0.4-0.6):           Proposed
    Tier 3 (< 0.4):             Proposed + review_needed tag
```

---

## Recommended Architecture: Event-Sourced Hybrid

Combine patterns 3, 4, and 5 with pattern 1 as the execution model:

### Final Design

```
                    Claude Code Hooks
                          |
                          v
                   UDS Listener (existing)
                          |
         +----------------+----------------+
         |                |                |
    Tool Events      Session Events    Knowledge Events
         |                |                |
         v                v                v
    +-----------------------------------------+
    |       Unified Event Log (JSONL)         |
    |  Append-only, rotated, with watermarks  |
    +-----------------------------------------+
              |                    |
              v                    v
    +------------------+  +-------------------+
    | Tier 1: Rules    |  | Tier 2: LLM      |
    | (in-process,     |  | (in-process,      |
    |  on session      |  |  batched,         |
    |  close)          |  |  periodic)        |
    +------------------+  +-------------------+
              |                    |
              v                    v
    +-----------------------------------------+
    |         Quality Gate Pipeline            |
    |  Dedup -> Contradiction -> Rate Limit   |
    +-----------------------------------------+
              |
              v
    +-----------------------------------------+
    |    Store (spawn_blocking + SQLite txn)    |
    |  Status: Active | Proposed              |
    |  Source: uds:auto | uds:llm | uds:propose|
    +-----------------------------------------+
```

### Why In-Process (Not Sidecar)

1. In-process is simplest (sidecar adds deployment complexity even with SQLite's relaxed locking)
2. The MCP server already runs a tokio runtime with spawn_blocking for DB ops
3. Signal sources are co-located in the server process
4. Existing patterns (confidence consumers, retrospective) prove this works

### Why Event Sourcing

1. Replay capability means extraction logic can improve without losing signals
2. JSONL files are cheap, human-readable, and match existing observation patterns
3. Clean separation between signal capture (fast, append) and processing (async)
4. Natural batch boundaries for LLM extraction

### Why Hybrid Tiers

1. Rules handle the 80% of clear patterns instantly
2. LLM handles the 20% that requires semantic understanding
3. Human review catches what both miss
4. Confidence tiers let the existing evolution system (crt-002) naturally
   promote good extractions and demote bad ones

### Implementation Phases

**Phase 1: Signal Capture** (~300 lines)
- Add knowledge interaction events to observation JSONL files
- Extend ObservationRecord with new event types (SearchQuery, KnowledgeStore, etc.)
- Add watermark tracking for incremental processing

**Phase 2: Rule-Based Extraction** (~500 lines)
- ExtractionRule trait (mirror DetectionRule from unimatrix-observe)
- 5 initial rules: knowledge gap, implicit convention, entry quality, dead knowledge, emerging dependency
- Quality gate pipeline
- Integration with session close handler

**Phase 3: LLM Extraction** (~800 lines)
- LlmClient trait + Claude API implementation
- Prompt builder with signal batch formatting
- Response parser with structured JSON extraction
- Batch scheduler (session-count or time-based triggers)

**Phase 4: Review and Feedback** (~400 lines)
- `context_review` MCP tool for human review of proposals
- Feedback loop: accepted/rejected proposals train extraction confidence
- Dashboard integration in `context_status`

### Risk Mitigations

| Risk | Mitigation |
|------|------------|
| Knowledge pollution | Rate limiting + quality gates + Proposed status |
| LLM hallucination | Structured output + dedup check + low initial confidence |
| Performance impact | All extraction is async; signal capture is append-only |
| Storage bloat | Event log rotation (7-day retention) + entry count caps |
| API key dependency | Tier 1 works without any API key; Tier 2 degrades gracefully |
| Schema evolution | Event types use serde(tag) with unknown-field tolerance |

---

## Appendix: Comparison Matrix

| Pattern | Feasibility | Complexity | Quality | Latency Impact | SQLite Compatible |
|---------|-------------|------------|---------|----------------|-------------------|
| 1. In-Process Observer | HIGH | LOW-MED | MEDIUM | NEGLIGIBLE | YES |
| 2. Sidecar Process | HIGH | MEDIUM | MEDIUM | NONE | YES (WAL mode enables direct reads) |
| 3. Event Sourcing | HIGH | MEDIUM | HIGH | LOW | YES (SQLite table or JSONL) |
| 4. LLM-in-the-Loop | HIGH | MEDIUM | VERY HIGH | NONE (async) | YES |
| 5. Hybrid (recommended) | HIGH | MED-HIGH | VERY HIGH | NEGLIGIBLE | YES |
| 6. SQLite concurrency | N/A | N/A | N/A | N/A | Informs all patterns |

**Recommendation: Pattern 5 (Hybrid) built on Pattern 3 (Event Sourcing)
executed via Pattern 1 (In-Process), incorporating Pattern 4 (LLM) for the
semantic extraction tier.**
