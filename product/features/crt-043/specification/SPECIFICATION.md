# crt-043: Behavioral Signal Infrastructure — Specification

## Objective

crt-043 adds two behavioral signal columns to the SQLite schema (v20 → v21): a
`goal_embedding BLOB` on `cycle_events` to support future H1 goal-clustering, and a
`phase TEXT` on `observations` to support future H3 phase-stratification. Both columns
are pure write-path additions — no retrieval path, search ranking, or MCP tool response
is changed. They are prerequisites for Group 6 (behavioral edge emission) and Group 7
(goal-conditioned briefing) in the ASS-040 roadmap.

---

## Functional Requirements

### Item B — Goal Embedding on cycle_events

**FR-B-01** — Schema column existence  
The `cycle_events` table MUST have a `goal_embedding BLOB` column after the v21
migration runs. The column is nullable; no default value.

**FR-B-02** — Embedding trigger condition  
When `context_cycle(type=start)` is called and the `goal` parameter is non-empty
(non-null, non-empty string), the system MUST schedule a background task to embed
the goal text.

**FR-B-03** — Embedding pipeline  
Goal text MUST be embedded via the existing `EmbedServiceHandle` pipeline using the
rayon `ml_inference_pool`. No blocking of the tokio executor is permitted during
embedding computation.

**FR-B-04** — Embedding serialization  
The resulting `Vec<f32>` MUST be serialized using
`bincode::serde::encode_to_vec(vec, bincode::config::standard())` before storage.
This is the canonical SQLite embedding blob format for the codebase (see ADR
requirement in Constraints).

**FR-B-05** — Paired decode helper  
A `decode_goal_embedding(blob: &[u8]) -> Result<Vec<f32>>` function MUST be shipped
in the same PR as the write path. It MUST use the inverse bincode call with the same
`config::standard()` configuration.

**FR-B-06** — Write method  
A new store method `update_cycle_start_goal_embedding(cycle_id: &str, bytes: &[u8])`
MUST issue a SQL UPDATE on the `cycle_events` start row identified by `cycle_id`
(the `topic` string, e.g. "crt-043") and `event_type = 'cycle_start'`.

**FR-B-07** — Fire-and-forget from MCP handler  
The embedding task MUST be spawned via `tokio::spawn` after the `context_cycle` tool
response is composed. The MCP handler MUST NOT await the embedding result.
`context_cycle` response text is unchanged from pre-crt-043 behavior.

**FR-B-08** — Ordering constraint (INSERT-before-UPDATE)  
The architect MUST choose one of the three options documented in SCOPE.md §Architecture
Constraint and record it as an ADR before delivery begins. The chosen mechanism must
guarantee the `cycle_events` INSERT row exists before the goal_embedding UPDATE
executes. Option 1 (fire from UDS listener after INSERT) or Option 2 (inline embed
before UDS dispatch) are preferred over Option 3 (retry polling).

**FR-B-09** — Skip on absent/empty goal  
When `goal` is absent, null, or empty string, the embedding task MUST NOT be spawned.
No warning is emitted in this case — the absence of a goal is the normal state for
cycles that do not carry one.

**FR-B-10** — Skip on embed service unavailability  
When the embed service handle is unavailable or returns an error, the embedding step
MUST be skipped. A `tracing::warn!` MUST be emitted. The cycle start MUST NOT be
blocked or return an error.

**FR-B-11** — No new crate dependencies  
No new entries in any `Cargo.toml` file are permitted. `bincode` and `EmbedServiceHandle`
are already in the workspace.

### Item C — Phase on observations

**FR-C-01** — Schema column existence  
The `observations` table MUST have a `phase TEXT` column after the v21 migration runs.
The column is nullable; no default value.

**FR-C-02** — ObservationRow struct field  
The `ObservationRow` struct in `listener.rs` MUST gain a `phase: Option<String>` field.

**FR-C-03** — Pre-capture timing  
At every observation write site, `current_phase` MUST be read from
`SessionState.current_phase` (via `session_registry.get_state(session_id)`) before
entering `spawn_blocking`. The captured value MUST be passed by value into the
`spawn_blocking` closure. This is the same pre-capture pattern used by `enrich_topic_signal`
for `topic_signal`.

**FR-C-04** — Write site coverage  
All four observation write sites in `listener.rs` MUST bind `phase`:
1. RecordEvent path (`insert_observation`)
2. Rework-candidate path (`insert_observation`)
3. RecordEvents batch path (`insert_observations_batch`)
4. ContextSearch path (`insert_observation`)

**FR-C-05** — NULL semantics  
`phase` MUST be NULL when: (a) no active cycle session exists for the session_id, or
(b) `current_phase` has not yet been set on the session state. NULL is a valid,
expected value and MUST NOT be treated as an error.

**FR-C-06** — Phase value passthrough  
Phase values MUST be stored as-is from `SessionState.current_phase` with no allowlist
validation or case normalization applied at write time. Canonical phase values
(e.g. "scope", "design", "delivery") are advisory documentation (see Domain Models).

**FR-C-07** — Composite index evaluation  
The delivery agent MUST evaluate adding a composite index on `(topic_signal, phase)` on
the `observations` table at implementation time. The evaluation result and decision MUST
be recorded before the PR is opened. Deferring this decision to Group 6 is not
acceptable (SR-06).

### Item M — Schema Migration

**FR-M-01** — Single migration step  
Both `goal_embedding BLOB` on `cycle_events` and `phase TEXT` on `observations` MUST be
added in a single migration step: v20 → v21. `CURRENT_SCHEMA_VERSION` is incremented
from 20 to 21.

**FR-M-02** — Atomic transaction  
Both ADD COLUMN statements MUST execute within a single `BEGIN`/`COMMIT` transaction.
The schema version bump MUST occur within the same transaction. A partial migration
(one column added, version bumped) MUST NOT be possible.

**FR-M-03** — Idempotency via pragma_table_info  
Before either ALTER TABLE statement executes, the migration MUST check
`pragma_table_info` for both target columns. If both columns already exist, the migration
MUST skip both ALTER statements and the version is not re-bumped. This follows the
established pattern from v15/v16 (col-025) and v12 (col-022).

**FR-M-04** — Migration test path  
Migration MUST be validated against a real v20 database through the full `Store::open()`
path — not against a fresh schema created at v21. This requirement reflects the lesson
from entry #378.

---

## Non-Functional Requirements

**NFR-01 — MCP handler latency**  
`context_cycle(type=start)` response latency MUST NOT increase by more than 5ms due to
this feature. The embedding work is fire-and-forget; any serialization or DB write that
happens synchronously before the response is returned must be negligible.

**NFR-02 — Executor blocking**  
Goal embedding computation MUST go through `ml_inference_pool` (rayon). It MUST NOT
execute on a tokio thread. `spawn_blocking` is required for any rayon-pool dispatch
that is awaited from an async context.

**NFR-03 — Store mutex contention**  
The `update_cycle_start_goal_embedding` UPDATE MUST NOT acquire the Store mutex
independently from other fire-and-forget work at cycle start. If other background
tasks already hold or acquire the Store for cycle-start writes, the embedding write
MUST be batched or sequenced with them (SR-03).

**NFR-04 — Cold-start degradation**  
All pre-v21 `cycle_events` rows have `goal_embedding = NULL`. All pre-v21 `observations`
rows have `phase = NULL`. Downstream Group 6/7 consumers MUST handle NULL gracefully.
This is accepted behavior, not a bug.

**NFR-05 — Backward compatibility**  
Old binaries cannot open a v21 database (standard schema version enforcement). This is
expected and acceptable. The migration is additive-only; no existing columns, rows, or
indexes are modified or removed.

**NFR-06 — Test coverage**  
All existing tests MUST pass. New unit tests (minimum) MUST cover the scenarios listed
in AC-12.

---

## Acceptance Criteria

**AC-01** — `cycle_events` column presence  
Verification: after running `Store::open()` on a v20 database, `pragma_table_info('cycle_events')`
returns a row with `name = 'goal_embedding'` and `type = 'BLOB'`.

**AC-02** — Embedding task spawned on non-empty goal  
Verification: calling `context_cycle(type=start, goal="design a query pipeline")` spawns
a background task that calls `EmbedServiceHandle::get_adapter()` and invokes embedding.
Test via mock or observable side-effect (e.g., stub embed handle records call count).

**AC-03** — Embedding persisted to cycle_events start row  
Verification: after the background task completes (awaited in test), a query of
`SELECT goal_embedding FROM cycle_events WHERE cycle_id = ? AND event_type = 'cycle_start'`
returns a non-NULL blob. The blob decodes via `decode_goal_embedding()` to a `Vec<f32>`
of the expected embedding dimension (384).

**AC-04** — Cycle start not blocked; skip paths emit correctly  
Verification (two sub-cases):  
  (a) When embed service is unavailable: `context_cycle(type=start, goal="text")` returns
  successfully; `tracing::warn!` is emitted; `goal_embedding` is NULL on the row.  
  (b) When goal is absent or empty: `context_cycle(type=start)` or
  `context_cycle(type=start, goal="")` returns successfully; no warning is emitted;
  `goal_embedding` is NULL on the row. No background task is spawned.

**AC-05** — No new crate dependencies  
Verification: `cargo metadata --no-deps` output is identical to pre-crt-043 for all
workspace members. No new entries appear in any `Cargo.toml` `[dependencies]` section.

**AC-06** — MCP response text unchanged  
Verification: the string returned by `context_cycle(type=start, ...)` in tests is
byte-for-byte identical to pre-crt-043 response text for the same inputs.

**AC-07** — `observations` column presence  
Verification: after running `Store::open()` on a v20 database, `pragma_table_info('observations')`
returns a row with `name = 'phase'` and `type = 'TEXT'`.

**AC-08** — ObservationRow carries phase field  
Verification: `ObservationRow` in `listener.rs` compiles with a `phase: Option<String>`
field. All construction sites compile without warnings.

**AC-09** — insert_observation and insert_observations_batch bind phase  
Verification: the SQL for both functions includes `:phase` (or positional equivalent)
bound to `observation.phase`. Confirmed by code review and by a test that reads back
the row and asserts the phase column matches what was written.

**AC-10** — Phase captured pre-spawn_blocking; NULL semantics correct  
Verification (two sub-cases):  
  (a) When `context_cycle(type=start, phase="design")` has been called for the session:
  a subsequent observation write produces a row where `phase = 'design'`.  
  (b) When no `context_cycle` has been called for the session: an observation write
  produces a row where `phase IS NULL`.

**AC-11** — Migration idempotency  
Verification: calling `Store::open()` on a database already at v21 completes without
error and does not increment the schema version beyond 21. `pragma_table_info` checks
both columns; neither ALTER TABLE is re-executed.

**AC-12** — Test coverage  
Verification: the test suite includes unit tests covering all of the following scenarios
(may be distributed across multiple test functions):  
  - Goal embedding background task fires and persists blob when embed service available  
  - Goal embedding skipped (no spawn) when goal is empty string  
  - Goal embedding skipped (no spawn) when goal is absent  
  - Goal embedding skipped gracefully when embed service unavailable; warn emitted  
  - Observation written with non-NULL phase when session has active phase  
  - Observation written with NULL phase when no active cycle for session  
  - v20 → v21 migration via `Store::open()` on a real v20 fixture database

**AC-13** — INSERT-before-UPDATE ordering guaranteed  
Verification: the architect's chosen mechanism (Option 1 or 2 from SCOPE.md) is
recorded as an ADR. An integration test or code-review assertion confirms the
`cycle_events` row exists before `update_cycle_start_goal_embedding` executes.
No silent no-ops from lost UPDATE races.

**AC-14** — decode_goal_embedding helper ships with write path  
Verification: `decode_goal_embedding(blob: &[u8]) -> Result<Vec<f32>>` is present in
the same module as the encode path. It uses `bincode::serde::decode_from_slice` with
`config::standard()`. A round-trip unit test (encode → decode → assert eq) is present.

---

## Domain Models

### Ubiquitous Language

| Term | Definition |
|------|------------|
| **cycle_events** | Append-only table recording lifecycle events for a feature cycle. Each row has `event_type` ("cycle_start", "cycle_phase_end", "cycle_stop"), `cycle_id` (the feature topic string, e.g. "crt-043"), and other metadata. `goal TEXT` was added in col-025 (schema v16). |
| **goal_embedding** | A `Vec<f32>` produced by the `EmbedServiceHandle` pipeline from the cycle's `goal` text, serialized as a bincode blob and stored on the `cycle_events` start row. Supports H1 (goal clustering). NULL for cycles with no goal and for all pre-v21 rows. |
| **observations** | Table recording every behavioral event observed by the hook listener. One row per observed event. Columns include `topic_signal` (feature attribution), `agent_role`, and after v21, `phase`. |
| **phase** | The free-text lifecycle phase name active on a session at observation write time. Sourced from `SessionState.current_phase`. Set by `context_cycle` events. Canonical values: "scope", "design", "delivery", "review". No allowlist at write time. Supports H3 (phase stratification). NULL when no active cycle or phase not yet set. |
| **topic_signal** | The feature ID column on `observations`. Populated at write time from hook event text (extract) or session registry fallback (enrich). This IS the feature attribution field; no separate `feature_id` column exists or is needed. |
| **SessionState.current_phase** | In-memory `Option<String>` on the session state object. Set synchronously by `context_cycle` calls. Read O(1) via Mutex. The authoritative source for `phase` capture. |
| **EmbedServiceHandle** | Handle to the rayon-based ONNX embedding service. Produces `Vec<f32>` from text input. Used via `get_adapter()` then `adapter.embed_entry("", text)`. Already present in the MCP handler context. |
| **ml_inference_pool** | The rayon thread pool used for CPU-bound ONNX inference. All embedding computation MUST route through this pool to avoid blocking the tokio executor. |
| **fire-and-forget** | A `tokio::spawn` task that runs to completion without the caller awaiting the result. Used for both the UDS hook dispatch and the goal embedding write in this feature. |
| **INSERT-before-UPDATE race** | The ordering hazard arising when the UDS-spawned INSERT and the MCP-handler-spawned UPDATE are unordered. The architect must choose a resolution mechanism that eliminates this race. |
| **bincode blob** | The serialization format for `goal_embedding`. Uses `bincode::serde::encode_to_vec(vec, config::standard())`. This is the first SQLite embedding blob pattern in the codebase; the ADR establishes it as the standard for Group 6 and beyond. |
| **cold-start degradation** | The expected condition where pre-v21 rows have NULL for new columns. Downstream consumers must treat NULL as a valid, non-error state. |
| **v20 → v21 migration** | The single migration step adding both `goal_embedding` to `cycle_events` and `phase` to `observations`. Wrapped in one transaction. Both `pragma_table_info` checks precede either ALTER TABLE. |

### Entity Relationships

```
cycle_events
  ├── cycle_id TEXT (FK-like: matches topic in session registry)
  ├── event_type TEXT ("cycle_start" | "cycle_phase_end" | "cycle_stop")
  ├── goal TEXT (added col-025 / v16)
  └── goal_embedding BLOB (added crt-043 / v21) ← NULL except on cycle_start rows
                                                    with non-empty goal

observations
  ├── topic_signal TEXT (feature attribution, populated by enrich_topic_signal)
  ├── agent_role TEXT
  └── phase TEXT (added crt-043 / v21) ← NULL when no active cycle / phase not set
```

---

## User Workflows

### Workflow 1: Cycle Start with Goal (Happy Path)

1. Agent calls `context_cycle(type=start, cycle_id="crt-043", goal="build behavioral signal infrastructure")`
2. MCP handler writes the cycle_start row via the UDS hook path (fire-and-forget).
3. MCP handler composes and returns the cycle start response text (unchanged).
4. Concurrently (or sequentially after INSERT, per architect's chosen mechanism): a
   background task embeds the goal text via `EmbedServiceHandle` on `ml_inference_pool`,
   serializes the result to bincode bytes, and calls `update_cycle_start_goal_embedding`
   to UPDATE the `cycle_events` start row.
5. The `goal_embedding` blob is now queryable for H1 goal-clustering (Group 6/7).

### Workflow 2: Cycle Start without Goal

1. Agent calls `context_cycle(type=start, cycle_id="crt-043")` (no goal parameter).
2. MCP handler writes the cycle_start row (goal = NULL).
3. MCP handler returns response text.
4. No embedding task is spawned. `goal_embedding` remains NULL.

### Workflow 3: Cycle Start — Embed Service Unavailable

1. Agent calls `context_cycle(type=start, cycle_id="crt-043", goal="some goal")`
2. MCP handler writes the cycle_start row.
3. MCP handler returns response text (not blocked).
4. Background task detects embed service unavailable; emits `tracing::warn!`; exits.
5. `goal_embedding` remains NULL on the row.

### Workflow 4: Observation Write with Active Phase

1. Agent is in a session where `context_cycle(type=start, phase="design")` was called.
2. Hook listener receives a RecordEvent or ContextSearch event.
3. `listener.rs` reads `session_registry.get_state(session_id)?.current_phase` → `Some("design")`.
4. The `phase` value is captured before `spawn_blocking`, passed into the closure.
5. `insert_observation` binds `phase = 'design'`. Row written to `observations`.

### Workflow 5: Observation Write — No Active Cycle

1. Agent writes an observation before any `context_cycle` call for the session.
2. `session_registry.get_state(session_id)?.current_phase` → `None` (or session not found).
3. `phase` is captured as `None` before `spawn_blocking`.
4. `insert_observation` binds `phase = NULL`. Row written. No error.

---

## Constraints

**C-01 — No retrieval path changes**  
No search ranking, briefing selection, or MCP tool response format is modified.
crt-043 is write-path infrastructure only.

**C-02 — No new crate dependencies**  
`bincode` and `EmbedServiceHandle` are already in the workspace. No new entries
in any `Cargo.toml`.

**C-03 — No UDS hook budget use for embedding**  
The UDS listener operates under a 50ms hook budget. Goal embedding MUST NOT happen
inside `handle_cycle_event`. It happens outside the UDS budget, in the MCP handler
spawn context.

**C-04 — INSERT signature stability**  
`insert_cycle_event` is called from the UDS listener. Its signature MUST NOT be
changed to accept embedding bytes. The embedding write is a separate UPDATE from
the MCP handler's spawn context.

**C-05 — Phase allowlist not enforced at write time**  
No allowlist is applied to `phase` values at write time. Canonical values ("scope",
"design", "delivery", "review") are advisory. Group 6 queries using phase
stratification must apply normalization (e.g., `LOWER()`) at query time.

**C-06 — Architect ADR required before delivery**  
The INSERT-before-UPDATE race (SR-01) MUST be resolved by an architect-authored ADR
choosing Option 1, 2, or 3 from SCOPE.md §Architecture Constraint. Delivery MUST NOT
begin until this ADR exists.

**C-07 — Bincode ADR required**  
The use of `bincode::serde` as the SQLite embedding blob serialization format MUST be
recorded as an ADR. The ADR must be broad enough to serve as the pattern for Group 6
embedding blobs (e.g., `goal_clusters` embeddings) and must note the follow-up
opportunity for primary entry embedding storage (O(1) `get_embedding` path).

**C-08 — Goal coverage is sparse by design**  
Only cycles that provide a non-empty `goal` parameter get a `goal_embedding` row.
This is intentional. Downstream Group 6/7 consumers MUST handle sparse coverage.
Requiring 100% goal_embedding coverage is explicitly out of scope.

**C-09 — No audit_log changes**  
The `audit_log` table is not touched.

**C-10 — No backfill**  
Pre-v21 rows are not backfilled. `goal_embedding` and `phase` are NULL for all
historical rows. No migration script or one-time job is part of this feature.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `EmbedServiceHandle` | Internal — `unimatrix-embed` | Already injected into MCP handler context. Availability in UDS listener path TBD by architect (SR-07). |
| `bincode` (serde feature) | Internal — already in workspace | No version change. `encode_to_vec` + `decode_from_slice` with `config::standard()`. |
| `SessionState.current_phase` | Internal — `unimatrix-server/session.rs` | Added crt-025. `Option<String>`, set via `set_current_phase()`. |
| `enrich_topic_signal` pattern | Internal — `listener.rs` | Pre-capture pattern (col-024 ADR-004). Item C follows identical timing contract. |
| `cycle_events` table + `idx_cycle_events_cycle_id` index | Schema — v16 | cycle_id index makes the `update_cycle_start_goal_embedding` UPDATE cheap. |
| `insert_cycle_event` | Internal — `db.rs:308` | Signature MUST NOT change (C-04). |
| `ml_inference_pool` (rayon) | Internal — server runtime | Required for embedding computation. |
| Schema v20 | Migration baseline | `CURRENT_SCHEMA_VERSION = 20`. Migration target is 21. |

---

## NOT in Scope

The following are explicitly excluded. Scope additions require a variance approved by
the vision guardian.

- **No observations.feature_id column** — `topic_signal` already is the feature ID.
- **No goal_clusters table** — Group 6 deliverable, not crt-043.
- **No goal-conditioned briefing changes** — Group 7 deliverable, not crt-043.
- **No behavioral edge emission (S6/S7)** — Group 6 deliverable, conditional on crt-043.
- **No audit_log changes** — explicitly excluded.
- **No agent_role instrumentation changes** — separate issue, not in scope.
- **No backfill of pre-v21 rows** — NULL for historical rows is accepted.
- **No external fetch for goal text** — goal comes exclusively from the `context_cycle`
  tool call parameter. No GitHub API, no `gh` CLI, no HTTP client.
- **No context_cycle MCP response format changes** — response text is unchanged.
- **No primary entry embedding storage in SQLite** — noted as a follow-up for the
  bincode ADR to document, but not implemented in crt-043.
- **No phase allowlist enforcement** — phase values stored as-is; normalization deferred
  to query time in Group 6.
- **No S6/S7/H1/H3 signal consumption** — this feature produces the columns; consuming
  them is Group 6/7.

---

## Open Questions

**OQ-01 (Blocking — Architect):** Is `EmbedServiceHandle` accessible in the UDS
listener construction path? This determines whether Option 1 (fire from UDS listener)
is available for resolving SR-01. Must be answered before the architect ADR is written.

**OQ-02 (Blocking — Architect):** Which of the three INSERT-before-UPDATE resolution
options is chosen? Options 1 and 2 are preferred. This must be recorded as an ADR
before delivery begins.

**OQ-03 (Delivery-time):** Should a composite index on `(topic_signal, phase)` be added
to `observations` in this migration? The delivery agent must evaluate query patterns for
Group 6 S6/S7 signal queries and decide. If added, it belongs in the v21 migration
transaction. If deferred, the justification must be documented.

**OQ-04 (Architect — ADR scope):** The bincode ADR should note the follow-up opportunity
of storing primary entry embeddings as SQLite blobs (O(1) `get_embedding`, addressing
crt-042 SR-01 PPR expander latency). Confirm the ADR is written broadly enough to cover
this without constraining the decision.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 20 entries; most relevant:
  - #3374 (col-024 ADR-004 enrich_topic_signal pattern) — directly describes the
    pre-capture timing contract Item C must follow
  - #3396 (col-025 ADR-001 goal stored on cycle_events) — establishes the cycle_events
    schema lineage and idempotent migration pattern
  - #1277 (col-022 ADR-005 schema v12 migration) — confirms pragma_table_info guard
    pattern and NULL-default ADD COLUMN convention
  - #2998, #2999, #3001 (crt-025 ADRs) — confirm SessionState.current_phase existence
    and phase snapshot timing patterns
