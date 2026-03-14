# Risk-Based Test Strategy: crt-019 — Confidence Signal Activation

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `compute_confidence` bare function pointer must become a capturing closure — the store's `record_usage_with_confidence` currently takes `Option<&dyn Fn(&EntryRecord, u64) -> f64>`; the new signature requires `alpha0`/`beta0` to be captured, forcing a type change to `Box<dyn Fn(...) + Send>`; a missed or incorrect change silently compiles but ignores the new prior | Critical | High | P1 |
| R-02 | `rerank_score` call sites omit the new `confidence_weight` parameter — 4 call sites in search.rs plus pipeline_retrieval.rs; any site that does not compile after adding the parameter will fail loudly, but if a refactor leaves a fallback (e.g. a default or old constant) the bug is silent; the risk is using 0.85 effectively at one site while others use the adaptive weight | High | Med | P2 |
| R-03 | Weight sum f64 exactness — the new vector `{0.16, 0.16, 0.18, 0.12, 0.14, 0.16}` must sum to exactly `0.92_f64`; IEEE 754 binary64 addition is not commutative; if the constants are summed in a different order in the invariant test vs. the code, or if any literal is mistyped, the `weight_sum_invariant_f64` assertion fails with no useful diagnostic and the formula produces a wrong total | High | Med | P2 |
| R-04 | T-REG-02 updated after weight change instead of before — C-02 mandates updating the golden assertions first; if a developer changes constants first, T-REG-02 fails immediately in CI, but worse: a partial implementation (some weights changed, others not) may accidentally produce a sum that passes the old assertion while being internally inconsistent | High | High | P2 |
| R-05 | Bayesian prior cold-start threshold discrepancy — SPEC (C-08 / FR-09) specifies `≥ 5` voted entries; ARCHITECTURE.md (ADR-002, Component 3) specifies `≥ 10`; the implemented threshold determines when the potentially-unstable empirical prior activates; if the implementation uses 5, it activates sooner and risks unstable estimates; if it uses 10, the SPEC assertion in C-08 is violated | High | High | P2 |
| R-06 | `observed_spread` initial value before first tick — SPEC documents `0.1471` as the initial value (pre-crt-019 measured value), giving `confidence_weight = 0.184` before the first tick completes; if the implementation initializes `ConfidenceState` with `observed_spread = 0.0`, `confidence_weight = clamp(0, 0.15, 0.25) = 0.15` — a silent regression to the exact pre-existing floor with no improvement until maintenance runs | High | Med | P2 |
| R-07 | `UsageDedup` fires before or after the `access_weight` multiplier — ADR-004 and SPEC C-05 confirm dedup-before-multiply, but the implementation site in `record_mcp_usage` must apply dedup first and multiply after; inverted order (multiply then dedup) would deduplicate a doubled list, producing 0 on the second call regardless; the `test_context_lookup_doubled_access` second-call assertion directly tests this | High | Med | P2 |
| R-08 | `context_get` implicit helpful vote spawns a second `spawn_blocking` task — C-04 mandates the vote is injected via `UsageContext.helpful` before the single existing task; a second task would reintroduce the blocking-pool regression fixed in vnc-010 (Unimatrix #771); the diff must show zero new `spawn_blocking` / `tokio::task::spawn*` additions to the `context_get` handler | High | Low | P3 |
| R-09 | `ConfidenceState` RwLock write contention — the background tick holds the write lock while updating four f64 fields (microseconds); concurrent search reads (`confidence_weight` clone) are blocked during this window; at high concurrency, read starvation is possible if tokio's `RwLock` does not give reader preference; in practice with 4-field writes this is negligible, but a missed `read()` hold that spans a search loop iteration (not just a clone) would serialize search calls | Med | Low | P4 |
| R-10 | `base_score(Status::Proposed, "auto")` regression — ADR-003 explicitly limits differentiation to `Active`; if the `if trust_source == "auto"` branch is placed outside the `Active` arm, Proposed entries with auto trust_source get 0.35 instead of 0.5, breaking T-REG-01 ordering `auto > stale` where `auto_extracted_new()` uses `Proposed` | High | Med | P2 |
| R-11 | `store.record_usage_with_confidence` deduplicates IDs internally — ADR-004 notes this as an unresolved risk; if the store deduplicates the `access_ids` list before the `UPDATE` loop, passing the same ID twice (the `flat_map` repeat approach) produces access_count += 1 instead of += 2; the ×2 signal silently vanishes; must be verified before the implementation is merged | High | High | P2 |
| R-12 | Empirical prior method-of-moments degeneracy — when all voted entries have identical helpfulness rates (e.g., all `p_i = 1.0`), the sample variance is 0; the formula `α₀ = μ * (μ*(1-μ)/σ² - 1)` produces division by zero or ±∞; the clamp `[0.5, 50.0]` must apply before any assignment to prevent NaN propagation into `helpfulness_score` and thus `compute_confidence` | High | Med | P2 |
| R-13 | Duration guard checked after instead of before `update_confidence` call — FR-05 specifies the guard must be checked before each call; if checked after, the last iteration executes even when over budget, and a slow entry (e.g., heavy confidence computation) can push the batch 200ms+ over the wall-clock limit; the intent is a hard pre-check | Med | Med | P3 |
| R-14 | `bayesian_helpfulness(2, 2, 3.0, 3.0)` assertion in AC-02 corrected from SCOPE — SCOPE AC-02 stated `> 0.5`; SPEC AC-02 corrected to `== 0.5` because `(2+3)/(4+6) = 0.5` exactly; if the implementation targets the original SCOPE assertion (>`0.5`) it will be wrong; the test must assert exact equality | Med | High | P3 |
| R-15 | `ConfidenceState` not wired into `SearchService` — the `ServiceLayer::new` constructor must thread `ConfidenceStateHandle` to both `StatusService` (writer) and `SearchService` (reader); missing the `SearchService` wire leaves `confidence_weight` at the initial default indefinitely; `SearchService` compiles fine with a default but never adapts | Med | Med | P3 |
| R-16 | Confidence spread target AC-01 not reachable with new formula on existing data — the spread target ≥ 0.20 requires the new formula to differentiate the actual live entry population; if the population is dominated by entries that score identically under the new weights (e.g., all zero-usage, zero-vote, `"agent"` active), the spread remains below 0.20; the synthetic calibration test `T-CAL-SPREAD-01` may pass while the live database fails AC-01 | Med | Med | P3 |
| R-17 | `MINIMUM_SAMPLE_SIZE` and `WILSON_Z` constants not removed — they are referenced in existing unit tests (T-05 helpfulness tests); if the constants are left in the file but unused, the code compiles but the old test assertions continue to pass against removed logic, creating dead test coverage; the removal must be complete and the tests replaced | Low | Low | P4 |

---

## Risk-to-Scenario Mapping

### R-01: compute_confidence closure vs. function pointer
**Severity**: Critical
**Likelihood**: High
**Impact**: If the store's `record_usage_with_confidence` retains `Option<&dyn Fn(&EntryRecord, u64) -> f64>`, any call from `UsageService` will use cold-start defaults `α₀=3, β₀=3` permanently, silently ignoring empirical prior updates. The helpfulness component never improves beyond the neutral 0.5 floor for unvoted entries. The feature's primary goal is undermined.

**Test Scenarios**:
1. Unit test: call `record_usage_with_confidence` with a captured closure that uses non-default `alpha0=2.0, beta0=8.0`; assert the stored confidence reflects those priors and not cold-start defaults.
2. Integration test: populate 10 voted entries with skewed helpfulness (all helpful), trigger a maintenance tick, then record a new access; assert that the newly-computed confidence for the accessed entry uses the empirical prior (score > 0.5 neutral), not the cold-start default.
3. Compiler-level check: assert `store.rs` signature for `record_usage_with_confidence` accepts `Box<dyn Fn(...) + Send>` or equivalent closure type, not a bare function pointer.

**Coverage Requirement**: At least one integration test must prove empirical prior values flow from `ConfidenceState` through `UsageService` to stored confidence. A unit test alone (which can mock the closure) is insufficient.

---

### R-02: rerank_score parameter threading — missing weight at any call site
**Severity**: High
**Likelihood**: Med
**Impact**: One call site using a stale constant (e.g. 0.85 from a local binding) while others use the adaptive weight produces inconsistent re-ranking within a single search result set. The initial sort (line 275) and co-access re-sort (lines 327–328) would use different weights, causing non-monotonic ordering that cannot be attributed to data.

**Test Scenarios**:
1. Integration test: after a maintenance tick that computes `observed_spread = 0.20`, call `context_search` and assert that the top result is the entry with the highest `(0.75 * sim + 0.25 * conf)` — not the highest under 0.85/0.15.
2. Compile-time: remove `SEARCH_SIMILARITY_WEIGHT` constant entirely (the architecture mandates this) — any remaining reference to the old constant fails to compile.
3. Unit test for each `rerank_score` call site pattern: verify `rerank_score(0.9, 0.8, 0.25)` returns `(0.75 * 0.9 + 0.25 * 0.8) = 0.875` not `(0.85 * 0.9 + 0.15 * 0.8) = 0.885`.

**Coverage Requirement**: An end-to-end integration test that verifies result ordering changes when `confidence_weight` changes from 0.15 to 0.25 on a fixed dataset.

---

### R-03: Weight sum f64 exactness
**Severity**: High
**Likelihood**: Med
**Impact**: If the new constants do not sum to exactly `0.92_f64`, the `weight_sum_invariant_f64` test fails at CI and blocks merge. More critically, the formula output is wrong — confidence values will be uniformly offset, affecting all entries. The resulting spread may still be ≥ 0.20 but the absolute values are wrong relative to the co-access boost (0.08).

**Test Scenarios**:
1. The existing `weight_sum_invariant_f64` test — must pass with updated constants.
2. Verify that `0.16_f64 + 0.16_f64 + 0.18_f64 + 0.12_f64 + 0.14_f64 + 0.16_f64 == 0.92_f64` in a standalone assertion (all values are exact in IEEE 754 binary64).
3. Verify `compute_confidence` on a known entry profile produces a confidence clamped to [0.0, 1.0] — not exceeding 1.0 due to sum errors.

**Coverage Requirement**: `weight_sum_invariant_f64` plus at least one golden-value assertion on `compute_confidence` with known inputs.

---

### R-04: T-REG-02 update ordering
**Severity**: High
**Likelihood**: High
**Impact**: If weight constants are changed before T-REG-02 is updated, CI fails immediately in an obvious way. The real risk is subtler: during development, a partially-updated constant file (some weights changed) passes T-REG-02's old assertions by coincidence (e.g., sum still happens to match), while the formula is internally inconsistent. The developer may merge without noticing.

**Test Scenarios**:
1. Verify that T-REG-02 contains assertions for the new constant values (0.16, 0.12, 0.16, 0.16) — not the old values (0.18, 0.14, 0.14, 0.14).
2. Verify T-REG-01 passes with the `auto_extracted_new()` profile after the combined weight + base_score changes.
3. PR review checklist: T-REG-02 update must appear as the first commit or first hunk in the implementation diff.

**Coverage Requirement**: T-REG-02 passing after weight changes is the primary coverage; T-REG-01 ordering verification is secondary.

---

### R-05: Cold-start threshold discrepancy (SPEC ≥5 vs. ARCH ≥10)
**Severity**: High
**Likelihood**: High
**Impact**: This is an unresolved contradiction between specification and architecture. SPEC C-08 and FR-09 mandate `≥ 5` voted entries. ADR-002 mandates `≥ 10`. The implemented value determines when empirical estimation activates. At threshold=5, a sparse population of 5 entries with skewed votes produces unstable α₀/β₀ that propagates to all 192+ entry confidence scores on every refresh tick. At threshold=10, entries 5–9 voted fall back to cold-start, producing a neutral 0.5 floor longer than necessary.

**Test Scenarios**:
1. **Canonical resolution test**: the implementation must assert the chosen threshold explicitly in a named constant (`MINIMUM_VOTED_POPULATION: usize`); the RISK-TEST-STRATEGY designates the ADR-002 threshold of `≥ 10` as authoritative (the architect raised it from 5 with explicit rationale about population stability — see ADR-002 — and the spec should be updated to match).
2. Unit test: call prior-estimation function with exactly 9 voted entries → assert cold-start defaults returned.
3. Unit test: call prior-estimation function with exactly 10 voted entries → assert empirical estimation is attempted.
4. Unit test: call with 10 identical-rate entries (σ²=0) → assert clamp applied, no NaN/panic.

**Coverage Requirement**: Explicit boundary tests at 9 and 10 voted entries; degeneracy test at zero variance.

---

### R-06: ConfidenceState initial value before first tick
**Severity**: High
**Likelihood**: Med
**Impact**: If `ConfidenceState` initializes with `observed_spread = 0.0`, the adaptive blend starts at `confidence_weight = 0.15` (the floor), giving no improvement until the first maintenance tick completes. The SPEC explicitly states the initial value is `0.1471`, giving `confidence_weight = 0.184` immediately on server start. The difference is observable in search result ordering from the first query.

**Test Scenarios**:
1. Unit test: construct a fresh `ConfidenceState::default()` or `ConfidenceState::new()` and assert `observed_spread == 0.1471` and `confidence_weight ≈ 0.184`.
2. Integration test: start the service without triggering a maintenance tick; call `context_search`; assert that the re-ranking uses `confidence_weight > 0.15` (i.e., the initial value, not the zero default).

**Coverage Requirement**: `ConfidenceState` initialization test is required; the integration test is strongly recommended.

---

### R-07: UsageDedup order relative to access_weight multiplier
**Severity**: High
**Likelihood**: Med
**Impact**: If the multiplier is applied before dedup filtering, a doubled ID list is passed to `filter_access`; the dedup may retain the first occurrence and suppress the second, resulting in access_count += 1 instead of += 2 for new entries. If dedup is inverted entirely, repeated lookups by the same agent accumulate infinite access_count increments.

**Test Scenarios**:
1. Unit test AC-08b scenario 1: new agent, new entry, `context_lookup` called once → assert `access_count == 2`, `helpful_count == 0`.
2. Unit test AC-08b scenario 2: same agent, same entry, `context_lookup` called twice in one session → assert `access_count` remains `2` after second call (dedup suppresses entirely).
3. Unit test: same entry, two different agents each call `context_lookup` → assert `access_count == 4` (two agents × 2 each).

**Coverage Requirement**: All three scenarios above are required. Scenario 2 specifically tests the dedup-before-multiply invariant.

---

### R-08: context_get implicit helpful vote spawns additional task
**Severity**: High
**Likelihood**: Low
**Impact**: A second `spawn_blocking` task in the `context_get` handler reinvokes the blocking thread pool saturation that vnc-010 fixed (Unimatrix #771). Under concurrent load, pool exhaustion blocks all MCP tool calls. The symptom is intermittent 50ms+ latency spikes on all tools, not just `context_get`. Hard to diagnose post-merge.

**Test Scenarios**:
1. Code review assertion: `context_get` handler diff must show zero new `spawn_blocking`, `tokio::spawn`, `spawn_blocking_with_mandate`, or `task::spawn*` calls.
2. Unit test NFR-04 compliance: `test_record_access_fire_and_forget_returns_quickly` (existing 50ms bound) must pass after the `helpful: Some(true)` injection; if the injection adds I/O on the calling thread, this test will fail.
3. Verify `UsageContext` construction in `context_get` shows `helpful: params.helpful.or(Some(true))` — not a separate vote call.

**Coverage Requirement**: Code review is the primary gate; the existing 50ms latency test provides secondary coverage.

---

### R-10: base_score Proposed/auto regression
**Severity**: High
**Likelihood**: Med
**Impact**: T-REG-01 asserts `auto > stale`; `auto_extracted_new()` uses `Status::Proposed` with `trust_source: "auto"`; `stale` uses `Status::Deprecated` → base_score 0.2. If the `auto` branch is outside the `Active` match arm, Proposed/auto gets 0.35 instead of 0.5, and the combined score may fall below `stale`. T-REG-01 fails loudly, but only if the test is run — during development with T-REG-02 being updated first (C-02), a developer might temporarily disable tests.

**Test Scenarios**:
1. Explicit unit test `auto_proposed_base_score_unchanged`: `base_score(Status::Proposed, "auto") == 0.5`.
2. T-REG-01 must pass end-to-end with the new formula; specifically the `good > auto` assertion with `auto_extracted_new()` profile.
3. Calibration scenario AC-12 `auto_vs_agent_spread` must use `Status::Active` entries — verify this scenario does not accidentally test Proposed status.

**Coverage Requirement**: The explicit `base_score(Proposed, "auto")` unit test is the critical coverage for this risk.

---

### R-11: store.record_usage_with_confidence internal ID deduplication
**Severity**: High
**Likelihood**: High
**Impact**: ADR-004 explicitly flags this as unverified. If the store's SQL loop deduplicates IDs before `UPDATE access_count = access_count + 1`, passing ID X twice produces access_count += 1, not += 2. The ×2 deliberate-retrieval signal silently vanishes. AC-08b passes at the `UsageService` layer but fails at the database layer — only a test that reads back from the store can detect this.

**Test Scenarios**:
1. Integration test: call `context_lookup` for a single entry with a fresh agent, wait for `spawn_blocking` completion, read the entry back from the store, assert `access_count == 2`.
2. Store-layer unit test: call `record_usage_with_confidence` with `entry_ids = [42, 42]` (duplicate), assert the entry's `access_count` increases by 2 (not 1).
3. If the store deduplicates: verify the fallback implementation (explicit `(id, increment)` pairs or `update_access_count(id, 2)`) works correctly.

**Coverage Requirement**: A store-layer test with duplicate IDs in the list is required before the `flat_map` repeat approach is committed. This is the highest-risk unresolved implementation question in the feature.

---

### R-12: Method-of-moments degeneracy (zero variance population)
**Severity**: High
**Likelihood**: Med
**Impact**: When all voted entries have identical helpfulness rates, `σ² = 0`, and the formula computes `α₀ = μ * (μ*(1-μ)/0 - 1)` = NaN or ±∞. If NaN propagates into `helpfulness_score`, the confidence formula returns NaN for all entries. NaN confidence values produce undefined re-ranking behavior (NaN comparisons in sort are undefined). The clamp `[0.5, 50.0]` is the only guard.

**Test Scenarios**:
1. Unit test: call method-of-moments estimation with 10 entries all at `p_i = 1.0` (all helpful) → assert `alpha0` clamped to 50.0, `beta0` clamped to 0.5 (or vice versa per the formula).
2. Unit test: same with all `p_i = 0.0` (all unhelpful).
3. Unit test: call `helpfulness_score(0, 0, f64::NAN, f64::NAN)` → assert result is clamped to [0.0, 1.0] and not NaN (defense-in-depth).
4. Property test: `compute_confidence` must never return NaN or values outside [0.0, 1.0] for any valid input combination.

**Coverage Requirement**: Zero-variance degeneracy test is required. NaN propagation test for `helpfulness_score` is required.

---

## Integration Risks

### IR-01: ConfidenceState wiring through ServiceLayer
`ConfidenceStateHandle` must be injected into both `StatusService` (writer) and `SearchService` (reader) via `ServiceLayer::new`. A partial wire (only one service gets the handle) means either writes never happen (search always reads initial value) or reads always see initial value (writes happen but search doesn't observe them). The `ServiceLayer::new` constructor change is a high-blast-radius edit — all tests that construct `ServiceLayer` must be updated.

**Test Scenarios**: Integration test that constructs a full `ServiceLayer`, triggers a maintenance tick, then calls `context_search` and asserts the `confidence_weight` used differs from the cold-start default.

### IR-02: compute_confidence call in status.rs refresh loop
The architecture specifies that `alpha0`/`beta0` must be snapshotted from `ConfidenceState` once before the refresh loop begins, not re-read per entry. If `ConfidenceState.read()` is called inside the loop, the write lock is potentially re-acquired 500 times in one tick, serializing any concurrent search that needs a read lock. The snapshot pattern is both a correctness and performance requirement.

**Test Scenarios**: Code review to verify the snapshot is taken outside the loop. Performance test: 500-entry refresh loop must complete within 200ms with concurrent search calls running.

### IR-03: UsageContext field additions across all construction sites
`access_weight: u32` is a new field on `UsageContext`. All construction sites (tools.rs × 3, usage.rs tests × multiple) must add `access_weight: 1`. Missed sites cause a compile error in Rust (struct construction requires all fields), so this risk is compile-time-detectable. The risk is that a missed site uses `..Default::default()` or a builder pattern that silently sets `access_weight: 0`, which would suppress access_count increments entirely for that tool.

**Test Scenarios**: Verify `UsageContext` does not implement `Default` with `access_weight: 0`; either no `Default` impl, or `Default::default().access_weight == 1`.

---

## Edge Cases

### EC-01: Zero active entries during spread computation
If `observed_spread` computation runs on an empty active population (e.g., development environment), `p95 - p5` is undefined. The implementation must handle an empty slice by returning 0.0 (or the initial default), keeping `confidence_weight` at the floor (0.15).

### EC-02: Single voted entry at the threshold boundary
At exactly 9 voted entries: cold-start. At exactly 10: empirical. The method-of-moments with n=10 is marginally stable. With n=10 and one extreme outlier (9 entries at 0.5, 1 entry at 1.0), the prior shifts modestly. This is the intended behavior but must be verified.

### EC-03: `helpful_count` overflow on u32
`helpful_count` and `unhelpful_count` are `u32`. At 2^32 votes, the Bayesian formula overflows. This is unrealistic in practice but the formula `(helpful_count + alpha0) / (total_votes + alpha0 + beta0)` with `u32` counts must operate in f64 space (cast before division). Verify the implementation casts before arithmetic.

### EC-04: `access_weight: 0` on UsageContext
If a construction site sets `access_weight: 0`, the `flat_map(repeat(id).take(0))` produces an empty vec, suppressing the access_count increment entirely — the access is deduped but never recorded. This is a silent data loss. Default should be 1, not 0.

### EC-05: `context_lookup` returning zero results
When `context_lookup` returns no entries (all IDs invalid), the access recording receives an empty list. The doubled weight has no entries to multiply; no increment occurs. This is correct behavior but must not panic or produce unexpected side effects.

### EC-06: Confidence refresh at exactly the 200ms boundary
The duration guard uses `start.elapsed() > Duration::from_millis(200)`. An iteration that starts at exactly 200ms passes the check and runs. The semantics are "break if already over budget, not if this iteration will push over budget." This is correct per FR-05 but the test must verify the guard fires pre-iteration, not post.

---

## Security Risks

### SEC-01: Bayesian prior manipulation via vote injection
The empirical prior `α₀/β₀` is estimated from the voted-entry population. An adversary with write access who creates many entries with skewed votes (all helpful or all unhelpful) can shift the population mean and thus shift α₀/β₀ for all entries. The `[0.5, 50.0]` clamp on α₀/β₀ limits damage: at the extreme clamp, `α₀=50, β₀=0.5` gives unvoted entries a score of `50/50.5 ≈ 0.99` — an effective helpfulness inflation attack.
**Blast radius**: Medium. Limited by the 50.0 clamp. Mitigated by requiring votes via MCP tool (Admin or authenticated agent), not arbitrary writes.
**Test scenario**: Verify that with all voted entries at `p_i = 1.0` (fully helpful), `alpha0` clamps to 50.0 and unvoted entry score is `50/(50+0.5) ≈ 0.99` — confirm this is the worst-case and document it as accepted.

### SEC-02: access_weight field as untrusted input surface
`UsageContext.access_weight` is constructed internally by MCP tool handlers, not from external input. The field never appears in MCP parameter schemas. However, if a future change exposes `access_weight` as an MCP parameter, an agent could set `access_weight: 1000` to inflate `access_count` by 1000 per lookup. The current design is safe because the field is server-internal. Verify the field is not exposed in `LookupParams` or any public schema.

### SEC-03: `context_get` implicit vote and knowledge quality degradation
Agents that call `context_get` frequently on low-quality entries will inflate their `helpful_count`, raising their confidence score, causing them to rank higher in search, causing more agents to retrieve them — a feedback loop. The UsageDedup one-vote-per-agent protection limits each agent to one vote, so an individual agent cannot self-amplify. But if many agents retrieve the same low-quality entry (e.g., because it appears in search results), collective implicit votes raise its score. This is partially intentional (frequently-retrieved entries are useful), but partially a concern for entries that are retrieved for negative reasons (no better option available).
**Blast radius**: Low. UsageDedup limits per-agent amplification. Addressed by crt-020 (explicit unhelpful votes).

---

## Failure Modes

### FM-01: Maintenance tick fails partway through prior computation
If `run_maintenance` panics after the confidence refresh loop but before the `ConfidenceState` write, the state retains the previous tick's values. `observed_spread` and `confidence_weight` are stale by one tick cycle (15 minutes). Behavior: search continues using the prior adaptive weight — no crash, graceful degradation. The stale value is the last-known-good value, not zero.
**Expected behavior**: No panic propagation; next tick will attempt the computation again.

### FM-02: `store.record_usage_with_confidence` returns an error
The confidence update inside `spawn_blocking` can fail if the store connection is locked or the database is full. The existing fire-and-forget pattern logs but does not surface errors to the caller. After crt-019, the same semantics apply to the new Bayesian formula computation inside the closure. The MCP tool responds successfully; the confidence update is eventually-consistent.
**Expected behavior**: Tool returns success; confidence update is silently dropped; logged at warn level.

### FM-03: `ConfidenceState` read lock poisoned
`RwLock` in Rust poisons if a writer panics while holding the write lock. If `run_maintenance` panics during the `ConfidenceState.write()` critical section (extremely unlikely — it's a 4-field f64 write), subsequent `read()` calls will panic. The server becomes unable to serve search requests.
**Expected behavior**: The application should call `unwrap_or_else(|e| e.into_inner())` on lock acquisition (the existing pattern from `CategoryAllowlist` poison recovery documented in MEMORY.md) to recover from poisoned state. Verify this pattern is applied to `ConfidenceState` lock acquisitions.

### FM-04: Duration guard fires on first iteration (single slow entry)
If the first entry in the 500-entry batch takes > 200ms to process (pathological case), the duration guard fires before processing entry 2, and zero entries are updated despite the tick running. The system logs "0 entries updated" and continues. Not a crash, but confidence values are never refreshed. The next tick will attempt again.
**Expected behavior**: Log warning indicating 0 entries updated within budget; next tick proceeds normally.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (sparse voted population → unstable α₀/β₀) | R-05, R-12 | ADR-002 raises threshold to ≥10 (vs SPEC ≥5 — discrepancy is R-05); clamp [0.5, 50.0] guards degeneracy (R-12) |
| SR-02 (atomicity of observed_spread + α₀/β₀ within a tick) | R-09, FM-03 | ADR-001/ADR-002: single `RwLock<ConfidenceState>` write covers all four values atomically; R-09 covers read-starvation edge case |
| SR-03 (rerank_score constant vs. runtime parameter) | R-02, R-06, IR-01 | ADR-001: parameter-passing chosen; `SEARCH_SIMILARITY_WEIGHT` removed; R-02 covers missed call sites; R-06 covers wrong initial value |
| SR-04 (base_score Proposed/auto ordering risk) | R-10 | ADR-003: differentiation limited to `Active` only; T-REG-01 preserved; R-10 covers implementation error path |
| SR-05 (UsageDedup before/after multiplier) | R-07, R-11 | ADR-004: dedup-before-multiply confirmed; R-07 covers ordering; R-11 covers store-layer dedup of duplicate IDs |
| SR-06 (T-REG-02 update order) | R-04 | SPEC C-02 mandates update-first; R-04 covers violation of that ordering |
| SR-07 (implicit helpful vote spawns second task) | R-08 | SPEC C-04 and ARCH Component 5: fold into existing UsageContext.helpful; R-08 covers implementation error |
| SR-08 (confidence refresh 500-entry batch holds lock_conn 200ms) | IR-02 | ARCH confirms all MCP paths already mediated by spawn_blocking (post-vnc-010); IR-02 covers snapshot pattern inside refresh loop |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical (P1) | 1 | 3 scenarios (R-01) |
| High (P2) | 9 | 24 scenarios (R-02, R-03, R-04, R-05, R-06, R-07, R-10, R-11, R-12) |
| Medium (P3) | 5 | 10 scenarios (R-08, R-13, R-14, R-15, R-16) |
| Low (P4) | 2 | 4 scenarios (R-09, R-17) |

**Integration test scope recommendation**: The highest-value integration tests are:
1. **R-01** — empirical prior flows through closure to stored confidence (end-to-end)
2. **R-11** — store layer does not deduplicate duplicate IDs in access list (store-layer unit)
3. **R-07** — `context_lookup` dedup-before-multiply with same-agent second call
4. **R-02** — result ordering under adaptive blend differs from 0.85/0.15 fixed blend
5. **R-05** — boundary at exactly 9 and 10 voted entries for prior activation threshold

Tests R-01 and R-11 are the highest-risk because both are silent correctness failures — the feature ships and appears to work, but the core behavioral change (empirical prior, doubled access) is not active.
