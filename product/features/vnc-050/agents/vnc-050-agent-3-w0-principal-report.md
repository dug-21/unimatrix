# vnc-050-agent-3-w0-principal — Wave 0 Report

## Report
Files created: src/http/principal.rs, src/http/token_table.rs, src/http/token_table/tests.rs, src/http/auth/scoped.rs, src/mcp/cap_ceiling.rs, src/mcp/cap_ceiling_http_tests.rs (crate unimatrix-server).
Files modified: src/http/auth.rs (LegacyDisabled, validate_principal default + PrincipalFuture alias, unauthorized_with, with_validator, pub with_token_table), src/http/mod.rs, src/mcp/mod.rs, src/mcp/context.rs (ToolContext.cap_ceiling), src/server.rs (+1 line: cap_ceiling: ceiling_from(..)), src/test_support.rs (literal), src/main.rs (auth layer -> with_token_table).

R-02 step 0: `principal_visible_in_tool_request_context_over_http` PASS with native rmcp propagation. No ADR-003 fallback applied.

Tests: workspace rc=0; unimatrix-server lib 4634 passed / 0 failed. New: 31 unit/integration tests (principal 6, token_table 14, scoped 7, cap_ceiling 6 + step 0).

Deviations / notes:
- main.rs uses new `pub fn StaticTokenAuthLayer::with_token_table(legacy, data_dir)` (binary crate cannot name pub(crate) validator types). `with_legacy_enabled` not yet wired — C5 (W1).
- ToolContext.cap_ceiling + build_context line added in W0 (needed for PROBE); require_cap use is W1.
- `reject_reserved_agent_id` deferred to W1 (server.rs wiring).
- Not-yet-consumed items carry `#[allow(dead_code)]` with rationale naming the W1/W2 consumer.
- `ceiling_from` NoParts branch not unit-tested (RequestContext not constructible without a Peer); Parts branches tested via `principal_ceiling` and step 0.
- Pre-existing clippy error in infra/transcript_hold.rs:268 (not touched).
- File-mode / writer tests (mint/revoke/lock/0600) are W1.

## Knowledge Stewardship
- Queried: Unimatrix MCP tools not invoked this session; used ADR-001/003 files directly.
- Stored: nothing novel to store -- rmcp Parts propagation already recorded (R-01 note, router.rs); step 0 re-confirmed it.
