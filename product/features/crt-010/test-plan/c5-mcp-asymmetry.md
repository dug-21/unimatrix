# C5: MCP Filter Asymmetry Fix — Test Plan

## Location
`crates/unimatrix-server/src/mcp/tools.rs` (integration tests)

## Tests

### T-MCP-01: context_search without filters applies Flexible mode (AC-13, FR-6.1)
- Call context_search with no topic/category/tags
- Insert: Active + Deprecated entries
- Expected: Deprecated entry in results with reduced score (0.7x penalty)

### T-MCP-02: context_search with filters still uses Active status (existing behavior)
- Call context_search with topic filter
- Expected: filters.status = Some(Status::Active) as before
- Deprecated entries excluded by pre-filter, Flexible penalties have no effect

### T-MCP-03: No new MCP tool parameters (AC-15)
- Verify SearchParams struct has not gained new fields
- Diff check: no new tool registrations

### T-MCP-04: ServiceSearchParams includes retrieval_mode: Flexible
- Verify the constructed ServiceSearchParams has `retrieval_mode: RetrievalMode::Flexible`

### T-MCP-05: Explicit status=Deprecated internal path — full score (AC-14)
- Construct ServiceSearchParams with filters.status = Some(Status::Deprecated)
- Search with Flexible mode
- Expected: Deprecated entries returned at full score (no penalty)

### T-MCP-06: Explicit status=Deprecated disables injection (AC-14b)
- Same as T-MCP-05 but with supersession chain
- Expected: no Active successors injected

## Risk Coverage

| Risk | Scenarios | Tests |
|------|-----------|-------|
| R-07 (explicit status + injection) | Penalties bypassed, injection disabled | T-MCP-05, T-MCP-06 |
| R-12 (default Flexible change) | Unfiltered search now penalizes | T-MCP-01 |
| AC-13 | Unfiltered MCP applies Flexible | T-MCP-01 |
| AC-14 | Explicit status bypass | T-MCP-05 |
| AC-14b | Injection disabled | T-MCP-06 |
| AC-15 | No new parameters | T-MCP-03 |
