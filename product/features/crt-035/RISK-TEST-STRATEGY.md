# Risk-Based Test Strategy: crt-035

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Back-fill NOT EXISTS sub-join uses three separate single-column indexes, not a composite — SQLite may not merge them for the reverse-lookup; full scan on large graphs | Med | Med | High |
| R-02 | T-BLR-08 misclassified as "not requiring modification" in the spec, but its `count == 1` assertion is broken — silent false-pass if delivery agent trusts the "no change needed" list | High | Med | Critical |
| R-03 | OQ-01 unresolved: `test_existing_edge_stale_weight_updated` count 1→2 not confirmed by architect — delivery agent may leave the old assertion intact | Med | Med | High |
| R-04 | OQ-02 unresolved: weight=0.0 forward edges back-filled as weight=0.0 reverse edges — PPR treats 0.0-weight edges as zero contribution; pair effectively invisible to PPR | Low | Low | Med |
| R-05 | `promote_one_direction` helper not atomic per pair — partial failure leaves forward at new weight and reverse at stale weight (or vice versa) for one tick interval; PPR sees asymmetric scores | Med | Low | Med |
| R-06 | `test_existing_edge_current_weight_no_update` is listed as "no change needed" but does not assert the reverse edge is inserted — the test becomes incomplete coverage after crt-035 | Low | Med | Med |
| R-07 | AC-12 test uses `SqlxStore` (real SQLite path per spec), but architecture doc describes an in-memory `TypedRelationGraph` path — contradiction may lead delivery agent to use the wrong fixture | Med | Med | High |
| R-08 | `count_co_access_edges` helper counts ALL CoAccess rows — after bidirectional change it returns 2N for N pairs; blast-radius tests that miss the 2× factor will fail silently on partial coverage | Med | High | Critical |
| R-09 | Migration block runs inside the main transaction; a back-fill error rolls back to v18 and the DB stays unupgraded — no recovery path documented; repeated open attempts may loop on migration error | Low | Low | Med |
| R-10 | `CURRENT_SCHEMA_VERSION` bump not guarded against concurrent branch version collision (e.g. a hotfix also targeting v19 lands in the same window) | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: NOT EXISTS self-join index coverage for back-fill SQL
**Severity**: Med
**Likelihood**: Med
**Impact**: On production databases with large bootstrap edge sets, the v18→v19 migration open could take seconds or minutes instead of milliseconds. The three single-column indexes (`idx_graph_edges_source_id`, `idx_graph_edges_target_id`, `idx_graph_edges_relation_type`) do not form a composite covering `(source_id, target_id, relation_type)`. SQLite must choose one index or perform a full scan for the NOT EXISTS inner select.

**Test Scenarios**:
1. In `migration_v18_to_v19.rs` — seed a v18 DB with 1000+ forward-only CoAccess edges and verify the migration completes without error (functional correctness at volume, not timing). This does not catch a slow path but confirms correctness at scale.
2. Manually run `EXPLAIN QUERY PLAN` on the back-fill SQL against a schema with the three existing indexes to confirm SQLite uses `idx_graph_edges_source_id` for the inner select lookup. Document the result in the migration test or spec.

**Coverage Requirement**: The migration integration test (MIG-U-03, MIG-U-04) must include a multi-row scenario (not just 1–2 edges) to give confidence that the index path is exercised. Delivery agent should run `EXPLAIN QUERY PLAN` and record the output.

---

### R-02: T-BLR-08 misclassification — `test_existing_edge_stale_weight_updated` breaks silently
**Severity**: High
**Likelihood**: Med
**Impact**: The spec's "Tests that do NOT require modification" section (line 369) initially lists this test without a break annotation, then corrects it as T-BLR-08 at line 405. A delivery agent reading only the "no change needed" table and missing T-BLR-08 will leave `assert_eq!(count, 1, "no duplicate")` intact. The assertion will fail in the test suite, but if the agent reorders their work, a lingering stale assertion passes against the pre-change DB state. Lesson #3548 (test exists but omits plan assertion) applies here.

**Test Scenarios**:
1. Gate-3b check: grep `co_access_promotion_tick_tests.rs` for the literal string `"no duplicate"` — it must not be present after crt-035; any match is a residual stale assertion.
2. Gate-3b check: grep for `count_co_access_edges` assertions — every assertion value should be even (0, 2, 4, 6, 10 ...) or zero; an odd value (1, 3, 5 ...) post-crt-035 indicates a missed blast-radius update.
3. T-BLR-08 explicitly: after tick on a pair where only the forward edge was pre-seeded, assert `count_co_access_edges == 2`, forward weight == 1.0, reverse weight == 1.0.

**Coverage Requirement**: T-BLR-08 is mandatory. The gate validator must grep for the "no duplicate" comment and for odd-valued `count_co_access_edges` assertions as non-negotiable checks.

---

### R-03: OQ-01 unresolved — count 1→2 for `test_existing_edge_stale_weight_updated`
**Severity**: Med
**Likelihood**: Med
**Impact**: If the architect does not explicitly confirm `count == 2` (not a duplicate, both directions), the delivery agent may interpret "no duplicate" as `count == 1` and leave the assertion unchanged. The test then fails at runtime rather than at review. This is OQ-01 from the spec — it is an open question and must be resolved before delivery.

**Test Scenarios**:
1. Delivery cannot start until OQ-01 is answered in the spec with an explicit `count == 2` resolution.
2. After resolution: the test asserts forward at 1.0, reverse at 1.0, count == 2, no third row.

**Coverage Requirement**: OQ-01 must be closed (architect sign-off on count = 2) before the test is written.

---

### R-04: OQ-02 unresolved — weight=0.0 back-fill produces 0-weight reverse edges
**Severity**: Low
**Likelihood**: Low
**Impact**: A production graph with corrupt or edge-case forward edges at weight=0.0 receives reverse edges at weight=0.0. PPR contributions from 0.0-weight edges are zero; the back-fill pair is effectively invisible to traversal. No data loss but the feature's PPR improvement is silently incomplete for those pairs.

**Test Scenarios**:
1. Migration test: seed one forward CoAccess edge with `weight = 0.0`; run migration; assert reverse edge is inserted with `weight = 0.0`. This confirms current behavior (no floor applied).
2. If architect decides to add a weight floor: assert `MAX(weight, floor)` in the back-fill SQL; add a test seeding 0.0 that asserts reverse weight == floor value.

**Coverage Requirement**: OQ-02 resolution determines the correct assertion. Either path (no floor or with floor) must have a test case for the weight=0.0 input.

---

### R-05: Partial tick failure leaves asymmetric edge weights for one tick interval
**Severity**: Med
**Likelihood**: Low
**Impact**: If `promote_one_direction` succeeds for the forward direction and fails for the reverse (or vice versa), the two directions hold different weights until the next tick. PPR traversals seeding either endpoint will see different neighbour scores for the same pair during the convergence window (~60s). ADR-001 accepts this as eventual consistency, but a test covering the convergence path confirms it.

**Test Scenarios**:
1. Pre-seed forward at weight 0.5 and reverse at weight 0.2; run one tick with `new_weight = 1.0`; assert both updated to 1.0 — confirms convergence when both are stale (FR-12, T-NEW-02).
2. Simulate a partial write failure (using the existing write-failure injection pattern) on the reverse direction only; assert forward updates and reverse retains its old weight; run a second tick; assert reverse also converges to 1.0.

**Coverage Requirement**: T-NEW-02 (FR-12 verification) covers the happy-path convergence. Failure-injection for the reverse-only failure path is recommended but not blocking.

---

### R-06: `test_existing_edge_current_weight_no_update` becomes incomplete after crt-035
**Severity**: Low
**Likelihood**: Med
**Impact**: This test checks that a forward edge at the current weight is not updated. After crt-035 the tick also inserts the reverse edge as a new row. The test does not assert the reverse edge exists, leaving that new behavior untested in this scenario. The spec lists this test as "no change needed" — this is correct for the existing assertions but incomplete for the new behavior.

**Test Scenarios**:
1. After crt-035: extend this test to also assert `fetch_co_access_edge(reverse_direction).is_some()` with the expected weight. The forward "no update" assertion remains; the reverse "inserted" assertion is added.

**Coverage Requirement**: This is a coverage gap rather than a break. Acceptable to address in a follow-up if not included in the initial delivery, but the tester should flag it.

---

### R-07: AC-12 test fixture contradiction — spec says SqlxStore, architecture says in-memory
**Severity**: Med
**Likelihood**: Med
**Impact**: The architecture document (Component 5, SR-06 section) describes the AC-12 test as using an in-memory `TypedRelationGraph` built via `make_graph_with_edges`, bypassing SQLite. The specification (AC-12, line 218) explicitly requires a real `SqlxStore` (tempfile-backed). These two descriptions are contradictory. Scope risk SR-06 was resolved as "architect confirms AC-12 hits the real SQLite path" — the spec reflects this resolution. The architecture doc contains a stale description from an earlier design iteration.

**Test Scenarios**:
1. Gate-3b check: grep `typed_graph.rs` for `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry`; confirm the test opens a `SqlxStore` (calls `SqlxStore::open` or equivalent tempfile-backed store), not a bare `TypedRelationGraph::new()`.
2. The test must insert the edge via SQL (`INSERT INTO graph_edges`) or the store's write API, then call `TypedGraphState::rebuild()`, then run PPR — confirming the full data path from storage to graph to PPR.

**Coverage Requirement**: The test must exercise the `GRAPH_EDGES → TypedRelationGraph → PPR` path. An in-memory shortcut misses the read side of `build_typed_relation_graph`. Non-negotiable per spec AC-12.

---

### R-08: `count_co_access_edges` returns 2N after bidirectional change — missed updates produce failing tests
**Severity**: Med
**Likelihood**: High
**Impact**: The test helper `count_co_access_edges` counts all CoAccess rows in `GRAPH_EDGES`. Before crt-035 it returned N (one per pair). After crt-035 it returns 2N (two per pair). Any blast-radius test updated to the wrong target value (e.g., `count == 3` updated to `count == 5` instead of `count == 6`) will either fail or silently accept an incorrect partial count. Lesson #3579 (absent test modules pass gate silently) and the SCOPE SR-05 blast radius analysis apply.

**Test Scenarios**:
1. Enumerate every `count_co_access_edges` call in the test file and verify the expected value is exactly `2 * (number of pairs processed in that test)`. Any odd value is wrong.
2. Gate-3c non-negotiable grep: the literal string `count_co_access_edges` must not appear adjacent to an odd integer assertion value in the test file.

**Coverage Requirement**: All 8 blast-radius test updates (T-BLR-01 through T-BLR-08) must have their count assertions verified via the 2× rule before gate-3b.

---

### R-09: Migration error inside main transaction rolls back to v18 with no documented recovery path
**Severity**: Low
**Likelihood**: Low
**Impact**: The back-fill SQL runs inside the main migration transaction. A SQL error (e.g., constraint violation that INSERT OR IGNORE does not handle, or I/O error) rolls back the entire transaction, leaving the DB at v18. The next open attempt re-runs the same migration and may loop. No retry limit or skip mechanism is specified.

**Test Scenarios**:
1. Migration idempotency test (MIG-U-06) already covers the success re-run path.
2. No test for forced failure mid-back-fill is required, but the spec should acknowledge that the loop-on-failure behavior is acceptable given the migration is data-only and idempotent on success.

**Coverage Requirement**: MIG-U-06 (idempotency) is sufficient. A forced-failure test is optional and not blocking delivery.

---

### R-10: Concurrent branch version collision on schema version 19
**Severity**: Low
**Likelihood**: Low
**Impact**: If another branch in the same window independently increments schema version to 19, the `current_version < 19` guard produces a conflict. SCOPE.md documents this assumption explicitly (§Migration Framework). No test can prevent this — it is a process risk.

**Test Scenarios**:
1. MIG-U-01 asserts `CURRENT_SCHEMA_VERSION == 19` — will catch a missed bump but not a collision.

**Coverage Requirement**: Process control only. No additional test needed.

---

## Integration Risks

**IR-01 — Tick → GRAPH_EDGES write pool contention**: `promote_one_direction` is called twice per pair, doubling the SQL calls per tick batch. On a heavily-loaded instance with a large qualifying set (at the `max_co_access_promotion_per_tick` cap), the write pool connection may be held for longer. No pool changes are required per NFR-06, but this doubles the write I/O per tick run. The risk is bounded: the operation is I/O-bound, and the cap limits total pairs processed.

**IR-02 — TypedGraphState rebuild lag**: Reverse edges written by the back-fill are immediately visible to `build_typed_relation_graph` on the next rebuild. There is no lag risk from caching because `TypedGraphState::rebuild` reads directly from `GRAPH_EDGES`. However, if `TypedGraphState` is rebuilt before the back-fill completes (hypothetically, in a race between migration and a search request), PPR will see a partially bidirectional graph. In practice, migration completes before the server accepts requests; this race does not occur in the normal startup sequence.

**IR-03 — `created_by` provenance in back-fill**: Back-filled reverse edges copy `created_by` from the forward edge (D1). Future tooling that filters `GRAPH_EDGES` by `created_by` to count tick-era edges will now count both forward and reverse tick edges. This doubles the `created_by = 'tick'` count without changing the `promoted_pairs` business metric. Any downstream analytics that count by `created_by` must be aware of this post-crt-035.

---

## Edge Cases

**EC-01 — Empty `GRAPH_EDGES` at migration time**: The back-fill SELECT returns zero rows; INSERT OR IGNORE inserts nothing. Migration completes successfully. Covered by MIG-U-07.

**EC-02 — All CoAccess edges already bidirectional** (e.g., re-run after successful migration): NOT EXISTS filter excludes all rows; INSERT OR IGNORE has nothing to insert. Idempotent. Covered by MIG-U-06.

**EC-03 — Non-CoAccess edges in `GRAPH_EDGES`** (`Supersedes`, `Contradicts`, `Supports`): `WHERE relation_type = 'CoAccess' AND source = 'co_access'` excludes all non-CoAccess rows. Covered by MIG-U-05.

**EC-04 — Single pair with forward edge at weight=0.0**: Back-fill inserts reverse at weight=0.0. PPR contribution is zero. See R-04.

**EC-05 — `promote_one_direction` called with `source_id == target_id`** (self-loop): The existing `test_self_loop_pair_no_panic` covers this for the tick. The bidirectional change calls the helper twice with `(a, a)` and `(a, a)` — the UNIQUE constraint silently handles the second insert as a duplicate of the first via `INSERT OR IGNORE`. No panic expected; the self-loop test should be confirmed to still pass.

**EC-06 — Back-fill SQL `strftime('%s','now')` for `created_at`**: All reverse edges written by back-fill share the migration timestamp, not the original forward edge timestamp. This is intentional and correct; `created_at` records when the row was written, not when the relationship was first observed.

---

## Security Risks

**SR-SEC-01**: crt-035 introduces no new external input surfaces. The back-fill SQL is a static query with no user-supplied parameters. The tick SQL parameters (`source_id`, `target_id`, `new_weight`) are derived from the `CO_ACCESS` table, itself written only by the internal co-access recording path — not from external tool calls or user input. No path traversal, injection, or deserialization risk is introduced.

**SR-SEC-02**: `bootstrap_only = 0` is hardcoded in the back-fill INSERT. Reverse edges are never marked bootstrap-only, so they are included in `build_typed_relation_graph` reads (which filter out `bootstrap_only = 1` rows). This is correct behavior, not a security risk.

---

## Failure Modes

**FM-01 — Back-fill fails mid-run**: Transaction rolls back; DB stays at v18. Server logs a migration error. Next open retries the full back-fill. Acceptable because the migration is idempotent on success.

**FM-02 — `promote_one_direction` fails on reverse direction**: The forward direction write may have succeeded. The function logs `warn!`, returns `(false, false)` for that call, and the tick continues to the next pair. On the next tick, the INSERT for the reverse direction is a no-op (edge already exists from a prior partial success) and the UPDATE path detects the delta and corrects the weight. One-tick convergence.

**FM-03 — `promote_one_direction` fails on forward direction**: Reverse direction is still attempted independently (infallible contract). If forward fails and reverse succeeds, the graph transiently has only the reverse edge for that pair. The next tick inserts the forward edge and both weights converge.

**FM-04 — Weight floor absent (OQ-02 unresolved)**: Forward edges at weight=0.0 produce reverse edges at weight=0.0 after back-fill. PPR treats these as invisible. No crash; silent under-coverage of the CoAccess signal for those pairs.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — weight divergence between forward/reverse | R-05 | Resolved: ADR-001 accepts eventual consistency; `promote_one_direction` called independently for each direction; convergence on next tick |
| SR-02 — 500-line file ceiling | — | Resolved: `promote_one_direction` helper required (FR-06, C-03); verified by `wc -l` gate check |
| SR-03 — `inserted_count` semantics shift | — | Resolved: D2 new log format specifies `promoted_pairs` + `edges_inserted` + `edges_updated`; verified by T-NEW-03 |
| SR-04 — NOT EXISTS self-join index performance | R-01 | Partially resolved: UNIQUE constraint covers `(source_id, target_id, relation_type)` per architect (ARCHITECTURE.md §Component 2). However, three separate single-column indexes exist in code, not a composite — OQ-03 is open. Delivery agent must run EXPLAIN QUERY PLAN to confirm. |
| SR-05 — test blast radius underspecified | R-02, R-08 | Spec enumerates all 8 broken tests (T-BLR-01 through T-BLR-08). T-BLR-08 appears in two sections of the spec — delivery agent must treat BOTH sections as authoritative. Gate-3b grep required. |
| SR-06 — AC-12 test may use synthetic fixture | R-07 | Spec (AC-12) explicitly requires `SqlxStore` + `TypedGraphState::rebuild()`. Architecture doc contains a stale in-memory description. Spec is authoritative. |
| SR-07 — near-threshold oscillation transient asymmetry | R-05 | Accepted: NFR-07 documents eventual consistency; both directions converge to same `new_weight` on the tick where both pass the delta guard |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-02, R-08) | T-BLR-08 mandatory; gate-3b grep for "no duplicate" + odd count values |
| High | 3 (R-01, R-03, R-07) | OQ-01 resolution before delivery; EXPLAIN QUERY PLAN confirmation; AC-12 SqlxStore path grep |
| Medium | 4 (R-04, R-05, R-06, R-09) | OQ-02 resolution; T-NEW-02 convergence test; R-06 coverage gap note |
| Low | 1 (R-10) | Process control only; no test required |

---

## Gate-3b Validation Requirements

These checks are **non-negotiable** and must be verified by the gate validator before delivery is accepted. They are not advisory — a failed check is a gate block.

### GATE-3B-01: `"no duplicate"` grep (R-02 — Critical)

```
grep -n '"no duplicate"' crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs
```

**Must return zero matches.** The string `"no duplicate"` in `test_existing_edge_stale_weight_updated` (T-BLR-08) encodes the old one-directional contract. Its presence after crt-035 means the test was not updated — it will assert `count == 1` and pass only if the implementation is incomplete. Any match is a delivery block.

### GATE-3B-02: Odd `count_co_access_edges` assertion grep (R-08 — Critical)

```
grep -n 'count_co_access_edges\|assert_eq!(count' crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs
```

Inspect every numeric assertion value in the output. **All post-crt-035 `count_co_access_edges` return values must be even (0, 2, 4, 6, 10…).** An odd value (1, 3, 5…) means a blast-radius test was missed or updated to the wrong target. Even-only is not a coincidence — it is the invariant that bidirectional edge pairs produce.

### GATE-3B-03: EXPLAIN QUERY PLAN on back-fill SQL (R-01 — High)

Run against the real schema (open a tempfile-backed `SqlxStore`, then execute):

```sql
EXPLAIN QUERY PLAN
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
SELECT g.target_id, g.source_id, 'CoAccess', g.weight, strftime('%s','now'),
       g.created_by, 'co_access', 0
FROM graph_edges g
WHERE g.relation_type = 'CoAccess'
  AND g.source = 'co_access'
  AND NOT EXISTS (
    SELECT 1 FROM graph_edges rev
    WHERE rev.source_id = g.target_id
      AND rev.target_id = g.source_id
      AND rev.relation_type = 'CoAccess'
  )
```

**Expected**: inner select uses `SEARCH graph_edges rev USING INDEX sqlite_autoindex_graph_edges_1` (the UNIQUE constraint B-tree). If the output shows `SCAN graph_edges rev` (full scan) for the NOT EXISTS sub-select, add a composite index `CREATE INDEX IF NOT EXISTS idx_ge_rev_lookup ON graph_edges (source_id, target_id, relation_type)` to the v18→v19 migration DDL before merging. Document the EXPLAIN output as a comment in `tests/migration_v18_to_v19.rs`.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection migration` — found #3579 (absent test modules), #2758 (gate-3c non-negotiable grep), #3548 (test omits plan assertion); applied to R-02 and R-08 severity assessments.
- Queried: `/uni-knowledge-search` for `risk pattern co_access graph promotion tick` — found #3822 (oscillation pattern), #3889 (back-fill reverse edges pattern), #3891 (ADR-006 updated); confirmed scope risk resolutions.
- Queried: `/uni-knowledge-search` for `SQLite migration back-fill performance NOT EXISTS index scan` — found #681 (create-new-then-swap), #374 (in-place migration procedure); confirmed no prior composite-index lesson on this table.
- Stored: nothing novel to store — the index coverage gap (R-01/OQ-03) is specific to this feature's schema state, not a cross-feature pattern. Existing #3889 covers the back-fill approach pattern.
