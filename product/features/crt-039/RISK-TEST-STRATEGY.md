# Risk-Based Test Strategy: crt-039

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Phase 8b write loop executes before `get_provider()` guard fires — Supports edges written without NLI scores (silent data corruption) | High | Med | Critical |
| R-02 | `test_run_graph_inference_tick_nli_not_ready_no_op` removed without replacement — no regression coverage for the Path A / Path B boundary | High | High | Critical |
| R-03 | Mutual-exclusion gap at cosine boundary 0.50 — a pair at exactly cosine 0.50 qualifies for both Phase 4 (`> 0.50` is false) and Phase 4b (`>= 0.50` is true), but only if explicit Supports-set subtraction is absent | High | Med | Critical |
| R-04 | `NliCandidatePair::Informs` / `PairOrigin::Informs` dead-code variants retained or partially removed — match arms still reachable, or removal breaks exhaustive matches silently | High | Med | Critical |
| R-05 | Cosine floor default 0.45 → 0.50 eliminates meaningful candidate pool — Group 3 starts from a near-empty graph; no pre-condition corpus gate blocks the change | Med | Med | High |
| R-06 | `apply_informs_composite_guard` signature change cascades to call sites — stale `nli_scores` argument passed at one call site, compiles if NliScores still imported, guard silently accepts all pairs | High | Low | High |
| R-07 | Phase 8b Informs write loop moved outside the NLI path block but `informs_metadata` is empty when `candidate_pairs` triggers early-return — zero Informs edges written when Supports candidates are absent | Med | Med | High |
| R-08 | `format_nli_metadata_informs` left in place as dead code — clippy warning not treated as error in CI; OQ-03 unresolved; downstream tooling reads absent NLI fields and silently gets nulls | Med | Med | High |
| R-09 | Contradiction scan block receives behavioral change during structural labeling — bracket or condition subtly altered, scan now runs unconditionally or never | Med | Low | Med |
| R-10 | Observability log (FR-14) emitted at wrong pipeline point — `informs_candidates_found` logged after dedup rather than before, masking "floor too high" vs "all deduped" diagnostic | Med | Med | Med |
| R-11 | Tick ordering invariant comment added but ordering itself disturbed — `run_graph_inference_tick` call site moved relative to contradiction scan during refactor | Med | Low | Med |
| R-12 | Cosine floor boundary semantics (`>=` vs `>`) silently inverted during refactor — 0.500 excluded instead of included | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Supports edges written without NLI scores (silent data corruption)
**Severity**: High
**Likelihood**: Med
**Impact**: Supports edges written with NLI entailment score 0.0 — every pair passes the `entailment > threshold` check if threshold is 0.0 default, corrupting the graph with semantically unvalidated Supports edges. No error raised; no log line. Bug would be invisible until graph quality review.

**Test Scenarios**:
1. TC-02: Integration test — NLI not ready (`NliServiceHandle::new()` Loading state). Execute `run_graph_inference_tick` with a Supports-eligible pair (cosine > `supports_candidate_threshold`). Assert zero Supports edges in `GRAPH_EDGES`. Assert `score_batch` was not called.
2. Structural review: confirm that in the restructured function body, no code path reaches `write_nli_edge` for `"Supports"` relation without passing through `get_provider()` → `Ok(provider)`. The ADR-001 control flow diagram is the spec — inspect against it.

**Coverage Requirement**: TC-02 must be an integration test (real Store), not a unit mock. It must assert the specific absence of Supports edges — not just "no panic." AC-14 and AC-16.

---

### R-02: Test coverage gap at Path A / Path B boundary
**Severity**: High
**Likelihood**: High
**Impact**: TR-01 removes the only test asserting behavior when NLI is not ready. If TC-01 and TC-02 are not written as replacements, the Path A / Path B split has zero automated validation. Lesson #3579 documents gate-3b delivery where entire test modules were absent — the pattern is recurrent.

**Test Scenarios**:
1. TC-01: Assert Phase 4b CAN write Informs edges when NLI not ready. Positive assertion — at least one Informs edge must be present, not merely "no error." Addresses AC-15.
2. TC-02: Assert Phase 8 does NOT write Supports edges when NLI not ready. These are two separate tests — not a single combined assertion. Addresses AC-16.
3. Gate check: pre-merge grep for `test_run_graph_inference_tick_nli_not_ready_no_op` must return empty. AC-16.

**Coverage Requirement**: Both TC-01 and TC-02 must be present and passing before gate-3c. Each must be an integration test with real Store reads confirming edge presence/absence. AC-02, AC-15, AC-16.

---

### R-03: Mutual-exclusion gap at cosine 0.50 boundary
**Severity**: High
**Likelihood**: Med
**Impact**: A pair at cosine exactly 0.50 that satisfies the `informs_category_pairs` filter could be written as both an Informs edge (Phase 8b) and a Supports edge (Phase 8, if NLI enabled). The architecture document asserts disjoint sets by construction, but the spec (FR-06, AC-13) requires explicit Phase 4 set subtraction. If the subtraction is omitted, the construction-level argument alone does not prevent overlap when `nli_informs_cosine_floor == supports_candidate_threshold`.

**Test Scenarios**:
1. TC-07: Unit test — populate `candidate_pairs` (Phase 4 Supports) with a pair at cosine 0.68. Run Phase 4b. Assert that pair is absent from `informs_metadata`. Addresses AC-13.
2. Boundary variant: pair at cosine exactly 0.50 — verify it appears in `informs_metadata` (Phase 4b accepted it via `>=`) and NOT in `candidate_pairs` (Phase 4 excluded it via strict `>`). This validates both sides of the boundary simultaneously.

**Coverage Requirement**: TC-07 must exercise the explicit subtraction, not rely on threshold arithmetic. If the subtraction is absent and the pair at 0.68 leaks into `informs_metadata`, the test catches it. AC-13, FR-06.

---

### R-04: Dead-code enum variants retained or partially removed
**Severity**: High
**Likelihood**: Med
**Impact**: `NliCandidatePair::Informs` and `PairOrigin::Informs` are explicitly removed per ADR-001. If removal is partial (variant declaration removed but match arms remain, or vice versa), the compiler will catch unreachable patterns in exhaustive matches — but only if all match sites are updated. If the variants are merely `#[allow(dead_code)]`-suppressed rather than deleted, the dead code persists and misleads Group 3 implementors.

**Test Scenarios**:
1. Compile-time check: `cargo build --workspace` with `#![deny(dead_code)]` active must pass. Any retained `Informs` variant causes a compile error.
2. Grep check: pre-merge `grep -rn 'NliCandidatePair::Informs\|PairOrigin::Informs'` in production code must return empty. AC-03 (indirect), ADR-001 consequence.
3. Exhaustive match audit: every `match nli_candidate_pair { ... }` and `match pair_origin { ... }` site must compile without `_ =>` wildcard masking a removed variant.

**Coverage Requirement**: Dead-code removal is a compile-time correctness concern, not a runtime test concern. The risk is in silent suppression (`allow(dead_code)`) or in match arm orphans. Code inspection at gate-3b.

---

### R-05: Cosine floor raise eliminates meaningful candidate pool
**Severity**: Med
**Likelihood**: Med
**Impact**: If the 0.45–0.499 cosine band contained the majority of qualifying pairs at current corpus size, raising the floor to 0.50 leaves Group 3 graph enrichment with a near-empty Informs graph to build on. ADR-003 explicitly requires a pre-condition corpus measurement before committing the default. Lesson #3723 (entry #3723) documents that threshold tuning without log coverage is a blind guess — the observability log (FR-14) partially mitigates this from tick 1, but does not block the risk before deployment.

**Test Scenarios**:
1. Pre-condition gate (implementor): Run HNSW scan against current active entry corpus. Count pairs in `[0.45, 0.50)` vs `[0.50, 1.0)` passing category pair filter. If `[0.45, 0.50)` contains >40% of qualifying pairs, spec writer must review before accepting default.
2. AC-11: Eval harness MRR >= 0.2913 on the 1,585-scenario harness (`product/research/ass-039/harness/scenarios.jsonl`). Regression from floor change caught by eval gate.
3. AC-17: Observability log records `informs_candidates_found` (pre-dedup, pre-cap) from tick 1. If this is zero in production, floor is too high — distinguishable from "all deduped."

**Coverage Requirement**: Eval gate (AC-11) is the quantitative regression check. Pre-condition corpus scan is the qualitative pre-deployment check. Both are required. ADR-003.

---

### R-06: Stale `nli_scores` argument at `apply_informs_composite_guard` call sites
**Severity**: High
**Likelihood**: Low
**Impact**: After ADR-002, `apply_informs_composite_guard` drops `nli_scores: &NliScores` from its signature. If any call site was not updated, the compiler catches it — this is a type error. However, if a test helper `informs_passing_scores()` is retained and the call site passes it via a cast or wrapper, the compile may succeed while the guard function ignores it. More likely: a test file that still constructs `NliScores` for the sole purpose of passing to this function will produce an unused-variable warning — which must be treated as an error.

**Test Scenarios**:
1. `cargo test --workspace` with `RUSTFLAGS="-D warnings"` — all call site updates are enforced at compile time. No manual test needed beyond CI green (AC-10).
2. Grep check: pre-merge `grep -n 'apply_informs_composite_guard' nli_detection_tick.rs` — all call sites must pass exactly one argument (`candidate`). Verify count matches expected call sites.
3. `informs_passing_scores()` helper: if retained only for Informs guard tests, it must be removed. If used by Phase 8 Supports tests, it may remain. ADR-002 consequence.

**Coverage Requirement**: Compiler enforcement is primary. Grep audit at gate-3b confirms no stale call sites. AC-03.

---

### R-07: Phase 8b Informs write loop skipped when `candidate_pairs` is empty
**Severity**: Med
**Likelihood**: Med
**Impact**: ADR-001 control flow includes: "if `candidate_pairs.is_empty()`: return early (no Phase 8)." This early return must occur only for the Path B (NLI Supports) path. If Phase 8b (Informs write loop) is placed inside this early-return block rather than after it, all Informs edges are lost whenever there are no Supports candidates — which is the common case when `nli_enabled = false` and no pairs exceed `supports_candidate_threshold`. The ARCHITECTURE.md flow diagram shows Phase 8b as a separate block after the Path B conditional; an implementor reading the spec loosely could nest it incorrectly. Lesson #2577 documents that boundary test ordering is exactly where implementation errors cluster.

**Test Scenarios**:
1. TC-01 variant: execute with HNSW state that produces zero Supports candidates (no pairs above `supports_candidate_threshold`) but at least one Informs candidate. Assert Informs edges ARE written. This directly catches the nesting error.
2. Separate TC: execute with zero candidates for both paths. Assert no writes, no panic.

**Coverage Requirement**: TC-01 must use a vector corpus where no pair exceeds `supports_candidate_threshold`, otherwise it does not exercise this specific risk. Test setup must be explicit about which threshold values are configured. AC-02, AC-15.

---

### R-08: `format_nli_metadata_informs` dead code / NLI fields in Informs edge metadata
**Severity**: Med
**Likelihood**: Med
**Impact**: If `format_nli_metadata_informs` is retained and not replaced by `format_informs_metadata`, Informs edges either carry NLI score fields with zero/garbage values (if the function is called with synthetic scores) or the function is dead code (if removed from the call path but not deleted). ADR-002 specifies replacement with `format_informs_metadata(cosine, source_category, target_category)`. OQ-03 is resolved by the architecture. Dead code in production files violates the no-dead-code convention and produces clippy warnings.

**Test Scenarios**:
1. AC-18: Confirm `format_nli_metadata_informs` has no dead-code warning in `cargo clippy --workspace`. If unused, the function must be deleted.
2. Informs edge metadata inspection: after TC-01 writes an Informs edge, query the edge metadata JSON. Assert fields `cosine` and category fields are present; assert `nli_neutral`, `nli_entailment`, `nli_contradiction` are absent.

**Coverage Requirement**: Clippy enforcement (AC-18) plus one metadata content assertion in TC-01. AC-18.

---

### R-09: Contradiction scan block behavioral change during structural labeling
**Severity**: Med
**Likelihood**: Low
**Impact**: The spec permits comment additions and whitespace only (NFR-07, SR-07 zero-diff behavioral constraint). Any bracket reordering or condition change during the labeling refactor would change when `scan_contradictions` runs. Because the contradiction scan condition uses `&&` with two sub-conditions (`is_multiple_of` and `get_adapter().is_ok()`), swapping their order or wrapping in a new outer `if` changes short-circuit evaluation semantics.

**Test Scenarios**:
1. AC-06: Existing contradiction scan tests pass without modification — behavioral identity confirmed.
2. Diff audit: git diff on the contradiction scan block must show only line additions (comments), no deletions or condition mutations.

**Coverage Requirement**: Existing tests for contradiction scan are the regression harness. Diff audit at gate-3b. AC-06, AC-07.

---

### R-10: Observability log emitted at wrong pipeline point
**Severity**: Med
**Likelihood**: Med
**Impact**: FR-14 requires `informs_candidates_found` to be the raw count before dedup and cap. If the log is emitted after the dedup filter in Phase 4b or after the Phase 5 truncation, the four fields collapse into indistinguishable values — all equal to the final written count. The diagnostic value (distinguishing "floor too high" from "dedup filtering all candidates") is lost. Lesson #3723 (threshold tuning blind without log coverage) is directly applicable.

**Test Scenarios**:
1. Construct a test corpus where dedup eliminates some candidates (existing Informs pairs in DB) and the cap eliminates others. After one tick, inspect the log output. Assert `informs_candidates_found > informs_candidates_after_dedup > informs_candidates_after_cap >= informs_edges_written` (or appropriate equality where expected).
2. If tracing subscriber test is not feasible in the test environment, inspect code ordering: the log must appear after Phase 4b candidate construction but before `query_existing_informs_pairs` dedup and before Phase 5 `truncate`. AC-17.

**Coverage Requirement**: Log field ordering is a code inspection check plus one scenario where the four values are demonstrably distinct. AC-17, FR-14.

---

### R-11: Tick ordering invariant disturbed during `run_single_tick` refactor
**Severity**: Med
**Likelihood**: Low
**Impact**: The ordering invariant (compaction → promotion → graph-rebuild → structural_graph_tick → contradiction_scan) is enforced by call sequence in `run_single_tick`. Removing the outer `if nli_enabled` wrapper is the only change to this file beyond comment additions. If the `run_graph_inference_tick` call is accidentally moved (e.g., placed before `TypedGraphState::rebuild` or after the contradiction scan block) during the refactor, the graph inference tick operates on a stale graph state or the contradiction scan runs first.

**Test Scenarios**:
1. Code inspection: `run_single_tick` body confirms correct call order. Ordering invariant comment (FR-11) is present and accurate.
2. Regression: existing integration tests that assert graph edge writes after a full tick pass — these implicitly validate that graph-rebuild precedes graph-inference. AC-07.

**Coverage Requirement**: Comment presence (AC-07) plus CI green on all tick-level integration tests. No new test required, but existing tick-order tests must not be deleted.

---

### R-12: Cosine floor boundary semantics inverted (`>=` vs `>`)
**Severity**: Low
**Likelihood**: Low
**Impact**: Phase 4b uses inclusive floor (`>=`). If a refactor accidentally introduces strict `>` comparison, the pair at exactly 0.500 is excluded. This is a precision regression that is subtle — existing tests at 0.499 and 0.500 will catch it only if they exercise the exact boundary value, not a nearby value like 0.501.

**Test Scenarios**:
1. TC-05: cosine exactly 0.500 included (`>=` semantics). AC-05.
2. TC-06: cosine exactly 0.499 excluded. AC-05.

**Coverage Requirement**: Both boundary tests are required. Tests using 0.501 or 0.498 do not cover this risk. AC-05, ADR-003.

---

## Integration Risks

**Phase boundary write ordering** (R-07): The most subtle integration risk is the placement of the Phase 8b Informs write loop relative to the Path B early-return. The architecture diagram is explicit, but the implementation must mirror it exactly — Phase 8b runs unconditionally after Phase 5 (after the Path A/Path B split), not inside the Path B block. A test with zero Supports candidates and nonzero Informs candidates is the detection vehicle.

**Dedup pre-filter and cap ordering** (AC-12, FR-09): `query_existing_informs_pairs` is loaded in Phase 2. Phase 4b uses it during candidate selection. Phase 5 truncates the already-deduped set. If a Phase 5 truncation bug truncates the full candidate list before dedup runs (wrong ordering), the 25-slot budget is consumed by already-existing pairs, and zero new edges are written. TC-01 with existing pairs in DB validates dedup-before-cap ordering.

**NliCandidatePair tagged union removal** (R-04): The `Informs` variant of this enum (introduced by crt-037) feeds into Phase 6 text fetch and Phase 7 NLI batch dispatch. After removal, any match arm that handled `NliCandidatePair::Informs` in Phase 6 or Phase 7 must also be removed. If Phase 6 iterates `pair_origins` and a match arm for `Informs` is left as `_ => {}`, Phase 7 may silently skip text fetch for an entry, producing empty strings in the NLI batch — a data quality bug in the Phase 8 Supports path even though Phase 8b is now separate.

---

## Edge Cases

**Empty active entry set**: `run_graph_inference_tick` is called unconditionally from tick 1. If the store has zero active entries (fresh deployment), Phase 2 DB reads return empty sets. Phase 3 produces zero source candidates. Phase 4 and Phase 4b produce zero candidates. Phase 5 truncates zero. Phase 8b iterates zero. No writes, no panic. Verify the function returns cleanly with zero entries in store.

**Single active entry**: Phase 4b HNSW search for a source with no neighbors above the floor returns an empty neighbor list. `informs_metadata` remains empty. No writes. This must not trigger any index-out-of-bounds or unwrap in the HNSW query path.

**All candidates deduped** (every candidate already in `existing_informs_pairs`): Phase 5 truncates a non-empty list but Phase 8b writes zero edges. `informs_edges_written = 0` in the log. Confirm the log still emits correctly (not short-circuited by a zero-count early return).

**Exactly 25 candidates** (cap boundary): `informs_metadata.truncate(25)` on a vec of exactly 25 is a no-op. `informs_metadata.truncate(25)` on a vec of 26 removes one. Verify the 25th candidate is written and the 26th is not.

**`supports_candidate_threshold` == `nli_informs_cosine_floor`** (both 0.50 by default): A pair at exactly 0.50 is excluded from Phase 4 (`> 0.50` strict) but included in Phase 4b (`>= 0.50` inclusive). The explicit Supports-set subtraction in Phase 4b cannot remove it because Phase 4 did not include it. This is correct behavior — but it is the hardest boundary scenario to reason about and must be explicitly tested (TC-07 boundary variant, R-03).

**Tick invoked before any HNSW vectors are indexed**: VectorIndex empty — HNSW search returns zero results for all sources. Phase 4 and Phase 4b produce zero candidates. Must not panic.

---

## Security Risks

**Untrusted input surface**: `run_graph_inference_tick` accepts `store: &Store`, `vector_index: &VectorIndex`, `config: &InferenceConfig`. All three are internal system objects, not user-supplied. The only external input paths to this tick are:

1. **Entry content read from DB (Phase 6, Path B only)**: Entry text is fetched for Phase 7 NLI scoring. After crt-039, Phase 4b does not fetch entry text — only Phase 8 (Supports) does. The text is passed to `score_batch` (rayon, CPU-bound inference). An adversarially crafted entry with extremely long text could cause memory pressure or latency in the NLI batch, but this is Path B only and does not affect the structural Informs path.

2. **`informs_category_pairs` from config**: Loaded from `InferenceConfig`. Not user-supplied per request — loaded at startup from `config.toml`. C-07 (no domain string literals) prevents hardcoded category strings in production code, reducing injection surface.

3. **HNSW neighbor results (Phase 4b)**: Cosine similarity scores are f32 values from the vector index. No parsing of external text. Cosine values are bounded `[-1.0, 1.0]` by construction. The inclusive floor comparison `>= 0.5` operates on these bounded values — no overflow or injection risk.

**Blast radius if component is compromised**: Phase 4b writes Informs edges to `GRAPH_EDGES`. A compromised tick could write arbitrary Informs edges (wrong source/target IDs, wrong weights). The `MAX_INFORMS_PER_TICK = 25` hard cap limits the write rate. The dedup pre-filter prevents duplicate edges. The temporal and cross-feature guards in `apply_informs_composite_guard` reduce (but do not eliminate) semantically invalid writes. The graph is a read-optimized enrichment structure — corrupted edges degrade retrieval quality but do not expose stored secrets or enable privilege escalation.

**W1-2 contract (NFR-03)**: Phase 4b must not invoke `score_batch`. A violation would cause CPU-bound ML inference to run on the async executor thread, degrading server responsiveness. Enforcement is by code inspection and NFR-01 (no rayon pool usage in Phase 4b). No user-accessible attack surface, but a correctness risk if the W1-2 boundary is accidentally crossed.

---

## Failure Modes

**`get_provider()` returns `Err` (expected production behavior)**: Path B is skipped entirely. Phase 8b Informs write loop executes normally via Path A. No error logged, no panic. Informs edges accumulate. This is the success path for crt-039's primary goal.

**VectorIndex search returns empty** (no neighbors above floor): `informs_metadata` is empty after Phase 4b. Phase 5 truncates zero. Phase 8b iterates zero. Tick completes silently. Observability log emits `informs_candidates_found=0`. This is the diagnostic signal for "floor too high" — distinguishable from other failure modes via FR-14 log fields.

**DB write failure in Phase 8b**: `write_nli_edge` returns an error for a given Informs candidate. The existing error handling (non-fatal per tick, logs error, continues to next candidate) must be preserved. The tick must not abort on a single write failure.

**Phase 5 truncation after shuffle**: `informs_metadata.shuffle()` before `truncate(25)` randomizes which candidates are written when the candidate count exceeds 25. This is correct behavior (uniform random sampling of the top candidates). The risk is if shuffle is accidentally removed — the tick would systematically favor the first 25 candidates in HNSW result order, introducing a structural bias toward high-cosine pairs that is not reflected in the spec's random-sampling intent.

**Contradiction scan condition inadvertently made unconditional** (R-09): `scan_contradictions` becomes O(N) ONNX re-computation on every tick, causing severe performance degradation. This is a latent risk in the comment-only refactor of the contradiction scan block. Zero-diff behavioral requirement is the mitigation.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (burst-control, dedup pre-filter placement) | R-07 (Phase 8b skipped when no Supports candidates), AC-12 | Resolved: Architecture confirms dedup pre-filter (Phase 2) runs before Phase 4b. Cap (Phase 5) is a hard write limit of 25. Dedup-before-cap ordering is enforced by call sequence. AC-12 formalizes this as an explicit code-ordering AC with test verification. |
| SR-02 (cosine floor empirical data missing) | R-05 (candidate pool eliminated) | Partially mitigated: ADR-003 requires implementor corpus scan. FR-14 observability log provides equivalent signal from tick 1 in production. Eval gate (AC-11, MRR >= 0.2913) is the quantitative backstop. Full mitigation depends on implementor running the pre-condition scan. |
| SR-03 (candidate set separation mechanism unspecified) | R-03 (mutual-exclusion gap at cosine 0.50 boundary) | Resolved differently from architecture: Architecture initially claimed disjoint-by-construction, but spec (FR-06, AC-13) requires explicit Phase 4 set subtraction. TC-07 validates this. The risk remains at implementation time if the subtraction is omitted. |
| SR-04 (Phase 1 guard split — Supports corruption if incomplete) | R-01 (Supports edges written without NLI scores) | Addressed by architecture (ADR-001 Option Z): `get_provider()` is placed after Phase 4b and after the Informs write loop. Control flow structurally prevents Phase 8 without a successful provider call. TC-02 validates this at the integration level. |
| SR-05 (test semantics change — no_op test vacuous rephrasing) | R-02 (test coverage gap at Path A / Path B boundary) | Addressed by spec: TR-01 requires explicit removal of the old test. TC-01 and TC-02 are required new tests (AC-15, AC-16). Two independent assertions, not one combined test. |
| SR-06 (observability gap — no signal from Phase 4b) | R-10 (log emitted at wrong pipeline point) | Addressed by FR-14 and AC-17: four structured log fields required. Risk remains that implementor places the log call at the wrong point in the pipeline. |
| SR-07 (contradiction scan separation — silent behavioral change) | R-09 (contradiction scan behavioral change during labeling) | Addressed by NFR-07 (zero-diff behavioral constraint) and AC-06 (existing tests pass). |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | TC-01, TC-02, TC-07; boundary variant of TC-07; TR-01 removal grep; dead-code compile check; structural diff audit of Path B entry point |
| High | 4 (R-05, R-06, R-07, R-08) | AC-11 eval gate; corpus pre-condition scan; TC-01 with zero-Supports-candidates setup; clippy clean check; AC-18 metadata field assertion |
| Medium | 4 (R-09, R-10, R-11, R-12) | AC-06 contradiction scan regression; FR-14 log field ordering assertion; tick ordering inspection + existing tick tests; TC-05, TC-06 boundary tests |
| Low | 1 (R-12 — covered by TC-05/TC-06) | Absorbed into Medium coverage above |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #3579 (gate-3b test omission), #2758 (gate-3c non-negotiable test validation), #2577 (boundary tests must ship in same pass). All three directly inform R-02 severity and TC-01/TC-02 coverage requirements.
- Queried: `/uni-knowledge-search` for "risk pattern nli_detection_tick graph inference tick" — found #3937 (NLI neutral-zone as detection signal — the pattern being removed), #3675 (tick candidate bound/shuffle pattern), #3949 (per-guard negative tests for composite guards — confirms TC-03/TC-04 approach), #3723 (threshold tuning blind without log — confirmed SR-06/R-10 severity).
- Queried: `/uni-knowledge-search` for "dead code enum variant removal" — found #3437/#3441 (Rust enum derive/dead-code pattern — informs R-04 risk framing).
- Stored: nothing novel to store — all patterns observed here (control-flow split risks, test coverage gaps on refactored no-op tests, cosine floor tradeoffs) are feature-specific to crt-039. A cross-feature pattern would require confirmation from a second feature.
