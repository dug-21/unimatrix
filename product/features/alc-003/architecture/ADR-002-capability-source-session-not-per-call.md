## ADR-002: Session Capabilities Cached at Startup; No Per-Call Registry Lookup

### Context

The current `build_context()` / `require_cap()` flow performs a database lookup on
every tool call to resolve the calling agent's capabilities. With `PERMISSIVE_AUTO_ENROLL`
in place, unknown agents are enrolled on first call and their record is written to the
registry. This creates two problems:

1. **Capability authority is per-call and LLM-controlled**: the LLM can send any
   `agent_id` and receive the capabilities of that enrolled agent, or trigger a new
   enrollment. An LLM sending `agent_id: "human"` receives Admin capabilities (after
   the protected-agent check was added in alc-002, this is blocked, but the principle
   remains — capability resolution was tied to the per-call label).

2. **Registry lookups on the hot path**: every tool call does a `spawn_blocking`
   database read. Post-nxs-011 (sqlx migration) this is async, but it is still a
   database round-trip per call that is avoidable.

SCOPE.md Goal §5 is explicit: "Capability resolution uses the authenticated session
exclusively. Per-call `agent_id` does not affect capabilities."

Two designs were considered:

**Option A — Cache per-call identity in a session-scoped hashmap:**
Maintain a `HashMap<String, Vec<Capability>>` in `UnimatrixServer` keyed by `agent_id`.
First call for a given `agent_id` does a registry lookup; subsequent calls hit the cache.
This reduces DB calls but preserves per-call capability resolution.

Problems: cache invalidation (if an admin changes an agent's capabilities mid-session,
the cache is stale); the LLM still controls capability resolution by choosing `agent_id`;
swarm subagents continue to get their own capability records even though the SCOPE.md
explicitly states they should not.

**Option B — Session capabilities cached once at startup (chosen):**
`UnimatrixServer` holds a `SessionAgent { agent_id, trust_level, capabilities }` set
at construction time. Every tool call uses `session_agent.capabilities` for capability
checks. Per-call `agent_id` is used only for audit attribution.

Problems addressed: no per-call DB lookups; LLM cannot influence capability resolution
by changing `agent_id`; swarm subagents appear in the audit log without polluting the
registry; the capability source is deterministic and operator-controlled.

### Decision

Session capabilities are cached in `UnimatrixServer` at startup as `session_agent: SessionAgent`.
`require_cap()` checks `self.session_agent.capabilities.contains(cap)` — no registry,
no database, no `spawn_blocking`. `agent_id` parameter is removed from `require_cap()`
because capability resolution no longer depends on which agent is calling.

`build_context()` sources the `trust_level` for `AuditContext` from
`self.session_agent.trust_level`. Audit attribution (`agent_id` in the audit log) is
sourced from `params.agent_id` if non-empty after trimming, else `self.session_agent.agent_id`.
These two paths are completely independent — no registry lookup occurs for either.

The `ResolvedIdentity` type and `resolve_agent()` method in `server.rs` are deleted.
`identity::resolve_identity()` in `mcp/identity.rs` is deleted. `extract_agent_id()` is
retained but its return value is now an audit label, not a registry key.

This decision supersedes ADR-003 (entry #79 in Unimatrix: "Agent Identity via Tool
Parameters") for the capability-resolution aspect. ADR-003's attribution model (per-call
`agent_id` for audit) is preserved — only the capability-resolution aspect is superseded.

### Consequences

**Easier:**
- No per-call DB reads for capability checks — `require_cap()` is now O(n) over a
  small in-memory `Vec<Capability>`; effectively O(1) for the capability count
- LLM cannot self-elevate by sending a different `agent_id` on a tool call
- Swarm subagents (researcher, architect, etc.) appear in audit log with their
  meaningful role labels without being auto-enrolled into the registry
- `require_cap()` becomes synchronous (no `spawn_blocking`) and simpler to test

**Harder:**
- All 12 tool handler call sites for `require_cap()` must be updated to remove the
  `agent_id` argument — mechanical but must be done atomically with the signature change
- Dynamic capability changes (admin revoking session agent capabilities) require a
  daemon restart to take effect — acceptable for W0-2 STDIO/daemon; W2-2 HTTP will
  revisit this when per-connection identity is introduced
- Integration tests that relied on per-call identity for capability differentiation must
  be rewritten to use session-level setup
