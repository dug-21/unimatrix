# col-014: Risk-Test Strategy

## Risk Register

### R-01: False Positive Feature Extraction (from SR-01)

**Severity**: Medium | **Likelihood**: Low | **Overall**: Low

Broader validation accepts more tokens as feature IDs from free text. Hyphenated English words (e.g., "well-known") could be extracted.

**Test coverage**:
- Unit: Verify common hyphenated English words are technically accepted (expected -- they pass safety gating)
- Integration: Verify attribution partitioning tolerates occasional false signals (existing `test_attribute_two_feature_session` pattern)
- Acceptance: End-to-end attribution with suffixed feature IDs produces correct results

**Mitigation**: Attribution's partition-based approach means false positives only matter if they dominate a session. Single-record noise does not create feature switches.

### R-02: Regression on Existing Feature IDs (from implementation)

**Severity**: High | **Likelihood**: Very Low | **Overall**: Low

All existing feature IDs (`col-002`, `nxs-001`, `eng-001`, `spike-042`, `api-100`) must remain valid after the change.

**Test coverage**:
- `test_is_valid_feature_id_positive` (existing, unchanged)
- `test_extract_feature_id_pattern_accepts_arbitrary_prefixes` (existing, unchanged)
- `test_attribute_sessions_with_arbitrary_prefix_feature` (existing, unchanged)

### R-03: Injection via Feature ID Tokens (from SR-01, safety)

**Severity**: High | **Likelihood**: Very Low | **Overall**: Low

Feature IDs extracted from observation data could contain malicious content if validation is too permissive.

**Test coverage**:
- `test_is_valid_feature_id_special_chars`: Verify `a]b-c`, `feat<script>-1`, `col-001;drop` are rejected
- Character allowlist (alphanumeric + hyphen + underscore + dot) blocks injection characters

## Scope Risk Traceability

| Scope Risk | Architecture Response | Test Coverage |
|------------|----------------------|---------------|
| SR-01: False positive increase | Hyphen requirement, partition tolerance | R-01 tests |
| SR-02: Dot in paths | Path segment extraction isolates dots | R-01 integration tests + existing path tests |
| SR-03: Underscore ambiguity | Hyphen requirement blocks pure-underscore | Covered by `test_is_valid_feature_id_no_hyphen` |

## Test Strategy Summary

| Category | Count | Coverage Target |
|----------|-------|-----------------|
| Unit: validation rules | 8 tests (6 new, 2 updated) | All 5 validation rules |
| Unit: existing regression | 5 tests (unchanged) | Existing positive/negative cases |
| Integration: E2E attribution | 1 test (new) | Suffixed feature in full pipeline |
| **Total** | 14 tests | |

## Top 3 Risks by Severity

1. **R-03**: Injection via feature ID tokens (High severity, Very Low likelihood)
2. **R-02**: Regression on existing feature IDs (High severity, Very Low likelihood)
3. **R-01**: False positive feature extraction (Medium severity, Low likelihood)
