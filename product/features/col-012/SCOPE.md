# col-012: Data Path Unification

## Problem Statement

Unimatrix has two independent data ingestion paths from Claude Code hooks:

1. **JSONL files** -- Shell scripts (`.claude/hooks/observe-*.sh`) write per-session observation files to `~/.unimatrix/observation/{session_id}.jsonl`
2. **UDS socket** -- Hook CLI sends structured requests to the running server process, which stores data in SQLite tables (SESSIONS, INJECTION_LOG, SIGNAL_QUEUE, CO_ACCESS)

The UDS handler's `RecordEvent` arm discards all generic hook events (tool calls, subagent lifecycle) -- it only processes rework-candidate events. The retrospective pipeline (`unimatrix-observe`) reads JSONL files from disk, parses them line-by-line, scans directories for session discovery, and infers feature attribution from file content.

This dual path creates fragility (JSONL has no indexing, no JOINs, no ACID guarantees, manual rotation), prevents cross-data correlation (tool execution cannot be joined with knowledge injection or session outcomes), and duplicates data that already flows through the UDS socket.

## Goals

1. Add an `observations` table to SQLite that persists all hook events currently discarded by RecordEvent
2. Migrate the retrospective pipeline from JSONL file parsing to SQL queries against the observations table
3. Replace directory-scanning session discovery with SESSIONS table queries
4. Replace content-scanning feature attribution with SESSIONS.feature_cycle lookups
5. Remove JSONL write path from shell hook scripts (hooks become pure UDS forwarders)
6. Remove JSONL discovery/parsing code from unimatrix-observe
7. Achieve net code reduction (~200 lines changed, infrastructure removed)
8. Preserve all 21 detection rules unchanged (same input type, different data source)

## Non-Goals

- Changing detection rule logic -- all 21 rules consume `Vec<ObservationRecord>` and continue to do so
- Adding new detection rules or extraction capabilities (that is col-013)
- Modifying the MetricVector computation, baseline comparison, or report synthesis
- Adding neural models or passive knowledge acquisition (that is crt-007+)
- Changing the `context_retrospective` MCP tool interface
- Multi-project or daemon-mode architecture (that is dsn-phase)
- Converting Python test infrastructure to Rust (no Python exists in the retrospective codebase; Python files in `product/test/infra-001/` are integration test harness infrastructure and are outside this feature's scope)
- Background maintenance tick (that is col-013)

## Background Research

### Python Investigation

**Finding: There is no Python in the retrospective codebase.** The entire `unimatrix-observe` crate is pure Rust (11 source files: lib.rs, parser.rs, files.rs, types.rs, attribution.rs, baseline.rs, detection/, error.rs, metrics.rs, report.rs, synthesis.rs). Python files exist only in `product/test/infra-001/` (integration test harness for the MCP server), which is unrelated to the retrospective pipeline and out of scope.

### Existing Architecture

- **unimatrix-observe crate**: Has no dependency on unimatrix-store or unimatrix-server (ADR-001). Provides JSONL parsing, session discovery, feature attribution, 21 detection rules, metric computation, baseline comparison, and report synthesis.
- **Shell hooks**: Four bash scripts (`observe-pre-tool.sh`, `observe-post-tool.sh`, `observe-subagent-start.sh`, `observe-subagent-stop.sh`) that write JSONL and forward to UDS.
- **UDS listener**: `RecordEvent` handler at `crates/unimatrix-server/src/uds/listener.rs:559` logs the event type and session_id, then returns `Ack` without storing anything. `RecordEvents` batch handler similarly discards.
- **Schema v6**: Current version after nxs-008 normalization. Tables: entries, entry_tags, vector_map, counters, feature_entries, co_access, outcome_index, observation_metrics, signal_queue, sessions, injection_log, audit_log, agent_registry.
- **SESSIONS table**: Already has `feature_cycle`, `role`, `outcome`, `started_at`, `ended_at` fields (from col-010).

### Key Research Documents

- `product/research/ass-015/data-unification-analysis.md` -- Field-by-field gap analysis, migration path, unified data model
- `product/research/ass-015/feature-scoping.md` -- Detailed scoping with CRT integration context

### Field Gap Analysis (from ASS-015)

All fields currently captured by JSONL are available in the RecordEvent payload:
- Tool name, tool input, response size, response snippet (PreToolUse/PostToolUse)
- Agent type, prompt snippet (SubagentStart/SubagentStop)
- Timestamps (upgrade from seconds to milliseconds)

UDS captures data that JSONL does not: injection records, session outcomes, co-access pairs, quality signals, compaction counts, feature cycle (explicit vs inferred).

### ADR-001 Boundary Consideration

unimatrix-observe currently has no dependency on unimatrix-store. After unification, the retrospective pipeline needs to read from SQLite. Two approaches:
1. Add unimatrix-store dependency to unimatrix-observe (breaks ADR-001)
2. Define a trait/abstraction in unimatrix-observe that unimatrix-server implements (preserves independence)

The architecture phase must resolve this.

## Proposed Approach

### Phase 1: Add observations table (schema v7)

Add a migration from v6 to v7 that creates the observations table:

```sql
CREATE TABLE IF NOT EXISTS observations (
    session_hash INTEGER NOT NULL,
    ts_millis    INTEGER NOT NULL,
    hook         TEXT NOT NULL,
    session_id   TEXT NOT NULL,
    tool         TEXT,
    input        TEXT,
    response_size INTEGER,
    response_snippet TEXT,
    PRIMARY KEY (session_hash, ts_millis)
);
CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(session_id);
CREATE INDEX IF NOT EXISTS idx_observations_ts ON observations(ts_millis);
```

### Phase 2: Persist events in RecordEvent handler

Extend the `RecordEvent` and `RecordEvents` handlers in `uds/listener.rs` to persist all events to the observations table via `spawn_blocking` (fire-and-forget, same pattern as injection log writes).

### Phase 3: Migrate retrospective pipeline

- New data source abstraction in unimatrix-observe (trait-based or function pointer)
- SQL-backed implementation reads from observations table
- Session discovery via SESSIONS table query
- Feature attribution via SESSIONS.feature_cycle
- Detection rules unchanged (same `Vec<ObservationRecord>` input)

### Phase 4: Remove JSONL infrastructure

- Remove JSONL writes from shell hook scripts
- Remove JSONL parsing, file discovery, and observation directory management from unimatrix-observe
- Remove observation stats from context_status (replace with observations table stats)

## Acceptance Criteria

- AC-01: Schema migration v6->v7 creates `observations` table with indexes
- AC-02: `RecordEvent` handler persists all hook events (PreToolUse, PostToolUse, SubagentStart, SubagentStop) to observations table
- AC-03: `RecordEvents` batch handler persists all events in a single transaction
- AC-04: Retrospective pipeline reads observation data from SQLite instead of JSONL files
- AC-05: Session discovery uses SESSIONS table instead of directory scanning
- AC-06: Feature attribution uses SESSIONS.feature_cycle instead of content scanning
- AC-07: All 21 detection rules pass with SQL-sourced data (same results as JSONL-sourced)
- AC-08: JSONL write path removed from shell hook scripts (hooks forward to UDS only)
- AC-09: JSONL parsing code removed from unimatrix-observe (parser.rs, files.rs obsoleted)
- AC-10: context_retrospective MCP tool produces identical report structure
- AC-11: context_status observation stats read from observations table (not file system)
- AC-12: All existing tests pass; new tests cover SQL data source path
- AC-13: Net code reduction achieved (JSONL infrastructure removed exceeds new SQL code)

## Constraints

- **Schema version**: Must migrate from v6 to v7 using existing migration infrastructure in `crates/unimatrix-store/src/migration.rs`
- **ADR-001**: unimatrix-observe independence from unimatrix-store must be addressed architecturally (trait boundary or restructuring)
- **Fire-and-forget pattern**: Observation writes must not block the UDS response path (same spawn_blocking pattern as injection log)
- **SQLite WAL mode**: Already in use; concurrent reads during writes handled naturally
- **Hook latency budget**: 50ms total for hook processing; observation write must fit within existing budget
- **No detection rule changes**: All 21 rules must be preserved byte-for-byte; only the data source changes
- **Backward compatibility**: Must handle databases that existed before v7 (observations table may be empty for historical sessions)

## Resolved Questions

1. **ADR-001 resolution**: Preserve independence via trait abstraction. Human directive: "There needs to be a solid business/technical reason why there should not be an abstraction." Default: do not break ADR-001. Architect must make a strong case if proposing otherwise.
2. **Dual-write period**: No dual-write. Single cutover. Retrospective must continue working after migration.
3. **Historical data**: Start fresh from v7 onward. No JSONL migration. Only 1 prod database; clean break accepted.
4. **session_hash computation**: Deferred to architect.

## Tracking

GH Issue will be created during Session 1 synthesis phase.
