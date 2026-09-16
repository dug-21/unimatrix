## ADR-008: AC-07 forward-compat — keep the external_identity seam and per-call MCP channel viable, build no enforcement

### Context
AC-07 is a forward-compat **constraint**, not a deliverable: the E1 trusted-identity work (harness-
attested agent identity → capability enforcement) is demand-pulled under trusted-identity #5734 and is
explicitly out of scope. The delivery must not foreclose it. The consumption seam already exists but is
dormant: `build_context_with_external_identity(..., external_identity: Option<&ResolvedIdentity>)`
(`mcp/server.rs`, ADR #4357) is always `None` today; PL-4 (#5705) `resolve_or_enroll` is fail-open.
SR-12 warns against making the seam "viable" but ceremonial — an N=1 always-`None` test gives false
confidence (#4974).

### Decision
Keep the seam viable by **not touching it and not poisoning it**:

1. **Do not fold the validated `session.agent` into MCP tool args** (ADR-007). The trusted identity
   stays on the observe channel; the MCP `context_*` args channel is left as-is, so a future E1
   consumer can activate `external_identity` on a server-trusted channel without first un-picking a
   spoofable-args shortcut.
2. **Keep the per-call MCP delivery-channel option open**: the installer (ADR-006) leaves
   `mcp.unimatrix` retrieval untouched and additive. This preserves both future channels identified in
   ass-106 §E1 — (i) a per-call MCP request-field stamp, and (ii) an MCP-proxy shim (`mcp-bridge.js`
   pattern) that stamps a transport-attested identity — neither is precluded by anything vnc-049 ships.
3. **Build no enforcement**: no PL-4 fail-closed flip, no `external_identity` activation, no
   `require_cap` rewiring. `build_context_with_external_identity` stays `None`.
4. **No ceremonial N=1 test** (SR-12). vnc-049 adds no test that asserts the seam "works" with a single
   always-`None` case. The seam's viability is documented (this ADR + ARCHITECTURE Integration Surface),
   not falsely evidenced. The plugin's in-process hold of the validated `session.agent` is recorded as
   the concrete future source E1 will consume.

### Consequences
Easier: E1 remains a clean future cycle with a real source (validated `session.agent`) and a real
dormant consumer (`external_identity`); no throwaway scaffolding to unwind. Harder: nothing in vnc-049
proves the seam end-to-end (by design — that is E1's job); the open question of whether OpenCode child
sessions share the parent MCP client (gates channel (i) vs the proxy (ii)) stays open, routed to the
E1 cycle. Cross-references ADR-007 (keeps identity off the spoofable channel).
