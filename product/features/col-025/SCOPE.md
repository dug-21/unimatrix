# col-025: Feature Goal Signal — context_cycle, Briefing, and Injection

## Problem Statement

Agents working on a feature today issue multiple `context_search` calls to build their
working context. `context_briefing` was designed to consolidate this into a single ranked
index, but the query it uses to drive retrieval is weak: when no explicit `task` parameter
is provided, `derive_briefing_query` falls back to the bare feature topic ID (e.g.,
`"col-025"`), which produces low-precision results against the knowledge base.

The UDS injection path (hook-driven `CompactPayload` and `SubagentStart`) has the same
problem: at hook fire time no agent-provided task exists, so the query falls through to the
topic-ID fallback. The result is generic injection content rather than content anchored to
what the feature is actually trying to accomplish.

There is no field in the system today that records the intended goal of a feature cycle.
`context_cycle(start)` starts a cycle but carries no semantic statement of intent.
The `keywords` field introduced in col-022 was removed from the wire protocol before any
consumer was written and is now inert.

## Goals

1. Add an optional `goal` parameter to `context_cycle(start)`: a 1–2 sentence plain-text
   statement of what the feature or task is trying to accomplish.
2. Persist the goal on the `cycle_events` start event row (`goal TEXT` column) — the
   authoritative, synchronous, durable record of feature lifecycle.
3. Cache the goal in `SessionState.current_goal: Option<String>`, loaded from
   `cycle_events` at session registration and on session resume, so it is available
   zero-cost on both the briefing and UDS injection hot paths.
4. Update `derive_briefing_query` step 2 (`synthesize_from_session`) to return
   `state.current_goal` — making the feature goal the fallback query when no explicit
   `task` parameter is provided.
5. Update the UDS injection path to use `current_goal` as the primary query signal
   (before topic-ID fallback), improving injection relevance for all hook events within
   an active feature cycle.

## Non-Goals

- Embedding the goal sentence as a vector for `FusedScoreInputs` or semantic re-ranking.
  The data model is compatible with this future addition; it is not in scope for this
  feature.
- Removing the dead `sessions.keywords TEXT` column. This is a valid cleanup candidate
  to batch with the schema migration but is tracked separately.
- Changing the `context_briefing` ranking or scoring pipeline. Goal improves the *query*
  fed into the existing pipeline; no scoring weights or fused inputs are modified.
- Storing goal on the `sessions` table. Goal is a feature property, not a session
  property. Sessions are transient and subject to retention cleanup; goal must survive
  across session boundaries and be available for retrospective review.
- Backfilling goal for historical cycle_events rows. Existing cycles will have
  `goal = NULL`; the system degrades gracefully to the existing fallback behavior.
- Making `goal` required. It is always optional; agents and tools that do not provide it
  continue to work exactly as today.

## Background Research

### cycle_events schema (confirmed, col-024 SCOPE)

Columns: `id` (AUTOINCREMENT PK), `cycle_id` TEXT, `seq` INTEGER, `event_type` TEXT
(`cycle_start` / `cycle_phase_end` / `cycle_stop`), `phase` TEXT, `outcome` TEXT,
`next_phase` TEXT, `timestamp` INTEGER NOT NULL.

`handle_cycle_event` writes the start row synchronously using the direct write pool
(ADR-003 crt-025). This makes `cycle_events` the right durability tier for goal — written
at the moment intent is declared, before any downstream observation.

Current schema version: v15 (v14→v15 added cycle_events in col-024). This feature
requires v16: `ALTER TABLE cycle_events ADD COLUMN goal TEXT`.

Index: `idx_cycle_events_cycle_id ON cycle_events(cycle_id)`. Goal retrieval is a
single indexed point lookup: `SELECT goal FROM cycle_events WHERE cycle_id = ?
AND event_type = 'cycle_start' LIMIT 1`.

### SessionState and session registration

`SessionState` is the in-memory struct held by `SessionRegistry` and consulted by both
the MCP handler and the UDS handler for session signals. Adding `current_goal:
Option<String>` follows the same pattern as other session fields.

Goal is loaded at two points:
- **Session registration** (`handle_cycle_event` on `CYCLE_START_EVENT`): the goal is
  available in the incoming event payload; write to cycle_events and set
  `state.current_goal` in one pass — no second DB read needed.
- **Session resume** (any session_id arriving after a server restart): load from
  `cycle_events` via the indexed lookup above. This is the same pattern used to
  reconstruct other session state on resume.

### derive_briefing_query (crt-027, entry #3325)

Three-step priority function already exists:
```
Step 1: explicit task param (wins if non-empty)
Step 2: synthesize_from_session(state)  ← currently returns None / weak signals
Step 3: topic string (always available, never empty)
```

`synthesize_from_session` returning `state.current_goal` directly satisfies the design
intent of step 2 and requires no changes to the priority logic or the function signature.
When both `task` and `goal` are present, `task` wins (step 1); goal does not contaminate
the task query.

The function is shared between the MCP handler (resolves session via `SessionRegistry`)
and the UDS handler (passes already-held session state directly). Both paths benefit
from `current_goal` without additional plumbing.

### UDS injection path

For `CompactPayload` (PreCompact hook), `IndexBriefingService::index` is called with
the session state in hand. No agent-provided task is available at this point. With goal
in `SessionState`, the query derivation naturally falls to step 2 (goal) rather than
step 3 (topic ID), producing semantically anchored injection content for the first time.

For `SubagentStart`, the hook uses `prompt_snippet` as the query (ADR-002 crt-027).
`prompt_snippet` is the spawning prompt — often long and noisy. Goal is the cleaner
signal when both are available. Precedence on the SubagentStart path: `prompt_snippet`
(non-empty) → `current_goal` → topic. This is consistent with the general pattern that
explicit caller-provided signal wins.

### Data modeler recommendation (confirmed)

Store on `cycle_events` start row. Cache in `SessionState.current_goal`. Load from
`cycle_events` on session registration/resume. Do not store on `sessions` table: wrong
abstraction level, wrong write path, wrong reliability tier for a value that must survive
across session boundaries and be available to retrospective review.

Forward-looking note (out of scope): the goal sentence could be embedded as a vector
and used as a context vector in `FusedScoreInputs`. The storage decision is compatible
with this; the embedding would live in session state, generated at cycle start from the
stored text.

## Proposed Approach

### Change 1: Schema migration v16

Add `goal TEXT` column to `cycle_events`:

```sql
ALTER TABLE cycle_events ADD COLUMN goal TEXT;
```

Idempotency guard via `pragma_table_info` (standard pattern, see col-022 ADR-005).
NULL default — no backfill. Old binaries cannot connect to v16 database (standard
constraint).

### Change 2: context_cycle wire protocol

Add `goal: Option<String>` to the `context_cycle` MCP tool parameters (start event only).
No change to `cycle_phase_end` or `cycle_stop` events. Wire protocol change is backward
compatible: existing callers omitting `goal` receive `None` and behavior is unchanged.

In `handle_cycle_event` for `CYCLE_START_EVENT`: extract `goal` from params, write to
the `cycle_events` row, and set `state.current_goal = goal` on the session immediately.

### Change 3: SessionState

Add `current_goal: Option<String>` to `SessionState`. Two population paths:
- Start path: set directly from event payload in `handle_cycle_event`.
- Resume path: query `cycle_events` for `goal` on session registration when
  `feature_cycle` is already set.

### Change 4: derive_briefing_query step 2

`synthesize_from_session(state)` returns `state.current_goal.clone()`. When `Some`,
this string becomes the briefing query (step 2 wins over step 3 topic fallback).
When `None` (no goal stored, or legacy cycle), step 3 topic-ID fallback runs unchanged.

### Change 5: UDS injection query precedence

On the `SubagentStart` path: after the existing `prompt_snippet` non-empty check
(step 1), add `current_goal` as step 2 before topic fallback.

On the `CompactPayload` path: `IndexBriefingService::index` receives the session state;
`derive_briefing_query` step 2 handles this automatically via Change 4.

## Acceptance Criteria

- AC-01: `context_cycle(start, goal: "...")` stores the goal text in `cycle_events` on
  the start event row and in `SessionState.current_goal`.
- AC-02: `context_cycle(start)` with no `goal` param stores `NULL` in `cycle_events`
  and `None` in `SessionState.current_goal`; all downstream behavior is unchanged.
- AC-03: After a server restart, a session associated with a feature cycle that has a
  stored goal loads `current_goal` from `cycle_events` on resume.
- AC-04: `context_briefing` called with no `task` param but an active session with a
  stored goal uses the goal as the retrieval query (step 2 in `derive_briefing_query`).
- AC-05: `context_briefing` called with an explicit non-empty `task` uses `task` as the
  query regardless of whether a goal is stored (step 1 wins).
- AC-06: `context_briefing` called with no `task` and no stored goal falls back to the
  topic-ID string (step 3), identical to today's behavior.
- AC-07: The `CompactPayload` UDS injection path uses `current_goal` as the query when
  no task is provided and a goal is stored.
- AC-08: The `SubagentStart` injection path uses `current_goal` when `prompt_snippet`
  is empty and a goal is stored.
- AC-09: Schema migration v15→v16 adds `goal TEXT` to `cycle_events` with idempotency
  guard; existing rows have `goal = NULL`.
- AC-10: All existing `context_cycle`, `context_briefing`, and `context_cycle_review`
  tests pass without modification (backward compatibility).
- AC-11: Unit tests cover: goal stored and retrieved on start (AC-01), absent goal
  (AC-02), resume from DB (AC-03), briefing query derivation priority (AC-04, AC-05,
  AC-06).

## Constraints

- `cycle_events.goal` is written only on `cycle_start` events. Phase-end and stop events
  do not carry or modify the goal field.
- Goal text is stored as-is; no truncation or validation at the storage layer. Callers
  are responsible for keeping goal concise (1–2 sentences). This can be enforced at the
  tool layer with a max-byte check if desired.
- `synthesize_from_session` must remain a pure function (no DB reads, no async) — it
  receives already-resolved `SessionState`. The session resume path (Change 3) is the
  only place a DB read for goal occurs.
- Schema version is v15 post-col-024. This feature owns v16. No other schema changes
  are in-flight on the main database at the time of scoping.
- The `keywords TEXT` column on `sessions` (dead since crt-025 WA-1) is not modified
  by this feature. Cleanup is a candidate to batch but tracked separately.

## Resolved Design Decisions

1. **goal lives on cycle_events, not sessions**: Confirmed by data modeler and design
   discussion. Goal is a feature property, not a session property. Sessions are transient;
   cycle_events is the authoritative, synchronous durability tier.

2. **task wins over goal in briefing**: When an agent explicitly provides `task`, it is
   the most specific available signal and wins unconditionally. Goal is a feature-level
   fallback, not a query modifier or blend input.

3. **goal is primary signal on injection, not briefing**: On UDS/hook paths no agent
   task is available; goal is the strongest signal present. Consistent with the asymmetry
   between tool-invoked briefing (agent knows its task) and hook-injected briefing (agent
   has not yet declared its task).

4. **No scoring pipeline changes**: The existing fused scoring mechanism handles
   relevance ranking. Goal improves the *query* fed into the pipeline; it does not
   introduce a new scoring dimension. Future GNN weight learning will operate on the
   existing signal inputs.

5. **goal embedding deferred**: Vectorizing the goal for semantic re-ranking in
   `FusedScoreInputs` is a natural follow-on. Storage decision is compatible. Not in
   scope here.

## Tracking

GH Issue: https://github.com/dug-21/unimatrix/issues/374
