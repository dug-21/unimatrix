# crt-033 Test Plan — OVERVIEW

## Feature Summary

crt-033 introduces `CYCLE_REVIEW_INDEX`, a SQLite memoization table for
`context_cycle_review`, and adds `pending_cycle_reviews` to `context_status`.
The feature spans five components across two crates (`unimatrix-store`,
`unimatrix-server`) plus a new integration test file.

---

## Overall Test Strategy

| Level | Scope | Primary Gate |
|-------|-------|-------------|
| Unit (cargo test) | Per-module logic: struct construction, serde round-trip, param parsing, pool-selection grep, schema constant assertions | All unit tests pass |
| Store integration | SqlxStore round-trips on a real SQLite DB, migration v17→v18 path, K-window query correctness, 4MB ceiling | All store integration tests pass |
| Server integration (infra-001) | MCP-visible behaviour: cycle_review memoization hit/miss, force parameter, pending_cycle_reviews in status JSON | smoke gate + tools + lifecycle suites pass |

Testing is layered: unit tests cover isolated logic; store integration tests verify
SQL correctness and migration; infra-001 exercises the full MCP surface. No level
substitutes for another.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Primary Test Location | Scenarios |
|---------|----------|-------------|----------------------|-----------|
| R-01 | Critical | migration | `tests/migration_v17_to_v18.rs`, sqlite_parity, server.rs grep | 6 |
| R-02 | High | cycle_review_index, tools_handler | concurrent async integration test | 3 |
| R-03 | High | tools_handler, cycle_review_index | store integration (raw SQL assert) | 3 |
| R-04 | High | tools_handler | tools_handler unit + store integration | 4 |
| R-05 | High | tools_handler | tools_handler unit (mock store) | 3 |
| R-06 | Medium | cycle_review_index, tools_handler | cycle_review_index unit (serde), tools_handler unit | 4 |
| R-07 | Medium | status_service, cycle_review_index | store integration (K-window SQL) | 6 |
| R-08 | Medium | tools_handler | tools_handler unit | 3 |
| R-09 | Medium | tools_handler | static grep check | 2 |
| R-10 | Low | cycle_review_index | concurrent store integration | 2 |
| R-11 | Low | cycle_review_index | cycle_review_index unit | 3 |
| R-12 | Low | cycle_review_index | static grep check | 2 |
| R-13 | Low | cycle_review_index | static grep check | 2 |

---

## Cross-Component Test Dependencies

1. `migration` must be verified before any `cycle_review_index` store tests (table
   must exist before rows can be written).
2. `cycle_review_index` store functions are a prerequisite for `tools_handler` tests
   that seed rows directly into the table.
3. `status_service` K-window query depends on `cycle_review_index` and `cycle_events`
   both being present — migration must pass first.
4. `status_response` formatter tests are independent of the other components.
5. `tools_handler` force=true + purged-signals test (R-04) seeds `cycle_review_index`
   rows directly, so `store_cycle_review` must be implemented first.

---

## Integration Harness Plan (infra-001)

### Suite Selection

crt-033 touches:
- `context_cycle_review` tool (server tool logic) → `tools`, `protocol`
- `context_status` tool (server tool logic + store/retrieval) → `tools`, `lifecycle`
- Schema change (v17→v18) → `lifecycle` (restart persistence), `volume`
- Any change at all → `smoke` (minimum gate)

**Required suites:**
- `smoke` — mandatory minimum gate
- `tools` — `context_cycle_review` and `context_status` parameter coverage
- `lifecycle` — restart persistence (schema migration durability), multi-step flows
- `volume` — scale behaviour with large payloads (NFR-03 4MB ceiling)

**Not required:**
- `confidence`, `contradiction`, `security`, `edge_cases` — no changes to scoring,
  contradiction detection, capability enforcement, or Unicode handling

### Existing Suite Coverage Gaps

The existing `tools` suite exercises `context_cycle_review` with a basic call but does
not cover:
1. The `force` parameter (new field on `RetrospectiveParams`)
2. Memoization hit/miss lifecycle (requires two sequential calls to same cycle)
3. `pending_cycle_reviews` field in `context_status` response

The existing `lifecycle` suite covers restart persistence but does not cover:
- `cycle_review_index` row survival across server restart

### New Integration Tests to Add

Add to `suites/test_tools.py`:

```python
# Force parameter is accepted; absent force is equivalent to false
def test_cycle_review_force_param_accepted(server):
    # POST context_cycle_review with force=true; assert no JSON-RPC error
    # fixture: server (fresh DB, no cycle data — expected error is NO_OBSERVATION_DATA,
    # not a param-validation error)
    ...

# context_status pending_cycle_reviews field present and is an array
def test_status_pending_cycle_reviews_field_present(server):
    # Call context_status; assert response JSON contains 'pending_cycle_reviews' key
    # whose value is an array (may be empty)
    ...
```

Add to `suites/test_lifecycle.py`:

```python
# cycle_review_index row persists across server restart
def test_cycle_review_persists_across_restart(server):
    # Step 1: call context_cycle_review for a cycle with live signals
    # Step 2: restart server (fixture teardown + new fixture)
    # Step 3: call context_cycle_review again for same cycle
    # Assert: second call returns same computed_at as first (memoization hit,
    # no recomputation)
    ...
```

**When NOT to add new integration tests** (already covered by existing suites):
- Basic `context_cycle_review` happy path (existing in tools suite)
- `context_status` basic response structure (existing in tools suite)
- Server restart data persistence (existing in lifecycle suite — confirms SQLite
  durability applies equally to `cycle_review_index`)

### Integration Test Conventions

- Use `server` fixture (function scope) for all new tests — fresh DB, no leakage.
- For the restart-persistence lifecycle test, use function-scope fixture with explicit
  restart rather than `shared_server`.
- Test naming: `test_cycle_review_{behavior}`, `test_status_pending_cycle_reviews_{behavior}`

---

## AC Coverage by Component

| AC-ID | Component Test Plan |
|-------|---------------------|
| AC-01 | migration.md |
| AC-02 | migration.md |
| AC-02b | migration.md |
| AC-03 | cycle_review_index.md, tools_handler.md |
| AC-04 | tools_handler.md |
| AC-04b | tools_handler.md |
| AC-05 | tools_handler.md |
| AC-06 | tools_handler.md |
| AC-07 | tools_handler.md |
| AC-08 | tools_handler.md, cycle_review_index.md |
| AC-09 | status_service.md, cycle_review_index.md |
| AC-10 | status_service.md, cycle_review_index.md |
| AC-11 | cycle_review_index.md (covered by AC-03) |
| AC-12 | tools_handler.md |
| AC-13 | migration.md |
| AC-14 | tools_handler.md |
| AC-15 | tools_handler.md |
| AC-16 | cycle_review_index.md |
| AC-17 | cycle_review_index.md (grep gate) |

---

## Open Questions from Test Design

1. **Mock vs real store for AC-04/AC-14 (memoization hit path)**: The hit path must
   confirm no observation-load queries are executed. SqlxStore does not expose a query
   counter. The recommended approach is to inject a pre-seeded `cycle_review_index` row
   directly via `store_cycle_review`, then call the handler with `force=false`, and
   assert that the `cycle_events` and `observations` tables remain empty (they were never
   populated, so any read from them would return nothing — but the test must not rely on
   returning nothing from empty tables as proof that the tables were not queried). The
   correct assertion is that `computed_at` in the stored row is unchanged after the second
   call and the returned `feature_cycle` matches. A future refactor to expose a query
   counter would strengthen this.

2. **Concurrent first-call test (R-02)**: Requires `tokio::spawn` with two futures.
   The existing `PoolConfig::test_default()` uses `max_connections=1` by default — this
   is intentionally conservative. The test should confirm both calls complete (no
   deadlock) and exactly one row exists in `cycle_review_index` afterward. If the pool
   config causes serialisation rather than true concurrency, that is acceptable — the
   goal is no deadlock, not true parallel execution.
