# col-028: Unified Phase Signal Capture (Read-Side + query_log)

## System Overview

Phase is the highest-signal discrete label for knowledge surfacing quality. Three
downstream learning loops — the phase-conditioned frequency table (ass-032 Loop 2),
Thompson Sampling per-(phase, entry) arms, and gap detection — all depend on
phase-tagged read-side events. Today phase is only captured on `context_store` writes;
every read-side event arrives phase-context-free.

This feature resolves two related gaps that share the same root cause:

- **Gap 1 (in-memory, #394)**: `UsageContext.current_phase` is `None` for all four
  read-side tools (`context_search`, `context_lookup`, `context_get`, `context_briefing`)
  despite `SessionState.current_phase` being available at call time. Additionally,
  `context_get` access weight (1) underrepresents deliberate full-content retrieval, and
  `context_briefing` access weight (1) overcounts by crediting entries the agent may
  never read.

- **Gap 2 (persistence, #397)**: The `query_log` table has no `phase` column. Phase is
  present in `SessionState` at write time but never persisted. All downstream analytics
  consumers receive phase-free data regardless of what is captured in memory.

Both changes are purely additive and backward-compatible. The feature also adds
`confirmed_entries: HashSet<u64>` to `SessionState` with no consumer in this feature —
populated now so Thompson Sampling inherits data from day one.

## Component Breakdown

### Component 1: SessionState (infra/session.rs)

**Responsibility**: Per-session in-memory state container.

**Changes**:
- Add `confirmed_entries: HashSet<u64>` field, initialised to `HashSet::new()` in
  `register_session`. Follows identical pattern to `signaled_entries`.
- Add `SessionRegistry::record_confirmed_entry(&self, session_id: &str, entry_id: u64)`
  method using the synchronous lock-and-mutate pattern of `record_category_store`.
- Update `make_state_with_rework` and all related test helpers (pattern #3180).

**Invariants**: In-memory only. Never persisted. Reset on session registration.
Consistent with all other `SessionState` fields.

### Component 2: Phase Helper Free Function (mcp/tools.rs)

**Responsibility**: Shared, testable phase extraction from session registry.

**Changes**:
- Add free function `current_phase_for_session(registry: &SessionRegistry, session_id: Option<&str>) -> Option<String>`.
- This encapsulates the `get_state(sid).and_then(|s| s.current_phase.clone())` pattern
  so it is testable without handler construction and makes intent explicit at every call
  site (ADR-001, D-04).

### Component 3: Four Read-Side Call Sites (mcp/tools.rs)

**Responsibility**: Phase capture, weight correction, and confirmed_entries population
for each read tool.

**Changes per call site**:

| Tool | Phase | Weight | confirmed_entries |
|------|-------|--------|-------------------|
| `context_search` | Capture via helper | 1 (unchanged) | — |
| `context_lookup` | Capture via helper | 2 (unchanged) | Insert if `target_ids.len() == 1` |
| `context_get` | Capture via helper | 1 → 2 | Always insert |
| `context_briefing` | Capture via helper | 1 → 0 | — |

Phase snapshot must be the first statement in each handler body, before any `await`
(ADR-002, pattern #3027). The `UsageContext.current_phase` doc comment must be updated
to state that read-side tools now populate this field (ADR-006).

### Component 4: D-01 Guard (services/usage.rs)

**Responsibility**: Prevent briefing calls from consuming a dedup slot when weight is 0.

**Changes**:
- In `record_briefing_usage`, add an early-return guard at the very top of the function
  body, before `filter_access` is called:
  ```rust
  if ctx.access_weight == 0 {
      return; // offer-only event; do not register dedup slot or increment access_count
  }
  ```

**Rationale**: `UsageDedup.access_counted` is a single `HashSet<(String, u64)>` shared
across ALL `AccessSource` variants. Without this guard, a briefing call at weight=0 would
still call `filter_access`, consuming the dedup slot and preventing a subsequent
`context_get` on the same entry from incrementing `access_count` — exactly the
highest-signal event in the pipeline (ADR-003, D-01, D-05).

### Component 5: Schema Migration v16→v17 (unimatrix-store)

**Responsibility**: Persist phase on `query_log` rows.

**Changes** (treated as one atomic change unit — SR-01):
1. `migration.rs`: Bump `CURRENT_SCHEMA_VERSION` to 17.
2. `migration.rs`: Add `if current_version < 17` branch with `pragma_table_info`
   pre-check for `query_log.phase`, `ALTER TABLE query_log ADD COLUMN phase TEXT`, and
   `CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)`.
3. `analytics.rs`: Add `phase: Option<String>` field to `AnalyticsWrite::QueryLog`
   variant. Add `?9` positional bind at the end of the INSERT (column order stability,
   SR-01 constraint).
4. `query_log.rs`: Add `phase: Option<String>` field to `QueryLogRecord`.
5. `query_log.rs`: Update `QueryLogRecord::new()` signature to accept
   `phase: Option<String>` as last parameter.
6. `query_log.rs` (`insert_query_log`): Pass `phase` through to `AnalyticsWrite::QueryLog`.
7. `query_log.rs` (`scan_query_log_by_sessions`, `scan_query_log_by_session`): Add
   `phase` to the SELECT column list in both queries.
8. `query_log.rs` (`row_to_query_log`): Add `phase` deserialization at index 9.

All eight items must land in the same commit. Divergence between the INSERT positional
params, both SELECT column lists, and `row_to_query_log` is a silent runtime error
(SR-01 high risk).

### Component 6: MCP context_search query_log Write Site (mcp/tools.rs)

**Responsibility**: Populate `phase` in the query_log row for MCP search calls.

**Changes**:
- The phase snapshot for the query_log write is taken from the same `get_state` call
  already used for the `UsageContext` phase snapshot. One `get_state` call, two consumers
  (SR-06 mitigation). Both snapshots share a single `let session_state = ...` binding
  before any `await`.
- Pass the phase value as the final argument to `QueryLogRecord::new(...)`.

**Note**: The UDS `insert_query_log` call site (`uds/listener.rs:1324`) is out of scope
for phase semantics but its `QueryLogRecord::new()` call must compile after the signature
change — pass `None` for phase (SR-03).

## Component Interactions

```
MCP Handler (tools.rs)
  │
  ├─ current_phase_for_session(registry, session_id)  ← free function [Phase Helper]
  │    └─ SessionRegistry.get_state(sid)              ← single lock acquisition
  │         └─ SessionState.current_phase.clone()
  │
  ├─ UsageContext { current_phase: Some(phase), access_weight: N, ... }
  │    └─ UsageService.record_access()               ← fire-and-forget
  │         └─ record_mcp_usage() | record_briefing_usage()
  │              └─ D-01 guard if weight==0 → return  [Component 4]
  │
  ├─ SessionRegistry.record_confirmed_entry()         ← get/context_lookup(single)
  │
  └─ QueryLogRecord::new(..., phase: Some(phase))     ← context_search only
       └─ store.insert_query_log()                    ← enqueue_analytics
            └─ AnalyticsWrite::QueryLog { phase }
                 └─ drain task → INSERT ?1..?9
```

## Technology Decisions

See ADR files for full rationale. Summary:

| Decision | Choice | ADR |
|----------|--------|-----|
| Phase helper shape | Free function, not method | ADR-001 |
| Phase snapshot timing | First statement, before any await | ADR-002 |
| Weight-0 guard location | `record_briefing_usage` top, before `filter_access` | ADR-003 |
| confirmed_entries trigger | Request-side cardinality (`target_ids.len()==1`) | ADR-004 |
| confirmed_entries consumer | None in this feature | ADR-005 |
| UsageContext doc comment | Updated as part of deliverable | ADR-006 |
| Phase column position | Appended as last positional param (?9) | ADR-007 |
| Schema migration pattern | pragma_table_info pre-check (idempotency) | ADR-007 |

## Integration Points

### Downstream Consumers (future features, not in scope)

- `ass-032` Phase-conditioned frequency table: queries `query_log.phase`
- Thompson Sampling: reads `SessionState.confirmed_entries` and `UsageContext.current_phase`
- Gap detection: queries `query_log.phase`

### Upstream Dependencies

- `crt-025` ADR-001: Phase snapshot pattern (pattern #3027) — already established,
  extended here to four additional call sites
- `col-025`: `SessionState.current_goal` — precedent for optional session field with
  None init (pattern for `confirmed_entries`)
- `nxs-011`: `enqueue_analytics` / analytics drain — unchanged; `AnalyticsWrite::QueryLog`
  variant extended with new field
- `UsageDedup` (no crate change) — internal to unimatrix-server

## Integration Surface

| Integration Point | Type/Signature | Location |
|-------------------|----------------|----------|
| `current_phase_for_session` | `fn(&SessionRegistry, Option<&str>) -> Option<String>` | `mcp/tools.rs` (free function, module-level) |
| `SessionState.confirmed_entries` | `HashSet<u64>` | `infra/session.rs` |
| `SessionRegistry::record_confirmed_entry` | `fn(&self, session_id: &str, entry_id: u64)` | `infra/session.rs` |
| `UsageContext.current_phase` | `Option<String>` | `services/usage.rs` (field, now populated for read tools) |
| `QueryLogRecord::new` | `fn(String, String, &[u64], &[f64], &str, &str, Option<String>) -> Self` | `unimatrix-store/src/query_log.rs` |
| `QueryLogRecord.phase` | `Option<String>` | `unimatrix-store/src/query_log.rs` |
| `AnalyticsWrite::QueryLog.phase` | `Option<String>` | `unimatrix-store/src/analytics.rs` |
| `CURRENT_SCHEMA_VERSION` | `u64 = 17` | `unimatrix-store/src/migration.rs` |

## Risk Mitigations

### SR-01 (High): Positional Column Index Fragility

The `analytics.rs` INSERT, both `scan_query_log_*` SELECT statements, and
`row_to_query_log` are a single atomic change unit. They must be modified together in
one commit and reviewed as a unit.

Enforcement mechanism: AC-17 (end-to-end round-trip test reading back `phase` value)
is the runtime guard against positional drift. The implementation spec must call this
out explicitly as the SR-01 guard test.

### SR-02 (Med): Schema Version Cascade

The following files assert `schema_version == 16` and must be updated to 17:

| File | Change Required |
|------|----------------|
| `crates/unimatrix-store/tests/migration_v15_to_v16.rs` | All `assert_eq!(... 16)` → 17; `test_current_schema_version_is_16` → `_is_17`; function name updates |
| `crates/unimatrix-server/src/server.rs` | Lines 2059 and 2084: `assert_eq!(version, 16)` → 17 |

Additionally, `migration_v14_to_v15.rs` uses `>= 15` guards (already pattern #2933
compliant) — no change needed there.

New file to create: `crates/unimatrix-store/tests/migration_v16_to_v17.rs` following
the `migration_v15_to_v16.rs` pattern, covering: fresh DB at v17, v16→v17 migration
from a v16 fixture, and idempotency (AC-19).

### SR-03 (Med): UDS Compile Fix

`uds/listener.rs:1324` — `QueryLogRecord::new(...)` call must pass `None` as the new
final `phase` argument. No semantic change; this is a mandatory compile fix. The
implementer must update this site even though UDS phase semantics are out of scope.

### SR-04 (Low): confirmed_entries Semantic Contract

The contract is: explicit-fetch-only (context_get always; context_lookup only when
single ID requested). This is defined in ADR-004 and ADR-005 and must not be silently
reinterpreted by the future Thompson Sampling consumer.

### SR-05 (Low): briefing access_count regression

No existing analytics query groups by access source or normalizes on historical briefing
weight. The `query_log` table does not track `access_weight`. The `co_access` and
`feature_entries` tables are not affected by briefing at all (D-05b: `record_briefing_usage`
has no co-access pair generation). Regression risk is confirmed low.

### SR-06 (Med): Shared Phase Snapshot at context_search

The phase snapshot at `context_search` must be a single `let session_state = ...`
binding reused by both `UsageContext.current_phase` and `QueryLogRecord::new`. Two
separate `get_state` calls would acquire the lock twice and could theoretically diverge
if a phase-end event arrives between the two calls.

### SR-07 (Low): D-01 Guard Bypass on Future Refactor

The D-01 guard is load-bearing in `record_briefing_usage`. If `record_mcp_usage` is
ever called with `AccessSource::Briefing`, the guard would be bypassed. The guard
location is correct for the current routing (all briefing flows through
`record_briefing_usage`). The risk is documented in ADR-003 as a structural note.
