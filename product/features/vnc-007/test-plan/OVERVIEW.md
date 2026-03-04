# Test Plan Overview: vnc-007 Briefing Unification

## Test Strategy

Testing is organized around the Risk-Based Test Strategy (RISK-TEST-STRATEGY.md). Each risk maps to specific test scenarios distributed across component test plans.

### Risk-to-Test Mapping

| Risk ID | Risk | Priority | Test Location | Scenarios |
|---------|------|----------|--------------|-----------|
| R-01 | CompactPayload behavioral regression | High | uds-rewiring.md | Snapshot comparison, budget boundary, edge cases |
| R-02 | MCP briefing semantic search regression | Med | mcp-rewiring.md | Entry ID comparison, co-access anchors, feature boost |
| R-03 | Feature flag + rmcp macro compatibility | Med | feature-flag.md | Build tests, tool count verification |
| R-04 | Injection history path latency (SearchService isolation) | High | briefing-service.md | Panicking SearchService test |
| R-05 | Quarantine exclusion in injection history | Med | briefing-service.md | Quarantined entry exclusion test |
| R-06 | Budget overflow with mixed sources | Low | briefing-service.md | Small budget boundary test |
| R-07 | Duties removal test breakage | Med | duties-removal.md | Negative assertion tests |
| R-08 | EmbedNotReady fallback | Low | briefing-service.md | Graceful degradation test |
| R-09 | CompactPayload format text divergence | Med | uds-rewiring.md | Section header preservation tests |
| R-10 | dispatch_unknown_returns_error test | Low | uds-rewiring.md | Test update |

## Test Categories

### Unit Tests (Rust, cargo test)

| Component | Test File | Est. Tests | Runs In |
|-----------|----------|-----------|---------|
| BriefingService | services/briefing.rs (#[cfg(test)] mod tests) | 12-15 | cargo test -p unimatrix-server |
| Duties Removal | response.rs (updated existing tests) | 4 updated | cargo test -p unimatrix-server |
| CompactPayload | uds_listener.rs (updated existing tests) | 3-5 updated | cargo test -p unimatrix-server |
| MCP Rewiring | tools.rs (updated existing tests) | 1-2 updated | cargo test -p unimatrix-server |
| Feature Flag | tools.rs / Cargo.toml | 0 (build tests) | cargo build --no-default-features |

### Integration Tests (Python, pytest)

| Suite | Applicable | New Tests Needed |
|-------|-----------|-----------------|
| test_tools.py | Yes — context_briefing smoke test | Verify briefing returns no duties field |
| test_security.py | Marginally — S3 validation | No new (BriefingService reuses gateway) |
| test_lifecycle.py | No | No |
| test_protocol.py | No | No |

## Integration Harness Plan

### Existing Suites That Apply

1. **test_tools.py**: Contains smoke tests for all MCP tools. The context_briefing test should be verified to still pass after duties removal (output format changes).

2. **test_security.py**: S3 input validation is handled by SecurityGateway which already has integration tests. No new tests needed.

### New Integration Tests Needed

1. **Briefing no-duties test** (in test_tools.py): Verify that context_briefing output for all formats (summary, markdown, json) contains no "duties" or "Duties" strings.

2. **UDS compact payload smoke test** (in test_protocol.py or new): Verify that CompactPayload via UDS still returns BriefingContent after rewiring.

### Rust Integration Tests

The existing Rust integration tests in uds_listener.rs (#[tokio::test]) for dispatch_request cover:
- `dispatch_compact_payload_*` — must still pass after rewiring
- `dispatch_briefing_returns_error` — must be UPDATED to expect BriefingContent instead of Error

## Test Count Baseline

Before vnc-007, verify baseline with:
```bash
cargo test -p unimatrix-server -- --list 2>&1 | grep -c "test$"
```

After vnc-007: count must be >= baseline (AC-33). Net test additions expected from BriefingService unit tests (~12-15 new) minus duties test removals (~0, tests updated not removed).

## Edge Case Coverage

| Edge Case | Component Test Plan |
|-----------|-------------------|
| Empty knowledge base | briefing-service.md |
| All injection entries quarantined | briefing-service.md |
| max_tokens=500 (minimum) | briefing-service.md |
| role=None but include_conventions=true | briefing-service.md |
| task=None but include_semantic=true | briefing-service.md |
| Duplicate entry_ids in injection history | briefing-service.md |
| Very large injection history (100+ entries) | briefing-service.md |
| Feature tag matches no entries | briefing-service.md |
