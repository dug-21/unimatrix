# Risk-Based Test Strategy: crt-002 Confidence Evolution

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Wilson score numerical instability at extreme inputs | High | Low | High |
| R-02 | Confidence not updated on all mutation paths (insert, correct, deprecate) | High | Med | Critical |
| R-03 | record_usage_with_confidence transaction failure loses both usage AND confidence | High | Low | High |
| R-04 | Re-ranking inverts intended search result ordering | Med | Med | High |
| R-05 | Weight constants do not sum to 1.0 after modification | High | Med | Critical |
| R-06 | update_confidence accidentally triggers index diffs | Med | Low | Med |
| R-07 | Freshness score produces NaN or infinity from edge-case timestamps | High | Low | High |
| R-08 | Component function returns value outside [0.0, 1.0] | Med | Low | Med |
| R-09 | Confidence computation panics inside record_usage_with_confidence, aborting usage write | High | Low | High |
| R-10 | base_score not updated when Status enum gains new variants | Med | Low | Med |
| R-11 | Existing crt-001 tests break due to confidence field changing from 0.0 | Med | High | High |
| R-12 | f64-to-f32 cast produces unexpected results at boundary values | Low | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Wilson Score Numerical Instability
**Severity**: High
**Likelihood**: Low
**Impact**: Incorrect helpfulness scores leading to wrong confidence values. Could systematically over- or under-rank entries with specific vote patterns.

**Test Scenarios**:
1. Wilson lower bound at n=1 (1 positive, 0 negative): verify result in (0.0, 1.0)
2. Wilson lower bound at n=5 (boundary: exactly MINIMUM_SAMPLE_SIZE): verify deviation from 0.5 neutral
3. Wilson lower bound at n=100,000: verify numerical stability (no NaN, no negative)
4. Wilson lower bound with p_hat=0.0 (all negative): verify result is 0.0
5. Wilson lower bound with p_hat=1.0 (all positive): verify result < 1.0
6. Wilson lower bound with p_hat=0.5 at various n: verify convergence toward 0.5
7. Compare computed values against known reference (Evan Miller's Wilson score calculator)

**Coverage Requirement**: Unit tests for all 7 scenarios. f64 intermediates must be verified by asserting exact (non-epsilon) match against hand-computed reference values.

### R-02: Confidence Not Updated on All Mutation Paths
**Severity**: High
**Likelihood**: Medium
**Impact**: Entries created or mutated after crt-002 deployment have stale confidence (0.0 on insert, or pre-mutation value on correct/deprecate). The confidence field would be inconsistent -- some entries computed, others not.

**Test Scenarios**:
1. Insert entry via `context_store`, verify confidence > 0.0 immediately
2. Correct entry via `context_correct`, verify new entry has computed confidence
3. Correct entry via `context_correct`, verify old (deprecated) entry has recomputed confidence with base_score=0.2
4. Deprecate entry via `context_deprecate`, verify confidence decreased (base_score 0.5 -> 0.2)
5. Verify confidence update failure on mutation path does not fail the mutation itself

**Coverage Requirement**: Integration tests for all 5 scenarios using actual tool handlers.

### R-03: Combined Transaction Failure
**Severity**: High
**Likelihood**: Low
**Impact**: If `record_usage_with_confidence` fails after updating some entries but before committing, both usage counters AND confidence are lost for the entire batch. This is the same risk as the existing `record_usage()` -- the transaction is atomic (all or nothing).

**Test Scenarios**:
1. Successful batch: 5 entries updated, all get new confidence values
2. Entry deleted between retrieval and usage recording: deleted entry skipped, others succeed
3. Verify fire-and-forget: transaction failure logged, retrieval response already sent

**Coverage Requirement**: Integration test for scenario 1. Scenario 2 is inherited from crt-001 tests. Scenario 3 verified by existing fire-and-forget test pattern.

### R-04: Re-ranking Inverts Search Results
**Severity**: Medium
**Likelihood**: Medium
**Impact**: A low-similarity but high-confidence entry outranks a high-similarity low-confidence entry. With alpha=0.85, this requires a large confidence gap to overcome even a small similarity gap.

**Test Scenarios**:
1. Two entries: similarity 0.90 vs 0.85, confidence 0.30 vs 0.90. Verify high-similarity entry still ranks first (0.85*0.90 + 0.15*0.30 = 0.81 vs 0.85*0.85 + 0.15*0.90 = 0.86 -- here the lower-similarity entry wins due to massive confidence gap). Document that this is expected behavior.
2. Two entries: similarity 0.92 vs 0.91, confidence 0.30 vs 0.80. Verify confidence breaks the tie in favor of higher confidence (0.85*0.92 + 0.15*0.30 = 0.83 vs 0.85*0.91 + 0.15*0.80 = 0.89).
3. Two entries: similarity 0.95 vs 0.70, any confidence values. Verify high-similarity entry always wins (0.25 similarity gap cannot be overcome by max 0.15 confidence contribution).
4. Verify `context_lookup` result ordering is unchanged (no re-ranking applied)
5. Verify `context_get` returns single entry without ranking consideration

**Coverage Requirement**: Unit tests for scenarios 1-3 using rerank_score(). Integration tests for scenarios 4-5.

### R-05: Weight Sum Invariant Violation
**Severity**: High
**Likelihood**: Medium (future modification)
**Impact**: Weights not summing to 1.0 means confidence values are systematically biased. If sum > 1.0, confidence exceeds 1.0 (clamped, but components lose their intended proportional influence). If sum < 1.0, confidence is systematically low.

**Test Scenarios**:
1. Assert W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 1.0 (exact f32 comparison after ensuring constants are defined to sum exactly)
2. Construct an entry with all component scores at 1.0: verify compute_confidence returns exactly 1.0
3. Construct an entry with all component scores at 0.0: verify compute_confidence returns exactly 0.0

**Coverage Requirement**: Unit test for all 3 scenarios. The weight sum test is a compile-time-like guard that runs on every test suite execution.

### R-06: update_confidence Triggers Index Diffs
**Severity**: Medium
**Likelihood**: Low
**Impact**: If `update_confidence()` uses the general `Store::update()` instead of targeted ENTRIES-only write, each confidence update performs 6 unnecessary index table reads. Per-retrieval overhead increases significantly.

**Test Scenarios**:
1. Call `update_confidence()` on an entry. Verify only ENTRIES table is modified (not TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP).
2. Call `update_confidence()` with same confidence value. Verify no error (idempotent).
3. Verify `update_confidence()` on non-existent entry_id returns an error.

**Coverage Requirement**: Unit test for scenario 1 (verify the method reads and writes only ENTRIES). Unit tests for scenarios 2-3.

### R-07: Freshness Score Edge Cases
**Severity**: High
**Likelihood**: Low
**Impact**: NaN or infinity in freshness computation propagates through the weighted sum, corrupting the entire confidence value.

**Test Scenarios**:
1. last_accessed_at = 0, created_at = 0, now = anything: verify returns 0.0 (no reference timestamp)
2. last_accessed_at = 0, created_at = now: verify returns 1.0 (just created)
3. last_accessed_at > now (clock skew): verify returns 1.0 (clamp to maximum freshness)
4. now - reference = u64::MAX (extreme age): verify returns ~0.0 (not NaN or infinity)
5. now = reference (just accessed): verify returns ~1.0

**Coverage Requirement**: Unit tests for all 5 scenarios.

### R-08: Component Function Out-of-Range
**Severity**: Medium
**Likelihood**: Low
**Impact**: A component returning > 1.0 or < 0.0 would cause confidence to exceed [0.0, 1.0] before clamping, or unexpectedly dominate/suppress the composite.

**Test Scenarios**:
1. usage_score(u32::MAX): verify <= 1.0
2. freshness_score with all extreme timestamp combinations: verify in [0.0, 1.0]
3. helpfulness_score(u32::MAX, 0): verify <= 1.0
4. helpfulness_score(0, u32::MAX): verify >= 0.0
5. Verify compute_confidence clamps final result to [0.0, 1.0] regardless of component values

**Coverage Requirement**: Unit tests for all 5 scenarios.

### R-09: Confidence Function Panic in Transaction
**Severity**: High
**Likelihood**: Low
**Impact**: If the confidence function panics inside `record_usage_with_confidence`, the `spawn_blocking` task aborts. The redb transaction is dropped without commit, losing both usage counter updates AND confidence. The fire-and-forget handler catches the JoinError and logs it.

**Test Scenarios**:
1. Verify that confidence function never panics for any valid EntryRecord (pure function property)
2. Verify that if confidence computation is bypassed (None), usage recording still succeeds
3. Verify fire-and-forget handler logs errors from spawn_blocking task failures

**Coverage Requirement**: Unit test for scenario 1 (property test with randomized inputs). Integration test for scenario 2 (record_usage_with_confidence with None). Scenario 3 covered by existing fire-and-forget test infrastructure.

### R-10: New Status Variant Not Handled
**Severity**: Medium
**Likelihood**: Low
**Impact**: If `Status` gains a new variant (e.g., `PendingReview` from col-002), `base_score()` would need updating. A match arm returning an unexpected value could silently mis-score entries.

**Test Scenarios**:
1. Verify base_score handles all current Status variants: Active, Deprecated, Proposed
2. Use exhaustive match in base_score (no wildcard) to get a compile error when new variants are added

**Coverage Requirement**: Unit test covering all Status variants. Code review: exhaustive match enforced.

### R-11: Existing crt-001 Tests Break
**Severity**: Medium
**Likelihood**: High
**Impact**: crt-001 tests that assert `entry.confidence == 0.0` after retrieval will fail because confidence is now computed. Tests that check exact EntryRecord equality will fail due to non-zero confidence.

**Test Scenarios**:
1. Audit all existing tests that assert `confidence == 0.0` or full EntryRecord equality
2. Update assertions to account for computed confidence values
3. Verify all existing crt-001 tests pass after crt-002 changes

**Coverage Requirement**: Full test suite pass after integration. No existing test disabled -- all updated to reflect new confidence behavior.

### R-12: f64-to-f32 Cast Boundary
**Severity**: Low
**Likelihood**: Low
**Impact**: f64 values very close to 0.0 or 1.0 might cast to slightly different f32 values. Since confidence is clamped to [0.0, 1.0] after cast, this only matters for exact equality assertions.

**Test Scenarios**:
1. Verify compute_confidence returns exactly 0.0 when all components are 0.0
2. Verify compute_confidence returns exactly 1.0 when all components are 1.0
3. Verify f32 result is within f32 epsilon of the f64 composite for typical values

**Coverage Requirement**: Unit tests for scenarios 1-2. Scenario 3 is informational (f32 precision in [0,1] is sufficient).

## Integration Risks

### IR-01: record_usage_with_confidence Backward Compatibility
The existing `record_usage()` must be preserved. The new `record_usage_with_confidence()` is called from the server layer. If the server accidentally calls the old method, confidence is not updated (silent regression).

**Mitigation**: The server integration (C3) calls `record_usage_with_confidence` directly. The old `record_usage` is retained only for the `EntryStore::record_access` trait implementation. Unit test verifies that `record_usage` and `record_usage_with_confidence(None)` produce identical results.

### IR-02: Confidence Function Dependency Direction
The confidence module lives in `unimatrix-server`. It depends on `EntryRecord` from `unimatrix-store` (via `unimatrix-core`). The `record_usage_with_confidence` method in `unimatrix-store` accepts a `dyn Fn(&EntryRecord, u64) -> f32` -- the store does not depend on the server. This dependency direction is correct and must be maintained.

**Mitigation**: Code review. The function pointer in the store method signature ensures the store crate has no knowledge of the confidence formula.

### IR-03: Re-ranking and Response Formatting Order
The response is formatted AFTER re-ranking (step 10 is after step 9b). The similarity score displayed in the response is the ORIGINAL similarity, not the blended score. This is correct -- the blended score is for ordering only, not for display.

**Mitigation**: Integration test verifying that displayed similarity scores match vector search output, not blended scores.

## Edge Cases

### EC-01: Entry with All Default Fields
An entry with access_count=0, helpful_count=0, unhelpful_count=0, correction_count=0, trust_source="", last_accessed_at=0, created_at=0, status=Active.
- Expected: base_score=0.5, usage=0.0, fresh=0.0, help=0.5, corr=0.5, trust=0.3
- Confidence: 0.20*0.5 + 0.15*0.0 + 0.20*0.0 + 0.15*0.5 + 0.15*0.5 + 0.15*0.3 = 0.295

### EC-02: Entry with Maximum Values
access_count=u32::MAX, helpful_count=u32::MAX, unhelpful_count=0, correction_count=100, trust_source="human", status=Active, just accessed.
- Expected: base=0.5, usage=1.0, fresh=1.0, help=Wilson(MAX,MAX)~1.0, corr=0.3 (100>6), trust=1.0
- Confidence: 0.20*0.5 + 0.15*1.0 + 0.20*1.0 + 0.15*1.0 + 0.15*0.3 + 0.15*1.0 = 0.895

### EC-03: Deprecated Entry
Same as EC-01 but status=Deprecated. base_score drops to 0.2.
- Confidence: 0.20*0.2 + 0.15*0.0 + 0.20*0.0 + 0.15*0.5 + 0.15*0.5 + 0.15*0.3 = 0.235

### EC-04: Single Entry Search
context_search returns exactly 1 result. Re-ranking still applies (single-element sort is a no-op). Confidence is still computed.

### EC-05: Empty Search Results
context_search returns 0 results. No re-ranking, no confidence computation, no usage recording. Existing behavior preserved.

## Security Risks

### SR-SEC-01: Confidence Manipulation via Formula Knowledge
If an adversary knows the formula weights and component functions (which are deterministic), they could theoretically optimize their gaming strategy. However, the crt-001 Layer 1 defenses (session dedup, two-counter) limit the raw input manipulation, and the additive formula with 15% weight caps bound the maximum impact to 22.5% (combined worst case from the research spike).

**Assessment**: Low risk. The formula is designed to be gaming-resistant even when fully known. Security through obscurity is not relied upon.

### SR-SEC-02: Confidence Field as Untrusted Input
The `confidence` field is system-computed. No tool accepts confidence as an input parameter. However, if an attacker modifies the redb database file directly, they could set arbitrary confidence values.

**Assessment**: Out of scope for crt-002. Database file integrity is a deployment concern, not an application concern. The existing `content_hash` chain provides tamper evidence for content; confidence is re-derivable from the raw signals.

## Failure Modes

### FM-01: Confidence Computation Error on Retrieval
**Behavior**: Usage recording fails (fire-and-forget logs warning). Retrieval response is already sent. Confidence stays at its previous value. Next retrieval re-attempts computation.
**Recovery**: Automatic on next retrieval.

### FM-02: update_confidence Error on Insert
**Behavior**: Entry is inserted successfully but confidence stays at 0.0. Logged as warning.
**Recovery**: Confidence computed on first retrieval of the entry.

### FM-03: update_confidence Error on Correct/Deprecate
**Behavior**: Correction/deprecation succeeds but confidence is stale. Logged as warning.
**Recovery**: Confidence computed on next retrieval of either the old or new entry.

### FM-04: NaN from Confidence Computation
**Behavior**: Should not occur (all edge cases handled with explicit guards). If it does, the f32 clamp catches it (NaN comparisons return false, so clamp to 0.0).
**Recovery**: Automatic. The clamped value is written and corrected on next computation.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Wilson f32 precision) | R-01 | Resolved: ADR-002 mandates f64 intermediates. Wilson computed in f64, cast to f32 only at final composite. |
| SR-02 (Freshness staleness) | — | Accepted: Human confirmed freshness decay between accesses is expected behavior. Relative ranking preserved. |
| SR-03 (Write contention) | R-03 | Resolved: ADR-001 merges confidence into existing usage write transaction. Zero additional transactions on retrieval path. |
| SR-04 (Re-ranking scope) | R-04 | Resolved: ADR-005 limits re-ranking to context_search only. Human clarified deterministic paths remain deterministic. |
| SR-05 (Deprecation behavior) | R-02 | Addressed: Specification FR-07 defines deprecation confidence update. Reversibility is natural -- reactivation triggers recomputation on next retrieval. |
| SR-06 (No confidence floor) | — | Accepted: ADR-003 documents deviation from product vision. Emergent minimum from formula structure is sufficient. |
| SR-07 (Coupling with crt-001) | R-09, IR-01 | Addressed: Function pointer decouples store from server. record_usage preserved for backward compatibility. |
| SR-08 (Full index diff) | R-06 | Resolved: Architecture C2 defines targeted update_confidence() that writes only ENTRIES table. |
| SR-09 (Search ordering change) | R-04 | Accepted: ADR-005 documents that context_search ordering changes are expected. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-02, R-05) | 8 scenarios |
| High | 5 (R-01, R-03, R-04, R-07, R-09, R-11) | 22 scenarios |
| Medium | 4 (R-06, R-08, R-10, R-12) | 12 scenarios |
| Low | 1 (R-12) | 3 scenarios |
| **Total** | **12 risks** | **45 scenarios** |
