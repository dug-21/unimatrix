# Agent Report: vnc-021-agent-2-spec

## Task
Write SPECIFICATION.md for vnc-021 (HTTPS Transport + Static Bearer Token Auth).

## Artifacts Produced
- `/workspaces/unimatrix/product/features/vnc-021/specification/SPECIFICATION.md`

## Key Decisions

1. **FR-14 BearerValidator trait definition included as functional requirement** — SCOPE.md references the trait as an enterprise extension seam (W2-3 entry #4627). The spec defines it with a single `validate` method signature so the architect has a clear interface contract.

2. **StaticTokenAuth skips /health, not PathDispatcher** — The auth bypass for `/health` is modeled as an exempt-path list in `StaticTokenAuth` rather than routing `/health` before the auth layer. This keeps path dispatching in one place and makes auth bypass explicit and auditable.

3. **NFR-03 connection limiting before TLS handshake** — Per SR-08, HTTP connection limits are enforced at accept level (semaphore) before TLS handshake completes. This prevents slow-TLS resource exhaustion from starving background tasks.

4. **FR-17 identity injection via existing seam only** — Per constraint #8 and SR-09, the spec explicitly prohibits a parallel identity resolution path. All HTTP-authenticated identity flows through `build_context_with_external_identity`. Integration tests must exercise the full chain (HTTP request -> auth -> identity injection -> tool dispatch -> audit log).

5. **Constraint #12 adapter boundary for rmcp isolation** — Added as an explicit constraint (from SR-01/SR-02 recommendations). The architect must design a thin adapter between tower middleware and rmcp's `StreamableHttpService` so rmcp-specific bugs can be worked around without restructuring.

6. **Constraint #13 extension propagation validation** — Added as an explicit constraint (from SR-02). `ResolvedIdentity` must survive rmcp's internal request handling. This must be verified early in implementation, not discovered late.

## Open Questions

None. All open questions from SCOPE.md were resolved in the scope document itself (HTTP listener in stdio mode: No; graceful shutdown: Yes via LifecycleHandles; content port 0: supported; ProjectRouter home: src/http/router.rs).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 19 entries returned. Key entries used: #77 (rmcp 0.16 ADR), #1913 (UnimatrixServer Clone sharing), #4627 (W2-3 StaticTokenAuth feature), #1898 (UDS socket separation pattern). All informed domain model definitions and constraint formulation.
