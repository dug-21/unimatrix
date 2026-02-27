# Test Plan: C8 Compaction Integration

## Component

C8: Compaction Integration (`crates/unimatrix-server/src/tools.rs` -- context_status handler)

## Risks Covered

| Risk | Description | Priority |
|------|-------------|----------|
| R-09 | Embed service unavailable during compaction | Med |
| R-18 | Empty knowledge base edge cases | Med |

## Integration Tests (tools.rs / server tests)

### IT-C8-01: Compaction triggers when maintain=true and stale ratio exceeds threshold
- Insert entries and create stale HNSW nodes (stale_ratio > 0.10)
- Initialize embed service
- Call context_status with maintain=true
- Assert: graph_compacted == true
- Assert: graph_stale_ratio reported correctly (post-compaction should be 0.0 or near 0.0)
- Covers: AC-11, AC-14

### IT-C8-02: Compaction skipped when maintain=false despite high stale ratio
- Insert entries and create stale HNSW nodes (stale_ratio > 0.10)
- Call context_status with maintain=false
- Assert: graph_compacted == false
- Assert: graph_stale_ratio still reflects pre-compaction state
- Covers: R-07

### IT-C8-03: Embed service unavailable -- compaction skipped gracefully
- Create conditions where stale_ratio > threshold
- Do NOT initialize embed service (or ensure it is unavailable)
- Call context_status with maintain=true
- Assert: graph_compacted == false
- Assert: context_status completes successfully (no panic)
- Assert: all other coherence scores are computed correctly
- Assert: lambda reflects current (degraded) graph quality
- Covers: R-09 scenarios 1, 3, 4

### IT-C8-04: graph_stale_ratio reported even without compaction
- Create stale HNSW nodes
- Call context_status with maintain=false
- Assert: graph_stale_ratio > 0.0 (reflects actual stale ratio)
- Assert: graph_compacted == false
- Covers: Pseudocode scenario 7

### IT-C8-05: Post-compaction stale_count == 0
- Insert entries, create stale nodes, trigger compaction
- Assert: vector index stale_count() == 0 after compaction
- Covers: AC-13

### IT-C8-06: Post-compaction search returns same entries
- Insert entries with known embeddings, search, record entry_ids
- Create stale nodes, trigger compaction via context_status
- Search again with same query
- Assert: same entry_ids returned
- Covers: AC-20

### IT-C8-07: Empty knowledge base -- no compaction
- Open fresh database (no entries)
- Call context_status with maintain=true
- Assert: graph_compacted == false (no stale nodes exist)
- Assert: graph_stale_ratio == 0.0
- Assert: all dimension scores == 1.0
- Assert: lambda == 1.0
- Assert: no recommendations
- Covers: R-18 scenarios 1-4, IT-06

### IT-C8-08: graph_compacted=true in status report
- Trigger compaction (maintain=true, high stale ratio, embed available)
- Read StatusReport
- Assert: graph_compacted field is true in JSON/markdown/summary
- Covers: AC-14

### IT-C8-09: Stale ratio below threshold -- compaction not triggered
- Insert entries with stale_ratio = 0.05 (below 0.10 threshold)
- Call context_status with maintain=true
- Assert: graph_compacted == false
- Assert: confidence refresh may still run (independent of compaction)

## End-to-End Integration Tests

### IT-C8-10: Full coherence pipeline happy path (IT-01)
- Store 10 entries with known timestamps and embeddings
- Some entries stale (old timestamps)
- Embed service available
- Call context_status with maintain=true
- Verify:
  - All 4 dimension scores computed
  - Stale entries refreshed
  - Compaction ran if stale_ratio > threshold
  - Lambda computed correctly
  - Recommendations generated if lambda < 0.8
  - StatusReport contains all 10 coherence fields
- Covers: IT-01

### IT-C8-11: Maintenance opt-out end-to-end (IT-04)
- Create stale entries and stale HNSW nodes
- Call context_status(maintain=false)
- Verify: scores computed, no writes, refreshed_count==0, compacted==false
- Call context_status(maintain=true)
- Verify: refresh and compaction execute
- Covers: IT-04

### IT-C8-12: Confidence refresh with batch cap end-to-end (IT-08)
- Store 150 stale entries
- Call context_status with maintain=true
- Assert: confidence_refreshed_count == 100 (batch cap)
- Call context_status with maintain=true again
- Assert: confidence_refreshed_count == 50 (remaining)
- Covers: IT-08

## Edge Cases

### EC-C8-01: Re-embedding fails during compaction
- Embed service returns error for embed_entries
- Assert: compaction skipped, old graph untouched
- Assert: graph_compacted == false
- Assert: warning logged

### EC-C8-02: compact() fails during graph build
- If testable: pass malformed embeddings to compact
- Assert: old index untouched, graph_compacted == false

## Dependencies

- C3 (vector compaction): VectorIndex::compact must be implemented
- C4 (coherence module): dimension scores and lambda computation
- C5 (confidence refresh): refresh logic in handler
- C6 (status extension): StatusReport coherence fields
- C7 (maintenance parameter): maintain flag gating

## Estimated Test Count

- 12-14 integration tests (including end-to-end scenarios)
