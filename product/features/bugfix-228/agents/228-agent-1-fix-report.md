# Agent Report: 228-agent-1-fix

## Summary

Implemented permissive auto-enroll for unknown agents (#228). Added `PERMISSIVE_AUTO_ENROLL` const to control whether unknown agents get `[Read, Write, Search]` (dev/true) or `[Read, Search]` (production/false). Updated all affected unit and integration tests.

## Files Modified

1. `crates/unimatrix-server/src/infra/registry.rs` -- added const, updated `resolve_or_enroll()` logic
2. `crates/unimatrix-server/src/mcp/identity.rs` -- updated `test_resolve_unknown_agent` assertion
3. `product/test/infra-001/suites/test_security.py` -- updated S-21, S-22, S-23 to expect success

## New Tests

- `test_permissive_auto_enroll_grants_read_write_search` -- verifies unknown agent gets Read+Write+Search but not Admin
- `test_enrolled_agent_has_write_when_permissive` -- replaces `test_enrolled_agent_lacks_write`, verifies Write is granted

## Test Results

- 1221 passed, 0 failed (unimatrix-server crate)
- No new clippy warnings in modified files

## Issues

None.

## Knowledge Stewardship

- Queried: Received from coordinator -- #265 (pre-flight enrollment), #217 (alc-002 outcome), #103 (ADR-007 trust gating)
- Stored: nothing novel to store -- targeted const addition with straightforward branching, no new pattern discovered
