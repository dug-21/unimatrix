# Risk-Based Test Strategy: crt-024 (Ranking Signal Fusion — WA-0)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `utility_delta` normalization: spec FR-05 updated to match architecture shift-and-scale `(val + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)`. **Divergence resolved** — test scenarios below remain valid as correctness checks. | High | Low | High |
| R-02 | NLI absence re-normalization division-by-zero: `nli_absent_sum == 0.0` when all five non-NLI weights are zero — pathological but reachable via config; must return 0.0, not panic | High | Med | High |
| R-03 | `PROVENANCE_BOOST == 0.0` guard: `prov_norm = raw / PROVENANCE_BOOST` divides by zero if operator sets the constant to 0 or it is ever changed to 0.0; ADR-001 notes this must be guarded | High | Med | High |
| R-04 | Regression test churn: all tests asserting specific `final_score` values break with new formula; if any are deleted rather than updated, coverage holes open permanently (entry #751 procedure) | High | High | Critical |
| R-05 | `apply_nli_sort` removal — orphaned tests: crt-023 unit tests call `apply_nli_sort` directly; if deleted without replacement, NLI entailment-ranking behavior loses coverage | High | High | Critical |
| R-06 | W3-1 training signal corruption: if default weights produce poor initial ranking quality (e.g., NLI underweighted, or util/prov neutral-floor effect), every context_search call trains W3-1 toward a degraded starting point | High | Med | High |
| R-07 | boost_map prefetch sequencing: if `spawn_blocking` for co-access completes after the scoring loop begins, some candidates score with `coac_norm = 0.0` instead of their true co-access value — silent ranking corruption | High | Med | High |
| R-08 | `coac_norm` constant duplication: `MAX_CO_ACCESS_BOOST` defined in `unimatrix-engine::coaccess`; if `search.rs` defines its own copy (even identical), any future engine change silently diverges | Med | Med | High |
| R-09 | Re-normalization applied in NLI-enabled path: `ScoreWeights::effective()` called when NLI is active; weights are re-normalized even though they should be used as configured; fused scores are systematically off | Med | Med | High |
| R-10 | `try_nli_rerank` return type migration: callers pattern-match on `Option<Vec<(EntryRecord, f64)>>`; after ADR-002 changes it to `Option<Vec<NliScores>>`, unupdated callers fail to compile or receive mistyped data | Med | High | High |
| R-11 | `utility_delta` negative range floor missing: `utility_delta` returns -0.05 (Ineffective/Noisy); without the shift formula, `util_norm` is negative; `fused_score` can go below 0.0 when `w_util` is applied | High | Med | High |
| R-12 | Weight sum validation bypass: `validate()` is called at startup but `InferenceConfig` could be constructed in tests or integration contexts bypassing `validate()`; invalid weight sums reach the scorer | Med | Med | Med |
| R-13 | Config backward compatibility: operators who do not add the six new fields to `config.toml` must get defaults summing ≤ 0.95; if `#[serde(default)]` is missing on any field, deserialization fails for existing configs | Med | Low | Med |
| R-14 | `status_penalty` applied inside `compute_fused_score` instead of outside: if implementer applies penalty inside the pure function, `FusedScoreInputs` must carry `status_penalty` and the function is no longer a pure signal combiner — ADR-004 invariant violated | Med | Med | Med |
| R-15 | NLI score index misalignment: `NliScores` returned from `try_nli_rerank` are indexed parallel to the input candidates slice; if candidates are reordered between NLI scoring and the fused pass, indices misalign — wrong NLI score applied per entry | High | Low | Med |
| R-16 | WA-2 extension: `FusedScoreInputs` and `FusionWeights` structs are fixed-field; WA-2 requires adding `phase_boost_norm` and `w_phase` — if these structs are defined without non-exhaustive markers or the extension contract is not tested, WA-2 becomes a breaking change | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: utility_delta Normalization Formula — RESOLVED
**Severity**: High → **Resolved pre-implementation**
**Likelihood**: Low (divergence fixed)
**Impact (if regression)**: `util_norm` computed incorrectly; negative utility entries pull `fused_score` below 0.0, breaking NFR-02. Spec FR-05 now canonically specifies shift-and-scale. Test scenarios remain as correctness regression guards.

**Test Scenarios**:
1. Unit test: `utility_delta = -0.05` (Ineffective). `util_norm` must equal `0.0` via shift-and-scale `(-0.05 + 0.05) / (0.05 + 0.05) = 0.0`. Assert `util_norm == 0.0`.
2. Unit test: `utility_delta = 0.0` (unclassified/neutral). `util_norm` must equal `0.5`. Assert `util_norm == 0.5`.
3. Unit test: `utility_delta = +0.05` (Effective). `util_norm` must equal `1.0`. Assert `util_norm == 1.0`.
4. Unit test: `fused_score` with `util_norm=0.0` and `w_util=0.05` must be ≥ 0.0 — no negative contribution from an Ineffective entry.
5. Integration test: entry classified as Ineffective; verify `ScoredEntry.final_score` is lower than the same entry at neutral utility, not negative.

**Coverage Requirement**: All three `utility_delta` boundary values (-0.05, 0.0, +0.05) must be tested. The formula `(val + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)` must be verified by name in a code comment, not just implied.

---

### R-02: NLI Absence Re-normalization Division-by-Zero
**Severity**: High
**Likelihood**: Med
**Impact**: Server panics when operator sets all five non-NLI weights to 0.0. Even though pathological, this is a reachable config state — an operator experimenting with NLI-only scoring could set all others to 0.0.

**Test Scenarios**:
1. Unit test: `w_sim=0, w_conf=0, w_coac=0, w_util=0, w_prov=0`, NLI absent. `ScoreWeights::effective(nli_available: false)` must return without panic; all scores should be `0.0`.
2. Unit test: single non-zero remaining weight (`w_sim=0.5`, all others 0.0), NLI absent. Re-normalization yields `w_sim_eff = 1.0`. Assert fused score equals `1.0 * similarity`.
3. Unit test: `w_nli=1.0`, all others 0.0, NLI absent. Confirm effective weights produce 0.0 scores without panic.

**Coverage Requirement**: The zero-denominator guard must be in `ScoreWeights::effective()`, not the scoring loop. At least one test must verify the guard path executes and does not panic.

---

### R-03: PROVENANCE_BOOST Division-by-Zero Guard
**Severity**: High
**Likelihood**: Med
**Impact**: If `PROVENANCE_BOOST` is ever 0.0 (or an operator route makes it 0), `prov_norm = prov_boost / PROVENANCE_BOOST` is `0/0 = NaN`. NaN propagates silently through the fused formula, producing `NaN` scores that corrupt sort order.

**Test Scenarios**:
1. Unit test: `PROVENANCE_BOOST = 0.0`, entry has provenance boost. Guard must produce `prov_norm = 0.0`.
2. Unit test: `PROVENANCE_BOOST = 0.02` (normal), boosted entry. Assert `prov_norm = 1.0`.
3. Unit test: `PROVENANCE_BOOST = 0.02`, unboosted entry. Assert `prov_norm = 0.0`.
4. Property test: any `fused_score` computed via `compute_fused_score` must be finite (`is_finite()` true). No NaN or infinity from provenance computation.

**Coverage Requirement**: Explicit guard before division; `prov_norm = if PROVENANCE_BOOST == 0.0 { 0.0 } else { raw / PROVENANCE_BOOST }`. Guard must be exercised by at least one test path.

---

### R-04: Regression Test Churn — Score Value Updates
**Severity**: High
**Likelihood**: High
**Impact**: Pre-crt-024 tests assert specific `final_score` values computed by `(1-cw)*sim + cw*conf + utility_delta + co_access_boost + prov_boost`. Every such assertion fails with the new formula. If tests are deleted rather than updated, the spec's AC-08 is violated and coverage holes open (entry #751 — Updating Golden Regression Values).

**Test Scenarios**:
1. Audit: `git diff --stat` before and after shows no deleted test files.
2. Audit: total test count after crt-024 ≥ total test count before crt-024.
3. For every test previously asserting `|computed_score - expected| < epsilon`, verify the expected value is updated to the new formula's output for the same inputs.
4. AC-11 regression test: Entry A (nli=0.9, coac=0.0) scores 0.540 under defaults; Entry B (nli=0.3, coac=1.0) scores 0.430. Assert Entry A ranks above Entry B.
5. Companion comment test: document the pre-crt-024 formula that produced the inversion, as proof the defect existed (without executing old code).

**Coverage Requirement**: Zero deleted test functions. AC-11 regression test present and passing. All score-value assertions updated with formula-derived expected values.

---

### R-05: apply_nli_sort Removal — Orphaned Test Coverage
**Severity**: High
**Likelihood**: High
**Impact**: crt-023 added unit tests specifically for `apply_nli_sort` (sort key semantics, tiebreak behavior, length mismatch handling). ADR-002 removes the function. If tests are deleted without replacement, NLI entailment ranking behavior has no direct unit coverage.

**Test Scenarios**:
1. Replacement test: NLI entailment dominant ranking. Entry with nli=0.9 scores above entry with nli=0.1 under equal sim, conf, coac — `compute_fused_score` unit test.
2. Replacement test: tiebreak behavior. Two entries with identical fused scores produce a stable sort order (deterministic per NFR-03).
3. Replacement test: length mismatch between candidates and NliScores (formerly tested by `apply_nli_sort` tests). Either assert panic-free degradation (0.0 for out-of-range index) or assert the pre-check rejects mismatches before scoring.
4. Verify no test file retains a call to `apply_nli_sort` after the function is removed — compile-time failure if present.

**Coverage Requirement**: Every behavior previously tested in `apply_nli_sort` unit tests must have a named successor test in the fused scorer suite. Migration must be one-to-one, not lossy.

---

### R-06: W3-1 Training Signal Corruption from Default Weights
**Severity**: High
**Likelihood**: Med
**Impact**: Default weights are W3-1's initialization point. If defaults produce systematic ranking errors (wrong signal dominance), every query-result pair collected during normal operation trains W3-1 toward a wrong model. Long-tail — error compounds silently over thousands of training samples.

**Test Scenarios**:
1. AC-11 regression test (already defined): confirms NLI dominance at defaults.
2. Constraint 9 verification test: with NLI disabled, re-normalized sim/conf ratio ensures sim-dominant ranking — `w_sim' = 0.4167 > w_conf' = 0.2500`.
3. Constraint 10 verification test: at full defaults (no NLI, no coac), high-sim entry (0.9/0.3) beats high-conf entry (0.5/0.9).
4. Human review checkpoint: defaults in ADR-003 (`w_nli=0.35, w_sim=0.25, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05`, sum=0.95) must match constants in `config.rs`. Any divergence silently changes W3-1's training baseline.

**Coverage Requirement**: All three numerical verifications from ADR-003 must appear as explicit unit tests — not just in the ADR prose. These tests are the only protection against a defaults typo corrupting W3-1's training.

---

### R-07: boost_map Prefetch Sequencing Race
**Severity**: High
**Likelihood**: Med
**Impact**: If the `spawn_blocking` call for `compute_search_boost` is initiated but its result is `.await`-ed after the scoring loop begins, some candidates are scored with `coac_norm = 0.0`. The ranking is silently wrong — co-access signal disappears for entries evaluated before the future resolves.

**Test Scenarios**:
1. Integration test: construct a search call with known co-access data for result entries. Assert the returned `ScoredEntry` ranks reflect co-access weighting — a high-coac entry must not score as if coac=0.
2. Code review: verify `boost_map` is fully `.await`-ed and its result bound to a variable before the scoring iterator begins. No interleaved `await` points between boost_map fetch and scoring loop.
3. Pipeline step ordering test: assert that Step 6c (boost_map prefetch) appears sequentially before Step 7 (NLI scoring + fused scoring) in the search pipeline. A code-level invariant comment suffices if the async sequencing is not directly testable.
4. NLI + co-access overlap test: when both NLI scoring (rayon pool) and boost_map prefetch (spawn_blocking) run concurrently, the result of each must be fully available before the scoring pass. Test with both NLI and co-access data present.

**Coverage Requirement**: At least one integration test must verify co-access values reach the scorer correctly (non-zero coac_norm for an entry with known co-access history). This confirms the async sequencing is correct.

---

### R-08: MAX_CO_ACCESS_BOOST Constant Duplication
**Severity**: Med
**Likelihood**: Med
**Impact**: If `search.rs` defines `const MAX_CO_ACCESS_BOOST: f64 = 0.03` locally rather than importing from `unimatrix_engine::coaccess`, a future engine change (e.g., increasing the cap) silently diverges. `coac_norm` is computed against the stale constant, normalization breaks.

**Test Scenarios**:
1. AC-07 verification: `grep` search in `search.rs` must find `MAX_CO_ACCESS_BOOST` only as an imported name (via `use unimatrix_engine::coaccess::MAX_CO_ACCESS_BOOST`), not as a `const` definition.
2. Unit test: `coac_raw = MAX_CO_ACCESS_BOOST` → `coac_norm = 1.0`. `coac_raw = MAX_CO_ACCESS_BOOST / 2.0` → `coac_norm = 0.5`. `coac_raw = 0.0` → `coac_norm = 0.0`. These tests use the imported constant directly, so they automatically detect divergence.

**Coverage Requirement**: CI compilation is the primary guard — if the import is wrong, build fails. Tests that use the imported constant as the reference value provide secondary verification.

---

### R-09: Spurious Re-normalization in NLI-Enabled Path
**Severity**: Med
**Likelihood**: Med
**Impact**: If `ScoreWeights::effective(nli_available: true)` incorrectly re-normalizes weights (or if `nli_available` is computed as `false` when NLI is active), fused scores are systematically inflated relative to configured weights. AC-13 catches this but only if the test exists.

**Test Scenarios**:
1. AC-13 unit test: weights sum to 0.90, NLI enabled, NLI score = 0.5. Assert `w_nli_eff == w_nli` (0.35 unchanged). Assert fused score equals hand-computed value using configured weights directly.
2. Unit test: `nli_available = true`, configure weights summing to 0.70. Scoring pass must use 0.70 as the effective weight sum, not 1.0. Assert fused score is less than 0.70 for all-ones inputs.
3. Negative test: `nli_available = false`, confirm re-normalization fires and `sum(effective weights) == 1.0`.

**Coverage Requirement**: Explicit test for the `nli_available = true` path asserting no re-normalization occurs. This is the complement test to R-02's re-normalization path.

---

### R-10: try_nli_rerank Return Type Migration
**Severity**: Med
**Likelihood**: High
**Impact**: `try_nli_rerank` returns `Option<Vec<(EntryRecord, f64)>>` before crt-024. ADR-002 changes it to `Option<Vec<NliScores>>`. Any call site that destructures the tuple or indexes into the f64 field fails to compile or applies the entailment score as if it were a similarity score.

**Test Scenarios**:
1. Compile-time verification: after the refactor, `cargo check` must pass with zero errors. This is the primary gate.
2. Unit test: `try_nli_rerank` returns `Some(vec_of_nli_scores)` for a valid NLI call. Assert the returned Vec has one `NliScores` per candidate.
3. Unit test: `try_nli_rerank` returns `None` on NLI failure. Assert scorer uses 0.0 for all NLI terms and applies re-normalization.
4. Type-level test: the return type annotation at the call site in `search()` must match the new signature — this is enforced by Rust's type system.

**Coverage Requirement**: Compile success is sufficient for the type migration. Behavioral tests (scenarios 2-3) verify the contract of the new return type.

---

### R-11: utility_delta Negative Range — Fused Score Goes Below Zero
**Severity**: High
**Likelihood**: Med
**Impact**: If the shift-and-scale formula is absent and `util_norm = utility_delta / UTILITY_BOOST` is used instead, `util_norm` is -1.0 for an Ineffective entry. With `w_util=0.05`, this subtracts 0.05 from the fused score. While small at w_util=0.05, the score guarantee (NFR-02, `fused_score ∈ [0,1]`) is violated. For higher `w_util` operator configs, the effect scales.

**Test Scenarios**:
1. Unit test: Ineffective entry (`utility_delta = -0.05`). `util_norm` must be exactly `0.0` (not -1.0). Assert `compute_fused_score` with `util_norm=0.0` returns a non-negative value.
2. Unit test: any fused score is ≥ 0.0 when all weights sum ≤ 1.0 and all other inputs are in [0, 1].
3. Property-style test: generate 100 random signal vectors in [0,1] with valid weights; assert all `fused_score` results are in [0, 1].
4. Boundary test: all signals at 0.0, all weights at default. Assert `fused_score == 0.0` (not negative).

**Coverage Requirement**: At least one test must construct an Ineffective entry and verify `fused_score >= 0.0`. This directly catches the "division without shift" implementation error.

---

## Integration Risks

### IR-01: Pipeline Step Ordering — Boost Map Before NLI Scoring
The architecture (ARCHITECTURE.md §Data Flow) places boost_map prefetch before the NLI scoring step. If an implementer interleaves `.await` points such that the rayon NLI batch and the `spawn_blocking` boost_map fetch run concurrently but both are awaited before scoring — that is correct. If the scoring loop begins before either `.await` is resolved — that is a bug. The sequencing constraint is: both futures must be fully resolved before the candidate iteration begins.

### IR-02: FusedScoreInputs Construction Per Candidate
`FusedScoreInputs` is constructed in the scoring loop for each candidate. If any field uses a shared mutable reference (e.g., `boost_map.get_mut` rather than `boost_map.get`), concurrent access risk exists under rayon. The architecture is clear: `boost_map` is consumed read-only during scoring (the struct is built by `spawn_blocking`, handed to the sync loop). Test that `boost_map` is accessed via shared reference only.

### IR-03: BriefingService Isolation
`BriefingService` uses `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01`. Any refactoring that extracts the normalization formula into a shared utility must not inadvertently use `MAX_CO_ACCESS_BOOST = 0.03` for briefing. Verify briefing tests pass unchanged (AC-14).

### IR-04: rerank_score Still Callable After Refactor
`rerank_score` in `unimatrix-engine/src/confidence.rs` must remain callable after crt-024. The fallback (NLI-absent) path in the non-NLI branch of search may still invoke it. Any accidental removal or signature change fails tests in other features that use this function.

---

## Edge Cases

### EC-01: All Six Weights at Zero
Config with all six weights at 0.0 passes validation (sum = 0.0 ≤ 1.0) but produces `fused_score = 0.0` for every candidate. Sort is undefined over equal scores. Must not panic; must produce deterministic sort (stable by original HNSW order or entry ID).

### EC-02: Weight Sum Exactly 1.0
Sum == 1.0 is valid. All `fused_score` values are in [0, 1] by construction. Headroom for WA-2 is zero. This is operator's choice — validation must not reject it.

### EC-03: Single Candidate in Result Set
One candidate passes HNSW. boost_map prefetch for one entry. NLI scoring for one entry. Scoring loop iterates once. Sort of one element is trivially correct. Truncation to k where k ≥ 1 returns the one entry. Must not panic.

### EC-04: NLI Returns All Zero Entailment Scores
NLI model produces `entailment = 0.0` for all candidates (degenerate model state). The `w_nli` term contributes 0.0 for all candidates — equivalent to NLI absent but without triggering re-normalization. Sort degrades to the remaining five signals. Must not panic; must produce a valid ordered result.

### EC-05: co_access_raw Exceeds MAX_CO_ACCESS_BOOST
`compute_search_boost` is bounded to [0, MAX_CO_ACCESS_BOOST = 0.03]. If the engine crate ever produces a raw boost slightly above 0.03 (floating-point epsilon), `coac_norm > 1.0`. The fused score could exceed 1.0 if this term is non-negligible. Consider whether `coac_norm = (raw / MAX_CO_ACCESS_BOOST).min(1.0)` is warranted.

### EC-06: status_penalty Applied at 0.0
A deprecated entry with `DEPRECATED_PENALTY = 0.0` (hypothetical future config) would produce `final_score = 0.0` regardless of fused score. While current constants are non-zero, the formula must not divide by `status_penalty` anywhere.

### EC-07: Candidate Count Mismatch Between NLI Scores and Candidates
If `try_nli_rerank` returns `Vec<NliScores>` with length ≠ candidates.len(), the scoring loop must handle the mismatch without an index-out-of-bounds panic. Either assert equality before scoring (fail fast) or default out-of-bounds indices to `nli_entailment = 0.0`.

---

## Security Risks

### SeR-01: Untrusted Weight Config Values
Weight fields are loaded from operator-controlled `config.toml`. A malformed config (`w_nli = -999.9`) reaches `validate()` at startup. Validation rejects it and the server refuses to start — no runtime injection risk. The guard is at server boot, before any request is served. Risk is bounded to denial-of-service via bad config, not data corruption.

**Blast radius**: Server fails to start. No data exposure. Mitigation: structured error message with field names allows rapid operator diagnosis.

### SeR-02: NaN Propagation from Unchecked Division
Division operations in normalization (`÷ MAX_CO_ACCESS_BOOST`, `÷ PROVENANCE_BOOST`, `÷ nli_absent_sum`) can produce NaN or infinity if the denominator is 0.0. NaN scores propagate silently into `ScoredEntry.final_score`, corrupting sort order for all affected results. The blast radius is incorrect search results returned to agents — wrong context injected into agent prompts, degraded agent quality.

**Mitigation**: All three division points must have `== 0.0` guards producing 0.0. NFR-02 should include an `is_finite()` assertion on the final score in debug builds.

### SeR-03: Weight Sum Bypass via Config Reload
If `InferenceConfig` is ever reloaded at runtime without re-running `validate()` (not currently in scope, but a future risk), an operator could inject a weight sum > 1.0 that produces `fused_score > 1.0`. No SIGHUP-based reload is present in the current architecture; this is a forward-looking risk for any future hot-reload feature.

---

## Failure Modes

### FM-01: Server Refuses to Start — Invalid Weights
`InferenceConfig::validate()` returns `Err` with `FusionWeightSumExceeded`. Server logs the sum and all six field values, then exits. Agents receive no response; they should retry against a server restart. Error message must be actionable — operator sees exactly which fields to reduce.

### FM-02: NLI Model Not Ready — Graceful Degradation
`try_nli_rerank` returns `None`. Scoring uses `ScoreWeights::effective(nli_available: false)` — five-weight re-normalized formula. Search continues; no MCP error returned. This is the correct silent degradation path from crt-023 constraint (NLI absence never errors callers).

### FM-03: boost_map Prefetch Timeout
`spawn_blocking_with_timeout` for `compute_search_boost` times out. Scoring proceeds with empty boost_map — all `coac_norm = 0.0`. Results ranked without co-access signal. This is an existing graceful degradation pattern. Must be logged at warn level; must not surface as MCP error.

### FM-04: All Candidates Produce fused_score = 0.0
Degenerate input (all signals at minimum, all weights at 0.0, or pathological normalization). Sort is over all-equal scores. Must produce a deterministic, stable sort (same input order → same output order). Do not rely on sort stability being implicit — either document it or force determinism via secondary sort key (e.g., entry UUID).

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: Default weights biasing W3-1 if NLI underweighted | R-06 | ADR-003 derives defaults from signal-role reasoning; three numerical verifications pass. Test requirement: all three must be automated unit tests, not just ADR prose. |
| SR-02: Six-term vs. four-term formula divergence (entry #2298) | — | Resolved at architecture level: ADR-001 canonicalizes the six-term formula as the implementation target. No residual architecture risk. |
| SR-03: NLI re-normalization denominator covers wrong subset | R-02 | ARCHITECTURE.md §NLI Absence Re-normalization explicitly uses all five non-NLI weights. Unit test for zero-denominator guard is required (R-02, Scenario 1). |
| SR-04: WA-2 extension contract if formula is fixed-arity | — | Resolved at architecture level: ADR-004 defines `FusedScoreInputs` and `FusionWeights` as extensible structs; WA-2 adds one field to each. No residual risk. |
| SR-05: apply_nli_sort fate unresolved — coverage gap | R-05 | ADR-002 decides: remove. Test migration required (R-05). Risk materializes if migration is incomplete. |
| SR-06: Operators tune weights to 0.95, WA-2 silently exceeds 1.0 | — | Resolved: defaults sum to 0.95 (0.05 headroom). Validation rejects > 1.0 at startup. WA-2 adds `w_phase` to the sum check. Residual: operators who manually set weights to sum 1.0 must retune when WA-2 ships. Not an implementation risk for crt-024. |
| SR-07: Co-access boost_map must be prefetched before scoring | R-07 | ARCHITECTURE.md §Data Flow and SPECIFICATION.md Step 6c make this an explicit pipeline step. Integration test required to confirm async sequencing is correct (R-07). |
| SR-08: BriefingService normalization constant mismatch | — | AC-14 and FR-13 explicitly exclude briefing. No implementation change touches briefing. Resolved by scope boundary. |
| SR-09: rerank_score duplication vs. behavioral divergence | — | ARCHITECTURE.md §Technology Decisions: fused formula in NLI-active path does not call `rerank_score`; it computes each term directly. `rerank_score` retained for fallback and existing tests. Accepted: some divergence between paths is intentional. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-04, R-05) | 12 scenarios (utility normalization correctness, test preservation audit, apply_nli_sort migration) |
| High | 8 (R-02, R-03, R-06, R-07, R-08, R-09, R-10, R-11) | 24 scenarios |
| Medium | 4 (R-12, R-13, R-14, R-15) | 8 scenarios |
| Low | 1 (R-16) | 1 scenario |

**Minimum test additions for crt-024** (beyond updating existing tests for score-value changes):
1. `util_norm` boundary values: -0.05 → 0.0, 0.0 → 0.5, +0.05 → 1.0
2. `coac_norm` boundary values: 0.0, 0.5×MAX, 1.0×MAX
3. `prov_norm` division-by-zero guard
4. NLI absence re-normalization: five-weight denominator, zero-denominator guard
5. NLI active path: no spurious re-normalization (AC-13)
6. AC-11 regression test: NLI-high beats co-access-high
7. ADR-003 Constraint 9 and 10 numerical verifications as unit tests
8. boost_map prefetch integration test with known co-access data
9. `try_nli_rerank` new return type behavioral tests
10. Fused score range guarantee: all outputs in [0, 1] and `is_finite()`

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection ranking scoring pipeline" — found entry #724 (Behavior-Based Ranking Tests: assert ordering not scores) directly relevant to R-04 and R-05; no gate rejections found for search pipeline features
- Queried: `/uni-knowledge-search` for "risk pattern" (category:pattern) — found entries #2964 (signal fusion pattern: sequential sort passes), #1042 (Pure Computation Engine Module Pattern), #749 (Calibration Scenario Builder Pattern); #2964 directly informed R-07 framing
- Queried: `/uni-knowledge-search` for "normalization pure function InferenceConfig" — found entry #751 (Updating Golden Regression Values procedure) directly informing R-04 coverage requirement; entry #2972 (ADR-004 crt-024) confirming pure function decision
- Queried: `/uni-knowledge-search` for "co-access boost normalization" — found entries #701, #702, #706 confirming prior co-access weight history (once deleted, now returning as config-driven); informs R-08 (constant duplication risk)
- Stored: nothing novel to store — risk patterns here are feature-specific; the zero-denominator guard pattern and NaN propagation risk are worth storing if they recur in a second feature, but this is the first observation
