# Agent Report: vnc-022-agent-4-capability-extension

## Status: COMPLETE

## Files Modified

1. `crates/unimatrix-server/src/http/auth.rs` -- Added `Capability::SessionWrite` to `StaticTokenValidator::validate_sync` returned capabilities vec
2. `crates/unimatrix-server/src/http/auth/tests.rs` -- Updated 2 existing tests (caps:3->4, exact set assertion), added 2 new tests per component test plan
3. `crates/unimatrix-server/src/uds/hook.rs` -- Fixed missing `transcript_excerpt: None` field in CompactPayload constructor (build fix for wire.rs change from another agent)

## Tests

- **21 passed, 0 failed** (auth module: `http::auth::tests`)
- **6 passed, 0 failed** (UDS module: `uds::tests` -- verified no regression)
- **3439 passed, 0 failed** (full `unimatrix-server --lib` test suite)

### New Tests
- `test_static_token_validator_includes_session_write` -- validates SessionWrite present, full set match, count == 4
- `test_static_token_validator_capabilities_complete_set` -- validates all 4 capabilities present via `contains()`, confirms Admin is NOT granted

### Updated Tests
- `test_valid_token_inserts_resolved_identity_into_extensions` (T-STA-08) -- caps:3 -> caps:4
- `test_bearer_validator_trait_valid_token` (T-STA-10) -- exact vec assertion updated to include SessionWrite

## Issues

- Build required fixing `crates/unimatrix-server/src/uds/hook.rs` where the compact-payload-wire agent's `transcript_excerpt` field addition to `CompactPayload` left a construction site in `build_request()` without the new field. Minimal fix applied (`transcript_excerpt: None`).
- `listener.rs` and `uds/mod.rs` have uncommitted changes from the dispatch-request-refactor agent; these are not part of this commit.
- Full workspace binary test (`cargo test --workspace`) OOM-killed at link time; lib-only tests pass cleanly.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- surfaced entry #4453 (propagate capability checks via dedicated bool, not trust level) and #4692-#4693 (vnc-022 ADRs for ObserveContext and capability parameter). Applied: confirmed SessionWrite addition aligns with ADR-002 capability parameterization design.
- Stored: nothing novel to store -- this was a straightforward value addition to a vec literal with no runtime gotchas or non-obvious patterns.
