# ASS-016 Part 2: Retrospective Data Architecture & SQLite Migration

**Status**: Research In Progress
**Date**: 2026-03-04
**Predecessor**: [storage-assessment.md](./storage-assessment.md) (2026-03-03)
**Scope**: Two-part analysis: (1) data architecture for richer retrospective analytics, (2) zero-functionality SQLite migration plan

---

## Part 1: Retrospective Analytics — Current State & Opportunities

### 1.1 What We Have Today

The retrospective system (col-002 + col-002b + col-010b) is a complete observation pipeline:

**Data Collection**: 4 Claude Code hooks → JSONL files in `~/.unimatrix/observation/`

| Hook | Captures | Record Fields |
|------|----------|---------------|
| PreToolUse | Tool name, input object | ts, hook, session_id, tool, input |
| PostToolUse | Response size, first 500 chars | + response_size, response_snippet |
| SubagentStart | Agent type, prompt snippet | ts, hook, session_id, tool (agent_type) |
| SubagentStop | Agent result (often empty) | ts, hook, session_id |

**Analysis Pipeline**: `context_retrospective` MCP tool

1. Discover session JSONL files
2. Parse records, attribute to feature cycle (content-based: file paths → task subjects → git branches)
3. Run 21 detection rules across 4 categories (Friction: 4, Session: 5, Agent: 7, Scope: 5)
4. Compute 22 universal metrics + dynamic per-phase metrics
5. Compare against historical baselines (mean + 1.5σ outlier flagging, ≥3 history required)
6. Generate recommendations (4 recognized hotspot types)
7. Store MetricVector in OBSERVATION_METRICS (bincode, one row per feature cycle)

**Detection Rules (21 total)**:

| Category | Rules | What They Detect |
|----------|-------|-----------------|
| Friction (4) | permission_retries, sleep_workarounds, search_via_bash, output_parsing_struggle | Developer friction with tooling |
| Session (5) | session_timeout, cold_restart, coordinator_respawns, post_completion_work, rework_events | Session health problems |
| Agent (7) | context_load, lifespan, file_breadth, reread_rate, mutation_spread, compile_cycles, edit_bloat | Agent behavior anti-patterns |
| Scope (5) | source_file_count, design_artifact_count, adr_count, post_delivery_issues, phase_duration_outlier | Scope creep indicators |

### 1.2 Data We Capture But Don't Analyze

Three DB tables exist with structured event data that the retrospective pipeline **does not consume**:

**SESSIONS table** (col-010, schema v5):
```
session_id → SessionRecord {
    feature_cycle, agent_role, started_at, ended_at,
    status (Active/Completed/TimedOut/Abandoned),
    compaction_count, outcome ("success"/"rework"/"abandoned"),
    total_injections
}
```

**INJECTION_LOG table** (col-010, schema v5):
```
log_id → InjectionLogRecord {
    session_id, entry_id, confidence, timestamp
}
```

**SIGNAL_QUEUE table** (col-009, schema v4):
```
signal_id → SignalRecord {
    session_id, created_at, entry_ids,
    signal_type (Helpful/Flagged),
    signal_source (ImplicitOutcome/ImplicitRework)
}
```

Additionally, **AUDIT_LOG** records every MCP tool call and **CO_ACCESS** tracks entry pair co-retrieval — neither is used in retrospective analysis.

### 1.3 The Structured Events Path (Designed, Not Implemented)

The RetrospectiveReport already has fields for richer analysis:
- `entries_analysis: Option<Vec<EntryAnalysis>>` — per-entry performance across sessions
- `narratives: Option<Vec<HotspotNarrative>>` — clustered evidence with file rankings
- Both are `None` on the current JSONL-only path

The `EntryAnalysis` struct tracks:
```rust
entry_id, title, category,
rework_flag_count,      // How often this entry was flagged during rework
injection_count,        // How many sessions received this entry
success_session_count,  // Sessions with successful outcomes
rework_session_count    // Sessions requiring rework
```

This is the foundation for answering: **"Does this knowledge entry actually help?"**

### 1.4 Data Architecture for Richer Retrospective

To unlock the full analytical potential, the retrospective needs to integrate three data paths:

```
┌──────────────────────┐     ┌───────────────────────┐     ┌──────────────────┐
│   JSONL Path         │     │  Structured Events    │     │  Knowledge Graph  │
│   (tool telemetry)   │     │  (session lifecycle)  │     │  (entry network)  │
├──────────────────────┤     ├───────────────────────┤     ├──────────────────┤
│ ObservationRecord[]  │     │ SESSIONS              │     │ CO_ACCESS        │
│ - tool calls         │     │ INJECTION_LOG         │     │ FEATURE_ENTRIES  │
│ - response sizes     │     │ SIGNAL_QUEUE          │     │ OUTCOME_INDEX    │
│ - file paths touched │     │ AUDIT_LOG             │     │ ENTRIES (scores) │
│ - timing gaps        │     │                       │     │                  │
└──────────┬───────────┘     └───────────┬───────────┘     └────────┬─────────┘
           │                             │                          │
           └─────────────┬───────────────┘──────────────────────────┘
                         │
              ┌──────────▼──────────────┐
              │   Unified Analysis      │
              │   Engine                │
              │                         │
              │  Current (21 rules):    │
              │  - Tool behavior        │
              │  - Session patterns     │
              │  - Scope indicators     │
              │                         │
              │  New (with integration):│
              │  - Entry effectiveness  │
              │  - Knowledge drift      │
              │  - Agent-entry coupling │
              │  - Session health score │
              │  - Cross-feature trends │
              └─────────────────────────┘
```

### 1.5 New Analyses Enabled by Data Integration

#### Tier 1: Available with current data, needs code integration

**A. Entry Effectiveness Scoring**
- Source: INJECTION_LOG + SESSIONS (outcome) + SIGNAL_QUEUE (Flagged)
- Question: "Which knowledge entries correlate with successful vs rework outcomes?"
- Metric: `entry_success_rate = success_sessions / (success_sessions + rework_sessions)`
- Output: Per-entry EntryAnalysis in retrospective report (struct already exists)
- Tables touched: SESSIONS, INJECTION_LOG, SIGNAL_QUEUE
- **Value**: Identifies knowledge entries that are actively harmful (misleading, outdated)

**B. Session Health Score**
- Source: SESSIONS + JSONL metrics
- Score: composite of `(1 - friction_hotspot_ratio) × outcome_factor × (1 - cold_restart_penalty)`
- Output: Per-session health ranking, worst-session deep-dive
- Tables touched: SESSIONS + existing JSONL analysis
- **Value**: Quickly identifies which sessions were problematic and why

**C. Knowledge Injection Patterns**
- Source: INJECTION_LOG + ENTRIES (confidence)
- Question: "Are we injecting high-confidence entries? Do low-confidence entries get injected by mistake?"
- Metric: avg confidence at injection time, injection frequency per entry
- Tables touched: INJECTION_LOG
- **Value**: Validates the confidence scoring system is actually useful

**D. Agent Role Inference** (heuristic, platform-constrained)
- Source: JSONL (tool sequence patterns within subagent brackets)
- Heuristic: Read-heavy → researcher, Edit-heavy → implementer, Bash-heavy → tester
- Limitation: SubagentStop often has empty agent_type (26/31 observed)
- **Value**: Better attribution despite shared session_id constraint

#### Tier 2: Requires new tables or significant new code

**E. Cross-Feature Trending**
- Source: OBSERVATION_METRICS (all historical MetricVectors)
- Computation: Time-series of metrics per phase across feature cycles
- New storage: `METRIC_TIMESERIES` table or extend OBSERVATION_METRICS
- **Value**: "Are we getting faster at phase 3b over time?"

**F. Compound Signal Detection**
- Source: All hotspot findings from detection rules
- Computation: Correlation matrix between co-occurring hotspots
- Example: high context_load + cold_restart → "agent lost context, had to rebuild"
- New storage: Optional (could be computed on-the-fly or cached)
- **Value**: Root-cause analysis vs symptom listing

**G. Threshold Convergence**
- Source: Historical hotspot findings + dismissed-hotspot feedback
- Computation: Adapt rule thresholds toward project norms (mean + 1.5σ of actual measurements)
- New storage: `THRESHOLD_HISTORY` or per-rule threshold table
- **Value**: Reduces false positives, becomes project-specific over time

**H. Audit Trail Analytics**
- Source: AUDIT_LOG
- Computation: Agent behavior patterns (which agents call which tools most?), error rates, latency distribution
- New storage: None (AUDIT_LOG already captured)
- **Value**: Operational health monitoring of the knowledge engine itself

### 1.6 Table Count Impact

| Data Integration | New Tables Needed | redb Friction |
|-----------------|-------------------|---------------|
| Tier 1 (A-D) | 0 | None — data already in existing tables |
| E: Cross-Feature Trending | 0-1 | Minor if storing time-series separately |
| F: Compound Signals | 0 | Computed on-the-fly |
| G: Threshold Convergence | 1 | New `THRESHOLD_HISTORY` table |
| H: Audit Analytics | 0 | AUDIT_LOG already exists |

**Key insight**: Most of the analytical value requires **code integration** (connecting existing tables), not new tables. The pressure to add tables comes from:
1. Threshold convergence history (1 new table)
2. Any future structured analytics caching
3. Potential denormalized views for query performance

This is a weaker case for "storage is the bottleneck" than initially assumed. The bigger issue is **query complexity**: joining SESSIONS + INJECTION_LOG + ENTRIES for entry effectiveness requires multi-table reads that are painful in redb (separate transactions, client-side joins, no SQL).

### 1.7 The Real Driver for SQLite

The retrospective analytics argument for SQLite isn't "we need more tables." It's:

1. **Multi-table correlation queries** are natural in SQL, painful in KV:
   ```sql
   -- Entry effectiveness: trivial in SQL
   SELECT e.id, e.title,
     COUNT(CASE WHEN s.outcome = 'success' THEN 1 END) as success_count,
     COUNT(CASE WHEN s.outcome = 'rework' THEN 1 END) as rework_count
   FROM injection_log il
   JOIN sessions s ON il.session_id = s.session_id
   JOIN entries e ON il.entry_id = e.id
   WHERE s.feature_cycle = ?
   GROUP BY e.id
   ```
   In redb: scan INJECTION_LOG, scan SESSIONS, point-lookup ENTRIES, client-side join.

2. **Aggregation queries** for trending and baselines:
   ```sql
   -- Phase duration trending: trivial in SQL
   SELECT feature_cycle, AVG(duration_secs), STDDEV(duration_secs)
   FROM phase_metrics
   WHERE phase_name = '3b'
   GROUP BY feature_cycle
   ORDER BY computed_at
   ```
   In redb: deserialize every MetricVector, extract phase, compute in Rust.

3. **Future FTS5** for content search within observation records (if we ever want to search tool inputs/outputs for patterns beyond regex).

---

## Part 2: Zero-Functionality SQLite Migration

### 2.1 Scope: What "Zero Functionality Change" Means

Replace redb with SQLite as the storage backend. Every current behavior preserved exactly:
- All 10 MCP tools return identical results
- All 17 tables' data preserved
- Semantic search works identically (HNSW stays in-memory, VECTOR_MAP bridge pattern preserved)
- Schema migration logic works
- Bincode serialization for blobs unchanged
- Confidence scoring, co-access boosting, signal queue — all unchanged

What changes: the storage implementation inside `crates/unimatrix-store/`.

### 2.2 Abstraction Boundary Analysis

The migration is well-scoped because of an existing clean boundary:

```
unimatrix-server  ─┐
unimatrix-engine  ─┼─→ EntryStore trait (unimatrix-core) ─→ StoreAdapter ─→ Store (redb)
unimatrix-vector  ─┘                                                          ↑
                                                                     ONLY THIS CHANGES
```

**Crates with zero changes**: unimatrix-engine, unimatrix-server, unimatrix-vector, unimatrix-core (adapter struct stays, just wraps different inner type).

**Crate with all changes**: unimatrix-store

### 2.3 Store API Surface (34 Methods)

| Category | Count | Methods |
|----------|-------|---------|
| CRUD | 4 | insert, update, update_status, delete |
| Reads | 15 | get, exists, query, query_by_*, get_vector_mapping, iter_vector_mappings, read_counter, get_metrics, list_all_metrics, co_access queries |
| Writes | 11 | record_usage*, update_confidence, put_vector_mapping, record_feature_entries, record_co_access_pairs, cleanup_stale_co_access, store_metrics, rewrite_vector_map, signal queue ops |
| Lifecycle | 4 | open, open_with_config, compact, begin_read/begin_write |

### 2.4 SQLite Schema (1:1 Mapping)

Every redb table maps to a SQLite table. No structural transformation needed:

```sql
-- Primary storage (blob stays bincode)
CREATE TABLE entries (id INTEGER PRIMARY KEY, data BLOB NOT NULL);

-- 5 index tables (would be CREATE INDEX in a normalized schema,
-- but keeping as tables for zero-change migration)
CREATE TABLE topic_index (topic TEXT NOT NULL, entry_id INTEGER NOT NULL, PRIMARY KEY (topic, entry_id));
CREATE TABLE category_index (category TEXT NOT NULL, entry_id INTEGER NOT NULL, PRIMARY KEY (category, entry_id));
CREATE TABLE tag_index (tag TEXT NOT NULL, entry_id INTEGER NOT NULL, PRIMARY KEY (tag, entry_id));
CREATE TABLE time_index (timestamp INTEGER NOT NULL, entry_id INTEGER NOT NULL, PRIMARY KEY (timestamp, entry_id));
CREATE TABLE status_index (status INTEGER NOT NULL, entry_id INTEGER NOT NULL, PRIMARY KEY (status, entry_id));

-- Vector bridge (unchanged semantics)
CREATE TABLE vector_map (entry_id INTEGER PRIMARY KEY, hnsw_data_id INTEGER NOT NULL);

-- Counters
CREATE TABLE counters (name TEXT PRIMARY KEY, value INTEGER NOT NULL);

-- Agent management
CREATE TABLE agent_registry (agent_id TEXT PRIMARY KEY, data BLOB NOT NULL);

-- Audit trail
CREATE TABLE audit_log (event_id INTEGER PRIMARY KEY, data BLOB NOT NULL);

-- Feature tracking
CREATE TABLE feature_entries (feature_id TEXT NOT NULL, entry_id INTEGER NOT NULL, PRIMARY KEY (feature_id, entry_id));

-- Co-access (symmetric, enforced ordering)
CREATE TABLE co_access (
    entry_id_a INTEGER NOT NULL,
    entry_id_b INTEGER NOT NULL,
    data BLOB NOT NULL,
    PRIMARY KEY (entry_id_a, entry_id_b),
    CHECK (entry_id_a < entry_id_b)
);

-- Outcome tracking
CREATE TABLE outcome_index (feature_cycle TEXT NOT NULL, entry_id INTEGER NOT NULL, PRIMARY KEY (feature_cycle, entry_id));

-- Observation metrics
CREATE TABLE observation_metrics (feature_cycle TEXT PRIMARY KEY, data BLOB NOT NULL);

-- Signal queue
CREATE TABLE signal_queue (signal_id INTEGER PRIMARY KEY, data BLOB NOT NULL);

-- Sessions
CREATE TABLE sessions (session_id TEXT PRIMARY KEY, data BLOB NOT NULL);

-- Injection log
CREATE TABLE injection_log (log_id INTEGER PRIMARY KEY, data BLOB NOT NULL);

-- Secondary indexes for efficient lookups
CREATE INDEX idx_co_access_b ON co_access(entry_id_b);
CREATE INDEX idx_injection_session ON injection_log(data);  -- see note below
```

**Note on INJECTION_LOG index**: The current session GC cascade scans all of INJECTION_LOG to find matching session_ids. With SQLite, we could add a `session_id` column (denormalized from the blob) for indexed lookups. But that's a schema enhancement, not zero-change. For zero-change, the blob scan approach works (or deserialize-and-filter, same as today).

### 2.5 What Changes File by File

| File | Current Lines | Change % | What Changes |
|------|--------------|---------|-------------|
| `db.rs` | 533 | 100% | redb::Builder → rusqlite::Connection, table creation → CREATE TABLE, transaction wrappers |
| `write.rs` | ~1800 | 80-90% | redb table.insert → SQL INSERT, index maintenance → SQL INSERT into index tables, same logic |
| `read.rs` | ~925 | 80-90% | redb range scan → SQL SELECT WHERE, same filter logic |
| `migration.rs` | 1421 | 20% | Version tracking unchanged, entry rewriting unchanged (still bincode→bincode) |
| `counter.rs` | 57 | 100% | Trivial rewrite to SQL |
| `schema.rs` | 657 | 10% | Remove redb table type definitions, keep EntryRecord/bincode logic |
| `query.rs` | 319 | 50% | Set intersection may move to SQL INTERSECT or stay client-side |
| `Cargo.toml` | - | - | `redb` → `rusqlite` with `bundled` feature |

**Files untouched**: Everything in unimatrix-core, unimatrix-engine, unimatrix-server, unimatrix-vector.

### 2.6 Risk Areas and Mitigations

#### Risk 1: Transaction Semantics Divergence
**redb**: Typed ReadTransaction/WriteTransaction with MVCC. Multiple concurrent readers, one writer.
**SQLite (WAL mode)**: Same concurrency model. One writer, concurrent readers.
**Risk**: Subtle differences in transaction isolation or error handling.
**Mitigation**: SQLite WAL mode has equivalent semantics. Test every transaction pattern.

#### Risk 2: CO_ACCESS Key Ordering
**redb**: Tuple key `(u64, u64)` with code enforcing `min < max`.
**SQLite**: Two columns with CHECK constraint.
**Risk**: Ordering bug could create duplicate pairs.
**Mitigation**: CHECK constraint + code-level `co_access_key()` helper unchanged.

#### Risk 3: Multimap Table Semantics (TAG_INDEX, FEATURE_ENTRIES)
**redb**: MultimapTable — native one-to-many via `table.get(key)` returning iterator.
**SQLite**: Regular table with composite PK.
**Risk**: Different iteration patterns could miss edge cases.
**Mitigation**: Same client-side HashSet intersection logic. SQL just provides the sets.

#### Risk 4: Counter Atomicity
**redb**: Read-increment-write in single write transaction.
**SQLite**: `UPDATE counters SET value = value + 1 WHERE name = ?` — atomic in transaction.
**Risk**: None significant. SQLite's approach is actually simpler.
**Mitigation**: Test concurrent ID generation.

#### Risk 5: Bincode Blob Compatibility
**redb**: Stores `&[u8]` directly.
**SQLite**: Stores as BLOB.
**Risk**: None. Bytes are bytes.
**Mitigation**: Roundtrip tests for every bincode-serialized type.

#### Risk 6: Performance Regression for Batch Operations
**redb**: Individual fsync'd writes are fastest (920ms vs 7,040ms SQLite at 1M scale).
**SQLite**: Batch writes in single transaction are efficient. Individual fsync'd writes are slow.
**Risk**: Signal queue drain, batch co-access updates could be slower.
**Mitigation**: Wrap batch operations in single transaction (which we already do). At our scale (~50 entries), difference is microseconds.

#### Risk 7: File Locking Differences
**redb**: Uses its own file locking (MVCC COW pages).
**SQLite**: WAL mode with `-wal` and `-shm` files alongside main `.db`.
**Risk**: PidGuard (vnc-004) assumes single database file. SQLite creates 3 files.
**Mitigation**: PidGuard guards the process, not the file. No change needed.

### 2.7 Migration Strategy: Reduce Risk

#### Phase 0: Dual-Backend Trait Test Harness (1-2 days)
Before touching production code, create a test harness that validates both backends produce identical results:

```rust
#[cfg(test)]
fn run_parity_test(test_fn: impl Fn(&dyn EntryStore)) {
    // Run against redb
    let redb_store = Store::open_redb(tempfile());
    test_fn(&StoreAdapter::new(redb_store));

    // Run against sqlite
    let sqlite_store = Store::open_sqlite(tempfile());
    test_fn(&StoreAdapter::new(sqlite_store));
}
```

This catches divergence immediately.

#### Phase 1: SQLite Store Implementation (3-5 days)
Implement `SqliteStore` with identical method signatures. Keep `redb::Store` untouched. Both implementations exist side by side.

Files created:
- `crates/unimatrix-store/src/sqlite/mod.rs`
- `crates/unimatrix-store/src/sqlite/db.rs`
- `crates/unimatrix-store/src/sqlite/read.rs`
- `crates/unimatrix-store/src/sqlite/write.rs`

Cargo.toml: Add `rusqlite` as dependency alongside `redb`. Feature-flag if desired.

#### Phase 2: Parity Testing (2-3 days)
Run all 234 store tests against both backends. Every test must pass on both. This is the critical risk-reduction step — if parity tests pass, the migration is safe.

#### Phase 3: Data Migration Tooling (1 day)
Write `redb_to_sqlite` export/import:
1. Open redb database
2. Read every table
3. Write to SQLite using new implementation
4. Verify row counts match

#### Phase 4: Cutover (1 day)
- Make `SqliteStore` the default in `Store::open()`
- Remove redb dependency
- Clean up dual-backend test infrastructure (keep parity test as integration test)

#### Phase 5: Cleanup (1 day)
- Remove `crates/unimatrix-store/src/db.rs` (old redb code)
- Remove `crates/unimatrix-store/src/write.rs` (old redb code)
- Flatten sqlite module into main store

**Total: 8-12 days of focused work (2-3 feature cycles)**

### 2.8 What NOT To Do During Migration

1. **Don't normalize the schema**. Keep entries as blobs. Normalization is a separate feature.
2. **Don't replace the 5 index tables with CREATE INDEX**. That changes query code paths. Do it in a follow-up.
3. **Don't replace HNSW with sqlite-vec**. That changes the vector pipeline. Separate feature.
4. **Don't add the injection_log session_id column**. Schema enhancement, not migration.
5. **Don't change bincode to JSON**. Serialization format is orthogonal to storage engine.

Each of these is valuable but should be a separate, testable change after the engine swap is proven stable.

### 2.9 Post-Migration Opportunities (Future Work)

Once on SQLite, these become low-effort improvements:

| Improvement | Effort | Value |
|------------|--------|-------|
| Replace 5 index tables with CREATE INDEX on entries columns | Medium | Eliminates ~300 lines of index sync code |
| Add session_id column to injection_log | Low | Enables indexed session GC cascade |
| SQL-based co-access reverse lookup | Low | Eliminates full table scan |
| FTS5 for content search within entries | Medium | Full-text search without embedding |
| sqlite-vec for small-scale vector search | Medium | Could replace HNSW for <1K entries |
| Denormalized entry columns (topic, category, status) | Medium | Enables SQL JOINs, eliminates client-side merges |

---

## Recommendations

### R1: Start with SQLite Migration (Zero-Change)
The retrospective analytics improvements (Part 1, Tier 1) require multi-table correlation queries that are painful in redb. Migrating first gives us the query foundation, then analytics improvements become straightforward SQL.

### R2: Use Dual-Backend Parity Testing
The highest-risk reduction comes from running identical tests against both backends simultaneously. This catches subtle semantic differences before they reach production.

### R3: Keep Bincode Blobs Initially
Don't combine the storage engine swap with a serialization format change. bincode→BLOB works identically in SQLite. Normalization can come later.

### R4: Feature Sequence After Migration

```
Step 1: SQLite migration (zero-change)           ← this research
Step 2: Integrate SESSIONS + INJECTION_LOG        ← Tier 1 analytics
        into retrospective pipeline
Step 3: Entry effectiveness scoring               ← EntryAnalysis population
Step 4: Schema normalization (optional)           ← index tables → SQL indexes
Step 5: Cross-feature trending                    ← Tier 2 analytics
```

### R5: Relationship to Server Refactoring

The storage-assessment.md (R6) recommended server refactoring before storage migration. That recommendation stands — the service layer cleanup reduces the migration surface. However, if the server refactoring is deferred, the migration is still tractable because the `EntryStore` trait boundary already isolates consumers.

---

## Appendix A: Observation Data Available for Analytics

### Hook-Captured Fields (per tool call)

| Field | Source | Available In |
|-------|--------|-------------|
| Timestamp (ms) | Hook script | JSONL |
| Session ID | Claude Code platform | JSONL |
| Tool name | Hook input | JSONL |
| Tool input (full JSON) | Hook input | JSONL |
| Response size (bytes) | PostToolUse | JSONL |
| Response snippet (500 chars) | PostToolUse | JSONL |
| Agent type | SubagentStart | JSONL |

### Derived Signals (from attribution engine)

| Signal | Derivation | Used By |
|--------|-----------|---------|
| Feature cycle | File path / task subject / git branch | Attribution |
| Phase name | Task subject parsing | Phase metrics |
| Completion boundary | TaskUpdate with status=completed | Post-completion detection |
| File paths touched | Read/Write/Edit input parsing | Breadth/reread rules |
| Bash command type | Regex on Bash input | Search-via-bash, compile cycles |

### Structured Events (DB tables, not yet in retrospective)

| Table | Key Data | Analytical Value |
|-------|----------|-----------------|
| SESSIONS | outcome, lifecycle status, injection count | Session health, outcome correlation |
| INJECTION_LOG | entry_id, confidence at injection time | Entry effectiveness, confidence validation |
| SIGNAL_QUEUE | entry_ids, Helpful/Flagged signal type | Entry performance signals |
| AUDIT_LOG | all MCP operations with timestamps | Tool usage patterns, error rates |
| CO_ACCESS | entry pair co-retrieval counts | Knowledge graph structure |

## Appendix B: Platform Constraints

| Constraint | Impact | Workaround |
|-----------|--------|-----------|
| Subagent tool calls share parent session_id | Cannot attribute to specific agents | Heuristic role inference from tool patterns |
| SubagentStop often has empty agent_type | Cannot identify worker roles | Infer from tool sequence |
| No token count in hook data | Cannot measure token efficiency | Use response_size as proxy |
| Edit responses echo entire file | Inflates response_size metrics | Separate edit_bloat metric |
| No hook for agent reasoning | Cannot observe decision process | Only tool-call sequence visible |
