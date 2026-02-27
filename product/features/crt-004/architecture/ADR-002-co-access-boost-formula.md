## ADR-002: Co-Access Boost Formula (Log-Transform + Cap)

### Context

Co-access boost must translate a raw co-access count into a bounded score that influences search ranking. The boost must:
1. Be bounded to prevent co-access from overriding similarity (the primary signal)
2. Use diminishing returns to resist gaming (an agent cannot trivially inflate co-access counts to dominate rankings)
3. Be consistent with existing patterns in the codebase (crt-002's `usage_score` uses the same log-transform approach)

The co-access boost is additive to the existing rerank score (`0.85 * similarity + 0.15 * confidence`). The total score space is [0.0, 1.15] before boost (similarity and confidence both max at 1.0). The boost should be small enough that it only matters when results are close in similarity.

### Decision

**Log-transformed boost with hard cap**:

```
raw_boost = ln(1 + co_access_count) / ln(1 + MAX_MEANINGFUL_CO_ACCESS)
boost = min(raw_boost, 1.0) * MAX_CO_ACCESS_BOOST
```

Constants:
- `MAX_MEANINGFUL_CO_ACCESS = 20.0` -- beyond 20 co-retrievals, the signal saturates
- `MAX_CO_ACCESS_BOOST = 0.03` -- maximum additive boost for search results
- `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` -- maximum additive boost for briefing (very small influence)

At key points:
- 1 co-access: boost = 0.03 * (ln(2)/ln(21)) = ~0.007
- 5 co-accesses: boost = 0.03 * (ln(6)/ln(21)) = ~0.018
- 10 co-accesses: boost = 0.03 * (ln(11)/ln(21)) = ~0.024
- 20 co-accesses: boost = 0.03 * 1.0 = 0.030 (max)
- 100 co-accesses: boost = 0.030 (capped)

For context, the difference between a similarity of 0.90 and 0.87 in the rerank score is `0.85 * 0.03 = 0.0255`. A maximum co-access boost of 0.03 can overcome a ~3.5% similarity gap -- enough to promote clearly related entries but not enough to promote irrelevant ones.

### Consequences

- **Gaming resistance**: Log-transform means doubling co-access count from 10 to 20 only increases boost by ~25% (0.024 -> 0.030). Linear counting would double the boost.
- **Consistent with crt-002**: Same pattern as `usage_score()` which uses `ln(1 + count) / ln(1 + MAX)`. Agents familiar with one formula understand the other.
- **Tunable**: `MAX_CO_ACCESS_BOOST` is a named constant. Increasing it strengthens co-access influence; decreasing it weakens it. No algorithmic change needed.
- **Small effect size**: At max boost (0.03), co-access is roughly equivalent to a 3.5% difference in embedding similarity. This is the "tiebreaker" behavior intended by the scope.
