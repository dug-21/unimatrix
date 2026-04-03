# Risk-Based Test Strategy: crt-045

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Post-construction write-back does not propagate to SearchService — Arc clone assumption invalidated by future refactor | High | Low | Medium |
| R-02 | Wired-but-unused: handle holds rebuilt state but search.rs reads from a stale or different clone | High | Low | Medium |
| R-03 | Test graph seeded with Quarantined entries produces vacuous `use_fallback=false` with empty `typed_graph` | High | Med | High |
| R-04 | Rebuild error (cycle or I/O) causes from_profile() to return Err, blocking all metric collection | High | Low | Medium |
| R-05 | TOML parse failure at profile load time prevents eval run before graph code is reached | Med | Low | Medium |
| R-06 | Baseline regression: rebuild call introduces latency or behavioral change on no-graph profiles | Med | Low | Low |
| R-07 | Rebuild hangs on corrupted GRAPH_EDGES with no timeout guard | Med | Low | Low |
| R-08 | typed_graph_handle() accessor promoted to pub — external callers can write graph state, breaking the single-writer invariant | Med | Low | Low |
| R-09 | mrr_floor=0.2651 baseline threshold has drifted since crt-042, causing gate fail on correct implementation | Med | Med | Medium |
| R-10 | Step 13b write-back occurs before ServiceLayer fully initialises SearchService — race condition in from_profile() | Low | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Post-Construction Write-Back May Not Propagate

**Severity**: High
**Likelihood**: Low (Arc::clone confirmed at services/mod.rs:419 — SR-01 resolved)
**Impact**: SearchService operates with cold-start empty graph for entire eval run. All graph-dependent phases (graph_expand, PPR, graph penalties) silently disabled. Profiles produce bit-identical results.

**Test Scenarios**:
1. After `from_profile()` against a graph-seeded snapshot, call `layer.typed_graph_handle().read()` and assert `use_fallback == false` — confirms write propagated through the shared Arc.
2. Call `layer.inner.search.search(params).await` and assert `Ok(_)` — confirms SearchService reads from the same Arc allocation and does not panic when `use_fallback == false`.

**Coverage Requirement**: AC-06 integration test must verify both handle state AND that a live search call does not error. Handle inspection alone (no live search) does not cover this risk.

---

### R-02: Wired-But-Unused Anti-Pattern (entry #1495, entry #3935)

**Severity**: High
**Likelihood**: Low
**Impact**: The write-back at Step 13b swaps the handle's inner state, but if `search.rs:Step 6d` reads from a different clone or captures a snapshot at construction time, the guard `if !use_fallback` remains `false` for all scenario replays. eval run completes without error but produces fallback-only results — the same bit-identical outcome as the pre-fix bug. This failure mode is invisible without a behavioural assertion.

**Test Scenarios**:
1. Seed snapshot with two Active entries and one S1/S2/S8 edge. Call `from_profile()`. Assert `use_fallback == false` on handle (structural). Then call `find_terminal_active(seeded_entry_id, &guard.typed_graph, &guard.all_entries)` and assert `Some(...)` returned (graph connectivity). Then call a live `search()` and assert `Ok(_)` (behavioural). All three layers must pass.
2. Construct an `EvalServiceLayer` without graph edges (empty snapshot). Assert `use_fallback == true` on handle — confirms the cold-start path still works and the write-back did not corrupt it.

**Coverage Requirement**: Three-layer assertion required (AC-06 + ADR-003): handle state, graph connectivity, live search call. Structural handle inspection alone is insufficient.

---

### R-03: Test Fixture with Quarantined Entries Produces False Vacuous Pass

**Severity**: High
**Likelihood**: Med (easy mistake when seeding; ADR-004 addendum, entry #3768)
**Impact**: `TypedRelationGraph::build_typed_relation_graph()` filters out Quarantined entries. If the test seeds only Quarantined entries or edges between them, rebuild succeeds with `use_fallback=false` but `typed_graph` is empty. The `non-empty graph` assertion (AC-06) fails or passes vacuously. The test proves nothing about graph population correctness.

**Test Scenarios**:
1. Seed at least two entries with status `Active` (status != Quarantined). Insert one graph edge between them via raw SQL `INSERT INTO graph_edges`. After `from_profile()`, assert `guard.typed_graph` has at least one node and one edge. This is the only scenario that is not vacuous.
2. (Regression guard) Seed a snapshot with one Active entry and one Quarantined entry connected by an edge. Assert graph has exactly one node (the Active entry) and zero edges — confirms quarantine filter is still honoured by rebuild.

**Coverage Requirement**: C-09 of SPECIFICATION.md. Fixture must use Active entries with confirmed graph edges. Quarantined-only seeding must not be used.

---

### R-04: Rebuild Error Aborts from_profile() — No Eval Metrics Collected

**Severity**: High
**Likelihood**: Low
**Impact**: If `TypedGraphState::rebuild()` returns `Err(...)` and `from_profile()` propagates that error, eval run terminates immediately. No MRR/P@5 data collected. AC-05 requires degraded mode: `Ok(layer)` returned with `use_fallback=true`.

**Test Scenarios**:
1. Seed a snapshot with a cycle-producing Supersedes edge set (A→B→A). Call `from_profile()`. Assert result is `Ok(layer)`, not `Err`. Assert `guard.use_fallback == true`. Assert a `warn!` log was emitted (if tracing subscriber is present in test).
2. (Optional) Call `from_profile()` with a read-only store that returns I/O error on graph query. Assert `Ok(layer)` with `use_fallback == true`.

**Coverage Requirement**: AC-05. At minimum one test covering the cycle-detected degraded path. The `Ok(layer)` assertion is the non-negotiable check.

---

### R-05: ppr-expander-enabled.toml Parse Failure Before Graph Code Executes

**Severity**: Med
**Likelihood**: Low (fixed by ADR-005: distribution_change=false)
**Impact**: `parse_profile_toml()` returns `EvalError::ConfigInvariant` if `distribution_change=true` without all three required targets. This gates the entire eval run before any graph code is reached, making the fix unobservable.

**Test Scenarios**:
1. Run `unimatrix eval run --profile ppr-expander-enabled.toml` against a populated snapshot. Assert exit code is 0 (no parse error). Assert MRR/P@5 output is present. (Manual AC-03 verification.)
2. Automated: parse `ppr-expander-enabled.toml` with `parse_profile_toml()` in a unit test. Assert result is `Ok(profile)`. Assert `profile.config.distribution_change == false` and `profile.config.inference.ppr_expander_enabled == true`.

**Coverage Requirement**: AC-03. The TOML must parse cleanly. The `distribution_change=false` comment must be present to prevent regression.

---

### R-06: Baseline Regression — rebuild() Call Changes Non-Graph Profile Behaviour

**Severity**: Med
**Likelihood**: Low
**Impact**: `TypedGraphState::rebuild()` is now unconditionally called in `from_profile()` for all profiles, including `baseline.toml` where `ppr_expander_enabled=false`. If rebuild has side effects (unexpected writes, log spam, or altered search behavior via `all_entries`), the baseline path may shift.

**Test Scenarios**:
1. Call `from_profile()` with `baseline.toml` (no inference section) against a graph-seeded snapshot. Assert `guard.use_fallback == false` (rebuild ran). Assert a search call returns `Ok(_)`. Assert search result count and order matches the pre-fix baseline (no behavioural change in non-PPR path).
2. Existing integration tests in `layer_tests.rs` and `eval/profile/tests.rs` — must all pass unchanged (AC-07, AC-08).

**Coverage Requirement**: AC-04, AC-07, AC-08. All existing tests serve as regression coverage. No new test needed beyond confirming existing suite passes.

---

### R-07: Rebuild Hangs on Corrupted GRAPH_EDGES — No Timeout Guard

**Severity**: Med
**Likelihood**: Low (sqlx query timeout provides implicit bound; explicit timeout deferred per SPECIFICATION.md)
**Impact**: `from_profile()` hangs indefinitely during eval run. No output, no error. Developer sees no progress.

**Test Scenarios**:
1. Not directly testable without injecting a blocking store. Accepted risk per SPECIFICATION.md: sqlx query timeout is the implicit guard. A follow-up issue should add `tokio::time::timeout` around rebuild if sqlx timeout is not configured.

**Coverage Requirement**: Accepted as residual risk within scope constraints. Defer to follow-up.

---

### R-08: typed_graph_handle() Accessor Visibility Widens to pub

**Severity**: Med
**Likelihood**: Low
**Impact**: External callers outside `unimatrix-server` could write to the graph state handle, violating the single-writer invariant (only background tick or eval construction should write). Breaks encapsulation of `ServiceLayer` internals.

**Test Scenarios**:
1. Compile-time check: the accessor is `pub(crate)` in `eval/profile/layer.rs`. Any attempt to call it from outside `unimatrix-server` should produce a compile error. No runtime test needed — visibility is enforced by the Rust compiler.

**Coverage Requirement**: ADR-004. PR reviewer must confirm accessor is `pub(crate)`, not `pub`. This is a code-review gate, not a runtime test.

---

### R-09: mrr_floor=0.2651 Threshold Has Drifted Since crt-042

**Severity**: Med
**Likelihood**: Med (baseline metrics can shift between feature cycles)
**Impact**: The eval gate for `ppr-expander-enabled.toml` fails on a correct implementation because the current baseline MRR is below 0.2651. Developer sees a gate failure that is not caused by the graph fix.

**Test Scenarios**:
1. Before merging, delivery agent must run `unimatrix eval run --profile baseline.toml` and confirm reported MRR matches or exceeds 0.2651. If not, ADR-005 and SPECIFICATION.md C-06 require a scope variance flag before changing the threshold.

**Coverage Requirement**: Manual pre-merge verification. Not automatable without a live snapshot.

---

### R-10: Write-Back Occurs Before SearchService Is Fully Initialised

**Severity**: Low
**Likelihood**: Low (from_profile() is single-threaded async; no concurrent access during construction)
**Impact**: Theoretical only — `from_profile()` is not called concurrently for the same instance. The write-back at Step 13b occurs after `with_rate_config()` returns; SearchService is fully constructed at that point.

**Test Scenarios**:
1. Not testable as a distinct scenario — concurrency is structurally excluded by `from_profile()`'s sequential async execution. Covered incidentally by AC-06 integration test.

**Coverage Requirement**: Accepted as resolved by architecture. No dedicated test required.

---

## Integration Risks

**IR-01: Arc clone chain broken by future ServiceLayer refactor.** The post-construction write-back relies on `services/mod.rs:419` using `Arc::clone(&typed_graph_state)`. If `with_rate_config()` is refactored to create a separate handle for SearchService, the write-back stops propagating. This must be documented in ADR-001 and any future refactor of `with_rate_config()` must check the Arc chain.

**IR-02: TypedGraphState::rebuild() reads GRAPH_EDGES — eval snapshot must contain this table.** Snapshots created before the GRAPH_EDGES table existed (pre-crt-021) will cause a store I/O error. Degraded mode (`use_fallback=true`) applies. Delivery agents using old snapshots will see the degraded path, not the fixed path. Snapshot age must be verified before running the eval gate.

**IR-03: VectorIndex must be present alongside snapshot.** Entry #2661 (lesson-learned): snapshot commands must copy all storage artifacts. If the snapshot at the eval path contains only the SQLite database and not the `vector/` sibling directory, the embed handle init will fail before graph rebuild is reached. This is a pre-existing risk, not introduced by crt-045, but is a common failure mode during manual harness runs.

**IR-04: find_terminal_active is an internal function.** ADR-003 proposes using `find_terminal_active` as a behavioural proxy in the test. If this function is not `pub(crate)` in `typed_graph.rs`, the test cannot call it without a visibility change. The delivery agent must verify visibility before choosing this assertion strategy — if unavailable, use direct graph node count instead.

---

## Edge Cases

**EC-01: Empty snapshot (zero entries, zero edges).** `TypedGraphState::rebuild()` returns `Ok(TypedGraphState { use_fallback: false, typed_graph: empty, all_entries: [] })`. The graph is non-fallback but empty. The `non-empty graph` assertion in AC-06 would fail — only valid for the seeded-graph test, not the empty-snapshot path. The empty-snapshot path is a valid degraded state, not a bug.

**EC-02: Snapshot with entries but no edges.** Rebuild succeeds, `use_fallback=false`, but `TypedRelationGraph` has nodes and zero edges. PPR will not traverse any graph path. `ppr_expander_enabled=true` activates Phase 0, which expands candidates by BFS — with zero edges, expansion produces no additional candidates. Result is functionally equivalent to baseline but traversal code executes (no panic).

**EC-03: Snapshot contains only Supersedes edges (no S1/S2/S8).** `TypedRelationGraph` will have Supersedes edges. `graph_expand` BFS traverses S1/S2/S8 only — Supersedes edges are not traversal edges. BFS produces no expansion. PPR personalisation vector is built from all edges, so Supersedes-only graph still populates a PPR vector. Results may differ from baseline but only in PPR reranking, not expansion.

**EC-04: Rebuild called on database with schema version older than GRAPH_EDGES table.** Store returns `StoreError::*`. Degraded mode: `use_fallback=true`, `Ok(layer)`. Eval run proceeds without graph. Warning logged.

**EC-05: TOML ppr-expander-enabled.toml has both `distribution_change=false` and an explicit `[profile.distribution_targets]` block.** `parse_profile_toml()` should accept this (targets are ignored when `distribution_change=false`). Must not cause a parse error — targets are structurally optional when the flag is false.

---

## Security Risks

**SR-SEC-01: Snapshot database is caller-supplied.** `from_profile()` opens a database file at a path provided by the profile TOML. The eval harness does not sandbox the snapshot database. A malformed database file could trigger SQLite parsing errors (StoreError) or, theoretically, an exploit via SQLite's file parser. Degraded mode (AC-05) ensures no abort. Blast radius: degraded eval run only — no write path exists (snapshot is read-only).

**SR-SEC-02: Profile TOML is caller-supplied and includes threshold values.** `parse_profile_toml()` deserialises f64 threshold values from TOML. A crafted profile with NaN or infinity for `mrr_floor`/`p_at_5_min` could cause silent comparison failures in gate logic. These are developer-facing files in `product/research/` — not user-supplied input. Blast radius: incorrect gate pass/fail signal. Not a production security concern.

**SR-SEC-03: typed_graph_handle() exposes a write handle.** The accessor returns `Arc<RwLock<TypedGraphState>>`. If misused (e.g., called from outside `unimatrix-server` via a `pub` promotion), external code could swap in an arbitrary graph state, bypassing the rebuild integrity check. Mitigated by `pub(crate)` visibility (R-08). No external attack surface.

---

## Failure Modes

**FM-01: Rebuild failure (cycle detected).** Expected: `tracing::warn!` emitted. `use_fallback=true` in handle. `from_profile()` returns `Ok(layer)`. Eval run proceeds in fallback mode producing baseline-equivalent results. No observable error to the user — only the warning log signals degraded state.

**FM-02: Rebuild failure (store I/O).** Same as FM-01. Eval run proceeds. Warning logged with error message.

**FM-03: ppr-expander-enabled.toml parse failure.** Expected: `EvalError::ConfigInvariant` returned from `parse_profile_toml()`. Eval run terminates before construction. Error message names the missing/invalid field. Fixed by ADR-005 (distribution_change=false).

**FM-04: VectorIndex not found at snapshot path.** Embed handle init fails before graph rebuild is reached. `from_profile()` returns `Err`. Eval run terminates. Error message names the missing vector directory. Pre-existing failure mode, not introduced by crt-045.

**FM-05: Snapshot database not found at path.** `SqlxStore::open_readonly()` returns error. `from_profile()` returns `Err`. Eval run terminates immediately. Error is clear and actionable.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (High): Post-construction Arc write may not propagate if SearchService holds value copy | R-01 | RESOLVED: `services/mod.rs:419` confirmed as `Arc::clone()`. ADR-001 documents. Live search call in AC-06 test provides runtime confirmation. |
| SR-02 (Med): rebuild() has no timeout guard — corrupted GRAPH_EDGES could hang from_profile() | R-07 | Accepted within scope. sqlx query timeout is implicit guard. Explicit `tokio::time::timeout` deferred per SPECIFICATION.md. Follow-up issue recommended. |
| SR-03 (Med): typed_graph_handle() accessor may be promoted to pub, widening API surface | R-08 | Mitigated by ADR-004: accessor is `pub(crate)`. PR review gate enforces this. No runtime test — compiler enforces visibility. |
| SR-04 (Low): Future profile sets distribution_change=true without required targets, causing silent parse fail | R-05 | Mitigated by ADR-005: comment in ppr-expander-enabled.toml explains why distribution_change=false is intentional. Parse-time unit test catches future regression. |
| SR-05 (High): Wired-but-unused — handle written but SearchService reads at query time from different clone | R-02 | Mitigated by ADR-003: AC-06 test requires three-layer assertion (handle state + graph connectivity + live search call). Entry #3935 confirms structural-only coverage is insufficient. |
| SR-06 (Med): Test snapshot without Active entries + edges produces vacuous non-empty-graph assertion | R-03 | Mitigated by C-09 of SPECIFICATION.md and ADR-003: test fixture must use at least two Active entries with one S1/S2/S8 edge via raw SQL. Quarantined-only seeding explicitly prohibited. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-02, R-03, R-04 elevated by SR evidence) | 6 scenarios minimum (AC-06 three-layer + cycle degraded + quarantine filter + live search) |
| Medium | 4 (R-01, R-05, R-06, R-09) | 4 scenarios (post-construction propagation, TOML parse, baseline regression, metric threshold) |
| Low | 3 (R-07, R-08, R-10) | 1 scenario (compiler visibility check for R-08; R-07 and R-10 accepted as residual) |

**Non-negotiable test scenarios** (gate-blocking if absent, per entry #2758):
1. `use_fallback == false` AND `typed_graph` non-empty after `from_profile()` with Active-entry + edge snapshot (R-03, AC-06)
2. Live `search()` call returns `Ok(_)` on graph-enabled layer (R-02, SR-05, ADR-003)
3. `Ok(layer)` returned on cycle-detected rebuild error with `use_fallback == true` (R-04, AC-05)
4. All existing `layer_tests.rs` and `eval/profile/tests.rs` tests pass unchanged (R-06, AC-08)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection eval harness` — found entry #3935 (tracing-test AC deferred: structural coverage without production path, gate-3b failure; directly informs R-02 requirement for live search assertion). Entry #2661 (snapshot must copy all artifacts; IR-03). Entry #2758 (non-negotiable test names must be confirmed before PASS claims; applied in Coverage Summary).
- Queried: `/uni-knowledge-search` for `wired-but-unused anti-pattern` — found entry #4100 (ADR-003 crt-045, already in architecture); entry #3691 (cold-start guard pattern for RwLock, informs R-01/R-02 scenarios).
- Queried: `/uni-knowledge-search` for `risk pattern TypedGraphState rebuild eval layer` — found entry #4096 (EvalServiceLayer cold-start pattern, confirmed scope risk SR-01 resolution is correctly documented).
- Stored: nothing novel to store — risks here are feature-specific to crt-045 wiring. Pattern entry #4096 already captures the cold-start anti-pattern. Recurring pattern across 2+ features would be: "eval layers that wrap live-server service layers must call the same initialisation hooks the background tick calls, or graph/NLI state remains cold." This is already captured in #4096 and #4100.
