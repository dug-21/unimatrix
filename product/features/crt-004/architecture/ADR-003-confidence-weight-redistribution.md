## ADR-003: Confidence Weight Redistribution Strategy

### Context

crt-002 established a six-factor additive confidence formula with weights summing to 1.0:
- Base: 0.20, Usage: 0.15, Freshness: 0.20, Helpfulness: 0.15, Correction: 0.15, Trust: 0.15

crt-004 adds a seventh factor: co-access affinity. The SCOPE resolves that this follows crt-002's research guidance. The key constraint: weights must still sum to 1.0.

Options:
1. **Proportional reduction**: Reduce all six weights proportionally to free up budget for co-access
2. **Selective reduction**: Reduce only the less-important factors
3. **Split integration**: Keep six factors in `compute_confidence` (summing to < 1.0) and add co-access as a separate additive term at query time

### Decision

**Split integration** (Option 3). The six existing factors are reduced proportionally by the co-access weight (0.08), so the stored confidence from `compute_confidence` produces values in [0.0, 0.92]. The co-access affinity (up to 0.08) is added at query time.

New weights for `compute_confidence`:
```
W_BASE  = 0.20 * (0.92/1.0) = 0.184 -> rounded to 0.18
W_USAGE = 0.15 * (0.92/1.0) = 0.138 -> rounded to 0.14
W_FRESH = 0.20 * (0.92/1.0) = 0.184 -> rounded to 0.18
W_HELP  = 0.15 * (0.92/1.0) = 0.138 -> rounded to 0.14
W_CORR  = 0.15 * (0.92/1.0) = 0.138 -> rounded to 0.14
W_TRUST = 0.15 * (0.92/1.0) = 0.138 -> rounded to 0.14
Sum: 0.18 + 0.14 + 0.18 + 0.14 + 0.14 + 0.14 = 0.92
W_COAC  = 0.08 (applied at query time)
Total effective: 0.92 + 0.08 = 1.00
```

The co-access affinity function:
```
co_access_affinity(partner_count, avg_partner_confidence) =
    W_COAC * min(ln(1 + partner_count) / ln(1 + MAX_MEANINGFUL_PARTNERS), 1.0)
            * avg_partner_confidence
```

Where `MAX_MEANINGFUL_PARTNERS = 10`. An entry with 10+ co-access partners with average confidence 1.0 gets the full 0.08 boost. An entry with no co-access partners gets 0.0.

### Consequences

- **No function signature change**: `compute_confidence(&EntryRecord, u64) -> f32` is unchanged. The function pointer in `record_usage_with_confidence` works as before.
- **Stored confidence slightly lower**: Max stored confidence drops from 1.0 to 0.92. Entries without co-access data will have slightly lower effective confidence than before. The magnitude is small (8% reduction at the theoretical maximum).
- **Existing tests need updating**: Confidence boundary tests that assert specific values will need new expected values. The weight sum test changes from asserting 1.0 to asserting 0.92 for the six stored factors.
- **Query-time overhead**: Each search/briefing result requires a CO_ACCESS lookup for the co-access affinity. At 5-10 results, this is 5-10 additional table lookups. Per ADR-001, each lookup is O(total_pairs).
- **Proportional reduction is fairest**: No single factor is disproportionately penalized. The relative ordering of factors (base > freshness > usage = helpfulness = correction = trust) is preserved.
