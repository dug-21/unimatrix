# Risk-Based Test Strategy: crt-013

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | W_COAC removal breaks confidence invariant tests or hidden callers | High | Low | Medium |
| R-02 | Episodic removal breaks compilation via undiscovered import paths | Medium | Low | Low |
| R-03 | Status penalty insufficient at extreme similarity differentials | High | Medium | High |
| R-04 | Penalty integration tests flaky due to non-deterministic embedding similarity | High | Medium | High |
| R-05 | crt-011 not landed — penalty tests produce misleading results on stale confidence data | High | Medium | High |
| R-06 | SQL aggregation diverges from Rust iteration on NULL/malformed entries | Medium | Medium | Medium |
| R-07 | Briefing k env var parsing accepts invalid values (0, negative, huge) | Medium | Low | Low |
| R-08 | StatusAggregates active_entries loading misses tag association | Medium | Medium | Medium |
| R-09 | Co-access boost + status penalty interaction — deprecated entry boosted past active | High | Low | Medium |
| R-10 | Removing episodic.rs field from AdaptationService changes its public API contract | Medium | Low | Low |
| R-11 | Comparison test (AC-10) passes on small test datasets but SQL diverges at scale | Low | Low | Low |
| R-12 | Env var UNIMATRIX_BRIEFING_K parsed at construction — runtime changes ignored silently | Low | Low | Low |

## Risk-to-Scenario Mapping

### R-01: W_COAC Removal Breaks Confidence Invariants
**Severity**: High
**Likelihood**: Low
**Impact**: Confidence computation silently changes behavior if W_COAC was used in an undiscovered path, or test suite fails on weight-sum assertions.

**Test Scenarios**:
1. After removing W_COAC and `co_access_affinity()`, `cargo test` compiles with zero errors — Rust compiler proves no callers exist
2. `weight_sum_stored_invariant` test still passes asserting sum = 0.92
3. Confidence computation for an entry produces identical values before and after removal (no behavioral change)

**Coverage Requirement**: Full workspace compilation + existing confidence unit tests pass unchanged (minus deleted tests)

### R-02: Episodic Removal Breaks Compilation
**Severity**: Medium
**Likelihood**: Low
**Impact**: Build failure due to unresolved imports of `EpisodicAugmenter` or `episodic_adjustments()`.

**Test Scenarios**:
1. After removing `episodic.rs`, `pub mod episodic` from `lib.rs`, and field/method from `service.rs` — workspace compiles cleanly
2. `grep -r "episodic" --include="*.rs"` returns zero matches across workspace

**Coverage Requirement**: Clean compilation. Grep verification as AC-01.

### R-03: Status Penalty Insufficient at Extreme Similarity Gaps
**Severity**: High
**Likelihood**: Medium
**Impact**: Deprecated entry with very high similarity (0.99) outranks active entry with moderate similarity (0.70), defeating the purpose of status penalties.

**Test Scenarios**:
1. Deprecated entry (similarity ~0.95) vs active entry (similarity ~0.70) — active still ranks above deprecated after 0.7 penalty. Math: deprecated `0.95*0.7=0.665` vs active `0.70*1.0=0.70`. Active wins, but barely.
2. Superseded entry (similarity ~0.99) vs active entry (similarity ~0.50) — superseded `0.99*0.5=0.495` vs active `0.50*1.0=0.50`. Active wins. Verify.
3. Extreme edge: deprecated at similarity 1.0, active at 0.69 — deprecated `1.0*0.7=0.70` vs active `0.69`. Deprecated wins. Document this as an accepted limitation.

**Coverage Requirement**: Tests T-SP-01 and T-SP-02 must cover the moderate-gap case (scope examples). Document the extreme-gap crossover point where penalties become insufficient. Assert on relative ranking per ADR-003.

### R-04: Penalty Test Flakiness from Non-Deterministic Embeddings
**Severity**: High
**Likelihood**: Medium
**Impact**: Tests intermittently fail because ONNX embedding pipeline doesn't produce embeddings with exact target cosine similarity, making ranking assertions unstable.

**Test Scenarios**:
1. Inject pre-computed embedding vectors with known cosine similarity (e.g., unit vectors with controlled dot product) — bypass ONNX pipeline entirely
2. Assert relative ranking (deprecated entry's position > active entry's position in results), not absolute score values
3. Verify test passes 100 times in a row (determinism check — can be done as CI smoke test)

**Coverage Requirement**: All T-SP-xx tests use injected embeddings per architecture design (SR-06 mitigation). No test depends on ONNX model output. No assertion references constants 0.7 or 0.5 per ADR-003.

### R-05: crt-011 Dependency Not Landed
**Severity**: High
**Likelihood**: Medium
**Impact**: Component 2 tests validate penalty behavior against entries whose confidence values were computed with duplicated session counts, producing misleading pass/fail results.

**Test Scenarios**:
1. Component 2 tests inject deterministic confidence values directly on entries (e.g., `confidence = 0.65`) rather than relying on confidence computation pipeline
2. Gate check: implementation does not begin until crt-011 merged and CI green (process control, not test)
3. If crt-011 introduces confidence regressions, crt-013 penalty tests should still pass because they're isolated from live confidence

**Coverage Requirement**: Zero dependency on live confidence computation in Component 2 tests. Confidence values set via direct entry mutation in test fixtures.

### R-06: SQL Aggregation Diverges from Rust Iteration
**Severity**: Medium
**Likelihood**: Medium
**Impact**: `context_status` returns different counts after optimization. Trust source distribution could differ if Rust defaults empty string differently than SQL CASE expression.

**Test Scenarios**:
1. Comparison test (AC-10): create dataset with entries covering all edge cases — NULL supersedes, empty trust_source, empty created_by, high correction_count, mixed statuses
2. Run both old (full-scan) and new (SQL aggregation) paths on same dataset, assert field-by-field equality on StatusAggregates
3. Include entry with `trust_source = ""` — verify both paths map to `"(none)"`
4. Include entry with `created_by = ""` AND `created_by IS NULL` (if possible) — verify both paths count them

**Coverage Requirement**: Comparison test with at least 10 entries covering all field combinations. Field-by-field equality assertion per ADR-004.

### R-07: Briefing k Env Var Parsing Edge Cases
**Severity**: Medium
**Likelihood**: Low
**Impact**: Invalid env var value (0, negative, "abc", very large) causes panic at service construction or unbounded memory allocation.

**Test Scenarios**:
1. `UNIMATRIX_BRIEFING_K` not set — default 3
2. `UNIMATRIX_BRIEFING_K=5` — uses 5
3. `UNIMATRIX_BRIEFING_K=0` — clamped to 1 (minimum)
4. `UNIMATRIX_BRIEFING_K=100` — clamped to 20 (maximum)
5. `UNIMATRIX_BRIEFING_K=abc` — fallback to default 3, no panic

**Coverage Requirement**: Unit tests for `BriefingService::new()` with various env var values. Clamp range [1, 20] verified.

### R-08: StatusAggregates Active Entries Missing Tags
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Lambda computation requires entry tags for outcome stats. If `load_active_entries_with_tags()` loads entries without their tags, coherence dimensions relying on tag data will compute incorrectly.

**Test Scenarios**:
1. Create active entries with tags, call `load_active_entries_with_tags()`, verify tags are populated
2. Compare tag data from new method vs old full-scan path — tags must match

**Coverage Requirement**: Integration test verifying active entries include correct tags. Part of AC-10 comparison test.

### R-09: Co-Access Boost + Status Penalty Interaction
**Severity**: High
**Likelihood**: Low
**Impact**: Deprecated entry excluded from co-access boost (correct), but if the exclusion logic has a bug, a deprecated entry could receive boost that pushes it above a penalized-but-active entry.

**Test Scenarios**:
1. T-SP-04: Query with deprecated entry that has strong co-access history — verify it receives zero boost
2. Active entry with co-access history receives boost, deprecated entry with identical co-access does not — active's final score includes boost, deprecated's does not
3. Verify `deprecated_ids` parameter correctly populated in `compute_search_boost()` call

**Coverage Requirement**: Test T-SP-04 explicitly. Integration test verifies the full pipeline interaction: penalty applied AND boost excluded for deprecated entries.

### R-10: AdaptationService API Change from Episodic Removal
**Severity**: Medium
**Likelihood**: Low
**Impact**: Removing `episodic` field from `AdaptationService` struct changes its constructor signature. Any external test or caller constructing `AdaptationService` directly will fail.

**Test Scenarios**:
1. After removal, all `AdaptationService::new()` call sites compile (constructor signature change)
2. `grep -r "AdaptationService" --include="*.rs"` — verify all construction sites updated

**Coverage Requirement**: Clean compilation across workspace. Rust compiler catches all constructor mismatches.

### R-11: Comparison Test Passes on Small Data, Diverges at Scale
**Severity**: Low
**Likelihood**: Low
**Impact**: SQL rounding or overflow differences only manifest with large datasets (e.g., `SUM(correction_count)` overflow for u32 vs u64).

**Test Scenarios**:
1. Include entry with `correction_count = u32::MAX` in comparison test dataset — verify SQL SUM doesn't overflow
2. Include 100+ entries to test GROUP BY accuracy beyond trivial counts

**Coverage Requirement**: At least one entry with extreme `correction_count` value in comparison test.

### R-12: Briefing k Parsed at Construction Only
**Severity**: Low
**Likelihood**: Low
**Impact**: Changing `UNIMATRIX_BRIEFING_K` env var after server start has no effect. Operator expects runtime configurability but gets construction-time only.

**Test Scenarios**:
1. Document in code comment that `semantic_k` is read at construction time
2. No test needed — this is a documentation/expectation risk, not a correctness risk

**Coverage Requirement**: Code comment on `BriefingService::new()` stating env var is read once at construction.

## Integration Risks

### IR-01: Component 1 + Component 2 Interaction
Removing `co_access_affinity()` (Component 1) while adding co-access exclusion tests (Component 2) touches adjacent code in `confidence.rs` and `coaccess.rs`. Risk of merge conflicts or accidentally breaking the co-access boost mechanism that Component 2 tests rely on.

**Mitigation**: Component 1 (removal) should be implemented before Component 2 (tests). Tests validate the surviving mechanisms, not the removed ones.

### IR-02: Component 4 Store Methods + StatusService Coupling
New `compute_status_aggregates()` and `load_active_entries_with_tags()` are called from `StatusService`. The integration boundary is the `StatusAggregates` struct. If struct fields don't match what `StatusService` expects, compilation fails — low risk. The real risk is semantic mismatch (e.g., `entries_without_attribution` counts different rows than old code).

**Mitigation**: AC-10 comparison test covers this explicitly.

### IR-03: Search Pipeline Ordering
The search pipeline applies: HNSW → fetch → filter/penalty → rerank → co-access boost → truncate. Component 2 tests must understand this ordering — a test that checks ranking before co-access boost is applied would see different results than after. Tests must assert on final results, not intermediate state.

**Mitigation**: All T-SP-xx tests go through the full `SearchService` interface, not internal functions.

## Edge Cases

| ID | Edge Case | Component | Expected Behavior |
|----|-----------|-----------|-------------------|
| EC-01 | Query matches only deprecated entries | C2 | Flexible mode returns results (penalized). Strict mode returns empty. |
| EC-02 | Query matches zero entries | C2 | Empty results in both modes (existing behavior, unchanged) |
| EC-03 | Entry is both superseded and deprecated | C2 | Superseded penalty (0.5) takes precedence or compounds. Verify which. |
| EC-04 | Briefing k > total entries in store | C3 | Returns all available entries, no panic |
| EC-05 | Briefing k = 1 | C3 | Returns single best candidate |
| EC-06 | All entries have empty trust_source | C4 | Trust distribution: `{"(none)": N}` |
| EC-07 | Zero entries in store | C4 | StatusAggregates with all zeros, empty active_entries |
| EC-08 | Entry with supersedes pointing to non-existent entry | C4 | Still counted in `entries_with_supersedes` (just counts non-NULL) |
| EC-09 | Co-access pair where both entries are deprecated | C2 | Neither receives boost, pair is effectively inert |

## Security Risks

### SEC-01: Env Var Parsing (Low Risk)
`UNIMATRIX_BRIEFING_K` is parsed as `usize`. Malicious env var values are bounded by:
- Type parsing (`usize::from_str` rejects non-numeric)
- Clamping to [1, 20]
- Construction-time only (no runtime injection)

**Blast radius**: At worst, briefing returns more candidates than intended (k=20 vs k=3). No data corruption or privilege escalation.

### SEC-02: SQL Aggregation Queries (Low Risk)
New SQL queries in `compute_status_aggregates()` use no external input — they aggregate over existing table data. No injection vector. Queries are string literals in Rust code, not parameterized from user input.

**Blast radius**: None. Internal read-only queries.

### SEC-03: Dead Code Removal (None)
Removing `co_access_affinity()` and `episodic.rs` reduces attack surface. No new untrusted input accepted.

## Failure Modes

| ID | Failure | Expected Behavior |
|----|---------|-------------------|
| FM-01 | `compute_status_aggregates()` SQL error | Return `StoreError`, `context_status` returns error to caller. No silent data loss. |
| FM-02 | `UNIMATRIX_BRIEFING_K` env var unparseable | Fall back to default k=3. Log warning. No panic. |
| FM-03 | Active entries query returns empty (no active entries) | Lambda computation skipped (existing behavior — lambda requires active entries). StatusAggregates has empty `active_entries`. |
| FM-04 | Co-access boost computation receives empty `deprecated_ids` | All entries eligible for boost (current behavior when no deprecated entries exist). Correct. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: W_COAC removal cascades | R-01 | Mitigated by ADR-001 (Option A). Architecture confirms grep shows W_COAC only in `confidence.rs`, `co_access_affinity()` only in test call sites. Compiler is final arbiter. |
| SR-02: crt-011 dependency not landed | R-05 | Mitigated by architecture decision: Component 2 tests inject deterministic confidence values, isolating from live confidence computation. Process gate: crt-011 must be merged before implementation. |
| SR-03: SQL equivalence without proof | R-06, R-11 | Mitigated by ADR-004 (single StatusAggregates method) and AC-10 comparison test. Both paths run on same dataset with field-by-field diff. NULL handling matched via SQL CASE expressions. |
| SR-04: Episodic removal breaks imports | R-02, R-10 | Mitigated by architecture grep confirming 3 files affected. Rust compiler catches all remaining references. Low residual risk. |
| SR-05: Briefing k scope creep | R-07, R-12 | Mitigated by minimal design: field + env var, no config struct. Clamp to [1, 20]. Follows existing pattern (no server-wide config exists). |
| SR-06: Integration test determinism | R-04 | Mitigated by ADR-003 (behavior-based tests). Pre-computed embeddings with known similarity injected. Ranking assertions only — no score value assertions. |
| SR-07: Missing indexes block optimization | R-11 | Mitigated by architecture analysis: queries scan full table (same I/O as before) avoiding deserialization. No new indexes needed at current scale. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-03, R-04, R-05) | 9 scenarios |
| Medium | 5 (R-01, R-06, R-08, R-09, R-10) | 10 scenarios |
| Low | 4 (R-02, R-07, R-11, R-12) | 8 scenarios |
| **Total** | **12** | **27 scenarios** |
