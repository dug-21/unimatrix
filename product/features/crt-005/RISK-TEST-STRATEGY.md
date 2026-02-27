# Risk-Based Test Strategy: crt-005 Coherence Gate

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Schema migration v2->v3 fails or produces corrupt entries when deserializing V2EntryRecord intermediate struct with f32 confidence, leaving database in unusable state | High | Low | High |
| R-02 | f32 scoring constants missed during f64 sweep -- a literal `0.85_f32` or implicit f32 type inference causes silent truncation in the scoring pipeline after upgrade | High | High | Critical |
| R-03 | HNSW graph compaction corrupts the active index -- if build-new-then-swap ordering is violated, search is broken until server restart | High | Low | High |
| R-04 | f64 precision loss at VectorIndex::map_neighbours_to_results -- the f32-to-f64 cast of `1.0 - n.distance` introduces representation artifacts if done in wrong order of operations | Med | Med | Med |
| R-05 | Lambda weight re-normalization produces incorrect composite when embedding dimension is excluded -- division by (1.0 - w_embed) yields wrong weights if w_embed is zero or weights do not sum to 1.0 | High | Med | High |
| R-06 | VECTOR_MAP-first compaction ordering -- crash between VECTOR_MAP write and in-memory swap leaves VECTOR_MAP with new data_ids that do not match the old HNSW graph; on restart, search returns empty results | High | Low | High |
| R-07 | Maintenance opt-out parameter does not prevent all writes -- confidence refresh or co-access cleanup still fires when maintenance=false | Med | Med | Med |
| R-08 | Confidence refresh during context_status exceeds batch cap or runs unbounded, blocking the diagnostic call for seconds | Med | Med | Med |
| R-09 | Embed service unavailable during compaction causes panic instead of graceful skip | Med | Med | Med |
| R-10 | Coherence dimension score functions return values outside [0.0, 1.0] at boundary inputs (division by zero, negative ratios) | High | Low | High |
| R-11 | Existing 811+ tests fail after f32->f64 type promotion due to hardcoded f32 literals, f32 comparison assertions, or f32 function signatures in test code | Med | High | High |
| R-12 | StatusReport coherence section missing or malformed in one or more response formats (summary, markdown, json) | Med | Med | Med |
| R-13 | V2EntryRecord intermediate struct field count or order mismatch with actual v2 schema causes deserialization failure for all entries during migration | High | Med | Critical |
| R-14 | compute_confidence weight constants (W_BASE through W_COAC) no longer sum to expected total after f64 promotion due to floating-point representation differences between f32 and f64 | High | Low | High |
| R-15 | Compaction re-embedding produces different embeddings than originals (model non-determinism or content changes), causing search result ordering to shift post-compaction | Med | Med | Med |
| R-16 | Stale confidence detection uses `max(updated_at, last_accessed_at)` but a retrieval path skips the `last_accessed_at` update, making entries appear staler than they are | Med | Low | Med |
| R-17 | update_confidence signature change from f32 to f64 breaks VectorStore trait object safety or test mock implementations | High | Med | High |
| R-18 | Graph compaction with zero active entries (empty knowledge base) causes division by zero or panics in stale ratio computation | Med | Low | Med |
| R-19 | Concurrent context_status calls both trigger compaction simultaneously, causing double-swap or VECTOR_MAP corruption | Med | Low | Low |
| R-20 | Maintenance recommendations generated when lambda >= threshold (false positive) or not generated when lambda < threshold (false negative) | Low | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Schema Migration v2->v3 Failure
**Severity**: High
**Likelihood**: Low
**Impact**: Database cannot be opened. All Unimatrix operations fail until the database is restored from backup or recreated.

**Test Scenarios**:
1. Migrate a v2 database with known f32 confidence values -- verify all entries readable with f64 confidence matching `original_f32 as f64` exactly
2. Migrate a v2 database with confidence=0.0 (default) -- verify 0.0_f64 after migration
3. Migrate a v2 database with confidence near f32 boundary values (f32::MIN_POSITIVE, f32::MAX, f32::EPSILON) -- verify lossless promotion
4. Verify schema_version counter reads 3 after successful migration
5. Verify migration is idempotent: running on an already-v3 database is a no-op
6. Verify migration chain: v0->v1->v2->v3 on an original v0 database produces correct results
7. Simulate migration of a database with 0 entries -- verify no error, schema version bumped to 3

**Coverage Requirement**: Integration tests in migration.rs for scenarios 1-7. The v2->v3 migration must be tested with real redb write transactions, not mocks.

### R-02: Residual f32 Constants After f64 Sweep
**Severity**: High
**Likelihood**: High
**Impact**: Silent precision truncation in the scoring pipeline. JSON output shows f32 artifacts (e.g., 0.8500000238418579). Confidence values lose precision at the truncation point.

**Test Scenarios**:
1. Assert all weight constants (W_BASE, W_USAGE, W_FRESH, W_HELP, W_CORR, W_TRUST, W_COAC) are f64 type -- compile-time or const assertion
2. Assert SEARCH_SIMILARITY_WEIGHT is f64
3. Assert MAX_CO_ACCESS_BOOST and MAX_BRIEFING_CO_ACCESS_BOOST are f64
4. Assert compute_confidence return type is f64 -- call with known inputs, verify precision beyond 7 decimal digits
5. Assert rerank_score accepts and returns f64 -- pass f64 values with >7 digits of precision, verify output preserves precision
6. Assert update_confidence accepts f64 -- store a value like 0.123456789012345_f64, read back, verify exact match
7. Assert SearchResult.similarity is f64 -- verify a search result carries f64 precision
8. Grep all `.rs` files in scoring paths for remaining `f32` type annotations (code review scenario)

**Coverage Requirement**: Unit tests for scenarios 1-7. Scenario 8 is a code review gate enforced by the tester during implementation review.

### R-03: HNSW Graph Compaction Corruption
**Severity**: High
**Likelihood**: Low
**Impact**: Search returns wrong results, no results, or panics. Recovery requires server restart and potential re-embedding of all entries.

**Test Scenarios**:
1. Compact an index with 10 active entries and 3 stale entries -- verify post-compaction stale_count() == 0
2. Compact an index -- verify search results for a known query return the same entries (same entry_ids) before and after compaction
3. Compact an index -- verify VECTOR_MAP contains updated data_ids for all active entries
4. Compact an index -- verify point_count() == number of active entries after compaction
5. Simulate compact failure during HNSW build (e.g., empty embeddings list) -- verify old index is untouched
6. Compact then insert a new entry -- verify the new entry is searchable alongside compacted entries

**Coverage Requirement**: Integration tests for scenarios 1-6 using VectorIndex with real HNSW graph.

### R-04: f64 Precision at the f32/f64 Cast Boundary
**Severity**: Med
**Likelihood**: Med
**Impact**: SearchResult.similarity values have unexpected representation (e.g., 0.8999999761581421 instead of 0.9) due to f32->f64 promotion of HNSW distance values.

**Test Scenarios**:
1. HNSW returns distance=0.1 (f32) -- verify SearchResult.similarity == (1.0_f64 - 0.1_f32 as f64), not (1.0_f32 - 0.1_f32) as f64
2. HNSW returns distance=0.0 -- verify similarity == 1.0_f64 exactly
3. HNSW returns distance=1.0 -- verify similarity == 0.0_f64 exactly
4. Verify the cast happens as `(1.0_f64 - distance as f64)` not `((1.0 - distance) as f64)` -- the order matters for precision
5. Verify rerank_score receives f64 similarity and produces f64 output without intermediate f32 narrowing

**Coverage Requirement**: Unit tests for scenarios 1-4 in index.rs. Unit test for scenario 5 in confidence.rs.

### R-05: Lambda Weight Re-normalization Edge Cases
**Severity**: High
**Likelihood**: Med
**Impact**: Lambda score is incorrect, leading to false maintenance recommendations or missed degradation. col-002 retrospective pipeline draws wrong conclusions from misleading lambda values.

**Test Scenarios**:
1. All four dimensions available -- verify lambda == weighted sum with DEFAULT_WEIGHTS
2. Embedding dimension excluded (None) -- verify lambda uses re-normalized 3-dimension weights summing to 1.0
3. All dimensions score 1.0 -- verify lambda == 1.0 regardless of which dimensions are available
4. All dimensions score 0.0 -- verify lambda == 0.0 regardless of which dimensions are available
5. Only one dimension deviates from 1.0 -- verify lambda reflects the weighted contribution correctly
6. Verify re-normalized weights: freshness 0.35/0.85, graph 0.30/0.85, contradiction 0.20/0.85 when embedding excluded
7. Custom CoherenceWeights with embedding_consistency=0.0 -- verify re-normalization handles zero-weight dimension correctly (no division by zero)
8. Verify weight struct constants sum to 1.0 exactly (compile-time guard)

**Coverage Requirement**: Unit tests for all 8 scenarios in coherence.rs. The weight sum invariant test is a regression guard.

### R-06: VECTOR_MAP-First Compaction Ordering
**Severity**: High
**Likelihood**: Low
**Impact**: After a crash between VECTOR_MAP write and in-memory swap, the on-disk state has new data_ids but the old HNSW graph has old data_ids. On restart, persistence reload fails to find HNSW points for new data_ids. Search returns empty until re-embedding.

**Test Scenarios**:
1. Verify compact writes VECTOR_MAP before swapping in-memory state -- inspect call ordering in implementation
2. Verify that if VECTOR_MAP write fails, in-memory HNSW and IdMap remain unchanged
3. Verify that after successful VECTOR_MAP write + in-memory swap, both VECTOR_MAP and in-memory state are consistent
4. Verify VECTOR_MAP update runs in a single write transaction (all data_ids or none)

**Coverage Requirement**: Integration tests for scenarios 2-4. Scenario 1 is a code review gate (verify ordering in compact implementation).

### R-07: Maintenance Opt-Out Completeness
**Severity**: Med
**Likelihood**: Med
**Impact**: Callers using maintenance=false for read-only diagnostics inadvertently trigger writes to ENTRIES or VECTOR_MAP, violating the behavioral contract.

**Test Scenarios**:
1. Call context_status(maintenance: false) -- verify confidence_refreshed_count == 0
2. Call context_status(maintenance: false) -- verify graph_compacted == false even when stale ratio exceeds threshold
3. Call context_status(maintenance: false) -- verify co-access stale pair cleanup is skipped
4. Call context_status(maintenance: false) -- verify coherence dimension scores are still computed and returned
5. Call context_status(maintenance: true) with stale entries -- verify confidence_refreshed_count > 0
6. Call context_status without maintenance parameter (default) -- verify maintenance behavior is enabled
7. Verify maintenance=false does not prevent contradiction scanning or embedding consistency checks (those are reads, not writes)

**Coverage Requirement**: Integration tests for all 7 scenarios.

### R-08: Confidence Refresh Batch Overflow
**Severity**: Med
**Likelihood**: Med
**Impact**: context_status blocks for an unacceptable duration, causing agent timeouts.

**Test Scenarios**:
1. Create 200 stale entries, call context_status -- verify confidence_refreshed_count <= MAX_CONFIDENCE_REFRESH_BATCH (100)
2. Create 50 stale entries, call context_status -- verify all 50 are refreshed (under the cap)
3. Call context_status twice with 200 stale entries -- verify second call refreshes the remaining entries
4. Verify stale entries are sorted by staleness (oldest first) for refresh priority

**Coverage Requirement**: Integration tests for scenarios 1-3. Unit test for scenario 4 (sorting order in confidence refresh selection).

### R-09: Embed Service Unavailable During Compaction
**Severity**: Med
**Likelihood**: Med
**Impact**: Compaction panics or returns an unhandled error, causing context_status to fail entirely.

**Test Scenarios**:
1. Call context_status when stale ratio > threshold but embed service is not initialized -- verify compaction skipped, graph_compacted == false
2. Verify maintenance_recommendations includes "HNSW compaction skipped: embed service unavailable"
3. Verify context_status still completes successfully and returns all other coherence scores
4. Verify lambda is computed with the current (pre-compaction) graph quality score

**Coverage Requirement**: Integration tests for all 4 scenarios.

### R-10: Dimension Score Boundary Values
**Severity**: High
**Likelihood**: Low
**Impact**: NaN, infinity, or negative values in dimension scores propagate into lambda, producing corrupt coherence metrics.

**Test Scenarios**:
1. confidence_freshness_score with 0 total entries -- verify returns 1.0
2. confidence_freshness_score with all entries stale -- verify returns 0.0
3. confidence_freshness_score with no entries stale -- verify returns 1.0
4. graph_quality_score with point_count=0 -- verify returns 1.0 (no division by zero)
5. graph_quality_score with stale_count > point_count (should not happen, but defensive) -- verify clamped to 0.0
6. graph_quality_score with stale_count=0 -- verify returns 1.0
7. embedding_consistency_score with total_checked=0 -- verify returns 1.0
8. embedding_consistency_score with all entries inconsistent -- verify returns 0.0
9. contradiction_density_score with total_active=0 -- verify returns 1.0
10. contradiction_density_score with total_quarantined > total_active (counter inconsistency) -- verify clamped to 0.0
11. All dimension scores with typical mid-range values -- verify within [0.0, 1.0]

**Coverage Requirement**: Unit tests for all 11 scenarios in coherence.rs. These are pure function tests with no I/O.

### R-11: Existing Test Suite Regression from f64 Promotion
**Severity**: Med
**Likelihood**: High
**Impact**: Build breaks or test failures block CI. If not addressed systematically, individual test fixes may introduce new bugs.

**Test Scenarios**:
1. All 166 store tests pass after EntryRecord.confidence promotion to f64
2. All 95 vector tests pass after SearchResult.similarity promotion to f64
3. All 453 server tests pass after confidence.rs, coaccess.rs, and tools.rs f64 changes
4. All 76 embed tests pass unchanged (no f64 changes in embed crate)
5. All 21 core tests pass after trait signature propagation
6. Tests with hardcoded confidence values (e.g., `confidence: 0.95`) compile and pass with f64 inference
7. Tests with f32 comparison assertions (e.g., `assert!((result - 0.85).abs() < f32::EPSILON)`) updated to f64 epsilon
8. Tests that construct EntryRecord with `confidence: 0.0` work with f64 type inference

**Coverage Requirement**: Full test suite pass (`cargo test --workspace`). No tests disabled. The f64 upgrade is verified by the existing test suite acting as a regression safety net.

### R-12: StatusReport Coherence Section Formatting
**Severity**: Med
**Likelihood**: Med
**Impact**: Agents parsing context_status responses cannot extract coherence data. col-002 retrospective pipeline cannot read lambda values.

**Test Scenarios**:
1. JSON format includes all 10 new coherence fields with correct f64 serialization
2. Markdown format includes a "Coherence" section with lambda, dimension scores, and recommendations
3. Summary format includes a coherence line with lambda and dimension breakdown
4. Maintenance recommendations appear in all three formats when lambda < threshold
5. Maintenance recommendations are absent in all three formats when lambda >= threshold
6. f64 values in JSON are serialized without f32 artifacts (no 0.8500000238418579)
7. graph_compacted boolean renders correctly in all formats

**Coverage Requirement**: Unit tests for all 7 scenarios in response.rs.

### R-13: V2EntryRecord Struct Mismatch
**Severity**: High
**Likelihood**: Med
**Impact**: Every entry in the database fails to deserialize during migration, making the database permanently inaccessible (migration rolls back, but retries also fail).

**Test Scenarios**:
1. V2EntryRecord has exactly 26 fields matching the current EntryRecord (minus the confidence type change)
2. V2EntryRecord field order matches bincode serialization order of v2 entries
3. V2EntryRecord with confidence: f32 successfully deserializes bytes written by current (v2) schema
4. Round-trip: serialize with current EntryRecord (v2), deserialize with V2EntryRecord -- all fields match
5. V2EntryRecord handles #[serde(default)] fields correctly (helpful_count, unhelpful_count, confidence)

**Coverage Requirement**: Unit tests for scenarios 3-5 using actual bincode serialization/deserialization. Scenario 1-2 verified by code review against current EntryRecord definition.

### R-14: Weight Sum Invariant After f64 Promotion
**Severity**: High
**Likelihood**: Low
**Impact**: Confidence values systematically biased. All search re-ranking affected. Lambda computation depends on correct confidence values.

**Test Scenarios**:
1. Assert W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 0.92 exactly in f64
2. Assert W_COAC == 0.08 exactly in f64
3. Assert total (0.92 + 0.08) == 1.0 exactly in f64
4. Assert DEFAULT_WEIGHTS sum (0.35 + 0.30 + 0.15 + 0.20) == 1.0 exactly in f64
5. Verify compute_confidence returns exactly 1.0 when all six stored components score 1.0 and co-access affinity is maximum
6. Verify compute_confidence returns 0.0 when all components score 0.0

**Coverage Requirement**: Unit tests for all 6 scenarios. These guard against rounding drift during the f32-to-f64 constant migration.

### R-15: Compaction Search Result Drift
**Severity**: Med
**Likelihood**: Med
**Impact**: Search results change ordering after compaction, confusing agents that cache or rely on stable ordering.

**Test Scenarios**:
1. Run a search query before compaction, record entry_ids in order. Run same query after compaction. Verify same entry_ids returned (order may differ due to HNSW non-determinism).
2. Insert entries, create stale nodes via re-embed, compact. Verify search results no longer include stale-node routing artifacts.
3. Verify similarity scores after compaction are within epsilon of pre-compaction scores (not exact due to HNSW construction non-determinism).

**Coverage Requirement**: Integration tests for all 3 scenarios.

### R-16: Staleness Detection False Positives
**Severity**: Med
**Likelihood**: Low
**Impact**: Entries that were recently accessed are incorrectly identified as stale, triggering unnecessary confidence refresh writes.

**Test Scenarios**:
1. Entry with last_accessed_at = now - 1 hour -- verify NOT identified as stale (threshold is 24h)
2. Entry with last_accessed_at = 0 but updated_at = now - 1 hour -- verify NOT stale (uses max of both)
3. Entry with both timestamps older than threshold -- verify IS stale
4. Entry with last_accessed_at = 0 and updated_at = 0 -- verify IS stale
5. Verify staleness threshold is configurable via named constant

**Coverage Requirement**: Unit tests for all 5 scenarios in coherence.rs.

### R-17: Trait Object Safety After Signature Change
**Severity**: High
**Likelihood**: Med
**Impact**: Compile failure in server crate where VectorStore and Store are used as trait objects. If trait object safety is broken, the entire server fails to compile.

**Test Scenarios**:
1. VectorStore trait with new compact method compiles as `dyn VectorStore`
2. Store trait with update_confidence(f64) compiles as `dyn`-compatible (if used as trait object)
3. Mock implementations of VectorStore implement compact correctly
4. All trait methods remain object-safe (no generics, no Self by value)

**Coverage Requirement**: Compile-time verification (if it compiles, it passes). Explicit test in core crate that constructs `Box<dyn VectorStore>`.

### R-18: Empty Knowledge Base Edge Cases
**Severity**: Med
**Likelihood**: Low
**Impact**: Division by zero or panic when computing coherence scores on an empty database.

**Test Scenarios**:
1. context_status on empty database -- verify all dimension scores are 1.0 (healthy by default)
2. context_status on empty database -- verify lambda == 1.0
3. context_status on empty database -- verify maintenance_recommendations is empty
4. Stale ratio computation with point_count=0 -- verify no division by zero, returns 0.0 ratio

**Coverage Requirement**: Integration test for scenario 1-3. Unit test for scenario 4.

### R-19: Concurrent Compaction
**Severity**: Med
**Likelihood**: Low
**Impact**: Two simultaneous context_status calls both detect stale ratio > threshold and both trigger compaction. The second compaction operates on the already-compacted index, which is wasteful but should not corrupt data.

**Test Scenarios**:
1. Verify compaction is safe to run on an index with stale_count=0 (no-op or harmless rebuild)
2. At current scale (single-agent stdio), concurrent calls are not possible. Document as known limitation.

**Coverage Requirement**: Unit test for scenario 1. Scenario 2 accepted at current architecture.

### R-20: Recommendation Generation Correctness
**Severity**: Low
**Likelihood**: Low
**Impact**: Agents receive misleading maintenance guidance.

**Test Scenarios**:
1. Lambda >= 0.8 -- verify empty recommendations
2. Lambda < 0.8 with stale confidence -- verify recommendation mentions stale count and oldest age
3. Lambda < 0.8 with high stale ratio -- verify recommendation mentions stale node percentage
4. Lambda < 0.8 with embedding inconsistencies -- verify recommendation mentions inconsistency count
5. Lambda < 0.8 with high quarantine ratio -- verify recommendation mentions quarantine percentage
6. All dimensions degraded -- verify multiple recommendations generated (one per degraded dimension)

**Coverage Requirement**: Unit tests for all 6 scenarios in coherence.rs.

## Integration Risks

### IR-01: Cross-Crate f64 Type Propagation
The f64 upgrade touches all crates except unimatrix-embed. A type mismatch at any crate boundary (store<->core, core<->vector, core<->server) will cause a compile error. However, implicit coercions between f32 and f64 do not exist in Rust -- the compiler catches these. The risk is limited to explicit `as f32` casts that might be left behind.

**Mitigation**: After implementation, grep all scoring-path `.rs` files for `as f32` to catch leftover truncation casts. The only legitimate `as f32` should be in contradiction.rs (HNSW domain) and embed pipeline.

### IR-02: context_status Handler Complexity Growth
The context_status handler currently performs: counter reads, distribution computation, contradiction scanning, embedding consistency checks, co-access stats, and co-access cleanup. crt-005 adds: dimension score computation, confidence refresh, graph compaction, lambda computation, and recommendation generation. The handler becomes the most complex single function in the codebase.

**Mitigation**: The architecture decomposes new functionality into coherence.rs (pure functions) and keeps the handler as an orchestrator. Integration tests must verify the full pipeline end-to-end, not just individual components.

### IR-03: StatusReport Construction Site Count
StatusReport gains 10 new fields. Every place that constructs StatusReport (production code + test helpers + test assertions) must be updated. Missing a field causes a compile error (Rust struct initialization), so this is caught at build time.

**Mitigation**: Search for `StatusReport {` and `StatusReport::` patterns to find all construction sites. Use `..Default::default()` or similar patterns in tests to reduce brittleness.

### IR-04: Confidence Refresh Interaction with Fire-and-Forget
Confidence is already updated via fire-and-forget after usage recording (crt-002). The crt-005 batch refresh during context_status creates a second write path for confidence. If both paths run concurrently (an agent retrieves an entry while context_status is refreshing it), the last writer wins. This is acceptable (both compute correct values) but the entry's confidence may briefly toggle between two slightly different values.

**Mitigation**: Accept at current scale (single-agent stdio). Document as known behavior.

### IR-05: HNSW Compaction Memory Pressure
During compaction, two HNSW graphs exist simultaneously in memory. At current scale (<1000 entries, 384-dim f32), this is ~3 MB extra. If entry count grows to 10K, this becomes ~30 MB. Not a risk at current scale but should be monitored.

**Mitigation**: The stale ratio threshold (10%) limits compaction frequency. The batch cap on confidence refresh limits concurrent memory from that path.

## Edge Cases

### EC-01: Database with All Entries Having Confidence 0.0
A fresh database or one where confidence was never computed (pre-crt-002 entries). After v2->v3 migration, all entries have confidence 0.0_f64. The confidence freshness dimension will score all entries as stale (if their timestamps are old), triggering a full batch refresh on the first context_status call.

### EC-02: HNSW Graph with 100% Stale Nodes
If every node in the HNSW graph is stale (all entries were re-embedded), stale_ratio = 1.0. Compaction should rebuild from active entries. If there are zero active entries in the store, the compaction produces an empty index.

### EC-03: Single Active Entry
One active entry, zero stale HNSW nodes, zero quarantined entries. All dimensions should score 1.0. Lambda should be 1.0. No recommendations. This is the minimal healthy state.

### EC-04: Lambda Exactly at Threshold (0.8)
Lambda == DEFAULT_LAMBDA_THRESHOLD exactly. Per AC-08, recommendations trigger when lambda < threshold. At exactly 0.8, no recommendations should be generated. Floating-point comparison must use strict less-than, not less-than-or-equal.

### EC-05: Confidence Value at f64 Extremes
An entry where compute_confidence produces a value very close to 0.0 or 1.0 (e.g., 0.9999999999999998). After update_confidence writes this to the database, the round-trip through bincode serialization must preserve the exact f64 bits. Verify no clamping or rounding occurs in the storage layer.

### EC-06: Staleness Threshold of Zero
If DEFAULT_STALENESS_THRESHOLD_SECS were set to 0, every entry would be considered stale on every context_status call. This should not happen (it is a constant, not user-configurable), but the code should not panic or infinite-loop in this case.

### EC-07: Graph Compaction with Duplicate Entry IDs in Embeddings
If the caller passes duplicate (entry_id, embedding) pairs to compact, the new index should handle this gracefully (either deduplicate or reject).

### EC-08: Embedding Consistency Score When One Entry Checked
total_checked = 1, inconsistent_count = 1: score = 0.0. total_checked = 1, inconsistent_count = 0: score = 1.0. Verify no off-by-one in the ratio computation.

## Security Risks

### SEC-01: Maintenance Parameter Abuse
**Untrusted input**: The `maintenance` parameter on context_status comes from MCP callers.
**Damage potential**: A caller could set `maintenance: true` repeatedly to trigger mass confidence refresh writes, causing write amplification. At current scale and with the batch cap (100 entries per call), this is bounded. At larger scale, an attacker could force compaction on every call by manipulating entries to maintain a high stale ratio.
**Blast radius**: Write latency on context_status calls. No data corruption (all writes are correct values).
**Mitigation**: The batch cap bounds write volume per call. The stale ratio threshold bounds compaction frequency (once compacted, stale_count drops to 0 and subsequent calls skip compaction). Accept at current scale.

### SEC-02: Coherence Score Manipulation
**Untrusted input**: Agents cannot directly set coherence scores (they are computed). However, an agent could manipulate inputs (e.g., quarantine many entries to lower contradiction density, or trigger many corrections to increase stale HNSW nodes) to artificially lower lambda and generate misleading recommendations.
**Blast radius**: Misleading maintenance recommendations in context_status output. No automatic actions triggered (quarantine remains human-decided).
**Mitigation**: Lambda is informational only. No automatic remediation based on lambda values. Accept.

### SEC-03: Schema Migration as Attack Vector
**Untrusted input**: The database file itself. If an attacker can modify the redb file to inject malformed entry bytes, the V2EntryRecord deserialization during migration could produce unexpected values or panic.
**Blast radius**: Migration fails, database stays at v2, Store::open returns error. No data corruption beyond what the attacker already achieved by modifying the file.
**Mitigation**: redb provides checksum integrity. bincode deserialization returns errors for malformed bytes (does not panic). Migration runs in a transaction that rolls back on error.

## Failure Modes

### FM-01: Schema Migration Failure
**Behavior**: migrate_v2_to_v3 encounters a corrupt entry. The redb write transaction rolls back. Store::open returns an error. The database remains at schema v2.
**Recovery**: On next Store::open, migration re-attempts from scratch. If the corrupt entry persists, manual intervention required (restore from backup or delete the corrupt entry).

### FM-02: Confidence Refresh Partial Failure
**Behavior**: update_confidence fails for one entry in the batch (e.g., entry deleted between scan and write). The specific entry is skipped. Other entries in the batch continue. confidence_refreshed_count reflects only successful refreshes.
**Recovery**: Automatic on next context_status call (the failed entry will be re-identified as stale if it still exists).

### FM-03: Graph Compaction Failure During Build
**Behavior**: HNSW construction fails (OOM or hnsw_rs error). The new graph is dropped. The old graph remains intact. graph_compacted = false. A maintenance recommendation describes the failure.
**Recovery**: Automatic on next context_status call (compaction re-attempted if stale ratio still exceeds threshold).

### FM-04: Graph Compaction VECTOR_MAP Write Failure
**Behavior**: The single write transaction for VECTOR_MAP fails (disk full). The new graph is dropped (VECTOR_MAP-first ordering: we have not swapped yet). The old graph and old VECTOR_MAP remain intact. context_status returns with graph_compacted = false.
**Recovery**: Fix disk space issue. Next context_status call re-attempts compaction.

### FM-05: Embed Service Timeout During Compaction
**Behavior**: embed_service.embed_entries() times out or returns an error while re-embedding active entries for compaction. Compaction is aborted. Old graph untouched. Recommendation emitted.
**Recovery**: Automatic on next context_status call once embed service is healthy.

### FM-06: NaN in Dimension Score
**Behavior**: Should not occur (all division-by-zero cases guarded). If it somehow does, lambda computation propagates NaN. StatusReport contains NaN coherence score. JSON serialization of NaN may produce "null" or error depending on serde configuration.
**Recovery**: Fix the unguarded division. Verify all dimension score functions are tested with zero-denominator inputs.

### FM-07: context_status Timeout from Compaction + Refresh
**Behavior**: At large scale, compaction (re-embedding all entries) plus confidence refresh (100 entries) could take 10+ seconds, exceeding MCP tool timeout.
**Recovery**: Reduce MAX_CONFIDENCE_REFRESH_BATCH. Accept that compaction is expensive and runs infrequently (only when stale ratio > 10%). Document latency expectations.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (schema migration atomicity) | R-01, R-13 | Resolved: Architecture C1 specifies single redb write transaction. V2EntryRecord intermediate struct handles f32->f64 cast. Migration is all-or-nothing. |
| SR-02 (residual f32 constants) | R-02, R-14 | Resolved: Architecture C2 provides exhaustive f32->f64 change inventory across all 5 crates. Weight sum invariant tested. |
| SR-03 (compaction destroys index) | R-03, R-06 | Resolved: ADR-004 mandates build-new-then-swap with VECTOR_MAP-first ordering. Old index untouched until swap. |
| SR-04 (embed service unavailable) | R-09 | Resolved: Architecture C8 gates compaction on embed service readiness. Graceful skip with recommendation. |
| SR-05 (noisy thresholds) | R-08, R-20 | Partially resolved: All thresholds are named constants (AC-16). Default values (24h staleness, 10% stale ratio, 0.8 lambda) are documented. Tuning is future work. |
| SR-06 (partial delivery) | -- | Resolved: Architecture defines Tier 1 (f64 + lambda read-only) and Tier 2 (refresh + compaction) as independently coherent subsets. |
| SR-07 (behavioral contract change) | R-07 | Resolved: ADR-002 adds maintenance opt-out parameter. Default true for self-healing, false for read-only diagnostics. |
| SR-08 (unavailable dimension inflates lambda) | R-05 | Resolved: ADR-003 excludes unavailable dimensions from weighted average and re-normalizes remaining weights. |
| SR-09 (test blast radius) | R-11 | Addressed: Architecture estimates 60-80 tests need mechanical f32->f64 updates. All changes are type promotions, no logic changes. |
| SR-10 (trait object safety) | R-17 | Resolved: Architecture C3 specifies compact(&self, ...) with concrete types. Object safety preserved. |
| SR-11 (write contention) | R-19 | Accepted: Single-agent stdio architecture makes concurrent context_status calls impossible at current scale. Documented as known limitation. |

## Test Coverage Requirements by Component

### coherence.rs (new module)
- Unit tests for each of the 4 dimension score functions with boundary values (R-10)
- Unit tests for compute_lambda with all dimensions, with excluded dimensions, with all-1.0, all-0.0 (R-05)
- Unit tests for weight re-normalization correctness (R-05)
- Unit tests for generate_recommendations with various lambda/threshold/count combinations (R-20)
- Unit tests for DEFAULT_WEIGHTS sum invariant (R-14)
- Unit tests for named constant values (threshold, batch cap, weights)

### confidence.rs (f64 upgrade)
- Update all weight constant tests from f32 to f64 (R-02, R-11)
- Update compute_confidence return type assertions to f64 (R-02)
- Update rerank_score parameter and return type tests to f64 (R-02, R-04)
- Update co_access_affinity tests to f64 (R-02)
- Verify weight sum invariant in f64 (R-14)
- Verify precision beyond 7 decimal digits for compute_confidence output (R-02)

### schema.rs (migration v2->v3)
- V2EntryRecord struct definition tests (R-13)
- Round-trip serialization: v2 bytes -> V2EntryRecord -> EntryRecord v3 (R-01, R-13)
- f32 confidence edge values through migration (R-01)
- Schema version counter after migration (R-01)
- Migration chain v0->v1->v2->v3 (R-01)

### index.rs (VectorIndex)
- SearchResult.similarity f64 type and precision (R-04)
- map_neighbours_to_results cast order verification (R-04)
- compact method: stale node elimination (R-03)
- compact method: VECTOR_MAP update (R-06)
- compact method: failure recovery (R-03)
- compact with empty index (R-18)
- compact with zero stale nodes (R-19)

### coaccess.rs (f64 upgrade)
- Update MAX_CO_ACCESS_BOOST and MAX_BRIEFING_CO_ACCESS_BOOST type tests (R-02)
- Update compute_search_boost and compute_briefing_boost return type tests (R-02)
- Verify boost values preserve f64 precision (R-02)

### tools.rs (context_status integration)
- End-to-end coherence computation in context_status (IR-02)
- Maintenance opt-out parameter behavior (R-07)
- Confidence refresh with batch cap (R-08)
- Graph compaction trigger based on stale ratio (R-03)
- Embed service unavailability handling (R-09)
- Stale confidence detection (R-16)

### response.rs (coherence formatting)
- JSON format with all coherence fields (R-12)
- Markdown format with coherence section (R-12)
- Summary format with coherence line (R-12)
- Maintenance recommendations in all formats (R-12)
- f64 serialization without f32 artifacts (R-02, R-12)

## Integration Test Scenarios

### IT-01: Full Coherence Pipeline (Happy Path)
Store 10 entries with known timestamps. Wait for staleness threshold to pass (or use deterministic time). Call context_status with maintenance=true. Verify: all dimension scores computed, stale entries refreshed, lambda computed, recommendations generated (or not), StatusReport contains all coherence fields.

### IT-02: Schema Migration End-to-End
Create a v2 database with known entries. Open with v3 code. Verify: migration runs, all entries readable with f64 confidence, schema_version == 3, subsequent context_status works correctly with coherence fields.

### IT-03: Graph Compaction End-to-End
Insert entries into HNSW. Create stale nodes by re-embedding entries via context_correct. Call context_status. Verify: compaction triggers, stale_count drops to 0, search results remain consistent, VECTOR_MAP updated.

### IT-04: Maintenance Opt-Out End-to-End
Create stale entries and stale HNSW nodes. Call context_status(maintenance: false). Verify: coherence scores computed, no writes performed, confidence_refreshed_count == 0, graph_compacted == false. Call again with maintenance: true. Verify: refresh and compaction execute.

### IT-05: f64 Scoring Pipeline End-to-End
Store an entry. Retrieve it via context_search. Verify: SearchResult.similarity is f64, confidence is f64, rerank_score produces f64 output, JSON response has clean f64 values without f32 artifacts.

### IT-06: Empty Knowledge Base Coherence
Open a fresh database. Call context_status. Verify: all dimensions 1.0, lambda 1.0, no recommendations, no errors.

### IT-07: Embed Service Unavailable During Compaction
Create stale HNSW nodes. Call context_status without initializing the embed service. Verify: compaction skipped, recommendation emitted, all other coherence fields computed correctly.

### IT-08: Confidence Refresh with Batch Cap
Create 150 stale entries. Call context_status. Verify: exactly 100 refreshed (batch cap). Call context_status again. Verify: remaining 50 refreshed.

## Regression Test Strategy

### f32->f64 Test Update Classification

The 811 existing tests fall into these categories for the f64 upgrade:

**Category A: Pass as-is (estimated ~550 tests)**
Tests that do not touch confidence, similarity, or scoring constants. Includes: embed tests (76), most store read/write tests, most server tool tests for non-scoring paths, core trait tests. These require zero changes.

**Category B: Mechanical type promotion (estimated 60-80 tests)**
Tests with hardcoded f32 confidence values (`confidence: 0.95`), f32 comparison assertions (`(result - 0.85).abs() < f32::EPSILON`), or f32 function signatures in test helpers. Changes are:
- Replace `f32` type annotations with `f64` in test code
- Replace `f32::EPSILON` with `f64::EPSILON` in assertions
- Replace `0.95_f32` literals with `0.95_f64` or just `0.95` (f64 is default)
- Update test helper functions that accept/return f32 confidence or similarity

**Category C: Assertion value changes (estimated 10-20 tests)**
Tests that assert exact f32 values which will differ slightly in f64. Example: a test asserting `result == 0.8500000238418579` (f32 representation of 0.85) should now assert `result == 0.85` (exact in f64). These require updating expected values.

**Category D: No change needed (estimated ~150 tests)**
Tests for contradiction detection, embedding consistency, input validation, and other non-scoring paths. These use f32 for HNSW-domain values which remain f32.

### Regression Verification Process

1. **Before any crt-005 changes**: Run `cargo test --workspace` and record pass count (should be 811+). This is the baseline.
2. **After Tier 1 (f64 upgrade)**: Run `cargo test --workspace`. Fix all compile errors from type changes (Category B). Fix all assertion failures from value changes (Category C). Verify pass count >= baseline.
3. **After Tier 2 (refresh + compaction)**: Run `cargo test --workspace`. Verify pass count >= Tier 1 count (new tests added, none removed).
4. **Final**: Verify no test is `#[ignore]`d or `#[cfg(skip)]`d as part of crt-005. Every existing test must pass, not be suppressed.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-02, R-13) | 13 scenarios |
| High | 8 (R-01, R-03, R-05, R-06, R-10, R-11, R-14, R-17) | 42 scenarios |
| Medium | 8 (R-04, R-07, R-08, R-09, R-12, R-15, R-16, R-18) | 31 scenarios |
| Low | 2 (R-19, R-20) | 8 scenarios |
| **Total** | **20 risks** | **94 scenarios** |
