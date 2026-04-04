# Risk-Based Test Strategy: crt-046 — Behavioral Signal Delivery

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Memoisation gate bypasses step 8b — `force=false` cache-hit early-return placed after step 8b insertion point, so behavioral edges and goal_cluster are never emitted on repeat calls | High | Med | Critical |
| R-02 | `write_graph_edge` return contract misused — emission counters keyed off `Ok(_)` instead of `rows_affected() > 0 == true`, inflating edge counts on UNIQUE conflicts (pattern #4041) | High | High | Critical |
| R-03 | Analytics drain shedding policy drops behavioral edges — `bootstrap_only=false` path accidentally subject to shed policy, producing silent edge loss under queue pressure | High | Med | Critical |
| R-04 | Silent observation parse failures — malformed `input` JSON silently drops entry IDs; `parse_failure_count` not returned in review result, making drops invisible to callers (SR-01) | High | Med | Critical |
| R-05 | Schema migration v21→v22 cascade incomplete — any of the 9 cascade sites from entry #3894 missed causes test failures or runtime assertion panics in `server.rs` | High | High | Critical |
| R-06 | Goal-cluster INSERT OR IGNORE silent partial-record persistence — if step 8b fails mid-way (goal_embedding stored, entry_ids incomplete), the partial row persists and subsequent force=true re-runs are no-ops (ADR-002 Harder consequence) | Med | Med | High |
| R-07 | Recency cap not enforced — `query_goal_clusters_by_embedding` scans full table instead of last 100 rows, causing O(N×D) latency cliff as table grows (SR-09) | Med | Med | High |
| R-08 | Briefing NULL short-circuit fires too late — `session_state.feature.is_none()` guard absent; `get_cycle_start_goal_embedding` called even when feature is absent, producing a meaningless DB query per briefing (SR-07, ADR-004) | Med | Med | High |
| R-09 | Pair cap enforced after iteration instead of before — O(N²) pair explosion for cycles with many `context_get` calls; cap check inside loop body rather than truncating the input set (NFR-04) | Med | Low | High |
| R-10 | Bidirectional edge emission misses one direction — only `A→B` enqueued, not `B→A`; graph traversal (crt-045 PPR) then fails to traverse from B to A | Med | Med | High |
| R-11 | Cold-start regressions — any NULL goal / empty goal_clusters / below-threshold match path alters existing semantic briefing output instead of falling through unchanged (NFR-02) | Med | Low | High |
| R-12 | Inactive entry leakage — cluster-derived IDs not filtered through `store.get_by_ids()` Active check; deprecated or quarantined entries appear in briefing results (AC-10) | Med | Low | High |
| R-13 | **RESOLVED (ADR-005)** — Zero-remaining-slot suppression eliminated by Option A score-based interleaving. Cluster entries compete on the same ranked list as semantic results and displace the weakest when cluster_score exceeds them. Risk no longer applies. | — | — | — |
| R-14 | `spawn_blocking` violation for sqlx — new store methods (`get_cycle_start_goal_embedding`, `insert_goal_cluster`, `query_goal_clusters_by_embedding`) called from `spawn_blocking`, violating ADR (entries #2266, #2249) | Med | Low | Med |
| R-15 | `goal_clusters` DDL mismatch between migration.rs and db.rs — byte-non-identical table definitions cause schema parity test failures or create_tables_if_needed divergence (ARCHITECTURE §Migration) | Med | Low | Med |
| R-16 | Outcome weight boundary — `outcome_to_weight` treats every non-"success" string (including rework indicators and None) as 0.5; a new outcome string added in future silently gets 0.5 rather than flagging as unknown | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Memoisation Gate Bypasses Step 8b

**Severity**: High
**Likelihood**: Med
**Impact**: Behavioral edges and goal_clusters row never written on any `force=false` call after the first; only the cold-code path (cache miss or force=true) ever exercises step 8b.

**Test Scenarios**:
1. Call `context_cycle_review` once (primes memoisation). Call again with `force=false`. Assert `graph_edges` count for `source='behavioral'` is identical after both calls — confirming idempotent re-emission ran, not that step 8b was bypassed.
2. Assert in code review / structural test that the memoisation early-return branch in `context_cycle_review` appears *after* the step 8b call site, not before (AC-15).
3. Integration test: seed two `context_get` observations; first call produces N behavioral edges; second `force=false` call produces same N (no new rows, no zero rows — confirms step 8b ran idempotently).

**Coverage Requirement**: AC-15 must have a dedicated integration test that calls twice and asserts row count stability. A static lint or file-order check alone is insufficient.

---

### R-02: write_graph_edge Return Contract Misuse

**Severity**: High
**Likelihood**: High
**Impact**: `edges_enqueued` counter over-reports when UNIQUE conflicts exist (NLI already owns the edge); misleads any future diagnostic logging. Root cause of crt-040 Gate 3a rework (entry #4041).

**Test Scenarios**:
1. Unit test `emit_behavioral_edges`: seed one existing `Informs` edge for pair (A,B) via NLI; emit the same pair as behavioral; assert `edges_enqueued == 0` (both directions conflicted), `pairs_skipped_on_conflict == 1`.
2. Unit test: emit a pair with no prior edge; assert `edges_enqueued == 2` (both directions new).
3. Code review gate: pseudocode for `emit_behavioral_edges` must lead with the three-case contract table from pattern #4041 before any implementation prose.

**Coverage Requirement**: At least one unit test must exercise the UNIQUE-conflict path and assert the counter is NOT incremented.

---

### R-03: Analytics Drain Shedding Policy Drops Behavioral Edges

**Severity**: High
**Likelihood**: Med
**Impact**: `bootstrap_only=false` edges shed under queue pressure without surfacing in test runs; behavioral signal silently absent from `graph_edges` after high-volume cycle reviews.

**Test Scenarios**:
1. Integration test: enqueue >1000 `GraphEdge` writes simultaneously (saturating the bounded mpsc channel of capacity 1000 per entry #2148); assert `bootstrap_only=false` behavioral edges are NOT in the shed_counter increment path.
2. Verify via code inspection that the drain task's shed logic branches on `bootstrap_only` or queue-full condition — behavioral edges must not be shed when `bootstrap_only=false`.
3. Integration test: confirm that after a cycle review with 200 pairs (400 directed edges) all 400 appear in `graph_edges` after drain flush.

**Coverage Requirement**: Explicitly exercise near-capacity queue to confirm behavioral edges are not shed. Code inspection alone is insufficient.

---

### R-04: Silent Parse Failures Not Returned in Review Result

**Severity**: High
**Likelihood**: Med
**Impact**: Callers cannot distinguish "no co-access pairs" from "all observations had malformed input JSON"; debugging requires server log access (SR-01).

**Test Scenarios**:
1. AC-13: Seed one malformed observation row (missing `id` field) alongside two valid rows. Call `context_cycle_review`. Assert returned result contains `parse_failure_count >= 1`. Assert valid rows still produce behavioral edges (partial recovery confirmed).
2. All-valid scenario: assert `parse_failure_count == 0` in result (not absent, not null).
3. All-malformed scenario: assert `parse_failure_count == N` and zero behavioral edges emitted — not an error returned to caller.

**Coverage Requirement**: FR-03 requires `parse_failure_count` surfaced in the MCP response, not only server logs. AC-13 is a non-negotiable test. Test must inspect the actual returned payload, not only side effects.

---

### R-05: Schema Migration v21→v22 Cascade Incomplete

**Severity**: High
**Likelihood**: High
**Impact**: Runtime assertion panic in `server.rs` on startup; `sqlite_parity` test failures; `grep -r 'schema_version.*== 21' crates/` returning non-zero matches at Gate 3a (AC-17).

**Test Scenarios**:
1. AC-12: Migration integration test — open a v21 fixture DB, run migration, assert `read_schema_version(&store) == 22` and `goal_clusters` table exists with column count == 7.
2. AC-17: Gate 3a checklist item — `grep -r 'schema_version.*== 21' crates/` returns zero matches.
3. `sqlite_parity.rs` must contain `test_create_tables_goal_clusters_exists` and `test_create_tables_goal_clusters_schema` (exact column count 7).
4. `server.rs` version assertions at both sites updated from 21 to 22.
5. Prior migration test renamed to `test_current_schema_version_is_at_least_21` with `>= 21` predicate.
6. `db.rs` hardcoded schema_version INSERT integer updated; `test_schema_version_initialized_to_21_on_fresh_db` renamed to `_22`.

**Coverage Requirement**: All 9 cascade sites from entry #3894 (as enumerated in ARCHITECTURE.md §Migration cascade checklist) must be addressed. Gate 3a is a FAIL if any site is missing.

---

### R-06: Goal-Cluster Partial-Record Persistence on Mid-Step Failure

**Severity**: Med
**Likelihood**: Med
**Impact**: A partial `goal_clusters` row (goal_embedding present, entry_ids incomplete due to a step-8b failure) persists permanently because INSERT OR IGNORE means no re-run can overwrite it (ADR-002 Harder consequence).

**Test Scenarios**:
1. Integration test: simulate a store failure during observation loading (after `get_cycle_start_goal_embedding` succeeds but before `insert_goal_cluster` is called); verify `goal_clusters` has zero rows (no partial row).
2. Confirm via unit test that `populate_goal_cluster` is called only after `entry_ids` is fully assembled — not speculatively mid-collection.
3. Test that `insert_goal_cluster` returns `Ok(false)` on a second call for the same `feature_cycle`, and does not return an error.

**Coverage Requirement**: Step 8b error sequence must be tested — confirm that partial failure leaves `goal_clusters` empty (not partial), because the insert is the final step of population.

---

### R-07: Recency Cap Not Enforced on Cosine Scan

**Severity**: Med
**Likelihood**: Med
**Impact**: `goal_clusters` grows unbounded; cosine scan becomes O(N×D) latency cliff arriving silently after many cycles (SR-09, ADR-003).

**Test Scenarios**:
1. AC-11: Seed 101 rows in `goal_clusters` where the 101st (oldest by `created_at`) has the highest cosine similarity to the test goal embedding. Assert that the oldest row's entry IDs do NOT appear in briefing output.
2. Unit test `query_goal_clusters_by_embedding`: seed 150 rows; assert result set contains at most 100 entries total (before threshold filtering).
3. Inspect SQL in `query_goal_clusters_by_embedding` — must contain `ORDER BY created_at DESC LIMIT 100`.

**Coverage Requirement**: AC-11 is the primary scenario. Must seed exactly 101 rows (not 100) to exercise the boundary condition. Verifies that row 101 is excluded even when it is the best semantic match.

---

### R-08: Briefing NULL Short-Circuit Fires Too Late

**Severity**: Med
**Likelihood**: Med
**Impact**: Sessions without feature attribution or without a stored goal pay unnecessary DB query cost per briefing call; cold-start sessions slower than pre-crt-046 baseline (SR-07, ADR-004).

**Test Scenarios**:
1. AC-16: Unit test with mock store — call briefing path with `session_state.feature = None`; assert `get_cycle_start_goal_embedding` is NOT called (zero store interactions).
2. Unit test: `session_state.feature = Some("crt-046")` but `get_cycle_start_goal_embedding` returns `Ok(None)`; assert `query_goal_clusters_by_embedding` is NOT called.
3. Integration test: briefing with `feature = None` produces result identical to pure-semantic baseline.

**Coverage Requirement**: Both guard levels (absent feature, NULL embedding) must each have a test confirming the next DB call is not issued. Mock store with call-count assertion is required for AC-16.

---

### R-09: Pair Cap Enforced After Iteration Instead of Before

**Severity**: Med
**Likelihood**: Low
**Impact**: O(N²) pair generation runs to completion before cap truncation; 1000 `context_get` calls produce ~500K pairs in memory before cap applies (NFR-04).

**Test Scenarios**:
1. AC-14: Seed a session with 21 distinct `context_get` observations (produces 210 pairs). Assert emitted edge count ≤ 400 (200 pairs × 2 directions). Assert warning log contains "pair cap" or equivalent.
2. Unit test `build_coaccess_pairs`: pass 25 distinct entry IDs (produces 300 pairs); assert returned `Vec` length ≤ 200 and `cap_hit == true`.
3. Verify `build_coaccess_pairs` halts pair enumeration when `pairs.len() == 200` — does not generate all pairs then truncate.

**Coverage Requirement**: AC-14 is the integration scenario. Unit test must confirm cap is enforced at enumeration time by checking return length when input would produce > 200 pairs.

---

### R-10: Bidirectional Edge Emission — One Direction Missing

**Severity**: Med
**Likelihood**: Med
**Impact**: Graph traversal (PPR, crt-045) cannot traverse from B to A if only A→B is emitted; behavioral signal is asymmetric and only half-effective.

**Test Scenarios**:
1. AC-01 extension: after `context_cycle_review` with pair (A, B), query `graph_edges WHERE source='behavioral'`; assert rows exist for BOTH `(source_id=A, target_id=B)` AND `(source_id=B, target_id=A)`.
2. Unit test `emit_behavioral_edges`: single pair input; assert `enqueue_analytics` called exactly twice with swapped source/target.
3. Multi-pair test: N pairs → 2N enqueue calls.

**Coverage Requirement**: Every edge emission test must assert both directions. A test that only checks `COUNT(*) >= 1` is insufficient.

---

### R-11: Cold-Start Regressions

**Severity**: Med
**Likelihood**: Low
**Impact**: Existing semantic briefing quality degraded for all pre-v22 sessions, sessions without goals, or fresh deployments; breaks NFR-02 bit-for-bit equivalence guarantee.

**Test Scenarios**:
1. AC-08: `session_state.current_goal` populated but `get_cycle_start_goal_embedding` returns NULL; assert result set is bit-for-bit identical to pure-semantic baseline (no blending artifacts).
2. AC-09: `goal_clusters` table empty; call briefing with a goal; assert output equals pure-semantic baseline.
3. Regression test: cosine similarity < 0.80 (below threshold); assert no cluster entries injected, result unchanged from semantic-only.
4. Regression test: `session_state.feature = None`; result unchanged.

**Coverage Requirement**: All four cold-start paths must have explicit tests (absent feature, NULL embedding, empty table, below-threshold). "Identical to baseline" means same IDs in same order — not just same count.

---

### R-12: Inactive Entry Leakage into Briefing Results

**Severity**: Med
**Likelihood**: Low
**Impact**: Deprecated or quarantined entries surfaced to agents via goal-conditioned blending; undermines knowledge quality guarantees.

**Test Scenarios**:
1. AC-10: Seed a `goal_clusters` row whose `entry_ids_json` includes a deprecated entry ID. Call briefing with matching goal. Assert deprecated entry does NOT appear in output.
2. Test with quarantined entry ID — same assertion.
3. Positive test: Active entry in cluster IDs DOES appear in output when slots remain.

**Coverage Requirement**: Must test both deprecated and quarantined status explicitly. Positive case required to confirm filtering doesn't accidentally exclude all cluster entries.

---

### R-13: Zero-Remaining-Slot Cluster Suppression — RESOLVED (ADR-005)

**Resolution**: Eliminated by Option A score-based interleaving (ADR-005). FR-21 updated: cluster entries are scored via `cluster_score = (entry.confidence × w_conf_cluster) + (goal_cosine × w_goal_boost)`, merged with semantic results into one ranked list, and compete for top-k=20 positions. A cluster entry displaces a semantic result only when its score exceeds that result's score. The blending path is not inert regardless of how many entries are in the knowledge base. No test scenario required.

**Test Scenarios**:
1. Seed a briefing scenario where semantic search returns exactly k=20 results. Seed a matching `goal_clusters` row with additional entry IDs. Assert output contains exactly k=20 entries and cluster-only IDs are absent — confirms spec-compliant suppression, not a bug.
2. Verify there is no error or warning emitted when this suppression occurs (silent per spec).

**Coverage Requirement**: One scenario test to document the accepted behavior and prevent future "fixing" of intentional suppression. Mark test with a comment citing FR-21 and SR-08.

---

### R-14: spawn_blocking Violation for sqlx

**Severity**: Med
**Likelihood**: Low
**Impact**: Deadlock or tokio runtime starvation; violates ADR (entries #2266, #2249).

**Test Scenarios**:
1. Code inspection: all three new store methods must be `async fn` called with `.await` from async handler context — no `spawn_blocking` wrapping.
2. Integration test: concurrent briefing + cycle_review calls; assert no deadlock or timeout (pool exhaustion).

**Coverage Requirement**: Static/structural verification via code review. Runtime concurrency test as defense-in-depth.

---

### R-15: goal_clusters DDL Mismatch Between migration.rs and db.rs

**Severity**: Med
**Likelihood**: Low
**Impact**: `sqlite_parity` test fails; `create_tables_if_needed` creates a different schema than migration runs, causing runtime column-not-found errors on fresh deployments.

**Test Scenarios**:
1. `sqlite_parity.rs` test `test_create_tables_goal_clusters_schema` asserts column count == 7 on the result of `create_tables_if_needed`.
2. Migration integration test asserts column count == 7 after running the v22 migration block.
3. Gate 3a checklist: the two DDL blocks must be textually diffed as part of PR review.

**Coverage Requirement**: Both `create_tables_if_needed` and migration paths must produce a 7-column `goal_clusters` table, each verified by a distinct test.

---

### R-16: Outcome Weight Boundary (Low Priority)

**Severity**: Low
**Likelihood**: Med
**Impact**: Future outcome strings silently assigned 0.5 weight without a warning; may silently miscalibrate edge weights.

**Test Scenarios**:
1. Unit test `outcome_to_weight`: assert `"success"` → 1.0, `None` → 0.5, `"rework"` → 0.5, an arbitrary unknown string → 0.5 (documents the exhaustive-match behavior explicitly).
2. No change required to implementation — test documents accepted behavior.

**Coverage Requirement**: One table-driven unit test covering all four cases. Exists primarily to prevent accidental breakage of the two-value contract.

---

## Integration Risks

**I-01: step 8b position relative to audit log (step 11)**
Step 8b must execute after `store_cycle_review` (step 8a) and before the audit log fires (step 11). If the call order is wrong, edges are emitted for a cycle whose review record hasn't been committed — potential inconsistency between review state and graph state. Test: verify that `graph_edges` are not present when store_cycle_review fails (step 8a fails → step 8b must not run).

**I-02: Analytics drain flush timing in integration tests**
`enqueue_analytics` is fire-and-forget with a 500ms flush interval (entry #2148). Integration tests that query `graph_edges` immediately after `context_cycle_review` may fail intermittently unless they force a drain flush or wait for the drain interval. All AC tests involving `graph_edges` must flush the drain before asserting.

**I-03: `load_observations_for_sessions` returns non-`context_get` rows**
The filter `tool = "context_get"` must be applied inside `collect_coaccess_entry_ids`, not assumed at the call site. If an upstream refactor stops filtering in `load_observations_for_sessions`, behavioral edge extraction must still be correct.

**I-04: `session_state.feature` vs `session_state.current_goal` independence at briefing time**
FR-16 requires both `feature` (for embedding lookup) and `current_goal` (to trigger the path at all). If feature is Some but current_goal is empty, the embedding lookup is wasted. Test: confirm that an empty `current_goal` activates the cold-start path before the DB call.

**I-05: `get_cycle_start_goal_embedding` reuse at both step 8b and briefing time**
The same store method is called from two sites. A subtle bug (e.g., incorrect `event_type` filter) affects both paths simultaneously. Both paths need independent integration tests rather than sharing fixtures.

---

## Edge Cases

**E-01: Single `context_get` observation in a session** — no pair possible; zero edges emitted. (AC-04 variant.)

**E-02: All `context_get` observations are for the same entry ID** — self-pair (A, A): canonical form `(A, A)`. Emit or skip? Spec is silent. Risk: self-loop graph edges. Recommendation: skip `(A, A)` pairs; test asserts self-pairs are excluded.

**E-03: `goal_embedding` BLOB decode failure** — `decode_goal_embedding` returns `Err`; treat as NULL (cold-start path). Test: seed a malformed BLOB in `cycle_events`; assert briefing returns pure-semantic result without panic.

**E-04: `entry_ids_json` contains duplicate entry IDs** — deduplication must occur before emitting pairs. Test: seed two identical `context_get` observations for the same entry in the same session; assert only one ID counted (not a pair with itself).

**E-05: `goal_clusters` row with empty `entry_ids_json` array `[]`** — blend step produces zero cluster entries. Test: seed a cluster row with `entry_ids_json = "[]"`; assert no crash and zero cluster entries injected.

**E-06: `feature_cycle` string with special characters** — e.g. `"crt-046/sub"`. Must survive SQL UNIQUE constraint and JSON serialization without escaping issues. Test with a slash-containing feature_cycle value.

**E-07: Cosine similarity exactly at threshold (0.80)** — must be included (≥ not >). Test: seed cluster with similarity == 0.80 to test embedding; assert it appears in results.

**E-08: `context_briefing` called with `feature` pointing to a cycle with no `cycle_start` event** — `get_cycle_start_goal_embedding` returns `Ok(None)` (no matching event_type row); cold-start path activates. Distinct from NULL embedding.

---

## Security Risks

**S-01: Observation `input` JSON injection**
The `input` field in `observations` is stored as raw TEXT from the MCP tool call. `collect_coaccess_entry_ids` parses it to extract `"id"`. A malicious or malformed input could contain SQL-unsafe content, but since the parsed value is a `u64` integer (not interpolated into SQL), there is no injection risk after parsing. However, a crafted JSON payload that passes `u64` parsing but contains an entry ID not owned by the session could pollute behavioral edges with fabricated co-access pairs.
- Blast radius: `graph_edges` contains spurious `Informs` edges for entry IDs the session never actually accessed.
- Mitigation: Entry ID validation (assert the extracted ID exists in `entries`) would close this gap, but is not currently specified. Risk is low given the internal deployment model.

**S-02: `goal_embedding` BLOB deserialization**
`decode_goal_embedding` uses bincode deserialization on data stored in `cycle_events` by the system itself. No external input path writes to `goal_embedding` directly — it is populated by a fire-and-forget spawn from `handle_cycle_event` using the embed service output. Deserialization of attacker-controlled data is not possible in the current architecture. Risk: Low.

**S-03: `entry_ids_json` stored in `goal_clusters` is user-influenced**
The JSON array is constructed from observation data, which originates from MCP tool call inputs. If an attacker can inject arbitrary `context_get` calls with crafted `id` values, those IDs appear in `entry_ids_json` and are later injected into briefing results via blending. The `store.get_by_ids()` Active filter provides a partial mitigation (only Active entries surface), but the `goal_clusters` row itself contains the unvalidated IDs.
- Blast radius: briefing results can be influenced by crafted historical observations. Limited to Active entries only.
- No mitigation required given the internal model, but the risk should be documented for future external-facing deployments.

---

## Failure Modes

**F-01: Step 8b failure (store error, panic)**
All step 8b errors are non-fatal per ARCHITECTURE §Error Handling. `context_cycle_review` must return the review result even if all of step 8b fails. Test: inject a store error into `emit_behavioral_edges`; assert the handler returns a successful response with the review record.

**F-02: `get_cycle_start_goal_embedding` DB error at briefing time**
Falls through to pure semantic retrieval (cold-start path). No error propagated to caller. Test: mock store returning `Err` from this method; assert briefing returns a non-error result identical to pure-semantic output.

**F-03: Analytics drain not running (drain task crashed)**
Behavioral edges enqueued but never written to `graph_edges`. The drain task crashing is outside the scope of crt-046, but the shed counter should increment and surface via `context_status`. No direct crt-046 test required; existing analytics drain health tests cover this.

**F-04: `goal_clusters` table not yet migrated (v21 DB accessing v22 code)**
Direct `write_pool_server()` call to insert into `goal_clusters` will fail with "table not found". Step 8b is non-fatal — error is logged, no crash. Migration gate (AC-12) prevents this in production. Test: confirm `insert_goal_cluster` error is logged at `warn!` and does not propagate to the response.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Test Scenario | Resolution |
|-----------|------------------|---------------|------------|
| SR-01 — Silent parse-failure drops | R-04 | AC-13: seed malformed row; assert `parse_failure_count` in response | FR-03 adds `parse_failure_count` to review result; ARCHITECTURE §step 8b logs at `warn!` |
| SR-02 — NLI edge weight asymmetry | R-02 (partial) | R-02 scenario 1: existing NLI edge → assert `edges_enqueued == 0` | Accepted per roadmap spec; INSERT OR IGNORE semantics; documented in ARCHITECTURE |
| SR-03 — `write_graph_edge` return contract | R-02 | R-02 scenarios 1–3; code review gate | ARCHITECTURE §write_graph_edge Return Contract table leads pseudocode |
| SR-04 — INSERT OR IGNORE vs INSERT OR REPLACE contradiction | — | AC-02, AC-15: idempotency tests | Resolved: INSERT OR IGNORE throughout (ADR-002); contradiction eliminated |
| SR-05 — Schema version cascade 7-touchpoint | R-05 | AC-12 (migration test), AC-17 (grep clean) | ARCHITECTURE §Migration cascade checklist enumerates all 9 sites |
| SR-06 — Pair cap warning surface | — | AC-14: warning in server logs confirmed | Resolved: cap-hit warning is server-log only; `parse_failure_count` is the only review-result addition |
| SR-07 — NULL short-circuit must fire before any DB query | R-08 | AC-16: mock store; assert no cluster query when embedding NULL | ADR-004 Guard B: two-level short-circuit; feature-absent guard fires first |
| SR-08 — Zero remaining slots for cluster entries | R-13 | AC-07: cluster entry displaces weakest semantic result | RESOLVED by ADR-005 (Option A): score-based interleaving; R-13 eliminated |
| SR-09 — Unbounded `goal_clusters` scan latency | R-07 | AC-11: 101-row recency boundary test | ADR-003: `ORDER BY created_at DESC LIMIT 100`; `idx_goal_clusters_created_at` index |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01–R-05) | 15 scenarios (AC-13, AC-15, AC-17, R-02 contract test, drain flush test, migration cascade) |
| High | 7 (R-06–R-12) | 18 scenarios (AC-10, AC-11, AC-14, AC-16, bidirectional emit, cold-start battery) |
| Med | 2 (R-14–R-15) | 4 scenarios (DDL parity, concurrency test) — R-13 resolved, no scenario required |
| Low | 1 (R-16) | 1 scenario (outcome_to_weight table-driven unit test) |

**Non-negotiable tests** (gate blockers):
- AC-13 — parse_failure_count in response (R-04)
- AC-15 — force=false step 8b re-emission (R-01)
- AC-11 — recency cap 101-row boundary (R-07)
- AC-17 — grep clean on `schema_version.*== 21` (R-05)
- R-02 contract test — UNIQUE-conflict path does NOT increment edges_enqueued counter (R-02)
- Drain flush before graph_edges assertion in all integration tests (R-03 / I-02)
