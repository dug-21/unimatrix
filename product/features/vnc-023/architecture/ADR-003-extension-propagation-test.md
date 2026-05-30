## ADR-003: Extension Propagation Integration Test for ResolvedIdentity

### Context

SR-08 identifies extension propagation as the highest-integration-risk item in the rmcp migration. The `StaticTokenAuth` middleware inserts a `ResolvedIdentity` into HTTP request extensions. rmcp's internal processing must preserve this identity so that tool handlers can access it via `RequestContext.extensions.get::<http::request::Parts>()`.

This was validated for rmcp 0.16.0 during vnc-021 (R-01 spike). The McpAdapter doc comment (router.rs line 366) confirms: "extensions DO propagate through rmcp (the `Parts` struct including `extensions` is injected into MCP message extensions)."

rmcp 1.7.0 may have changed internal request processing. The `Parts` extraction path could be affected by any refactoring of the HTTP-to-MCP bridge inside `StreamableHttpService`. Without explicit validation, a silent regression would cause all tool calls to lose identity context -- capabilities would default to anonymous, audit attribution would be blank.

Alternatives considered:
1. **Assume compatibility**: The R-01 spike confirmed it works in 0.16. But rmcp internals are not API-stable -- this is exactly the kind of change that breaks silently.
2. **Unit test with mock**: Cannot test rmcp's internal request processing without running the full HTTP stack.
3. **Integration test through the full HTTP/MCP stack**: Validates the actual code path. Catches silent regressions.

### Decision

Validate extension propagation as part of the existing test suite execution (`cargo test --workspace`). The existing tests that exercise tool calls through the HTTP transport path already implicitly validate this -- if `ResolvedIdentity` stops propagating, any test that calls a capability-gated tool through HTTP will fail with a permission error rather than succeeding.

If no existing test covers this path end-to-end (HTTP request with auth -> tool call that reads identity), add one integration test that:
1. Constructs a `UnimatrixServer` with known configuration
2. Sends an HTTP request through `McpAdapter` with `ResolvedIdentity` in extensions
3. Invokes a tool that requires identity (any capability-gated tool)
4. Asserts the tool succeeds (not permission-denied)

The test should be in the existing `server.rs` test module or `http/router.rs` test module, extending existing test infrastructure rather than creating new scaffolding.

### Consequences

- **Easier**: Catches silent regressions in extension propagation that would otherwise manifest as production auth failures.
- **Easier**: Uses existing test infrastructure (duplex transport, `serve_client`, test helpers).
- **Harder**: If rmcp 1.7 changes extension propagation behavior, the test failure will point directly at the issue, but the fix may require understanding rmcp internals to find a new extraction path.
