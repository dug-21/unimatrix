# Test Plan: C7 Maintenance Parameter

## Component

C7: Maintenance Parameter (`crates/unimatrix-server/src/tools.rs` -- StatusParams + handler gating)

## Risks Covered

| Risk | Description | Priority |
|------|-------------|----------|
| R-07 | Maintenance opt-out completeness | Med |

## Integration Tests (tools.rs / server tests)

### IT-C7-01: maintain=false -- confidence refresh skipped
- Store entries with old timestamps (stale)
- Call context_status with maintain=false
- Assert: confidence_refreshed_count == 0
- Assert: entry confidence values unchanged
- Covers: R-07 scenario 1, AC-09

### IT-C7-02: maintain=false -- graph compaction skipped
- Create conditions where stale_ratio > threshold (stale HNSW nodes)
- Call context_status with maintain=false
- Assert: graph_compacted == false
- Covers: R-07 scenario 2

### IT-C7-03: maintain=false -- co-access cleanup skipped
- Create stale co-access pairs
- Call context_status with maintain=false
- Assert: stale co-access pairs still present after call
- Covers: R-07 scenario 3

### IT-C7-04: maintain=false -- dimension scores still computed
- Store entries (some stale, some quarantined)
- Call context_status with maintain=false
- Assert: coherence score < 1.0 (reflects degradation)
- Assert: all 4 dimension scores present in response
- Assert: lambda computed
- Covers: R-07 scenario 4

### IT-C7-05: maintain=true -- confidence refresh runs
- Store entries with old timestamps
- Call context_status with maintain=true
- Assert: confidence_refreshed_count > 0
- Covers: R-07 scenario 5

### IT-C7-06: maintain absent (default) -- same as false
- Call context_status without maintain parameter
- Assert: confidence_refreshed_count == 0
- Assert: graph_compacted == false
- Assert: dimension scores still computed
- Covers: R-07 scenario 6, AC-09

### IT-C7-07: maintain=false -- contradiction scanning still runs
- Store entries with contradictions (quarantined entries)
- Call context_status with maintain=false
- Assert: contradiction_density_score reflects quarantined entries
- Assert: this is a read operation, no entries modified
- Covers: R-07 scenario 7

## MCP Tool Schema Tests

### IT-C7-08: maintain parameter in MCP tool schema
- Verify the context_status tool schema includes "maintain" parameter
- Verify it is typed as boolean
- Verify it is not required (optional)
- This may be verified via the tool listing endpoint or schema inspection

## StatusParams Tests

### UT-C7-01: StatusParams.maintain defaults to None
- Construct StatusParams without maintain field
- Assert: maintain == None
- Resolve: unwrap_or(false) -> false

### UT-C7-02: StatusParams.maintain=true resolution
- Construct StatusParams with maintain=Some(true)
- Assert: unwrap_or(false) -> true

### UT-C7-03: StatusParams.maintain=false resolution
- Construct StatusParams with maintain=Some(false)
- Assert: unwrap_or(false) -> false

## Dependencies

- C5 (confidence refresh): refresh logic must exist for maintain=true to trigger it
- C8 (compaction integration): compaction wiring must exist for maintain=true to trigger it

## Estimated Test Count

- 3 unit tests
- 8 integration tests
- ~11 total new tests
