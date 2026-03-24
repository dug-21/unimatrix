# SPECIFICATION: col-025 — Feature Goal Signal

## Objective

Agents operating within a feature cycle today fall back to the bare topic-ID string (e.g., `"col-025"`) when deriving retrieval queries for `context_briefing` and UDS hook injection, producing low-precision context. This feature introduces an optional `goal` parameter to `context_cycle(start)` — a 1–2 sentence plain-text statement of intent — persists it on the `cycle_events` start row, caches it in `SessionState`, and wires it as the step-2 query signal in both the MCP briefing path and the UDS injection path.

---

## Functional Requirements

### FR-01: context_cycle goal parameter

`context_cycle` with `action = "start"` MUST accept an optional `goal: Option<String>` parameter in its wire protocol. The parameter MUST be ignored for `cycle_phase_end` and `cycle_stop` events.

### FR-02: goal persistence on cycle_events

When `context_cycle(start)` is invoked with a non-null `goal`, the value MUST be written to the `goal TEXT` column on the `cycle_start` event row in `cycle_events` within the same synchronous write that creates the row. When `goal` is absent, the column MUST be written as `NULL`.

### FR-03: goal max-byte guard at the tool handler layer

The tool handler MUST reject a `goal` value that exceeds 2 048 bytes (UTF-8 encoded) with a structured error response. Values within the limit are passed to storage without modification. No truncation is performed silently.

### FR-04: SessionState.current_goal field

`SessionState` MUST include a `current_goal: Option<String>` field. The field is populated by two paths:

- **Start path**: set directly from the `context_cycle(start)` payload in `handle_cycle_event`, without an additional DB read.
- **Resume path**: on session registration when a `feature_cycle` is already set (e.g., after server restart), loaded via a single indexed point lookup: `SELECT goal FROM cycle_events WHERE cycle_id = ? AND event_type = 'cycle_start' LIMIT 1`.

### FR-05: Resume-path null-safety

If the `cycle_events` lookup on resume returns no row (pre-v16 cycle, row absent, or DB error), `current_goal` MUST be set to `None` and session registration MUST complete successfully. A DB error on the lookup MUST NOT fail the session registration; it MUST be logged and treated as `None`.

### FR-06: derive_briefing_query step 2 — goal as synthesized signal

`synthesize_from_session(state)` MUST return `state.current_goal.clone()` when it is `Some`. This satisfies step 2 of the three-step priority function. The function MUST remain a pure synchronous function — no DB access, no async — operating only on already-resolved `SessionState`.

### FR-07: derive_briefing_query priority contract (unchanged structure)

The three-step priority order MUST remain:

1. Explicit `task` parameter (non-empty string) — wins unconditionally.
2. `synthesize_from_session(state)` → `current_goal` (when `Some`).
3. Topic-ID string — always non-empty, always the final fallback.

No changes are made to the function signature, the caller dispatch, or steps 1 and 3.

### FR-08: CompactPayload injection path — goal as query

The `CompactPayload` (PreCompact hook) path calls `IndexBriefingService::index` with the current session state. Because no agent-provided task is available on this path, `derive_briefing_query` step 2 MUST supply `current_goal` as the retrieval query when it is set, without any additional plumbing to the CompactPayload handler.

### FR-09: SubagentStart injection path — goal as step-2 signal

The `SubagentStart` hook handler MUST implement the following three-step query precedence explicitly (this path does not go through `derive_briefing_query` step 2 automatically):

1. `prompt_snippet` — used when non-empty (existing behaviour, unchanged).
2. `current_goal` — used when `prompt_snippet` is empty and `current_goal` is `Some`.
3. Topic-ID string — final fallback when neither is available.

When `prompt_snippet` is non-empty AND `current_goal` is set, `prompt_snippet` MUST win; `current_goal` MUST NOT override it.

### FR-10: Schema migration v15 → v16

The schema migration MUST add `goal TEXT` to `cycle_events` via:

```sql
ALTER TABLE cycle_events ADD COLUMN goal TEXT;
```

The migration MUST include an idempotency guard using `pragma_table_info` (established pattern #1264) so that re-running the migration on an already-upgraded database is a no-op. Schema version in the `COUNTERS` table MUST be updated to 16. Existing rows receive `NULL` by default; no backfill is performed.

---

## Non-Functional Requirements

### NFR-01: Zero-cost hot path

`current_goal` MUST be resolved from in-memory `SessionState` on the briefing and injection hot paths. No synchronous DB read occurs after session registration. The only DB read for goal is the resume-path lookup in FR-04, which occurs once per session resume.

### NFR-02: Backward compatibility

All callers that omit `goal` from `context_cycle(start)` MUST receive exactly today's behaviour with no observable change. All existing `context_cycle`, `context_briefing`, and `context_cycle_review` tests MUST pass without modification.

### NFR-03: Wire protocol backward compatibility

The `goal` field on the `context_cycle` wire protocol is optional with no default behaviour change. Old clients omitting the field are treated identically to `goal = None`.

### NFR-04: Pure synthesize_from_session

`synthesize_from_session` MUST NOT acquire any lock, perform any I/O, or introduce any async boundary. It is called on both the MCP and UDS hot paths; its cost must be O(1) clone of an `Option<String>`.

### NFR-05: Goal text stored verbatim

Goal text is stored without transformation (no normalisation, lowercasing, or trimming) beyond the byte-length check in FR-03. The downstream retrieval system receives exactly what the caller provided.

### NFR-06: Schema version assertion coverage

All migration test files that assert `schema_version` at version ≤ 15 MUST be updated to assert version 16 as part of this feature's delivery (SR-01, pattern #2933).

---

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|---------------------|
| AC-01 | `context_cycle(start, goal: "...")` stores the goal text in `cycle_events` on the start event row and in `SessionState.current_goal`. | Unit test: invoke handler with goal, assert DB row `goal` column = supplied text, assert `state.current_goal = Some(text)`. |
| AC-02 | `context_cycle(start)` with no `goal` param stores `NULL` in `cycle_events` and `None` in `SessionState.current_goal`; all downstream behaviour is unchanged. | Unit test: invoke handler without goal, assert DB row `goal = NULL`, assert `state.current_goal = None`, run existing test suite. |
| AC-03 | After a server restart, a session associated with a feature cycle that has a stored goal loads `current_goal` from `cycle_events` on resume. | Unit test: write `cycle_start` row with goal to DB, call session-resume path, assert `state.current_goal = Some(text)`. |
| AC-04 | `context_briefing` called with no `task` param but an active session with a stored goal uses the goal as the retrieval query (step 2 in `derive_briefing_query`). | Unit test: call `derive_briefing_query` with `task = None`, `state.current_goal = Some("...")`, `topic = "col-025"`, assert returned query = goal text. |
| AC-05 | `context_briefing` called with an explicit non-empty `task` uses `task` as the query regardless of whether a goal is stored (step 1 wins). | Unit test: call `derive_briefing_query` with `task = Some("explicit task")`, `state.current_goal = Some("goal")`, assert returned query = "explicit task". |
| AC-06 | `context_briefing` called with no `task` and no stored goal falls back to the topic-ID string (step 3), identical to today's behaviour. | Unit test: call `derive_briefing_query` with `task = None`, `state.current_goal = None`, `topic = "col-025"`, assert returned query = "col-025". |
| AC-07 | The `CompactPayload` UDS injection path uses `current_goal` as the query when no task is provided and a goal is stored. | Unit/integration test: call `IndexBriefingService::index` with session state where `current_goal = Some("goal")` and no task param, assert query passed to retrieval = goal text. |
| AC-08 | The `SubagentStart` injection path uses `current_goal` when `prompt_snippet` is empty and a goal is stored. | Unit test: call SubagentStart handler with `prompt_snippet = ""`, `state.current_goal = Some("goal")`, assert query = goal text. |
| AC-09 | Schema migration v15→v16 adds `goal TEXT` to `cycle_events` with idempotency guard; existing rows have `goal = NULL`. | Migration integration test: apply v16 migration to v15 DB, assert `pragma_table_info(cycle_events)` contains `goal`, assert existing rows `goal IS NULL`, re-run migration, assert no error. |
| AC-10 | All existing `context_cycle`, `context_briefing`, and `context_cycle_review` tests pass without modification (backward compatibility). | CI: existing test suite passes unmodified on the feature branch. |
| AC-11 | Unit tests cover: goal stored and retrieved on start (AC-01), absent goal (AC-02), resume from DB (AC-03), briefing query derivation priority (AC-04, AC-05, AC-06). | Test file review: named test cases map 1:1 to AC-01–AC-06. |
| AC-12 | `SubagentStart` path: when `prompt_snippet` is non-empty AND `current_goal` is set, `prompt_snippet` wins and `current_goal` is NOT used as the query. | Unit test: call SubagentStart handler with `prompt_snippet = "non-empty"`, `state.current_goal = Some("goal")`, assert query = prompt_snippet text. (SR-03) |
| AC-13 | A `goal` value exceeding 2 048 bytes is rejected at the tool handler with a structured error; no DB write occurs. | Unit test: supply 2 049-byte goal, assert error response, assert no row written to `cycle_events`. (SR-02) |
| AC-14 | Session resume when `cycle_events` has no matching `cycle_start` row (pre-v16 or missing) sets `current_goal = None` and completes registration without error. | Unit test: call resume path with no matching row in DB, assert `state.current_goal = None`, assert session registration succeeds. (SR-05) |
| AC-15 | Session resume when the DB lookup returns an error sets `current_goal = None`, logs the error, and completes registration without propagating the error. | Unit test: inject DB error on resume lookup, assert `state.current_goal = None`, assert registration succeeds, assert error is logged. (SR-05) |
| AC-16 | All migration test files asserting `schema_version` ≤ 15 are updated to assert version 16. | Code review + CI: no test file asserts `schema_version = 15` or lower after delivery. (SR-01) |

---

## Domain Models

### cycle_events (table)

The authoritative, synchronous durability record for feature lifecycle events. Each row represents a single lifecycle event within a feature cycle.

| Column | Type | Notes |
|--------|------|-------|
| `id` | INTEGER PK AUTOINCREMENT | Row identity |
| `cycle_id` | TEXT | Feature cycle identifier (indexed) |
| `seq` | INTEGER | Sequence within cycle |
| `event_type` | TEXT | `cycle_start` / `cycle_phase_end` / `cycle_stop` |
| `phase` | TEXT | Active phase name |
| `outcome` | TEXT | Phase outcome |
| `next_phase` | TEXT | Successor phase |
| `timestamp` | INTEGER NOT NULL | Unix epoch ms |
| `goal` | TEXT | **NEW (v16)** — intent statement; non-null only on `cycle_start` rows |

**Index**: `idx_cycle_events_cycle_id ON cycle_events(cycle_id)` — supports the goal resume lookup.

**Goal retrieval query**: `SELECT goal FROM cycle_events WHERE cycle_id = ? AND event_type = 'cycle_start' LIMIT 1`

### SessionState

In-memory struct held by `SessionRegistry`. Consulted by both the MCP handler and the UDS handler for session signals. Lives only for the duration of a server process; reconstructed on resume from `cycle_events`.

**New field**: `current_goal: Option<String>` — the goal text for the active feature cycle, or `None` if none was provided or the cycle predates v16.

### derive_briefing_query (shared function)

A pure, synchronous free function that computes the retrieval query for both the MCP briefing path and the UDS injection path. Implements a three-step priority waterfall:

1. `task` — explicit caller-provided query string (most specific signal).
2. `synthesize_from_session(state)` → `state.current_goal` — feature-level intent (new in col-025).
3. `topic` — bare feature/topic ID string (least specific, always non-null).

**Contract**: no I/O, no async, O(1) per invocation.

### SubagentStart injection path

The hook handler for `SubagentStart` events. Uses a parallel three-step query precedence that mirrors `derive_briefing_query` but is implemented explicitly because `prompt_snippet` (the spawning prompt) is the first-priority signal on this path:

1. `prompt_snippet` — the spawning agent's prompt (non-empty check).
2. `current_goal` — feature-level intent from `SessionState` (new in col-025).
3. `topic` — topic-ID fallback.

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **goal** | A 1–2 sentence plain-text statement of what a feature cycle is trying to accomplish, provided by the agent starting the cycle. |
| **current_goal** | The in-memory cached value of `goal` on `SessionState`, populated at cycle start or session resume. |
| **feature cycle** | The lifecycle unit tracked by `cycle_events` — begins with `cycle_start`, ends with `cycle_stop`. |
| **session resume** | Reconstruction of `SessionState` after a server restart, using `cycle_events` as the source of truth. |
| **derive_briefing_query** | The shared function that selects the best available query string for retrieval, using a three-step priority waterfall. |
| **injection path** | The UDS hook-driven path that injects `context_briefing` output into agent prompts (CompactPayload and SubagentStart). |
| **prompt_snippet** | The spawning agent's prompt text, available on the `SubagentStart` hook path only. |
| **step-2 signal** | The `synthesize_from_session` return value in `derive_briefing_query` — previously always `None`, now returns `current_goal`. |

---

## User Workflows

### Workflow 1: Agent starts a cycle with a goal

1. Agent calls `context_cycle(action="start", feature_cycle="col-025", goal="Improve briefing query relevance by anchoring retrieval to declared feature intent.")`.
2. Server writes `cycle_start` row to `cycle_events` with `goal` column populated.
3. Server sets `state.current_goal = Some(goal_text)` on the session.
4. Any subsequent `context_briefing` call with no `task` param uses the goal as the retrieval query automatically.
5. Any `SubagentStart` hook for agents spawned in this cycle uses `current_goal` as step-2 fallback when `prompt_snippet` is empty.

### Workflow 2: Agent starts a cycle without a goal (legacy)

1. Agent calls `context_cycle(action="start", feature_cycle="col-099")` — no `goal` param.
2. Server writes `cycle_start` row with `goal = NULL`.
3. `state.current_goal = None`.
4. All briefing and injection paths behave identically to today: `derive_briefing_query` falls through to step 3 (topic-ID).

### Workflow 3: Server restarts mid-cycle

1. Server restarts; `SessionState` is cleared.
2. An agent issues any request with a `session_id` tied to an existing `feature_cycle`.
3. Session resume path fires: `SELECT goal FROM cycle_events WHERE cycle_id = ? AND event_type = 'cycle_start' LIMIT 1`.
4. If a row is found with non-null `goal`, `state.current_goal = Some(goal_text)`.
5. If no row or `goal = NULL`, `state.current_goal = None`. Registration completes either way.
6. Subsequent briefing and injection calls use the reconstructed `current_goal` exactly as in Workflow 1.

### Workflow 4: SubagentStart hook fires during an active cycle

1. A `SubagentStart` hook fires for a spawned agent. `prompt_snippet` contains the spawning prompt.
2. If `prompt_snippet` is non-empty: use it as the query (step 1 — unchanged behaviour).
3. If `prompt_snippet` is empty and `state.current_goal` is `Some`: use `current_goal` (step 2 — new).
4. If neither: use topic-ID string (step 3 — unchanged fallback).

---

## Constraints

- `cycle_events.goal` is written only on `cycle_start` event rows. `cycle_phase_end` and `cycle_stop` events do not carry or modify the goal field.
- `synthesize_from_session` MUST remain a pure synchronous function. No DB reads, no async. Only the session resume path in `handle_cycle_event` (or equivalent session reconstruction) performs a DB read for goal.
- Schema version ownership: this feature owns v16. No other schema changes are in-flight on the main database at the time of scoping (verified in SCOPE.md §Constraints). If any concurrent in-flight work has bumped to v16 before delivery starts, the architect must resolve the collision before implementation.
- `sessions.keywords TEXT` column (dead since crt-025 WA-1) MUST NOT be modified by this feature. Cleanup is tracked separately.
- Goal text byte limit: 2 048 bytes, enforced at the tool handler layer. The storage layer imposes no further validation.
- Goal is not embedded as a vector in this feature. The `FusedScoreInputs` pipeline and scoring weights are unchanged.
- Old binaries cannot connect to a v16 database (standard version-gate constraint).

---

## Dependencies

### Crates (internal)

| Crate | Role |
|-------|------|
| `unimatrix-store` | Schema migration (v16), `cycle_events` writes/reads, `SessionState` definition |
| `unimatrix-server` | `handle_cycle_event`, `derive_briefing_query`, `synthesize_from_session`, `IndexBriefingService`, UDS hook handlers |

### Established Patterns (from Unimatrix)

| Entry | Pattern | Application |
|-------|---------|-------------|
| #1264 | Idempotent ALTER TABLE Guard via `pragma_table_info` | v15→v16 migration idempotency |
| #2933 | Schema Version Cascade: All Older Migration Test Files Must Update | AC-16 enforcement — audit all `schema_version` assertions ≤ 15 |
| #3000 | ADR-003 crt-025: CYCLE_EVENTS Uses Direct Write Pool | Goal write follows direct write pool path, not analytics drain |
| #3325 | Three-Step Query Derivation Priority — Shared Free Function | Step-2 slot already exists; `synthesize_from_session` is the hook |
| #3383 | cycle_events-first observation lookup pattern | Resume-path point lookup via `idx_cycle_events_cycle_id` |

### External Services

None. All changes are in-process within the Unimatrix server.

---

## NOT in Scope

- Embedding the `goal` sentence as a vector for `FusedScoreInputs` or semantic re-ranking. Storage decision is compatible; deferred.
- Removing `sessions.keywords TEXT` (dead column from crt-025 WA-1). Tracked separately.
- Changes to the `context_briefing` ranking or scoring pipeline. Goal improves the query; no scoring weights or fused inputs are modified.
- Storing `goal` on the `sessions` table. Sessions are transient; `cycle_events` is the authoritative durability tier.
- Backfilling `goal` for historical `cycle_events` rows. Existing cycles receive `NULL`; system degrades gracefully.
- Making `goal` required. It is always optional.
- Changes to `cycle_phase_end` or `cycle_stop` event handling.
- Changes to `derive_briefing_query` step 1 (explicit `task`) or step 3 (topic-ID fallback) behaviour.

---

## Open Questions

1. **OQ-01 (for architect)**: The `sessions.keywords TEXT` column cleanup is explicitly excluded (Non-Goals). Should it be batched with the v16 migration as a zero-cost ADD, or does coupling the cleanup risk the scope boundary warned in SR-04? Recommend architect documents the decision explicitly in ARCHITECTURE.md.

2. **OQ-02 (for architect)**: FR-05 specifies that a DB error on the resume-path goal lookup MUST be logged and treated as `None`. Confirm the logging target and severity (e.g., `tracing::warn!` vs `tracing::error!`) matches the convention used for other non-fatal session-reconstruction errors in the codebase.

3. **OQ-03 (for architect)**: The 2 048-byte max on `goal` (FR-03, AC-13) is a reasonable bound for a 1–2 sentence statement, but it is not sourced from an existing crate-wide constant. Architect should confirm this limit or substitute a project-standard value, and decide whether to introduce a named constant (e.g., `MAX_GOAL_BYTES`).

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for schema migration (cycle_events, ALTER TABLE, idempotency, version cascade) — found patterns #1264, #2933, #3000 (direct write pool), #370/#681 (create-new-then-swap, not applicable here).
- Queried: `/uni-query-patterns` for SessionState / derive_briefing_query / session resume — found #3325 (three-step query derivation), #3210 (SessionRegistry pre-resolution), #3297 (SubagentStart session_id routing).
- Queried: `/uni-query-patterns` for SubagentStart / UDS injection / prompt_snippet — found #3230 (SubagentStart routing pattern), #3251 (ADR-006: hookSpecificOutput envelope), #3243 (ADR-002: SubagentStart routing), #3324 (hook-side stdout format dispatch).
- All relevant established patterns are incorporated into constraints and dependency tables above.
