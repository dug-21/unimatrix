# col-028: Unified Phase Signal Capture (Read-Side + query_log)

## Problem Statement

Phase is the highest-signal discrete feature for knowledge surfacing quality. Three
downstream learning loops depend on knowing which workflow phase an agent was in when they
interacted with an entry: the phase-conditioned frequency table, Thompson Sampling
per-(phase, entry) arms, and gap detection. Today, phase is captured only on
`context_store` writes. Every read-side event is phase-context-free.

This feature resolves two related gaps that share the same root cause, the same source
pattern, and unlock the same three downstream consumers:

**Gap 1 — In-memory (#394)**: `UsageContext.current_phase` is always `None` for
`context_search`, `context_lookup`, `context_get`, and `context_briefing`. These tools
have access to `SessionState.current_phase` at call time but do not snapshot it.
Additionally, `context_get` access weight (1) does not reflect its semantically stronger
signal (deliberate full-content retrieval), and briefing access weight (1) overcounts by
crediting entries the agent may never have read as full-weight access events.

**Gap 2 — Persistence (#397)**: The `query_log` table has no `phase` column. Phase is
present in `SessionState.current_phase` at write time but is never persisted. All
downstream analytics consumers — phase-conditioned frequency table, gap detection — query
`query_log` and receive phase-free data regardless of what is captured in memory.

The phase-conditioned frequency table (ass-032 Loop 2) and Thompson Sampling features
are blocked on this data. Both changes are additive and backward-compatible.

Affected agents: every agent that calls any of the four read-side MCP tools, in every
session.

## Goals

1. All four read-side MCP tools capture `current_phase` at call time via a shared
   free-function helper `current_phase_for_session(&SessionRegistry, Option<&str>)`.
2. `context_get` access_weight corrected from 1 to 2 (deliberate full-content retrieval).
3. `context_briefing` access_weight corrected from 1 to 0 (offer event, not selection).
4. Weight-0 briefing calls do not consume a dedup slot (D-01 guard in `record_briefing_usage`).
5. `context_get` always adds the retrieved entry ID to `SessionState.confirmed_entries`.
6. `context_lookup` adds to `confirmed_entries` when `target_ids.len() == 1` (request-side
   cardinality, D-02).
7. `SessionState` gains `confirmed_entries: HashSet<u64>` initialised empty on session
   registration. No consumer in this feature (D-03).
8. Schema migration v16→v17: `ALTER TABLE query_log ADD COLUMN phase TEXT` with
   `idx_query_log_phase` index, guarded by `pragma_table_info` pre-check.
9. `CURRENT_SCHEMA_VERSION` bumped from 16 to 17.
10. All `insert_query_log` call sites in scope (MCP `context_search`) populate phase from
    a session state snapshot taken before `spawn_blocking`.

## Non-Goals

- **No changes to scoring pipeline** — `w_phase_explicit` remains 0.0 per ADR-003.
- **No consumers of `confirmed_entries`** — the field is added now so Thompson Sampling
  inherits populated data; the first consumer is a future feature.
- **No phase-conditioned frequency table** — separate downstream feature.
- **No Thompson Sampling** — separate downstream feature.
- **No gap detection** — separate downstream feature.
- **No backfill of historical `query_log` rows** — pre-existing rows get `phase = NULL`.
- **No changes to UDS `insert_query_log` call site** (`uds/listener.rs:1324`) — the UDS
  path does not have a session registry reference in the write scope. UDS rows continue
  to write `phase = NULL`. This is acceptable; UDS injection is not the primary analytics
  path for phase-conditioned queries.
- **No changes to `context_correct`, `context_deprecate`, `context_quarantine`** — these
  are write/mutation tools, not read-side retrieval tools with phase-learning semantics.
- **No persistence of `confirmed_entries`** — in-memory only, reset on session
  registration, consistent with all other `SessionState` fields.

## Background Research

### Phase Snapshot Pattern (crt-025, ADR-001, pattern #3027)

`context_store` (tools.rs:524–532) is the canonical implementation. Phase is read
synchronously before any `await`:

```rust
let session_state = ctx.audit_ctx.session_id.as_deref()
    .and_then(|sid| self.session_registry.get_state(sid));
let current_phase: Option<String> =
    session_state.as_ref().and_then(|s| s.current_phase.clone());
```

`get_state` returns a `Clone`, not a reference. All four read-side tools must follow this
same pattern via a shared free function (D-04 rationale: testable without handler
construction).

### SessionState Fields (infra/session.rs, confirmed)

Current fields (as of col-025, schema v16):

```rust
pub struct SessionState {
    // col-009
    pub signaled_entries: HashSet<u64>,
    // crt-025
    pub current_phase: Option<String>,
    // crt-026
    pub category_counts: HashMap<String, u32>,
    // col-025
    pub current_goal: Option<String>,
    // ... other fields
}
```

`confirmed_entries: HashSet<u64>` is a new field following the same pattern as
`signaled_entries` (HashSet, empty init, never persisted).

Pattern #3180: every new `SessionState` field requires updating `make_state_with_rework`
and related test helpers.

### Four Read-Side Call Sites (tools.rs, confirmed)

| Tool | Line | Current access_weight | Current current_phase |
|------|------|-----------------------|-----------------------|
| `context_search` | ~357–369 | 1 | None |
| `context_lookup` | ~473–485 | 2 | None |
| `context_get` | ~677–689 | 1 | None |
| `context_briefing` | ~1001–1014 | 1 | None |

`context_lookup` already uses weight 2 — no change needed for weight, only phase capture.

### UsageDedup Architecture (infra/usage_dedup.rs, confirmed)

`UsageDedup` contains a single `access_counted: HashSet<(String, u64)>` shared across
ALL access sources. `filter_access` is called in both `record_mcp_usage` and
`record_briefing_usage` against the same set. This confirms the D-01 collision: a
briefing appearance registers `(agent_id, entry_id)` in `access_counted`, consuming the
dedup slot. A subsequent `context_get` for the same entry is then filtered out (returns
empty `access_ids`) and produces zero access count increment.

The D-01 guard must be in `record_briefing_usage`: if `ctx.access_weight == 0`, return
immediately before calling `filter_access`, so no dedup slot is consumed.

### D-05 Findings (confirmed from source)

**D-05a — access_count arithmetic:**

`usage.rs` lines 169–187 use a flat_map repeat approach: each entry ID is repeated
`access_weight` times in `multiplied_all_ids` and `multiplied_access_ids`. For
`access_weight = 0`, the `<= 1` branch runs producing `entry_ids.to_vec()` in
`multiplied_all_ids` — one copy. The `UsageContext` doc comment (line 62–63) states
"A value of 0 silently drops the access increment (EC-04)" but this is a contract
declaration, not enforced by the current flat_map logic. The `record_briefing_usage`
D-01 guard (return early if weight == 0) is the correct enforcement point: it prevents
`record_usage_with_confidence` from being called at all for weight-0 events.

**D-05b — co-access pairs in record_briefing_usage:**

`record_briefing_usage` (usage.rs:313–349) contains ONLY a call to
`record_usage_with_confidence`. There is no `generate_pairs`, no
`filter_co_access_pairs`, no co-access recording of any kind. Briefing does not generate
co-access pairs today. At weight-0 with the D-01 guard, `record_usage_with_confidence`
is never called — so this remains true.

**D-05c — briefing and McpTool use the same dedup set:**

Confirmed: `filter_access` in both paths calls into the same `DedupState.access_counted`
HashSet. The collision scenario is real. D-01 guard is required and sufficient.

### query_log Write Sites (confirmed)

Two call sites for `insert_query_log`:

1. **`tools.rs:397`** (MCP `context_search`): `tokio::task::spawn_blocking`,
   `source = "mcp"`. In scope for this feature.
2. **`uds/listener.rs:1324`** (UDS injection): `spawn_blocking_fire_and_forget`,
   `source = "uds"`. Out of scope (see Non-Goals).

`QueryLogRecord` is constructed via `QueryLogRecord::new(...)` in both sites. Adding
`phase: Option<String>` to the constructor requires updating both call sites and all test
callers (`eval/scenarios/tests.rs` has 15+ calls to `QueryLogRecord::new` or
`insert_query_log_row`).

`AnalyticsWrite::QueryLog` variant (analytics.rs:51–60) and its SQL INSERT
(analytics.rs:480–495) must also be updated.

### Current Schema Version (confirmed)

`migration.rs:19`: `pub const CURRENT_SCHEMA_VERSION: u64 = 16;`

This feature introduces v16→v17.

### Migration Pattern (confirmed)

All column additions use the `pragma_table_info` pre-check (seen in v14→v15 for
`feature_entries.phase`, v15→v16 for `cycle_events.goal`, v13→v14 for
`domain_metrics_json`, v7→v8 for `pre_quarantine_status`):

```rust
let has_phase_column: bool = sqlx::query_scalar::<_, i64>(
    "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'",
)
.fetch_one(&mut **txn).await
.map(|count| count > 0)
.unwrap_or(false);

if !has_phase_column {
    sqlx::query("ALTER TABLE query_log ADD COLUMN phase TEXT")
        .execute(&mut **txn).await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
}
```

No backfill. Pre-existing rows get `phase = NULL`. Downstream consumers must handle NULL
as "no-phase session."

### scan_query_log Read Path

`query_log.rs` has two read methods: `scan_query_log_by_sessions` and
`scan_query_log_by_session`. Both use positional SELECT column lists. Adding `phase` as
column index 9 requires updating both SELECT statements and `row_to_query_log`.

## Proposed Approach

### Part 1 — SessionState and SessionRegistry (infra/session.rs)

Add `confirmed_entries: HashSet<u64>` to `SessionState`, initialised to `HashSet::new()`
in `register_session`. Add `record_confirmed_entry(&self, session_id: &str, entry_id: u64)`
to `SessionRegistry` following the synchronous lock-and-mutate pattern of
`record_category_store`. Update `make_state_with_rework` test helper per pattern #3180.

### Part 2 — Free Function and Four Call Sites (mcp/tools.rs)

Add `current_phase_for_session(registry: &SessionRegistry, session_id: Option<&str>) -> Option<String>`
as a free function (D-04). Call it before any `await` in each handler.

Changes per call site:

| Tool | Phase | Weight | confirmed_entries |
|------|-------|--------|-------------------|
| `context_search` | Capture | 1 (unchanged) | — |
| `context_lookup` | Capture | 2 (unchanged) | Insert if `target_ids.len() == 1` |
| `context_get` | Capture | 1 → 2 | Always insert |
| `context_briefing` | Capture | 1 → 0 | — |

### Part 3 — D-01 Guard (services/usage.rs)

In `record_briefing_usage`, add at the top:
```rust
if ctx.access_weight == 0 {
    return; // offer-only event; do not register dedup slot or increment access_count
}
```
This must appear before the `filter_access` call.

### Part 4 — Schema Migration v16→v17 (unimatrix-store)

1. Bump `CURRENT_SCHEMA_VERSION` to 17 in `migration.rs`.
2. Add `if current_version < 17` branch in `run_main_migrations` with
   `pragma_table_info` pre-check for `query_log.phase` column, `ALTER TABLE` if absent,
   and `CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)`.
3. Update schema version counter bind to 17.
4. Add `phase: Option<String>` to `AnalyticsWrite::QueryLog` variant.
5. Update SQL INSERT in analytics.rs to include `phase`.
6. Add `phase: Option<String>` to `QueryLogRecord` struct.
7. Update `QueryLogRecord::new()` signature to accept `phase: Option<String>`.
8. Update `scan_query_log_by_sessions`, `scan_query_log_by_session`, and
   `row_to_query_log` for the new column.
9. Update all callers of `QueryLogRecord::new()` (tools.rs MCP call site, test helpers).

### Part 5 — MCP context_search query_log write site

Snapshot phase before the `spawn_blocking` (ADR-001 crt-025 pattern):
```rust
let phase_for_log = ctx.audit_ctx.session_id.as_deref()
    .and_then(|sid| self.session_registry.get_state(sid))
    .and_then(|s| s.current_phase.clone());
```
Pass to `QueryLogRecord::new(...)`. This snapshot can be shared with the UsageContext
phase snapshot taken at the same call site.

## Acceptance Criteria

- AC-01: `context_search` passes `current_phase: Some(phase)` in `UsageContext` when the
  active session has a phase set; passes `None` when no session or no phase.

- AC-02: `context_lookup` passes `current_phase: Some(phase)` in `UsageContext` when the
  active session has a phase set; passes `None` when no session or no phase.

- AC-03: `context_get` passes `current_phase: Some(phase)` in `UsageContext` when the
  active session has a phase set; passes `None` when no session or no phase.

- AC-04: `context_briefing` passes `current_phase: Some(phase)` in `UsageContext` when
  the active session has a phase set; passes `None` when no session or no phase.

- AC-05: `context_get` uses `access_weight: 2` (changed from 1). A unit test confirms
  an entry fetched via `context_get` produces `access_count = 2` on first access.

- AC-06: `context_briefing` uses `access_weight: 0`. A unit test confirms briefing does
  not increment `access_count` for any returned entry.

- AC-07: A briefing call on entry X followed by a `context_get` on entry X increments
  `access_count` by 2 (weight=2 for get). The briefing call does not consume the dedup
  slot. This directly validates D-01.

- AC-08: `SessionState` has `confirmed_entries: HashSet<u64>` initialised empty on
  `register_session`.

- AC-09: After `context_get` for entry ID X, `confirmed_entries` contains X for that session.

- AC-10: After `context_lookup` with a single target ID X, `confirmed_entries` contains X.
  After a multi-target lookup, `confirmed_entries` is not updated.

- AC-11: `context_lookup` access_weight remains 2 (unchanged).

- AC-12: Phase snapshot is taken synchronously before any `await` in each of the four
  handlers (ADR-001 crt-025 race-condition contract). The phase helper is a free function,
  not four independent `get_state` sequences.

- AC-13: `CURRENT_SCHEMA_VERSION = 17`. A unit test asserts the constant equals 17.

- AC-14: The `query_log` table has a `phase TEXT` column and `idx_query_log_phase` index
  after v16→v17 migration.

- AC-15: The v16→v17 migration is idempotent. Running it on a database that already has
  the `phase` column does not fail.

- AC-16: A `context_search` call in a session with active phase "delivery" writes
  `phase = "delivery"` to the `query_log` row. A call with no active session writes
  `phase = NULL`.

- AC-17: `QueryLogRecord` has `phase: Option<String>`. `scan_query_log_by_session` returns
  records with the correct phase value. `row_to_query_log` correctly deserializes it.

- AC-18: Pre-existing `query_log` rows read back with `phase = None` after migration.

- AC-19: A migration integration test (following `migration_v15_to_v16.rs` pattern) covers
  fresh DB at v17, v16→v17 migration from a v16 fixture, and idempotency.

- AC-20: `make_state_with_rework` and all related test helpers are updated for
  `confirmed_entries`. All existing tests pass.

## Constraints

- **Synchronous snapshot before any `await` (ADR-001 crt-025)**: The phase snapshot
  must be taken before the first `await` in each handler. `get_state` returns a `Clone`,
  satisfying this without holding the lock across awaits.

- **`pragma_table_info` pre-check required**: SQLite does not support `ALTER TABLE ADD
  COLUMN IF NOT EXISTS`. All ADD COLUMN migrations in this codebase use the pre-check.
  Deviation is not allowed.

- **D-01 guard must be in `record_briefing_usage`, not `record_mcp_usage`**: Briefing
  continues to pass `AccessSource::Briefing`, which routes to `record_briefing_usage`.
  The early-return guard for weight-0 belongs there, before `filter_access` is called.

- **`QueryLogRecord::new()` caller count**: `eval/scenarios/tests.rs` has 15+ call sites
  via `insert_query_log_row` helper. All must be updated. The UDS `insert_query_log` call
  site is out of scope but its `QueryLogRecord::new()` call must still compile — pass
  `None` for phase.

- **Column order stability in analytics.rs**: The SQL INSERT for `query_log` uses
  positional `?1`..`?N` binding. The `phase` column must be added as the last positional
  parameter to avoid reindexing existing binds.

- **`session_registry` reference in query_log write scope**: The phase snapshot for the
  query_log write can reuse the same `get_state` call used for the UsageContext phase,
  since both snapshots are taken at the same point before any `await`.

- **File size limit (500 lines)**: `infra/session.rs` is large. Adding one field and one
  method is within limit. `mcp/tools.rs` is the most likely constraint — verify line
  count before changes and split if approaching 500 lines.

## Design Decisions (All Resolved)

**D-01 — Weight-0 dedup bypass (confirmed required)**: A briefing call at weight-0 must
not call `filter_access`. The `UsageDedup.access_counted` HashSet is shared across all
access sources. Without the guard, briefing burns the dedup slot and a subsequent
`context_get` produces zero access count increment — exactly the highest-signal event
in the pipeline. Guard: `if ctx.access_weight == 0 { return; }` at the top of
`record_briefing_usage`, before `filter_access`.

**D-02 — `context_lookup` confirmed_entries: request-side cardinality**: `target_ids.len() == 1`
(single ID in the request), not "single entry returned". Request cardinality reflects
agent intent. Multiple IDs is batch retrieval; "single entry returned" is a filtering
artifact.

**D-03 — `confirmed_entries` added now with no consumer**: Session-ephemeral. Past
sessions cannot be retroactively reconstructed. Deferring means Thompson Sampling cold
starts with zero data. Add now; Thompson Sampling inherits populated state.

**D-04 — Phase helper as free function**: `current_phase_for_session` takes
`(&SessionRegistry, Option<&str>)`. Four call sites, critical signal path. A free
function is testable without handler construction and makes intent explicit at the call
site.

**D-05 — weight-0 arithmetic and co-access (confirmed facts):**
- (a) The access increment uses a flat_map repeat approach (`iter::repeat(id).take(weight)`).
  For weight=0, the `<= 1` branch runs producing one copy of each ID in `multiplied_all_ids`.
  EC-04 ("weight 0 silently drops the access increment") is a contract claim not enforced
  by the flat_map path — the D-01 early-return guard is the enforcement mechanism.
- (b) `record_briefing_usage` does not contain `generate_pairs` or `filter_co_access_pairs`.
  Briefing does not generate co-access pairs today, and the D-01 guard prevents
  `record_usage_with_confidence` from being called at all for weight-0 events, so this
  remains true after the change.

## Open Questions

None. All design decisions are resolved. Both query_log write sites are identified.
Schema version is confirmed at 16. Dedup collision is confirmed and the guard location is
determined.

## Tracking

GH Issues: #394 (in-memory phase capture + weight corrections), #397 (query_log.phase
migration). Will be updated with GH Issue link after Session 1.
