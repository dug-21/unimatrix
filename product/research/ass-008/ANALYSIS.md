# ASS-008: LLM-Resistant Agent Authentication — Synthesis & Recommendations

**Date**: 2026-02-24
**Type**: Research Spike — Synthesis
**Status**: Complete
**Companion files**:
- `RESEARCH-cryptographic-identity.md` — OCAP, process-level binding, warrants, TEEs
- `RESEARCH-adversarial-attacks.md` — 31 attack vectors, threat model, risk matrix
- `RESEARCH-novel-architectures.md` — 3 prototype architectures, phased deployment
- `RESEARCH-existing-implementations.md` — libraries, standards, readiness assessment

---

## Executive Summary

**The core problem**: Unimatrix's agent identity is a self-reported string. Any LLM can claim to be `"system"` or `"human"` and gain full capabilities. This is not a bug — it's an architectural gap. Self-reported identity is fundamentally incompatible with LLM agents because LLMs are text generators, and identity claims are text.

**The core finding**: Authentication must operate **outside the LLM's control surface**. The LLM must neither possess nor be able to fabricate the credential. Identity must be an emergent property of infrastructure (process, connection, cryptographic material) — not a claim in the LLM's output stream.

**The evidence**:
- 84.3% average attack success rate against LLM agents (Agent Security Bench, ICLR 2025)
- 84.6% success rate for inter-agent trust exploitation vs. 46.2% for direct prompt injection
- 100% of LLM prompt injection defenses were bypassed (SaTML 2025 — all 72 defenses broken)
- 90% attack success rate for PoisonedRAG with just 5 entries in millions
- MCP stdio transport provides **zero** authentication mechanism (by design)

**The solution**: A phased "Citadel" architecture that progressively removes the LLM from the identity-proving loop, starting with opaque server-resolved tokens (immediate), adding cryptographic capability warrants (medium-term), and process-level identity binding (long-term).

---

## 1. Why Self-Reported Identity Cannot Work

### The Fundamental Problem

In traditional service-to-service auth, identity claims work because services are deterministic programs — a service configured to send `agent_id: "payment-service"` will reliably do so. LLMs break this assumption:

1. **Generative capability**: LLMs produce arbitrary text, including identity claims
2. **Context window as attack surface**: Anything in the LLM's context influences its outputs. If identity is text, it's attackable.
3. **Tool call manipulation**: LLMs generate tool call parameters. If identity is a parameter, the LLM chooses the value.

### Current Vulnerability

```rust
// identity.rs — the LLM controls this value entirely
pub fn extract_agent_id(agent_id: &Option<String>) -> String {
    match agent_id {
        Some(id) => id.trim().to_string(),  // accepts ANY string
        None => "anonymous".to_string(),
    }
}
```

Nothing prevents an LLM from passing `agent_id: "system"` and gaining System trust with all capabilities.

### The "Agents Rule of Two" (Meta AI, Oct 2025)

An agent must satisfy **no more than two** of:
1. Can process untrustworthy inputs
2. Has access to sensitive systems or private data
3. Can change state or communicate externally

Unimatrix agents with Write capability satisfy all three — the "lethal trifecta." Authentication architecture must constrain this.

---

## 2. Attack Landscape (Key Findings)

From 31 documented attack vectors across 7 categories:

### Most Dangerous Attacks for Unimatrix

| Attack | Success Rate | Why It Matters |
|--------|-------------|----------------|
| **Direct identity spoofing** | Trivial (100%) | Any LLM can claim `agent_id: "system"` |
| **Indirect injection via retrieved knowledge** | 90% (PoisonedRAG) | Poisoned entries instruct agents to escalate |
| **Inter-agent trust exploitation** | 84.6% | Agents that resist human jailbreaks "immediately failed when the same request came from a peer agent" |
| **Token exfiltration via side channels** | Demonstrated | LLMs can encode credentials in tags, topics, metadata fields |
| **Unicode homoglyph evasion** | Routinely bypasses regex | Our content scanner's ~35 regex patterns are vulnerable |

### The Critical Insight

LLM-level defenses (system prompts, guardrails, alignment) **do not work reliably**. The SaTML competition proved that every LLM defense can be broken with sufficient effort. Therefore:

> **Runtime enforcement is the only viable path.** The LLM must not be part of the authentication loop at all.

---

## 3. The Principle: Identity is Infrastructure, Not Claims

Every recommended approach across all research converges on one principle:

> **The LLM expresses intent. Infrastructure proves identity.**

| What the LLM does | What infrastructure does |
|-------------------|-------------------------|
| "Store this convention about error handling" | Attach verified agent_id, trust level, capabilities |
| "Search for patterns related to testing" | Verify capability token, enforce scope restrictions |
| "Correct this outdated entry" | Verify warrant authorizing correction of specific entry |

The separation is total: the LLM generates the *what*, infrastructure handles the *who*.

### How This Maps to MCP

```
Current (broken):
  LLM generates tool call → { tool: "context_store", agent_id: "human", ... }
                                                      ^^^^^^^^^^^^^^^^
                                                      LLM controls this

Proposed:
  LLM generates tool call → { tool: "context_store", content: "...", ... }
  Runtime attaches token  → { ..., _meta: { session_token: "a8f3..." } }
                                                           ^^^^^^^^^^
                                                           LLM cannot control this
  Unimatrix resolves      → token → { agent_id: "uni-rust-dev", trust: Restricted }
```

---

## 4. Recommended Architecture: Phased Citadel

Three phases, each deployable independently, each adding a defense layer.

### Phase 1: Guardian (Opaque Server-Resolved Tokens)

**Effort**: ~200 LOC, 2-3 days
**Spoofing resistance**: HIGH
**Dependencies**: None (works today)

```
                ┌─────────────────┐
                │  MCP Client     │
                │  (Claude Code)  │
                └────────┬────────┘
                         │ sets env var: UNIMATRIX_SESSION_TOKEN=<random>
                         │
                ┌────────▼────────┐
                │  MCP Runtime    │  reads env var, attaches to every request
                │  (deterministic │  in _meta field
                │   program)      │
                └────────┬────────┘
                         │ { ..., _meta: { session_token: "a8f3c..." } }
                         │
                ┌────────▼────────┐
                │   Unimatrix     │  resolves token → agent identity
                │   MCP Server    │  LLM-provided agent_id becomes advisory only
                └─────────────────┘
```

**How it works**:
1. New redb table: `SESSION_TOKENS` maps opaque random strings to `ResolvedIdentity`
2. Token created at session start (via admin tool, config file, or env var)
3. MCP client runtime includes token in every request's `_meta` field
4. Unimatrix resolves token server-side — the LLM never sees the mapping
5. No token = Restricted/anonymous (backward compatible)

**What it blocks**:
- LLM claiming false agent_id (**blocked** — token determines identity, not agent_id param)
- Prompt injection escalation (**blocked** — even if injected, LLM can't change the token)
- Cross-session token reuse (**blocked** — tokens have TTL and session binding)

**What it doesn't block**:
- Confused deputy (agent has valid token but is manipulated into wrong actions)
- Token leakage via LLM output (mitigated — tokens are opaque, but still a risk)
- Capability escalation within the token's granted permissions

**Near-term deployment**: Users set `UNIMATRIX_SESSION_TOKEN` as an env var in their MCP client config. Zero orchestrator changes needed.

### Phase 2: Warrant Layer (Cryptographic Capability Scoping)

**Effort**: ~400 additional LOC, 3-5 days
**Spoofing resistance**: VERY HIGH
**Dependencies**: Phase 1 + capability token library

```
                ┌─────────────────┐
                │  Orchestrator   │  holds root warrant
                │  (scrum-master) │
                └────────┬────────┘
                         │ attenuates warrant:
                         │   scope: { topic: "nxs-005", ops: [Read, Search] }
                         │   ttl: 30 minutes
                         │
                ┌────────▼────────┐
                │  Worker Agent   │  holds narrowed warrant
                │  (rust-dev)     │  can only read/search in nxs-005
                └────────┬────────┘
                         │ { ..., _meta: { session_token: "...", warrant: "..." } }
                         │
                ┌────────▼────────┐
                │   Unimatrix     │  verifies signature + scope
                │   MCP Server    │  rejects out-of-scope operations
                └─────────────────┘
```

**How it works**:
1. Orchestrator receives a root warrant (minted by Unimatrix admin tool or config)
2. Orchestrator attenuates warrant per-task: narrows scope, shortens TTL
3. Worker agent's runtime includes warrant in requests
4. Unimatrix verifies cryptographic signature and checks scope
5. Authority can only narrow through delegation chain — never widen

**Library options**:

| Library | Maturity | Verification Speed | Rust Native | Agent-Specific |
|---------|----------|-------------------|-------------|----------------|
| **Biscuit** (`biscuit-auth`) | Production (6+ years) | ~264-419us | Yes | No (general purpose) |
| **Tenuo** (`tenuo`) | Beta (3 months) | ~27us | Yes | Yes (MCP integration) |
| **Macaroons** | Not production-safe | Fast | Yes | No |

**Recommendation**: Start with **Biscuit** for stability. Monitor **Tenuo** — if it reaches v1.0, its agent-specific design and 10x faster verification make it compelling.

**What it adds over Phase 1**:
- Confused deputy protection (**blocked** — warrant constrains what the agent CAN do, not just who it IS)
- Capability escalation (**blocked** — warrants can only narrow, never widen)
- Task-scoped access (**new** — agent can only touch entries in assigned topic/category)

### Phase 3: Process Identity Binding (Maximum Resistance)

**Effort**: ~400 additional LOC, transport change
**Spoofing resistance**: MAXIMUM (kernel-enforced)
**Dependencies**: Phase 1 + Phase 2 + Unix domain socket transport

```
                ┌─────────────────┐
                │  MCP Client     │  connects via Unix domain socket
                └────────┬────────┘
                         │ Unix socket connection
                         │
                ┌────────▼────────┐
                │   Unimatrix     │  SO_PEERCRED → kernel reports PID/UID/GID
                │   MCP Server    │  maps PID → registered agent process
                └─────────────────┘  identity is kernel-verified, unspoofable
```

**How it works**:
1. Unimatrix listens on a Unix domain socket instead of (or alongside) stdio
2. On connection, `SO_PEERCRED` returns the peer process's PID, UID, and GID
3. These are kernel-verified — no userspace process can spoof them
4. Unimatrix maps the verified PID/UID to a registered agent identity
5. Combined with warrant verification for scope control

**What it adds**:
- Token theft becomes irrelevant (identity is process-level, not token-level)
- Even a fully compromised LLM cannot claim another process's identity
- The LLM literally has zero control over the authentication credential

**Trade-off**: Requires switching from stdio to Unix socket transport, which needs MCP client support or a shim.

---

## 5. The Security Onion (Defense in Depth)

All three phases compose into a layered defense:

```
Layer 1: Process Identity (Phase 3)
  └─ SO_PEERCRED on Unix socket — kernel-enforced, unspoofable

Layer 2: Session Authentication (Phase 1)
  └─ Opaque token → server-side identity resolution

Layer 3: Capability Warrants (Phase 2)
  └─ Cryptographic scope + TTL + subtractive delegation

Layer 4: Request Validation (existing, vnc-002)
  └─ Input length limits, category allowlist, field validation

Layer 5: Content Scanning (existing, vnc-002)
  └─ ~35 regex patterns for injection + PII detection

Layer 6: Audit Trail (existing, vnc-001)
  └─ Append-only audit log, content hash chain, provenance fields

Layer 7: Behavioral Monitoring (future, Cortical phase)
  └─ Rate limiting, anomaly detection, contradiction detection
```

**Key principle**: Assume every layer will be breached. Each provides independent value. The system remains partially secure even when individual layers fail.

---

## 6. Implementation Decisions

### Decision 1: What Happens to `agent_id` in Tool Params?

**Recommendation**: Keep it, but make it **advisory only**.

- The `agent_id` parameter remains in tool schemas for backward compatibility
- It is logged in audit trail as "claimed identity" (useful for debugging/correlation)
- It is **never** used for capability resolution or trust level assignment
- Actual identity comes from session token (Phase 1) or process identity (Phase 3)

### Decision 2: Token Injection Mechanism

**Options evaluated**:
| Mechanism | LLM Can Access? | Requires Client Changes? |
|-----------|:---------------:|:------------------------:|
| Environment variable (`UNIMATRIX_SESSION_TOKEN`) | No (env vars are process-level) | Minimal (MCP config) |
| Tool `_meta` field | Technically yes, but runtime-injected | Yes (client must support) |
| MCP `initialize` handshake | No (handled before LLM runs) | Yes |
| Unix socket SO_PEERCRED | No (kernel-level) | Yes (transport change) |

**Recommendation**: Start with **environment variable** (zero client changes). The MCP client config already supports env vars for servers. Evolve to `_meta` injection when MCP clients support it.

### Decision 3: Key Management

For Phase 2 (warrants), a signing key is needed:
- **Option A**: Generated on first run, stored in redb → Simple, single-machine only
- **Option B**: Provided via environment variable → Supports key rotation, multi-machine
- **Option C**: Derived from user passphrase → User-controlled, but adds UX friction

**Recommendation**: Option A for now, evolve to B. A single-machine knowledge engine doesn't need distributed key management.

### Decision 4: Backward Compatibility Period

- **Phase 1 launch**: Agents without tokens work as Restricted (current behavior preserved)
- **Deprecation warning**: Log a warning when identity comes from `agent_id` param instead of token
- **Hard cutover**: After 2 feature cycles, require tokens for any trust level above Restricted

### Decision 5: Biscuit vs Tenuo

| Factor | Biscuit | Tenuo |
|--------|---------|-------|
| Maturity | 6+ years, Eclipse Foundation | 3 months, beta |
| Stars | 227 | 27 |
| API stability | Stable | "APIs may evolve" |
| Rust support | First-class (`biscuit-auth`) | First-class (Rust core) |
| Verification speed | ~264-419us | ~27us |
| Agent-specific features | None | MCP integration, semantic constraints |
| Policy language | Datalog (very flexible) | Structured constraints |
| Attenuation | Yes (Datalog blocks) | Yes (monotonic) |
| Risk | Low | API churn, low adoption |

**Recommendation**: **Biscuit** for Phase 2 initial implementation. Re-evaluate Tenuo at v1.0. Biscuit's Datalog is more expressive and its maturity reduces risk. The 10x speed difference (419us vs 27us) is irrelevant for our request rates.

---

## 7. What Unimatrix Can Do Without Orchestrator Changes

Even without Claude Code or Cursor implementing token injection natively:

1. **Environment variable at startup**: User sets `UNIMATRIX_SESSION_TOKEN` in MCP client config. The token maps to a pre-configured identity. This is the simplest path.

2. **Admin tool for token creation**: Human user (Privileged trust) calls a `session_create` tool that returns a token. They configure it for subsequent agent sessions.

3. **First-connection token exchange**: During MCP `initialize`, Unimatrix generates a session token and returns it in capabilities. The MCP client runtime caches and reuses it.

4. **Config-based pre-registration**: Map known agent names to pre-generated tokens in a configuration file loaded at startup.

Option 1 is deployable today with zero external changes.

---

## 8. Migration Path

```
Current state:
  LLM → agent_id param → resolve_or_enroll → capabilities
  Risk: ANY LLM can claim ANY identity

Phase 1 (Guardian):
  Runtime → session_token → resolve_session → capabilities
  LLM → agent_id param → audit log only (advisory)
  Risk: Token theft, confused deputy

Phase 2 (Warrant):
  Runtime → session_token + warrant → verify_warrant → scoped capabilities
  Risk: Orchestrator compromise

Phase 3 (Citadel):
  OS → PID/UID (SO_PEERCRED) + Runtime → token + warrant → full stack
  Risk: Kernel vulnerability (extremely unlikely)
```

---

## 9. Roadmap Mapping

| Phase | Feature | Milestone | Effort |
|-------|---------|-----------|--------|
| 1 | Opaque session tokens | vnc-004 or vnc-005 | 2-3 days |
| 1 | `agent_id` becomes advisory | Same | Part of above |
| 2 | Biscuit integration | vnc-005 or vnc-006 | 3-5 days |
| 2 | Warrant-scoped operations | Same | Part of above |
| 3 | Unix domain socket transport | vnc-007+ or standalone | 5-8 days |
| 3 | Process identity binding | Same | Part of above |
| -- | Behavioral monitoring | crt-001 | Separate feature |
| -- | Contradiction detection | crt-003 | Separate feature |

---

## 10. Open Questions

1. **How do we handle the human user's identity?** The `"human"` agent currently gets Privileged trust via bootstrap. With token-based auth, the human's token is pre-configured at install time or in MCP client config.

2. **Multi-orchestrator support**: If multiple MCP clients connect (Claude Code + Cursor), each needs its own session. Sessions should be namespaced by connection.

3. **Dynamic scope elevation**: What happens when an agent discovers mid-task that it needs broader access? Options: request new warrant from orchestrator, or fail with clear error explaining required scope.

4. **Token rotation frequency**: Per-session (simple) vs. per-request (maximum security, complex). Per-session with short TTL (30 min) is the pragmatic choice.

5. **Warrant storage**: Should warrants be passed in every request, or cached server-side after first presentation? Passing per-request is simpler and avoids cache invalidation issues.

---

## Sources

This synthesis draws from 150+ references across the 4 companion research documents. Key sources:

### Foundational
- Agent Security Bench (ICLR 2025) — 84.3% attack success rate
- SaTML LLM CTF (2025) — all 72 defenses broken
- Meta AI "Agents Rule of Two" (Oct 2025)
- OWASP Top 10 for Agentic Applications (2026)
- "Breaking the Protocol" (arXiv:2601.17549)

### Architecture
- Service mesh identity model (Istio, SPIFFE/SPIRE)
- Tenuo cryptographic warrants (github.com/tenuo-ai/tenuo)
- Biscuit authorization tokens (biscuitsec.org)
- Microsoft FIDES (Information Flow Control)
- Google DeepMind CaMeL (arXiv:2503.18813)

### Attack Research
- PoisonedRAG (USENIX Security 2025) — 90% success with 5 entries
- AgentPoison (NeurIPS 2024) — 80%+ with <0.1% poison rate
- MemoryGraft (arXiv:2512.16962) — persistent memory poisoning
- EchoLeak (CVE-2025-32711) — first real-world zero-click exploit
- Promptware Kill Chain (arXiv:2601.09625)

### Standards
- MCP Authorization Specification (2025-11-25)
- OWASP Secure MCP Server Development Guide (2026)
- OpenID Connect for Agents (OIDC-A 1.0)
- A2A Protocol (Google, 2025)

Full bibliographies in companion research documents.
