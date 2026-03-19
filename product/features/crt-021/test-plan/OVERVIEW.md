# Test Plan Overview: crt-021 (W1-1 Typed Relationship Graph)

## Overall Test Strategy

crt-021 is a pure infrastructure upgrade spanning three crates. No MCP tool signatures
change; no external behavior changes. The test strategy has three layers:

1. **Unit tests** — Pure functions in `unimatrix-engine` (graph construction, penalty,
   traversal, type round-trips, weight validation). All 25+ existing `graph.rs` tests must
   pass unchanged on `TypedRelationGraph`. New unit tests cover new types and behaviors.

2. **Integration tests** — `unimatrix-store` migration tests (v12→v13 on synthetic databases,
   R-06 empty table, CoAccess threshold + weight normalization, idempotency). Analytics drain
   tests verify `AnalyticsWrite::GraphEdge` persists correctly. Schema DDL verification.

3. **Compile-time enforcement** — `cargo build --workspace` with `SQLX_OFFLINE=true` must
   succeed after `sqlx-data.json` regeneration. Zero `SupersessionState`/`SupersessionStateHandle`
   symbol occurrences in compiled source (grep gate).

End-to-end server integration tests exercise the existing search path through infra-001;
no new MCP surface is added by this feature.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test Location | Test Type |
|---------|----------|-------------|---------------|-----------|
| R-01 | Critical | engine-types | `graph.rs` unit tests | Unit |
| R-02 | Critical | engine-types | `graph.rs` unit tests + code review | Unit |
| R-03 | Critical | engine-types | `graph.rs` unit tests | Unit |
| R-04 | High | background-tick, server-state | `background.rs` integration | Integration |
| R-05 | High | server-state | `typed_graph.rs` unit tests | Unit |
| R-06 | Critical | store-migration | `migration.rs` integration tests | Integration |
| R-07 | High | store-analytics | `analytics.rs` unit + integration | Unit + Integration |
| R-08 | Med | store-migration | `migration.rs` idempotency test | Integration |
| R-09 | High | store-schema | CI compile gate | CI |
| R-10 | Med | engine-types, store-analytics | `graph.rs` unit tests | Unit |
| R-11 | High | background-tick | Tick timing assertion | Integration |
| R-12 | Med | engine-types | `graph.rs` unit tests | Unit |
| R-13 | Low | store-migration | Code inspection | Inspection |
| R-14 | Med | server-state, all server files | `cargo build` compile gate | CI |
| R-15 | Med | store-migration | Migration weight assertion | Integration |

---

## Cross-Component Test Dependencies

| Dependency | Direction | Notes |
|-----------|-----------|-------|
| `build_typed_relation_graph` takes `&[GraphEdgeRow]` | engine-types ← store | `GraphEdgeRow` must be defined first; engine tests construct `GraphEdgeRow` directly |
| `TypedGraphState::rebuild` calls `store.query_graph_edges()` | server-state ← store | Server-state integration tests need a live `SqlxStore` with seeded `GRAPH_EDGES` |
| Background tick: compaction before rebuild | background-tick depends on store | Tick integration tests open a full `SqlxStore`, seed orphaned edges, trigger tick, read `TypedGraphStateHandle` |
| Migration: v13 schema required for all store tests | store-migration → store-schema | Schema DDL test can run on fresh DB; migration test operates on synthetic v12 DB |

---

## Integration Harness Plan

### Which Existing Suites Cover This Feature

| Suite | Coverage | Applicability |
|-------|----------|---------------|
| `smoke` | MCP handshake, search, store round-trip | MANDATORY gate — must pass unchanged |
| `protocol` | JSON-RPC compliance, tool discovery | No change expected; must pass |
| `tools` | All 9 tools including `context_search` | Search behavior unchanged; must pass |
| `lifecycle` | Multi-step flows including correction chains | Graph rebuild happens in background; search results semantically unchanged |
| `confidence` | Confidence formula, re-ranking | Search re-ranking uses `graph_penalty`; typed graph must produce identical penalties |

No new MCP tool is added. No tool signature changes. The typed graph is internal infrastructure
only. The existing `tools` and `lifecycle` suites provide full regression coverage of the
search path through the typed graph.

### Gaps: New Behavior Not Covered by Existing Suites

1. **v12→v13 migration on existing database** — the infra-001 suites always start from a
   fresh database. An existing-schema migration test is not exercisable via infra-001.
   This gap is covered by the `store-migration` unit/integration tests in `migration.rs`.

2. **GRAPH_EDGES orphaned-edge compaction** — the background tick compaction step deletes
   orphaned graph edges before rebuild. This is observable only via direct store inspection,
   not through any MCP tool. Covered by background-tick unit tests.

3. **`AnalyticsWrite::GraphEdge` drain** — analytics drain behavior is not exposed via MCP.
   Covered by `store-analytics` integration tests.

### New Integration Tests to Add

These tests must be added to `crates/unimatrix-store/src/migration.rs` `#[cfg(test)]` module,
following the existing `run_main_migrations` pattern:

#### Test 1: v12→v13 migration with Supersedes bootstrap (AC-05, AC-06, AC-18)
```
test_v12_to_v13_supersedes_bootstrap
```
- Open synthetic v12 database (set schema_version=12 in counters)
- Insert entries with `supersedes IS NOT NULL`
- Run `migrate_if_needed`
- Assert: `schema_version = 13`, `graph_edges` exists, Supersedes row count matches input,
  all rows have `bootstrap_only=0`, `source='bootstrap'`,
  `source_id = entry.supersedes`, `target_id = entry.id`

#### Test 2: R-06 — empty co_access table migration succeeds (MANDATORY, R-06, AC-07)
```
test_v12_to_v13_empty_co_access_succeeds
```
- Open synthetic v12 database
- Leave `co_access` table empty
- Run `migrate_if_needed`
- Assert: migration completes without error, `schema_version = 13`,
  zero rows with `relation_type='CoAccess'` in `graph_edges`

#### Test 3: CoAccess threshold + weight normalization (AC-07, R-15)
```
test_v12_to_v13_co_access_threshold_and_weights
```
- Open synthetic v12 database
- Insert `co_access` rows: `(1,2,count=2)`, `(1,3,count=3)`, `(1,4,count=5)`
- Run `migrate_if_needed`
- Assert: `(1,2)` pair produces NO edge, `(1,3)` and `(1,4)` produce edges
- Assert: weight for `(1,4)` equals 1.0 (max), weight for `(1,3)` equals 0.6
- Assert: all CoAccess edges have `bootstrap_only=0`

#### Test 4: Idempotency (AC-05 robustness, R-08)
```
test_v12_to_v13_idempotent_double_run
```
- Run migration twice on the same database
- Assert: row counts identical after both runs, no constraint violations

#### Test 5: No Contradicts at bootstrap (AC-08)
```
test_v12_to_v13_no_contradicts_bootstrapped
```
- After any v12→v13 migration, assert zero `relation_type='Contradicts'` rows in `graph_edges`

### No New infra-001 Suite Tests Needed

The search path is semantically unchanged. The existing `smoke`, `tools`, and `lifecycle`
suites provide sufficient regression coverage. The `GRAPH_EDGES` table is internal and
unreachable from any MCP tool. New integration tests live in `migration.rs` and unit test
modules, not in infra-001.

---

## Acceptance Criteria Coverage Map

| AC-ID | Test Type | Component File |
|-------|-----------|---------------|
| AC-01 | Unit | engine-types.md |
| AC-02 | Unit | engine-types.md |
| AC-03 | Unit | engine-types.md |
| AC-04 | Integration | store-schema.md |
| AC-05 | Integration | store-migration.md |
| AC-06 | Integration | store-migration.md |
| AC-07 | Integration | store-migration.md |
| AC-08 | Integration | store-migration.md |
| AC-09 | Unit + Integration | store-analytics.md |
| AC-10 | Unit | engine-types.md |
| AC-11 | Unit | engine-types.md |
| AC-12 | Unit | engine-types.md |
| AC-13 | Integration | server-state.md |
| AC-14 | Integration | background-tick.md |
| AC-15 | Unit | server-state.md |
| AC-16 | Manual (Unimatrix) | OVERVIEW |
| AC-17 | Unit + Integration | store-analytics.md |
| AC-18 | Integration | store-migration.md |
| AC-19 | CI shell | OVERVIEW |
| AC-20 | Grep + Unit | engine-types.md |
| AC-21 | Integration | store-migration.md |
