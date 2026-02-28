# Test Plan: session-timeout

## Unit Tests

The session timeout wraps `tokio::time::timeout` around the rmcp session future. The actual timeout behavior is best verified via integration testing because it requires a running MCP session.

### test_session_idle_timeout_value

- Location: main.rs (or a dedicated test if main.rs tests exist)
- Arrange: Read SESSION_IDLE_TIMEOUT constant
- Assert: Value equals Duration::from_secs(30 * 60) (1800 seconds)
- Risks: None (sanity check)
- Note: Since SESSION_IDLE_TIMEOUT is a const in the binary crate (main.rs), it cannot be directly imported in lib tests. This verification is done via code review (Gate 3b) and the integration smoke test.

## Integration Verification (Stage 3c)

The session timeout behavior will be verified by:
1. Running the integration smoke tests (which start and stop the server)
2. Verifying the server starts and serves tool calls normally (timeout does NOT fire during active use)
3. The actual 30-minute timeout cannot be tested in CI (too long); correctness relies on `tokio::time::timeout` being well-tested

## Documented Assumptions

- `tokio::time::timeout` correctly cancels the inner future when elapsed (well-tested in tokio)
- rmcp's session future only completes when the transport closes (documented in rmcp)
- Active tool calls keep the session future alive (rmcp internal behavior)
- The timeout only fires when no client is connected and the session is blocking

## AC-05 Verification

AC-05 (zombie server detection) relies on the timeout triggering graceful_shutdown. This is verified by:
1. Code review: timeout wrapper calls graceful_shutdown on expiry
2. Existing graceful_shutdown tests verify the shutdown sequence
3. Integration smoke tests verify no regression in normal operation
