# Test Plan Overview: crt-019 — Confidence Signal Activation

## Overall Test Strategy

crt-019 spans two crates (`unimatrix-engine` and `unimatrix-server`) and introduces seven
coordinated changes. Testing must cover three layers:

1. **Unit tests** — pure formula functions in `unimatrix-engine/src/confidence.rs` and new
   server-layer structs/services. Every new or changed function gets direct unit tests with
   concrete arithmetic assertions.
2. **Pipeline tests** — end-to-end formula validation using `tests/pipeline_calibration.rs` and
   `tests/pipeline_regression.rs`. These tests use realistic entry profiles and verify ordering
   invariants across the full scoring pipeline.
3. **Integration tests** — the infra-001 harness exercises the compiled binary through the MCP
   protocol. Required for behaviors invisible to unit tests: empirical prior flow through closure
   to stored confidence (R-01), store-layer dedup behavior for doubled access (R-11), and
   adaptive blend effect on search result ordering (R-02).

### Risk-Driven Priorities

The test strategy is ordered by risk priority from RISK-TEST-STRATEGY.md:

| Priority | Risks | Testing Approach |
|----------|-------|-----------------|
| P1 (Critical) | R-01 | Integration test — closure type change requires end-to-end proof |
| P2 (High) | R-02, R-03, R-04, R-05, R-06, R-07, R-10, R-11, R-12 | Unit + pipeline + store-layer |
| P3 (Medium) | R-08, R-13, R-14, R-15, R-16 | Code review + unit tests |
| P4 (Low) | R-09, R-17 | Code review + verify constant removal |

---

## Risk-to-Test Mapping

| Risk ID | Description | Test Location | Test Name(s) |
|---------|-------------|---------------|--------------|
| R-01 | compute_confidence closure vs. bare fn pointer | `services/usage.rs` + integration | `test_empirical_prior_flows_through_closure`, integration R-01 scenario |
| R-02 | rerank_score call sites miss confidence_weight | `confidence.rs` unit + pipeline_retrieval.rs | `test_rerank_score_adaptive_weight_*`, T-RET-01 updated |
| R-03 | Weight sum f64 exactness | `confidence.rs` unit | `weight_sum_invariant_f64` (updated) |
| R-04 | T-REG-02 updated after weight change | `pipeline_regression.rs` | T-REG-02 first-commit ordering check |
| R-05 | Cold-start threshold at 9 vs 10 | `confidence.rs` or `status.rs` | `test_prior_below_threshold_9`, `test_prior_at_threshold_10` |
| R-06 | ConfidenceState initial observed_spread | `services/confidence.rs` unit | `test_confidence_state_initial_observed_spread`, `test_confidence_state_initial_weight` |
| R-07 | UsageDedup fires before access_weight | `services/usage.rs` unit | `test_context_lookup_doubled_access_second_call_zero` |
| R-08 | context_get spawns second task | Code review + existing NFR-04 test | `test_record_access_fire_and_forget_returns_quickly` |
| R-09 | RwLock write contention | Code review | No dedicated test; lock acquisition uses poison recovery pattern |
| R-10 | base_score(Proposed, "auto") regression | `confidence.rs` unit | `auto_proposed_base_score_unchanged` |
| R-11 | Store deduplicates duplicate IDs | Store-layer unit + integration | `test_store_record_usage_duplicate_ids`, AC-08b integration |
| R-12 | Method-of-moments degeneracy (zero variance) | `status.rs` unit | `test_prior_zero_variance_all_helpful`, `test_prior_zero_variance_all_unhelpful`, `test_helpfulness_nan_inputs` |
| R-13 | Duration guard checked after (not before) update | Code review + unit | Guard placement assertion in refresh loop test |
| R-14 | bayesian_helpfulness(2,2,3,3) == 0.5 not > 0.5 | `confidence.rs` unit | AC-02 Bayesian assertions |
| R-15 | ConfidenceState not wired into SearchService | Integration | IR-01 integration test |
| R-16 | Confidence spread target not reachable | Pipeline calibration | `T-CAL-SPREAD-01` synthetic scenario |
| R-17 | MINIMUM_SAMPLE_SIZE / WILSON_Z not removed | Grep + compile | Verify no reference to removed constants |

---

## Implementation Ordering Guard (R-04 / C-02)

**The test plan flags this as a mandatory implementation ordering requirement:**

The first commit (or first hunk) of the implementation diff MUST update T-REG-02 in
`pipeline_regression.rs` to the new weight values BEFORE any weight constant changes in
`confidence.rs`. This is not a test to write — it is a process invariant that the
implementing agent must enforce. The test plan records this guard explicitly.

New T-REG-02 assertions (to replace current values):
```
assert_eq!(W_BASE, 0.16, "W_BASE changed");
assert_eq!(W_USAGE, 0.16, "W_USAGE changed");
assert_eq!(W_FRESH, 0.18, "W_FRESH unchanged");
assert_eq!(W_HELP, 0.12, "W_HELP changed");
assert_eq!(W_CORR, 0.14, "W_CORR unchanged");
assert_eq!(W_TRUST, 0.16, "W_TRUST changed");
```

Also update T-REG-02's sum check to use `assert_eq!` (exact equality) rather than tolerance,
consistent with the new `weight_sum_invariant_f64` requirement (AC-11).

---

## Cross-Component Test Dependencies

| Dependency | Consumer | Provider | Note |
|------------|----------|----------|------|
| `helpfulness_score` new 4-param signature | pipeline_calibration `ablation_pair` | `confidence.rs` | ablation_pair "helpfulness" case must pass alpha0/beta0 |
| `base_score` new 2-param signature | pipeline_calibration `confidence_with_adjusted_weight` | `confidence.rs` | line 94 must become `base_score(entry.status, &entry.trust_source)` |
| `compute_confidence` new 4-param signature | `pipeline_regression.rs`, `pipeline_calibration.rs`, `services/status.rs`, `services/usage.rs` | `confidence.rs` | all call sites must pass alpha0/beta0 |
| `rerank_score` new 3-param signature | `pipeline_retrieval.rs`, `services/search.rs` | `confidence.rs` | SEARCH_SIMILARITY_WEIGHT import in pipeline_retrieval.rs must be replaced |
| `ConfidenceState` initialization | `services/search.rs` read path, `services/status.rs` write path | `services/confidence.rs` | must initialize with observed_spread = 0.1471 |
| `UsageContext.access_weight` field | `mcp/tools.rs` all 3 construction sites | `services/usage.rs` | default must be 1, not 0 (EC-04) |
| `MINIMUM_VOTED_POPULATION` constant | empirical prior computation | `services/status.rs` or `services/confidence.rs` | must equal 10 (ADR-002 authoritative) |

---

## Integration Harness Plan (infra-001)

### Mandatory Gate

`python -m pytest suites/ -v -m smoke --timeout=60` must pass before Gate 3c.
Run from `product/test/infra-001/`.

### Relevant Existing Suites

This feature touches confidence system, server tool logic (context_get, context_lookup), and
store/retrieval behavior. The following suites apply:

| Suite | Reason | Priority |
|-------|--------|----------|
| `smoke` | Minimum gate — always | Mandatory |
| `confidence` | 6-factor formula, re-ranking, base scores per status | High — formula changes |
| `tools` | context_get implicit vote, context_lookup doubled access | High — tool behavior changes |
| `lifecycle` | Confidence evolution across multiple steps; state accumulation | High — R-01 end-to-end |
| `edge_cases` | Empty DB spread computation, boundary values for prior threshold | Medium |

Suites `protocol`, `security`, `contradiction`, and `volume` are not directly affected by
crt-019 changes and are not required beyond the smoke gate. They should be run as part of the
pre-merge full suite.

### Existing Suite Coverage Gaps for crt-019

The existing infra-001 suites validate the current confidence system (Wilson score, fixed
0.85/0.15 blend). After crt-019, these tests will exercise the new formula. Some existing tests
may need updates:

| Suite | Potential Gap | Action |
|-------|--------------|--------|
| `confidence` | Tests may assert Wilson score behavior or the 0.15 fixed confidence weight | Update assertions in Stage 3c to match Bayesian formula and adaptive blend |
| `lifecycle` | `confidence_evolution` scenario may assert specific numeric confidence values | Update numeric assertions to match new formula outputs |
| `tools` | No tests for doubled access_count on context_lookup or implicit helpful on context_get | Add new tests (see below) |

### New Integration Tests to Write (Stage 3c)

These tests must be added to the infra-001 suites as part of crt-019:

#### 1. `test_empirical_prior_flows_to_stored_confidence` (R-01 Critical)

**Suite**: `suites/test_lifecycle.py`
**Fixture**: `server`
**Scenario**:
1. Store 10+ entries via `context_store`, each with skewed helpfulness signals (simulate via
   multiple `context_get` calls with `helpful: true`).
2. Trigger maintenance via `context_status` with `maintain: true`.
3. Read back confidence values via `context_get` for the entries.
4. Assert that entries with all-helpful signals have confidence materially above the cold-start
   neutral (> cold-start output), demonstrating the empirical prior was used, not cold-start.
**Purpose**: Proves the closure type change works — empirical alpha0/beta0 flows from
`ConfidenceState` through `UsageService` to stored confidence. Unit tests cannot catch the
bare-function-pointer silent failure.

```python
@pytest.mark.smoke
def test_empirical_prior_flows_to_stored_confidence(server):
    # Store 10+ entries with helpful votes, trigger maintenance,
    # assert confidence reflects empirical prior (not cold-start neutral)
    ...
```

#### 2. `test_context_lookup_doubled_access_count` (R-11, AC-08b)

**Suite**: `suites/test_tools.py`
**Fixture**: `server`
**Scenario**:
1. Store an entry via `context_store`, capture the ID.
2. Call `context_lookup` with the ID using a fresh agent.
3. Wait for spawn_blocking (small sleep or poll).
4. Call `context_get` on the entry, inspect `access_count` in response metadata.
5. Assert `access_count == 2` (not 1).
6. Call `context_lookup` again with same agent.
7. Assert `access_count` remains 2 (dedup suppresses second increment).
**Purpose**: Verifies store-layer does not deduplicate duplicate IDs (R-11) and that
dedup-before-multiply ordering is correct (R-07).

```python
def test_context_lookup_doubled_access_count(server):
    # First lookup: access_count == 2; second lookup same agent: access_count still == 2
    ...
```

#### 3. `test_context_get_implicit_helpful_vote` (AC-08a)

**Suite**: `suites/test_tools.py`
**Fixture**: `server`
**Scenario**:
1. Store an entry, capture ID.
2. Call `context_get` with `helpful: null` (omitted) for a fresh agent.
3. Wait, then read back the entry.
4. Assert `helpful_count == 1`.
5. Call `context_get` with `helpful: false`.
6. Assert `helpful_count` remains 1 (no increment on explicit false).
**Purpose**: Verifies FR-06 implicit vote injection.

```python
def test_context_get_implicit_helpful_vote(server):
    # helpful_count increments on first get with helpful=null, not on helpful=false
    ...
```

#### 4. `test_adaptive_blend_weight_changes_with_spread` (R-02, AC-06)

**Suite**: `suites/test_confidence.py`
**Fixture**: `server`
**Scenario**:
1. Query `context_search` before any maintenance — verify results are ordered.
2. Trigger maintenance (populate spread from real data if possible).
3. Assert that the re-ranking formula uses `confidence_weight > 0.15` on initial server start
   (because `observed_spread` initializes to 0.1471, giving weight 0.184).
**Note**: This is harder to test at the MCP level since `confidence_weight` is not exposed.
The test may instead verify result ordering under a controlled dataset where the ordering
differs between weight=0.15 and weight=0.184.

```python
def test_search_uses_adaptive_confidence_weight(server):
    # Verify result ordering reflects confidence_weight > 0.15
    ...
```

### Suite Selection Command

```bash
cd product/test/infra-001

# Mandatory smoke gate
python -m pytest suites/ -v -m smoke --timeout=60

# Feature-specific suites
python -m pytest suites/test_confidence.py suites/test_tools.py suites/test_lifecycle.py suites/test_edge_cases.py -v --timeout=60
```

---

## Test File Summary

| File | Tests Added | Tests Updated | Tests Removed |
|------|-------------|---------------|---------------|
| `confidence.rs` (unit tests) | Bayesian assertions (AC-02), base_score 2-param, `adaptive_confidence_weight`, `auto_proposed_base_score_unchanged`, prior degeneracy | T-05 Wilson tests, T-11 rerank_score 2-param | `wilson_reference_*` tests (3), `MINIMUM_SAMPLE_SIZE` / `WILSON_Z` constant tests |
| `pipeline_regression.rs` | — | T-REG-02 weight constants, T-REG-01 golden values | — |
| `pipeline_calibration.rs` | `auto_vs_agent_spread` (AC-12), `T-CAL-SPREAD-01` | `confidence_with_adjusted_weight` helper, `ablation_pair` helpfulness case | — |
| `pipeline_retrieval.rs` | — | T-RET-01 (remove `SEARCH_SIMILARITY_WEIGHT` import, pass confidence_weight) | — |
| `services/usage.rs` (unit) | `test_context_get_implicit_helpful_vote`, `test_context_lookup_doubled_access`, `test_usage_context_default_access_weight` | Existing UsageContext construction sites (add `access_weight: 1`) | — |
| `services/confidence.rs` (unit) | `test_confidence_state_initial_spread`, `test_confidence_state_initial_weight`, `test_confidence_state_update_atomicity` | — | — |
| `status.rs` (unit) | `test_empirical_prior_computation_*` (5 scenarios), `test_refresh_loop_duration_guard` | — | — |
| infra-001 `test_tools.py` | `test_context_lookup_doubled_access_count`, `test_context_get_implicit_helpful_vote` | Existing confidence assertions | — |
| infra-001 `test_lifecycle.py` | `test_empirical_prior_flows_to_stored_confidence` | `confidence_evolution` if it asserts numeric values | — |
| infra-001 `test_confidence.py` | `test_search_uses_adaptive_confidence_weight` | Wilson score assertions | — |
