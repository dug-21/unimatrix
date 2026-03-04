# Test Plan: MCP Rewiring (tools.rs)

## Test Infrastructure

MCP tool handler tests in tools.rs use the full server infrastructure. These are integration-level tests that exercise the full pipeline from tool invocation through to response formatting.

## Test Scenarios

### T-MR-01: context_briefing delegates to BriefingService (AC-13)
```
Verify: After rewiring, context_briefing handler calls services.briefing.assemble()
Method: grep -n "briefing.assemble\|services.briefing" tools.rs
Assert: delegation call present
```

### T-MR-02: context_briefing produces output without duties (AC-10, AC-14)
```
Setup: Store convention entries and knowledge entries in test store
Call: context_briefing(role="architect", task="design module")
Assert: Output does NOT contain "Duties" or "duties"
Assert: Output contains "Conventions" section
Assert: Output contains convention entries
```

### T-MR-03: context_briefing retains transport concerns (AC-15)
```
Method: Code inspection
Assert: Identity resolution (resolve_agent) still present
Assert: Capability check (require_capability Read) still present
Assert: Format param parsing still present
Assert: Usage recording (record_usage_for_entries) still present
Assert: Audit event logging still present
```

### T-MR-04: Tool description updated (no duties mention)
```
Method: grep -n "duties" tools.rs in context_briefing description string
Assert: No mention of "duties" in tool description
```

### T-MR-05: Existing MCP briefing tests pass
```
Run: cargo test -p unimatrix-server context_briefing
Assert: All existing tests pass (with updated assertions for no duties)
```

## Updated Existing Tests

The following existing tests in tools.rs may reference duties and need update:
- Any test that constructs a Briefing struct with duties field
- Any test that asserts on duties count in summary output

Scan for: `grep -n "duties\|Duties" crates/unimatrix-server/src/tools.rs`

## Risk Coverage

| Risk | Test(s) | Status |
|------|---------|--------|
| R-02 (MCP search regression) | T-MR-02 (behavioral check) | Partial — full semantic search comparison requires populated vector index |
| R-07 (Duties removal breakage) | T-MR-02, T-MR-04 | Covered |
