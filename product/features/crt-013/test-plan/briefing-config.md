# Test Plan: Configurable Briefing Neighbor Count

## Unit Tests (R-07)

### parse_semantic_k Tests

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| default_when_unset | env var not set | 3 | R-07 |
| configured_value | UNIMATRIX_BRIEFING_K=5 | 5 | R-07 |
| clamp_zero_to_minimum | UNIMATRIX_BRIEFING_K=0 | 1 | R-07 |
| clamp_large_to_maximum | UNIMATRIX_BRIEFING_K=100 | 20 | R-07 |
| invalid_value_fallback | UNIMATRIX_BRIEFING_K=abc | 3 | R-07 |
| clamp_one_accepted | UNIMATRIX_BRIEFING_K=1 | 1 | R-07 |
| clamp_twenty_accepted | UNIMATRIX_BRIEFING_K=20 | 20 | R-07 |

**Note**: Tests that set env vars must use a serial test guard or test the parsing logic directly (not via env var) to avoid test flakiness from parallel execution. Preferred approach: test the `parse_semantic_k()` function by temporarily setting/unsetting the env var within a single-threaded test, or extract the parsing into a pure function that takes `Option<&str>` as input.

### Briefing Service Integration

| Test | Assertion | AC |
|------|-----------|-----|
| existing briefing tests pass | All T-BS-xx tests unchanged | AC-08 |
| briefing_uses_configured_k | BriefingService constructed with k=5, verify it's wired through | AC-08 |

## Edge Cases

| Edge | Test | Expected |
|------|------|----------|
| EC-04: k > total entries | Implicit in existing tests (small stores) | Returns all available, no panic |
| EC-05: k = 1 | clamp_one_accepted | Returns single result |
| R-12: construction-time only | Code comment | Documented, no test |
