# Risk-Based Test Strategy: crt-042 (PPR Expander)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Flag-off regression: Phase 0 code path is not wholly bypassed when `ppr_expander_enabled = false`, producing non-bit-identical search output | High | Med | Critical |
| R-02 | S1/S2 Informs edges are single-direction in GRAPH_EDGES; Outgoing-only traversal sees only half the graph silently | High | High | Critical |
| R-03 | Quarantine bypass: a quarantined entry reachable via `graph_expand` enters `results_with_scores` due to missing or mis-ordered check in Phase 0 | High | Low | High |
| R-04 | O(N) latency at full expansion: 200 × O(7000) comparisons per search causes unacceptable P95 latency when expander is enabled; no measurement gate enforced before flag flip | High | Med | High |
| R-05 | Combined ceiling overflow: Phase 0 (200) + Phase 5 (50) + HNSW (20) = 270 max pool; if Phase 5 does not correctly see Phase 0 entries as already-in-pool, pool could exceed 270 | Med | Low | High |
| R-06 | Back-fill race: S1/S2 back-fill migration runs concurrently with crt-042 eval; GRAPH_EDGES table is partially populated during the eval run, producing non-reproducible P@5 measurement | Med | Med | High |
| R-07 | Eval gate failure: P@5 shows no improvement despite correct implementation (graph too sparse, S1/S2 single-direction, or budget-boundary bias excluding relevant entries) | Med | Med | High |
| R-08 | InferenceConfig hidden test sites: new fields added to struct but not to all literal construction sites in tests; compilation passes but runtime defaults diverge | Med | High | High |
| R-09 | `edges_of_type()` boundary violation: `graph_expand.rs` uses `.edges_directed()` or `.neighbors_directed()` directly, bypassing the sole traversal boundary (SR-01 / entry #3627) | Med | Low | Med |
| R-10 | Timing instrumentation absent or at wrong log level: `debug!` trace not emitted, or emitted at `info!`, making latency measurement impossible or flooding production logs | Med | Med | Med |
| R-11 | BFS visited-set missing: cycles in graph (bidirectional CoAccess edges) cause infinite BFS loop or exponential re-expansion before max_candidates cap is hit | Med | Low | Med |
| R-12 | Seed exclusion failure: `graph_expand` returns IDs already in the seed set, injecting duplicate entries into `results_with_scores` before Phase 1 | Med | Low | Med |
| R-13 | Determinism failure: BFS frontier not processed in sorted node-ID order; same inputs produce different results across runs (flaky tests, non-reproducible eval) | Med | Low | Med |
| R-14 | Config validation conditional gap: `expansion_depth = 0` or `max_expansion_candidates = 0` accepted when `ppr_expander_enabled = false`, causes panic or undefined behavior at flag-flip time (NLI trap repeat — entry #3817) | Med | Low | Med |
| R-15 | Embedding skip silent data loss: `get_embedding` returns None for an entry that has a valid embedding (HNSW layer-0 miss — entry #1712); expanded entry silently excluded | Low | Low | Low |
| R-16 | Phase 0 insertion point wrong: Phase 0 runs after Phase 1 (personalization vector already built), expanded entries receive zero personalization mass — same failure mode as pre-crt-042 | High | Low | High |
| R-17 | S8 CoAccess directionality gap: S8 writes a < b single-direction; crt-035 promotion tick writes both directions, but any S8-only CoAccess pair (not yet promoted) is invisible from the higher-ID seed | Med | Low | Med |

---

## Risk-to-Scenario Mapping

### R-01: Flag-Off Regression (bit-identical)
**Severity**: High  
**Likelihood**: Med  
**Impact**: Every existing retrieval test fails or drifts silently; existing baselines invalidated; production behavior changes without eval gate.

**Test Scenarios**:
1. Run the full existing search integration test suite with `ppr_expander_enabled = false` (the default). Assert zero diff in result sets, scores, and ordering vs. pre-crt-042 baseline snapshots.
2. Construct a search request with `ppr_expander_enabled = false` where the graph contains entries reachable from seeds. Assert `results_with_scores` length equals the HNSW k=20 result count (no expansion happened).
3. Assert no timing `Instant::now()` is called on the flag-false path (confirming the guard is first in Phase 0, not after any side-effectful code).

**Coverage Requirement**: All existing integration tests must pass unchanged. A dedicated flag-off invariant test must confirm pool size is unchanged from HNSW output.

---

### R-02: S1/S2 Single-Direction Graph (blocking prerequisite)
**Severity**: High  
**Likelihood**: High  
**Impact**: Half the S1/S2 graph is invisible to Outgoing traversal. P@5 improvement is silently halved or zeroed for queries where the relevant entry is in the lower-ID position. Eval gate may pass vacuously on CoAccess edges alone.

**Test Scenarios**:
1. (Prerequisite gate — AC-00) Before Phase 0 code is written: query `GRAPH_EDGES` directly and confirm whether any `relation_type = 'Informs'` edge has a symmetric reverse partner. Count unidirectional vs. bidirectional Informs pairs. Document result.
2. (Post-gate) If back-fill was applied: assert that for a sample of S1/S2-sourced Informs edges, both `(a, b)` and `(b, a)` rows exist in `GRAPH_EDGES`.
3. Construct a unit test with an S1/S2-style graph (one direction only, A→B, seed=B). Assert `graph_expand` returns empty (confirming the behavior is understood) and document this as the failure mode the back-fill fixes.
4. Construct the same unit test after back-fill (both A→B and B→A). Assert `graph_expand({B})` returns `{A}`.

**Coverage Requirement**: A SQL query or integration fixture must confirm bidirectionality of Informs edges before the feature ships. The back-fill confirmation is a shipping gate, not a post-ship check.

---

### R-03: Quarantine Bypass
**Severity**: High  
**Likelihood**: Low  
**Impact**: Quarantined entries (potentially poisoned or flagged for security review) enter the result pool and are returned to callers.

**Test Scenarios**:
1. (AC-13 / AC-14) Construct a graph with seed B connected to entry Q (quarantined status). Assert Q is absent from `results_with_scores` after Phase 0 with expander enabled.
2. Verify the quarantine check is applied AFTER `entry_store.get()` and BEFORE `results_with_scores.push()` — not before the fetch and not after the push.
3. Assert that skipping Q produces no warning or error log (silent skip as specified by NFR-03).
4. Construct a two-hop scenario: seed → non-quarantined A → quarantined Q. Assert Q is absent, A is present.

**Coverage Requirement**: At least one integration test with a quarantined graph-reachable entry, asserting absence from results. Must cover both direct (1-hop) and transitive (2-hop) reachability of quarantined entries.

---

### R-04: O(N) Latency at Full Expansion
**Severity**: High  
**Likelihood**: Med  
**Impact**: With expander enabled, Phase 0 latency addition may exceed the ≤50ms-over-baseline gate at current corpus size (~7,000 entries, 200 expanded entries = ~1.4M f32 comparisons). Gate is a delta over the pre-crt-042 baseline (measure baseline with expander disabled in the same eval run), not an absolute P95. If flag is flipped to default-on without latency data, production search degrades.

**Test Scenarios**:
1. (AC-24) Assert that every search invocation with `ppr_expander_enabled = true` emits a `debug!` log line containing `elapsed_ms`, `seeds`, `expanded_raw`, and `added` fields.
2. Run the eval profile `ppr-expander-enabled.toml` against the live DB snapshot with `RUST_LOG=..search=debug`. First measure P95 with expander disabled (baseline). Then measure P95 with expander enabled. Compute the delta. Record against the ≤50ms-addition-over-baseline gate condition (ADR-005).
3. Assert that with `ppr_expander_enabled = false`, no `Instant::now()` call or timing log is emitted from Phase 0 (zero overhead on default path).
4. (Delivery investigation) Document whether `VectorIndex.id_map.entry_to_data` provides an O(1) embedding path. If yes, implement and add a test asserting the O(N) scan path is not used.

**Coverage Requirement**: Timing instrumentation must be tested for presence (log line emitted) and content (required fields). The latency gate measurement must be captured and recorded as part of feature completion evidence.

---

### R-05: Combined Ceiling Overflow
**Severity**: Med  
**Likelihood**: Low  
**Impact**: `results_with_scores` pool exceeds 270 entries before PPR scoring; PPR operates over a larger-than-documented pool; sorting and truncation still work but behavior is undocumented and untested.

**Test Scenarios**:
1. Construct a scenario where `graph_expand` returns exactly 200 entries (max_expansion_candidates hit), and Phase 5 attempts to inject 50 more PPR-only entries. Assert total pool size is at most 270 (20 HNSW + 200 Phase 0 + 50 Phase 5).
2. Assert that Phase 5 sees Phase 0 entries as already-in-pool (the `NOT in results_with_scores` check in Phase 5 covers Phase 0 entries).
3. Add an inline comment in the Phase 0 implementation documenting the 270-entry ceiling formula.

**Coverage Requirement**: One integration test exercising max_expansion_candidates cap with Phase 5 also active. Pool size assertion is mandatory.

---

### R-06: Back-fill Race During Eval
**Severity**: Med  
**Likelihood**: Med  
**Impact**: If the S1/S2 back-fill migration runs while the eval snapshot is being taken or during the eval run, edge counts are non-reproducible. MRR/P@5 measurements cannot be compared reliably against the baseline.

**Test Scenarios**:
1. Confirm the eval harness uses `unimatrix snapshot` (WAL-isolated DB copy) before `eval run`. The snapshot must be taken after the back-fill migration completes and committed — not during.
2. Assert the eval profile `ppr-expander-enabled.toml` uses the snapshot path, not the live DB.
3. Document in the delivery brief: "Back-fill migration must be committed and verified before the eval snapshot is taken."

**Coverage Requirement**: Procedural check — confirm snapshot is taken from a stable post-migration state. Add an assertion in the eval run script or delivery gate checklist.

---

### R-07: Eval Gate Failure
**Severity**: Med  
**Likelihood**: Med  
**Impact**: Feature ships behind the flag but P@5 shows no improvement. Flag cannot be enabled by default. The expander provides no measurable value without a clear diagnosis and owner.

**Test Scenarios**:
1. (AC-25 / AC-23) Run eval with expander enabled: assert MRR >= 0.2856 (no regression). Record P@5 value. Any increase above 0.1115 is the success signal.
2. Construct AC-25 regression test: an entry E with embedding dissimilar to query Q (outside HNSW k=20), connected by a positive edge to a seed. Assert E appears in results with expander on, absent with expander off.
3. If eval gate fails: check (a) S1/S2 bidirectionality confirmed, (b) Phase 0 insertion point is before Phase 1 (R-16), (c) BFS actually traverses edges (not empty result from degenerate graph). Document failure diagnosis.

**Coverage Requirement**: AC-25 cross-category integration test is mandatory and must be included in the test suite regardless of eval gate outcome. It is the behavioral proof that the architecture change works.

---

### R-08: InferenceConfig Hidden Test Sites
**Severity**: Med  
**Likelihood**: High (historical pattern — entries #4044, #2730, #4013)  
**Impact**: New fields `ppr_expander_enabled`, `expansion_depth`, `max_expansion_candidates` are added to the struct but not to test literal constructions. Tests compile but use stale struct literals; validation tests silently skip the new fields; default-value assertions miss.

**Test Scenarios**:
1. After adding the three new fields, `grep` the entire test suite for `InferenceConfig {` and `InferenceConfig::new(` literal constructions. Each must either include all three new fields or use `..Default::default()`.
2. Assert that `InferenceConfig::default()` returns `ppr_expander_enabled = false`, `expansion_depth = 2`, `max_expansion_candidates = 200` — matching the serde default functions atomically (entry #3817).
3. Assert that `InferenceConfig::merged()` correctly propagates all three new fields from project-level config override (three-level config merge pattern).
4. Confirm the serde default function return values match `Default::default()` values for all three fields.

**Coverage Requirement**: Grep-verified coverage of all `InferenceConfig` literal sites. Separate unit tests for each new field's default value and merge behavior.

---

### R-09: edges_of_type() Boundary Violation
**Severity**: Med  
**Likelihood**: Low  
**Impact**: Direct petgraph API calls bypass the sole traversal boundary established in crt-030 (entry #3627). Future edge type additions (entry #3950) silently drop the new type from expansion traversal.

**Test Scenarios**:
1. (AC-16) Code inspection: `grep` `graph_expand.rs` for `.edges_directed(` and `.neighbors_directed(`. Assert zero matches.
2. Add a new edge type in a test-only graph and confirm `graph_expand` does not traverse it (excluded type silently excluded).
3. Confirm the module-level doc comment in `graph_expand.rs` states the `edges_of_type()` invariant.

**Coverage Requirement**: Code inspection test (grep-based). At least one unit test verifying excluded edge types (Supersedes, Contradicts) are not traversed.

---

### R-10: Timing Instrumentation — Wrong Level or Absent
**Severity**: Med  
**Likelihood**: Med  
**Impact**: If instrumentation is absent, latency gate cannot be measured. If at `info!` level, production logs are flooded on every search request.

**Test Scenarios**:
1. (AC-24) Assert the `debug!` trace is emitted during a test search with `ppr_expander_enabled = true` using a tracing subscriber that captures debug-level events.
2. Assert the trace contains all required fields: seed count, expanded_raw count, added count, elapsed_ms.
3. Assert the trace is NOT emitted when `ppr_expander_enabled = false`.
4. Assert the macro used is `tracing::debug!`, not `tracing::info!` or `tracing::warn!`.

**Coverage Requirement**: At least one test using a tracing test subscriber (e.g., `tracing-test` crate) to assert the debug event is emitted with correct fields. This is a behavioral test of instrumentation, not just a compile-time check. Note: entry #3935 documents a gate failure where tracing tests were deferred — do not defer this.

---

### R-11: BFS Visited-Set Missing
**Severity**: Med  
**Likelihood**: Low  
**Impact**: Bidirectional CoAccess edges (A→B and B→A) cause the BFS to oscillate. Without a visited set, A expands to B, B expands back to A, A expands to B again... until max_candidates is hit with duplicates or a stack overflow occurs.

**Test Scenarios**:
1. Construct a graph with bidirectional edges (A↔B) and seed {A} at depth=2. Assert `graph_expand` returns `{B}` (not `{A, B, A, B, ...}`) and terminates.
2. Construct a graph with a cycle (A→B→C→A) and seed {A} at depth=3. Assert termination and no duplicate IDs in the result set.
3. Assert `graph_expand` terminates in bounded time for any graph with `max_candidates=200`.

**Coverage Requirement**: At least one cycle test and one bidirectional-edge test. Both must assert termination and absence of duplicate IDs.

---

### R-12: Seed Exclusion Failure
**Severity**: Med  
**Likelihood**: Low  
**Impact**: Seeds appear in the `graph_expand` return set. Phase 0 adds them to `results_with_scores` a second time, creating duplicate entries that inflate Phase 1 personalization mass for seed entries.

**Test Scenarios**:
1. (AC-08) Construct a graph with edge A→B and seeds {A, B}. Assert neither A nor B appears in the `graph_expand` return set.
2. Construct a graph where a seed has a self-loop (A→A). Assert A does not appear in the return set.
3. After Phase 0, assert no entry ID appears in `results_with_scores` more than once.

**Coverage Requirement**: Explicit seed-exclusion unit test. Post-Phase-0 deduplication invariant assertion.

---

### R-13: Determinism Failure
**Severity**: Med  
**Likelihood**: Low  
**Impact**: Non-deterministic BFS frontier ordering makes results irreproducible. Eval runs produce different MRR/P@5 measurements on the same snapshot. Unit tests are flaky.

**Test Scenarios**:
1. (NFR-04) Call `graph_expand` twice with identical inputs. Assert the return `HashSet` contains the same IDs on both calls.
2. Construct a graph where two paths reach different entries at the same hop depth. Assert the set of returned entries is consistent across 100 calls.
3. Assert no `HashMap::iter()` or unordered iteration drives the BFS frontier selection within the candidate cap.

**Coverage Requirement**: Determinism property test (multi-call assertion). Frontier ordering must be tested with a graph where budget-boundary behavior is exercised.

---

### R-14: Config Validation Conditional Gap
**Severity**: Med  
**Likelihood**: Low (NLI trap is documented — ADR-004 explicitly addresses this)  
**Impact**: `expansion_depth = 0` or `max_expansion_candidates = 0` are accepted at server start when `ppr_expander_enabled = false`. When operator flips the flag in production, the server processes queries with depth=0 (returns empty expansion every time) or max=0 (returns empty expansion every time), silently producing zero benefit.

**Test Scenarios**:
1. (AC-18) Assert `InferenceConfig::validate()` returns error for `expansion_depth = 0` when `ppr_expander_enabled = false`.
2. (AC-19) Assert `InferenceConfig::validate()` returns error for `expansion_depth = 11` when `ppr_expander_enabled = false`.
3. (AC-20) Assert `InferenceConfig::validate()` returns error for `max_expansion_candidates = 0` when `ppr_expander_enabled = false`.
4. (AC-21) Assert `InferenceConfig::validate()` returns error for `max_expansion_candidates = 1001` when `ppr_expander_enabled = false`.

**Coverage Requirement**: Four explicit validation unit tests, each with `ppr_expander_enabled = false`. This is the critical difference from the NLI pattern (entry #3817).

---

### R-15: get_embedding Layer-0 Miss
**Severity**: Low  
**Likelihood**: Low (crt-014 fixed this for the tick path — entry #1724)  
**Impact**: An entry with a valid embedding assigned to a non-zero HNSW layer is not returned by `get_embedding()` if it uses `get_layer_iterator(0)` instead of `IntoIterator`. Entry is silently excluded from expansion.

**Test Scenarios**:
1. Confirm `vector_store.get_embedding()` uses `IntoIterator` over `&PointIndexation` (all layers), not `get_layer_iterator(0)`. Code inspection test.
2. (AC-15) Construct a scenario where an expanded entry has no stored embedding. Assert it is silently skipped.

**Coverage Requirement**: Code path inspection confirming the crt-014 fix (entry #1724) applies on the search path. One skip-on-None test.

---

### R-16: Phase 0 Insertion Point Wrong
**Severity**: High  
**Likelihood**: Low  
**Impact**: If Phase 0 runs after Phase 1 (personalization vector already built), expanded entries receive zero personalization mass — identical to the pre-crt-042 failure mode. The feature appears to ship but produces no retrieval improvement.

**Test Scenarios**:
1. (AC-02) Assert that after Phase 0 and before Phase 1, `results_with_scores` contains entries not present in the original HNSW output.
2. Assert the Phase 1 seed_scores `HashMap` includes entries sourced from Phase 0 expansion (i.e., expanded entries have non-zero scores in the personalization vector).
3. Code inspection: confirm Phase 0 block is the first block inside the `if !use_fallback` branch in Step 6d, before any `seed_scores` construction.

**Coverage Requirement**: Integration test asserting expanded entries appear in Phase 1's input. This is the core correctness invariant of the architecture.

---

### R-17: S8 CoAccess Directionality Gap
**Severity**: Med  
**Likelihood**: Low  
**Impact**: S8 writes `a = min(ids)` → `b = max(ids)` (single direction). The crt-035 promotion tick writes both directions for promoted CoAccess pairs. Any S8-only CoAccess pair not yet promoted by the tick is only reachable from the lower-ID seed, not the higher-ID seed. Traversal from the higher-ID seed finds no CoAccess edges to the lower-ID partner.

**Test Scenarios**:
1. Query `GRAPH_EDGES` for `relation_type = 'CoAccess'` rows. For each `(a, b)` row, assert the reverse `(b, a)` row also exists (or is covered by the crt-035 tick). Document any gaps.
2. Construct a unit test with an S8-style unidirectional CoAccess edge (A→B only, A < B). Seed {B}. Assert `graph_expand` does not return A (confirming the gap is understood). Seed {A}. Assert `graph_expand` returns B. Document this as the condition where crt-035 tick coverage is required.

**Coverage Requirement**: Directionality verification query at delivery time. Document whether S8-only pairs need the back-fill or tick coverage.

---

## Integration Risks

**Phase 0 ↔ Phase 1 boundary**: Phase 0 must complete and populate `results_with_scores` before Phase 1 reads it to build `seed_scores`. Any refactoring that inlines or reorders these phases destroys the personalization mass guarantee for expanded entries.

**Phase 0 ↔ Phase 5 disjointness**: Phase 5 must treat Phase 0 entries as already-in-pool. The `NOT in results_with_scores` check must cover Phase 0 entries. If the check uses a stale snapshot of entry IDs taken before Phase 0, Phase 5 re-injects Phase 0 entries and the 270-ceiling is violated.

**Phase 0 ↔ Step 6c co-access prefetch ordering**: Per ADR-002, Phase 0 runs before Step 6c. If ordering is inverted (Phase 0 after 6c), expanded entries get `coac_norm = 0.0` in fused scoring — silent loss of co-access boost signal for all expanded entries.

**`use_fallback` guard**: Phase 0 lives inside the `if !use_fallback` block. When PPR is disabled entirely (`use_fallback = true`), Phase 0 must not execute. Both the PPR-disabled and expander-disabled paths must be tested.

**`edges_of_type()` extension gap** (entry #3950): Adding a new `RelationType` variant in a future feature requires updating `graph_expand`'s positive-edge-type list or the new type is silently excluded from expansion. The list of positive edge types in `graph_expand` must be explicitly maintained alongside `RelationType` changes.

---

## Edge Cases

- `seed_ids` is empty (e.g., HNSW returns zero results): `graph_expand` must return empty set immediately without panicking.
- Graph has no edges (dense graph with no edges between nodes): BFS terminates immediately; `graph_expand` returns empty set.
- `depth = 0`: returns empty set immediately (AC-12).
- `max_candidates = 1`: returns at most one entry; BFS terminates after first reachable entry.
- All expanded entries are quarantined: Phase 0 adds zero entries; `results_with_scores` equals HNSW-only results.
- All expanded entries have no stored embedding: same as quarantine case above.
- `max_expansion_candidates = 200` hit at depth 1 (highly connected seeds): depth 2 is never explored. This is correct behavior per the early-exit contract.
- Seed with no outgoing positive edges: contributes zero entries to expansion; BFS terminates for that seed immediately.
- Graph with >270 reachable entries from seeds: Phase 0 caps at 200, Phase 5 caps at 50. Total pool cap of 270 is enforced by the independent caps of each phase, not by a combined ceiling check.
- Corpus growth: at 70,000 active entries, 200 × O(70,000) = 14M comparisons. The latency gate (P95 < 50ms at 7k corpus) does not automatically hold at 10× scale. Corpus size must be recorded alongside latency measurements.

---

## Security Risks

**Untrusted input surface**: `graph_expand` receives `seed_ids` derived from HNSW results. HNSW results come from the query embedding, which originates from user-supplied text. A user cannot directly control `seed_ids` values — they are entry IDs from the database, not user-supplied integers. No injection risk in the BFS itself.

**Quarantine bypass (R-03)**: The primary security risk. Graph edges can point to quarantined entries. The quarantine check in Phase 0 (`SecurityGateway::is_quarantined`) is the sole security enforcement point for expanded entries. If this check is missing, reordered, or applied to the wrong status field, quarantined entries enter results. The check is in `search.rs` (caller responsibility), not in `graph_expand` itself — this separation creates a security contract gap if `graph_expand` is called from other sites.

**Future caller risk** (SR-07): Any future caller of `graph_expand` outside `search.rs` that omits the quarantine check silently leaks quarantined entries. The `graph_expand` function has no internal security enforcement by design (pure function contract). The module-level doc comment must document this caller obligation explicitly (FR-06).

**Blast radius**: `graph_expand` operates on the in-memory `TypedRelationGraph` (read-only, cloned). No write operations. No SQL queries. No file I/O. The blast radius of a bug in `graph_expand` is limited to result-set correctness — it cannot corrupt storage or crash the server.

---

## Failure Modes

**Expander flag off (expected path)**: Phase 0 guard evaluates to false, returns immediately. Zero latency addition. Zero change to results. All downstream phases operate identically to pre-crt-042.

**`entry_store.get()` fails for an expanded ID**: Skip the entry silently (same pattern as Phase 5). Do not fail the search request. Log at `debug!` level if useful for diagnosis.

**`vector_store.get_embedding()` returns None**: Skip the entry silently (AC-15). The entry has no retrievable cosine score and cannot participate meaningfully in personalization.

**`graph_expand` returns empty set** (degenerate cases): Phase 0 adds zero entries. Pipeline continues with HNSW-only seeds. No error.

**All expanded entries are quarantined or embedding-missing**: Phase 0 adds zero entries after filtering. Same as above. No error, no log at warn/error level.

**Config validation failure at server start** (`expansion_depth = 0`): Server refuses to start. Error message identifies the invalid field and expected range. This is the intended failure mode — caught at startup, not at query time.

**Eval gate fails (MRR regression)**: Feature flag remains `false` by default. The flag exists precisely to prevent this from being user-visible. Investigation required before flag can be enabled. Owner must be named in the delivery brief (SR-05).

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: get_embedding() O(N) — 200 × O(N) latency | R-04 | Architecture wires `debug!` timing instrumentation (ADR-005). P95 < 50ms gate defined before default enablement. Delivery agent investigates O(1) path via `id_map.entry_to_data`. |
| SR-02: BFS sorted node-ID bias toward older entries | R-13 | Accepted as documented limitation (SPEC C-09). Determinism is tested; bias is documented for post-measurement follow-up. |
| SR-03: S1/S2 single-direction Informs edges | R-02 | Hard blocking gate (AC-00): delivery agent must confirm directionality before Phase 0 code is written. If single-direction, back-fill issue filed before crt-042 ships. |
| SR-04: Phase 0 + Phase 5 combined ceiling (270) unspecified | R-05 | Architecture documents 270-entry ceiling explicitly (ARCHITECTURE.md + NFR-08). Phase 5 sees Phase 0 entries as in-pool via `results_with_scores`. Test coverage required. |
| SR-05: Eval gate failure with no owner | R-07 | Delivery brief must name an owner and decision timeline for eval gate failure scenario. The risk is accepted but must not be unowned. |
| SR-06: Direction semantics ambiguity (entry #3754) | — | Fully addressed in architecture. ADR-006 specifies traversal behaviorally. All ACs in spec are behavioral (no Direction:: references). No residual testing risk — direction is verified by behavioral outcome tests. |
| SR-07: Quarantine caller responsibility gap | R-03 | FR-06 documents the caller contract explicitly. `graph_expand` module doc comment states the obligation. Tests verify quarantine is enforced in Phase 0 and that `graph_expand` itself is pure (no quarantine check inside). |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 7 scenarios minimum; R-02 is a blocking gate before implementation |
| High | 6 (R-03, R-04, R-05, R-06, R-07, R-08, R-16) | 20 scenarios minimum across all High risks |
| Med | 8 (R-09–R-14, R-17) | 16 scenarios minimum |
| Low | 1 (R-15) | 2 scenarios minimum |

**Non-negotiable tests** (gate blockers per entry #2758 / #3579 pattern):
- AC-01 flag-off regression test (existing suite must pass unchanged)
- AC-14 quarantine bypass test (explicit fixture required)
- AC-24 timing instrumentation emission test (tracing subscriber required — do not defer per entry #3935)
- AC-25 cross-category behavioral regression test (the core feature proof)
- AC-18/19/20/21 config validation tests (four tests, unconditional validation confirmed)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection"` — found entries #3579 (Gate 3b: zero test modules), #2758 (Gate 3c: grep non-negotiable tests), #3935 (tracing test AC deferred at Gate 3b). All directly inform mandatory test requirements.
- Queried: `/uni-knowledge-search` for `"risk pattern search pipeline PPR graph traversal"` — found entries #3730 (pipeline pattern), #3740 (submodule pattern), #3744 (PPR direction trap), #3896 (PPR regression test trap), #3950 (RelationType extension checklist).
- Queried: `/uni-knowledge-search` for `"SQLite graph edges directionality migration back-fill"` — found entries #3889 (back-fill pattern), #3891 (ADR-006 directionality decision).
- Queried: `/uni-knowledge-search` for `"get_embedding O(N) latency"` — found entries #1712 (layer-0 miss bug), #1724 (IntoIterator fix).
- Queried: `/uni-knowledge-search` for `"InferenceConfig validation hidden test sites"` — found entries #4044, #2730, #3817, #3769, #4013. All inform R-08 and config test requirements.
- Stored: nothing novel to store — all risk patterns identified (hidden test sites, quarantine caller contract, tracing test deferral trap) are already in Unimatrix. The combination of O(N) expansion + feature flag + latency gate is specific to crt-042 and not yet a cross-feature pattern.
