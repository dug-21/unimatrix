# Risk-Based Test Strategy: nxs-002 (Vector Index)

**Feature**: nxs-002 (Nexus Phase)
**Agent**: nxs-002-agent-3-risk
**Date**: 2026-02-22

---

## 1. Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | IdMap desync with VECTOR_MAP | Critical | Medium | Critical |
| R-02 | Dimension mismatch silent corruption | Critical | Medium | Critical |
| R-03 | Filtered search returns wrong results | High | Medium | High |
| R-04 | Persistence round-trip data loss | High | Medium | High |
| R-05 | RwLock deadlock or starvation | High | Low | High |
| R-06 | Re-embedding stale point corruption | High | Medium | High |
| R-07 | Empty index edge cases | Medium | High | Medium |
| R-08 | Similarity score computation error | Medium | Low | Medium |
| R-09 | Data ID overflow or collision | Medium | Low | Medium |
| R-10 | hnsw_rs API misuse | High | Low | Medium |
| R-11 | Load fails with corrupt/missing files | Medium | Medium | Medium |
| R-12 | usize/u64 cast boundary | Low | Low | Low |

---

## 2. Risk-to-Scenario Mapping

### R-01: IdMap Desync with VECTOR_MAP (CRITICAL)

**Severity**: Critical
**Likelihood**: Medium
**Impact**: If IdMap diverges from VECTOR_MAP, search results map to wrong entries. Filtered search uses wrong data IDs. Results are silently incorrect -- no error is raised.

**Test Scenarios**:
1. Insert 100 vectors. Verify IdMap forward and reverse maps match VECTOR_MAP for every entry.
2. After re-embedding entry E, verify IdMap `entry_to_data[E]` matches VECTOR_MAP `get_vector_mapping(E)`.
3. After dump + load, verify rebuilt IdMap matches VECTOR_MAP exactly.
4. Insert vector for non-existent entry_id (entry not in Store). Verify VECTOR_MAP and IdMap are still consistent (VECTOR_MAP write succeeds independently of ENTRIES table).

**Coverage Requirement**: Every insert, re-embed, and load operation must verify IdMap consistency with VECTOR_MAP. Property test: for any sequence of inserts, `IdMap.entry_to_data[e]` == `Store.get_vector_mapping(e)` for all mapped entries.

---

### R-02: Dimension Mismatch Silent Corruption (CRITICAL)

**Severity**: Critical
**Likelihood**: Medium -- dimension validation is our responsibility (hnsw_rs does not validate). A bug in validation lets wrong-dimension vectors through, corrupting distance calculations silently.
**Impact**: Search returns nonsensical results. Index is corrupted permanently (no way to identify which vectors are wrong-dimension without re-embedding all).

**Test Scenarios**:
1. Attempt to insert a 128-d vector. Verify `VectorError::DimensionMismatch { expected: 384, got: 128 }`.
2. Attempt to insert a 512-d vector. Verify same error.
3. Attempt to insert a 0-d vector (empty). Verify same error.
4. Attempt to insert a 383-d vector (off-by-one). Verify error.
5. Attempt to insert a 385-d vector (off-by-one). Verify error.
6. Attempt to search with a 128-d query. Verify `DimensionMismatch` error.
7. Attempt to search with a 0-d query. Verify error.
8. Insert valid 384-d vector, then search with valid 384-d query. Verify success.

**Coverage Requirement**: Both `insert` and `search` must validate dimensions. Off-by-one edge cases covered.

---

### R-03: Filtered Search Returns Wrong Results (HIGH)

**Severity**: High
**Likelihood**: Medium -- the filter construction pipeline (entry_ids -> data_ids -> sorted Vec -> FilterT) has multiple translation steps, each a potential failure point.
**Impact**: context_search returns entries that should have been filtered out, or misses entries that should appear.

**Test Scenarios**:
1. Insert 10 vectors. Filter to allow 3 specific entry IDs. Verify only those 3 appear in results.
2. Filter with entry IDs that have no vector mappings. Verify empty result (no error).
3. Filter with mix of mapped and unmapped entry IDs. Verify only mapped entries appear.
4. Filter with all entry IDs (no restriction). Verify same results as unfiltered search.
5. Filter with single entry ID. Verify that entry appears if it is similar enough.
6. Filter with empty allow-list. Verify empty result.
7. Insert entries with known similar embeddings. Filter to exclude the most similar. Verify it is excluded and second-most-similar appears as top result.

**Coverage Requirement**: Filter correctness for all combinations: empty, single, subset, all, unknown IDs. Verify exclusion actually works (not just inclusion).

---

### R-04: Persistence Round-Trip Data Loss (HIGH)

**Severity**: High
**Likelihood**: Medium -- hnsw_rs persistence is NOT atomic. The dump produces two files plus our metadata file. Corruption of any file means data loss.
**Impact**: Index is lost on restart. Requires full re-embedding (expensive if embedding pipeline is API-based).

**Test Scenarios**:
1. Insert 100 vectors. Dump. Load. Search for each vector's embedding. Verify same results as before dump.
2. Insert 100 vectors. Dump. Load. Verify `point_count()` matches.
3. Insert 100 vectors. Dump. Load. Verify IdMap is consistent with VECTOR_MAP.
4. Dump to non-existent directory. Verify error (not panic).
5. Load from directory with missing .hnsw.graph file. Verify `VectorError::Persistence`.
6. Load from directory with missing .hnsw.data file. Verify `VectorError::Persistence`.
7. Load from directory with missing .meta file. Verify `VectorError::Persistence`.
8. Load from empty directory. Verify `VectorError::Persistence`.
9. Dump, modify metadata file to corrupt `next_data_id`, load. Verify error or fallback.
10. Multiple dump/load cycles (dump, load, insert more, dump, load). Verify data integrity across cycles.

**Coverage Requirement**: Full round-trip verification with search result comparison. All missing-file scenarios. Multi-cycle persistence.

---

### R-05: RwLock Deadlock or Starvation (HIGH)

**Severity**: High
**Likelihood**: Low -- the code is straightforward (no nested locks in the same call path), but RwLock interactions between the hnsw lock and IdMap lock need careful review.
**Impact**: Application hangs. MCP server becomes unresponsive.

**Test Scenarios**:
1. Verify that VectorIndex has no nested lock acquisitions (code review, not runtime test).
2. Insert 1000 vectors sequentially. Verify no deadlock (functional test that exercises write path thoroughly).
3. Concurrent inserts from multiple threads: spawn 10 threads, each inserting 100 vectors. Verify all succeed with no hangs (timeout test).
4. Concurrent searches from multiple threads: spawn 10 threads, each searching 100 times. Verify all succeed.
5. Concurrent insert + search: one thread inserting, another searching simultaneously. Verify no deadlock.

**Coverage Requirement**: Code review for nested lock patterns. Functional tests for sequential operations. Concurrency tests with timeout enforcement.

---

### R-06: Re-Embedding Stale Point Corruption (HIGH)

**Severity**: High
**Likelihood**: Medium -- re-embedding creates a stale point in hnsw_rs. If the stale point appears in search results and maps to the wrong entry, results are incorrect.
**Impact**: Search returns correct entry_id (IdMap maps both old and new data_id to the same entry_id), but the old embedding is less accurate. The stale point wastes memory and may degrade recall quality.

**Test Scenarios**:
1. Insert entry E with embedding A. Re-embed E with embedding B (different). Search for B. Verify E appears as top result.
2. After re-embedding, verify `contains(E)` returns true.
3. After re-embedding, verify `stale_count() == 1`.
4. After re-embedding, verify `point_count()` increased by 1 (old point still in hnsw_rs).
5. Insert entry E with embedding A. Re-embed E with a very different embedding B. Search for A. Verify E still appears (old stale point maps to E correctly).
6. Re-embed same entry 5 times. Verify stale_count increases, VECTOR_MAP has latest mapping.

**Coverage Requirement**: Re-embedding must preserve correctness. Stale points must not cause incorrect entry mappings.

---

### R-07: Empty Index Edge Cases (MEDIUM)

**Severity**: Medium
**Likelihood**: High -- empty index is the initial state before any embeddings are generated. The first search after startup (before nxs-003 runs) hits this.
**Impact**: Panic or confusing error on search. Should return empty results gracefully.

**Test Scenarios**:
1. Create new VectorIndex. Call `search`. Verify empty Vec returned.
2. Create new VectorIndex. Call `search_filtered`. Verify empty Vec returned.
3. Create new VectorIndex. Call `point_count()`. Verify 0.
4. Create new VectorIndex. Call `contains(42)`. Verify false.
5. Create new VectorIndex. Call `stale_count()`. Verify 0.
6. Create new VectorIndex. Call `dump`. Verify success (dumps empty index).
7. Load an empty dumped index. Verify success. Search returns empty.

**Coverage Requirement**: Every public method must handle empty index without panic.

---

### R-08: Similarity Score Computation Error (MEDIUM)

**Severity**: Medium
**Likelihood**: Low -- the formula `1.0 - distance` is simple, but floating-point edge cases (NaN, infinity, negative distances) could produce wrong scores.
**Impact**: Incorrect similarity scores confuse downstream consumers (vnc-002 near-duplicate threshold, confidence scoring).

**Test Scenarios**:
1. Insert two identical normalized vectors. Verify similarity ~= 1.0 (within f32 tolerance).
2. Insert two orthogonal normalized vectors. Verify similarity ~= 0.0.
3. Insert a vector and its negation. Verify similarity ~= -1.0 (or close, depending on DistDot behavior with negated normalized vectors).
4. Verify results are sorted by similarity descending.
5. Insert 10 vectors at known angles. Verify similarity ordering matches expected angular ordering.

**Coverage Requirement**: Known-value tests for similarity extremes. Ordering verification.

---

### R-09: Data ID Overflow or Collision (MEDIUM)

**Severity**: Medium
**Likelihood**: Low -- AtomicU64 starting at 0 with increment 1. Overflow requires 2^64 inserts (impossible). Collision requires concurrent increment to produce same value (AtomicU64 prevents this).
**Impact**: Two entries share the same hnsw data ID. One overwrites the other in VECTOR_MAP.

**Test Scenarios**:
1. Insert 1000 vectors. Verify all data IDs are unique (collect from VECTOR_MAP, check set size).
2. Verify AtomicU64 counter starts at 0 and increments monotonically.
3. After load, verify counter is restored correctly (next insert produces data_id > all existing).

**Coverage Requirement**: Uniqueness verification. Counter persistence across dump/load.

---

### R-10: hnsw_rs API Misuse (MEDIUM)

**Severity**: High
**Likelihood**: Low -- the API is documented, but subtle misuse (wrong parameter order, forgetting set_searching_mode) could cause silent quality degradation.
**Impact**: Search quality degrades silently. Inserts fail.

**Test Scenarios**:
1. After inserting vectors, search returns meaningful results (not random). Verify by self-search: each entry's embedding produces that entry as top result.
2. Verify `get_nb_point()` matches expected count after inserts.
3. Verify that search works correctly after mode transition (insert mode -> search mode).

**Coverage Requirement**: Self-search validation (AC-13). Point count consistency.

---

### R-11: Load Fails with Corrupt/Missing Files (MEDIUM)

**Severity**: Medium
**Likelihood**: Medium -- hnsw_rs dump is not atomic. Power loss during dump, disk full, or file system corruption can leave files in bad state.
**Impact**: Index cannot be loaded. Must fall back to re-embedding.

**Test Scenarios**:
1. Load from non-existent directory. Verify `VectorError::Persistence`.
2. Load from directory with partial files (graph but no data). Verify error.
3. Load from directory with zero-byte files. Verify error.
4. Verify error message is descriptive (includes path information).

**Coverage Requirement**: All missing/corrupt file permutations produce clear errors, not panics.

---

### R-12: usize/u64 Cast Boundary (LOW)

**Severity**: Low
**Likelihood**: Low -- all target platforms are 64-bit. `usize` == `u64`.
**Impact**: On 32-bit platforms, data IDs > 2^32 would be truncated, causing collisions.

**Test Scenarios**:
1. Compile-time assertion: `assert!(std::mem::size_of::<usize>() >= 8)` in a test.
2. Insert with data_id = u32::MAX + 1. Verify no truncation (search returns correct result).

**Coverage Requirement**: Compile-time assertion for 64-bit platform. One boundary test.

---

## 3. Integration Risks

### IR-01: VECTOR_MAP Write Failure During Insert

If `Store::put_vector_mapping` fails after hnsw_rs insert succeeds, the vector exists in hnsw_rs but not in VECTOR_MAP. The point is unreachable -- it wastes memory but does not corrupt results (IdMap won't have it, VECTOR_MAP won't have it, so it is invisible).

**Mitigation**: The insert method should attempt VECTOR_MAP write after hnsw_rs insert. If VECTOR_MAP write fails, the hnsw_rs point is "leaked" but benign. Return the error to the caller. Document this behavior.

**Test Scenario**: Mock or trigger a store write failure after hnsw_rs insert. Verify error is returned and index state is not corrupted.

### IR-02: VECTOR_MAP Iteration for IdMap Rebuild

The `load` path needs to iterate all VECTOR_MAP entries. unimatrix-store does not currently expose this. Adding `Store::iter_vector_mappings()` is required.

**Mitigation**: Implement the iteration method in unimatrix-store as part of nxs-002 implementation. Test with 0, 1, 100, and 10K entries.

**Test Scenario**: Load after inserting 0, 1, 100 entries. Verify IdMap matches VECTOR_MAP exactly.

### IR-03: Store and VectorIndex Lifecycle Mismatch

If VectorIndex is created with a Store that has existing VECTOR_MAP entries (from a previous session), but VectorIndex is created fresh (not loaded), the VECTOR_MAP entries are orphaned -- they reference data IDs that don't exist in the hnsw_rs index.

**Mitigation**: Document that `VectorIndex::new` creates a fresh index. If VECTOR_MAP has entries, the caller should use `VectorIndex::load` instead. The `new` method does not clear VECTOR_MAP (that would destroy crash-recovery data).

**Test Scenario**: Create Store, insert entries with vectors, close. Reopen Store, create new VectorIndex (not load). Verify VectorIndex is empty. Verify VECTOR_MAP still has old entries.

### IR-04: Concurrent VectorIndex and Store Access

VectorIndex holds `Arc<Store>` and calls `put_vector_mapping` during insert. Store's VECTOR_MAP write acquires a redb write transaction. If another caller is simultaneously writing to the Store (inserting entries), redb serializes the writes. No data corruption, but potential latency spike.

**Mitigation**: redb handles write serialization safely. Document that VectorIndex inserts may block on Store write contention.

---

## 4. Edge Cases

### EC-01: Search with top_k = 0
Return empty vec.

### EC-02: Search with top_k > index size
Return all vectors in the index.

### EC-03: Search with ef_search < top_k
Use top_k as effective ef_search.

### EC-04: Insert with entry_id = 0
Entry ID 0 is sentinel in nxs-001. VectorIndex does not enforce this constraint -- it is the caller's responsibility. Document this.

### EC-05: Insert with entry_id = u64::MAX
Valid. No special handling needed.

### EC-06: Filtered search where all entries are filtered out
Return empty vec.

### EC-07: Very large embedding values (f32::MAX)
hnsw_rs computes DistDot as `1 - dot(a, b)`. For very large values, this can overflow. Document that embeddings must be L2-normalized.

### EC-08: NaN in embedding
Unpredictable hnsw_rs behavior. Should we validate for NaN? Recommendation: validate and return error. Add `VectorError::InvalidEmbedding` for NaN/infinity values.

### EC-09: Dump to read-only directory
File system error. Return `VectorError::Persistence` with IO error details.

### EC-10: Load with wrong VectorConfig dimension
Loaded index has 384-d vectors but config says 256-d. IdMap loads fine but subsequent inserts would fail dimension check. Recommendation: store dimension in metadata, validate on load.

---

## 5. Failure Modes

### FM-01: hnsw_rs Index Corruption
**Trigger**: Crash during dump, disk full, file system corruption.
**Behavior**: `VectorIndex::load` returns `VectorError::Persistence`. Index cannot be loaded.
**Recovery**: Create new empty index. Trigger full re-embedding from Store entries. VECTOR_MAP in redb is crash-safe and survives.

### FM-02: VECTOR_MAP Desync from hnsw_rs
**Trigger**: VECTOR_MAP write fails during insert (IR-01). Or manual VECTOR_MAP modification.
**Behavior**: Some entries invisible to search (hnsw_rs has point but IdMap/VECTOR_MAP don't know about it). Or filtered search sends wrong data IDs.
**Recovery**: Rebuild index from scratch (re-embed all entries). This is the nuclear option.

### FM-03: Memory Exhaustion
**Trigger**: Too many entries for available RAM. At 384d: 100K entries ~ 183 MB in hnsw_rs alone.
**Behavior**: Allocation failure in hnsw_rs. Likely panic (hnsw_rs does not return Result from insert).
**Recovery**: Reduce index size. Deprecate old entries. Rebuild index without deprecated entries.
**Prevention**: Document memory estimates per entry count. vnc-002's `context_status` tool should report vector index size.

### FM-04: Disk Full During Dump
**Trigger**: Insufficient disk space for .hnsw.graph + .hnsw.data files.
**Behavior**: `file_dump` returns error (partial files may exist).
**Recovery**: Free disk space. Re-dump. Partial files should be deleted by the caller.

---

## 6. Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 12 scenarios |
| High | 4 (R-03, R-04, R-05, R-06) | 28 scenarios |
| Medium | 4 (R-07, R-08, R-09, R-10) | 18 scenarios |
| Low | 1 (R-12) | 2 scenarios |
| Integration | 4 (IR-01..04) | 5 scenarios |
| Edge Cases | 10 (EC-01..10) | 10 scenarios |
| **Total** | **25** | **~75 scenarios** |

### Test Priority Order

1. **R-02** (Dimension mismatch) -- Test FIRST. If dimension validation is wrong, all other tests on corrupted data are meaningless.
2. **R-01** (IdMap desync) -- Core data integrity.
3. **R-03** (Filtered search) -- Correct filtering is critical for context_search quality.
4. **R-06** (Re-embedding) -- Common operation, subtle stale-point behavior.
5. **R-04** (Persistence) -- Data survival across restarts.
6. **R-07** (Empty index) -- First thing users hit.
7. **R-08** (Similarity scores) -- Downstream consumers depend on score quality.
8. **R-05** (Concurrency) -- Validated by concurrent tests, code review for lock patterns.
9. **R-10** (API misuse) -- Self-search validation.
10. **R-09, R-11, R-12** -- Lower priority, basic tests sufficient.
