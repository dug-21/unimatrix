# Test Plan: Transport Rewiring

## Integration Tests

### TS-14: All existing tests pass (AC-15, AC-17, R-06)
- Action: `cargo test --package unimatrix-server`
- Verify: All existing tests pass without modification
- Verify: Test count >= 680

### TS-19: MCP context_search delegates to SearchService (AC-02)
- Verification method: Code inspection
- `grep -c 'services.search.search\|self.services.search' tools.rs` returns >= 1
- No inline embed+search+rank logic remains in context_search handler

### TS-20: UDS handle_context_search delegates to SearchService (AC-03)
- Verification method: Code inspection
- `grep -c 'services.search.search\|\.search\.search' uds_listener.rs` returns >= 1
- No inline embed+search+rank logic remains in handle_context_search

### TS-21: ConfidenceService replaces inline blocks (AC-04)
- Verification method: Code inspection
- `grep -c 'compute_confidence' tools.rs` returns 0 (excluding comments/docs)
- `grep -c 'confidence.recompute' tools.rs` returns >= 1

### TS-25: MCP produces identical responses (AC-13)
- Action: Run context_search, context_store, context_correct via MCP tool handlers
- Verify: Response format unchanged
- Verify: Same inputs produce same outputs
- Note: This is verified by existing tests passing (TS-14)

### TS-26: UDS produces identical responses (AC-14)
- Action: Run handle_context_search via UDS path
- Verify: HookResponse format unchanged
- Note: This is verified by existing tests passing (TS-14)

### TS-27: No new crates (AC-16)
- Verification: `grep '\[workspace\]' -A 50 Cargo.toml | grep 'members'` unchanged
- Or: `ls crates/` shows no new directories

## Notes

- Most transport-rewiring tests are verification-by-inspection or verified by existing test pass-through.
- The key gate is TS-14: if all existing tests pass, the rewiring preserved behavior.
- New service-level tests (in other components) add coverage for the new code paths.
