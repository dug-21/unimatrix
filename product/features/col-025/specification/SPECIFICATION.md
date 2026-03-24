# SPECIFICATION: col-025 — Feature Goal Signal

## Objective

Agents operating within a feature cycle today fall back to the bare topic-ID string (e.g., `"col-025"`) when deriving retrieval queries for `context_briefing` and UDS hook injection, producing low-precision context. This feature introduces an optional `goal` parameter to `context_cycle(start)` — a 1–2 sentence plain-text statement of intent — persists it on the `cycle_events` start row, caches it in `SessionState`, and wires it as the step-2 query signal in both the MCP briefing path and the UDS injection path. A named constant `CONTEXT_GET_INSTRUCTION` is prepended as a header to all `format_index_table` output so agents can navigate from the index to full entry content.

---

## Functional Requirements

### FR-01: context_cycle goal parameter

`context_cycle` with `action = "start"` MUST accept an optional `goal: Option<String>` parameter in its wire protocol. The parameter MUST be ignored for `cycle_phase_end` and `cycle_stop` events.

### FR-02: goal persistence on cycle_events

When `context_cycle(start)` is invoked with a non-null `goal`, the value MUST be written to the `goal TEXT` column on the `cycle_start` event row in `cycle_events` within the same synchronous write that creates the row. When `goal` is absent, the column MUST be written as `NULL`.

### FR-03: MAX_GOAL_BYTES constant — two enforcement behaviors

A single named constant `MAX_GOAL_BYTES` (1 024 bytes, UTF-8 encoded) is the shared limit for all `goal` inputs. Enforcement differs by transport:

- **MCP path**: the tool handler MUST reject a `goal` value exceeding `MAX_GOAL_BYTES` with a descriptive structured error response. No DB write occurs. The agent corrects and retries.
- **UDS path**: a `goal` value exceeding `MAX_GOAL_BYTES` MUST be truncated at the nearest valid UTF-8 character boundary at or before the limit and written (last-writer-wins — a corrected retry from the same cycle overwrites the truncated value). No error is returned.

Values within the limit are passed to storage without further modification on both paths.

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

### FR-09: SubagentStart injection path — goal as primary signal

The `SubagentStart` hook handler MUST route to `IndexBriefingService` when `goal` is present, using `goal` as the query. The three-step query precedence on this path is:

1. `current_goal` — used when `SessionState.current_goal` is `Some`. Routes to `IndexBriefingService` with goal as query.
2. `prompt_snippet` — used when `current_goal` is `None` and `prompt_snippet` is non-empty (existing behavior, unchanged).
3. Topic-ID string — final fallback when neither is available.

When `current_goal` is `Some` AND `prompt_snippet` is non-empty, `current_goal` MUST win; `prompt_snippet` MUST NOT override it.

### FR-10: Schema migration v15 → v16

The schema migration MUST add `goal TEXT` to `cycle_events` via:

```sql
ALTER TABLE cycle_events ADD COLUMN goal TEXT;
```

The migration MUST include an idempotency guard using `pragma_table_info` (established pattern #1264) so that re-running the migration on an already-upgraded database is a no-op. Schema version in the `COUNTERS` table MUST be updated to 16. Existing rows receive `NULL` by default; no backfill is performed.

### FR-11: Empty and whitespace-only goal normalization

At the MCP handler, a `goal` value that is an empty string or consists entirely of whitespace characters MUST be normalized to `None` before any further processing. Blank strings MUST NOT be stored in `cycle_events` or placed in `SessionState.current_goal`.

### FR-12: CONTEXT_GET_INSTRUCTION header in format_index_table output

A named constant `CONTEXT_GET_INSTRUCTION` MUST be defined and prepended as a single header line to all output produced by `format_index_table`. The instruction is agent-readable and brief (e.g., `"Use context_get with the entry ID for full content when relevant."`). It appears once at the top of the formatted output, not once per row. This header MUST appear in both:

- MCP `context_briefing` response output.
- UDS `CompactPayload` injection content.

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

### NFR-05: Goal text stored verbatim within byte limit

Goal text is stored without transformation (no normalisation, lowercasing, or trimming) beyond:

- The `MAX_GOAL_BYTES` enforcement in FR-03 (hard reject on MCP; UTF-8-boundary truncation on UDS).
- The empty/whitespace normalization to `None` in FR-11 (MCP path only).

The downstream retrieval system receives exactly what the caller provided, subject to the above.

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
| AC-08 | The `SubagentStart` injection path routes to `IndexBriefingService` using `current_goal` as the query when `current_goal` is set, regardless of `prompt_snippet`. | Unit test: call SubagentStart handler with `state.current_goal = Some("goal")` and `prompt_snippet = "anything"`, assert `IndexBriefingService` is invoked with query = goal text. |
| AC-09 | Schema migration v15→v16 adds `goal TEXT` to `cycle_events` with idempotency guard; existing rows have `goal = NULL`. | Migration integration test: apply v16 migration to v15 DB, assert `pragma_table_info(cycle_events)` contains `goal`, assert existing rows `goal IS NULL`, re-run migration, assert no error. |
| AC-10 | All existing `context_cycle`, `context_briefing`, and `context_cycle_review` tests pass without modification (backward compatibility). | CI: existing test suite passes unmodified on the feature branch. |
| AC-11 | Unit tests cover: goal stored and retrieved on start (AC-01), absent goal (AC-02), resume from DB (AC-03), briefing query derivation priority (AC-04, AC-05, AC-06). | Test file review: named test cases map 1:1 to AC-01–AC-06. |
| AC-12 | `SubagentStart` path: when `current_goal` is `Some`, it wins over a non-empty `prompt_snippet`; the query used is the goal text, not the prompt_snippet text. | Unit test: call SubagentStart handler with `state.current_goal = Some("goal")` and `prompt_snippet = "non-empty snippet"`, assert query = goal text. (SR-03) |
| AC-13a | A `goal` value exceeding `MAX_GOAL_BYTES` (1 024 bytes) on the MCP path is rejected with a descriptive structured error; no DB write occurs. | Unit test: supply 1 025-byte goal via MCP handler, assert error response, assert no row written to `cycle_events`. (SR-02) |
| AC-13b | A `goal` value exceeding `MAX_GOAL_BYTES` (1 024 bytes) on the UDS path is truncated at the nearest valid UTF-8 character boundary at or below 1 024 bytes and written; the truncated value appears in `cycle_events`. | Unit test: supply oversized goal via UDS handler, assert DB row `goal` column length ≤ 1 024 bytes, assert value is valid UTF-8. (SR-02) |
| AC-14 | Session resume when `cycle_events` has no matching `cycle_start` row (pre-v16 or missing) sets `current_goal = None` and completes registration without error. | Unit test: call resume path with no matching row in DB, assert `state.current_goal = None`, assert session registration succeeds. (SR-05) |
| AC-15 | Session resume when the DB lookup returns an error sets `current_goal = None`, logs the error, and completes registration without propagating the error. | Unit test: inject DB error on resume lookup, assert `state.current_goal = None`, assert registration succeeds, assert error is logged. (SR-05) |
| AC-16 | All migration test files asserting `schema_version` ≤ 15 are updated to assert version 16. | Code review + CI: no test file asserts `schema_version = 15` or lower after delivery. (SR-01) |
| AC-17 | A `goal` value that is an empty string or whitespace-only is normalized to `None` at the MCP handler; no blank string is written to `cycle_events` or placed in `SessionState.current_goal`. | Unit test: supply `""` and `"   "` as goal via MCP handler, assert DB row `goal = NULL`, assert `state.current_goal = None`. |
| AC-18 | All `format_index_table` output (MCP briefing responses and UDS CompactPayload injection) is prefixed with the `CONTEXT_GET_INSTRUCTION` header exactly once, before the first table row. | Unit test: call `format_index_table` with one or more entries, assert output starts with `CONTEXT_GET_INSTRUCTION` constant text; assert the constant text does not appear again within the table rows. |

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

The hook handler for `SubagentStart` events. Routes to `IndexBriefingService` when goal is present. Three-step query precedence (explicit, not handled by `derive_briefing_query`):

1. `current_goal` — used when `SessionState.current_goal` is `Some`. Goal is the primary signal; routing decision is made here.
2. `prompt_snippet` — the spawning agent's prompt (non-empty check); used when `current_goal` is `None`.
3. `topic` — topic-ID fallback when neither is available.

When `current_goal` is set it wins unconditionally over `prompt_snippet`, consistent with goal being the declared feature-level intent and `prompt_snippet` being an indirect inference.

### Constants

| Constant | Value | Scope |
|----------|-------|-------|
| `MAX_GOAL_BYTES` | 1 024 | Shared limit for goal byte-length enforcement on both MCP (hard reject) and UDS (truncate) paths |
| `CONTEXT_GET_INSTRUCTION` | `"Use context_get with the entry ID for full content when relevant."` (exact wording subject to implementation) | Header prepended once to all `format_index_table` output |

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **goal** | A 1–2 sentence plain-text statement of what a feature cycle is trying to accomplish, provided by the agent starting the cycle. |
| **current_goal** | The in-memory cached value of `goal` on `SessionState`, populated at cycle start or session resume. |
| **feature cycle** | The lifecycle unit tracked by `cycle_events` — begins with `cycle_start`, ends with `cycle_stop`. |
| **session resume** | Reconstruction of `SessionState` after a server restart, using `cycle_events` as the source of truth. |
| **derive_briefing_query** | The shared function that selects the best available query string for retrieval, using a three-step priority waterfall. |
| **injection path** | The UDS hook-driven path that injects `context_briefing` output into agent prompts (CompactPayload and SubagentStart). |
| **prompt_snippet** | The spawning agent's prompt text, available on the `SubagentStart` hook path only. On this path, `current_goal` takes precedence when set. |
| **step-2 signal** | The `synthesize_from_session` return value in `derive_briefing_query` — previously always `None`, now returns `current_goal`. |
| **MAX_GOAL_BYTES** | Named constant (1 024 bytes) for the maximum byte length of a `goal` value. Shared between MCP and UDS paths; enforcement behavior differs per transport. |
| **CONTEXT_GET_INSTRUCTION** | Named constant holding the agent-readable header prepended once to all `format_index_table` output, directing agents to use `context_get` for full entry content. |

---

## User Workflows

### Workflow 1: Agent starts a cycle with a goal

1. Agent calls `context_cycle(action="start", feature_cycle="col-025", goal="Improve briefing query relevance by anchoring retrieval to declared feature intent.")`.
2. MCP handler normalizes goal: non-empty, within `MAX_GOAL_BYTES`, non-whitespace — passes through.
3. Server writes `cycle_start` row to `cycle_events` with `goal` column populated.
4. Server sets `state.current_goal = Some(goal_text)` on the session.
5. Any subsequent `context_briefing` call with no `task` param uses the goal as the retrieval query automatically.
6. Any `SubagentStart` hook for agents spawned in this cycle routes to `IndexBriefingService` using `current_goal` as the query, regardless of `prompt_snippet`.

### Workflow 2: Agent starts a cycle without a goal (legacy)

1. Agent calls `context_cycle(action="start", feature_cycle="col-099")` — no `goal` param.
2. Server writes `cycle_start` row with `goal = NULL`.
3. `state.current_goal = None`.
4. All briefing and injection paths behave identically to today: `derive_briefing_query` falls through to step 3 (topic-ID); SubagentStart falls through to `prompt_snippet` then topic-ID.

### Workflow 3: Server restarts mid-cycle

1. Server restarts; `SessionState` is cleared.
2. An agent issues any request with a `session_id` tied to an existing `feature_cycle`.
3. Session resume path fires: `SELECT goal FROM cycle_events WHERE cycle_id = ? AND event_type = 'cycle_start' LIMIT 1`.
4. If a row is found with non-null `goal`, `state.current_goal = Some(goal_text)`.
5. If no row or `goal = NULL`, `state.current_goal = None`. Registration completes either way.
6. Subsequent briefing and injection calls use the reconstructed `current_goal` exactly as in Workflow 1.

### Workflow 4: SubagentStart hook fires during an active cycle

1. A `SubagentStart` hook fires for a spawned agent. `prompt_snippet` may or may not be present.
2. If `state.current_goal` is `Some`: route to `IndexBriefingService` with goal as query (step 1 — new behaviour; `prompt_snippet` value is irrelevant to this decision).
3. If `state.current_goal` is `None` and `prompt_snippet` is non-empty: use `prompt_snippet` as query (step 2 — existing behaviour, unchanged).
4. If neither: use topic-ID string (step 3 — unchanged fallback).

### Workflow 5: Agent provides an oversized goal

- **MCP path**: handler checks `goal.len() > MAX_GOAL_BYTES`, returns a descriptive structured error. No write occurs. Agent corrects goal and retries.
- **UDS path**: oversized goal is truncated at the nearest valid UTF-8 character boundary ≤ `MAX_GOAL_BYTES` and written. A later corrected retry from the same cycle overwrites the truncated value (last-writer-wins).

---

## Constraints

- `cycle_events.goal` is written only on `cycle_start` event rows. `cycle_phase_end` and `cycle_stop` events do not carry or modify the goal field.
- `synthesize_from_session` MUST remain a pure synchronous function. No DB reads, no async. Only the session resume path in `handle_cycle_event` (or equivalent session reconstruction) performs a DB read for goal.
- Schema version ownership: this feature owns v16. No other schema changes are in-flight on the main database at the time of scoping (verified in SCOPE.md §Constraints). If any concurrent in-flight work has bumped to v16 before delivery starts, the architect must resolve the collision before implementation.
- `sessions.keywords TEXT` column (dead since crt-025 WA-1) MUST NOT be modified by this feature. Cleanup is tracked separately.
- Goal byte limit: `MAX_GOAL_BYTES` = 1 024 bytes, enforced at the handler layer. The storage layer imposes no further validation. Enforcement behavior differs by transport (FR-03).
- Goal is not embedded as a vector in this feature. The `FusedScoreInputs` pipeline and scoring weights are unchanged.
- Old binaries cannot connect to a v16 database (standard version-gate constraint).
- Empty or whitespace-only goal values MUST NOT reach storage; normalization to `None` occurs at the MCP handler layer (FR-11). The UDS path does not perform whitespace normalization (truncation only).

---

## Dependencies

### Crates (internal)

| Crate | Role |
|-------|------|
| `unimatrix-store` | Schema migration (v16), `cycle_events` writes/reads, `SessionState` definition |
| `unimatrix-server` | `handle_cycle_event`, `derive_briefing_query`, `synthesize_from_session`, `IndexBriefingService`, UDS hook handlers, `format_index_table`, `MAX_GOAL_BYTES`, `CONTEXT_GET_INSTRUCTION` |

### Established Patterns (from Unimatrix)

| Entry | Pattern | Application |
|-------|---------|-------------|
| #1264 | Idempotent ALTER TABLE Guard via `pragma_table_info` | v15→v16 migration idempotency |
| #2933 | Schema Version Cascade: All Older Migration Test Files Must Update | AC-16 enforcement — audit all `schema_version` assertions ≤ 15 |
| #3000 | ADR-003 crt-025: CYCLE_EVENTS Uses Direct Write Pool | Goal write follows direct write pool path, not analytics drain |
| #3325 | Three-Step Query Derivation Priority — Shared Free Function | Step-2 slot already exists; `synthesize_from_session` is the hook |
| #3383 | cycle_events-first observation lookup pattern | Resume-path point lookup via `idx_cycle_events_cycle_id` |
| #3398 | ADR-003 col-025: SubagentStart Injection Uses Explicit Goal Branch | SubagentStart precedence: goal first, prompt_snippet second |
| #3400 | ADR-005 col-025: Goal Byte-Length Guard at MCP Tool Handler Layer | `MAX_GOAL_BYTES` constant; MCP hard-reject; UDS truncate |
| #3246 | ADR-005 crt-027: IndexEntry as Typed WA-5 Contract Surface | `format_index_table` surface; `CONTEXT_GET_INSTRUCTION` header placement |

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
- Whitespace normalization on the UDS path. UDS applies truncation only; blank-string normalization is MCP-only.
- Per-row repetition of `CONTEXT_GET_INSTRUCTION`. The constant appears once as a header, not once per table row.

---

## Open Questions

1. **OQ-01 (for architect)**: The `sessions.keywords TEXT` column cleanup is explicitly excluded (Non-Goals). Should it be batched with the v16 migration as a zero-cost ADD, or does coupling the cleanup risk the scope boundary warned in SR-04? Recommend architect documents the decision explicitly in ARCHITECTURE.md.

2. **OQ-02 (for architect)**: FR-05 specifies that a DB error on the resume-path goal lookup MUST be logged and treated as `None`. Confirm the logging target and severity (e.g., `tracing::warn!` vs `tracing::error!`) matches the convention used for other non-fatal session-reconstruction errors in the codebase.

3. **OQ-03 (for architect)**: `CONTEXT_GET_INSTRUCTION` wording is specified as an example in FR-12. Architect should confirm or adjust the exact instruction text before implementation. The constant name is settled; only the string value is open.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for schema migration (cycle_events, ALTER TABLE, idempotency, version cascade) — found patterns #1264, #2933, #3000 (direct write pool), #370/#681 (create-new-then-swap, not applicable here).
- Queried: `/uni-query-patterns` for SessionState / derive_briefing_query / session resume — found #3325 (three-step query derivation), #3210 (SessionRegistry pre-resolution), #3297 (SubagentStart session_id routing).
- Queried: `/uni-query-patterns` for SubagentStart / UDS injection / prompt_snippet — found #3230 (SubagentStart routing pattern), #3251 (ADR-006: hookSpecificOutput envelope), #3243 (ADR-002: SubagentStart routing), #3324 (hook-side stdout format dispatch).
- Queried: `/uni-query-patterns` for MAX_GOAL_BYTES / CONTEXT_GET_INSTRUCTION / format_index_table — found #3400 (ADR-005 col-025: goal byte-length guard, single constant, dual transport behavior), #3246 (ADR-005 crt-027: format_index_table / IndexEntry contract surface), #3398 (ADR-003 col-025: SubagentStart goal-first precedence).
- All relevant established patterns are incorporated into constraints and dependency tables above.
