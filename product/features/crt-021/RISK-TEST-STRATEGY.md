# Risk-Based Test Strategy: crt-021 (W1-1 Typed Relationship Graph)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `graph_penalty` behavioral regression — TypedRelationGraph with Supersedes-only filter produces wrong penalty for one or more of the 25+ existing test cases | High | Med | Critical |
| R-02 | `edges_of_type` filter boundary bypassed — a call site in `graph_penalty`, `find_terminal_active`, or their private helpers calls `.edges_directed()` directly instead of `edges_of_type`, allowing non-Supersedes edges to contaminate penalty traversal | High | Med | Critical |
| R-03 | `bootstrap_only=1` edges reach `graph_penalty` — structural exclusion in `build_typed_relation_graph` is absent or conditional, allowing heuristic edges to penalize valid entries | High | Med | Critical |
| R-04 | Tick sequencing violated — GRAPH_EDGES compaction and/or VECTOR_MAP compaction races with `TypedGraphState::rebuild`, producing an in-memory graph built on stale (pre-compaction) edge data | High | Low | High |
| R-05 | Cold-start regression — `TypedGraphState::new()` does not set `use_fallback=true`, causing `graph_penalty` to be called on an empty graph before first tick, returning 0.0 instead of `FALLBACK_PENALTY` | High | Low | High |
| R-06 | CoAccess weight normalization NULL — `MAX(count)` is NULL when `co_access` is empty, violating `weight REAL NOT NULL` constraint and aborting the v12→v13 migration on fresh databases | High | High | Critical |
| R-07 | `weight: f32` NaN/Inf propagation — a non-finite weight reaches `INSERT INTO graph_edges`, is persisted, and later loaded into `TypedRelationGraph` where it silently corrupts search re-ranking | High | Low | High |
| R-08 | v12→v13 migration not idempotent — running the migration twice on the same database inserts duplicate edges (UNIQUE constraint missing or not enforced) | Med | Low | Med |
| R-09 | `sqlx-data.json` stale after schema change — compile-time SQL validation silently disabled for all new GRAPH_EDGES queries; SQL errors surface only at runtime | Med | High | High |
| R-10 | `RelationType` string deserialization silent failure — an unknown string in `GRAPH_EDGES.relation_type` (e.g., typo, future variant from a newer schema) causes `from_str` to return `None` and the edge to be silently dropped from the in-memory graph | Med | Med | Med |
| R-11 | Orphaned-edge compaction cost regression — unbounded DELETE from graph_edges with NOT IN subquery against `entries` exceeds TICK_TIMEOUT on large graphs (entry #1777 precedent: compute_report() tick inflation) | Med | Med | High |
| R-12 | Supersedes edges sourced from `GRAPH_EDGES` vs. `entries.supersedes` diverge — if `build_typed_relation_graph` accidentally uses GRAPH_EDGES Supersedes rows as sole source (instead of entries.supersedes for node construction), cycle detection operates on a graph that may be missing authoritative edges added since the last migration | Med | Low | Med |
| R-13 | `AnalyticsWrite::GraphEdge` shed silently drops bootstrap edges — bootstrap migration writes go through the analytics queue and are dropped at capacity 1000; fresh databases bootstrapped under write load may have incomplete graphs until next tick | Low | Low | Low |
| R-14 | `TypedGraphState` rename incomplete — one or more of the ~20 call sites retains `SupersessionState` or `SupersessionStateHandle`, masked by a type alias introduced to suppress a compile error | Med | Med | Med |
| R-15 | `GRAPH_EDGES` CoAccess weight formula — FR-09 specifies normalized `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)`; implementer must use this formula exactly; a flat `weight=1.0` implementation loses co-access signal strength expected by W3-1 GNN | Med | Med | Med |

---

## Risk-to-Scenario Mapping

### R-01: graph_penalty behavioral regression
**Severity**: High
**Likelihood**: Med
**Impact**: Incorrect confidence scoring for all searches touching superseded entries. Invisible regression — no error, just wrong rankings.

**Test Scenarios**:
1. Run all 25+ existing `graph.rs` unit tests (ORPHAN, DEAD_END, PARTIAL_SUPERSESSION, CLEAN_REPLACEMENT, hop-decay, cycle detection) against `TypedRelationGraph` with only Supersedes edges. All must produce values identical to the old `SupersessionGraph` results.
2. Build a `TypedRelationGraph` with mixed edge types (add Contradicts and CoAccess edges alongside Supersedes edges for the same node pairs). Assert `graph_penalty` returns the same value as if non-Supersedes edges were absent.
3. Unit test each of the 6 penalty priority cases explicitly against the typed graph to catch any dispatch-order regression.

**Coverage Requirement**: All 25+ existing graph.rs unit tests must pass without modification to expectations. One additional mixed-type regression test (scenario 2). Zero tolerance for deviation.

---

### R-02: edges_of_type filter boundary bypassed
**Severity**: High
**Likelihood**: Med
**Impact**: Non-Supersedes edges contaminate `graph_penalty` traversal, producing wrong penalties silently. The bug is structural and would affect every search result touching a node with CoAccess or Contradicts edges.

**Test Scenarios**:
1. Build a `TypedRelationGraph` where a node has a Contradicts edge to a non-superseded active entry. Assert `graph_penalty` does not return any penalty case that would be produced by traversing that edge.
2. Build a `TypedRelationGraph` where a node has a CoAccess edge forming a "chain" to an inactive entry. Assert `find_terminal_active` does not follow that edge.
3. Code review: assert that `graph_penalty`, `find_terminal_active`, `dfs_active_reachable`, and `bfs_chain_depth` contain no calls to `.edges_directed()` directly — only `edges_of_type`.

**Coverage Requirement**: At least one explicit test per traversal function (`graph_penalty`, `find_terminal_active`) with a polluted graph. No direct `.edges_directed()` calls at penalty call sites.

---

### R-03: bootstrap_only=1 edges reach graph_penalty
**Severity**: High
**Likelihood**: Med
**Impact**: Heuristic Contradicts edges (false positives from cosine similarity) penalize valid entries through any future scoring path operating on all edge types. Non-obvious because only W3-1 GNN would expose it, but the structural guarantee must be established now.

**Test Scenarios**:
1. Build a `GraphEdgeRow` list where a Supersedes edge has `bootstrap_only=true`. Call `build_typed_relation_graph`. Assert the resulting `TypedRelationGraph` has zero edges (the edge is structurally excluded).
2. Build a list with one `bootstrap_only=false` Supersedes edge and one `bootstrap_only=true` Supersedes edge for the same source node. Assert only the confirmed edge appears in the graph.
3. Assert `graph_penalty` applied to a node with only a `bootstrap_only=true` Supersedes edge in `GRAPH_EDGES` returns `FALLBACK_PENALTY` or the ORPHAN case (the node has no scored outgoing edges), not a chain-follow penalty.

**Coverage Requirement**: Explicit structural exclusion test in `build_typed_relation_graph`. One penalty regression test per AC-12.

---

### R-04: Tick sequencing violated
**Severity**: High
**Likelihood**: Low
**Impact**: TypedRelationGraph is rebuilt from stale (pre-compaction) data. Orphaned edges referencing deleted entries remain in the in-memory graph for one full tick cycle, potentially causing `graph_penalty` to traverse dangling node references.

**Test Scenarios**:
1. Integration test: insert an orphaned edge (source_id references a deleted entry). Trigger the tick sequence manually. Assert: (a) the orphaned row is deleted from `GRAPH_EDGES` before `TypedGraphState::rebuild` is called; (b) the rebuilt graph does not contain the orphaned edge.
2. Assert the tick sequence is strictly serial in `background.rs` — no `tokio::spawn` or `join!` across steps 2, 3, 4.

**Coverage Requirement**: One sequencing integration test. Code inspection of `background.rs` to confirm no concurrent dispatch.

---

### R-05: Cold-start regression
**Severity**: High
**Likelihood**: Low
**Impact**: On cold start, `graph_penalty` is called on an empty `TypedRelationGraph`, returns 0.0 (no penalty — every entry looks active), causing search results to include superseded entries before the first tick completes.

**Test Scenarios**:
1. Unit test: `TypedGraphState::new()` returns `use_fallback=true` with an empty `typed_graph` and empty `all_entries`. Assert `use_fallback=true`.
2. Unit test: search path logic given `use_fallback=true` returns `FALLBACK_PENALTY` without calling `build_typed_relation_graph` or `graph_penalty`.
3. Regression: the existing cold-start test for `SupersessionState` must pass for the renamed `TypedGraphState`.

**Coverage Requirement**: `new()` unit test + search path cold-start unit test. No performance change on initial startup.

---

### R-06: CoAccess weight normalization NULL
**Severity**: High
**Likelihood**: High
**Impact**: On any database where `co_access` is empty at migration time (new deployments, test environments), `MAX(count)` returns NULL and the INSERT violates `weight REAL NOT NULL`, aborting the entire v12→v13 migration. Database stays at schema version 12; server fails to start.

**Test Scenarios**:
1. Migration integration test on a synthetic v12 database with zero `co_access` rows. Assert migration completes without error, `schema_version=13`, zero CoAccess edges written.
2. Migration integration test with `co_access` rows all below threshold (count=2). Assert migration completes, zero CoAccess edges, no NULL weight written.
3. Assert the CoAccess bootstrap SQL uses `COALESCE(..., 1.0)` or equivalent NULL guard.

**Coverage Requirement**: Mandatory — this is a High likelihood failure that kills migrations on clean installs. Must be in the migration integration test suite.

---

### R-07: weight: f32 NaN/Inf propagation
**Severity**: High
**Likelihood**: Low
**Impact**: NaN weight in `GRAPH_EDGES` propagates into `TypedRelationGraph.RelationEdge.weight`. The weight field is not used by current `graph_penalty`, but is loaded into memory and available to W3-1 GNN. A NaN weight silently poisons the GNN feature vector.

**Test Scenarios**:
1. Unit test: pass `f32::NAN`, `f32::INFINITY`, `f32::NEG_INFINITY` to the weight validation guard. Assert all three are rejected with a logged error and the event is not enqueued.
2. Unit test: pass valid weights (0.0, 0.5, 1.0, `f32::MAX`) through the guard. Assert all pass.
3. Integration test: attempt to enqueue `AnalyticsWrite::GraphEdge` with `weight=f32::NAN`. Assert the drain task does NOT write a row to `graph_edges`.

**Coverage Requirement**: Validation unit test + drain task integration test. Both per AC-17.

---

### R-08: v12→v13 migration not idempotent
**Severity**: Med
**Likelihood**: Low
**Impact**: Running `migrate_if_needed` twice (possible in test environments or after a partial failure) inserts duplicate edges. Schema version counter is not bumped twice (migration check prevents this), but idempotency depends entirely on `INSERT OR IGNORE` + `UNIQUE` constraint.

**Test Scenarios**:
1. Run the v12→v13 migration twice on the same database. Assert row counts are identical after both runs. Assert no unique constraint violations.
2. Verify `CREATE TABLE IF NOT EXISTS` prevents DDL error on second run.

**Coverage Requirement**: One idempotency test in the migration test module.

---

### R-09: sqlx-data.json stale after schema change
**Severity**: Med
**Likelihood**: High
**Impact**: `cargo build` with `SQLX_OFFLINE=true` succeeds but all new GRAPH_EDGES queries bypass compile-time SQL validation. SQL syntax errors, missing columns, or wrong bind parameter counts in `query_graph_edges()` and the `GraphEdge` drain arm surface only at runtime.

**Test Scenarios**:
1. CI gate: `cargo build --workspace` with `SQLX_OFFLINE=true` must succeed after `sqlx-data.json` is committed. If it fails, the cache is stale.
2. Manual verification that `sqlx-data.json` was regenerated via `cargo sqlx prepare` and committed as part of the crt-021 PR.

**Coverage Requirement**: CI enforcement per AC-19. Blocking — do not merge without passing `SQLX_OFFLINE=true` build.

---

### R-10: RelationType string deserialization silent failure
**Severity**: Med
**Likelihood**: Med
**Impact**: An unrecognized `relation_type` string in `GRAPH_EDGES` (from a future schema extension, data corruption, or typo during a manual insert) causes `from_str` to return `None`. If edges are silently dropped rather than erroring, the in-memory graph is missing edges with no observable signal.

**Test Scenarios**:
1. Unit test: `RelationType::from_str("UnknownType")` returns `None`. Assert no panic.
2. Integration test: insert a `GRAPH_EDGES` row with `relation_type="UnknownType"`. Call `query_graph_edges()` + `build_typed_relation_graph`. Assert the function either returns an error or logs a warning and skips the unrecognized row (no panic, no silent misclassification).
3. Assert that unknown-type edges are counted/logged so the operator can detect schema skew.

**Coverage Requirement**: `from_str` unit test + deserialization behavior test. The silent drop case is the risk — the test must verify the fallback behavior is observable.

---

### R-11: Orphaned-edge compaction cost regression
**Severity**: Med
**Likelihood**: Med
**Impact**: An unbounded `DELETE FROM graph_edges WHERE source_id NOT IN (SELECT id FROM entries)` on a large graph inflates tick cost, potentially exceeding `TICK_TIMEOUT`. Precedent: entry #1777 (compute_report() tick inflation with 5 wasted phases).

**Test Scenarios**:
1. Measure tick duration in an integration test with a large synthetic `GRAPH_EDGES` table (e.g., 10,000 edges, 1,000 orphaned). Assert tick duration does not exceed a defined threshold (or compare to baseline tick duration without compaction).
2. If a per-tick batch limit is specified (e.g., LIMIT 500), test that a large orphaned set is processed across multiple ticks without blocking.

**Coverage Requirement**: Tick duration regression test or explicit accepted-cost documentation from the architect. Entry #1777 makes this a known risk category.

---

### R-12: Supersedes edge source divergence
**Severity**: Med
**Likelihood**: Low
**Impact**: If `build_typed_relation_graph` uses GRAPH_EDGES Supersedes rows as the source of truth instead of `entries.supersedes`, cycle detection may miss edges added to `entries.supersedes` after the last migration (before the next tick writes them to GRAPH_EDGES).

**Test Scenarios**:
1. Unit test: add an entry with a supersession to `all_entries` but provide no matching GRAPH_EDGES row for it. Assert the Supersedes edge is still present in the built graph (proving entries.supersedes is the authoritative source for Supersedes node construction).
2. Assert the architecture comment in `build_typed_relation_graph` documents which source takes priority.

**Coverage Requirement**: One node-construction unit test confirming `entries.supersedes` authority for Supersedes edges.

---

### R-13: AnalyticsWrite::GraphEdge shed during bootstrap
**Severity**: Low
**Likelihood**: Low
**Impact**: Bootstrap edges written via the analytics queue during migration are dropped if the queue is at capacity. The migration is idempotent so the next restart will re-insert them, but a one-tick window exists where the graph is incomplete.

**Test Scenarios**:
1. Document the accepted risk in `ARCHITECTURE.md` §2c (already done in the architecture doc).
2. No test required — the migration path uses direct SQL inserts, not the analytics queue. Bootstrap edges in the migration are safe.

**Coverage Requirement**: Verify via code inspection that the v12→v13 bootstrap inserts are direct SQL (not enqueued via `AnalyticsWrite`). No analytics queue shed risk on the migration path.

---

### R-14: TypedGraphState rename incomplete
**Severity**: Med
**Likelihood**: Med
**Impact**: A missed call site compiles only because a type alias was introduced, papering over the semantic upgrade. The type alias is invisible in code review and defeats the intent of the rename.

**Test Scenarios**:
1. `cargo build --workspace` with no type aliases touching `SupersessionState` or `SupersessionStateHandle`. The compiler enforces completeness.
2. `grep` over the codebase post-implementation confirms zero occurrences of `SupersessionState` and `SupersessionStateHandle` outside of comments/docs.

**Coverage Requirement**: Compile-time enforcement (not a test, a constraint). One grep assertion in the gate checklist.

---

### R-15: CoAccess weight formula — implementer uses flat 1.0 instead of normalized formula
**Severity**: Med
**Likelihood**: Med
**Impact**: FR-09 specifies the normalized formula `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)` for CoAccess edge weights. If an implementer uses flat `weight=1.0` instead, all CoAccess edges have equal weight regardless of frequency, losing the signal strength difference that W3-1 GNN relies on. Note: there is no spec/architecture discrepancy — both FR-09 and Architecture §2b specify the normalized formula. The risk is incorrect implementation, not ambiguity.

**Test Scenarios**:
1. After migration with synthetic co_access data (count=2,3,5), assert `GRAPH_EDGES` CoAccess rows have `weight` values consistent with the normalized formula. The count=5 pair must have a strictly higher weight than the count=3 pair. Both must have weight < 1.0 (count=5 is the max, so it equals 1.0; count=3 should equal 0.6). The test must fail if flat `weight=1.0` is used.
2. Assert the migration SQL contains `COALESCE` / `NULLIF` (not a literal `1.0` for the CoAccess insert weight).

**Coverage Requirement**: One assertion in the migration integration test checking the actual weight values, not just row presence. This is already covered by AC-07.

---

## Integration Risks

### Search Hot Path — per-query graph rebuild from state snapshot
The search path clones `all_entries` and `all_edges` on every query from `TypedGraphState`, then calls `build_typed_relation_graph` outside the lock. Under high query volume, this clone + rebuild is executed concurrently by multiple threads. The old `SupersessionGraph` had the same pattern (cloned entries). The risk is that `all_edges` clone cost grows as `GRAPH_EDGES` grows, particularly once W1-2 NLI starts adding edges. No test is required for crt-021 (edges start at bootstrap count), but the architecture must document the clone cost as a future concern.

The architecture addresses this with Option A (store the pre-built `TypedRelationGraph` in `TypedGraphState`, not the raw rows) — per FR-16/FR-22. The implementer must follow this: the state holds `typed_graph: TypedRelationGraph`, not `all_edges: Vec<GraphEdgeRow>`. The architecture doc's §3b pseudocode shows a rebuild-per-search pattern that contradicts FR-16 and FR-22 — this is the authoritative discrepancy risk. The spec (FR-22: "reads the pre-built graph from TypedGraphState under a read lock — it does not rebuild the graph on each query") governs.

### Background tick — write_pool vs analytics queue routing
Compaction uses `write_pool` directly (correct — bounded maintenance write). Bootstrap migration inserts use SQL directly (correct — not analytics queue). The `AnalyticsWrite::GraphEdge` variant exists for future W1-2 runtime edge writes only. If an implementer mistakenly routes the bootstrap migration inserts through the analytics queue, they become shed-able and incomplete graphs result. Verify via code inspection that no `AnalyticsWrite` enqueue calls exist in `migration.rs`.

### W1-2 contract boundary
SR-07 is closed: DELETE+INSERT promotion path is designed in W1-1 schema. The boundary risk is that W1-2 implements promotion using `UPDATE graph_edges SET bootstrap_only=0` (simpler) rather than DELETE+INSERT, leaving stale `created_by="bootstrap"` attribution on confirmed edges. The risk is documentation/contract, not crt-021 correctness. The promotion mechanism (AC-21) must be verified via an integration test that demonstrates the DELETE+INSERT pattern explicitly, establishing the contract for W1-2.

---

## Edge Cases

- **Empty `entries` table at migration**: v12→v13 on a fresh database with zero entries. Bootstrap Supersedes INSERT produces zero rows. CoAccess INSERT produces zero rows. Migration must complete without error. Schema version must reach 13.
- **All `co_access` counts below threshold**: CoAccess bootstrap INSERT produces zero rows. Must not write any CoAccess edges, must not error.
- **Cycle in `entries.supersedes`**: The existing `petgraph::algo::is_cyclic_directed` check detects cycles on the Supersedes sub-graph and returns `FALLBACK_PENALTY`. This behavior is unchanged — verify the typed graph's cycle detection call still operates only on Supersedes edges (not all edges, which would cause false cycle detection on CoAccess bidirectional pairs).
- **`build_typed_relation_graph` called with zero edges**: Must return a valid empty `TypedRelationGraph` with all entry nodes present. `graph_penalty` on a node with no outgoing Supersedes edges must return ORPHAN_PENALTY, not panic.
- **`graph_edges` table has more edges than nodes**: `TypedRelationGraph` may contain edges to nodes not in `node_index` if `all_entries` and `all_edges` are snapshotted at slightly different read times. The builder must handle this gracefully (skip unmapped edges with a logged warning, not panic).
- **`RelationType::Prerequisite` round-trip**: Although no edges of this type are created in crt-021, the round-trip test (AC-02) must include `Prerequisite` to avoid a deserialization surprise in W3-1.
- **TICK_TIMEOUT during compaction**: If the DELETE is slow, the tick wraps in `TICK_TIMEOUT`. The in-memory graph is NOT rebuilt (the rebuild was waiting on compaction). The next tick will retry compaction before rebuild. Verify the tick does not leave a partially-compacted state if the timeout fires mid-DELETE.

---

## Security Risks

### External input surface
crt-021 adds no new MCP tools and no new external-facing API. `TypedRelationGraph` is internal infrastructure only. The risk surface is limited to:

**`AnalyticsWrite::GraphEdge` — caller-supplied field values**: `relation_type: String`, `created_by: String`, `source: String` are caller-supplied strings inserted directly into `GRAPH_EDGES` via parameterized sqlx queries. Parameterized queries prevent SQL injection. No raw string interpolation. Blast radius if a malformed string is inserted: cosmetic data corruption in `GRAPH_EDGES`; no effect on penalty computation (only `relation_type` is used for filtering, and `from_str` returns `None` for unknown values).

**`weight: f32` — NaN/Inf validation**: Already addressed by R-07. A non-finite weight reaching persistence would corrupt future GNN feature vectors. The validation guard (AC-17) is the mitigation.

**Migration bootstrap SQL**: The v12→v13 migration is a fixed SQL block with no caller-supplied parameters. No injection surface.

**`query_graph_edges()` — unbounded SELECT**: A very large `GRAPH_EDGES` table causes a large `Vec<GraphEdgeRow>` to be allocated during tick rebuild. This is a resource exhaustion concern, not a security concern for crt-021 scope. W3-1 should monitor graph size.

**Blast radius if GRAPH_EDGES is truncated or corrupted**: The in-memory `TypedRelationGraph` is rebuilt from GRAPH_EDGES each tick. If GRAPH_EDGES is empty (truncated), the next tick produces an empty graph. `graph_penalty` returns ORPHAN_PENALTY for every node. Search results degrade (superseded entries appear active) but the system continues operating. The `use_fallback` flag does NOT activate on empty-graph cases (only on cycle detection or cold-start) — this is a correctness risk, not a security risk.

---

## Failure Modes

| Failure | Expected Behavior | Testable? |
|---------|-------------------|-----------|
| `query_graph_edges()` fails (store error) | `TypedGraphState::rebuild` returns `Err`; caller retains old state; graph not updated this tick | Yes — inject store error, assert old state preserved |
| `build_typed_relation_graph` detects cycle | Returns `GraphError::CycleDetected`; search path returns `FALLBACK_PENALTY` | Yes — build cycle, call penalty, assert FALLBACK |
| GRAPH_EDGES compaction DELETE fails | Tick logs error; rebuild still runs on pre-compaction state (orphaned edges visible for one cycle) | Yes — inject write_pool error, assert rebuild proceeds |
| Migration aborts mid-run (partial insert) | Schema version stays at 12; next startup retries the entire v12→v13 block; `INSERT OR IGNORE` is idempotent | Yes — partial migration simulation |
| `AnalyticsWrite::GraphEdge` dropped (shed) | Edge not persisted; graph missing the edge until W1-2 retry; no error surfaced to caller | Accepted documented risk for W1-1 bootstrap (N/A); Critical for W1-2 runtime edges |
| `TypedGraphState` write lock poisoned | `.unwrap_or_else(|e| e.into_inner())` poison recovery applies; graph is served from last consistent state | Existing convention — verify poison recovery path in rename |

---

## SR-08 Recommendation: metadata TEXT column in v13 vs v14

**The question**: Should `GRAPH_EDGES` add a `metadata TEXT` (JSON) column in the v13 migration now, or defer to a v14 migration when W3-1 ships?

**Cost of adding in v13 (now)**:
- Migration cost: one line of DDL — `metadata TEXT`. Zero data migration. Purely additive.
- Schema change: `GraphEdgeRow` gains one field (`metadata: Option<String>`). `sqlx-data.json` regeneration is already required for v13 — no additional CI overhead.
- All insert paths need a NULL default (trivial — `DEFAULT NULL`).
- W3-1 GNN can store per-edge feature blobs (NLI confidence, co-access count at edge creation time) without a schema migration.

**Cost of deferring to v14**:
- A v14 `ALTER TABLE graph_edges ADD COLUMN metadata TEXT` migration is needed when W3-1 ships.
- SQLite `ALTER TABLE ADD COLUMN` is supported but requires `DEFAULT NULL` or a literal default — no backfill needed.
- Risk: W3-1 implementer discovers the missing column mid-design and raises a blocking scope issue. Delay cost: 1–2 days of design/spec rework at W3-1 gate.
- Risk: W3-1 architecture is designed assuming `metadata` exists in `GRAPH_EDGES`; if W3-1 skips the migration for speed, the GNN feature vector is incomplete and silently produces lower-quality predictions.

**Assessment**: The cost of adding `metadata TEXT DEFAULT NULL` in v13 is effectively zero — it is one DDL line added while the migration is already being written. The cost of deferring is not the v14 migration itself (which is easy) but the coordination risk: W3-1 must remember to add it, and W3-1 may ship without it if not flagged. Given that `weight: f32` alone is insufficient to store NLI confidence as a distinct field (ADR-001 documents Contradicts edges carry NLI confidence in `weight`, conflating it with Supersedes weight=1.0), the `metadata` column is the only clean path for W3-1 to store edge-type-specific numeric features.

**Recommendation**: Add `metadata TEXT DEFAULT NULL` to `GRAPH_EDGES` in v13. Human decision required. If the human confirms W3-1 will not require per-edge metadata beyond `weight: f32`, defer is acceptable.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-02 | Addressed: `edges_of_type` centralized filter method defined in architecture; all traversal functions prohibited from calling `.edges_directed()` directly |
| SR-02 | R-13 | Addressed: architecture §2c documents the shed risk and prescribes direct `write_pool` path for W1-2 NLI confirmed edges; W1-1 bootstrap migration uses direct SQL (not analytics queue) — no shed risk in W1-1 |
| SR-03 | R-11 | Partially addressed: indexes on `source_id` and `target_id` mitigate NOT IN query cost; batching deferred as post-ship optimization per spec NF-09; tick duration regression test required |
| SR-04 | — | Closed: AC-08 redefined as "empty bootstrap, schema columns ready for W1-2"; no Contradicts edges at migration; confirmed by entry #2404 and architecture §AC-08 Status |
| SR-05 | — | Closed: `Prerequisite` variant reserved, no write paths, documented in FR-04 and AC-20 |
| SR-06 | R-14 | Addressed: architecture and spec prohibit type aliases; compiler enforces ~20 call site rename; enumerated in architecture §3a |
| SR-07 | — | Closed: DELETE+INSERT promotion path designed in architecture §SR-07; W1-1 schema enables W1-2 without modification; AC-21 specifies verification |
| SR-08 | See recommendation above | Open — human decision required on `metadata TEXT` column in v13 vs v14 |
| SR-09 | R-09 | Addressed: AC-19 and NF-08 require `sqlx-data.json` regeneration; CI gate enforces `SQLX_OFFLINE=true` build |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-06) | Min 4 scenarios each; all 25+ existing graph.rs tests for R-01; migration suite for R-06 |
| High | 5 (R-04, R-05, R-07, R-09, R-11) | Min 2 scenarios each; CI gate for R-09 |
| Medium | 5 (R-08, R-10, R-12, R-14, R-15) | Min 1 scenario each; compile-time enforcement for R-14; migration weight assertion for R-15 |
| Low | 1 (R-13) | Code inspection only; documented accepted risk |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — found entry #1203 (cascading rework from single-pass gate validators), entry #1204 (test plan/pseudocode cross-reference gap). Not directly applicable to crt-021's risk profile.
- Queried: `/uni-knowledge-search` for risk patterns — found entry #1607 (SupersessionGraph pattern, directly used), entry #2403 (typed graph upgrade path, directly used), entry #2125 (analytics drain unsuitable for writes callers read back immediately, confirmed SR-02 analysis), entry #2057 (background task shutdown protocol risk, not applicable to crt-021 scope).
- Queried: `/uni-knowledge-search` for SQLite migration / tick compaction — found entry #681 / #370 (create-new-then-swap migration pattern), entry #836 (new table procedure), entry #1777 referenced in scope doc (tick inflation precedent, informs R-11).
- Queried: `/uni-knowledge-search` for sqlx-data.json — found entry #2061 (ADR-004 nxs-011: sqlx-data.json single workspace-level file, directly informs R-09 coverage requirement).
- Stored: nothing novel to store — R-06 (CoAccess NULL weight on empty table) and R-15 (spec/architecture formula discrepancy pattern) are feature-specific, not cross-feature patterns yet. R-11 (tick cost regression from unbounded maintenance DELETE) reinforces entry #1777 pattern but does not extend it.
