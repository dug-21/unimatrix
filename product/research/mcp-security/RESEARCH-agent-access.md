# Research: Agent-Based Access Control, Intent Mapping, and Multi-Agent Security

**Date**: 2026-02-23
**Context**: Security research for Unimatrix MCP layer (pre-vnc-001)

---

## 1. Agent Identity and Authentication

### Current State of the Art

The industry is converging on treating AI agents as first-class identity principals, distinct from both human users and traditional machine identities (NHIs). Key developments:

**Strata Identity Research (2026)**: Survey of 500+ AI and identity leaders found 89% agree agents need unique, non-human-like identity. 68% of organizations with deployed agents report at least one identity-related security incident. The "AI Agent Identity Crisis" is driven by agents that share ambient credentials, inherit overly broad permissions, and lack auditable identity trails.

**MCP Specification (June 2025)**: Mandates OAuth 2.1 for HTTP transport, with MCP servers as Resource Servers. Key requirements: PKCE mandatory, RFC 8707 Resource Indicators, minimal initial scopes with progressive elevation. Anti-patterns explicitly forbidden: token passthrough, using sessions for authentication.

**Zero-Trust Identity Framework (arXiv 2505.19301)**: Proposes Decentralized Identifiers (DIDs) with Verifiable Credentials (VCs) for agents. Each agent holds a unique DID anchored cryptographically. VCs attest to roles, capabilities, compliance status. Enables peer-to-peer trust without hierarchical identity providers. Implementation includes Agent Naming Service (ANS) for capability-aware discovery.

### Practical Spectrum for Local Systems

| Approach | Complexity | Fit for Unimatrix |
|----------|-----------|-------------------|
| Agent enrollment with pre-shared keys | Low | Good for local single-machine |
| Signed JWT bearer tokens per agent | Medium | Good for multi-agent orchestration |
| mTLS between agent processes | Medium-High | Strong but complex for stdio |
| Full OAuth 2.1 (MCP spec) | High | Required for HTTP transport |
| DIDs/VCs | Very High | Future cross-project scenarios |

Sources:
- [Strata: The AI Agent Identity Crisis](https://www.strata.io/blog/agentic-identity/the-ai-agent-identity-crisis-new-research-reveals-a-governance-gap/)
- [MCP Authorization Specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
- [arXiv 2505.19301: Zero-Trust Identity for Agentic AI](https://arxiv.org/html/2505.19301v1)
- [WSO2: Why AI Agents Need Their Own Identity](https://wso2.com/library/blogs/why-ai-agents-need-their-own-identity-lessons-from-2025-and-resolutions-for-2026/)
- [CyberArk: AI Agents and Identity Risks](https://www.cyberark.com/resources/blog/ai-agents-and-identity-risks-how-security-will-shift-in-2026)

---

## 2. Intent-to-Access Mapping

### Declarative Intent (Production-Ready)

Agents declare their intended operation at request time. The operation type maps to permitted capabilities.

**Oso Authorization Framework**: Policy-as-code where each tool call carries metadata about operation type, target resource, and scope. A Policy Decision Point evaluates intent against declared policies before execution.

**Tenuo Cryptographic Warrants**: Before executing a task, the orchestrator mints a warrant declaring exactly what tools, paths, and arguments the agent may use. The warrant IS the declared intent, cryptographically enforced at tool-call time (~27 microseconds verification). Eliminates the need to infer intent.

### Semantic Intent Inference (Prototype/Theoretical)

**ISACA (2025)**: "Understanding the intent of data flowing between components requires solutions that will understand whether particular prompts or natural language are part of normal interactions, or potentially harmful or malicious in intent."

**Agentic AI Protection Framework (2026)**: Describes an "Agentic AI Firewall capable of understanding intent at the semantic layer to prevent Tool Misuse and Capability Abuse."

### Unimatrix Application

Our tool model already has implicit intent -- `context_store` intends to write, `context_search` intends to read. The mapping is deterministic, not inferential. This is a strength: we don't need semantic intent analysis, just capability verification per tool.

Sources:
- [Oso: AI Agent Permissions](https://www.osohq.com/learn/ai-agent-permissions-delegated-access)
- [Tenuo: Capability Tokens](https://tenuo.ai/)
- [ISACA: Safeguarding Agentic AI Workflows](https://www.isaca.org/resources/news-and-trends/industry-news/2025/safeguarding-the-enterprise-ai-evolution-best-practices-for-agentic-ai-workflows)

---

## 3. Least Privilege for AI Agents

### The Core Challenge

Strata articulates it well: "Agents don't follow fixed workflows. They reason. They plan. They adapt. What they need to do isn't fully known until execution time." Static permission grants inevitably become over-broad.

### Patterns

**Task-Scoped Permissions**: Permissions minted per-task with shortest possible TTL. Tenuo warrants implement this. Oso's JIT access model issues short-lived tokens revoked after task completion. **Maturity: Production-ready.**

**Topic-Restricted Access**: Agents can only read/write entries matching specific categories or topics. Natural for Unimatrix given its `{topic, category, query}` model. Must be custom-built. **Maturity: Well-understood pattern, no off-the-shelf solution for knowledge engines.**

**Read-Only vs. Read-Write Separation**: OWASP explicitly recommends: "Grant agents only read-only or write permissions to specific resources, not blanket access." **Maturity: Production-ready.**

**Progressive Scope Elevation**: MCP spec recommends: "Minimal initial scope set containing only low-risk discovery/read operations. Incremental elevation via targeted scope challenges when privileged operations are first attempted." **Maturity: Specified in MCP protocol.**

**Delegated Access**: Oso model -- agent inherits invoking user's permissions. Downstream services enforce the user's current permissions in real-time. "If the user loses access to a record, so does the agent." **Maturity: Production-ready.**

### Unimatrix Mapping

| Agent Type | Access Level |
|-----------|-------------|
| Worker agents (code generators, testers) | Read-only, scoped to assigned topic/category |
| Orchestrator agents (scrum-master) | Read-write, scoped to active feature |
| Human user via MCP client | Full access |
| Validators | Read-only across features (cross-reference) |

Sources:
- [OWASP AI Agent Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/AI_Agent_Security_Cheat_Sheet.html)
- [MCP Security Best Practices](https://modelcontextprotocol.io/specification/2025-06-18/basic/security_best_practices)
- [Strata: Rethink of Least Privilege](https://www.strata.io/blog/why-agentic-ai-forces-a-rethink-of-least-privilege/)
- [AWS: Least Privilege for Agentic Workflows](https://docs.aws.amazon.com/wellarchitected/latest/generative-ai-lens/gensec05-bp01.html)

---

## 4. Multi-Agent Trust Models

### Hierarchical Trust (42% of enterprise implementations)

Orchestrator agents have higher trust than workers. Trust flows downward. Workers cannot escalate beyond delegated authority.

**OWASP Trust Levels**: UNTRUSTED, INTERNAL, PRIVILEGED, SYSTEM. Inter-agent messages digitally signed (JWT with timestamps). Payloads sanitized by trust level. Message freshness validated (reject >5 minutes). Circuit breakers (failure_threshold=5, 60-second recovery).

**ARIA (Agent Relationship-based Identity and Authorization)**: Every delegation recorded as a cryptographically verifiable relationship in a graph. On-Behalf-Of chains track delegation from human to orchestrator to worker. Token exchange with Proof-of-Possession across trust boundaries.

### Decentralized Trust (DIDs + VCs)

Agents from different organizations establish trust through verifiable identity (DID), capability claims (VCs), and reputation scores. Parent agents spawn sub-agents with unique DIDs and Provenance VCs documenting delegation chain.

### Unimatrix Trust Hierarchy

| Trust Level | Agent Type | Capabilities |
|------------|-----------|-------------|
| SYSTEM | Unimatrix MCP server | Full database access |
| PRIVILEGED | Human user | All tools, all topics |
| INTERNAL | Orchestrator (uni-scrum-master) | Read-write, scoped to feature |
| RESTRICTED | Worker (rust-dev, tester) | Read-only, assigned component |

Sources:
- [OWASP AI Agent Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/AI_Agent_Security_Cheat_Sheet.html)
- [Strata: ARIA Identity Playbook](https://www.strata.io/blog/agentic-identity/new-identity-playbook-ai-agents-not-nhi-8b/)
- [arXiv 2505.19301: Zero-Trust Framework](https://arxiv.org/html/2505.19301v1)

---

## 5. Stray Agent Prevention

### Techniques

**Agent Enrollment/Allowlisting**: Explicit agent registries with approved identities. Agents must be registered and credentialed before accessing resources. **Maturity: Production-ready.**

**Behavioral Validation**: Continuous monitoring breaking interactions into granular steps, flagging suspicious behavior. Volume spikes, unexpected tool usage, unusual access patterns. **Maturity: Commercial products exist (Zenity, Obsidian Security).**

**Cryptographic Tool-Call Gating**: Tenuo gates tool calls with cryptographic warrants. Even if prompt-injected, "the authority still can't escape its bounds." **Maturity: Beta.**

**Transport-Level Isolation**: MCP stdio inherently limits access to the spawning process. MCP spec recommends unix domain sockets for additional isolation. **Maturity: Built into MCP.**

### Unimatrix Defense Stack

1. **Stdio transport** (default) -- inherent process isolation
2. **Agent registry** -- reject unknown agent_ids at connection
3. **Capability verification** -- check every tool call against registered capabilities
4. **Rate limiting** -- cap requests per agent per time window

---

## 6. Capability-Based Security (Object Capabilities)

### Why It Fits Unimatrix

Capability-based security eliminates ambient authority by design. An agent without a capability token literally cannot invoke an operation. Capabilities can only be narrowed when delegated, never broadened.

**Tenuo** (Rust core, MIT/Apache-2.0) implements this:
1. Orchestrator mints warrant: `mint(Capability("context_store", topic="nxs-003"))`
2. Warrant is signed, time-bound, specifies exact tools/paths/arguments
3. Agent presents warrant when calling tools
4. Verification: ~27 microseconds, offline
5. Warrant expires when task completes

**Attenuation**: Orchestrator holds `{read, write}` for `nxs-003`. Delegates `{read}` only to worker. Worker cannot escalate. Each step is cryptographically traceable.

**Prompt injection defense**: Even if injected, the agent cannot call tools it doesn't hold warrants for. Authority is cryptographic, not behavioral.

### Proposed Capability Model

```
Capability {
    agent_id: String,
    operations: Vec<Operation>,  // Read, Write, Search, Update, Deprecate
    scope: Scope {
        topics: Option<Vec<String>>,
        categories: Option<Vec<String>>,
        entry_ids: Option<Vec<u64>>,
    },
    ttl: Duration,
    issuer: AgentId,
    signature: [u8; 64],
}
```

Sources:
- [Tenuo](https://tenuo.ai/) / [GitHub](https://github.com/tenuo-ai/tenuo) / [crates.io](https://crates.io/crates/tenuo)
- [A2A Capability-Based Authorization Discussion](https://github.com/a2aproject/A2A/discussions/1404)

---

## 7. Audit Trails

### What to Log

| Event | Fields |
|-------|--------|
| Tool invocation | agent_id, tool, params (redacted), timestamp, outcome |
| Auth decision | agent_id, operation, scope, allow/deny, reason |
| Data access | agent_id, entry_ids, access_type, query scope |
| Entry mutation | agent_id, entry_id, operation, before_hash, after_hash |
| Capability grant/revoke | issuer_id, target_id, capabilities, ttl |

### Correlation

- `request_id` per user/agent request
- `session_id` per MCP session
- `agent_id` per agent instance
- `trace_id` for distributed tracing
- Compatible with W3C Trace Context

### Integrity

- Append-only storage
- Optional hash chaining (each entry includes SHA-256 of previous)
- Digital signatures for non-repudiation (future)

Sources:
- [Tetrate: MCP Audit Logging](https://tetrate.io/learn/ai/mcp/mcp-audit-logging)
- [ISACA: Auditing Agentic AI](https://www.isaca.org/resources/news-and-trends/industry-news/2025/the-growing-challenge-of-auditing-agentic-ai)
- [Adopt AI: Audit Trails for Agents](https://www.adopt.ai/glossary/audit-trails-for-agents)
