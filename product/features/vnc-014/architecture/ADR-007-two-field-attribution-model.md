## ADR-007: Two-Field Attribution Model — agent_id (Spoofable) vs agent_attribution (Transport-Attested)

### Context

SR-08 identifies a semantic ambiguity: after vnc-014, `AuditEvent` will have both `agent_id`
(agent-declared, tool parameter) and `agent_attribution` (transport-attested, from `clientInfo.name`).
Downstream consumers (audit queries, compliance tooling in W2-3) must not conflate these.

The distinction is architectural, not incidental:

- `agent_id` — declared by the calling agent in the tool parameter. This is a routing and
  context identity. It can be anything the agent provides. It is the value used to look up
  capabilities and apply rate limiting. It is intrinsically spoofable.

- `agent_attribution` — set by the server from the MCP transport layer at `initialize` time.
  The calling agent cannot influence this value after the `initialize` handshake. In the OSS
  tier, it is `clientInfo.name` (the MCP client implementation name). In the enterprise tier
  (W2-3+), it will be the JWT `sub` claim. It is the compliance evidence field.

This distinction maps to how audit fields are used in practice:
- Identity routing, rate limiting, per-agent behavior: use `agent_id`
- Compliance reporting, non-repudiation, client identification: use `agent_attribution`

The coexistence of both fields in `AuditEvent` is intentional and correct. Forcing them into
a single field would lose the semantic distinction (spoofable vs. transport-attested).

### Decision

Document the two-field model as a first-class architectural principle for `AuditEvent`:

| Field | Source | Spoofable | Purpose |
|-------|--------|-----------|---------|
| `agent_id` | Tool parameter (agent-declared) | Yes | Routing, rate limiting, per-agent behavior |
| `agent_attribution` | `clientInfo.name` via MCP `initialize` (transport-attested) | No | Compliance, non-repudiation, client identification |

Code comments on `AuditEvent.agent_attribution` in `schema.rs` and on the field population
in `build_context_with_external_identity()` must explain this distinction explicitly.

No merging of the two fields. No deprecation of `agent_id`. Both fields are required.

W2-3 will upgrade `agent_attribution` to carry JWT `sub` when bearer auth is active — this is
handled by the `external_identity: Option<&ResolvedIdentity>` Seam 2 parameter (ADR-003).
When `external_identity` is `Some`, `agent_attribution` is populated from the JWT identity
rather than `clientInfo.name`. This upgrade path is forward-compatible: the field name does not
change, only its source.

### Consequences

Easier:
- Compliance tooling has a dedicated, non-spoofable column to query
- W2-3 can upgrade attribution to JWT without schema changes
- Audit semantics are unambiguous for downstream consumers

Harder:
- Documentation and code comments must be maintained to keep this distinction visible as the
  codebase evolves
- Any future tool that constructs `AuditEvent` must actively decide what to put in both fields
  (no implicit default covers the distinction)
