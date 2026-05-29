# Agent Report: vnc-021-agent-1-pseudocode

## Task
Produce per-component pseudocode files for vnc-021 HTTPS transport with static bearer token authentication.

## Files Produced

| File | Component | Lines |
|------|-----------|-------|
| `pseudocode/OVERVIEW.md` | Cross-cutting | ~90 |
| `pseudocode/token-manager.md` | C4: Token Manager | ~100 |
| `pseudocode/config-extensions.md` | C7: Config Extensions | ~120 |
| `pseudocode/static-token-auth.md` | C2: StaticTokenAuth | ~180 |
| `pseudocode/tls-config.md` | C5: TLS Configuration | ~90 |
| `pseudocode/health-handler.md` | C6: Health Handler | ~50 |
| `pseudocode/path-router.md` | C3: Path Router | ~200 |
| `pseudocode/http-listener.md` | C1: HTTP Listener | ~170 |
| `pseudocode/lifecycle-integration.md` | C8: Lifecycle Integration | ~170 |

## Wave Grouping

| Wave | Components | Rationale |
|------|-----------|-----------|
| 0 | R-01 spike test (in path-router) | Must validate extension propagation before building auth chain |
| 1 | token-manager, config-extensions | No dependencies; foundation for later waves |
| 2 | static-token-auth, tls-config, health-handler | Depend on Wave 1 outputs |
| 3 | path-router | Depends on spike result + auth + health |
| 4 | http-listener | Depends on all middleware components |
| 5 | lifecycle-integration | Wires everything into main.rs |

## Open Questions

1. **schema_version source**: The health handler needs the current schema migration version. Need to verify whether `unimatrix_store` exports a `SCHEMA_VERSION` constant or if it requires a runtime query. If runtime, the health handler needs a startup-time snapshot.

2. **credential_type wiring gap**: `build_context_with_external_identity` in `server.rs` builds `AuditSource::Mcp` but does not currently set a `credential_type` field. The implementation agent must trace the audit emission path (`AuditContext` -> `AuditEvent` -> `audit_log` INSERT) to find where `credential_type` is populated and add `"static_token"` for external identity paths. This is flagged in both `static-token-auth.md` and `lifecycle-integration.md`.

3. **rmcp StreamableHttpService constructor**: The exact constructor signature for `StreamableHttpService` in rmcp 0.16 needs verification. It may require `StreamableHttpServerConfig` for session management parameters. The implementation agent should check rmcp 0.16 docs/source.

4. **hyper Body type alignment**: rmcp's `StreamableHttpService` implements `Service<Request<RequestBody>>` where `RequestBody` may be rmcp's own body type, not `hyper::Body`. Type conversion may be needed in the `McpAdapter`.

5. **CallerId::HttpBearer construction site**: The pseudocode shows `CallerId::Agent(identity.agent_id.clone())` in `build_context_with_external_identity`. For HTTP callers, this should be `CallerId::HttpBearer(...)` instead. The implementation agent must determine where the caller_id construction happens -- it may need to be parameterized based on whether `external_identity` is `Some` or `None`.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` -- found ADR entries #4665-4670 for vnc-021 decisions, #319 (CallerId pattern), #4661 (rmcp dep landscape), #4362 (ResolvedIdentity placement), #4368 (RequestContext as named parameter)
- Deviations from established patterns: none -- CallerId extension follows the existing transport-typed enum pattern (#319)
