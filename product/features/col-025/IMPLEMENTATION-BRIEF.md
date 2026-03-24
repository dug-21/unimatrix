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
| schema-migration-v16 | pseudocode/schema-migration-v16.md | test-plan/schema-migration-v16.md |
| session-state-extension | pseudocode/session-state-extension.md | test-plan/session-state-extension.md |
| cycle-event-handler | pseudocode/cycle-event-handler.md | test-plan/cycle-event-handler.md |
| mcp-cycle-wire-protocol | pseudocode/mcp-cycle-wire-protocol.md | test-plan/mcp-cycle-wire-protocol.md |
| session-resume | pseudocode/session-resume.md | test-plan/session-resume.md |
| briefing-query-derivation | pseudocode/briefing-query-derivation.md | test-plan/briefing-query-derivation.md |
| subagent-start-injection | pseudocode/subagent-start-injection.md | test-plan/subagent-start-injection.md |
| format-index-table-header | pseudocode/format-index-table-header.md | test-plan/format-index-table-header.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map
lists expected components from the architecture — actual file paths are filled during delivery.

---

## Goal

Add an optional `goal` parameter to `context_cycle(start)` — a 1–2 sentence plain-text
statement of what a feature cycle is trying to accomplish — persist it on the `cycle_events`
start row (v16 schema migration), cache it in `SessionState.current_goal`, and wire it as
the step-2 query signal in both the `context_briefing` MCP path (via `derive_briefing_query`)
and the UDS hook injection path (via an explicit goal-first branch on SubagentStart and
automatically via the shared function on CompactPayload). A named constant
`CONTEXT_GET_INSTRUCTION` is prepended once to all `format_index_table` output so agents
receiving a briefing table — whether from a tool call or UDS injection — immediately know
how to act on it. The change replaces the weak topic-ID fallback that previously drove
retrieval when no explicit `task` was provided.

---

## Resolved Decisions Table

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Where to store goal | `goal TEXT` on `cycle_events` start row. Goal is a feature property, not a session property; sessions are transient and subject to retention cleanup. Goal must survive server restarts and be available to retrospective review. `sessions.keywords TEXT` (dead since crt-025 WA-1) must not be touched. | ADR-001, SCOPE.md §Resolved-1 | architecture/ADR-001-goal-on-cycle-events-not-sessions.md |
| How `synthesize_from_session` returns goal | Replace entire body to return `state.current_goal.clone()`. Old topic-signal concatenation synthesis (`"{feature_cycle} {signals}"`) is removed. When `None`, step 3 topic-ID fallback runs unchanged. Pure sync function contract preserved. Shared between MCP briefing and UDS CompactPayload paths — both benefit with no additional wiring. | ADR-002, SCOPE.md §Resolved-2 | architecture/ADR-002-synthesize-from-session-returns-current-goal.md |
| SubagentStart routing when goal is present | Explicit branch: when `current_goal` is `Some(g)` and non-empty, immediately route to `IndexBriefingService::index(query: &g, k: 20)`. Goal wins unconditionally over `prompt_snippet` (prompt_snippet is spawn boilerplate, not semantic intent). Do NOT fall through to transcript/RecordEvent path. When goal is absent, existing transcript / prompt_snippet / RecordEvent path runs unchanged. | ADR-003, SR-03 | architecture/ADR-003-subagent-start-injection-explicit-branch.md |
| Session resume DB failure contract | Any DB error on `get_cycle_start_goal` degrades to `current_goal = None` via `unwrap_or_else` + `tracing::warn!`. Session registration always completes with `HookResponse::Ack`. Never blocks session usability. | ADR-004, SR-05 | architecture/ADR-004-session-resume-goal-degrade-to-none.md |
| Goal byte-length enforcement | One constant: `MAX_GOAL_BYTES = 1024`. MCP path: hard-reject with descriptive `CallToolResult::error` when `goal.len() > MAX_GOAL_BYTES`; no DB write; agent corrects and retries. UDS path: truncate at nearest valid UTF-8 char boundary at or below 1024 bytes + `tracing::warn!`; write truncated value (last-writer-wins). Empty/whitespace goal normalized to `None` at MCP handler (trim + empty check). Authoritative value: ADR-005 and SPECIFICATION.md (both 1024). Note: ARCHITECTURE.md §New Interfaces and §Integration Surface tables carry stale `4096` — delivery-time cleanup. | ADR-005, SR-02 | architecture/ADR-005-goal-byte-length-guard-at-tool-layer.md |
| `CONTEXT_GET_INSTRUCTION` header on all `format_index_table` output | Static constant defined in `src/services/index_briefing.rs`. Prepended once as a single header line before the table in every `format_index_table` output, on both MCP briefing responses and UDS injection payloads. Never inlined at call sites — constant name only. All existing `format_index_table` tests must be audited and updated (R-11). | ADR-006 | architecture/ADR-006-context-get-instruction-constant.md |

---

## Files to Create or Modify

### unimatrix-store

| Path | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/migration.rs` | Modify | Add v15→v16 migration: `ALTER TABLE cycle_events ADD COLUMN goal TEXT` with `pragma_table_info` idempotency guard; bump `CURRENT_SCHEMA_VERSION` to 16 |
| `crates/unimatrix-store/src/db.rs` | Modify | Update `insert_cycle_event` to accept `goal: Option<&str>` at the last bind position; add `get_cycle_start_goal(cycle_id: &str) -> Result<Option<String>>` async read helper |
| `crates/unimatrix-store/tests/migration_v15_to_v16.rs` | Create | New migration integration test: apply to v15 DB; assert `goal` column present; assert existing rows NULL; re-run (idempotency); assert `CURRENT_SCHEMA_VERSION = 16` |
| `crates/unimatrix-store/tests/migration_v14_to_v15.rs` | Modify | Update `schema_version` assertion from 15 to 16 (SR-01 / AC-16 cascade) |
| `crates/unimatrix-store/tests/sqlite_parity.rs` | Modify | Audit: update any `CURRENT_SCHEMA_VERSION` assertion to expect 16 (AC-16) |
| `crates/unimatrix-store/tests/sqlite_parity_specialized.rs` | Modify | Audit: update any `CURRENT_SCHEMA_VERSION` assertion to expect 16 (AC-16) |

### unimatrix-server

| Path | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/infra/session.rs` | Modify | Add `current_goal: Option<String>` to `SessionState`; init to `None` in `register_session`; add `set_current_goal(&self, session_id: &str, goal: Option<String>)` to `SessionRegistry` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Add `goal: Option<String>` to `CycleParams`; on `CycleType::Start`: trim, normalize whitespace to `None`, hard-reject if `goal.len() > MAX_GOAL_BYTES` with `CallToolResult::error`; emit validated goal in `ImplantEvent` payload |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | `handle_cycle_event`: extract goal from payload, UDS byte guard (truncate at UTF-8 char boundary + warn if > MAX_GOAL_BYTES), set `current_goal` synchronously via registry, pass goal to `insert_cycle_event`. `SessionRegister` arm: `get_cycle_start_goal` with `unwrap_or_else` degradation to `None` + warn. `SubagentStart` arm: explicit goal-present branch routing to `IndexBriefingService::index(query: &g, k: 20)`. |
| `crates/unimatrix-server/src/services/index_briefing.rs` | Modify | Replace `synthesize_from_session` body to return `state.current_goal.clone()`; remove old topic-signal synthesis. Define `CONTEXT_GET_INSTRUCTION` constant. Update `format_index_table` to prepend `CONTEXT_GET_INSTRUCTION` as first line before the table header. Audit and update all `format_index_table` test assertions (R-11). |

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

Goal retrieval query (session resume path):
```sql
SELECT goal FROM cycle_events WHERE cycle_id = ?1 AND event_type = 'cycle_start' LIMIT 1
```
Served by existing `idx_cycle_events_cycle_id` index.

### SessionState (modified)

New field added to the existing struct in `crates/unimatrix-server/src/infra/session.rs`:

```rust
pub current_goal: Option<String>
// None  — no goal provided, or pre-v16 cycle, or DB error on resume
// Some  — set from context_cycle(start) payload or reconstructed from cycle_events on session resume
```

Initialized to `None` in `register_session`. All existing `SessionState { .. }` struct
literals in tests must be updated to include `current_goal: None` (or use
`..Default::default()`). See pattern #3180.

### CycleParams (modified MCP wire struct)

```rust
pub goal: Option<String>
// Present and processed only when action = CycleType::Start.
// Silently ignored for CycleType::PhaseEnd and CycleType::Stop.
// After MCP validation: trimmed, empty/whitespace normalized to None, byte-checked.
```

### Constants

```rust
// crates/unimatrix-server/src/services/index_briefing.rs
pub const CONTEXT_GET_INSTRUCTION: &str =
    "Use context_get with the entry ID for full content when relevant.";

// Adjacent to MAX_INJECTION_BYTES / MAX_PRECOMPACT_BYTES (listener.rs or shared constants module)
pub const MAX_GOAL_BYTES: usize = 1024;
```

`MAX_GOAL_BYTES = 1024` is the single shared limit for both MCP (hard-reject) and UDS
(truncate) paths. The stale `4096` values in ARCHITECTURE.md §New Interfaces (line 187)
and §Integration Surface (line 210) are delivery-time cleanup items; ADR-005 is authoritative.

---

## Function Signatures

### New (unimatrix-store / `crates/unimatrix-store/src/db.rs`)

```rust
pub async fn get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>>;
// Ok(Some(goal)) — cycle_start row exists with non-NULL goal
// Ok(None)       — row absent, or goal IS NULL (no goal was provided, or pre-v16 NULL)
// Err(...)       — DB infrastructure failure (caller degrades to None via unwrap_or_else)
```

### Modified (unimatrix-store / `crates/unimatrix-store/src/db.rs`)

```rust
pub async fn insert_cycle_event(
    &self,
    cycle_id: &str,
    seq: i64,
    event_type: &str,
    phase: Option<&str>,
    outcome: Option<&str>,
    next_phase: Option<&str>,
    timestamp: i64,
    goal: Option<&str>,    // NEW — bound at last position; None for non-start events
) -> Result<()>;
// One call site exists in listener.rs (ARCHITECTURE.md OQ-01) — verify before modifying.
```

### New (unimatrix-server / `crates/unimatrix-server/src/infra/session.rs`)

```rust
impl SessionRegistry {
    pub fn set_current_goal(&self, session_id: &str, goal: Option<String>);
    // Idempotent. Safe under concurrent calls. Called on cycle start and session resume.
}
```

### Modified (unimatrix-server / `crates/unimatrix-server/src/services/index_briefing.rs`)

```rust
// Signature unchanged:
fn synthesize_from_session(state: &SessionState) -> Option<String> {
    state.current_goal.clone()
    // Pure, sync, O(1). No I/O. No async. Old topic-signal synthesis removed.
}
// Consequence: existing tests that assert the "{feature_cycle} {signals}" format
// must be updated (R-05).
```

### Session resume pattern (from ADR-004)

```rust
// In SessionRegister arm — crates/unimatrix-server/src/uds/listener.rs
let goal = store.get_cycle_start_goal(&feature_cycle)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, cycle_id = %feature_cycle,
            "col-025: goal resume lookup failed, degrading to None");
        None
    });
session_registry.set_current_goal(&session_id, goal);
```

### SubagentStart routing (from ADR-003)

```
// In SubagentStart arm — crates/unimatrix-server/src/uds/listener.rs
// Check goal FIRST, before any transcript extraction:
if let Some(g) = session_registry
        .get_state(session_id)?
        .current_goal
        .as_deref()
        .filter(|g| !g.is_empty())
{
    // Route to IndexBriefingService — goal wins unconditionally over prompt_snippet
    let payload = index_briefing_service.index(&g, session_state, 20).await?;
    // inject payload; return early — do NOT fall through to transcript path
    return Ok(inject(payload));
}
// else: fall through to existing transcript / prompt_snippet / RecordEvent path (unchanged)
```

---

## Constraints

1. `cycle_events.goal` is written **only** on `cycle_start` event rows. Phase-end and stop events always have `goal = NULL`.
2. `synthesize_from_session` **must remain a pure synchronous function** — no DB reads, no async, no lock acquisition. O(1) clone of `Option<String>`. Called on the MCP and UDS hot paths.
3. `MAX_GOAL_BYTES = 1024` is the **authoritative value** (ADR-005, SPECIFICATION.md). The stale `4096` in ARCHITECTURE.md §New Interfaces and §Integration Surface and in RISK-TEST-STRATEGY.md §Security section are delivery-time cleanup items — overwrite when implementing the constant.
4. Empty or whitespace-only goal must be normalized to `None` **at the MCP handler only** (trim → if empty → `None`). The UDS path does not perform whitespace normalization; it applies UTF-8-boundary truncation only.
5. `sessions.keywords TEXT` (dead since crt-025 WA-1) **must not be touched** by this feature. Tracked separately (SCOPE.md §Non-Goals, SR-04).
6. Goal is **not** embedded as a vector. `FusedScoreInputs`, scoring weights, and the embedding pipeline are entirely unchanged.
7. Schema version v16 is owned by this feature. Verify no concurrent in-flight work has bumped to v16 before starting the migration (SCOPE.md §Constraints, ALIGNMENT-REPORT.md §Scope Gaps).
8. Old binaries cannot connect to a v16 database — standard schema version gate.
9. `CONTEXT_GET_INSTRUCTION` appears **once** as a header line per `format_index_table` invocation — never per row, never inlined at call sites.
10. `insert_cycle_event` has exactly **one call site** in `listener.rs` (ARCHITECTURE.md OQ-01). Verify before changing the signature.
11. Resolve ARCHITECTURE.md OQ-03 before implementing the SubagentStart component: confirm that `session_id` is reliably populated in the SubagentStart hook payload before a `CYCLE_START_EVENT` has been processed for that session (R-12 integration test must cover this wiring).

---

## Dependencies

### Internal Crates

| Crate | Role |
|-------|------|
| `unimatrix-store` | Schema migration (v15→v16), `cycle_events` write/read (`insert_cycle_event`, `get_cycle_start_goal`) |
| `unimatrix-server` | MCP tool handler, UDS listener, `SessionRegistry`, `IndexBriefingService`, `format_index_table`, SubagentStart hook arm |

### External Crates (no new additions)

| Crate | Usage |
|-------|-------|
| `rusqlite` (bundled) | Positional bind for `goal` column |
| `tokio` | `get_cycle_start_goal` is async |
| `tracing` | `warn!` on resume DB error and UDS truncation |

### Established Patterns (from Unimatrix)

| Entry | Pattern | Application |
|-------|---------|-------------|
| #1264 | Idempotent ALTER TABLE guard via `pragma_table_info` | v15→v16 migration idempotency |
| #2933 | Schema version cascade: all older migration test files must update | AC-16 — audit all `schema_version` assertions ≤ 15 |
| #3000 | ADR-003 crt-025: cycle_events uses direct write pool | Goal write follows direct write pool path |
| #3325 | Three-step query derivation priority — shared free function | Step-2 slot; `synthesize_from_session` is the implementation point |
| #3383 | cycle_events-first observation lookup pattern | Resume-path point lookup via `idx_cycle_events_cycle_id` |
| #3398 | ADR-003 col-025: SubagentStart injection uses explicit goal branch | Precedence: goal → prompt_snippet → topic; routes to IndexBriefingService |
| #3400 | ADR-005 col-025: Goal byte-length guard at MCP tool handler layer | One constant; MCP hard-reject; UDS truncate at char boundary |
| #3246 | ADR-005 crt-027: IndexEntry as typed WA-5 contract surface | `format_index_table` surface; CONTEXT_GET_INSTRUCTION placement |
| #3301 | Graceful degradation via empty fallback, not early return | Resume DB error → None + warn; session proceeds |
| #3180 | SessionState field additions require updating `make_session_state` and struct literals | Audit all `SessionState { .. }` construction sites in tests |

---

## NOT in Scope

- Embedding the `goal` sentence as a vector for `FusedScoreInputs` or semantic re-ranking. Storage decision is compatible; this is a future addition.
- Removing `sessions.keywords TEXT` (dead column, crt-025 WA-1). Tracked separately.
- Changes to the `context_briefing` ranking or scoring pipeline. Goal improves the query; no scoring weights or fused inputs are modified.
- Storing `goal` on the `sessions` table.
- Backfilling `goal` for historical `cycle_events` rows. Existing cycles receive `NULL` and degrade gracefully to topic-ID fallback.
- Making `goal` required.
- Changes to `cycle_phase_end` or `cycle_stop` event handling.
- Changes to `derive_briefing_query` step 1 (explicit `task`) or step 3 (topic-ID fallback) behaviour.
- Whitespace normalization on the UDS path. UDS applies UTF-8-boundary truncation only.
- Per-row repetition of `CONTEXT_GET_INSTRUCTION`. The constant appears once as a single header line, not once per table row.

---

## Alignment Status

**Overall: PASS with one residual WARN. Feature is ready for delivery.**
(Source: ALIGNMENT-REPORT.md revision 3 — final clean report, 2026-03-24)

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly fills Wave 1A step-2 slot (`synthesize_from_session` returning `None`/weak signals). No overreach into Wave 2 or W3-1. |
| Milestone Fit | PASS | Wave 1A / WA-4b layer. Schema migration follows established ALTER TABLE pattern. |
| Scope Gaps | PASS | No items in SCOPE.md are unaddressed. One delivery-time check: verify schema is at exactly v15 before starting. |
| Scope Additions | PASS (accepted by human) | ADR-003 routing reversal (goal wins over prompt_snippet; routes to IndexBriefingService instead of ContextSearch) and ADR-006 (`CONTEXT_GET_INSTRUCTION` header) are both explicitly accepted. AC-18 and R-11 cover delivery requirements for ADR-006. |
| Architecture Consistency | WARN | ARCHITECTURE.md §New Interfaces (line 187) and §Integration Surface (line 210) state `pub const usize = 4096` — stale from pre-ADR-005 revision. RISK-TEST-STRATEGY.md §Security section and §Scope Risk Traceability also reference `4096` in prose. ADR-005 and SPECIFICATION.md are authoritative at `MAX_GOAL_BYTES = 1024`. R-07 test scenarios use the constant name (not a literal), so test logic is correct. Delivery must overwrite the four stale references. |
| Risk Completeness | PASS | All SR-01–SR-06 traced; 14-risk register complete; 9 non-negotiable gate-3c tests identified. |

### Prior WARNs resolved

- **WARN-1 (byte-limit split)**: Resolved. ADR-005 settled one constant at 1024; SPECIFICATION.md agrees throughout. No two-constant discrepancy.
- **WARN-2 (CONTEXT_GET_INSTRUCTION scope addition)**: Accepted by human. AC-18 added to specification; R-11 added to risk strategy.

### Delivery checklist from alignment

1. Overwrite the four stale `4096` references (two in ARCHITECTURE.md, two in RISK-TEST-STRATEGY.md) with `1024` when implementing `MAX_GOAL_BYTES`.
2. Verify schema is at exactly v15 before writing the v16 migration.
3. Resolve ARCHITECTURE.md OQ-03 (SubagentStart `session_id` timing) before implementing Component 7.
4. Audit all `SessionState { .. }` struct literals and test helpers for the new `current_goal` field (pattern #3180 / R-06).
5. Audit all `format_index_table` test assertions; introduce `strip_briefing_header(s: &str) -> &str` test helper (R-11).
6. Verify exactly one `insert_cycle_event` call site before touching the signature.

---

## Open Questions for Delivery

1. **OQ-01**: Confirm exactly one `insert_cycle_event` call site in `listener.rs` (ARCHITECTURE.md OQ-01). Pre-delivery grep required.
2. **OQ-02**: Confirm log severity convention (`tracing::warn!`) for non-fatal session-reconstruction errors matches existing patterns in `listener.rs` for similar failure modes.
3. **OQ-03**: `CONTEXT_GET_INSTRUCTION` exact wording is open — the constant name is settled; only the string value requires final confirmation at implementation time (ADR-006 §Decision provides example text).
4. **OQ-04**: Confirm `session_id` is reliably available in the SubagentStart hook arm before a `CYCLE_START_EVENT` has been processed for that session (ARCHITECTURE.md OQ-03 / R-12). Resolve before implementing the SubagentStart component.
