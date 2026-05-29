# Scope Risk Assessment: vnc-021

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | rmcp 0.16.0 pinned — `StreamableHttpService` is new/lightly-adopted API surface; undocumented behavioral constraints or session lifecycle bugs may emerge under concurrent load | High | Medium | Architect should isolate rmcp HTTP surface behind a thin adapter so session bugs can be worked around without restructuring the listener |
| SR-02 | rmcp `transport-streamable-http-server-session` feature interaction with tower middleware is unproven in this codebase — request extension propagation (`Parts`, `ResolvedIdentity`) may not survive rmcp's internal request handling | High | Medium | Validate extension propagation in a spike test before committing to the middleware design; ref Unimatrix #4367 (rmcp 0.16 traps) |
| SR-03 | Claude Code bug #28293 (headers not forwarded on tool call POSTs) — if unfixed upstream, the `claude mcp add -H` workaround is the only auth path; breakage or deprecation of `-H` flag would block Claude Code clients entirely | Medium | Medium | Document workaround prominently; architect should not assume native `.mcp.json` header support |
| SR-04 | `tokio-rustls` + `rustls-pemfile` are new direct dependencies — while transitively present, promoting to direct deps may surface version conflicts with existing `reqwest`/`hyper-rustls` chain | Low | Low | Pin versions matching the existing lockfile transitives; ref Unimatrix #4661 (dep landscape) |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | ProjectRouter "single-project default" seam adds structural complexity that is untestable until W2-6 — dead code risk, maintenance burden without validation | Medium | Medium | Spec should define minimal integration tests proving the seam is exercised (request flows through ProjectRouter even in single-project mode) |
| SR-06 | `/observe` 501 stub is a structural seam for W2-7 — scope boundary between "register the route" and "implement the handler" may blur during implementation, pulling W2-7 work forward | Low | Medium | Spec should explicitly constrain `/observe` to a static 501 response with zero handler logic |
| SR-07 | "All three clients use curl-based shell hooks" — scope assumes curl availability and POSIX shell on all client platforms; Windows/WSL clients are implicitly excluded | Medium | Low | Architect should note platform constraint; spec should state POSIX-only for hook setup |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | HTTP listener shares the tokio runtime with UDS listener, background ticks, NLI inference, and write queue — resource contention under concurrent HTTP + UDS load could starve background tasks | High | Medium | Architect should consider connection limits (AC-22 helps) and whether the HTTP acceptor needs its own task budget or spawn constraints |
| SR-09 | `build_context_with_external_identity` seam (server.rs:388) was designed but never exercised in production — first real activation via HTTP auth may expose assumptions in identity resolution that don't hold for bearer-token callers | Medium | Medium | Spec should require integration tests that exercise the full path: HTTP request -> auth middleware -> identity injection -> tool dispatch -> audit log verification |
| SR-10 | Existing `health.rs` is a CLI subcommand probing UDS — adding an HTTP health endpoint creates two health surfaces with potentially divergent semantics; confusion about which to use for monitoring | Low | Low | Architect should clarify naming: CLI `health` = UDS probe, HTTP `/health` = version endpoint; distinct purposes |

## Assumptions

1. **rmcp 0.16.0 HTTP features are stable** (SCOPE.md lines 54-58) — if the `transport-streamable-http-server` feature has breaking bugs, the pinned version constraint prevents upgrading. Mitigation depends on rmcp maintainer responsiveness.
2. **All dependencies are already transitive** (SCOPE.md lines 62-76) — assumes lockfile state is current. A `cargo update` or dependency bump in another crate could invalidate the "no new downloads" claim.
3. **`subtle` constant-time comparison is sufficient for token auth** (SCOPE.md line 74) — assumes the token comparison hot path has no timing side-channels introduced by surrounding code (e.g., early-return on header parsing before reaching `ConstantTimeEq`).
4. **Single TCP listener is sufficient for personal cloud** (SCOPE.md line 129) — if users need separate TLS configs for MCP vs health monitoring, the single-listener model breaks. Accepted risk for personal cloud tier.

## Design Recommendations

1. **(SR-01, SR-02)** Architect should design the HTTP listener with a clear adapter boundary between tower middleware and rmcp's `StreamableHttpService`, so rmcp-specific issues can be isolated without restructuring auth or routing.
2. **(SR-08)** Enforce HTTP connection limits early in the tower stack (before TLS handshake completes) to prevent resource exhaustion from slow-TLS attacks or connection floods.
3. **(SR-09)** Spec writer should make the `build_context_with_external_identity` integration path a first-class test target — not a byproduct of transport tests.
4. **(SR-03)** Architect should not design around the assumption that Claude Code's `.mcp.json` will support headers natively. The `-H` workaround should be the primary documented path.
