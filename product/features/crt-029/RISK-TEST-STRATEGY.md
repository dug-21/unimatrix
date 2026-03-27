# Risk-Based Test Strategy: crt-029 — Background Graph Inference

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | False-positive `Contradicts` edges written by tick silently suppress valid search results via col-030 `suppress_contradicts` | High | Med | Critical |
| R-02 | `get_embedding` O(N) scan unbounded if source-candidate cap is not enforced before Phase 4 | High | Med | Critical |
| R-03 | Threshold boundary validation accepts equal values (`candidate == edge`) if predicate uses `>` instead of `>=` | Med | Med | High |
| R-04 | `query_existing_supports_pairs()` performs a full GRAPH_EDGES scan in memory each tick at large graph sizes | Med | Low | Medium |
| R-05 | Rayon pool starvation: tick and post-store NLI contend on the same pool; tick blocks under sustained `context_store` load | Med | Low | Medium |
| R-06 | `compute_graph_cohesion_metrics` pool choice ambiguous — two conflicting ADRs (#3593 vs #3595); write-pool use creates chronic contention with tick writes | High | Low | High |
| R-07 | `InferenceConfig` struct literal constructions not updated for four new fields — compile failure at merge (52 occurrences) | Med | High | High |
| R-08 | Cap logic inlined in `write_inferred_edges_with_cap` rather than extracted — untestable without live ONNX model | Med | Med | High |
| R-09 | Rayon closure inside `run_graph_inference_tick` calls `tokio::Handle::current()` — panic in rayon worker thread (no Tokio runtime) | High | Med | Critical |
| R-10 | W1-2 violated: `score_batch` dispatched via `spawn_blocking` instead of `rayon_pool.spawn()` — blocks tokio executor | High | Low | High |
| R-11 | `write_nli_edge` / `format_nli_metadata` / `current_timestamp_secs` not promoted to `pub(crate)` — `nli_detection_tick.rs` fails to compile | Med | Med | High |
| R-12 | Priority ordering not enforced: cross-category and isolated pairs dropped at cap in favour of same-category pairs | Med | Med | Medium |
| R-13 | `INSERT OR IGNORE` on duplicate pair re-runs NLI scoring if pre-filter `HashSet` is stale (e.g., built before a concurrent post-store NLI write) | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: False-positive `Contradicts` edges via tick suppress valid results
**Severity**: High
**Likelihood**: Med
**Impact**: A `Contradicts` edge written with a softer threshold than `nli_contradiction_threshold` interacts with `suppress_contradicts` (col-030, always-on) to silently hide a valid result from `context_search`. No operator signal is produced; the suppression is invisible.

**Architecture mitigation**: `write_inferred_edges_with_cap` takes `contradiction_threshold` as an explicit parameter; the call site passes `config.nli_contradiction_threshold` — the same value used by `run_post_store_nli`. AC-19† verifies no softer value is introduced.

**Test Scenarios**:
1. Unit test: mock `NliScores` with `contradiction = nli_contradiction_threshold` exactly — assert no `Contradicts` edge written (strict `>`).
2. Unit test: mock `NliScores` with `contradiction = nli_contradiction_threshold + 0.01` — assert `Contradicts` edge written.
3. Code review: grep `nli_detection_tick.rs` for any literal contradiction threshold value not sourced from `config.nli_contradiction_threshold`.
4. Integration test: run tick, then call `context_search` for a pair that scored at threshold — assert result is not suppressed.

**Coverage Requirement**: AC-10 and AC-19† must both pass. No call path through `write_inferred_edges_with_cap` may pass a value lower than `config.nli_contradiction_threshold` as `contradiction_threshold`.

---

### R-02: Unbounded `get_embedding` O(N) scans per tick
**Severity**: High
**Likelihood**: Med
**Impact**: Without a pre-embedding source-candidate cap, iterating all N active entries through `get_embedding` (O(N) per call via HNSW linear scan) can consume the full tick budget on embedding lookups alone, leaving no time for NLI scoring. At N=10,000 entries this is 10,000 O(N) scans — O(N²) total.

**Architecture mitigation**: `select_source_candidates` caps output to `max_graph_inference_per_tick` source IDs (ADR-003). Phase 3 operates on metadata only; `get_embedding` is not called until Phase 4 and only for the capped list.

**Test Scenarios**:
1. Unit test (AC-06c): seed N=50 active entries, set `max_graph_inference_per_tick=5`; mock `get_embedding` with a call counter — assert counter never exceeds 5 per tick.
2. Unit test: `select_source_candidates` with 200 active entries and `max_sources=10` — assert returned `Vec` length ≤ 10.
3. Benchmark/timing test (optional, not gate-blocking): at N=1000 entries, tick completes within 500ms with default cap=100.

**Coverage Requirement**: AC-06c must pass. `select_source_candidates` must have a unit test asserting the output length invariant independently of the tick function.

---

### R-03: Threshold boundary validation accepts equal values
**Severity**: Med
**Likelihood**: Med
**Impact**: If `InferenceConfig::validate()` uses `>` instead of `>=` in the rejection predicate, a config with `supports_candidate_threshold = supports_edge_threshold = 0.7` passes validation. Every HNSW candidate then immediately satisfies the edge threshold, causing all neighbours to receive `Supports` edges without meaningful NLI discrimination.

**Architecture mitigation**: `InferenceConfig::validate()` rejects `supports_candidate_threshold >= supports_edge_threshold` (strict `>=`). AC-02 names this predicate explicitly (Unimatrix entry #3655 confirms the pattern).

**Test Scenarios**:
1. Unit test (AC-02): validate with `candidate_threshold = 0.7`, `edge_threshold = 0.7` — assert `Err`.
2. Unit test: validate with `candidate_threshold = 0.69`, `edge_threshold = 0.7` — assert `Ok`.
3. Unit test: validate with `candidate_threshold = 0.71`, `edge_threshold = 0.7` — assert `Err` (candidate > edge is also invalid).
4. Unit test (AC-03): validate with `candidate_threshold = 0.0` or `1.0` — assert `Err` for each boundary value.

**Coverage Requirement**: AC-02 and AC-03 must pass. All four boundary combinations (equal, below, above, out-of-range) require dedicated test cases.

---

### R-04: Pre-filter full GRAPH_EDGES scan at scale
**Severity**: Med
**Likelihood**: Low
**Impact**: `query_existing_supports_pairs()` loads all non-bootstrap `Supports` edges into a `HashSet` on every tick. At large graph sizes (tens of thousands of edges) this is a full table scan in memory each tick cycle. At current scale this is acceptable; it becomes a concern when edge count grows into the hundreds of thousands.

**Architecture mitigation**: ADR-004 uses a targeted SQL query (`WHERE relation_type = 'Supports' AND bootstrap_only = 0`) against the `UNIQUE(source_id, target_id, relation_type)` index — avoids loading all edge types. The index provides a bounded scan rather than a full-table sequential read.

**Test Scenarios**:
1. Unit test (ADR-004): empty GRAPH_EDGES — `query_existing_supports_pairs()` returns empty `HashSet`.
2. Unit test: GRAPH_EDGES contains only bootstrap Supports rows (`bootstrap_only = 1`) — returns empty `HashSet`.
3. Unit test: GRAPH_EDGES contains mixed bootstrap and non-bootstrap Supports rows — returns only non-bootstrap pairs.
4. Unit test: GRAPH_EDGES contains Contradicts and CoAccess rows — none appear in the result set.

**Coverage Requirement**: AC-15 must pass. `query_existing_supports_pairs()` requires its own unit tests covering the four cases above, independent of the tick function.

---

### R-05: Rayon pool starvation under concurrent post-store NLI
**Severity**: Med
**Likelihood**: Low
**Impact**: `run_graph_inference_tick` and `run_post_store_nli` share `RayonPool`. Under sustained `context_store` load, the tick's single `rayon_pool.spawn()` queues behind post-store dispatches. If the pool is exhausted, the tick's `.await` blocks on the tokio thread until a rayon worker is free. With default 100 pairs this queues at most 50ms of work, degrading gracefully within `TICK_TIMEOUT`.

**Architecture mitigation**: Single-dispatch pattern (one `rayon_pool.spawn()` per tick) minimises contention duration. Degrades by queuing, not blocking. TICK_TIMEOUT=120s provides headroom. At default cap the worst-case NLI time is ~50ms.

**Test Scenarios**:
1. Integration test: fire 10 concurrent `context_store` calls while tick is running — assert tick completes without panic, `TICK_TIMEOUT` not exceeded.
2. Unit test: tick with `max_graph_inference_per_tick = 1000` (upper bound) still produces a single `rayon_pool.spawn()` call (not multiple).

**Coverage Requirement**: The single-dispatch invariant (AC-08) is the primary mitigation. NFR-06 acknowledgement in code comments documenting degradation behaviour satisfies the documentation requirement.

---

### R-06: `compute_graph_cohesion_metrics` pool ambiguity (UNRESOLVED — human decision required)
**Severity**: High
**Likelihood**: Low
**Impact**: Two conflicting ADRs exist in Unimatrix (#3593 says `write_pool_server()`, #3595 says `read_pool()`). If `compute_graph_cohesion_metrics` uses the write pool, every operator `context_status` call contends with active tick writes on the same pool connection. SQLite's single-writer model serialises all write-pool operations; the status call blocks tick writes and vice versa during high-frequency operation.

**Architecture mitigation**: Architecture doc states the function uses `read_pool()` (referencing entry #3619), confirming SR-06 is mitigated. However, the two conflicting ADRs (#3593 vs #3595) have not been reconciled. One must be deprecated.

**Test Scenarios**:
1. Pre-merge: confirm `compute_graph_cohesion_metrics` source uses `read_pool()` — grep `unimatrix-store` for the function and assert pool call.
2. Integration test: run tick write loop while polling `context_status` concurrently — assert no write-pool deadlock or timeout.

**Coverage Requirement**: A human must resolve the ADR conflict (#3593 vs #3595) and deprecate the incorrect entry before the implementation brief is written. C-12 in the spec requires the architect to confirm the pool choice. This remains an open item.

---

### R-07: `InferenceConfig` struct literal compile failures (52 occurrences)
**Severity**: Med
**Likelihood**: High
**Impact**: Adding four fields to `InferenceConfig` without updating all struct-literal constructions causes compile failures at merge. crt-023 had 7 missed occurrences (Unimatrix entry #2730). The current count is 52 occurrences, a 7x larger surface. A wave-1 compile cycle blocked by missed literals is a known gate-failure pattern.

**Architecture mitigation**: Architecture doc explicitly names the SR-07 mitigation: `Default` impl is a struct literal — all four fields added there. Tests using `..InferenceConfig::default()` tail are safe; bare struct literals fail to compile (desired catch). C-11 mandates a grep pass before PR open.

**Test Scenarios**:
1. Pre-merge gate (AC-18†): `grep -rn 'InferenceConfig {' crates/unimatrix-server/src/` — every occurrence must include all four new fields or `..InferenceConfig::default()` tail.
2. Compile check: `cargo check -p unimatrix-server` passes with no struct-field errors.
3. Unit test (AC-17): parse minimal TOML without the four new fields — assert each default value matches spec.

**Coverage Requirement**: AC-18† is a merge gate condition. The grep must be performed before the PR is opened. The compile check is non-negotiable; any failure here blocks the wave.

---

### R-08: Cap logic inlined rather than extracted — untestable unit
**Severity**: Med
**Likelihood**: Med
**Impact**: If `write_inferred_edges_with_cap` cap logic is inlined in the tick function body rather than extracted as a standalone function, testing the cap boundary requires a live ONNX model. This was the precise gate-3c failure mode in crt-023 (Unimatrix entry #2800). The cap is a critical correctness invariant — it must be independently verifiable.

**Architecture mitigation**: ADR-002 mandates `write_inferred_edges_with_cap` as a named standalone `async fn` in `nli_detection_tick.rs`. The function takes `pairs`, `nli_scores`, thresholds, and `max_edges` as scalars — no `InferenceConfig` dependency. Mock `NliScores` are sufficient for unit tests.

**Test Scenarios**:
1. Unit test (AC-11): construct mock `NliScores` all scoring above threshold, set `cap=3`, pass 10 pairs — assert exactly 3 edges written, function returns `3`.
2. Unit test: cap=0 (invalid by AC-04 but defensible) — assert 0 edges written.
3. Unit test: cap >= pairs.len() — assert all eligible pairs written.
4. Unit test: mixed Supports + Contradicts scores — assert combined count respects cap (not per-type caps).

**Coverage Requirement**: AC-11 must pass. `write_inferred_edges_with_cap` must have dedicated unit tests callable without live ONNX. The function's test module must exist in `nli_detection_tick.rs` under `#[cfg(test)]`.

---

### R-09: Tokio handle access inside rayon closure — panic
**Severity**: High
**Likelihood**: Med
**Impact**: Rayon worker threads have no Tokio runtime. Any async call, `Handle::current()`, or `.await` inside the rayon closure panics at runtime with "no current Tokio runtime". This was the exact failure mode in crt-022 (Unimatrix entries #3339, #3353). The tick's rayon closure must be a synchronous CPU-bound closure only.

**Architecture mitigation**: The rayon dispatch calls `provider.score_batch(&pairs)` — a sync function. All DB access (text fetch, edge write) happens outside the rayon closure on the tokio thread. The architecture diagram confirms this separation.

**Test Scenarios**:
1. Code review gate: `nli_detection_tick.rs` rayon closure body must contain no `.await`, no `tokio::`, no `Handle::current()`.
2. Integration test with NLI enabled: run tick, assert no panic in thread output.
3. Unit test: tick function structure demonstrates single-dispatch, sync closure pattern (structure test, not execution test).

**Coverage Requirement**: This is a compile-time-invisible runtime panic. Code review of the rayon closure is mandatory. The pattern from entry #2742 (collect owned data before spawn) must be followed.

---

### R-10: W1-2 violated — `score_batch` via `spawn_blocking`
**Severity**: High
**Likelihood**: Low
**Impact**: Using `spawn_blocking` instead of `rayon_pool.spawn()` for `score_batch` creates an uncontrolled thread-pool interplay: `spawn_blocking` allocates a blocking thread from tokio's thread pool, not the rayon pool. Under sustained NLI load, this can exhaust the tokio blocking-thread budget and stall the entire server. It also defeats the rayon pool's concurrency management.

**Architecture mitigation**: C-01 is a hard constraint. AC-08 is a gate-3c condition. Architecture ADR-001 places all NLI scoring in `nli_detection_tick.rs` with explicit W1-2 documentation.

**Test Scenarios**:
1. Code review: search `nli_detection_tick.rs` for `spawn_blocking` — must be absent.
2. Unit test structure: tick test demonstrates that the rayon path is exercised via pool mock, not blocking thread.

**Coverage Requirement**: AC-08 (code review + clippy). Zero `spawn_blocking` calls in the tick module is a merge gate condition.

---

### R-11: `pub(crate)` promotions missing in `nli_detection.rs`
**Severity**: Med
**Likelihood**: Med
**Impact**: `write_nli_edge`, `format_nli_metadata`, and `current_timestamp_secs` are currently private in `nli_detection.rs`. If `nli_detection_tick.rs` attempts to call them without the visibility promotion, the crate fails to compile. This is a mechanical dependency that is easy to miss if the implementor adds the new file before promoting the helpers.

**Architecture mitigation**: ADR-001 explicitly lists the three functions and the `pub(crate)` promotion requirement. The architecture doc names the specific line numbers / functions.

**Test Scenarios**:
1. Compile check: `cargo check -p unimatrix-server` passes — this catches the visibility error immediately.
2. Pre-merge: grep `nli_detection.rs` for `pub(crate) fn write_nli_edge`, `pub(crate) fn format_nli_metadata`, `pub(crate) fn current_timestamp_secs` — assert all three present.

**Coverage Requirement**: Compile-time catch. No dedicated test beyond `cargo check`. Delivery agent should promote helpers in wave-1 before writing any `nli_detection_tick.rs` code that calls them.

---

### R-12: Priority ordering not enforced at cap boundary
**Severity**: Med
**Likelihood**: Med
**Impact**: If `select_source_candidates` or the Phase-5 sort does not correctly place cross-category pairs before isolated pairs before same-category pairs, the per-tick budget may be spent on the lowest-value pairs first. The col-029 metrics (`cross_category_edge_count`, `isolated_entry_count`) would improve more slowly than expected. This is a correctness issue, not a safety issue.

**Architecture mitigation**: `select_source_candidates` applies the three-tier ordering explicitly. Phase 5 re-sorts the expanded pair set by the same priority before truncation.

**Test Scenarios**:
1. Unit test (AC-07): seed 3 cross-category pairs, 3 isolated-entry pairs, 4 same-category pairs; set `cap=3` — assert only the 3 cross-category pairs are written.
2. Unit test: cap=6 — assert 3 cross-category + 3 isolated pairs written, 0 same-category.
3. Unit test: all same-category pairs — assert pairs written in similarity-descending order.
4. Unit test: `select_source_candidates` with a mix — assert cross-category source IDs appear first in the output `Vec`.

**Coverage Requirement**: AC-07 must pass with a mix of pair types and a binding cap. The test must assert both what was written and what was dropped.

---

### R-13: Pre-filter `HashSet` stale under concurrent post-store NLI writes
**Severity**: Low
**Likelihood**: Low
**Impact**: The `HashSet` of existing Supports pairs is built once at the start of the tick (Phase 2). If `run_post_store_nli` writes a new Supports edge for a pair that is also in the tick's candidate set during Phase 4–7, the tick's pre-filter will not see it. The pair proceeds to NLI scoring. The write attempt issues `INSERT OR IGNORE`, which is harmless — the duplicate is silently ignored by the `UNIQUE` constraint. The only cost is one wasted NLI call.

**Architecture mitigation**: `INSERT OR IGNORE` is the explicit backstop. The pre-filter is an optimisation, not the deduplication mechanism. SCOPE.md and FR-07 both document this.

**Test Scenarios**:
1. Unit test: seed a Supports edge after the tick's pre-filter is built; assert the tick's `INSERT OR IGNORE` does not produce a duplicate row.
2. Idempotency test (AC-16): run the tick twice on the same data — assert edge count does not double.

**Coverage Requirement**: AC-16 idempotency case covers this. No additional dedicated test is required.

---

## Integration Risks

### `nli_detection_tick.rs` module boundary
The new module imports three `pub(crate)` symbols from `nli_detection.rs`. If those promotions are incomplete or the `pub mod nli_detection_tick;` declaration is missing from `services/mod.rs`, the entire server crate fails to compile. The module declaration must be the first thing added to `mod.rs`; promotions must precede any use of the shared functions.

### `write_inferred_edges_with_cap` vs `write_edges_with_cap` divergence
The two functions share the same low-level `write_nli_edge` helper but have different signatures and threshold semantics. A future refactor that changes `write_nli_edge`'s signature must update both callers. Risk: a crt-023-style refactor assumes `write_edges_with_cap` is the only caller of `write_nli_edge`.

### Background tick call site ordering in `background.rs`
`run_graph_inference_tick` must run after `maybe_run_bootstrap_promotion`. If ordering is reversed, bootstrap-promoted edges may not yet be visible to the tick's pre-filter. This is a subtle sequencing constraint with no compile-time enforcement.

### Phase 6 text fetch using `write_pool` (bootstrap promotion precedent)
Architecture uses `store.get_content_via_write_pool()` to see recently committed rows. This places a write-pool read inside the tick, creating a marginal contention surface with the edge writes in Phase 8. At default scale (100 pairs) this is benign; at cap=1000 it may serialize with concurrent post-store NLI writes.

### `query_by_status(Active)` returns full `EntryRecord`
Phase 2 loads all active `EntryRecord` structs. The tick uses only `id` and `category` fields. At large N this is wasteful memory allocation. Not a correctness risk, but a performance risk at scale. The architecture acknowledges this is acceptable at current scale.

---

## Edge Cases

| Edge Case | Risk | Test Required |
|-----------|------|---------------|
| Empty graph (0 active entries) | `query_by_status` returns empty `Vec`; tick should no-op gracefully, log 0 edges | Unit test: assert no panic, 0 edges written |
| Graph fully inferred (all pairs in pre-filter `HashSet`) | All pairs skip NLI; 0 NLI calls, 0 edges written; tick runs in O(1) after pre-filter | Unit test: pre-populate all pairs as existing Supports edges; assert NLI not called |
| NLI disabled (`nli_enabled = false`) | `run_graph_inference_tick` not called from `background.rs`; no-op (AC-14) | Integration test: `nli_enabled = false`, assert tick not invoked |
| NLI not ready (`nli_handle.get_provider()` returns `Err`) | Early return in Phase 1; no queries, no embeddings, no NLI (AC-05) | Unit test: stub `NliServiceHandle` returning `Err`; assert 0 DB calls |
| Cap exactly at `max_graph_inference_per_tick` | Writing exactly N edges; `edges_written == max_graph_inference_per_tick`; next tick starts fresh | Unit test: cap=10, 10 eligible pairs — assert all 10 written, function returns 10 |
| Single active entry | HNSW search returns no neighbours; 0 pairs; tick completes with 0 edges | Unit test: 1 entry in DB; assert 0 edges |
| All entries same category | No cross-category priority boost; isolated entries get second-tier priority | Unit test: verify same-category entries still processed in similarity order |
| Pair `(A, B)` and `(B, A)` both HNSW candidates | Deduplication normalises to `(min, max)`; one NLI call; one directed edge written | Unit test: seed A and B as mutual HNSW neighbours; assert single NLI call |
| Source candidate with no embedding (`get_embedding` returns `None`) | Skip that source in Phase 4; continue to next candidate | Unit test: one source returns `None`; assert remaining sources processed |
| `score_batch` result length != pairs length | Defensive check: skip write if lengths mismatch; log `warn` | Unit test: mock scorer returning shorter result; assert no panic, partial write |

---

## Security Risks

**Untrusted input surface**: `run_graph_inference_tick` reads only from the internal `Store` and `VectorIndex`. It does not accept any external input — no MCP tool parameters, no user-supplied text path into this function. The tick's input is bounded to active entries already in the store, which were validated at `context_store` time.

**Blast radius if compromised**: An adversary who can write arbitrary entries to the store could craft entries whose NLI scores reliably produce `Contradicts` edges for targeted pairs. This would trigger `suppress_contradicts` in `SearchService::search`, silently suppressing search results. This is not a new attack surface — `run_post_store_nli` has the same exposure. The mitigation is the NLI threshold floor (R-01) and the fact that the tick does not lower existing thresholds.

**`INSERT OR IGNORE` semantic**: Not a SQL injection risk — all identifiers (`source_id`, `target_id`) are `u64` values from trusted internal state. No string interpolation in SQL.

**ONNX model trust**: The cross-encoder model is loaded at server startup from a local path. No remote model fetch occurs in the tick path. No new trust surface introduced.

---

## Failure Modes

| Failure | Expected Behaviour | Verification |
|---------|--------------------|-------------|
| `query_by_status` returns `Err` | Tick logs `warn`, returns without writing edges | Unit test with mock store returning `Err` |
| `query_entries_without_edges` returns `Err` | Tick logs `warn`, proceeds with empty isolated set (priority degraded, not failed) | Unit test: assert tick continues, 0 isolated-priority edges |
| `query_existing_supports_pairs` returns `Err` | Tick logs `warn`, proceeds with empty pre-filter (NLI called on all pairs; `INSERT OR IGNORE` backstop catches duplicates) | Unit test: assert tick continues, idempotency preserved |
| `get_embedding` returns `Err`/`None` for a source | Source skipped; remaining candidates processed | Unit test: one source fails embedding; assert remaining sources processed |
| `rayon_pool.spawn` future panics (ONNX error) | Tick catches the `Err` from the rayon join handle; logs `warn`; 0 edges written for that batch | Integration test: ONNX model not loaded; assert graceful no-op |
| `write_nli_edge` returns `false` (INSERT conflict) | `INSERT OR IGNORE` silently ignores; `edges_written` not incremented for that pair | Unit test: pre-existing edge; assert `edges_written = 0` for that pair |
| Tick exceeds `TICK_TIMEOUT` | `run_single_tick` timeout fires; tick aborted mid-batch; partial edges committed | Not directly testable; mitigated by default cap (50ms << 120s) |
| `nli_detection_tick.rs` > 800 lines after implementation | File size constraint violated; merge blocked by NFR-05 | Pre-merge: `wc -l nli_detection_tick.rs`; must be ≤ 800 |

---

## Scope Risk Traceability

| Scope Risk | Severity | Architecture Risk | Resolution |
|-----------|----------|------------------|------------|
| SR-01 — NLI false-positive Contradicts via tick silences results | High | R-01 | `write_inferred_edges_with_cap` takes `contradiction_threshold` as explicit parameter; call site passes `config.nli_contradiction_threshold` exactly. AC-19† enforces no softer value. ADR-002 documents the contract. |
| SR-02 — `get_embedding` O(N) per call unbounded | High | R-02 | `select_source_candidates` caps to `max_graph_inference_per_tick` before Phase 4. ADR-003 documents the derivation. AC-06c unit test enforces the call count. |
| SR-03 — Threshold boundary `>=` vs `>` ambiguity | Med | R-03 | `InferenceConfig::validate()` uses `>=` reject predicate (explicit in AC-02 wording). Unit tests cover equal-value case. |
| SR-04 — Pre-filter query scale (GRAPH_EDGES covering index) | Med | R-04 | ADR-004 uses `query_existing_supports_pairs()` with targeted SQL against the `UNIQUE` index. Scale boundary documented in architecture (NFR-07). |
| SR-05 — Rayon pool contention post-store vs tick | Med | R-05 | Single-dispatch pattern (AC-08) minimises contention window. TICK_TIMEOUT provides 120s headroom. Documented in NFR-06. |
| SR-06 — `compute_graph_cohesion_metrics` pool choice ambiguous | High | R-06 | **UNRESOLVED**: Architecture doc cites `read_pool()` per entry #3619, but two conflicting ADRs exist in Unimatrix (#3593 write-pool, #3595 read-pool). Human must reconcile the conflict and deprecate the incorrect ADR before delivery. |
| SR-07 — `InferenceConfig` struct literal trap (52 occurrences) | Med | R-07 | C-11 mandates grep pass pre-PR. AC-18† is a named merge gate. Unimatrix entry #2730 pattern applied. |
| SR-08 — Cap logic testability | Med | R-08 | ADR-002: `write_inferred_edges_with_cap` is a standalone function with scalar threshold parameters. Independently testable with mock `NliScores`. AC-11 unit test validates. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-09) | 9 scenarios (3+3+3) |
| High | 5 (R-03, R-06, R-07, R-08, R-10, R-11) | 14 scenarios |
| Medium | 4 (R-04, R-05, R-12, R-13) | 12 scenarios |
| Low | 1 (R-13) | 2 scenarios |

**Mandatory test modules**:
- `nli_detection_tick.rs` — inline `#[cfg(test)]` module (entry #3631 pattern): no-op guard, source candidate cap, priority ordering, cap enforcement, pre-filter skip, idempotency, threshold boundary
- `unimatrix-store` read tests: `query_entries_without_edges` (4 cases), `query_existing_supports_pairs` (4 cases)
- `infra/config.rs` tests: `InferenceConfig` defaults (AC-01, AC-17), validation reject cases (AC-02, AC-03, AC-04, AC-04b)

**Pre-merge gates (not unit tests)**:
- `grep -rn 'InferenceConfig {' crates/unimatrix-server/src/` — 52 occurrences updated (AC-18†)
- `wc -l nli_detection_tick.rs` ≤ 800 (NFR-05)
- `grep -n 'spawn_blocking' nli_detection_tick.rs` — must return empty (C-01, AC-08)
- `grep -n 'pub(crate) fn write_nli_edge\|pub(crate) fn format_nli_metadata\|pub(crate) fn current_timestamp_secs' nli_detection.rs` — all three present (R-11)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned gate failures — found entries #2758, #3579 (gate-3b test omission), #3548 (coverage-gap). Informed R-09 rayon panic severity (entries #3339, #3353).
- Queried: `/uni-knowledge-search` for risk patterns — found entry #3655 (tick candidate-bound + contradiction floor pattern), #2730 (InferenceConfig struct literal trap), #2800 (cap logic testability lesson).
- Queried: `/uni-knowledge-search` for pool/contention patterns — found entries #3593 and #3595 (conflicting col-029 ADRs on `compute_graph_cohesion_metrics` pool choice). Both entries active; conflict unresolved. Surfaced as R-06 unresolved risk.
- Stored: nothing novel to store — all patterns (struct literal trap, cap logic testability, candidate bound before embedding, contradiction threshold floor) are already in Unimatrix as entries #2730, #2800, #3655, #3653. No new cross-feature pattern discovered.
