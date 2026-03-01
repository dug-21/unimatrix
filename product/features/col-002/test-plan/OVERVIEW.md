# Test Plan Overview: col-002 Retrospective Pipeline

## Test Strategy

| Level | Scope | Tools |
|-------|-------|-------|
| Unit | unimatrix-observe crate (parser, attribution, detection, metrics, files, types) | cargo test -p unimatrix-observe |
| Unit | unimatrix-store OBSERVATION_METRICS methods | cargo test -p unimatrix-store |
| Unit | unimatrix-server error/response/validation extensions | cargo test -p unimatrix-server |
| Integration | context_retrospective e2e, context_status observation fields | infra-001 suites |
| Shell | Hook script JSONL output validation | bash pipe tests |

## Risk-to-Test Mapping

| Risk | Priority | Test Location | Coverage |
|------|----------|---------------|----------|
| R-01 (JSONL parsing drops records) | High | observe-parser unit tests | 4 scenarios |
| R-02 (Attribution misattributes) | High | observe-attribution unit tests | 6 scenarios |
| R-03 (Timestamp parsing edge cases) | Medium | observe-parser unit tests | 5 scenarios |
| R-04 (MetricVector bincode breaks) | Medium | observe-types unit tests | 5 scenarios |
| R-05 (DetectionRule trait extensibility) | Medium | observe-detection unit tests | 5 scenarios |
| R-06 (OBSERVATION_METRICS regression) | Medium | store-observation unit tests | 4 scenarios |
| R-07 (Hook scripts fail silently) | Medium | hooks shell tests | 4 scenarios |
| R-08 (File cleanup deletes active files) | Medium | observe-files unit tests | 4 scenarios |
| R-09 (Concurrent retrospective calls) | Medium | server-retrospective integration | 2 scenarios |
| R-10 (Permission retries false positives) | Low | observe-detection unit tests | 3 scenarios |
| R-11 (Phase name extraction) | Low | observe-metrics unit tests | 4 scenarios |
| R-12 (Directory permissions) | Medium | hooks shell tests | 2 scenarios |
| R-13 (Large session files) | Medium | observe-parser unit tests | 2 scenarios |
| R-14 (StatusReport test churn) | Low | compile verification | automatic |

## Cross-Component Test Dependencies

1. server-retrospective tests depend on unimatrix-observe being functional
2. server-status-ext tests depend on observe-files functions
3. Integration tests depend on the compiled server binary including context_retrospective
4. Hook shell tests are independent of Rust code

## Integration Harness Plan

### Existing Suites That Apply

| Suite | Relevance | Why |
|-------|-----------|-----|
| `tools` | HIGH | New context_retrospective tool must be discovered and callable |
| `protocol` | MEDIUM | Tool discovery should list the new tool |
| `lifecycle` | MEDIUM | Schema changes (new table) must not break existing flows |
| `smoke` | MANDATORY | Minimum gate -- critical paths must still work |

### Gap Analysis

The following behaviors have no existing suite coverage:

1. **context_retrospective tool** -- entirely new tool, no existing tests
2. **context_status observation fields** -- new fields in status output
3. **OBSERVATION_METRICS table** -- new storage surface

### New Integration Tests Needed (Stage 3c)

Add to `suites/test_tools.py`:

```python
# context_retrospective tests
def test_retrospective_no_data_returns_error(server):
    """FR-09.7: empty dir + no stored MV -> error"""

def test_retrospective_returns_report_structure(server):
    """FR-09.5: verify report has expected fields"""

def test_retrospective_cached_result(server):
    """FR-09.6: store MV, call again with no new data, verify is_cached"""
```

Add to `suites/test_lifecycle.py`:

```python
def test_retrospective_stores_metrics(shared_server):
    """FR-09.4: verify MetricVector persisted after analysis"""
```

Add to `suites/test_tools.py` (status extension):

```python
def test_status_includes_observation_fields(server):
    """FR-11.1: verify observation fields in status output"""
```

### Test Convention Notes

- Use `server` fixture for stateless tests (fresh DB)
- Use `shared_server` for tests that need persisted metrics
- Observation directory is the default path; integration tests may not have observation files, so expect no-data responses
- Hook scripts are NOT tested through integration harness (they are shell scripts tested separately)
