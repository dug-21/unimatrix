# crt-002 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | `compute_confidence(entry, now)` returns [0.0, 1.0] for any valid EntryRecord | test | Property test with randomized fields; edge cases (all defaults, all max, deprecated) | PENDING |
| AC-02 | Formula uses six weighted components summing to 1.0 | test | Assert W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 1.0; assert all-1.0-components -> 1.0; all-0.0 -> 0.0 | PENDING |
| AC-03 | usage_score applies log transform, clamps to [0.0, 1.0] | test | usage_score(0)==0.0, usage_score(50)==~1.0, usage_score(500)==1.0, usage_score(u32::MAX)==1.0 | PENDING |
| AC-04 | freshness_score applies exponential decay with half-life | test | freshness(now)==~1.0, freshness(1week)==~0.37, freshness(2weeks)==~0.14; fallback to created_at; clock skew returns 1.0 | PENDING |
| AC-05 | helpfulness_score returns 0.5 when votes < 5, Wilson otherwise | test | (0,0)->0.5, (3,0)->0.5, (4,0)->0.5, (5,0)->Wilson!=0.5, (8,2)->Wilson, (80,20)->Wilson | PENDING |
| AC-06 | correction_score: 1-2 corrections > uncorrected > 3-5 > 6+ | test | correction_score(0)==0.5, (1)==0.8, (2)==0.8, (3)==0.6, (5)==0.6, (6)==0.3, (100)==0.3 | PENDING |
| AC-07 | trust_score: "human" > "system" > "agent" > unknown | test | trust_score("human")==1.0, ("system")==0.7, ("agent")==0.5, ("")==0.3, ("other")==0.3 | PENDING |
| AC-08 | base_score returns 0.5 for Active/Proposed, 0.2 for Deprecated | test | base_score(Active)==0.5, base_score(Deprecated)==0.2, base_score(Proposed)==0.5 | PENDING |
| AC-09 | Confidence recomputed after every retrieval in usage write transaction | test | Insert entry, retrieve via context_search, read back entry, confidence > 0.0 | PENDING |
| AC-10 | Confidence computed on insert via context_store | test | Store entry, read back, confidence matches expected initial value (~0.525 for agent) | PENDING |
| AC-11 | Confidence recomputed on correction via context_correct | test | Correct entry, new entry has computed confidence, old entry has recomputed (lower) confidence | PENDING |
| AC-12 | Confidence recomputed on deprecation via context_deprecate | test | Deprecate entry, confidence decreased (base_score 0.5 -> 0.2 reduces confidence) | PENDING |
| AC-13 | context_search re-ranks by alpha*similarity + (1-alpha)*confidence, alpha=0.85 | test | Two entries with close similarity, different confidence; higher-confidence entry ranks first | PENDING |
| AC-14 | Re-ranking operates on existing top-k, does not change HNSW search | test | rerank_score unit test; integration: same entries returned, order may differ | PENDING |
| AC-15 | Wilson score uses z=1.96 (95% confidence) | test | wilson_lower_bound(80, 100) matches reference value from Evan Miller calculator | PENDING |
| AC-16 | Weight constants and search alpha are named constants | grep | grep for W_BASE, W_USAGE, W_FRESH, W_HELP, W_CORR, W_TRUST, SEARCH_SIMILARITY_WEIGHT in confidence.rs | PENDING |
| AC-17 | update_confidence avoids full index-diff | test | Call update_confidence, verify only ENTRIES table written (not index tables) | PENDING |
| AC-18 | Deprecated entries receive base_score 0.2 | test | base_score(Status::Deprecated) == 0.2 | PENDING |
| AC-19 | Confidence updates on retrieval are fire-and-forget | test | Confidence error does not fail retrieval response | PENDING |
| AC-20 | All component functions are pure, independently testable | test | Each function tested with no setup beyond input values, no global state | PENDING |
| AC-21 | Wilson handles edge cases: n=0 -> 0.5, all helpful -> <1.0, all unhelpful -> 0.0 | test | helpfulness_score(0,0)==0.5; wilson_lower_bound(100,100)<1.0; wilson_lower_bound(0,100)==0.0 | PENDING |
| AC-22 | Existing retrieval behavior unchanged; confidence values now non-zero | test | Same entries returned, same format; confidence field is non-zero in responses | PENDING |
