# Risk-Based Test Strategy: crt-034

Recurring co_access → GRAPH_EDGES Promotion Tick

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Silent absorption of write failures leaves edges un-promoted with no caller-visible signal | High | Med | Critical |
| R-02 | Weight normalization uses global MAX(count): if max_count=0 (empty table after WHERE filter), division produces NaN/panic in Rust before the no-row guard fires | High | Low | High |
| R-03 | Scalar subquery MAX(count) correctness: subquery re-evaluated per row if SQLite planner does not inline it; result could differ from the batch-level max under concurrent co_access writes | Med | Low | High |
| R-04 | INSERT OR IGNORE no-op detection misread: rows_affected==0 is the no-op signal; if the driver returns rows_affected==1 for an ignored insert (implementation variance), weight UPDATE is skipped on every tick | High | Low | High |
| R-05 | Tick ordering violation: promotion runs AFTER TypedGraphState::rebuild() if the call site in background.rs is misplaced, silently deferring freshly promoted edges by one cycle | High | Low | High |
| R-06 | SR-05 signal-loss detectability: the early-tick warn! fires only when current_tick < 5; if the tick counter resets on server restart, the window re-opens and masks legitimate zero-row state | Med | Med | High |
| R-07 | Config field max_co_access_promotion_per_tick not included in merge_configs() stanza: project-level override silently ignored; cap always uses global default | Med | Med | High |
| R-08 | CO_ACCESS_GRAPH_MIN_COUNT diverges from migration's CO_ACCESS_BOOTSTRAP_MIN_COUNT if either is changed independently; promoted edges and bootstrapped edges use different thresholds | Med | Low | Med |
| R-09 | Near-threshold pair oscillation: pairs hovering at count=3 are fetched every tick; INSERT OR IGNORE no-op is correct but the SELECT weight + conditional UPDATE path executes needlessly when weight has not changed | Low | High | Med |
| R-10 | One-directional edge contract violated if implementor writes (entry_id_b, entry_id_a) edges alongside (entry_id_a, entry_id_b); UNIQUE constraint does not prevent this — it treats them as distinct rows | Med | Low | Med |
| R-11 | Per-tick cap ORDER BY count DESC omitted: arbitrary drain order under cap means low-signal pairs promoted before high-signal pairs | High | Low | High |
| R-12 | File size limit: co_access_promotion_tick.rs exceeds 500 lines (lesson #3580 — gate-3b file size violations discovered late) | Low | Med | Med |
| R-13 | Inserted edges missing required metadata fields (bootstrap_only=0, source='co_access', created_by='tick'): downstream audit and GC rely on these for correct co_access edge identification | Med | Med | High |

---

## Risk-to-Scenario Mapping

### R-01: Silent absorption of write failures leaves edges un-promoted

**Severity**: High
**Likelihood**: Med
**Impact**: A SQLite timeout or constraint error on a single INSERT or UPDATE is swallowed by the infallible contract. The pair waits one tick. If errors are systematic (pool exhaustion, locked write pool), no pairs are promoted and no caller sees an error. Observability is the only defense.

**Test Scenarios**:
1. Inject a write failure on the first INSERT call; assert the function returns `()`, emits a `warn!` log containing the pair identifiers, and continues attempting remaining pairs.
2. Inject a write failure on the UPDATE path; assert remaining pairs are processed and the `tracing::info!` log at the end reflects the correct inserted/updated counts (excluding the failed pair).
3. Confirm the tick-level `tracing::info!` log always fires even when all individual writes fail.

**Coverage Requirement**: At least one test must simulate a write failure mid-batch and verify both the warn! emission and the continuation of remaining pairs. The info! summary log must appear.

---

### R-02: Division by zero when max_count=0

**Severity**: High
**Likelihood**: Low
**Impact**: If max_count returns NULL from the subquery (co_access table is empty after the WHERE filter), SQLite returns NULL per row. If Rust decodes a NULL i64 as 0 and the code performs `count as f32 / max_count as f32`, the result is NaN or a panic. The query-returns-no-rows guard (early return on empty fetch_all) prevents this — but only if the guard is placed before the per-pair loop.

**Test Scenarios**:
1. Run the promotion tick against an empty `co_access` table; assert no panic, no warn!, info! log shows 0 inserted / 0 updated.
2. Run against a `co_access` table with rows all below threshold (count=1, threshold=3); assert identical no-op behavior.
3. Assert the Rust row type for max_count is `Option<i64>` and the code handles `None` as an early-return.

**Coverage Requirement**: Empty table and all-sub-threshold tests are mandatory. These correspond directly to AC-09.

---

### R-03: Scalar subquery MAX(count) correctness under concurrent writes

**Severity**: Med
**Likelihood**: Low
**Impact**: SQLite evaluates the scalar subquery once per result row if the planner does not cache it. Under concurrent co_access writes (unlikely — SQLite serializes writes), max_count could change between rows, making normalization inconsistent within a single tick's batch. More practically: if the query is hand-written incorrectly as a correlated subquery referencing the outer row, max_count tracks each row's own count (always 1.0 after normalization), defeating the global normalization requirement.

**Test Scenarios**:
1. Seed 10 pairs with counts [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], set cap to 3; assert promoted pairs use max_count=10 (global), not the max within the selected top-3 batch (count=10 is in the top-3, so this also tests the subquery correctly includes the full qualifying set). Mirrors AC-13 exactly.
2. Seed pairs so the highest-count pair is NOT in the capped batch (e.g., 5 pairs with counts [1,2,3,4,100], cap=3 selects [100, 4, 3]); assert max_count=100 across all promoted pairs.

**Coverage Requirement**: AC-13 test is mandatory. The "max is outside the capped batch" scenario is a distinct additional scenario.

---

### R-04: INSERT OR IGNORE no-op detection via rows_affected

**Severity**: High
**Likelihood**: Low
**Impact**: The two-step write logic branches on `rows_affected == 0` to detect an already-existing edge. If sqlx or the underlying driver returns an unexpected value (e.g., 1 for a no-op IGNORE), the weight UPDATE check is skipped on every tick, making weight refresh permanently non-functional.

**Test Scenarios**:
1. Pre-insert a `CoAccess` edge for a pair, then run the promotion tick with that pair still qualifying; assert the edge count in `GRAPH_EDGES` does not increase (no duplicate row).
2. With an existing edge whose weight is stale (delta > 0.1), run the tick; assert the weight IS updated — confirming the no-op branch correctly triggers the SELECT+UPDATE path.
3. With an existing edge whose weight is current (delta <= 0.1), run the tick; assert `rows_affected == 0` for the UPDATE and weight is unchanged. Corresponds to AC-03 / AC-14.

**Coverage Requirement**: All three scenarios required. AC-02, AC-03, and AC-14 map directly to these.

---

### R-05: Tick ordering violation in background.rs

**Severity**: High
**Likelihood**: Low
**Impact**: If `run_co_access_promotion_tick` is called after `TypedGraphState::rebuild()`, newly promoted edges are not visible to PPR until the next tick cycle. This is a silent correctness failure — no error, no warning, graphs appear to work but co-access signal lags by one tick permanently.

**Test Scenarios**:
1. Code review (static verification): confirm the call to `run_co_access_promotion_tick` in background.rs appears between the orphaned-edge compaction block and the `TypedGraphState::rebuild()` call. This is AC-05.
2. Integration test (if background.rs has testable tick sequencing): seed a qualifying pair, fire one tick, assert the pair is visible in `TypedRelationGraph` state after the tick (not deferred to the next).

**Coverage Requirement**: AC-05 is verified by code review. An integration-level ordering test is desirable but complex; code review is the primary gate.

---

### R-06: Early-tick warn! window re-opens on server restart

**Severity**: Med
**Likelihood**: Med
**Impact**: `current_tick` is a per-process counter starting at 0 on each server start. After a server restart (routine deployment, crash), ticks 0–4 fire the SR-05 warn! for zero qualifying rows even when all pairs are already promoted. This produces false-positive warnings in logs, eroding the signal value of the warn!.

**Test Scenarios**:
1. Run the promotion function with `current_tick=0` and a fully-promoted table (all edges already in `GRAPH_EDGES`); assert no warn! is emitted (because qualifying_count > 0, not because current_tick >= 5 — the warn fires only when qualifying_count == 0).
2. Run with `current_tick=0` and an empty `co_access` table; assert warn! IS emitted (legitimate signal-loss scenario).
3. Run with `current_tick=10` and an empty `co_access` table; assert warn! is NOT emitted (past the early-run window).

**Coverage Requirement**: The warn! condition is `qualifying_count == 0 AND current_tick < PROMOTION_EARLY_RUN_WARN_TICKS`. Tests must cover all four quadrants of (qualifying_count=0/positive) × (tick<5/tick>=5).

---

### R-07: Config field absent from merge_configs()

**Severity**: Med
**Likelihood**: Med
**Impact**: If `max_co_access_promotion_per_tick` is added to the `InferenceConfig` struct and `Default` impl but omitted from `merge_configs()`, project-level config overrides are silently ignored. The cap always uses the global default (200). Operators cannot throttle the tick.

**Test Scenarios**:
1. Unit test: build a config with project-level `max_co_access_promotion_per_tick = 50` and global `max_co_access_promotion_per_tick = 200`; after merge, assert the merged config has 50.
2. Unit test: build a config where only the global sets the field to 300; assert the merged config has 300.
3. Unit test: validate that `max_co_access_promotion_per_tick = 0` returns a validation error naming the field. Corresponds to AC-10.
4. Unit test: validate that 1 and 10000 are accepted, 10001 is rejected. Corresponds to ADR-004 boundary tests.

**Coverage Requirement**: All four sub-tests required. These correspond to AC-06 and AC-10.

---

### R-08: Threshold constant divergence between tick and migration

**Severity**: Med
**Likelihood**: Low
**Impact**: `CO_ACCESS_GRAPH_MIN_COUNT = 3` in `unimatrix-store` and `CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3` in `migration.rs` are separate symbols. If either is changed without updating the other (e.g., a future feature lowers the bootstrap threshold to 2), bootstrapped edges and tick-promoted edges use different qualification criteria. Pairs in the [2, 3) range exist in GRAPH_EDGES from the bootstrap but are never refreshed by the tick.

**Test Scenarios**:
1. Compile-time: confirm `CO_ACCESS_GRAPH_MIN_COUNT` is used by the tick's batch query (not a bare literal `3`). Static check via grep or code review.
2. Unit test: assert `CO_ACCESS_GRAPH_MIN_COUNT == 3` (value correctness) and that the constant is re-exported from `unimatrix-store`. Corresponds to AC-07.

**Coverage Requirement**: AC-07 constant existence test plus a note in the implementation that the migration constant is a separate symbol — both must use the same value.

---

### R-09: Near-threshold pair re-evaluation overhead

**Severity**: Low
**Likelihood**: High
**Impact**: Pairs at count=3 (exactly at threshold) are fetched every tick, run the INSERT OR IGNORE path (no-op), then the SELECT weight path, then the delta check (no-op if weight unchanged). Zero writes occur, but two read queries are executed per near-threshold pair per tick. At table sizes cited (~0.34 MB), this is negligible — but it is untested behavior that should be confirmed as correct (no spurious writes).

**Test Scenarios**:
1. Unit test: promote a pair, keep its count unchanged, run the tick a second time; assert `GRAPH_EDGES` has exactly one row for that pair and weight is unchanged. Corresponds to AC-14.
2. Unit test: promote a pair, drop its count below threshold, run the tick; assert the `GRAPH_EDGES` row still exists (no GC). Corresponds to AC-15.

**Coverage Requirement**: AC-14 and AC-15 are mandatory. The "no spurious writes" invariant must be verifiable by asserting rows_affected on the UPDATE path.

---

### R-10: One-directional edge contract violated by implementor

**Severity**: Med
**Likelihood**: Low
**Impact**: If the implementor writes both `(entry_id_a, entry_id_b)` and `(entry_id_b, entry_id_a)` edges in crt-034 (thinking bidirectionality is correct), it diverges from the bootstrap's one-directional layout. PPR then treats newly promoted pairs differently from bootstrapped pairs — asymmetric co-access signal within the same graph. The UNIQUE constraint does not prevent this; it treats the two orderings as distinct rows.

**Test Scenarios**:
1. Unit test: after promoting a pair (entry_id_a=1, entry_id_b=2), query `GRAPH_EDGES` for CoAccess edges involving either entry; assert exactly one row exists with source_id=1, target_id=2, and no reverse row (source_id=2, target_id=1).

**Coverage Requirement**: This is a structural correctness test. One mandatory assertion per promotion test that counts total CoAccess rows for the pair and confirms exactly one direction.

---

### R-11: ORDER BY count DESC omitted from batch query

**Severity**: High
**Likelihood**: Low
**Impact**: Without `ORDER BY count DESC LIMIT cap`, SQLite returns an arbitrary subset when the qualifying pairs exceed the cap. Low-signal pairs (count=3) are promoted before high-signal pairs (count=100). The PPR graph gains noise edges before meaningful co-access edges, reducing retrieval quality.

**Test Scenarios**:
1. Unit test: seed 10 pairs with counts [3, 3, 3, 3, 3, 10, 20, 50, 80, 100], set cap=3; after one tick, assert the three edges in `GRAPH_EDGES` correspond to counts [100, 80, 50] (the highest three). Corresponds to AC-04.

**Coverage Requirement**: AC-04 is mandatory. The test must explicitly verify which pairs were selected, not just that N edges exist.

---

### R-12: File size limit violation

**Severity**: Low
**Likelihood**: Med
**Impact**: Gate-3b consistently catches 500-line violations discovered late (lesson #3580, #1203). The promotion module is pure SQL — straightforward — but a thorough test suite embedded in the same file could push it over 500 lines.

**Test Scenarios**:
1. CI gate: `wc -l crates/unimatrix-server/src/services/co_access_promotion_tick.rs` asserts under 500 lines.

**Coverage Requirement**: Self-check by implementation agent before submitting. Gate validator must verify.

---

### R-13: Inserted edge missing required metadata fields

**Severity**: Med
**Likelihood**: Med
**Impact**: Downstream audit queries and GH #409 GC logic identify tick-promoted co_access edges by `source = 'co_access'`, `created_by = 'tick'`, `relation_type = 'CoAccess'`, `bootstrap_only = 0`. A missing or wrong field makes edges indistinguishable from bootstrap edges or other relation types. GC targeting the wrong edges causes data loss.

**Test Scenarios**:
1. Unit test: promote one pair, SELECT the resulting row from `GRAPH_EDGES`; assert `bootstrap_only = 0`, `source = 'co_access'`, `created_by = 'tick'`, `relation_type = 'CoAccess'`. Corresponds to AC-12.
2. Assert `weight` is in [0.0, 1.0] and is the correct normalized value for the pair's count relative to global MAX.

**Coverage Requirement**: AC-12 is mandatory and must check all four metadata fields in a single row assertion.

---

## Integration Risks

**I-01: GH #409 race (SR-05)** — If GH #409 (co_access pruning) merges and deploys before crt-034, qualifying pairs crossing the threshold are pruned before the first promotion tick runs. Signal is lost silently. The SR-05 early-tick warn! in `run_co_access_promotion_tick` is the only detection mechanism. Mitigation: enforce GH milestone ordering; verify at delivery time that #409 is not yet merged.

**I-02: Analytics drain path not used** — The analytics drain (`AnalyticsWrite::GraphEdge`) is INSERT OR IGNORE only. If future code refactors the tick to route through the drain, the conditional UPDATE for weight refresh silently becomes a no-op. The direct `write_pool_server()` path must be preserved. Tests should assert the promotion tick does NOT enqueue to the analytics drain.

**I-03: TypedGraphState::rebuild() consumes freshly promoted edges** — The in-memory `TypedRelationGraph` is rebuilt after promotion. If the rebuild reads only edges marked `bootstrap_only = 0` (or applies any filter not documented), tick-promoted edges may be silently excluded. Code review of `TypedGraphState::rebuild()` must confirm it reads all `CoAccess` edges regardless of `created_by`.

**I-04: cleanup_stale_co_access ordering** — The promotion tick must run AFTER `cleanup_stale_co_access()` (called inside `maintenance_tick()`). If a co_access row is pruned mid-tick by a concurrent cleanup, the INSERT OR IGNORE produces a dangling edge pointing to an entry that no longer qualifies. The orphaned-edge compaction in step 2 handles the structural case (dead entry_id), but the threshold case (pair count dropped below 3 after cleanup) is not cleaned until #409.

---

## Edge Cases

**E-01: Single qualifying pair** — Only one row passes the threshold; max_count equals that row's count; normalized weight = 1.0. Assert that weight = 1.0 is stored correctly and passes the delta guard correctly on subsequent ticks (existing weight 1.0, new computed weight 1.0, delta = 0.0 <= 0.1, no UPDATE).

**E-02: All pairs at identical count** — e.g., 50 pairs all with count=5; max_count=5; all normalized weights = 1.0. The cap selects the first 200 (or configured limit) in undefined ORDER (since count is tied). ORDER BY must be stable — add a secondary sort (e.g., `entry_id_a ASC`) or accept that tie-breaking is arbitrary but deterministic within a single SQLite session.

**E-03: Cap exactly equals qualifying count** — 200 qualifying pairs, cap=200; all pairs processed in one tick. Confirm the tick correctly logs 200 inserted (or fewer if some already exist) without off-by-one errors.

**E-04: max_co_access_promotion_per_tick = 1** — minimum valid cap; one pair promoted per tick. Assert correct behavior: highest-count pair is selected, promoted, and subsequent ticks promote the next-highest until convergence.

**E-05: Weight drift exactly at delta boundary** — existing edge weight = 0.5, new computed weight = 0.6 (delta = 0.1 exactly). Boundary behavior: `|new - existing| > delta` (strictly greater) means delta=0.1 exactly is NOT updated. Test this boundary explicitly to prevent off-by-one direction errors.

**E-06: entry_id_a == entry_id_b** — the `co_access` table convention requires `entry_id_a < entry_id_b`, but there is no DB-level constraint. If a self-loop pair exists (a, a), the promoted edge has source_id == target_id. PPR behavior on self-loops is undefined. The tick should not crash, and the edge should be inserted or ignored normally. This is an upstream data quality issue, but the tick must not panic.

---

## Security Risks

**S-01: No external input accepted by this tick** — `run_co_access_promotion_tick` reads from `co_access` (internal table, written by the co-access recording pipeline) and writes to `graph_edges`. Both tables are internal SQLite tables, not exposed to MCP callers. There is no untrusted external input entering this feature.

**S-02: Config field injection** — `max_co_access_promotion_per_tick` is read from TOML config. An operator with filesystem access could set this to 10000 (max valid), causing the tick to process a large batch. This is bounded by the validation range [1, 10000] and is within the operator's privilege scope. No SQL injection risk: the value is used as a LIMIT parameter bound via sqlx parameterized query.

**S-03: SQL parameter binding** — The batch query uses `?1` and `?2` positional parameters via sqlx. `CO_ACCESS_GRAPH_MIN_COUNT` (i64) and the cap (usize) are typed values, not string interpolation. No injection surface.

**Blast radius if compromised**: The promotion tick writes only to `GRAPH_EDGES`. Worst-case data corruption is the PPR graph gaining incorrect CoAccess edge weights, degrading retrieval ranking. No entry content, no user data, no auth state is accessible from this tick.

---

## Failure Modes

**FM-01: All writes time out** — Under severe write pool contention, every INSERT and UPDATE in the batch times out. The tick logs N `warn!` messages (one per pair), emits an `info!` log with "0 inserted, 0 updated", and returns `()`. The caller (background.rs tick loop) continues normally. No pairs are promoted this tick; they are retried on the next tick. This is correct behavior per the infallible tick contract.

**FM-02: Batch fetch fails** — If the SELECT query itself fails (e.g., connection error), the function should log at `warn!` and return early as a clean no-op. The info! log should reflect 0 inserted / 0 updated. The function must NOT panic.

**FM-03: max_count NULL from subquery** — If the scalar subquery returns NULL (no rows match the WHERE filter), the outer query also returns no rows (the WHERE is the same predicate). Rust's `fetch_all` returns an empty Vec. The tick is a no-op. No division by zero occurs because the per-pair loop never executes. This must be confirmed by test (R-02).

**FM-04: Server restart mid-promotion** — The tick is not transactional across pairs. A server crash mid-batch leaves some pairs promoted and others not. On restart, `current_tick` resets to 0 and the SR-05 warn! window re-opens. The next tick re-evaluates all qualifying pairs; already-promoted pairs hit the INSERT OR IGNORE no-op path. Correctness is preserved. The SR-05 warn! fires for ticks 0–4 post-restart even if all pairs were previously promoted — this is an acceptable false-positive.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Med: global MAX query round-trip) | R-03 | Resolved by ADR-001: MAX embedded as scalar subquery in the single batch fetch. One SQL round-trip. Test scenario R-03.2 validates the subquery computes global max even outside the capped batch. |
| SR-02 (Med: INSERT OR IGNORE + conditional UPDATE doubles write pool round-trips) | R-01, R-04 | Accepted per ADR-001: UPSERT rejected (semantic mismatch with delta guard). Per-pair contention is absorbed by infallible contract. R-01 validates warn! emission and continuation; R-04 validates correct no-op detection. |
| SR-03 (Low: near-threshold pair oscillation, potential churn) | R-09 | Mitigated by INSERT OR IGNORE no-op + delta guard. AC-14 and AC-15 tests cover idempotency. R-09 scenarios confirm zero spurious writes. |
| SR-04 (Low: one-directional edge directionality must be documented) | R-10 | Resolved by ADR-006: v1 matches bootstrap (source=entry_id_a, target=entry_id_b). R-10 scenario confirms no reverse edge is written. |
| SR-05 (High: GH #409 race causes silent signal loss) | R-06, I-01 | Mitigated by ADR-005: warn! on qualifying_count==0 for current_tick < PROMOTION_EARLY_RUN_WARN_TICKS. R-06 tests the four quadrants of the warn! condition. I-01 notes the deployment-level guard required. |
| SR-06 (Low: future tick steps could displace insertion point) | R-05 | Mitigated by ADR-005: anchor comment with ORDERING INVARIANT block in background.rs. R-05 covers static verification of call site position (AC-05). |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 3 scenarios (write failure mid-batch, warn! emission, info! always fires) |
| High | 7 (R-02, R-03, R-04, R-05, R-06, R-07, R-11, R-13) | 18 scenarios across the group |
| Med | 4 (R-08, R-09, R-10, R-03 partial) | 7 scenarios |
| Low | 2 (R-09 partial, R-12) | 3 scenarios (AC-14, AC-15, wc-l gate check) |

**Mandatory tests (must pass at Gate 3c)**:
- AC-01: basic promotion (new qualifying pair → GRAPH_EDGES row)
- AC-02: weight refresh on drift > delta
- AC-03: no UPDATE when drift <= delta
- AC-04: cap + ORDER BY count DESC (highest-count pairs selected)
- AC-07: CO_ACCESS_GRAPH_MIN_COUNT constant exists and equals 3
- AC-08: EDGE_SOURCE_CO_ACCESS constant exists and equals "co_access"
- AC-09: empty/sub-threshold table → clean no-op
- AC-10: validation rejects max_co_access_promotion_per_tick=0
- AC-11: write failure → warn!, tick continues, returns ()
- AC-12: inserted row metadata fields (all four)
- AC-13: global MAX normalization (not batch-local)
- AC-14: double-tick idempotency (no duplicate rows)
- AC-15: sub-threshold pair not GC'd by this tick

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection background tick" — found #3579 (gate-3b missing tests), #3580 (500-line limit violations), #3723 (tick completion log without distribution). Applied: R-12 (file size risk) and R-01 (infallible tick observability risk elevated to Critical).
- Queried: `/uni-knowledge-search` for "risk pattern recurring tick GRAPH_EDGES promotion" — found #3821 (GRAPH_EDGES tick write path pattern), #3822 (near-threshold idempotency pattern), #1616 (background tick dedup/ordering). Applied: R-09 (near-threshold oscillation), R-05 (ordering), R-04 (no-op detection).
- Queried: `/uni-knowledge-search` for "SQLite migration co_access weight normalization" — found #2428 (window function weight normalization with empty-table guard, R-06 pattern). Applied: R-02 (max_count=0 guard), R-03 (normalization correctness).
- Stored: nothing novel to store — existing patterns #3821 and #3822 already cover the primary recurring-tick and near-threshold patterns for this domain. The SR-05 warn!/restart false-positive risk (R-06) is specific to the tick-counter-reset interaction and not yet generalized across 2+ features.
