# SPECIFICATION: col-028 — Unified Phase Signal Capture (Read-Side + query_log)

## Objective

Phase is the highest-signal discrete feature for knowledge surfacing quality. Today, every
read-side MCP tool call (`context_search`, `context_lookup`, `context_get`,
`context_briefing`) emits phase-free usage events, and the `query_log` table has no `phase`
column, blocking the phase-conditioned frequency table (ass-032 Loop 2), Thompson Sampling
per-(phase, entry) arms, and gap detection. This feature resolves both gaps atomically via
shared infrastructure: a free-function phase helper used by all four read-side handlers, a
corrected access_weight for `context_get` and `context_briefing`, a dedup-bypass guard for
weight-0 briefing events, a new `confirmed_entries` field on `SessionState`, and a
v16→v17 schema migration adding `query_log.phase`.

---

## Functional Requirements

**FR-01** — `context_search`, `context_lookup`, `context_get`, and `context_briefing` must
each call `current_phase_for_session` before any `await` in their handler body and pass the
result as `current_phase` in their `UsageContext`.

**FR-02** — `current_phase_for_session` must be a free function (not a method on
`UnimatrixHandler`) with signature:
```
pub(crate) fn current_phase_for_session(
    registry: &SessionRegistry,
    session_id: Option<&str>,
) -> Option<String>
```
The function returns `registry.get_state(session_id?)?.current_phase.clone()`, or `None`
if `session_id` is `None` or no matching session is found. It must not hold any lock across
an `await`.

**FR-03** — `context_get` must use `access_weight: 2` (changed from 1). The existing
implicit helpful vote (`params.helpful.or(Some(true))`) is unchanged.

**FR-04** — `context_briefing` must use `access_weight: 0` (changed from 1). The
`AccessSource::Briefing` routing is unchanged.

**FR-05** — `record_briefing_usage` in `services/usage.rs` must return immediately if
`ctx.access_weight == 0`, before calling `filter_access`, so that no dedup slot in
`UsageDedup.access_counted` is consumed for the briefing call. This is the D-01 guard and
the EC-04 contract enforcement point.

**FR-06** — `SessionState` must gain a `confirmed_entries: HashSet<u64>` field, initialised
to `HashSet::new()` in `register_session`. The field is in-memory only; never persisted.

**FR-07** — `SessionRegistry` must expose `record_confirmed_entry(&self, session_id: &str,
entry_id: u64)`, following the synchronous lock-and-mutate pattern of
`record_category_store`. This method inserts `entry_id` into the session's
`confirmed_entries` set.

**FR-08** — `context_get` must call `record_confirmed_entry` for the retrieved entry ID
after a successful retrieval, for every call regardless of the `helpful` vote.

**FR-09** — `context_lookup` must call `record_confirmed_entry` when `target_ids.len() == 1`
(request-side cardinality). Multi-ID lookup must not update `confirmed_entries`.

**FR-10** — `CURRENT_SCHEMA_VERSION` in `migration.rs` must be bumped from 16 to 17.

**FR-11** — The v16→v17 migration branch in `run_main_migrations` must:
1. Guard the `ALTER TABLE` with a `pragma_table_info` pre-check for `query_log.phase`.
2. Execute `ALTER TABLE query_log ADD COLUMN phase TEXT` if the column is absent.
3. Execute `CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)`.
4. Update the `schema_version` counter to 17.

**FR-12** — `AnalyticsWrite::QueryLog` variant in `analytics.rs` must gain a
`phase: Option<String>` field.

**FR-13** — The SQL INSERT for `query_log` in `analytics.rs` must add `phase` as the final
positional parameter (`?9`), bound after `source`. No existing bind indices must change.

**FR-14** — `QueryLogRecord` struct in `query_log.rs` must gain a `phase: Option<String>`
field.

**FR-15** — `QueryLogRecord::new()` must accept `phase: Option<String>` as its final
parameter and assign it to the struct field.

**FR-16** — `insert_query_log` in `query_log.rs` must pass `record.phase.clone()` in the
`AnalyticsWrite::QueryLog` variant.

**FR-17** — Both SELECT column lists in `scan_query_log_by_sessions` and
`scan_query_log_by_session` must include `phase` as the tenth column (index 9). The
`row_to_query_log` deserializer must read index 9 as `Option<String>`.

**FR-18** — The `context_search` handler must snapshot phase before the `spawn_blocking`
for the query log write, using the same `get_state` call used for the `UsageContext` phase.
The single snapshot variable must be shared between both uses; two `get_state` calls at the
same site are not permitted.

**FR-19** — The UDS call site at `uds/listener.rs:1324` must pass `None` for `phase` in
`QueryLogRecord::new(...)`. This is a compile-fix only; no phase semantics change for UDS.

**FR-20** — The `eval/scenarios/tests.rs` helper `insert_query_log_row` and all 15+
call sites that construct `query_log` rows directly must be updated to include a `phase`
column binding (passing `NULL` / `None`). All must compile without warnings.

**FR-21** — The `make_query_log` struct literal in `mcp/knowledge_reuse.rs` (test helper)
must be updated to include `phase: None`.

---

## Non-Functional Requirements

**NFR-01 (Performance)** — `current_phase_for_session` acquires the `SessionRegistry`
mutex for microseconds (clone of `Option<String>`). It must not be called more than once
per handler invocation. The single clone is sufficient for both `UsageContext.current_phase`
and `phase_for_log` at the `context_search` write site.

**NFR-02 (Correctness — race-condition contract)** — The phase snapshot must be the first
statement in the handler body (or the first statement before the first `await`). Per ADR-001
(crt-025) and ADR-002 (col-028): if phase capture appears after any `await`, a concurrent
`context_cycle(start)` could update `SessionState.current_phase` and the wrong phase would
be attributed. Compiler cannot enforce this; delivery must verify by inspection.

**NFR-03 (Backward compatibility)** — Pre-existing `query_log` rows must read back with
`phase = None` after migration. NULL in the SQLite column maps to `None` in
`QueryLogRecord.phase`. No backfill is performed.

**NFR-04 (Idempotency)** — The v16→v17 migration is idempotent. Running it on a database
that already has `query_log.phase` must succeed without error (enforced by the
`pragma_table_info` pre-check for `ALTER TABLE` and `CREATE INDEX IF NOT EXISTS`).

**NFR-05 (File size)** — `mcp/tools.rs` must not exceed 500 lines after all four call-site
changes. Delivery must verify current line count before making changes and split the module
if approaching the limit.

**NFR-06 (No scoring pipeline changes)** — `w_phase_explicit` remains 0.0 per ADR-003.
No changes to the re-ranking formula.

---

## Acceptance Criteria

AC IDs are stable across design, delivery, and test phases. Criteria AC-01 through AC-20
are carried forward unchanged from SCOPE.md. AC-21 through AC-24 close SR-01, SR-02, SR-03,
and SR-04/SR-06.

### Read-Side Phase Capture (AC-01–AC-04)

**AC-01** — `context_search` passes `current_phase: Some(phase)` in `UsageContext` when
the active session has a phase set; passes `None` when no session or no phase.
Verification: unit test with a mock `SessionRegistry`.

**AC-02** — `context_lookup` passes `current_phase: Some(phase)` in `UsageContext` when
the active session has a phase set; passes `None` when no session or no phase.
Verification: unit test with a mock `SessionRegistry`.

**AC-03** — `context_get` passes `current_phase: Some(phase)` in `UsageContext` when the
active session has a phase set; passes `None` when no session or no phase.
Verification: unit test with a mock `SessionRegistry`.

**AC-04** — `context_briefing` passes `current_phase: Some(phase)` in `UsageContext` when
the active session has a phase set; passes `None` when no session or no phase.
Verification: unit test with a mock `SessionRegistry`.

### Access Weight Corrections (AC-05–AC-06)

**AC-05** — `context_get` uses `access_weight: 2`. A unit test confirms that an entry
fetched via `context_get` produces `access_count = 2` on first access (i.e., the entry is
not in the dedup set before the call).
Verification: unit test inspecting `access_count` increment.

**AC-06** — `context_briefing` uses `access_weight: 0`. A unit test confirms that a
briefing call does not increment `access_count` for any returned entry.
Verification: unit test inspecting `access_count` before and after briefing call.

### D-01 Guard (AC-07)

**AC-07** — A briefing call on entry X followed by a `context_get` on entry X increments
`access_count` by 2 (weight=2 for get). The briefing call must not consume the dedup slot.
Verification: integration test issuing briefing then `context_get` and asserting
`access_count = 2` (not 0).

### confirmed_entries (AC-08–AC-11)

**AC-08** — `SessionState` has `confirmed_entries: HashSet<u64>` initialised empty on
`register_session`.
Verification: unit test calling `register_session` and asserting `confirmed_entries` is
empty.

**AC-09** — After `context_get` for entry ID X, `confirmed_entries` for that session
contains X.
Verification: unit test calling `context_get` handler and inspecting session state.

**AC-10** — After `context_lookup` with a single target ID X, `confirmed_entries` contains
X. After a multi-target lookup (two or more IDs in request), `confirmed_entries` is not
updated.
Verification: two unit tests — one single-ID, one multi-ID.

**AC-11** — `context_lookup` access_weight remains 2 (unchanged from pre-feature state).
Verification: static code review plus existing lookup tests pass.

### Phase Snapshot Contract (AC-12)

**AC-12** — Phase snapshot is taken synchronously before any `await` in each of the four
read-side handlers. The call uses the shared free function `current_phase_for_session`, not
four independent `get_state` sequences.
Verification: code review (delivery gate checklist item). The free function is unit-testable
in isolation.

### Schema Migration (AC-13–AC-19)

**AC-13** — `CURRENT_SCHEMA_VERSION = 17`. A unit test in `migration.rs` asserts
`CURRENT_SCHEMA_VERSION == 17`.
Verification: `#[test] fn test_current_schema_version_is_17()`.

**AC-14** — After opening a v17 database (fresh or migrated), the `query_log` table has a
`phase TEXT` column and `idx_query_log_phase` index present.
Verification: `migration_v16_to_v17.rs` tests T-V17-01 and T-V17-03.

**AC-15** — The v16→v17 migration is idempotent. Running it on a database that already has
the `phase` column does not fail or add a duplicate column.
Verification: `migration_v16_to_v17.rs` test T-V17-04 (idempotency).

**AC-16** — A `context_search` call in a session with active phase "delivery" writes
`phase = "delivery"` to the `query_log` row. A call with no active session writes
`phase = NULL`.
Verification: integration test.

**AC-17** — `QueryLogRecord` has `phase: Option<String>`. `scan_query_log_by_session`
returns records with the correct phase value read back from the database.
`row_to_query_log` correctly deserializes column index 9 as `Option<String>`.
Verification: integration test writing a row with `phase = Some("design".to_string())` and
reading it back via `scan_query_log_by_session`. This is the primary guard against
positional drift across analytics.rs INSERT, both SELECTs, and the deserializer (SR-01).

**AC-18** — Pre-existing `query_log` rows (inserted before migration) read back with
`phase = None` after v16→v17 migration.
Verification: `migration_v16_to_v17.rs` inserts a row pre-migration, migrates, and asserts
the row's phase is `None`.

**AC-19** — A migration integration test file `migration_v16_to_v17.rs` following the
`migration_v15_to_v16.rs` pattern covers:
- T-V17-01: fresh DB initialises at v17 with `phase` column present.
- T-V17-02: v16→v17 migration from a v16 fixture adds `phase` column.
- T-V17-03: `idx_query_log_phase` index exists after migration.
- T-V17-04: idempotency — running migration on already-v17 DB succeeds.
- T-V17-05: pre-existing rows read back with `phase = None`.
- T-V17-06: schema_version counter = 17 after migration.
Verification: all six tests pass.

### Test Infrastructure (AC-20)

**AC-20** — `make_state_with_rework` and all related test helpers in `infra/session.rs`
and adjacent test modules are updated to include `confirmed_entries: HashSet::new()`. All
existing tests pass without modification.
Verification: `cargo test --workspace` passes with no new failures.

### SR-01 Atomic Change Surface (AC-21)

**AC-21** — The `analytics.rs` INSERT (`?9` = `phase`), `scan_query_log_by_sessions`
SELECT (tenth column), `scan_query_log_by_session` SELECT (tenth column), and
`row_to_query_log` deserializer (index 9) are updated in the same commit. No partial state
is permitted where one diverges from the others.
Verification: AC-17 read-back round-trip test. If any of the four sites is missed, the
integration test fails with a runtime column-index error.

### SR-02 Migration Test Cascade (AC-22)

**AC-22** — The following migration test files reference `schema_version = 16` (via
`assert_eq!` or comment) and must each be audited and updated before gate:

| File | Required change |
|------|----------------|
| `crates/unimatrix-store/tests/migration_v15_to_v16.rs` | Update `test_current_schema_version_is_16` test name and all `assert_eq!(..., 16)` assertions to 17; update inline comments. |
| `crates/unimatrix-store/tests/migration_v14_to_v15.rs` | The `>= 15` assertions already tolerate version bumps (pattern #2933). Confirm no `== 16` assertions are present; update any inline comments that reference "bumped to 16". |

No other migration test files contain `== 16` assertions. The delivery agent must grep for
`schema_version.*== 16` across all test files before closing this AC.
Verification: `grep -r 'schema_version.*== 16' crates/` returns zero matches in final state.

### SR-03 UDS Compile Fix (AC-23)

**AC-23** — `uds/listener.rs:1324` passes `None` for `phase` in `QueryLogRecord::new(...)`.
This is a one-line change: the existing six-argument call gains a seventh argument `None`.
No phase semantics change; UDS rows continue to write `phase = NULL`.
Verification: `cargo build --workspace` compiles without error.

### SR-04 confirmed_entries Contract Documentation (AC-24)

**AC-24** — The `confirmed_entries` field in `SessionState` carries a doc comment that
precisely states its semantic contract:
- Populated by `context_get` (always) and `context_lookup` (single-ID requests only,
  request-side cardinality, not result-set cardinality).
- Not populated by briefing, search, write, or mutation tools.
- In-memory only; reset on `register_session`; never persisted.
- First consumer is Thompson Sampling (future feature). The contract must not be silently
  reinterpreted by future consumers.
Verification: doc comment present on the field in source code.

---

## Domain Models

### SessionState (infra/session.rs)

The per-session in-memory state container. After this feature:

```
SessionState {
    // existing fields (unchanged)
    session_id: String,
    role: Option<String>,
    feature: Option<String>,
    injection_history: Vec<InjectionRecord>,
    coaccess_seen: HashSet<Vec<u64>>,
    compaction_count: u32,
    signaled_entries: HashSet<u64>,    // col-009: implicit signal tracking
    rework_events: Vec<ReworkEvent>,
    agent_actions: Vec<SessionAction>,
    last_activity_at: u64,
    topic_signals: HashMap<String, TopicTally>,
    current_phase: Option<String>,     // crt-025: active workflow phase
    category_counts: HashMap<String, u32>,
    current_goal: Option<String>,      // col-025: active feature cycle goal

    // NEW — col-028
    /// Entry IDs explicitly retrieved by the agent this session.
    ///
    /// Populated by `context_get` (always) and `context_lookup` (single-ID
    /// requests only — request-side cardinality, not result-set cardinality).
    /// Not populated by briefing, search, write, or mutation tools.
    /// In-memory only; reset on register_session; never persisted.
    /// First consumer: Thompson Sampling (future feature).
    pub confirmed_entries: HashSet<u64>,
}
```

### SessionRegistry (infra/session.rs)

Thread-safe `HashMap<String, SessionState>` behind a `Mutex`. New method added:

```
pub fn record_confirmed_entry(&self, session_id: &str, entry_id: u64)
```

### UsageContext (services/usage.rs)

The context struct passed to `UsageService::record_access`. The `current_phase` field
already exists; this feature populates it for all four read-side tools (previously always
`None` for read-side tools).

### QueryLogRecord (unimatrix-store/src/query_log.rs)

Captures search telemetry for a single `context_search` call. After this feature:

```
pub struct QueryLogRecord {
    pub query_id: i64,           // AUTOINCREMENT, 0 on insert
    pub session_id: String,
    pub query_text: String,
    pub ts: u64,
    pub result_count: i64,
    pub result_entry_ids: String,
    pub similarity_scores: String,
    pub retrieval_mode: String,
    pub source: String,
    pub phase: Option<String>,   // NEW — col-028: workflow phase at query time
}
```

### AnalyticsWrite::QueryLog (unimatrix-store/src/analytics.rs)

Variant of the analytics write enum routed through the bounded channel. After this feature:

```
QueryLog {
    session_id: String,
    query_text: String,
    ts: i64,
    result_count: i64,
    result_entry_ids: Option<String>,
    similarity_scores: Option<String>,
    retrieval_mode: Option<String>,
    source: String,
    phase: Option<String>,   // NEW — col-028
}
```

### Phase

An `Option<String>` snapshotted from `SessionState.current_phase` at MCP call time. `None`
means no active phase signal has been emitted in the session. Non-`None` values are
free-form strings set by `context_cycle(start)` (e.g. "design", "delivery", "bugfix").
NULL in `query_log.phase` means "no phase" and must not be treated as a distinct phase label
by downstream analytics consumers.

---

## User Workflows

### Workflow 1 — Agent calls context_search during a phased session

1. Agent sends `context_search` with active `session_id`.
2. Handler calls `current_phase_for_session(&self.session_registry, session_id)` —
   synchronously, before any `await`.
3. Phase value (e.g. `Some("delivery")`) is stored in a local variable.
4. `UsageContext { current_phase: phase.clone(), access_weight: 1, ... }` is passed to
   `UsageService::record_access`.
5. `phase` is also passed to `QueryLogRecord::new(...)` as the final argument.
6. `query_log` row is written with `phase = "delivery"`.
7. Downstream analytics can filter `query_log` by phase.

### Workflow 2 — Agent calls context_briefing then context_get on the same entry

1. Agent sends `context_briefing`. Handler captures phase, constructs
   `UsageContext { access_weight: 0, current_phase: Some("design"), ... }`.
2. `UsageService::record_access` routes to `record_briefing_usage`.
3. D-01 guard: `ctx.access_weight == 0` is true; function returns immediately.
4. `UsageDedup.access_counted` does not contain `(agent_id, entry_X)`.
5. Agent sends `context_get` for `entry_X`. Handler captures phase, constructs
   `UsageContext { access_weight: 2, helpful: Some(true), current_phase: Some("design"), ... }`.
6. `record_mcp_usage` calls `filter_access` — entry X passes (dedup slot was not consumed).
7. `access_count` for entry X increments by 2.
8. `record_confirmed_entry(session_id, entry_X)` is called; entry X enters
   `confirmed_entries`.

### Workflow 3 — Agent calls context_lookup with a single ID

1. Handler calls `current_phase_for_session` before `await`.
2. After retrieval, `target_ids.len() == 1` — `record_confirmed_entry` is called.
3. `UsageContext { access_weight: 2, current_phase: ... }` is passed (weight unchanged).

### Workflow 4 — Agent calls context_lookup with multiple IDs

1. Phase is captured (same as above).
2. After retrieval, `target_ids.len() > 1` — `record_confirmed_entry` is NOT called.
3. `confirmed_entries` is unchanged.

### Workflow 5 — Database opens at v16, migration runs

1. `migrate_if_needed` reads `schema_version = 16`.
2. `run_main_migrations` enters the `current_version < 17` branch.
3. `pragma_table_info` query checks for `query_log.phase` column.
4. Column is absent; `ALTER TABLE query_log ADD COLUMN phase TEXT` executes.
5. `CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)` executes.
6. `schema_version` counter is updated to 17.
7. Transaction commits.
8. All pre-existing rows have `phase = NULL`.

---

## Exact Signatures and Code Contracts

This section provides exact text for load-bearing declarations. Downstream agents must use
these verbatim.

### current_phase_for_session (FR-02, D-04, ADR-001 col-028)

```rust
pub(crate) fn current_phase_for_session(
    registry: &SessionRegistry,
    session_id: Option<&str>,
) -> Option<String> {
    session_id.and_then(|sid| registry.get_state(sid))
              .and_then(|s| s.current_phase.clone())
}
```

Location: `crates/unimatrix-server/src/mcp/tools.rs` (free function, module scope).

### SessionState.confirmed_entries field declaration (FR-06, D-03, AC-24)

```rust
// col-028 fields
/// Entry IDs explicitly retrieved by the agent this session.
///
/// Populated by `context_get` (always) and `context_lookup` (single-ID
/// requests only — request-side cardinality, not result-set cardinality).
/// Not populated by briefing, search, write, or mutation tools.
/// In-memory only; reset on register_session; never persisted.
/// First consumer: Thompson Sampling (future feature).
pub confirmed_entries: HashSet<u64>,
```

Location: `SessionState` struct in `crates/unimatrix-server/src/infra/session.rs`.
Initialisation in `register_session`: `confirmed_entries: HashSet::new()`.

### D-01 guard in record_briefing_usage (FR-05, AC-07, ADR-003 col-028)

```rust
fn record_briefing_usage(&self, entry_ids: &[u64], ctx: UsageContext) {
    // D-01 guard (col-028): weight-0 is an offer-only event.
    // Must appear before filter_access to avoid burning the dedup slot.
    // EC-04 contract enforcement: access_count is NOT incremented for briefing.
    if ctx.access_weight == 0 {
        return;
    }
    let agent_id = ctx.agent_id.clone().unwrap_or_default();
    // ... existing body continues unchanged ...
```

Location: `crates/unimatrix-server/src/services/usage.rs`, top of `record_briefing_usage`.

### QueryLogRecord field addition and constructor signature (FR-14, FR-15, AC-17)

Field addition:
```rust
pub phase: Option<String>,  // col-028: workflow phase at query time; None for UDS rows
```

New constructor signature:
```rust
pub fn new(
    session_id: String,
    query_text: String,
    entry_ids: &[u64],
    similarity_scores: &[f64],
    retrieval_mode: &str,
    source: &str,
    phase: Option<String>,   // NEW — col-028
) -> Self
```

The `phase` field is assigned directly from the argument. No computation.

### Migration SQL — v16→v17 (FR-11, AC-14, AC-15)

```rust
// v16 → v17: query_log.phase column (col-028)
if current_version < 17 {
    let has_phase_column: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'",
    )
    .fetch_one(&mut **txn)
    .await
    .map(|count| count > 0)
    .unwrap_or(false);

    if !has_phase_column {
        sqlx::query("ALTER TABLE query_log ADD COLUMN phase TEXT")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query("UPDATE counters SET value = 17 WHERE name = 'schema_version'")
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
}
```

### analytics.rs INSERT update (FR-12, FR-13, AC-21)

The `AnalyticsWrite::QueryLog` match arm SQL must become:

```sql
INSERT INTO query_log
    (session_id, query_text, ts, result_count,
     result_entry_ids, similarity_scores, retrieval_mode, source, phase)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
```

Bind order: existing eight binds unchanged; `.bind(phase)` appended as ninth bind.

### query_log.rs SELECT updates (FR-17, AC-17, AC-21)

Both SELECT statements must include `phase` as the tenth column:

```sql
SELECT query_id, session_id, query_text, ts, result_count,
       result_entry_ids, similarity_scores, retrieval_mode, source, phase
FROM query_log
WHERE ...
```

`row_to_query_log` must read index 9:

```rust
phase: row.try_get::<Option<String>, _>(9)
          .map_err(|e| StoreError::Database(e.into()))?,
```

---

## Constraints

**C-01 — Synchronous phase snapshot before any `await` (ADR-001 crt-025, ADR-002 col-028)**
The `current_phase_for_session` call must be the first statement in the handler body (before
any `await`). `get_state` returns a `Clone`, not a reference. No lock is held across an
`await`.

**C-02 — pragma_table_info pre-check required**
SQLite does not support `ALTER TABLE ADD COLUMN IF NOT EXISTS`. The pre-check pattern is
mandatory for all ADD COLUMN migrations in this codebase. Deviation is not allowed.

**C-03 — D-01 guard must be in `record_briefing_usage`, not at the `AccessSource` dispatch level**
`record_briefing_usage` is the correct guard site per ADR-003 col-028. The guard intercepts
weight-0 before `filter_access` is called. Placing it at the dispatch level in `record_access`
is architecturally preferable for future-proofing (SR-07) but is out of scope for this
feature and would require additional ADR review.

**C-04 — Single `get_state` call per handler invocation**
`context_search` has two consumers of the phase value: `UsageContext.current_phase` and
`QueryLogRecord.phase`. Both must read from a single `get_state` result captured before
the first `await`. Two separate calls are prohibited.

**C-05 — Phase column added as last positional parameter**
`analytics.rs` uses positional `?1`..`?N` binding for `query_log`. The `phase` bind must be
added as `?9` after the existing eight binds. No existing bind index may change.

**C-06 — No changes to scoring pipeline**
`w_phase_explicit` remains 0.0 per ADR-003. The re-ranking formula is unchanged.

**C-07 — confirmed_entries has no consumer in this feature**
`record_confirmed_entry` is called; no code reads `confirmed_entries` in this feature.
Implementing a consumer is out of scope and constitutes a scope variance.

**C-08 — UDS call site: compile-fix only**
`uds/listener.rs:1324` must compile after the `QueryLogRecord::new` signature gains
`phase: Option<String>`. Pass `None`. No semantic changes to the UDS path.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `SessionRegistry::get_state` | Existing — read | Used by `current_phase_for_session`; returns `Clone` not ref |
| `SessionRegistry::record_category_store` | Existing — pattern | `record_confirmed_entry` follows this lock-and-mutate pattern |
| `UsageContext.current_phase` | Existing field | Already declared; this feature populates it for read-side tools |
| `UsageDedup.filter_access` | Existing | D-01 guard must precede this call in `record_briefing_usage` |
| `AnalyticsWrite::QueryLog` | Existing variant | Gains `phase: Option<String>` field |
| `SqlxStore::insert_query_log` | Existing | Passes `record.phase.clone()` to the variant |
| `pragma_table_info` pattern | Existing — v7→v8, v13→v14, v14→v15, v15→v16 | Identical pre-check idiom |
| `make_state_with_rework` | Existing test helper | Must be updated per pattern #3180 |
| `migration_v15_to_v16.rs` | Existing test file | Pattern to follow for `migration_v16_to_v17.rs` |

---

## NOT in Scope

- Changes to the scoring pipeline or `w_phase_explicit`.
- Any consumer of `confirmed_entries` (Thompson Sampling is a separate feature).
- Phase-conditioned frequency table (ass-032 Loop 2 — separate feature).
- Thompson Sampling per-(phase, entry) arms — separate feature.
- Gap detection — separate feature.
- Backfill of historical `query_log` rows — pre-existing rows get `phase = NULL`.
- Phase capture for `context_correct`, `context_deprecate`, `context_quarantine` (these are
  write/mutation tools, not read-side retrieval tools with phase-learning semantics).
- Phase capture for the UDS `insert_query_log` call site (no session registry reference in
  scope at that call site; UDS rows get `phase = NULL`).
- Persistence of `confirmed_entries` (in-memory only, consistent with all other
  `SessionState` fields).
- Moving the D-01 guard to the `AccessSource` dispatch level (SR-07 risk noted; out of scope
  pending architect review and ADR).
- Any change to `context_search`, `context_lookup`, `context_get`, or `context_briefing`
  behaviour beyond phase capture, weight corrections, and confirmed_entries recording.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for col-028 phase capture, session migration patterns,
  dedup slot, confirmed_entries — found ADR-001, ADR-002, ADR-003 col-028 (#3504, #3505,
  #3506), pattern #3503 (UsageDedup weight-0 gotcha), pattern #3510 (shared access_counted
  set), pattern #2933 (schema version cascade), pattern #3004 (analytics drain phase-snapshot
  integration test pattern). All findings directly applied.
