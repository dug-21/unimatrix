# Scope Risk Assessment: crt-002

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Wilson score f32 precision loss. The Wilson formula involves `z^2/(4n^2)` terms that produce very small floats. With f32 arithmetic, intermediate values may lose precision for large n, producing incorrect lower bounds. | Med | Low | Architect should evaluate whether f64 intermediate computation with f32 final result is needed. Unit test Wilson at n=10000+. |
| SR-02 | Freshness decay makes stored confidence time-dependent. Between accesses, the freshness component becomes stale. If another feature reads `confidence` from EntryRecord and trusts it as current, the value may be hours/days old. | Med | Med | Architect should document that stored confidence reflects state at last observation. Downstream consumers needing live confidence must recompute freshness. |
| SR-03 | Confidence write contention on retrieval path. Every retrieval now triggers a read-modify-write for confidence in addition to crt-001's usage write. Two write transactions per retrieval (usage + confidence) increases serialized write pressure on redb. | Med | Low | Architect should consider combining confidence update into the existing usage write transaction rather than a separate transaction. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Search re-ranking scope ambiguity. SCOPE says re-rank after top-k retrieval, but `context_briefing` also performs internal searches. Does re-ranking apply to briefing's internal search, its final assembly, or both? | Med | Med | Spec writer should define re-ranking behavior for each of the four retrieval tools (search, lookup, get, briefing). |
| SR-05 | Confidence on deprecation creates implicit behavior. Reducing base_score for deprecated entries changes their confidence without the user requesting it. If a deprecated entry is later queried by ID (`context_get`), the lower confidence may surprise the consumer. | Low | Med | Spec writer should clarify: is reduced base_score on deprecation a permanent state or is it reversible on re-activation? |
| SR-06 | "No confidence floor" decision conflicts with product vision. The vision (crt-002 row) specifies "floor at 0.1". SCOPE decides "zero is honest." This is a deliberate deviation that should be explicitly acknowledged. | Low | Low | Architect should document this as an ADR. The deviation is well-reasoned but should be traceable. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Tight coupling with crt-001 fire-and-forget path. Confidence update piggybacks on crt-001's `record_usage_for_entries()`. If that method's signature or error handling changes, confidence updates break silently. | Med | Low | Architect should define a clear boundary between usage recording and confidence computation -- separate functions called sequentially, not interleaved logic. |
| SR-08 | `Store::update()` full index diff on confidence write. If `update_confidence()` is not implemented and the code falls back to `Store::update()`, every confidence write triggers unnecessary index diff checks across 6 index tables. | Med | Med | Architect must prioritize the targeted `update_confidence()` method. Falling back to `Store::update()` is unacceptable for per-retrieval writes. |
| SR-09 | Re-ranking changes search result ordering. Agents that depend on deterministic search ordering (e.g., always getting the same top-3 for the same query) will see different results as confidence evolves. | Low | Med | Spec writer should document that search ordering is now non-deterministic across time due to confidence evolution. |

## Assumptions

1. **crt-001 is merged and stable** (SCOPE: "crt-001 (merged) now populates the raw signals"). If crt-001 has bugs in its usage recording, confidence computation inherits those errors. The risk strategist assumes crt-001's data is correct.

2. **Single write transaction per retrieval is sufficient** (SCOPE: "fire-and-forget"). Assumes redb write serialization is not a bottleneck at current scale (single-agent stdio). This assumption breaks under multi-agent concurrency.

3. **Flat base_score is acceptable** (SCOPE: "flat 0.5 for all entries"). Assumes content quality heuristics are not needed for v1. If entries vary dramatically in quality, a flat base_score underweights the base component (20% of total confidence is always 0.10).

4. **Minimum sample size of 5 is appropriate** (SCOPE: AC-05). This is a judgment call. Too high and helpfulness never activates for infrequently-voted entries. Too low and small-sample gaming succeeds. The research spike did not provide empirical justification for 5 specifically.

## Design Recommendations

1. **(SR-01, SR-03, SR-08)** Combine confidence computation into the crt-001 usage write transaction. Read entries once, update counters AND confidence in the same transaction. Avoids a second read-modify-write cycle and eliminates SR-03 contention. Use f64 intermediates for Wilson computation to avoid SR-01 precision issues.

2. **(SR-04)** Define re-ranking scope per tool: `context_search` yes, `context_lookup` no (no similarity scores), `context_get` no (single entry), `context_briefing` internal search component only.

3. **(SR-07)** Implement confidence computation as a pure function called after usage recording completes, not interleaved with it. Clear input boundary: takes `&EntryRecord` + `now`, returns `f32`.
