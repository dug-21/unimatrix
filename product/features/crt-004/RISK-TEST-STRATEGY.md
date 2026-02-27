# Risk-Based Test Strategy: crt-004 Co-Access Boosting

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Confidence weight redistribution causes ranking regression | High | Med | Critical |
| R-02 | Co-access feedback loop: boosted entries get more co-access signal | High | Med | Critical |
| R-03 | Full table scan for partner lookup exceeds latency budget | Med | Med | High |
| R-04 | Quadratic pair generation creates write amplification | Med | High | High |
| R-05 | Session dedup race condition between concurrent agents | Med | Low | Med |
| R-06 | Co-access boost overrides similarity for dissimilar entries | High | Low | High |
| R-07 | Stale pair cleanup removes valuable long-term patterns | Med | Med | High |
| R-08 | Quarantined entry co-access partners receive undeserved boost | Med | Med | High |
| R-09 | CoAccessRecord bincode serialization mismatch | High | Low | High |
| R-10 | Co-access affinity computation produces NaN or value outside [0.0, 0.08] | Med | Low | Med |
| R-11 | StatusReport extension breaks existing status response parsing | Med | Med | High |
| R-12 | Co-access recording failure silently dropped (fire-and-forget) | Low | Med | Med |
| R-13 | Briefing boost changes which entries appear in orientation | Med | Med | High |

## Risk-to-Scenario Mapping

### R-01: Confidence Weight Redistribution Causes Ranking Regression
**Severity**: High
**Likelihood**: Medium
**Impact**: Entries previously ranked highly lose position. Agents receive lower-quality results. All confidence-dependent behavior (search re-ranking, briefing assembly) affected.

**Test Scenarios**:
1. Weight sum invariant: six stored weights sum to exactly 0.92
2. Effective weight sum: stored (0.92) + co-access (0.08) = 1.00
3. Boundary entry: maximum-confidence entry under old weights vs new weights -- verify no catastrophic ranking change
4. All-defaults entry: compute_confidence with all defaults under new weights -- verify reasonable value
5. Existing crt-002 confidence tests updated with new expected values and still pass
6. rerank_score with effective confidence (stored + affinity) stays in reasonable range

**Coverage Requirement**: Unit tests for all 6 scenarios. Weight constants must be compile-time verifiable.

### R-02: Co-Access Feedback Loop
**Severity**: High
**Likelihood**: Medium
**Impact**: Certain entries become permanently "sticky" in results because co-access boost increases retrieval, which increases co-access signal, which increases boost.

**Test Scenarios**:
1. Boost cap: verify co-access boost never exceeds MAX_CO_ACCESS_BOOST (0.03)
2. Log-transform diminishing returns: boost at count=20 vs count=100 -- verify negligible difference
3. High-count pair: verify boost at count=1000 equals boost at count=20 (both capped)
4. Staleness decay: pair with old last_updated excluded from boost even with high count

**Coverage Requirement**: Unit tests for boost formula at boundary values. Integration test verifying staleness exclusion.

### R-03: Full Table Scan for Partner Lookup Exceeds Latency Budget
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Search latency increases noticeably when CO_ACCESS table is large.

**Test Scenarios**:
1. Partner lookup at 0 pairs: verify returns empty immediately
2. Partner lookup at 100 pairs: verify correct results
3. Partner lookup at 10K pairs: verify completes within 20ms (NFR-02)
4. Multiple anchor lookups (3 anchors at 10K pairs): verify total < 60ms

**Coverage Requirement**: Unit tests for correctness at small scale. Performance assertions at medium scale (10K pairs) if test infrastructure supports timing.

### R-04: Quadratic Pair Generation Creates Write Amplification
**Severity**: Medium
**Likelihood**: High
**Impact**: Large result sets produce many CO_ACCESS writes, slowing the usage recording pipeline.

**Test Scenarios**:
1. Cap enforcement: 15 entry IDs generate pairs from only first 10 (45 pairs, not 105)
2. Single entry: no pairs generated (k=1, k*(k-1)/2 = 0)
3. Two entries: exactly 1 pair generated
4. Exactly 10 entries: exactly 45 pairs generated
5. Empty input: no pairs, no transaction opened

**Coverage Requirement**: Unit tests for pair generation at boundary values. Verify transaction count.

### R-05: Session Dedup Race Condition
**Severity**: Medium
**Likelihood**: Low
**Impact**: Same co-access pair recorded twice if two concurrent agents trigger recording simultaneously before the Mutex lock is acquired.

**Test Scenarios**:
1. Sequential dedup: same pair filtered on second call
2. Different pairs: both pairs pass through
3. Concurrent access: two threads recording the same pair -- verify only one passes

**Coverage Requirement**: Unit tests for sequential behavior. Concurrent test with two threads for the race condition.

### R-06: Co-Access Boost Overrides Similarity for Dissimilar Entries
**Severity**: High
**Likelihood**: Low
**Impact**: An entry with low similarity but high co-access count ranks above a genuinely relevant entry.

**Test Scenarios**:
1. Similarity dominance: entry with similarity 0.95 + no co-access ranks above entry with similarity 0.85 + max co-access boost
2. Tiebreaker behavior: two entries with similar similarity -- co-access boost correctly breaks tie
3. Anchor selection: only top-3 results are anchors -- verify lower-ranked results don't anchor

**Coverage Requirement**: Unit tests with computed expected scores.

### R-07: Stale Pair Cleanup Removes Valuable Long-Term Patterns
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Long-established co-access relationships lost after 30 days without reinforcement.

**Test Scenarios**:
1. Fresh pair: not cleaned up
2. Stale pair (31 days old): cleaned up
3. Boundary: pair exactly at 30-day mark -- verify behavior
4. Cleanup count: verify correct count returned
5. Active pairs preserved: cleanup does not affect fresh pairs

**Coverage Requirement**: Integration tests with controlled timestamps.

### R-08: Quarantined Entry Partners Receive Undeserved Boost
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Entries frequently co-retrieved with a now-quarantined entry still receive boost from that relationship.

**Test Scenarios**:
1. Quarantined partner excluded: entry A co-accessed with quarantined entry B -- B excluded from A's partner list
2. Deprecated partner excluded: same for deprecated entries
3. Active partner included: active partners correctly returned

**Coverage Requirement**: Integration test with quarantined/deprecated entries in CO_ACCESS.

### R-09: CoAccessRecord Bincode Serialization Mismatch
**Severity**: High
**Likelihood**: Low
**Impact**: Corrupted co-access data. All co-access lookups return errors or wrong values.

**Test Scenarios**:
1. Roundtrip: serialize then deserialize produces identical record
2. Count boundaries: count=0, count=1, count=u32::MAX
3. Timestamp boundaries: last_updated=0, last_updated=u64::MAX

**Coverage Requirement**: Unit tests for serialization roundtrip at boundary values.

### R-10: Co-Access Affinity Computation Edge Cases
**Severity**: Medium
**Likelihood**: Low
**Impact**: NaN or out-of-range affinity corrupts effective confidence.

**Test Scenarios**:
1. Zero partners: affinity = 0.0
2. Max partners (10+): affinity capped at W_COAC (0.08)
3. Zero average confidence: affinity = 0.0
4. Max average confidence (1.0): affinity = W_COAC * saturated_partner_score
5. Combined: effective confidence clamped to [0.0, 1.0]

**Coverage Requirement**: Unit tests for all 5 boundary scenarios.

### R-11: StatusReport Extension Breaks Existing Parsing
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Tools or scripts parsing context_status output break due to new fields.

**Test Scenarios**:
1. Summary format includes co-access section
2. Markdown format includes Co-Access Patterns heading
3. JSON format includes co_access object
4. Empty co-access data: fields present with zero values (not omitted)

**Coverage Requirement**: Integration tests for all three response formats.

### R-12: Co-Access Recording Failure Silently Dropped
**Severity**: Low
**Likelihood**: Medium
**Impact**: Co-access data not accumulated. Boost never activates. Feature appears broken with no error signal.

**Test Scenarios**:
1. Recording failure logged: verify tracing::warn emitted on error
2. Tool response unaffected: recording failure does not fail the tool call
3. Partial failure: some pairs recorded, some fail -- verify partial success

**Coverage Requirement**: Unit test for error handling path.

### R-13: Briefing Boost Changes Orientation Content
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Agents receive different briefing content due to co-access boost. Could be beneficial (related knowledge surfaces) or harmful (irrelevant entries promoted by association).

**Test Scenarios**:
1. Small boost effect: verify MAX_BRIEFING_CO_ACCESS_BOOST (0.01) does not dramatically reorder briefing
2. No co-access data: briefing identical to pre-crt-004 behavior
3. High co-access data: verify boost applied but does not override relevance

**Coverage Requirement**: Integration test comparing briefing with and without co-access data.

## Integration Risks

### CO_ACCESS Table Initialization (C1 -> Store::open)
Risk: Table creation fails or is missed, causing runtime errors on first co-access write.
Test: Verify Store::open creates CO_ACCESS table; verify co-access write succeeds on fresh database.

### Usage Pipeline Extension (C3 -> server.rs)
Risk: Co-access recording step errors, causing the entire `record_usage_for_entries` to fail (losing regular usage recording).
Test: Verify co-access step is isolated -- its failure does not affect steps 1-4.

### Search Pipeline Extension (C6 -> tools.rs)
Risk: Co-access boost step errors, causing context_search to fail entirely.
Test: Verify graceful degradation -- if CO_ACCESS lookup fails, search returns results without boost.

### Confidence Formula Change (C5 -> confidence.rs)
Risk: Weight change causes existing crt-002 tests to fail.
Test: Update expected values in all crt-002 confidence tests. Run full test suite.

## Edge Cases

- **Single-entry result set**: No co-access pairs generated (k=1). No boost applied.
- **All results are co-access partners of anchor**: All get boost. Relative ordering determined by individual boost values.
- **Entry is its own co-access partner**: Impossible -- `co_access_key(x, x)` produces `(x, x)` but pair generation excludes self-pairs.
- **Deleted entry in CO_ACCESS**: Partner lookup returns deleted entry ID. Entry fetch fails. Skip gracefully.
- **CO_ACCESS table empty**: Boost computation returns empty map. No boost applied. Search/briefing behave identically to pre-crt-004.
- **All co-access pairs are stale**: Same as empty table -- no boost applied.
- **Concurrent status calls with cleanup**: Both attempt to delete stale pairs. redb transactions provide isolation -- one wins, other is no-op.

## Security Risks

### Co-Access Count Inflation
**Untrusted input**: An agent could repeatedly call search tools with the same query to inflate co-access counts.
**Mitigation**: Session dedup prevents per-session inflation. The log-transform on count (ADR-002) means even successful inflation beyond dedup has diminishing returns. The max boost is 0.03.
**Blast radius**: Low. Inflated co-access counts cause slightly altered search rankings. Cannot inject new knowledge or modify existing entries.

### Co-Access Data Exfiltration via Status
**Untrusted input**: Any agent with Read capability can call `context_status` and see co-access pair data (entry IDs, counts).
**Mitigation**: Co-access data is metadata, not content. Entry IDs are not sensitive. This is consistent with existing status reporting (which already exposes counts, categories, and entry metadata).
**Blast radius**: Minimal. Co-access pairs reveal usage patterns but not entry content.

### Denial of Service via Large Result Sets
**Untrusted input**: A crafted query could produce large result sets, generating many co-access pairs.
**Mitigation**: MAX_CO_ACCESS_ENTRIES cap (10) bounds pair generation to 45 pairs per call regardless of result set size.
**Blast radius**: Low. Write volume is bounded.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| CO_ACCESS write fails | tracing::warn logged, tool response unaffected, co-access data lost for this call |
| CO_ACCESS read fails during search | No boost applied, search returns results ranked by similarity+confidence only |
| CO_ACCESS read fails during briefing | No boost applied, briefing returns pre-crt-004 behavior |
| CO_ACCESS read fails during status | Co-access fields set to 0, stale_pairs_cleaned = 0, warning logged |
| Staleness cleanup fails | Warning logged, stale pairs persist until next status call |
| Partner entry no longer exists | Skipped during boost computation, no error |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Quadratic pair generation) | R-04 | MAX_CO_ACCESS_ENTRIES cap at 10 (45 pairs max). ADR-002 documents the bound. |
| SR-02 (Confidence weight regression) | R-01 | Proportional weight reduction preserves relative factor ordering. ADR-003 documents exact weights. Existing tests updated. |
| SR-03 (Feedback loop) | R-02 | Log-transform + hard cap (ADR-002). Max boost 0.03. Staleness decay removes stale signal. |
| SR-04 (Function pointer signature) | -- | Resolved: split integration (ADR-003). compute_confidence unchanged. Co-access applied separately at query time. |
| SR-05 (Briefing mechanism ambiguity) | R-13 | Specified: briefing boost uses same algorithm as search with MAX_BRIEFING_CO_ACCESS_BOOST = 0.01. |
| SR-06 (Staleness cleanup latency) | R-07 | Lazy staleness (filter on read) for boost. Eager cleanup only during context_status. |
| SR-07 (Table initialization) | -- | Low risk. Follows existing pattern (13th table). |
| SR-08 (Quarantined partner boost) | R-08 | Partners filtered by status during boost computation. Quarantined and deprecated entries excluded. |
| SR-09 (Rerank integration) | -- | Co-access is separate post-rerank step. Existing rerank_score unchanged. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 10 scenarios |
| High | 6 (R-03, R-04, R-06, R-07, R-08, R-09) | 18 scenarios |
| Medium | 5 (R-05, R-10, R-11, R-12, R-13) | 14 scenarios |
| Low | 0 | 0 scenarios |
| **Total** | **13** | **42 scenarios** |
