# Risk-Based Test Strategy: crt-041

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Dual-endpoint quarantine guard missing on one JOIN — silently writes edges to quarantined entries | High | Med | Critical |
| R-02 | S2 vocabulary term interpolated into SQL string rather than bound — SQL injection via config.toml | High | Med | Critical |
| R-03 | InferenceConfig `impl Default` and serde `default_*()` functions diverge on one or more of the 5 new fields | High | High | Critical |
| R-04 | S1 GROUP BY materializes full Cartesian pair set before LIMIT fires — OOM or multi-second stall at large corpus (OQ-01) | Med | Med | High |
| R-05 | S8 watermark stuck behind a single malformed-JSON `audit_log` row — perpetual re-scan of same event_id | Med | Low | High |
| R-06 | S8 watermark written before edge writes complete — crash window permanently skips a batch | Med | Low | High |
| R-07 | S1/S2 edges written with `source='nli'` because `write_nli_edge` is reused instead of `write_graph_edge` — silent GNN feature corruption | High | Low | High |
| R-08 | crt-040 prerequisite absent at delivery start — `write_graph_edge` not present, call sites written against wrong function | Med | Med | High |
| R-09 | Orphaned S1/S2 edges accumulate after tag or vocabulary change — PPR traverses stale connections (OQ-03) | Med | Low | Med |
| R-10 | S8 batch cap applied on audit_log rows rather than pairs — large search events (k=20) yield 190 pairs per row, cap bypassed | Med | Med | Med |
| R-11 | S2 false-positive matches: raw substring `instr()` without space-padding matches "api" inside "capabilities" | Med | Med | Med |
| R-12 | S8 processes `context_briefing` or failed-search (`outcome != 0`) rows — low-quality edges injected into graph | Low | Low | Med |
| R-13 | `inferred_edge_count` in `GraphCohesionMetrics` incorrectly counts S1/S2/S8 edges — metric semantics break for downstream tooling | Med | Low | Med |
| R-14 | S2 with empty vocabulary emits SQL error or panics instead of returning no-op | Low | Med | Med |
| R-15 | Eval gate run before the first `TypedGraphState::rebuild` tick — S1/S2/S8 edges written but not yet visible in PPR, false gate failure (SR-09 / OQ-04) | Low | Med | Low |
| R-16 | `graph_enrichment_tick.rs` exceeds 500-line limit — workspace rule violation | Low | Low | Low |
| R-17 | `validate()` missing range check for a zero-value cap field — `LIMIT 0` silently disables a source or `% 0` panics at runtime | Med | Low | Med |

---

## Risk-to-Scenario Mapping

### R-01: Dual-endpoint quarantine guard missing on one JOIN

**Severity**: High  
**Likelihood**: Med  
**Impact**: Quarantined entries silently appear as edge endpoints. PPR traverses them. On graph-rebuild the traversal produces results from entries operators have explicitly removed. Historical precedent: production bug documented in entry #3981 (co_access_promotion_tick).

**Test Scenarios**:
1. Corpus with a qualifying S1 pair where one entry is Quarantined (status=3) — run S1, assert zero edges written for any pair containing that entry.
2. Same test for S2: vocabulary-matching pair where one entry is Quarantined, assert no edge.
3. Same test for S8: audit_log row with target_ids containing a Quarantined entry ID — assert that pair is not inserted.
4. Regression: add an Active entry that becomes Quarantined mid-session; run the tick again; assert no new edge to the now-quarantined entry.

**Coverage Requirement**: Integration tests required for all three sources (S1, S2, S8) with quarantined-endpoint fixtures. Each test must verify BOTH endpoint positions (entry as source_id AND as target_id) are filtered.

---

### R-02: S2 SQL injection via vocabulary term

**Severity**: High  
**Likelihood**: Med  
**Impact**: A vocabulary term containing `'`, `--`, or `; DROP TABLE` could corrupt or destroy the database. The operator controls config.toml — but defense-in-depth is mandatory per ADR-002 (SR-01 mitigation).

**Test Scenarios**:
1. Configure `s2_vocabulary` with a term containing a single quote: `"it's"`. Run the S2 tick. Assert no SQL error occurs and results are semantically correct (the term matches entries containing `it's` as a word).
2. Configure `s2_vocabulary` with a term containing `--` (SQL comment). Run S2. Assert no SQL error and no unexpected rows are produced.
3. Code review gate: assert no vocabulary term is ever concatenated into a SQL string literal (verify `push_bind` is used at every construction point).

**Coverage Requirement**: Unit test for the SQL construction path with adversarial vocabulary terms. The SECURITY comment at the `push_bind` site must be present in the code review checklist.

---

### R-03: InferenceConfig dual-site default divergence

**Severity**: High  
**Likelihood**: High  
**Impact**: `InferenceConfig::default()` and TOML deserialization of an absent field return different values. Tests that use `InferenceConfig::default()` observe different behavior than a production deployment with an empty config.toml. This was a concrete bug in crt-032 (entry #3817). High likelihood because crt-040 and crt-041 both add fields in the same delivery window.

**Test Scenarios**:
1. `test_inference_config_s1_s2_s8_defaults_match_serde`: for each of the five new fields, assert `InferenceConfig::default().field == toml::from_str("").unwrap().field`. This is the pre-delivery verification test mandated by ADR-005.
2. Assert `InferenceConfig::default().s2_vocabulary` is an empty Vec (not the 9-term list), matching the SCOPE.md §Design Decision 3 resolution.
3. Assert the four numeric fields (`max_s1_edges_per_tick=200`, `max_s2_edges_per_tick=200`, `s8_batch_interval_ticks=10`, `max_s8_pairs_per_batch=500`) match in both paths.

**Coverage Requirement**: The serde-match test must run in the `config.rs::tests` module and be part of the mandatory pre-PR verification step. Failure here blocks delivery.

---

### R-04: S1 GROUP BY full materialization at large corpus (OQ-01)

**Severity**: Med  
**Likelihood**: Med  
**Impact**: At 10,000 entries with dense tags, the entry_tags self-join Cartesian product before GROUP BY could be millions of intermediate rows, causing a tick that takes multiple seconds or exhausts SQLite's temporary storage. The LIMIT cap does not help if it fires after GROUP BY.

**Test Scenarios**:
1. Integration test with 200 entries and 5+ tags each — measure S1 tick wall-clock time. Assert completion within 500ms (NFR-03).
2. `EXPLAIN QUERY PLAN` output for the S1 SQL — verified in the implementation brief or an architectural note that LIMIT + ORDER BY fires before full GROUP BY materialization, or that the query is restructured as a two-phase pre-filter.
3. If the two-phase restructuring is chosen (per OQ-01 resolution): test both phases independently — pre-filter finds all pairs with ≥1 shared tag, scoring phase finds pairs with ≥3.

**Coverage Requirement**: At minimum one timing integration test (NFR-03: ≤500ms at ≤1,200 entries). The OQ-01 query-plan verification must be documented in the implementation brief before delivery; it is not purely a test artifact.

---

### R-05: S8 watermark stuck on malformed JSON row

**Severity**: Med  
**Likelihood**: Low  
**Impact**: An `audit_log` row written by a prior bug with `target_ids = 'not-json'` permanently prevents the watermark from advancing past that event_id. Every subsequent S8 batch re-encounters the malformed row, logs a warn!, and makes no forward progress.

**Test Scenarios**:
1. Insert three audit_log rows: rows 1 and 3 are valid `context_search` rows; row 2 has `target_ids = 'not-json'`. Run S8. Assert: row 1 and row 3's valid pairs are written as edges, a `warn!` is emitted for row 2, and the watermark advances to 3 (not stuck at 1).
2. Run S8 a second time on the same state. Assert zero new edges written (idempotency) and watermark remains 3.

**Coverage Requirement**: Integration test with a malformed-JSON fixture between two valid rows. Watermark state must be inspected directly from the `counters` table after the test.

---

### R-06: S8 watermark written before edge writes

**Severity**: Med  
**Likelihood**: Low  
**Impact**: If the process crashes between watermark update and edge writes, that batch is permanently lost without error. Silent data loss: co-retrieval signal is discarded.

**Test Scenarios**:
1. Test the write ordering contract: run S8 with a mock that records the order of SQL statements executed — assert `counters::set(watermark)` is called AFTER all `write_graph_edge` calls for the batch.
2. Simulate crash recovery: manually set the watermark to a value lower than already-written batch event_ids, run S8 again, assert no duplicate edges are written (INSERT OR IGNORE handles idempotency) and watermark advances correctly.

**Coverage Requirement**: The ordering invariant must be tested at the integration level. The mock-ordering test covers the crash-between-steps scenario.

---

### R-07: S1/S2/S8 edges silently tagged with source='nli'

**Severity**: High  
**Likelihood**: Low  
**Impact**: If `write_nli_edge` is called instead of `write_graph_edge`, edges are tagged `source='nli'`. GNN feature construction (W3-1) assigns wrong edge-type features. `inferred_edge_count` is inflated. The corruption is silent — no runtime error occurs. Discovered only when GNN training uses stale edge labels.

**Test Scenarios**:
1. After running S1, S2, S8 ticks on a synthetic corpus, query `graph_edges WHERE source = 'S1'` and assert count > 0. Assert `graph_edges WHERE source = 'nli'` count is unchanged (no NLI edges added in the test).
2. Assert `EDGE_SOURCE_S1`, `EDGE_SOURCE_S2`, `EDGE_SOURCE_S8` constants have values `"S1"`, `"S2"`, `"S8"` respectively (compilation-level constant value test, AC-22).

**Coverage Requirement**: Source-value assertion in every S1/S2/S8 integration test. Named constant values checked in a unit test.

---

### R-08: crt-040 prerequisite absent at delivery

**Severity**: Med  
**Likelihood**: Med  
**Impact**: If `write_graph_edge(source: &str, ...)` does not exist in `nli_detection.rs`, the delivery agent either (a) calls `write_nli_edge` (silently retags edges — R-07), or (b) is blocked waiting for unscoped work. Either path introduces a delivery risk.

**Test Scenarios**:
1. Pre-delivery gate: `grep -n "pub(crate) async fn write_graph_edge"` in `nli_detection.rs` must return a match. This is a delivery pre-flight check, not a runtime test.
2. If the function is added by crt-041: write a unit test asserting `write_graph_edge` with `source="test_src"` writes a row with that source value and `write_nli_edge` still writes `source='nli'` — confirming no retag regression.

**Coverage Requirement**: Pre-flight check is mandatory (ADR-001, C-06). If the function is added in crt-041, its delegation contract with `write_nli_edge` must be integration-tested.

---

### R-09: Orphaned S1/S2 edges from stale tag/vocabulary overlap (OQ-03) — CLOSED

**Resolution**: Verified in `background.rs:513-515`. The compaction SQL is source-agnostic:
`DELETE FROM graph_edges WHERE source_id NOT IN (...) OR target_id NOT IN (...)`. No filter
on `source` column — S1/S2/S8 edges are removed whenever their endpoint is quarantined or
deleted, exactly like NLI and co_access edges. Risk is closed; no test scenario required
beyond the endpoint-quarantine guard already tested in AC-03/AC-08/AC-14.

---

### R-10: S8 batch cap semantics — cap on rows not pairs

**Severity**: Med  
**Likelihood**: Med  
**Impact**: If `max_s8_pairs_per_batch` is applied as `LIMIT ?cap` on audit_log rows rather than on accumulated pairs, a single search event returning 20 results (190 pairs) exhausts the cap in one row. The remaining cap allocation is never used. For the default cap=500, the first 3 rows consume 570 pairs — the cap is meaningless.

**Test Scenarios**:
1. Set `max_s8_pairs_per_batch = 5`. Insert one audit_log row with `target_ids` producing 10 pairs (e.g., 5 entry IDs: 10 pairs). Run S8. Assert exactly 5 edges are written (cap on pairs, not rows).
2. Set `max_s8_pairs_per_batch = 3`. Insert two audit_log rows each with 6 entries (15 pairs each). Run S8. Assert at most 3 edges written and watermark advances to the last fully-processed row's event_id (ADR-003: partial-row watermark semantics).

**Coverage Requirement**: AC-21 covers the basic case. The partial-row watermark test (scenario 2) is the edge case that is otherwise untested.

---

### R-11: S2 false-positive substring matches

**Severity**: Med  
**Likelihood**: Med  
**Impact**: Without space-padding, `instr(lower(text), lower("api"))` matches "capabilities", "rapid", "mapping". Edges are written between unrelated entries. PPR traverses false connections, degrading retrieval quality.

**Test Scenarios**:
1. AC-10 (already in spec): term `"api"` in vocabulary, entry with content `"capabilities only"` — assert no edge written.
2. Term `"cache"` in vocabulary, entry with content `"cached"` — assert no edge written (suffix match suppressed by space-padding).
3. Positive case: term `"api"` in vocabulary, entry with content `"the api is documented here"` — assert edge IS written.

**Coverage Requirement**: At least one false-positive suppression test (scenario 1 or 2) and one true-positive test (scenario 3). Both are required to confirm the space-padding boundary works in both directions.

---

### R-12: S8 processing wrong operation types or failed searches

**Severity**: Low  
**Likelihood**: Low  
**Impact**: `context_briefing` results are injected as CoAccess edges. Briefing retrievals are lexically different from user-initiated searches — the behavioral signal is semantically wrong. Failed searches (`outcome != 0`) were denied or errored; their result sets are invalid.

**Test Scenarios**:
1. AC-17: `context_briefing` row with target_ids — assert zero S8 edges written for that row.
2. AC-18: `context_search` row with `outcome = 1` — assert zero S8 edges written.
3. Positive: `context_search` row with `outcome = 0` — assert edges are written.

**Coverage Requirement**: AC-17 and AC-18 from the spec cover this directly.

---

### R-13: `inferred_edge_count` incorrectly counts S1/S2/S8 edges

**Severity**: Med  
**Likelihood**: Low  
**Impact**: The `inferred_edge_count` field is defined as counting only `source='nli'` edges (NFR-06). If the query is inadvertently broadened (e.g., `WHERE source != ''` instead of `WHERE source = 'nli'`), metric semantics break. Downstream tooling depending on `inferred_edge_count` to represent NLI confidence infers incorrectly.

**Test Scenarios**:
1. AC-30: insert edges for all four sources (S1, S2, S8, NLI). Assert `inferred_edge_count` equals only the NLI-source count.
2. Run `compute_graph_cohesion_metrics()` before and after S1/S2/S8 ticks — assert `inferred_edge_count` is unchanged (it should not change since S1/S2/S8 write non-NLI sources).

**Coverage Requirement**: AC-30 is the primary test. Scenario 2 provides pre/post regression coverage.

---

### R-14: S2 with empty vocabulary errors or panics

**Severity**: Low  
**Likelihood**: Med  
**Impact**: An empty vocabulary produces zero CASE WHEN expressions in the dynamically constructed SQL, resulting in a syntactically invalid query. If the early-return guard is absent or bypassed, the SQL execution returns an error or panics.

**Test Scenarios**:
1. AC-07: set `s2_vocabulary = []`, run S2 tick, assert zero rows in `graph_edges` with `source='S2'` and no panic.
2. Set `s2_vocabulary = []` and inject a pool that would error if queried — assert S2 does not even call the pool (pure early-return).

**Coverage Requirement**: AC-07 covers the no-op contract. Scenario 2 is optional but adds confidence that no SQL is issued at all.

---

### R-15: Eval gate run before TypedGraphState::rebuild tick (SR-09 / OQ-04)

**Severity**: Low  
**Likelihood**: Med  
**Impact**: S1/S2/S8 write edges after `TypedGraphState::rebuild` has already run in the same tick. New edges are visible in `graph_edges` immediately but not in the PPR graph until the next tick's rebuild. Running the eval gate before the next tick shows edges in `graph_edges` but MRR is unchanged — a misleading but not broken state.

**Test Scenarios**:
1. NFR-08: the eval gate procedure must require at least one complete tick post-delivery. Verify the implementation brief or CI gate script enforces this wait.
2. Integration test: run S1 tick, immediately call `compute_graph_cohesion_metrics()` — assert `cross_category_edge_count` is updated (edges are in the table). This confirms the metrics read from `graph_edges` directly and do not require a PPR rebuild.

**Coverage Requirement**: The tick-wait requirement must be in the delivery instructions (AC-32). Scenario 2 confirms cohesion metrics are readable without waiting for a PPR rebuild.

---

### R-16: `graph_enrichment_tick.rs` file size violation

**Severity**: Low  
**Likelihood**: Low  
**Impact**: Workspace 500-line rule violation. Test splits are specified in ADR-001 and AC-31.

**Test Scenarios**:
1. AC-31: PR review step — `wc -l crates/unimatrix-server/src/services/graph_enrichment_tick.rs` must be ≤500 (excluding test file). This is a delivery gate check, not a runtime test.

**Coverage Requirement**: Checked at PR review. No runtime test needed.

---

### R-17: `validate()` missing range check — zero-value cap causes LIMIT 0 or modulo panic

**Severity**: Med  
**Likelihood**: Low  
**Impact**: A zero value for `max_s1_edges_per_tick` (or any of the four numeric fields) produces `LIMIT 0`, silently disabling the source. A zero `s8_batch_interval_ticks` causes `tick % 0` — integer division by zero, panic at runtime. Historical precedent: entry #3766 documents a missing validate() that silently disabled a feature via LIMIT 0.

**Test Scenarios**:
1. AC-24: set each of the four numeric fields to 0, assert `InferenceConfig::validate()` returns an error naming the field.
2. Set `s8_batch_interval_ticks = 0` in a constructed config (bypassing validate()), call `run_single_tick` — assert a panic does NOT occur (i.e., validate() prevents this config from reaching the tick). This validates the validate()-before-tick contract.

**Coverage Requirement**: AC-24 covers the validate() path. Scenario 2 confirms the startup guard prevents zero values from reaching tick execution.

---

## Integration Risks

**S1 self-join index coverage**: The S1 query joins `entry_tags` twice on `tag`. The index `idx_entry_tags_tag` exists. If the query planner does not use it for the self-join on equality, performance degrades. Risk is low given the index exists, but should be confirmed via `EXPLAIN QUERY PLAN` in the implementation brief.

**S8 bulk pair-ID validation query**: ADR-003 specifies a bulk `SELECT id FROM entries WHERE id IN (...) AND status != 3` for quarantine filtering (Option B). The sqlx variable binding limit applies — if a search result set has 20 IDs and multiple rows are batched, the IN clause could exceed 999 parameters (SQLite max). Entry #3442 documents the chunked batch IN-clause pattern for exactly this scenario. The implementation must use chunked batches if the ID set can exceed the SQLite limit.

**S8 partial-row watermark on batch cap**: When the `max_s8_pairs_per_batch` cap is reached mid-row, the watermark must advance only to the last fully-processed row's event_id, not to the partially-processed row. Incorrect watermark on partial processing causes silent re-processing gaps (some pairs from that row written, rest skipped on next batch). This is the ADR-003 partial-row semantics and is the most complex ordering invariant in S8.

**write_graph_edge signature compatibility**: The architecture doc specifies the signature as `(store: &Store, source_id: u64, target_id: u64, relation_type: &str, weight: f64, created_at: u64, source: &str, metadata: Option<&str>) -> bool`. If crt-040 shipped with a different signature, crt-041 call sites will fail to compile. Delivery agent must confirm the exact signature before writing call sites.

**Tick ordering: S1/S2 before TypedGraphState::rebuild on NEXT tick**: S1/S2 write Informs edges. `TypedGraphState` uses Informs edges for PPR traversal. Since rebuild runs BEFORE S1/S2 in the same tick, new S1/S2 edges are always one tick behind. This is accepted behavior but must not be accidentally "fixed" by moving S1/S2 before graph-rebuild, which would break the tick ordering invariant.

---

## Edge Cases

**S1 — entry with exactly 3 shared tags**: The HAVING threshold is `>= 3`. An entry pair with exactly 3 shared tags must produce an edge; a pair with 2 must not. Off-by-one in the HAVING clause would silently under- or over-generate edges.

**S1 — weight capping at 1.0**: A pair with 11 shared tags should get weight=1.0 (not 1.1). Test that `min(shared_tag_count * 0.1, 1.0)` is applied in Rust after the query result, not that the SQL formula itself is truncated.

**S2 — pair with exactly 2 vocabulary term matches (one per side)**: ADR-002 specifies "pair must total ≥2 matches across both entries." A pair where entry e1 has 1 match and e2 has 1 match (total = 2) qualifies. A pair where only e1 has 2 matches and e2 has 0 also qualifies (total = 2). Both must produce edges.

**S2 — vocabulary term longer than any content word**: A term like `"authentication-service-v2"` with hyphens will not word-boundary match with space-padding unless the content also uses spaces around the compound. This is expected behavior, but callers using hyphenated terms should be warned in documentation.

**S8 — singleton target_ids list**: `target_ids = '[42]'` produces zero pairs (N=1 → N*(N-1)/2 = 0). S8 must not error or panic on zero-pair rows; it silently skips them and advances the watermark.

**S8 — duplicate entry IDs in target_ids**: If `target_ids = '[1, 1, 2]'`, the pair (1,1) should be excluded (a<b constraint), and (1,2) and (1,2) are the same pair, deduplicated by INSERT OR IGNORE.

**S8 — audit_log row with `target_ids = '[]'`**: Empty array is valid JSON. Zero pairs produced. Watermark advances. No error.

**Config — `max_s1_edges_per_tick = 1`**: Cap of 1 should write exactly 1 edge on a corpus producing 10+ qualifying pairs, selecting the pair with the highest tag-count overlap.

---

## Security Risks

**S2 SQL construction — vocabulary term injection (R-02)**: The primary injection surface. `sqlx::QueryBuilder::push_bind` is the mitigation. Any regression to string interpolation (e.g., format!("{term}") into the SQL) reintroduces the full SQLi surface. The SECURITY comment in the construction loop is both a documentation artifact and a code review trigger. The injection test (vocabulary term with `'`) is the automated guard.

**S8 JSON parsing from audit_log**: `target_ids` is operator-written JSON. While audit_log writes are internal, a hypothetical bug in a prior tool handler that writes arbitrary content to `target_ids` could inject unexpected u64 values. The quarantine guard (filtering non-Active entry IDs) limits blast radius — an injected ID that does not correspond to an Active entry is silently dropped. No path traversal risk; only integer IDs are extracted.

**Untrusted input surface**: No HTTP input. All data flows are internal (SQLite reads). Vocabulary terms come from config.toml (operator-controlled). The SQL injection risk (R-02) is the sole external-input risk; the parameterized binding mitigation structurally eliminates it.

**Blast radius if S2 SQL construction is compromised**: S2 uses `write_pool_server()` with SQLite WAL. A successful SQL injection via a malicious vocabulary term could drop or corrupt the `graph_edges` table. However, config.toml requires filesystem write access — an attacker capable of modifying config.toml already has full access. Defense-in-depth applies; the risk remains documented.

---

## Failure Modes

**S1 SQL error (e.g., table schema change)**: S1 logs at `warn!` with error message and entry count 0. S2 and S8 continue unaffected. No tick loop halt. Edges from prior successful ticks persist in the table.

**S2 SQL error during dynamic query construction**: Possible if the `QueryBuilder` produces syntactically invalid SQL for unusual vocabulary content. Logged at `warn!`, S2 returns without writing edges. The next tick retries (S2 is stateless; no watermark). If the error is permanent (malformed vocabulary term that survives `push_bind`), every S2 tick will warn until config is corrected.

**S8 watermark read failure**: If `counters::get` fails (unlikely; SQLite read), S8 logs at `warn!` and returns without processing. Edges from prior batches persist. The watermark is unchanged — the same batch is retried on the next S8 run.

**S8 watermark write failure**: If `counters::set` fails after edge writes, S8 logs at `warn!`. The same batch is re-processed on the next S8 run. INSERT OR IGNORE prevents duplicate edges. Acceptable at-least-once re-processing per ADR-003 and SR-03.

**S8 bulk quarantine-filter query failure**: If the bulk `SELECT id FROM entries WHERE id IN (...)` fails, S8 cannot validate pair endpoints. Safe behavior: log at `warn!` and skip writing any pairs from this batch. Watermark does not advance. Retry on next run.

**Config validate() rejection at startup**: If any of the four numeric InferenceConfig fields is out of range, `validate()` returns an error. The server refuses to start with an informative error message naming the out-of-range field. This is a hard startup gate, not a degraded mode.

**S2 empty vocabulary (non-error)**: The S2 tick returns immediately after a debug trace. No SQL is issued. Behavior is documented as intentional (operator opt-in). Not a failure mode — expected default behavior.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (S2 SQL injection) | R-02 | `sqlx::QueryBuilder::push_bind` eliminates interpolation; AC-11 test with adversarial term; SECURITY comment at construction site |
| SR-02 (S1 GROUP BY materialization) | R-04 | OQ-01 resolved in architecture/spec: query plan verification required in implementation brief; NFR-03 timing test ≤500ms at ≤1,200 entries |
| SR-03 (S8 at-least-once re-processing on crash) | R-06 | ADR-003 write-after-commit ordering; INSERT OR IGNORE idempotency; AC-16 test for re-processing scenario |
| SR-04 (crt-040 `write_graph_edge` prerequisite) | R-08 | Hard prerequisite gate in ADR-001 (grep pre-flight); C-06 constraint; AC-28 verification step |
| SR-05 (GraphCohesionMetrics field definition) | — | Resolved by ADR-004: both fields already exist in col-029; no new fields needed; risk closed |
| SR-06 (S1/S2 additive-only stale edges) | R-09 | OQ-03 must be answered in implementation brief; if crt-039 compaction does not cover S1/S2, deferred with documentation |
| SR-07 (InferenceConfig dual-maintenance trap) | R-03 | ADR-005: dual-site test `test_inference_config_s1_s2_s8_defaults_match_serde`; C-07 constraint; both sites must be updated atomically |
| SR-08 (S8 malformed JSON stuck watermark) | R-05 | ADR-003: log + advance watermark past malformed row; AC-20 test with non-JSON `target_ids` between two valid rows |
| SR-09 (eval gate before first rebuild tick) | R-15 | Architecture doc and NFR-08 both require at least one full tick post-delivery; AC-32; OQ-04 resolved via wall-clock wait or `last_tick_at` signal |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 9 scenarios minimum — all three sources tested for quarantine guard, injection test with adversarial term, serde-match test for all 5 fields |
| High | 5 (R-04, R-05, R-06, R-07, R-08) | 8 scenarios — timing test, malformed-JSON watermark, write-order mock, source-value assertions, prerequisite gate |
| Medium | 7 (R-09–R-13, R-17) | 10 scenarios — cap semantics, false-positive suppression, operation-type filters, metric isolation, validate() range checks |
| Low | 2 (R-15, R-16) | 3 scenarios — tick-wait procedure verification, cohesion metrics readable without PPR rebuild, file size check |

**Total: 17 risks, 30 scenarios minimum.**

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned graph enrichment — found #3978, #3981 (quarantine dual-JOIN production bug); #3713 (graph inference stall); #2800 (cap logic testability)
- Queried: `/uni-knowledge-search` for risk patterns — found #4026 (S8 watermark pattern), #3884 (INSERT OR IGNORE idempotency), #3822 (near-threshold oscillation)
- Queried: `/uni-knowledge-search` for InferenceConfig dual-maintenance — found #3817 (dual-site trap), #4013 (hidden test sites), #4028 (crt-040 ADR-002)
- Queried: `/uni-knowledge-search` for quarantine dual-JOIN — found #3978, #3981 (directly inform R-01 severity elevation)
- Stored: nothing novel to store — all relevant patterns are already in Unimatrix (#4026 for S8 watermark, #3817/#3980/#3981 for existing risk patterns); no new cross-feature risk pattern emerged that is not already captured
