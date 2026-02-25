# Specification: crt-002 Confidence Evolution

## Objective

Compute a meaningful confidence score for every knowledge entry from multiple independent usage signals, write it to the existing `confidence: f32` field on EntryRecord, and use it to improve search result ranking. Confidence evolves continuously through inline computation on retrieval, insert, and mutation paths.

## Functional Requirements

### FR-01: Confidence Formula

FR-01a: Confidence is computed as an additive weighted composite of six independent signal components, each mapped to [0.0, 1.0], with weights summing to 1.0.

FR-01b: The six components are: base quality (status-dependent), usage frequency (log-transformed access count), freshness (exponential decay from last access), helpfulness (Wilson score lower bound), correction quality (correction chain assessment), and trust source (creator trust level).

FR-01c: The composite function accepts an `EntryRecord` reference and a current unix timestamp, returning an f32 in [0.0, 1.0].

FR-01d: All intermediate arithmetic uses f64 precision. Only the final composite value is cast to f32.

### FR-02: Component Functions

FR-02a: `base_score` returns 0.5 for Active entries and 0.2 for Deprecated entries. Proposed entries return 0.5 (treated as active for scoring purposes).

FR-02b: `usage_score` computes `ln(1 + access_count) / ln(1 + MAX_MEANINGFUL_ACCESS)` where MAX_MEANINGFUL_ACCESS = 50. Values above 1.0 are clamped to 1.0. An access_count of 0 returns 0.0.

FR-02c: `freshness_score` computes `exp(-age_hours / half_life_hours)` where half_life_hours = 168 (1 week). The reference timestamp is `last_accessed_at` if > 0, otherwise `created_at`. If both are 0, return 0.0. If `now` is less than the reference timestamp (clock skew), return 1.0.

FR-02d: `helpfulness_score` returns 0.5 (neutral prior) when total votes (helpful_count + unhelpful_count) < 5 (MINIMUM_SAMPLE_SIZE). When total votes >= 5, returns the Wilson score lower bound with z = 1.96.

FR-02e: `correction_score` returns: 0.5 for correction_count = 0, 0.8 for correction_count 1-2, 0.6 for correction_count 3-5, 0.3 for correction_count >= 6.

FR-02f: `trust_score` returns: 1.0 for "human", 0.7 for "system", 0.5 for "agent", 0.3 for any other value (including empty string).

### FR-03: Wilson Score Lower Bound

FR-03a: The Wilson score lower bound formula at 95% confidence (z = 1.96):

```
p_hat = positive / total
lower = (p_hat + z^2/(2*total) - z * sqrt(p_hat*(1-p_hat)/total + z^2/(4*total^2))) / (1 + z^2/total)
```

FR-03b: When total = 0, the function must not be called (guarded by MINIMUM_SAMPLE_SIZE check in `helpfulness_score`).

FR-03c: When all votes are positive (p_hat = 1.0), Wilson correctly returns a value < 1.0 (uncertainty from finite sample).

FR-03d: When all votes are negative (p_hat = 0.0), Wilson correctly returns 0.0 (the lower bound cannot be negative).

### FR-04: Confidence on Retrieval Path

FR-04a: After every successful retrieval that triggers usage recording (`context_search`, `context_lookup`, `context_get`, `context_briefing`), confidence is recomputed for each accessed entry using the post-update counter values.

FR-04b: The confidence computation occurs inside the same write transaction as usage counter updates (no separate transaction).

FR-04c: Confidence computation failures are logged but do not fail the retrieval or the usage recording. If the confidence function returns a value outside [0.0, 1.0] (should not happen with correct implementation), clamp before writing.

FR-04d: The confidence displayed in the current retrieval response reflects the value from the PREVIOUS computation (one-retrieval lag). The current computation updates the stored value for the NEXT retrieval.

### FR-05: Confidence on Insert

FR-05a: When a new entry is created via `context_store`, compute initial confidence from available signals and write it using `update_confidence()`.

FR-05b: Initial confidence uses: base_score(Active) = 0.5, usage_score(0) = 0.0, freshness_score(now, now, now) = 1.0, helpfulness_score(0, 0) = 0.5 (neutral), correction_score(0) = 0.5, trust_score(entry.trust_source).

FR-05c: For a human-authored entry: initial confidence = 0.20*0.5 + 0.15*0.0 + 0.20*1.0 + 0.15*0.5 + 0.15*0.5 + 0.15*1.0 = 0.10 + 0.00 + 0.20 + 0.075 + 0.075 + 0.15 = 0.60.

FR-05d: For an agent-authored entry: initial confidence = 0.20*0.5 + 0.15*0.0 + 0.20*1.0 + 0.15*0.5 + 0.15*0.5 + 0.15*0.5 = 0.10 + 0.00 + 0.20 + 0.075 + 0.075 + 0.075 = 0.525.

### FR-06: Confidence on Correction

FR-06a: When an entry is corrected via `context_correct`, recompute confidence for the new correction entry and for the deprecated original entry.

FR-06b: The new correction entry gets initial confidence computed from its fields (correction_count = 0 for a fresh correction, trust_source from the correcting agent).

FR-06c: The deprecated original entry gets recomputed confidence with base_score(Deprecated) = 0.2.

### FR-07: Confidence on Deprecation

FR-07a: When an entry is deprecated via `context_deprecate`, recompute its confidence with base_score(Deprecated) = 0.2.

FR-07b: The deprecation confidence update uses `update_confidence()` (targeted, no index diff).

### FR-08: Search Re-ranking

FR-08a: `context_search` results are re-ranked using a blended score: `alpha * similarity + (1 - alpha) * confidence` where alpha = 0.85 (SEARCH_SIMILARITY_WEIGHT).

FR-08b: Re-ranking applies after the top-k candidates are fetched from the vector index and their full EntryRecords are loaded.

FR-08c: Re-ranking sorts results in descending order of blended score.

FR-08d: Re-ranking does NOT apply to: `context_lookup` (deterministic, no similarity), `context_get` (single entry, no ranking), or the lookup/get paths within `context_briefing`.

FR-08e: The internal search component of `context_briefing` receives re-ranking naturally because it shares the `context_search` code path.

### FR-09: Targeted Confidence Update

FR-09a: `Store::update_confidence(entry_id, confidence)` reads the entry from ENTRIES, sets the confidence field, and writes it back. No index table operations.

FR-09b: If the entry does not exist, return an error (entry may have been deleted between retrieval and confidence update).

FR-09c: The method opens and commits its own write transaction.

## Non-Functional Requirements

### NFR-01: Performance

NFR-01a: Confidence computation for a single entry must complete in under 1 microsecond (pure arithmetic, no I/O). This is verified by the absence of any I/O in the computation path.

NFR-01b: The retrieval path must not have additional write transactions compared to the crt-001 baseline. Confidence is computed inside the existing usage write transaction.

NFR-01c: The mutation paths (insert, correct, deprecate) each add at most one additional write transaction (`update_confidence`). These are infrequent operations compared to retrievals.

### NFR-02: Numerical Stability

NFR-02a: All component functions and Wilson score use f64 arithmetic internally. The f32 cast occurs only at the final composite value.

NFR-02b: Wilson score must be numerically correct for total votes up to 100,000 (stress test boundary).

### NFR-03: Testability

NFR-03a: All six component functions are pure (no side effects, deterministic given inputs). Each must be independently unit-testable.

NFR-03b: The Wilson score implementation must have explicit test cases matching known-good values from established references (Evan Miller's tables).

NFR-03c: The composite function must be testable with a constructed EntryRecord (no database required).

### NFR-04: Backward Compatibility

NFR-04a: Existing retrieval tool behavior is unchanged -- same parameters, same response structure. Only the `confidence` value in responses changes from 0.00 to computed values.

NFR-04b: `context_search` result ordering may change due to re-ranking. This is an expected behavioral change, not a compatibility break.

NFR-04c: The `record_usage()` method signature is preserved. The new `record_usage_with_confidence()` is an addition, not a replacement.

## Acceptance Criteria with Verification Methods

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-01 | `compute_confidence(entry, now)` returns a value in [0.0, 1.0] for any valid EntryRecord | Unit test: property test with randomized EntryRecord fields |
| AC-02 | The confidence formula uses six weighted components with weights summing to 1.0 | Unit test: assert W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 1.0 |
| AC-03 | usage_score applies log transform and clamps to [0.0, 1.0] | Unit test: usage_score(0)=0.0, usage_score(50)=~1.0, usage_score(500)=1.0 |
| AC-04 | freshness_score applies exponential decay with configurable half-life | Unit test: freshness_score(now)=~1.0, freshness_score(1_week_ago)=~0.37 |
| AC-05 | helpfulness_score returns 0.5 when total votes < 5, Wilson lower bound otherwise | Unit test: (0,0)->0.5, (3,0)->0.5, (8,2)->Wilson, (80,20)->Wilson |
| AC-06 | correction_score returns 0.8 for 1-2 corrections, 0.6 for 3-5, 0.3 for 6+ | Unit test: exact match for each correction_count bracket |
| AC-07 | trust_score maps "human"=1.0, "system"=0.7, "agent"=0.5, other=0.3 | Unit test: exact match for each trust_source value |
| AC-08 | base_score returns 0.5 for Active/Proposed, 0.2 for Deprecated | Unit test: exact match for each status |
| AC-09 | Confidence recomputed after every retrieval in the usage write transaction | Integration test: insert entry, retrieve, check confidence > 0.0 |
| AC-10 | Confidence computed on insert via `context_store` | Integration test: store entry, read back, confidence matches expected initial value |
| AC-11 | Confidence recomputed on correction via `context_correct` | Integration test: correct entry, both old (deprecated) and new entries have recomputed confidence |
| AC-12 | Confidence recomputed on deprecation via `context_deprecate` | Integration test: deprecate entry, confidence decreases (base_score 0.5 -> 0.2) |
| AC-13 | `context_search` re-ranks results by blended score (alpha=0.85) | Integration test: two entries with similar similarity but different confidence, higher-confidence entry ranks first |
| AC-14 | Re-ranking operates on existing top-k candidates, does not change HNSW search | Unit test: rerank_score computes correct blend; integration test: same entries returned, different order |
| AC-15 | Wilson score uses z=1.96 (95% confidence level) | Unit test: wilson_lower_bound(80, 100) matches reference value |
| AC-16 | All weight constants and search blend alpha are named constants | Code review: no magic numbers in confidence computation |
| AC-17 | `update_confidence()` avoids full index-diff overhead | Unit test: update_confidence changes only ENTRIES table, not index tables |
| AC-18 | Deprecated entries receive base_score 0.2 (vs Active 0.5) | Unit test: base_score(Deprecated) == 0.2 |
| AC-19 | Confidence updates on retrieval path are fire-and-forget | Integration test: confidence computation error does not fail the retrieval |
| AC-20 | All component functions are pure and independently unit-testable | Unit tests: each function tested in isolation with no setup beyond input values |
| AC-21 | Wilson score handles edge cases: n<5 returns 0.5, all helpful returns <1.0, all unhelpful returns 0.0 | Unit test: explicit edge case assertions |
| AC-22 | Existing retrieval behavior unchanged; confidence values in responses change from 0.00 to computed | Integration test: same entries returned, same format, non-zero confidence |

## Domain Models

### Confidence Score

A scalar f32 value in [0.0, 1.0] representing the system's assessment of a knowledge entry's quality and reliability. Computed from six independent signals, each resistant to gaming in isolation, and collectively bounded so that gaming any single signal has limited impact on the composite.

**Not a probability.** Confidence is not "the probability this entry is correct." It is a composite quality score that weighs usage evidence, creator trust, correction history, and community assessment.

**Not static.** Confidence evolves every time the entry is accessed, voted on, corrected, or deprecated. The freshness component also introduces time-dependence (entries that are not accessed gradually lose freshness score).

### Component Functions

Pure functions mapping EntryRecord fields to [0.0, 1.0]. Each captures one dimension of quality:

| Component | Signal | Measures | Gaming Resistance |
|-----------|--------|----------|-------------------|
| base_score | status | Entry lifecycle state | Not gameable (status changes require capabilities) |
| usage_score | access_count | How much the entry is used | Log-transform, 15% weight cap |
| freshness_score | last_accessed_at, created_at | How recently the entry was used | Not gameable (always-updated timestamp) |
| helpfulness_score | helpful_count, unhelpful_count | Whether users find it useful | Wilson score, min sample, 15% weight cap |
| correction_score | correction_count | Correction chain stability | Not gameable (corrections require Write) |
| trust_score | trust_source | Creator credibility | Not gameable (set at creation time) |

### Wilson Score Lower Bound

A statistical method for estimating the true proportion of positive outcomes from a finite sample. Used by Reddit, Yelp, and similar systems facing adversarial voting. In Unimatrix, it estimates the true helpfulness ratio from helpful/unhelpful vote counts.

Key property: entries with few votes get conservative (lower) scores, preventing small-sample manipulation. An entry with 3 helpful votes out of 3 scores lower (~0.44) than one with 80 helpful out of 100 (~0.71).

### Blended Search Score

A weighted combination of embedding similarity and confidence used to re-rank `context_search` results:

```
blended = 0.85 * similarity + 0.15 * confidence
```

Similarity remains the dominant signal (85%). Confidence acts as a tiebreaker when similarity scores are close.

## User Workflows

### Workflow 1: Agent Retrieves Knowledge (Confidence Displayed)

1. Agent calls `context_search(query="error handling patterns")`
2. Server performs HNSW search, fetches top-k entries
3. Server re-ranks results by blended score (similarity * 0.85 + confidence * 0.15)
4. Server formats response with entry details including `confidence: 0.72`
5. Agent reads confidence to assess quality: 0.72 means well-used, well-voted entry
6. In background: server updates usage counters and recomputes confidence for next retrieval

### Workflow 2: Agent Stores New Knowledge (Confidence Seeded)

1. Agent calls `context_store(title="Convention: use Result<T, E>", ...)`
2. Server inserts entry (existing flow)
3. Server computes initial confidence: ~0.525 (agent trust) or ~0.60 (human trust)
4. Server writes confidence via `update_confidence()`
5. Response shows non-zero confidence

### Workflow 3: Agent Corrects Knowledge (Confidence Updated)

1. Agent calls `context_correct(original_id=42, title="Updated convention", ...)`
2. Server creates correction entry, deprecates original (existing flow)
3. Server computes confidence for new correction entry (~0.525)
4. Server recomputes confidence for deprecated original (base_score drops to 0.2)
5. New correction entry has higher confidence than deprecated original

### Workflow 4: Confidence Evolves Over Time

1. Entry created with confidence ~0.525 (agent-authored)
2. First retrieval: confidence updated with access_count=1, freshness=1.0 -> ~0.55
3. Multiple retrievals with helpful=true votes: helpfulness rises above 0.5 -> ~0.65
4. After 1 week without access: freshness decays to ~0.37 -> confidence drops to ~0.55
5. Another retrieval resets freshness to 1.0 -> confidence rises back to ~0.65
6. Entry corrected once: correction_score goes from 0.5 to 0.8 -> ~0.70

## Constraints

- **No schema changes.** crt-002 writes to the existing `confidence: f32` field. No new fields, no new tables, no migration.
- **Fire-and-forget pattern.** Confidence updates on the retrieval path must not block responses or fail retrievals.
- **Synchronous store, async server.** Confidence computation and writes are synchronous in the store layer. The server wraps them in `spawn_blocking`.
- **bincode positional encoding.** Confidence is at a fixed position in EntryRecord. `update_confidence` must read-modify-write the full record even though only one field changes.
- **No background tasks.** The server has no scheduler. Confidence is computed inline, triggered by user-facing operations only.
- **Weight sum invariant.** The six weights must sum to exactly 1.0. This is enforced by test.

## Dependencies

### Crate Dependencies (no new external crates)
- `unimatrix-store`: EntryRecord, Status, serialize_entry, deserialize_entry, ENTRIES table
- `unimatrix-core`: re-exports EntryRecord and Status
- `std::f64::consts` for `LN_2` and `E` (logarithm and exponential)

### Internal Dependencies
- crt-001 (merged): `record_usage()` method, `helpful_count` and `unhelpful_count` fields, `UsageDedup` session tracking
- vnc-001/002/003 (merged): tool handlers, identity resolution, capability checks
- nxs-001 (merged): `confidence: f32` field on EntryRecord, `Store::get()`, `Store::update()`

## NOT in Scope

- Batch recomputation of all entries' confidence
- `min_confidence` filter parameter on retrieval tools
- Confidence history or time series tracking
- Implicit outcome correlation (AUDIT_LOG mining)
- Agent diversity signal (unique accessor count)
- Anomaly detection integration
- UI for confidence weight tuning
- Background confidence decay process
- Schema migration (no new fields)
- New external crate dependencies
