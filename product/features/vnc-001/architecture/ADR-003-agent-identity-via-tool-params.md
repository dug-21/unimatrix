## ADR-003: Agent Identity via Tool Parameters

### Context

Every tool call needs an agent identity for capability checks and audit logging. Options for conveying identity:

1. **Tool parameter**: `agent_id: Option<String>` on every tool's params struct. Self-reported, unverified.
2. **MCP `_meta` field**: Future MCP spec addition for per-request metadata. Not yet in rmcp 0.16.
3. **Client info from initialize**: `clientInfo` in the MCP handshake identifies the MCP client (e.g., "claude-code"), not the specific agent making the call.
4. **Bearer token**: OAuth 2.1 on HTTPS transport. Requires HTTP transport we don't have.

For stdio transport, the MCP connection is per-process. Multiple agents running in the same Claude Code session share one MCP connection. There is no protocol-level mechanism to distinguish which agent is making a given tool call.

### Decision

Use `agent_id: Option<String>` as an optional parameter on every tool. Default to `"anonymous"` when absent.

The internal pipeline is transport-agnostic:
```
tool param agent_id -> ResolvedIdentity -> capability check -> audit log
```

When MCP adds `_meta.agent_id` support or when HTTPS with OAuth 2.1 is added, only the extraction step changes. The ResolvedIdentity struct and everything downstream remain identical.

This is self-reported identity on stdio. The threat model acknowledges this: stdio transport inherits OS-level process isolation. The agent running the tool call is already trusted to the extent that the user trusts the LLM client. The registry exists to support capability differentiation (not cryptographic authentication) and to enable audit trails.

### Consequences

- **Easier:** Works today on stdio. No protocol extensions needed. Agents that set `agent_id` get differentiated treatment; agents that don't get safe defaults.
- **Easier:** Transport-agnostic internal pipeline. Adding verified identity (HTTPS + OAuth) later requires changing one extraction function, not the whole pipeline.
- **Harder:** Self-reported identity is spoofable on stdio. A malicious agent can claim to be `"human"`. Mitigated by: (a) stdio is single-machine, (b) the trust model is advisory not cryptographic for stdio, (c) audit log records all claims for forensic analysis.
- **Not affected:** Capability checks and audit logging work identically regardless of identity source.
