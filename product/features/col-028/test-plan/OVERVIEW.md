# Test Plan Overview: col-028
# Unified Phase Signal Capture — Read-Side + query_log

## Test Strategy Summary

col-028 is a pure additive feature: no existing behavior is removed, only gaps filled.
The test strategy has three tiers:

1. **Unit tests** — validate each component in isolation with no database or MCP wire
   involved. Cover AC-01 through AC-12 (phase capture, weight corrections,
   confirmed_entries, D-01 guard, current_phase_for_session correctness).

2. **Store integration tests** — exercise the unimatrix-store crate directly via
   SqlxStore. Cover AC-13 through AC-19 (schema migration, phase round-trip).
   These are the migration_v16_to_v17.rs tests and AC-17 integration test. They do NOT
   go through the MCP wire.

3. **infra-001 integration tests (MCP wire)** — exercise the compiled binary end-to-end.
   Mandatory smoke gate plus targeted suites. AC-07 (D-01 dedup) and AC-16 (phase in
   query_log via real analytics drain) are the two tests that MUST be covered at this
   tier; their correctness cannot be confirmed by unit tests alone.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Covering Tests | Tier |
|---------|----------|---------------|------|
| R-01 D-01 dedup collision | Critical | AC-07 positive + AC-07 negative arm; AC-06 briefing weight=0 | Unit + infra-001 integration |
| R-02 Positional column index drift | Critical | AC-17 round-trip (real drain); AC-21 code review | Store integration + code review |
| R-03 Phase snapshot race | Critical | AC-12 code review gate; `current_phase_for_session` unit tests | Code review + unit |
| R-04 Dual get_state at context_search | High | AC-16 integration test; C-04 code review | infra-001 + code review |
| R-05 Schema version cascade | High | AC-22 grep check; AC-13 unit; T-V17-06 | grep gate + unit + store integration |
| R-06 UDS compile break | High | AC-23 cargo build | Compile gate |
| R-07 context_get weight not corrected | High | AC-05 unit test | Unit |
| R-08 context_briefing weight not corrected | High | AC-06 unit test | Unit |
| R-09 confirmed_entries missing from test helpers | High | AC-20 cargo test --workspace | Compile + full suite |
| R-10 Phase not written to query_log | Medium | AC-16 integration test (real drain) | infra-001 |
| R-11 Migration idempotency | Medium | T-V17-04 | Store integration |
| R-12 Pre-existing row deserialization | Medium | T-V17-05 | Store integration |
| R-13 confirmed_entries cardinality | Medium | AC-10 positive + AC-10 negative (both arms required) | Unit |
| R-14 context_lookup weight drifted | Low | AC-11 + existing tests pass | Existing coverage |
| R-15 Doc comment stale | Low | AC-24 grep/code review | Code review |
| R-16 D-01 guard future bypass | Low | AC-07 as canary | (documented accepted risk) |

---

## Critical Priority Tests — Detail

### R-01 (AC-07): D-01 Dedup Guard — Positive AND Negative Arms

AC-07 is the primary guard against the highest-priority risk. Two sub-tests are required:

**Positive arm** (guard in place, expected behavior):
- Register session with agent_id.
- Call `record_briefing_usage` with entry X, `access_weight: 0`.
- Assert `UsageDedup.access_counted` does NOT contain `(agent_id, X)`.
- Call `record_mcp_usage` (context_get path) with entry X, `access_weight: 2`.
- Assert `access_count` for X increments by 2 (not 0).

**Negative arm** (guard absent, proves guard is load-bearing):
This test must document the counterfactual. The test simulates the absent-guard scenario
by calling `filter_access` directly (or by constructing UsageDedup without the guard),
confirming that WITHOUT the guard, briefing DOES consume the dedup slot and the
subsequent context_get produces `access_count += 0`.

The negative arm is the evidence that the guard is not redundant. It must be explicitly
present in `test-plan/usage-d01-guard.md` and implemented in Stage 3b/3c.

**Coverage requirement**: AC-07 must be exercised at the unit level against real
`UsageService`, NOT just `record_briefing_usage` in isolation. The dedup state must
be inspected directly, not inferred from absence of failure.

### R-02 (AC-17): Phase Round-Trip via Real Analytics Drain

The analytics.rs INSERT, both scan_query_log_* SELECTs, and row_to_query_log are
treated as a single atomic unit (SR-01, ADR-007). The round-trip test is the runtime
guard against any of the four sites diverging.

**Required approach (pattern #3004)**:
- Use real `SqlxStore` + analytics drain, NOT a stub or mock.
- Write a `QueryLogRecord` with `phase = Some("design")` via `insert_query_log`.
- Flush/drain the analytics queue.
- Read back via `scan_query_log_by_session`.
- Assert `record.phase == Some("design")`.
- Repeat with `phase = None`; assert `None` is returned (not panic, not empty string).

See `test-plan/tools-read-side.md` for the full AC-17 test specification and
`test-plan/migration-v16-v17.md` for the store-layer version.

---

## Cross-Component Test Dependencies

| Dependency | Impact on Test Ordering |
|-----------|------------------------|
| `QueryLogRecord::new` signature (Part 2 store changes) | AC-16 and AC-17 tests require Part 2 changes to land first — the updated constructor must be present before writing phase-aware rows |
| `CURRENT_SCHEMA_VERSION = 17` | T-V17-01 through T-V17-06 all assume the constant is 17; they fail immediately if migration.rs still has 16 |
| `make_state_with_rework` updated | All existing SessionState unit tests (AC-20) fail to compile until this helper is updated |
| D-01 guard in `record_briefing_usage` | AC-07 integration test fails until the guard is in place |

Part 2 (store changes: migration.rs, analytics.rs, query_log.rs) compiles independently.
Part 1 (server changes: session.rs, tools.rs, usage.rs) depends on Part 2's
`QueryLogRecord::new` updated signature. Delivery must apply Part 2 first or atomically.

---

## Integration Harness Plan (infra-001)

### Mandatory Gate

`python -m pytest suites/ -v -m smoke --timeout=60`

Must pass before Gate 3c. This validates that the server compiles, starts, and handles
basic tool calls without regression.

### Suite Selection for col-028

| Suite | Rationale |
|-------|-----------|
| `smoke` | Mandatory minimum gate — any change at all |
| `tools` | Feature modifies four read-side tool handlers (`context_search`, `context_lookup`, `context_get`, `context_briefing`) — every parameter, weight, and response must still be correct |
| `lifecycle` | Multi-step flows: briefing→get sequence (D-01 guard), store→search (phase in query_log) — schema + retrieval behavior |
| `confidence` | `context_get` weight changed from 1→2; confidence scores depend on access_weight; must verify no regression in confidence re-ranking |

Suites NOT required for this feature:
- `contradiction` — no changes to contradiction logic
- `security` — no new content scanning or capability boundaries
- `volume` — no scale-specific behavior changed
- `edge_cases` — edge cases covered by unit tests (EC-01 through EC-07)

### New Integration Tests to Add (Stage 3c)

Two new tests must be added to the infra-001 harness in Stage 3c. These cover behavior
that is only observable through the full MCP wire path:

#### Test 1: AC-07 D-01 Dedup Guard (add to `suites/test_lifecycle.py`)

```python
# Naming: test_briefing_then_get_does_not_consume_dedup_slot
# Fixture: server (fresh DB, no state leakage)
```

Scenario:
1. Store entry X via `context_store`.
2. Call `context_briefing` (which includes entry X in returned entries).
3. Call `context_get` for entry X.
4. Call `context_get` for entry X again (second call — should be deduplicated, no increment).
5. Retrieve entry X via `context_lookup`.
6. Assert `access_count` == 2 (not 0, not 4): briefing did not consume the dedup slot;
   first context_get incremented by weight=2; second context_get was deduplicated.

This test fails if: (a) the D-01 guard is absent (access_count = 0 after context_get
following briefing), or (b) context_briefing incorrectly increments access_count itself
(access_count would be > 2).

#### Test 2: AC-16 Phase Written to query_log (add to `suites/test_lifecycle.py`)

```python
# Naming: test_context_search_phase_persisted_to_query_log
# Fixture: server (fresh DB)
```

Scenario (requires session with active phase):
1. Call `context_cycle` to start a session with phase "delivery".
2. Call `context_search`.
3. Wait for analytics drain to flush (pattern #3004 — use a small sleep or drain signal).
4. Call `context_status` (or a raw query if accessible) to confirm `query_log.phase`.
5. Assert phase value equals "delivery" in the retrieved row.

Note: If the MCP interface does not expose direct query_log inspection, this test can be
validated at the store integration tier in `migration_v16_to_v17.rs` (AC-17). The
infra-001 test validates the end-to-end MCP path including the context_cycle
phase-setting → context_search write sequence.

### Fixture Choice

Both new tests use the `server` fixture (function scope, fresh DB) — no state should
accumulate between test runs for these scenarios.

### Triage Rules for Pre-Existing Failures

If any existing infra-001 test fails during Stage 3c:

1. Check whether the failure is in code modified by col-028 (tools.rs, usage.rs,
   session.rs, migration.rs, analytics.rs, query_log.rs). If yes: fix it in this PR.
2. If the failure is in code NOT touched by col-028: file a GH Issue using the template
   in USAGE-PROTOCOL.md. Mark the test `@pytest.mark.xfail(reason="Pre-existing: GH#NNN")`.
   Do NOT fix unrelated failures in this PR.
3. If the test assertion is wrong (bad expected value): fix the test in this PR. Document
   in RISK-COVERAGE-REPORT.md as "Test X had incorrect assertion."
4. Known pre-existing xfails: GH#303 (import::tests pool timeout), GH#305
   (test_retrospective_baseline_present) — these remain xfail; no action needed.

---

## Code-Review Gates (Not Automatable)

These items cannot be verified by automated tests and must be checked by a human reviewer
before the PR merges:

| Gate | Check |
|------|-------|
| AC-12 | `current_phase_for_session` is the first statement in each of the four handler bodies in `mcp/tools.rs`, before any `.await` |
| AC-21 | `analytics.rs` INSERT, both `scan_query_log_*` SELECTs, and `row_to_query_log` are all modified in the same commit |
| AC-22 | `grep -r 'schema_version.*== 16' crates/` returns zero matches |
| AC-24 | `confirmed_entries` field carries its full doc comment per SPECIFICATION.md §Exact Signatures |
| NFR-05 | `mcp/tools.rs` does not exceed 500 lines after all four call-site changes |
| C-04 | Exactly one `get_state` call in the `context_search` handler body |

---

## Minimum Gate Summary

To pass Gate 3c, ALL of the following must be true:

1. `cargo test --workspace` — green, no new failures (AC-20, AC-23)
2. `python -m pytest suites/ -v -m smoke --timeout=60` — all pass (minimum infra gate)
3. `python -m pytest suites/test_tools.py suites/test_lifecycle.py suites/test_confidence.py -v --timeout=60` — pass (feature suites)
4. AC-07 integration test passes (D-01 guard — briefing→get access_count = 2)
5. AC-17 round-trip test passes (phase read back from store)
6. `grep -r 'schema_version.*== 16' crates/` returns zero matches (AC-22)
7. `cargo build --workspace` succeeds (AC-23)
