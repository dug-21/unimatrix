# Test Plan Overview: col-013 Extraction Rule Engine

## Test Strategy

Tests are organized by component and wave, with integration tests following unit tests.
All tests trace back to the Risk Register (R-01 through R-07) and Acceptance Criteria.

## Risk Mapping

| Risk | Test Coverage | Priority |
|------|--------------|----------|
| R-01 (low-quality entries) | Quality gate unit tests (all 6 checks), extraction rule unit tests | P0 |
| R-02 (silent tick failure) | Background tick integration test, tick_metadata update test | P0 |
| R-03 (CRT regressions) | trust_score, contradiction, coherence_by_source unit tests | P0 |
| R-04 (observation query perf) | Watermark pattern test with synthetic data | P1 |
| R-05 (write contention) | Covered by existing edge_cases suite | P1 |
| R-06 (type migration) | cargo check --workspace after migration | P0 |
| R-07 (rate limit reset) | Rate limit unit test with hour boundary | P0 |

## Integration Harness Plan (infra-001)

### Mandatory Gate
```
cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60
```

### Existing Suites to Run
| Suite | Why |
|-------|-----|
| smoke | Mandatory minimum gate |
| tools | context_status field changes |
| confidence | trust_score "auto" = 0.35 |
| lifecycle | auto-entry searchability |
| contradiction | refactor regression |
| edge_cases | concurrent ops with background tick |

### New Integration Tests (7 total)
| Test ID | Suite | Description |
|---------|-------|-------------|
| T-S10 | test_tools.py | Status reports maintenance timing fields |
| T-S11 | test_tools.py | Status reports extraction_stats field |
| T-S12 | test_tools.py | Status reports coherence_by_source field |
| T-S13 | test_tools.py | maintain=true silently ignored |
| C-21 | test_confidence.py | Auto-trust entry has lower confidence |
| L-26 | test_lifecycle.py | Auto-extracted entry is searchable |
| L-27 | test_lifecycle.py | Auto-extracted entry appears in briefing |

### Test Execution Order (Stage 3c)
1. cargo test --workspace
2. pytest -m smoke
3. pytest suites/test_tools.py
4. pytest suites/test_confidence.py
5. pytest suites/test_lifecycle.py
6. pytest suites/test_contradiction.py
7. pytest suites/test_edge_cases.py

## Test Count Expectations

| Component | New Unit Tests | New Integration Tests |
|-----------|---------------|----------------------|
| type-migration | 4 (trust_score, contradiction) | 0 |
| extraction-rules | ~18 (rules + quality gate) | 0 |
| background-tick | 3 (status fields, maintain ignore) | 5 (tick, maintenance, extraction) |
| infra-001 | -- | 7 (new tests in Python suites) |
| **Total** | **~25** | **~12** |
