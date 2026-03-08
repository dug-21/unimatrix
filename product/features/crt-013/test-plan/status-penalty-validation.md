# Test Plan: Status Penalty Validation

## Integration Tests (R-03, R-04, R-05, R-09)

All tests in `crates/unimatrix-server/src/services/search_penalty_tests.rs`.

### Test Design Principles

1. Pre-computed embeddings with controlled cosine similarity (R-04 mitigation)
2. Deterministic confidence values set directly on entries (R-05 mitigation)
3. Assertions on relative ranking only — no score constants (ADR-003)
4. Full search pipeline execution via SearchService (not internal functions)

### Test Cases

| Test | Risk | AC | Assertion |
|------|------|----|-----------|
| T-SP-01: deprecated_below_active_flexible | R-03 | AC-04 | Active entry ranks above deprecated entry at comparable similarity |
| T-SP-02: superseded_below_active_flexible | R-03 | AC-04 | Active entry ranks above superseded entry |
| T-SP-03: strict_mode_excludes_deprecated_superseded | R-03 | AC-05 | Deprecated and superseded absent from Strict mode results |
| T-SP-04: deprecated_excluded_from_coaccess_boost | R-09 | AC-06 | Deprecated entry receives no co-access boost |
| T-SP-05: deprecated_only_query_returns_results | R-03 | AC-07 | Non-empty results in Flexible mode for deprecated-only matches |
| T-SP-06: superseded_successor_injection | R-03 | AC-04 | Injected successor ranks above superseded entry |

### Embedding Construction

Use unit vectors in 384-dim space with controlled dot product:
- `vec_query`: unit vector along dimension 0
- `vec_high_sim`: vector with ~0.95 cosine similarity to query
- `vec_moderate_sim`: vector with ~0.70 cosine similarity to query

Construction: mix query direction with orthogonal direction in known proportions.

### Entry Setup Pattern

```
Each test:
1. Create fresh store + services
2. Insert entries with known status, confidence values
3. Insert pre-computed embeddings into vector index
4. Execute search
5. Assert ranking order
```

### Edge Cases

| Edge | Test | Expected |
|------|------|----------|
| EC-01: Only deprecated matches | T-SP-05 | Non-empty results |
| EC-03: Both superseded and deprecated | T-SP-01 | Superseded penalty harsher (0.5 < 0.7) |
| EC-09: Both entries in co-access pair deprecated | T-SP-04 | Neither gets boost |

## Assertions That Must NOT Appear

- `assert_eq!(score, 0.7)` or any hardcoded penalty constant
- `assert_eq!(score, 0.598)` or any computed score value
- Any reference to constants `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY` in assertions

## Assertions That MUST Appear

- `assert!(results[i].entry.id == active_id)` (ranking order)
- `assert!(results.iter().all(|r| r.entry.status == Status::Active))` (strict exclusion)
- `assert!(!results.is_empty())` (deprecated-only query)
