# col-025 Implementation Brief — Feature Goal Signal

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-025/SCOPE.md |
| Architecture | product/features/col-025/architecture/ARCHITECTURE.md |
| Specification | product/features/col-025/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-025/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-025/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| schema-migration | pseudocode/schema-migration.md | test-plan/schema-migration.md |
| session-state | pseudocode/session-state.md | test-plan/session-state.md |
| cycle-event-handler | pseudocode/cycle-event-handler.md | test-plan/cycle-event-handler.md |
| mcp-cycle-params | pseudocode/mcp-cycle-params.md | test-plan/mcp-cycle-params.md |
| session-resume | pseudocode/session-resume.md | test-plan/session-resume.md |
| briefing-query-derivation | pseudocode/briefing-query-derivation.md | test-plan/briefing-query-derivation.md |
| subagent-start-injection | pseudocode/subagent-start-injection.md | test-plan/subagent-start-injection.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Add an optional `goal` parameter to `context_cycle(start)` that captures a feature cycle's plain-text intent, persists it durably on `cycle_events`, and wires it as the step-2 query signal in `derive_briefing_query` — replacing the weak topic-ID fallback that currently drives `context_briefing` retrieval and UDS hook injection when no explicit `task` is provided. The result is semantically anchored retrieval for all agents operating within a feature cycle that was started with a goal.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| goal storage location | Store on `cycle_events` start row, not `sessions` table. Goal is a feature property that must survive session boundaries and server restarts. `sessions` rows are transient and subject to retention cleanup. | SCOPE.md §Resolved-1, data modeler recommendation | architecture/ADR-001-goal-on-cycle-events-not-sessions.md |
| synthesize_from_session semantics | Replace entire body to return `state.current_goal.clone()`. Removes the prior topic-signal concatenation synthesis. When `None`, step 3 topic-ID fallback runs unchanged. | SCOPE.md §Resolved-2, ADR-002 analysis | architecture/ADR-002-synthesize-from-session-returns-current-goal.md |
| SubagentStart injection path | Explicit goal fallback branch inside the existing SubagentStart arm after transcript extraction — not routed through `handle_compact_payload` or `derive_briefing_query`. Precedence: `transcript_query (non-empty)` → `current_goal` → `RecordEvent/topic`. | SR-03, ADR-003 analysis | architecture/ADR-003-subagent-start-injection-explicit-branch.md |
| session resume DB failure contract | `get_cycle_start_goal` failure degrades to `current_goal = None` via `unwrap_or_else`; emits `tracing::warn!`; session registration always completes with `HookResponse::Ack`. | SR-05, ADR-004 analysis | architecture/ADR-004-session-resume-goal-degrade-to-none.md |
| byte-length guard placement and limits | MCP tool handler rejects at `MCP_MAX_GOAL_BYTES = 2048` with `CallToolResult::error`. UDS listener truncates (char-boundary-safe) at `UDS_MAX_GOAL_BYTES = 4096` with `tracing::warn!`. Two named constants required (see Alignment Status). | SR-02, ADR-005 analysis | architecture/ADR-005-goal-byte-length-guard-at-tool-layer.md |

---

## Files to Create/Modify

### unimatrix-store

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/db.rs` | Modify | Bump `CURRENT_SCHEMA_VERSION` to 16; add `goal: Option<&str>` parameter to `insert_cycle_event`; add `get_cycle_start_goal` read helper |
| `crates/unimatrix-store/src/migration.rs` | Modify | Add v15→v16 migration step: idempotent `ALTER TABLE cycle_events ADD COLUMN goal TEXT` via `pragma_table_info` guard |
| `crates/unimatrix-store/tests/migration_v15_to_v16.rs` | Create | v15→v16 migration integration test including idempotency scenario and column presence assertion |
| `crates/unimatrix-store/tests/migration_v14_to_v15.rs` | Modify | Update `schema_version` assertion from 15 to 16 (SR-01 cascade) |
| `crates/unimatrix-store/tests/sqlite_parity.rs` | Modify | Update `CURRENT_SCHEMA_VERSION` assertion to 16 (SR-01 cascade — confirm whether assertion exists) |
| `crates/unimatrix-store/tests/sqlite_parity_specialized.rs` | Modify | Update `CURRENT_SCHEMA_VERSION` assertion to 16 (SR-01 cascade — confirm whether assertion exists) |

### unimatrix-server

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/infra/session.rs` | Modify | Add `current_goal: Option<String>` field to `SessionState`; add `set_current_goal(&self, session_id: &str, goal: Option<String>)` to `SessionRegistry` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Add `goal: Option<String>` to `CycleParams`; enforce `MCP_MAX_GOAL_BYTES = 2048` guard before passing goal to payload; emit goal in `ImplantEvent` |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | Extract and enforce `UDS_MAX_GOAL_BYTES = 4096` (char-boundary-safe truncation with warn) in `handle_cycle_event` for `CYCLE_START_EVENT`; set `state.current_goal` synchronously; pass goal to `insert_cycle_event`; add explicit goal fallback branch to SubagentStart arm |
| `crates/unimatrix-server/src/services/index_briefing.rs` | Modify | Replace `synthesize_from_session` body to return `state.current_goal.clone()`; remove prior topic-signal concatenation; update `make_session_state` test helper to include `current_goal` field |

---

## Data Structures

### cycle_events (table) — v16 schema

```
id          INTEGER PK AUTOINCREMENT
cycle_id    TEXT  (indexed: idx_cycle_events_cycle_id)
seq         INTEGER
event_type  TEXT  — 'cycle_start' | 'cycle_phase_end' | 'cycle_stop'
phase       TEXT
outcome     TEXT
next_phase  TEXT
timestamp   INTEGER NOT NULL
goal        TEXT  — NEW (v16); non-null only on cycle_start rows; NULL for all other event types
```

### SessionState (struct) — new field

```rust
current_goal: Option<String>
// Populated on cycle start (from ImplantEvent payload, no DB read)
// Populated on session resume (from cycle_events DB lookup, once per resume)
// Always None for cycles without a stored goal or pre-v16 cycles
```

### CycleParams (MCP wire struct) — new field

```rust
goal: Option<String>
// Present only when action = "start"; ignored for cycle_phase_end and cycle_stop
// Validated: len (UTF-8 bytes) <= MCP_MAX_GOAL_BYTES (2048) before any downstream use
```

### Byte-Length Constants (two distinct constants required)

```rust
MCP_MAX_GOAL_BYTES: usize = 2048  // MCP path: reject with CallToolResult::error
UDS_MAX_GOAL_BYTES: usize = 4096  // UDS path: truncate char-boundary-safe + tracing::warn!
```

---

## Function Signatures

### New: Store::get_cycle_start_goal

```rust
async fn get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>>
```

Executes: `SELECT goal FROM cycle_events WHERE cycle_id = ?1 AND event_type = 'cycle_start' LIMIT 1`

Returns:
- `Ok(Some(goal))` — cycle_start row found with non-NULL goal
- `Ok(None)` — row found but goal IS NULL, or no matching row
- `Err(...)` — DB infrastructure failure (caller degrades to None via unwrap_or_else)

### New: SessionRegistry::set_current_goal

```rust
fn set_current_goal(&self, session_id: &str, goal: Option<String>)
```

Idempotent. Sets `SessionState.current_goal` for the given session. Called both on cycle start and on session resume. Must be safe under concurrent calls for the same session_id.

### Modified: Store::insert_cycle_event

```rust
// Before: fn insert_cycle_event(&self, cycle_id, seq, event_type, phase, outcome, next_phase, timestamp)
// After:  fn insert_cycle_event(&self, cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal: Option<&str>)
// goal is bound at bind position 8. Only meaningful when event_type = 'cycle_start'.
```

Delivery must verify exactly one call site exists in `listener.rs` before modifying the signature.

### Modified: synthesize_from_session

```rust
// Before: synthesizes "{feature_cycle} {top_3_signals}" from topic_signals
// After:  fn synthesize_from_session(state: &SessionState) -> Option<String> {
//             state.current_goal.clone()
//         }
// Pure, sync, O(1) clone. No I/O, no async, no lock acquisition.
```

---

## Constraints

- `cycle_events.goal` is written ONLY on `cycle_start` event rows. Phase-end and stop events do not carry or modify goal.
- `synthesize_from_session` must remain a pure synchronous function. No DB reads, no async, no lock acquisition. Cost is O(1) clone of `Option<String>`.
- `sessions.keywords TEXT` (dead since crt-025 WA-1) MUST NOT be touched by this feature. Column is out of scope.
- Goal text is stored verbatim (no normalization, no lowercasing, no trimming) beyond the byte-length check.
- Empty string goal (`""`) behavior: delivery must decide whether to treat as `None` or store verbatim. RISK-TEST-STRATEGY expects `None` treatment; NFR-05 says verbatim. Resolve before coding the tool handler. (ALIGNMENT-REPORT note: "spec must clarify.")
- Schema version ownership: this feature owns v16. Verify no concurrent in-flight schema change has already bumped to v16 before writing the migration.
- Old binaries cannot connect to a v16 database — standard schema gate constraint.
- SubagentStart goal branch: `session_id` must be confirmed reliably available in the SubagentStart arm of `dispatch_request` before implementing Component 7 (ARCHITECTURE.md OQ-03).

---

## Dependencies

### Internal Crates

| Crate | Role |
|-------|------|
| `unimatrix-store` | Schema migration, `cycle_events` read/write, `SessionState` struct definition |
| `unimatrix-server` | MCP tool handler, UDS listener, `SessionRegistry`, `IndexBriefingService`, SubagentStart hook arm |

### External Crates

None new. All changes use existing dependencies:
- `rusqlite` (bundled) — SQL binding for goal column
- `tokio` — existing async runtime; `get_cycle_start_goal` is an async fn
- `tracing` — warn log on resume DB error and UDS truncation

### Established Patterns (from Unimatrix)

| Entry | Pattern | Application |
|-------|---------|-------------|
| #1264 | Idempotent ALTER TABLE Guard via `pragma_table_info` | v15→v16 migration idempotency |
| #2933 | Schema Version Cascade: All Older Migration Test Files Must Update | AC-16 — audit all `schema_version` assertions ≤ 15 |
| #3000 | ADR-003 crt-025: CYCLE_EVENTS Uses Direct Write Pool | Goal write follows direct write pool, not analytics drain |
| #3325 | Three-Step Query Derivation Priority — Shared Free Function | Step-2 slot is the hook; `synthesize_from_session` is the implementation point |
| #3383 | cycle_events-first observation lookup pattern | Resume-path point lookup via `idx_cycle_events_cycle_id` |

---

## NOT in Scope

- Embedding the `goal` sentence as a vector for `FusedScoreInputs` or semantic re-ranking. Data model is compatible; deferred.
- Removing `sessions.keywords TEXT` (dead column from crt-025 WA-1). Tracked separately.
- Changes to the `context_briefing` ranking or scoring pipeline. Goal improves the query; no scoring weights or fused inputs are modified.
- Storing `goal` on the `sessions` table.
- Backfilling `goal` for historical `cycle_events` rows.
- Making `goal` required.
- Changes to `cycle_phase_end` or `cycle_stop` event handling.
- Changes to `derive_briefing_query` step 1 (explicit `task`) or step 3 (topic-ID fallback).
- `FusedScoreInputs` changes, GNN weight learning, or W3-1 territory.

---

## Alignment Status

**Overall: PASS with two WARNs. Feature is ready for delivery.**

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — directly fills the `synthesize_from_session` gap identified in Wave 1A |
| Milestone Fit | PASS — Wave 1A scope; no W2 or W3-1 pre-build |
| Architecture Consistency | PASS — all five Goals map to named components; five ADRs present |
| Risk Completeness | PASS — all SR-01 through SR-06 risks traced and resolved |
| Scope Gaps | WARN (see below) |
| Scope Additions | WARN (see below) |

### WARN-1: Dual Byte-Limit Split (MCP 2048 / UDS 4096)

SCOPE.md envisioned a single tool-layer guard. ADR-005 establishes two distinct limits and behaviors: MCP path rejects at 2048 bytes; UDS path truncates at 4096 bytes (char-boundary-safe) with a warn log. The UDS path is fire-and-forget and cannot return an error, making this split technically necessary.

**Delivery resolution required**: Define two named constants — `MCP_MAX_GOAL_BYTES = 2048` and `UDS_MAX_GOAL_BYTES = 4096`. Do NOT use a single `MAX_GOAL_BYTES` constant shared across both paths, as this would cause either the MCP limit to be wrong (too permissive at 4096) or the UDS limit to be wrong (too restrictive at 2048). AC-13 governs the MCP 2049-byte rejection test.

### WARN-2: MAX_GOAL_BYTES Naming Ambiguity

SPECIFICATION.md OQ-03 asked for a named constant; ADR-005 names it `MAX_GOAL_BYTES` but this is the UDS value (4096). The MCP value (2048) is unnamed in the ADR. The dual-constant resolution in WARN-1 above closes this.

### Open: Empty-String Goal Behavior

RISK-TEST-STRATEGY expects `goal = ""` to be treated as `None` (R-11 scenario 3). SPECIFICATION.md NFR-05 states verbatim storage. Delivery must resolve this tension explicitly before implementing the tool handler. Recommended resolution: treat `goal = Some("")` (empty after trim or raw empty) as `None` at the tool handler layer, consistent with the test expectation.
