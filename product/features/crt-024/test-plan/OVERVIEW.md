# Test Plan Overview: crt-024 (Ranking Signal Fusion — WA-0)

## Overall Test Strategy

crt-024 is a scoring formula replacement confined to two files: `search.rs` (pipeline) and
`config.rs` (weight fields + validation). There is no schema change, no new MCP tool, no new
external dependency. The test strategy reflects this:

- **Unit tests dominate**: The core deliverable (`compute_fused_score`, `FusionWeights::effective`,
  `InferenceConfig::validate` additions) is pure logic — no async, no I/O. Pure unit tests are
  the primary verification instrument.
- **Integration tests verify MCP-visible behavior**: Co-access signal reaching the scorer, score
  range in returned `ScoredEntry.final_score`, NLI-enabled vs. NLI-absent output differences.
- **No new integration suites required**: The formula change is internal to the search pipeline.
  Existing `tools` and `lifecycle` suites exercise `context_search` end-to-end. New integration
  tests are additions to those existing suites, not a new suite.
- **Regression audit is mandatory**: Every pre-crt-024 test asserting a specific `final_score`
  value must be updated, not deleted (R-04, AC-08). Net test count must not decrease.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Location | Test Type | Scenarios |
|---------|----------|---------------|-----------|-----------|
| R-01 | Critical | `compute-fused-score` | unit | util_norm boundaries: -0.05→0.0, 0.0→0.5, +0.05→1.0 |
| R-02 | High | `compute-fused-score` | unit | zero-denominator guard in `FusionWeights::effective(nli=false)` |
| R-03 | High | `compute-fused-score` | unit | `PROVENANCE_BOOST==0.0` guard: prov_norm=0.0 without panic |
| R-04 | Critical | `search-service` | unit+audit | All pre-crt-024 score assertions updated; git audit confirms no deletions |
| R-05 | Critical | `search-service` | unit | apply_nli_sort behaviors migrated to fused scorer coverage |
| R-06 | High | `compute-fused-score` | unit | ADR-003 Constraints 9+10 + AC-11 as named unit tests |
| R-07 | High | `search-service` | integration | boost_map prefetch: known co-access entry scores non-zero coac_norm |
| R-08 | High | `search-service` | grep+unit | MAX_CO_ACCESS_BOOST import-only; coac_norm boundary arithmetic |
| R-09 | High | `compute-fused-score` | unit | NLI-enabled path: no re-normalization; weights used as configured |
| R-10 | High | `search-service` | compile+unit | try_nli_rerank return type: Option<Vec<NliScores>>; behavioral test |
| R-11 | High | `compute-fused-score` | unit | Ineffective entry: util_norm=0.0; fused_score >= 0.0 |
| R-12 | Med | `inference-config` | unit | validate() cannot be bypassed; tests construct InferenceConfig directly |
| R-13 | Med | `inference-config` | unit | Config without new fields: all six parse to defaults, sum<=0.95 |
| R-14 | Med | `compute-fused-score` | unit | status_penalty applied after compute_fused_score, not inside |
| R-15 | Med | `search-service` | unit | NliScores index alignment with candidates slice |
| R-16 | Low | `score-structs` | compile | FusedScoreInputs/FusionWeights struct fields are named, extensible |
| R-NEW | High | `search-service` | unit | EvalServiceLayer wiring: w_sim=1.0 profile produces score==sim*penalty |

---

## Cross-Component Test Dependencies

| Dependency | Upstream | Downstream | Constraint |
|-----------|----------|------------|------------|
| `FusionWeights::effective` | `score-structs` | `compute-fused-score`, `search-service` | Must be tested before fused scorer integration |
| `compute_fused_score` purity | `compute-fused-score` | `search-service` | Extract as standalone function (ADR-004); search-service tests call it directly |
| `InferenceConfig` weight fields | `inference-config` | `search-service`, `score-structs` | Default values must be verified before testing the formula uses them |
| Pre-crt-024 score assertions | `search-service` | all | Existing tests must be updated before new tests run alongside them |

---

## Integration Harness Plan

### Suite Selection

crt-024 touches `SearchService` — store/retrieval behavior and search tool logic. Based on the
suite selection table:

| Suite | Run? | Reason |
|-------|------|--------|
| `smoke` | YES (mandatory gate) | Any change at all |
| `tools` | YES | Search tool logic is directly modified |
| `lifecycle` | YES | Store→search multi-step flows exercise the full pipeline including scoring |
| `confidence` | YES | Confidence signal is a weighted term in the fused formula |
| `edge_cases` | YES | Empty DB operations, concurrent ops cover the degenerate scoring paths |
| `protocol` | NO | No protocol or handshake changes |
| `contradiction` | NO | No contradiction detection changes |
| `security` | NO | No content scanning or capability changes |
| `volume` | NO | Scoring formula change has no scale-specific behavior differences |

### Existing Suite Coverage of crt-024 Behavior

The existing suites cover crt-024 concerns as follows:

| crt-024 Risk | Covered by existing suite? | Notes |
|-------------|---------------------------|-------|
| Single fused scoring pass (AC-04) | Partially (tools/lifecycle) | search returns ordered results; ordering is verifiable |
| Score range [0,1] in ScoredEntry | Partially (confidence suite) | confidence suite checks `final_score` in [0,1] |
| co-access signal reaching scorer | NO — new test needed | No existing test constructs known co-access state and verifies its effect on ranking |
| NLI-active vs NLI-absent scoring difference | Partially (tools suite) | NLI-related search tests exist but do not assert score differences |
| BriefingService untouched (AC-14) | YES (tools suite) | Briefing tests pass without modification |

### New Integration Tests Needed

Two behavioral gaps require new integration tests added to existing suite files:

**Addition 1: `suites/test_lifecycle.py` — co-access signal verification**

```python
def test_search_coac_signal_reaches_scorer(shared_server):
    """
    R-07: Verify co-access boost reaches the fused scorer (non-zero coac_norm).
    Store entry A. Simulate co-access by accessing A and a companion entry together
    (via multiple context_search calls using the same agent_id). Then search again
    and assert that A's final_score is higher than an entry with identical signals
    but no co-access history.
    Fixture: shared_server (state must accumulate across calls to build co-access).
    """
```

**Addition 2: `suites/test_tools.py` — NLI-absent vs NLI-active score divergence**

```python
def test_search_nli_absent_uses_renormalized_weights(server):
    """
    R-09, AC-06: When NLI model is absent, the five non-NLI weights are re-normalized
    and the returned final_score is in [0,1]. Assert score is finite and non-negative
    for all returned ScoredEntry items.
    Fixture: server (NLI absent at cold start).
    """
```

### When NOT to Add Integration Tests

- Formula arithmetic correctness (pure unit tests suffice — ADR-004 guarantees testability)
- Weight validation error messages (unit tests on InferenceConfig suffice)
- apply_nli_sort migration coverage (unit tests in search.rs suffice)
- PROVENANCE_BOOST guard (constant guard — no MCP-visible effect that requires integration testing)

---

## Test File Locations

| Component | Unit Tests | Notes |
|-----------|-----------|-------|
| `InferenceConfig` weight fields | `crates/unimatrix-server/src/infra/config.rs` (existing `#[cfg(test)]` block) | Follow `NliFieldOutOfRange` test pattern |
| `FusedScoreInputs`, `FusionWeights`, `ScoreWeights::effective` | `crates/unimatrix-server/src/services/search.rs` (existing `#[cfg(test)]` block) | Adjacent to implementation |
| `compute_fused_score` | `crates/unimatrix-server/src/services/search.rs` | Pure function — no async, no fixtures |
| `SearchService` pipeline | `crates/unimatrix-server/src/services/search.rs` | Update existing `apply_nli_sort` tests → fused scorer tests |
| Integration: co-access | `product/test/infra-001/suites/test_lifecycle.py` | New test |
| Integration: NLI-absent | `product/test/infra-001/suites/test_tools.py` | New test |

---

## Test Execution Order (Stage 3c)

1. `cargo test --workspace 2>&1 | tail -30` — unit tests (must pass with zero failures)
2. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` — mandatory gate
3. `python -m pytest suites/test_tools.py suites/test_lifecycle.py suites/test_confidence.py suites/test_edge_cases.py -v --timeout=60`
4. Triage any failures per USAGE-PROTOCOL.md decision tree
5. File GH Issues + add `xfail` for any pre-existing failures unrelated to this feature
