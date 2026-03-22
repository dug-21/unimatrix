# SPECIFICATION: crt-025 — WA-1: Phase Signal + FEATURE_ENTRIES Tagging

GH #330 | Schema v14 → v15

---

## Objective

The engine currently records which knowledge entries are produced during a feature cycle but has no record of *which workflow phase* was active when each entry was produced. This feature introduces explicit phase lifecycle signaling into `context_cycle`, stores one append-only event row per lifecycle signal in a new `CYCLE_EVENTS` table, propagates the active phase into `SessionState`, tags each `feature_entries` row with the phase that produced it, and enriches `context_cycle_review` with a phase narrative and cross-cycle category distribution comparison.

---

## Canonical Phase Vocabulary

The engine stores phase strings as opaque labels. However, GNN training (W3-1) requires consistent discrete class labels across all features. The following vocabulary is the canonical set that all Unimatrix protocols must use:

| Phase Token | When Used |
|-------------|-----------|
| `scope` | Scope definition, problem statement, risk assessment |
| `design` | Architecture, pseudocode, specification authoring |
| `implementation` | Code writing and wiring |
| `testing` | Test authoring, test runs, coverage review |
| `gate-review` | Gate passage or rejection, PR review |

Protocols MUST use these exact tokens. The engine does not enforce vocabulary membership — it enforces only format (see FR-02). Inconsistent tokens (e.g., `impl`, `Scope`, `gate_review`) produce fragmented GNN labels.

---

## Ubiquitous Language Glossary

| Term | Definition |
|------|-----------|
| **Feature cycle** | A bounded work unit tracked from start to completion, identified by a `topic` string (e.g., `crt-025`). Corresponds to a `session.feature_cycle` and the `cycle_id` key in `CYCLE_EVENTS`. |
| **Phase** | A named stage within a feature cycle (e.g., `scope`, `implementation`). A phase is opaque to the engine; it is a string label validated only for format. |
| **Phase transition** | The moment when `SessionState.current_phase` changes, caused by a `phase-end` event carrying `next_phase`, or a `start` event carrying `next_phase`. |
| **Cycle event** | A single lifecycle signal emitted via `context_cycle`. One row in `CYCLE_EVENTS` per call. |
| **Session** | A single Claude Code invocation, identified by a `session_id`. Multiple sessions may share a feature cycle. |
| **`SessionState.current_phase`** | The in-memory, per-session record of which phase is currently active. Authoritative source for phase tagging at `context_store` time. |
| **Phase tag** | The `phase` value written to `feature_entries.phase` when an entry is recorded. `NULL` when no phase is active. |
| **Phase narrative** | The ordered sequence of cycle events rendered by `context_cycle_review`, derived from `CYCLE_EVENTS` rows ordered by `seq`. |
| **Rework** | A phase name appearing more than once in the event sequence for a feature cycle, indicating the phase was re-entered. |
| **Cross-cycle comparison** | A comparison of the current feature's per-phase category distribution against the mean of all prior features that have phase-tagged data. |
| **`CYCLE_EVENTS`** | The new append-only table. One row per `context_cycle` invocation. |
| **`outcome` category** | A knowledge entry category retired by this feature. New entries with `category = "outcome"` are blocked at ingest. Existing entries are not deleted. |

---

## Domain Models

### Updated `CycleParams` (MCP Wire Schema)

```
CycleParams {
    type:        String          -- required; "start" | "phase-end" | "stop"
    topic:       String          -- required; feature cycle identifier
    phase:       Option<String>  -- optional; canonical phase token (e.g., "scope")
    outcome:     Option<String>  -- optional; free-form outcome description
    next_phase:  Option<String>  -- optional; phase that becomes active after this event
    agent_id:    Option<String>  -- optional; caller identity
    format:      Option<String>  -- optional; "markdown" | "json"
}
```

Note: `keywords` is removed from the struct. Callers passing `keywords` have it silently ignored (no `deny_unknown_fields`).

### Updated `ValidatedCycleParams`

```
ValidatedCycleParams {
    cycle_type:  CycleType           -- Start | PhaseEnd | Stop
    topic:       String              -- validated feature cycle id
    phase:       Option<String>      -- normalized (lowercase, trim); None if absent
    outcome:     Option<String>      -- max 512 chars; None if absent
    next_phase:  Option<String>      -- normalized (lowercase, trim); None if absent
}
```

`keywords: Vec<String>` field is removed.

### Updated `CycleType` Enum

```
enum CycleType {
    Start,
    PhaseEnd,
    Stop,
}
```

### `CYCLE_EVENTS` Table (new, schema v15)

```sql
CREATE TABLE IF NOT EXISTS cycle_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    cycle_id   TEXT    NOT NULL,
    seq        INTEGER NOT NULL,
    event_type TEXT    NOT NULL,
    phase      TEXT,
    outcome    TEXT,
    next_phase TEXT,
    timestamp  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cycle_events_cycle_id ON cycle_events (cycle_id);
```

| Column | Type | Notes |
|--------|------|-------|
| `id` | INTEGER PK AUTOINCREMENT | Surrogate key |
| `cycle_id` | TEXT NOT NULL | Feature cycle identifier; equals `CycleParams.topic` |
| `seq` | INTEGER NOT NULL | Monotonically increasing per `cycle_id`; computed as `COALESCE(MAX(seq), -1) + 1` scoped to `cycle_id` |
| `event_type` | TEXT NOT NULL | One of `"cycle_start"`, `"cycle_phase_end"`, `"cycle_stop"` |
| `phase` | TEXT NULL | Phase active at event time; `NULL` if not provided |
| `outcome` | TEXT NULL | Free-form outcome annotation; `NULL` if not provided |
| `next_phase` | TEXT NULL | Phase that becomes active after this event; `NULL` if not provided |
| `timestamp` | INTEGER NOT NULL | Unix epoch seconds |

### Updated `FEATURE_ENTRIES` Schema (schema v15)

```sql
-- Existing columns unchanged:
--   feature_id TEXT NOT NULL
--   entry_id   INTEGER NOT NULL
--   PRIMARY KEY (feature_id, entry_id)
-- Added column:
ALTER TABLE feature_entries ADD COLUMN phase TEXT;
```

| Column | Type | Notes |
|--------|------|-------|
| `phase` | TEXT NULL | Phase active when entry was stored; `NULL` for pre-WA-1 rows and rows stored before any phase transition |

### Updated `SessionState`

```
SessionState {
    -- existing fields unchanged --
    session_id:        String
    role:              Option<String>
    feature:           Option<String>
    injection_history: Vec<InjectionRecord>
    coaccess_seen:     HashSet<u64>
    compaction_count:  u32
    rework_events:     Vec<ReworkEvent>
    topic_signals:     HashMap<String, TopicTally>
    -- new field --
    current_phase:     Option<String>   -- active phase label; None until first phase signal
}
```

`current_phase` is initialized to `None` when a session is registered.

### Updated `AnalyticsWrite::FeatureEntry` Variant

```
AnalyticsWrite::FeatureEntry {
    feature_id: String,
    entry_id:   u64,
    phase:      Option<String>,  -- new field; snapshot at enqueue time
}
```

Phase is captured at enqueue time from `SessionState.current_phase`, not read at drain-flush time. This prevents SR-07 (stale phase at flush).

---

## Functional Requirements

### FR-01: `CycleParams` Struct Update

FR-01.1: `CycleParams` MUST include `phase: Option<String>`, `outcome: Option<String>`, and `next_phase: Option<String>` fields.

FR-01.2: `CycleParams` MUST NOT include a `keywords` field in its struct definition.

FR-01.3: A `CycleParams` deserialized from JSON containing an unknown field (including `keywords`) MUST succeed without error. Unknown fields are silently discarded.

FR-01.4: `type` and `topic` remain required fields. Absence of either MUST produce a deserialization error.

### FR-02: Phase String Validation

FR-02.1: `validate_cycle_params` MUST normalize `phase` and `next_phase` values to lowercase before validation (e.g., `"Scope"` → `"scope"`).

FR-02.2: After normalization, any `phase` or `next_phase` value containing a space character MUST be rejected with an error message identifying the invalid field.

FR-02.3: After normalization, any `phase` or `next_phase` value exceeding 64 characters MUST be rejected with an error message.

FR-02.4: An empty string for `phase` or `next_phase` MUST be rejected.

FR-02.5: `None` values for `phase` and `next_phase` are always valid regardless of event type.

FR-02.6: Any `outcome` value exceeding 512 characters MUST be rejected with a descriptive error. `None` is always valid.

### FR-03: `validate_cycle_params` Extension

FR-03.1: `validate_cycle_params` MUST accept `"phase-end"` as a valid `type` value, mapping it to `CycleType::PhaseEnd`.

FR-03.2: `"start"` and `"stop"` MUST remain valid and map to `CycleType::Start` and `CycleType::Stop` respectively.

FR-03.3: Any `type` value other than `"start"`, `"phase-end"`, and `"stop"` MUST be rejected with a descriptive error naming all valid values.

FR-03.4: The function signature MUST remain `fn validate_cycle_params(...) -> Result<ValidatedCycleParams, String>`. It MUST NOT return `ServerError` (hook-path constraint, ADR-004).

FR-03.5: The `keywords` parameter MUST be removed from the function signature.

FR-03.6: `ValidatedCycleParams` MUST include `phase`, `outcome`, and `next_phase` fields and MUST NOT include a `keywords` field.

FR-03.7: The hook path MUST handle `"phase-end"` events by emitting a `HookRequest::RecordEvent` with `event_type = "cycle_phase_end"`. On validation failure, the hook MUST log a warning and fall through to the generic observation path — it MUST NOT hard-fail or return an error to the transport.

### FR-04: `CYCLE_EVENTS` Write on Every `context_cycle` Call

FR-04.1: Every successful `context_cycle` invocation MUST produce exactly one INSERT to `cycle_events`.

FR-04.2: The `event_type` column MUST be:
- `"cycle_start"` for `type = "start"`
- `"cycle_phase_end"` for `type = "phase-end"`
- `"cycle_stop"` for `type = "stop"`

FR-04.3: The `seq` value MUST be `COALESCE(MAX(seq), -1) + 1` computed within the scope of the same `cycle_id`. Multiple sessions sharing a feature cycle share one monotonic sequence per `cycle_id`.

FR-04.4: A `"phase-end"` event received with no prior `"start"` row for the same `cycle_id` MUST still insert into `CYCLE_EVENTS`. The append log is an audit trail; orphaned events are valid.

FR-04.5: The `CYCLE_EVENTS` INSERT MUST be a fire-and-forget write that does not block the MCP tool response path. It MUST complete within the hook latency budget (40ms total transport timeout).

FR-04.6: `phase`, `outcome`, and `next_phase` columns MUST be written from the validated parameters. Absent optional fields are stored as SQL `NULL`.

### FR-05: `SessionState.current_phase` Updates

FR-05.1: `SessionState` MUST include a `current_phase: Option<String>` field, initialized to `None` on session registration.

FR-05.2: On a `"start"` event: if `next_phase` is present in the validated params, `SessionState.current_phase` MUST be set to `Some(next_phase)` synchronously within the UDS listener handler for that session, before any `context_store` can execute. If `next_phase` is absent, `current_phase` remains `None`.

FR-05.3: On a `"phase-end"` event: if `next_phase` is present, `SessionState.current_phase` MUST be updated to `Some(next_phase)`. If `next_phase` is absent, `current_phase` is left unchanged.

FR-05.4: On a `"stop"` event: `SessionState.current_phase` MUST be cleared to `None`.

FR-05.5: `current_phase` mutation MUST be synchronous within the UDS handler's own task — not queued behind the analytics drain. (Mitigates SR-01.)

### FR-06: Phase Tagging in `feature_entries`

FR-06.1: When `context_store` records a feature entry (via `record_feature_entries` or `AnalyticsWrite::FeatureEntry`), the current `SessionState.current_phase` MUST be captured at enqueue/call time and written to `feature_entries.phase`.

FR-06.2: For the `AnalyticsWrite::FeatureEntry` variant, `phase` MUST be a field on the variant struct and MUST be captured when the event is enqueued — not read from `SessionState` at drain-flush time. (Mitigates SR-07.)

FR-06.3: Entries stored before any phase transition (i.e., when `current_phase = None`) MUST receive `phase = NULL` in `feature_entries`.

FR-06.4: Pre-existing `feature_entries` rows (before schema v15) MUST NOT be backfilled. They retain `phase = NULL` as correct historical data.

FR-06.5: Both write paths — `record_feature_entries` (direct write pool) and `AnalyticsWrite::FeatureEntry` (analytics drain) — MUST propagate `phase` to the inserted row.

### FR-07: Schema Migration v14 → v15

FR-07.1: Schema version MUST advance from 14 to 15 in `run_main_migrations`.

FR-07.2: A new `cycle_events` table MUST be created with the schema defined in the Domain Models section, including the `idx_cycle_events_cycle_id` index.

FR-07.3: A `phase TEXT` nullable column MUST be added to `feature_entries`. The migration MUST use the `pragma_table_info` pre-check pattern before `ALTER TABLE ADD COLUMN` (SQLite does not support `IF NOT EXISTS` for columns).

FR-07.4: Migration MUST be idempotent: running it twice MUST NOT produce an error or corrupt state.

FR-07.5: `create_tables_if_needed` in `db.rs` MUST be updated to include the `cycle_events` table and the `feature_entries.phase` column for fresh database creation.

FR-07.6: A migration integration test MUST cover the v14 → v15 transition, verifying both the new table and the added column are present after migration.

### FR-08: `outcome` Category Retirement

FR-08.1: `"outcome"` MUST be removed from `INITIAL_CATEGORIES` in `CategoryAllowlist`.

FR-08.2: After this change, `CategoryAllowlist::new()` MUST contain 7 categories (not 8).

FR-08.3: A `context_store` call with `category = "outcome"` MUST return a category-rejected error (`ServerError::InvalidCategory`).

FR-08.4: Existing entries with `category = "outcome"` in the database MUST NOT be deleted or modified. Only new ingest is blocked.

FR-08.5: `outcome_tags.rs` validation logic is called only when `category == "outcome"`. Since new ingest of `outcome` is blocked, this path becomes unreachable for new entries. The file MUST be retained — its removal is tracked in GH #338.

FR-08.6: All existing tests that assert `al.validate("outcome").is_ok()` MUST be updated to assert `al.validate("outcome").is_err()`.

### FR-09: `context_cycle_review` Phase Narrative

FR-09.1: `context_cycle_review` MUST query `CYCLE_EVENTS` ordered by `seq` for the given `cycle_id` (derived from `feature_cycle`).

FR-09.2: When `CYCLE_EVENTS` rows exist for the queried feature cycle, the response MUST include a phase narrative section containing:
- Ordered list of cycle events with their `event_type`, `phase`, `outcome`, and `next_phase` values
- Rework detection: a flag or count per phase name that appears more than once in the sequence
- Per-phase category distribution: for each distinct `phase` value in `feature_entries`, a count of entries by category

FR-09.3: When no `CYCLE_EVENTS` rows exist for the queried feature cycle, the phase narrative section MUST be silently omitted. No placeholder or "not available" text is emitted.

FR-09.4: All existing `context_cycle_review` behavioral telemetry output (observation metrics, hotspot detection, baseline comparison) MUST be unchanged.

FR-09.5: `RetrospectiveReport` MUST be extended with an optional `phase_narrative: Option<PhaseNarrative>` field (`#[serde(skip_serializing_if = "Option::is_none")]`).

### FR-10: Cross-Cycle Comparison

FR-10.1: `context_cycle_review` MUST compute a cross-cycle comparison: the current feature's per-phase category distribution compared against the mean of all prior features that have phase-tagged data in `feature_entries`.

FR-10.2: The cross-cycle comparison MUST be silently omitted when fewer than 2 prior features have any phase-tagged `feature_entries` rows.

FR-10.3: "Prior features" excludes the current `feature_cycle` being reviewed.

FR-10.4: The comparison MUST be rendered as a per-phase table: for each phase token, the current feature's category counts vs. the mean count across prior features.

FR-10.5: The cross-cycle comparison result MUST be included in the `phase_narrative` section of `RetrospectiveReport` when present.

---

## Non-Functional Requirements

### NFR-01: Hook Latency

All `CYCLE_EVENTS` writes are fire-and-forget. The synchronous portion of the hook handler for `phase-end` (in-memory `SessionState` mutation) MUST complete in under 1ms. The DB write is async and MUST NOT block the hook transport path.

### NFR-02: `current_phase` Mutation Ordering

`SessionState.current_phase` MUST be mutated synchronously within the UDS listener's per-session task before the handler returns. This guarantees that any `context_store` call processed after the `phase-end` event observes the updated phase. The mutation MUST NOT be queued behind the analytics drain (mitigates SR-01).

### NFR-03: Phase Capture at Enqueue

The `phase` value written to `feature_entries.phase` MUST be the value of `SessionState.current_phase` at the moment the feature entry event is enqueued (not at drain-flush time). This prevents phase skew when `current_phase` advances before the drain fires (mitigates SR-07).

### NFR-04: `seq` Monotonicity

`seq` values MUST be monotonically increasing per `cycle_id`. The implementation using `SELECT COALESCE(MAX(seq), -1) + 1` is safe when the UDS listener serializes events per session. The architect must verify or enforce per-`cycle_id` write serialization (see SR-02 in SCOPE-RISK-ASSESSMENT.md).

### NFR-05: Schema Migration Safety

Migration v14 → v15 MUST be idempotent. The `pragma_table_info` pre-check pattern established in prior migrations (#681, #836) MUST be followed for the `feature_entries.phase` column addition.

### NFR-06: Backward Compatibility

- Old `context_cycle` callers passing `keywords` are unaffected: the field is silently discarded.
- `context_cycle_review` callers querying pre-WA-1 features receive unchanged output (no phase narrative section emitted).
- `feature_entries` rows with `phase = NULL` are valid and remain queryable.

### NFR-07: Test Coverage

All new database operations require unit or integration tests:
- `CYCLE_EVENTS` INSERT (all three event types)
- `feature_entries.phase` column write (both NULL and non-NULL cases)
- Schema v14 → v15 migration idempotency
- `validate_cycle_params` for all new cases: `"phase-end"` type, `phase` format validation, `next_phase` format validation
- `CategoryAllowlist` after `"outcome"` removal

---

## Acceptance Criteria

Each criterion maps to one or more functional requirements and specifies a verification method.

| AC-ID | Criterion | Verification |
|-------|-----------|-------------|
| AC-01 | `CycleParams` has `phase`, `outcome`, `next_phase` fields and no `keywords` field. Deserialization of JSON with `keywords` succeeds and silently discards it. | Unit test: deserialize `{"type":"start","topic":"crt-025","keywords":["k"]}` — succeeds with `keywords` not accessible on struct |
| AC-02 | `validate_cycle_params` accepts `"phase-end"` as a valid type, returning `CycleType::PhaseEnd`. `"start"` and `"stop"` remain valid. All other values (e.g., `"pause"`, `"restart"`, `""`) are rejected with a descriptive error naming the three valid values. | Unit tests for each valid and a sample of invalid `type` values |
| AC-03 | A `phase` value containing a space character is rejected. A `phase` value exceeding 64 characters is rejected. Normalization to lowercase is applied before validation. `"Scope"` becomes `"scope"`. | Unit tests: `"scope review"` rejected; `"a".repeat(65)` rejected; `"Scope"` normalizes to `"scope"` |
| AC-04 | `phase` on any event type, if provided and valid, is stored in `CYCLE_EVENTS.phase`. If absent on `"start"`, stored as `NULL`. | Integration test: insert start event without `phase`; verify `cycle_events.phase IS NULL` |
| AC-05 | `next_phase` on `"start"` immediately sets `SessionState.current_phase = Some(next_phase)`. A subsequent `context_store` call in the same session receives the non-NULL phase for tagging. | Integration test: `start` with `next_phase="scope"`, then `context_store`; verify `feature_entries.phase = "scope"` |
| AC-06 | `next_phase` on `"phase-end"` updates `SessionState.current_phase`. If `next_phase` is absent on `"phase-end"`, `current_phase` is left unchanged. | Unit test on `SessionState` mutation logic |
| AC-07 | `"stop"` event clears `SessionState.current_phase` to `None`. A subsequent `context_store` in the same session receives `phase = NULL`. | Integration test |
| AC-08 | Each `context_cycle` call produces exactly one INSERT to `CYCLE_EVENTS` with a monotonically increasing `seq` value scoped to the `cycle_id`. Three sequential calls for the same topic produce `seq = 0, 1, 2`. | Integration test: 3 calls → verify rows with seq 0, 1, 2 |
| AC-09 | `context_store` writes the current `SessionState.current_phase` to `feature_entries.phase` at insert time. Entries stored before any phase transition receive `phase = NULL`. | Integration tests for both NULL and non-NULL phase cases |
| AC-10 | Schema version advances from 14 to 15. Migration is idempotent (running twice produces no error and leaves schema unchanged). `CYCLE_EVENTS` table and `feature_entries.phase` column exist after migration. | Migration integration test covering v14 → v15 |
| AC-11 | `create_tables_if_needed` in `db.rs` creates `cycle_events` table and `feature_entries` with `phase` column on a fresh database. | Fresh-DB integration test |
| AC-12 | `context_cycle_review` response includes a phase narrative section when `CYCLE_EVENTS` data exists: ordered phase list, rework flag for repeated phases, per-phase category counts. | Integration test: seed `CYCLE_EVENTS` rows, call `context_cycle_review`, assert phase narrative present |
| AC-13 | `context_cycle_review` response is unchanged (no phase section) when no `CYCLE_EVENTS` data exists for the queried feature cycle. | Integration test: pre-WA-1 feature cycle, assert response has no phase narrative field |
| AC-14 | `context_cycle_review` cross-cycle comparison is included when 2 or more prior features have phase-tagged `feature_entries` rows. Silently omitted when fewer than 2 prior features have phase data. | Integration tests for both the threshold-met and threshold-not-met cases |
| AC-15 | `"outcome"` is removed from `CategoryAllowlist`. `context_store` with `category = "outcome"` returns `ServerError::InvalidCategory`. `CategoryAllowlist::new()` has 7 categories. | Unit test on `CategoryAllowlist::new()`; integration test on `context_store` rejection |
| AC-16 | Hook path handles `"phase-end"` events by emitting `cycle_phase_end` event type and validating params. On validation failure, hook logs a warning and falls through to generic observation path without hard-failing. | Unit test: invalid `phase` in hook event → warning logged, no error returned to transport |
| AC-17 | All new database operations are tested: `CYCLE_EVENTS` insert for all three event types, `feature_entries` phase column write, schema v14 → v15 migration idempotency. | See AC-08, AC-09, AC-10 test coverage |

---

## User Workflows

### Workflow 1: Standard Feature Cycle with Phase Signals

```
coordinator → context_cycle(type="start", topic="crt-025", next_phase="scope")
             → SessionState.current_phase = "scope"
             → CYCLE_EVENTS row: (cycle_id="crt-025", seq=0, event_type="cycle_start",
                                   phase=NULL, next_phase="scope")

agent       → context_store(category="decision", ...) [while current_phase="scope"]
             → feature_entries row: (feature_id="crt-025", entry_id=N, phase="scope")

coordinator → context_cycle(type="phase-end", topic="crt-025", phase="scope",
                             outcome="no variances", next_phase="design")
             → SessionState.current_phase = "design"
             → CYCLE_EVENTS row: (seq=1, event_type="cycle_phase_end",
                                   phase="scope", outcome="no variances", next_phase="design")

coordinator → context_cycle(type="stop", topic="crt-025", phase="testing",
                             outcome="all tests pass")
             → SessionState.current_phase = None
             → CYCLE_EVENTS row: (seq=N, event_type="cycle_stop",
                                   phase="testing", outcome="all tests pass")
```

### Workflow 2: Phase Narrative Retrieval

```
agent → context_cycle_review(feature_cycle="crt-025")
      → engine queries CYCLE_EVENTS for cycle_id="crt-025"
      → engine queries FEATURE_ENTRIES for feature_id="crt-025" grouped by phase
      → response includes:
          - phase narrative: ordered event list with rework detection
          - per-phase category distribution
          - cross-cycle comparison (if ≥ 2 prior features have phase data)
```

### Workflow 3: Pre-WA-1 Feature Review (Backward Compatible)

```
agent → context_cycle_review(feature_cycle="col-022")  [no CYCLE_EVENTS rows]
      → engine queries CYCLE_EVENTS: 0 rows
      → response: all existing telemetry sections present; phase narrative section absent
```

### Workflow 4: Outcome Category Store (Now Rejected)

```
agent → context_store(category="outcome", ...)
      → CategoryAllowlist.validate("outcome") → Err(InvalidCategory)
      → MCP returns error to caller
```

---

## Constraints

| ID | Constraint | Source |
|----|-----------|--------|
| C-01 | Phase string format: no spaces, lowercase after normalization, max 64 chars. Hard requirement from W3-1 GNN training pipeline. | SCOPE §Constraints |
| C-02 | `validate_cycle_params` must return `Result<ValidatedCycleParams, String>` (not `ServerError`). Hook path cannot use `ServerError`. | SCOPE §Constraints, ADR-004 |
| C-03 | `ImplantEvent` wire protocol is unchanged. New fields travel as payload map keys. No struct changes in `unimatrix-engine`. | SCOPE §Non-Goals |
| C-04 | `sessions.keywords` column is left in place; stop populating it. Its removal requires a more invasive migration, tracked in a follow-up. | SCOPE §Non-Goals |
| C-05 | No backfill of existing `feature_entries` rows. Pre-existing rows get `phase = NULL`. | SCOPE §Non-Goals |
| C-06 | No changes to `context_store` wire protocol. Phase tagging is automatic from in-memory `SessionState.current_phase`. | SCOPE §Non-Goals |
| C-07 | No changes to `context_cycle_review` behavioral telemetry pipeline (observation metrics, hotspot detection, baseline comparison). | SCOPE §Non-Goals |
| C-08 | Schema migration must use `pragma_table_info` pre-check before `ALTER TABLE ADD COLUMN` on `feature_entries`. | SCOPE §Constraints |
| C-09 | `seq` is computed as `COALESCE(MAX(seq), -1) + 1` scoped to `cycle_id`. Safe under UDS listener per-session serialization assumption. | SCOPE §Constraints |
| C-10 | Hook latency budget: 40ms total transport timeout. `CYCLE_EVENTS` INSERT is fire-and-forget. | SCOPE §Constraints |
| C-11 | `CategoryAllowlist` removal of `"outcome"` does not delete existing entries. Only new ingest is blocked. | SCOPE §Background |
| C-12 | `AnalyticsWrite::FeatureEntry` is `#[non_exhaustive]`. Adding a `phase` field to the variant is a structural change. External crate match arms with catch-all `_ => {}` are unaffected. Internal crate match arms must be updated. | Codebase: `analytics.rs` |

---

## Dependencies

### Upstream (must be complete)

| Dependency | Status | Notes |
|------------|--------|-------|
| WA-0 (`crt-024`) | Complete (PR #336) | Ranking signal fusion. Not touched by this feature. |
| col-023 (W1-5) | Complete (PR #332) | `observation_metrics.domain_metrics_json`. Independent of `feature_entries` and `CYCLE_EVENTS`. |

### Downstream (depend on this feature)

| Feature | Dependency |
|---------|-----------|
| WA-2 | Consumes `SessionState.current_phase` for phase-conditioned category affinity boosting. WA-1 must ship first. |
| W3-1 | Consumes `FEATURE_ENTRIES.phase` as supervised GNN training labels. WA-1 must accumulate data before W3-1 training begins. |
| WA-4 | Phase-conditioned proactive injection uses `SessionState.current_phase` for cache rebuild at phase transitions. |

### Crate Dependencies

| Crate | Usage |
|-------|-------|
| `unimatrix-store` | Schema migration, `CYCLE_EVENTS` write, `feature_entries.phase` write, `AnalyticsWrite::FeatureEntry` variant update |
| `unimatrix-server` | `CycleParams`, `ValidatedCycleParams`, `CycleType`, `CategoryAllowlist`, `SessionState`, UDS listener, `context_cycle_review` handler |
| `unimatrix-observe` | `RetrospectiveReport` extension with `phase_narrative` field |
| SQLite (rusqlite 0.34 bundled / sqlx) | Schema v15 migration |

---

## NOT In Scope

The following are explicitly excluded to prevent scope creep:

- **No `context_store` wire protocol changes.** Callers pass no new fields; phase comes from `SessionState`.
- **No WA-2 category histogram boosting.** That is a separate feature dependent on this one.
- **No semantic interpretation of phase strings.** The engine stores opaque labels. Protocol consistency is enforced upstream, not by the engine.
- **No backfill of `feature_entries.phase` for pre-WA-1 rows.** `NULL` is correct for historical data.
- **No removal of `sessions.keywords` column.** Column is left in place; stop populating. Removal tracked in follow-up.
- **No removal of `outcome_tags.rs` file.** The file is retained; its removal is tracked in GH #338.
- **No deletion of existing `outcome`-category entries.** Only new ingest is blocked.
- **No changes to behavioral telemetry pipeline** (`SqlObservationSource`, detection rules, hotspot pipeline, baseline comparison computation).
- **No W3-1 GNN implementation.** This feature only produces the training data; the GNN is in a later wave.
- **No changes to `ImplantEvent` struct in `unimatrix-engine`.** New fields travel through the existing payload map.
- **No hook binary changes beyond handling `cycle_phase_end` event type.**

---

## Open Questions

None. All questions resolved per SCOPE.md §Open Questions.

The SR-02 concurrency concern (`seq` monotonicity under concurrent sessions sharing a `cycle_id`) is delegated to the architect: the architect must decide during design whether UDS listener per-`cycle_id` write serialization is structurally enforced or whether `seq` is treated as advisory with `timestamp` as the true ordering at query time.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for phase tagging, schema migration, analytics drain, session state, acceptance criteria patterns — found relevant ADRs (#1273 col-022 wire protocol reuse, #681/#836 migration patterns, #2125 analytics drain visibility), lesson-learned #981 (NULL feature_cycle silent failure), and pattern #2987 (keywords inert — confirms zero-risk removal). No prior phase-tagging or `CYCLE_EVENTS` precedent exists; this is a new domain.
