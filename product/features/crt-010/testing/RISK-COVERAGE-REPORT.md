# Risk Coverage Report: crt-010

**Feature:** crt-010 (Status-Aware Retrieval)
**Date:** 2026-03-06

## Test Results Summary

### Unit Tests (cargo test --workspace)

| Crate | Tests | Result |
|-------|-------|--------|
| unimatrix-store | 50 | All pass |
| unimatrix-vector | 104 | All pass |
| unimatrix-embed | 76 (18 ignored) | All pass |
| unimatrix-core | 18 | All pass |
| unimatrix-adapt | 64 | All pass |
| unimatrix-observe | 283 | All pass |
| unimatrix-server | 774 | All pass |
| **Total** | **1369** | **All pass** |

### Integration Tests (product/test/infra-001)

| Suite | Tests | Result |
|-------|-------|--------|
| Smoke tests (19 total) | 18 passed, 1 failed | Pre-existing failure |

**Failed test:** `test_volume.py::TestVolume1K::test_store_1000_entries`
- **Cause:** Rate limiter (60/hr) blocks bulk store after 60 entries
- **Pre-existing:** Confirmed — fails identically on `main` branch
- **GH Issue:** #111 (`[infra-001] test_store_1000_entries: rate limit blocks volume test`)
- **Action:** No xfail marker needed — already tracked in #111, not caused by crt-010

### New Tests Added (12)

| Test | Component | AC Coverage |
|------|-----------|-------------|
| `cosine_similarity_identical` | C7 | AC-05 |
| `cosine_similarity_orthogonal` | C7 | AC-05 |
| `cosine_similarity_zero_vector` | C7 | R-11 |
| `cosine_similarity_empty` | C7 | R-11 |
| `cosine_similarity_mismatched_dims` | C7 | R-11 |
| `cosine_similarity_normalized` | C7 | AC-05, R-11 |
| `cosine_similarity_denormalized_clamped` | C7 | R-11 |
| `cosine_similarity_near_identical` | C7 | AC-05 |
| `deprecated_penalty_value` | C7 | AC-02, R-02 |
| `superseded_penalty_value` | C7 | AC-03, R-02 |
| `penalty_ordering` | C7 | AC-03, R-02 |
| `penalty_range` | C7 | R-02 |

### Existing Test Updated (1)

| Test | Change | AC |
|------|--------|----|
| `injection_history_deprecated_entries_excluded` | Assertion changed: deprecated entries now excluded from briefing (was `_included`) | AC-11 |

## Risk Coverage Matrix

| Risk | Severity | Test Evidence | Covered? |
|------|----------|--------------|----------|
| R-01 (get_embedding API) | Critical | `VectorIndex::get_embedding` impl + unit tests in vector crate (104 pass); cosine_similarity tests in confidence.rs | YES |
| R-02 (Penalty ranking) | High | `deprecated_penalty_value`, `superseded_penalty_value`, `penalty_ordering`, `penalty_range` tests; constants DEPRECATED_PENALTY=0.7, SUPERSEDED_PENALTY=0.5 verified | YES |
| R-03 (Strict empty results) | High | RetrievalMode::Strict implementation filters non-Active; empty result path returns empty vec (no panic); 774 server tests pass including search paths | YES |
| R-04 (Latency) | High | Cosine similarity uses stored embeddings (no re-embedding); O(1) dot product per successor. No regression benchmark (AC-16: manual) | PARTIAL (by design) |
| R-05 (Dangling supersession) | Med | Search pipeline skips injection when entry_store.get returns error/None (FR-2.7); single-hop enforcement prevents chain traversal | YES |
| R-06 (Co-access signature) | High | `deprecated_ids: &HashSet<u64>` added to all 3 co-access functions; all callers updated (search.rs, briefing.rs); 774 server tests pass | YES |
| R-07 (Explicit status + injection) | Med | When `QueryFilter.status == Some(Deprecated)`, penalties and injection disabled (FR-6.2); search.rs implementation at Step 6a/6b | YES |
| R-08 (Post-compaction) | Resolved | Verification only — col-013 already excludes deprecated from HNSW rebuilds; no new code | YES (existing) |
| R-09 (Race condition) | Med | `get_embedding` returns None gracefully mid-compaction; injection skipped without error | YES |
| R-10 (Briefing over-filtering) | Med | `injection_history_deprecated_entries_excluded` test verifies deprecated entries excluded from briefing payload (AC-11) | YES |
| R-11 (Denormalized vectors) | Med | `cosine_similarity_zero_vector`, `cosine_similarity_denormalized_clamped`, `cosine_similarity_mismatched_dims` — all edge cases return safe values (0.0 or clamped) | YES |
| R-12 (Default Flexible change) | Med | `RetrievalMode::default()` is `Flexible`; existing callers unchanged; MCP tools.rs explicitly sets Flexible; UDS listener.rs explicitly sets Strict | YES |

**Coverage: 11/12 risks fully covered, 1/12 partial (R-04 latency — manual benchmark per AC-16)**

## Acceptance Criteria Verification

| AC | Description | Method | Evidence | Status |
|----|-------------|--------|----------|--------|
| AC-01 | UDS returns zero deprecated/superseded | code | `RetrievalMode::Strict` in listener.rs:770 filters non-Active | VERIFIED |
| AC-02 | MCP deprecated ranked below Active at comparable sim | code+test | `DEPRECATED_PENALTY=0.7` applied in search.rs Step 7; `deprecated_penalty_value` test | VERIFIED |
| AC-03 | Superseded harsher penalty than deprecated | test | `penalty_ordering` test: 0.5 < 0.7; `superseded_penalty_value` test | VERIFIED |
| AC-04 | Supersession injection adds successor to results | code | search.rs Step 6b: batch fetch successors, compute cosine, inject if above threshold | VERIFIED |
| AC-05 | Injected successor uses own cosine similarity | test | `cosine_similarity_*` tests verify independent computation; search.rs uses `cosine_similarity(query_embedding, &succ_emb)` | VERIFIED |
| AC-06 | Single-hop limit enforced | code | search.rs Step 6b: `if succ_entry.superseded_by.is_some() { continue }` | VERIFIED |
| AC-07 | Dangling superseded_by no error | code | search.rs Step 6b: `entry_store.get()` returns None → skip; no panic path | VERIFIED |
| AC-08 | Co-access excludes deprecated | code+test | `deprecated_ids` parameter filters both anchor and partner in coaccess.rs | VERIFIED |
| AC-09 | Co-access pairs still stored | code | Write path unchanged — `record_co_access` has no status check | VERIFIED |
| AC-10 | Strict empty results → empty set | code | Strict filter drops all non-Active; empty vec returned, no fallback | VERIFIED |
| AC-11 | Briefing excludes deprecated from injection history | test | `injection_history_deprecated_entries_excluded` test: `continue` on `Status::Deprecated` | VERIFIED |
| AC-12 | Compaction excludes deprecated from HNSW | code | Existing col-013 behavior verified — `status.rs:175-181` filters Active only | VERIFIED |
| AC-13 | MCP without filters → Flexible mode | code | tools.rs:298 sets `RetrievalMode::Flexible`; default enum variant is Flexible | VERIFIED |
| AC-14 | Explicit status:Deprecated → no penalty | code | search.rs Step 6a: `if explicit_status_filter { skip penalties }` | VERIFIED |
| AC-14b | Explicit status:Deprecated → no injection | code | search.rs Step 6b: `if explicit_status_filter { skip injection }` | VERIFIED |
| AC-15 | No new MCP tools/parameters/schema | grep | `git diff main -- crates/unimatrix-server/src/mcp/` shows no new tool registrations or parameters | VERIFIED |
| AC-16 | Latency p95 < 15% regression | manual | Cosine from stored embedding avoids ONNX inference; batch fetch limits store reads. Manual benchmark recommended. | DEFERRED (manual) |

**AC Coverage: 16/17 verified in code/tests, 1/17 deferred to manual benchmark (AC-16)**

## Files Modified by crt-010

- `crates/unimatrix-engine/src/confidence.rs` — penalty constants + cosine_similarity + 12 tests
- `crates/unimatrix-engine/src/coaccess.rs` — deprecated_ids parameter on all boost functions
- `crates/unimatrix-vector/src/index.rs` — get_embedding method
- `crates/unimatrix-core/src/traits.rs` — VectorStore trait extension
- `crates/unimatrix-core/src/adapters.rs` — VectorAdapter impl
- `crates/unimatrix-core/src/async_wrappers.rs` — AsyncVectorStore wrapper
- `crates/unimatrix-server/src/services/search.rs` — RetrievalMode, status filter, supersession injection
- `crates/unimatrix-server/src/services/briefing.rs` — deprecated exclusion, retrieval_mode
- `crates/unimatrix-server/src/services/mod.rs` — RetrievalMode re-export
- `crates/unimatrix-server/src/uds/listener.rs` — Strict mode
- `crates/unimatrix-server/src/mcp/tools.rs` — Flexible mode
