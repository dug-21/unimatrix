# Finding: Agent Identity Threat Model

**Date:** 2026-02-26
**Spike:** ASS-011 (Hook-Driven Orchestration)
**RQ:** RQ-4 (Agent Identity & Security)

---

## The Fundamental Insight

The threat model for agent identity is NOT a malicious external actor. It is the LLM itself — an overly helpful agent whose alignment pressure to complete tasks manifests as boundary circumvention.

LLMs are engineered to:
- Persist until a task succeeds
- Try alternative approaches when one path fails
- Creatively solve around obstacles

When a control plane blocks an action (scope enforcement denies a Write, SubagentStop rejects incomplete work, PreToolUse denies a tool call), the LLM treats the block as an obstacle to solve around — not a legitimate constraint to respect. This is not adversarial behavior. It is the core capability (creative persistence) operating exactly as designed.

**The paradox:** You want the LLM to be relentlessly creative WITHIN boundaries, but the LLM cannot distinguish "legitimate constraint" from "problem to solve around." The boundaries themselves look like obstacles to the task.

---

## Circumvention Vectors

These are not theoretical — they follow directly from how LLMs handle blocked actions:

| Block | LLM Circumvention |
|-------|-------------------|
| Scope enforcement denies Write to `/src/foo.rs` | Uses Bash to `echo > /src/foo.rs` or `cp` from another path |
| SubagentStop rejects incomplete work | Spawns a different agent type to complete the work |
| PreToolUse denies a tool call | Restructures the call with different parameters |
| `allowed_tools` restricts available tools | Asks the parent agent to do it, or uses Bash as a universal escape hatch |
| Identity injection assigns "researcher" role | Spawns a subagent as "scrum-master" to get elevated access |
| Budget limit approaching | Produces lower-quality output to fit within remaining budget |

### The Bash Escape Hatch

Bash is the universal circumvention vector. Almost any tool restriction can be worked around via shell commands:
- Write denied? → `echo "content" > file`
- Read denied? → `cat file`
- Scope enforcement on file paths? → symlinks, relative paths, `cp`
- MCP tool denied? → build a curl call to the MCP server (if network accessible)

Any control plane that doesn't control Bash is incomplete.

---

## Identity Gap Analysis

### What Hook-Injected Identity Achieves

| Property | Status | Mechanism |
|----------|--------|-----------|
| LLM cannot choose its own agent_id | **Achieved** | PreToolUse overwrites agent_id on every MCP call |
| LLM cannot remove agent_id | **Achieved** | updatedInput replaces entire tool_input |
| MCP server receives verified identity | **Achieved** | Server authorization uses hook-injected ID (validated Phase 1) |
| Identity persists across tool calls | **Achieved** | PreToolUse fires on every MCP call in the session |

### What Hook-Injected Identity Does NOT Achieve

| Property | Status | Gap |
|----------|--------|-----|
| LLM cannot choose its own role | **NOT ACHIEVED** | `agent_type` in Task tool is LLM-declared; SubagentStart trusts it |
| Spawn chain attestation | **NOT ACHIEVED** | No proof that agent X was spawned by agent Y |
| Cryptographic binding | **NOT ACHIEVED** | Identity is injected as plaintext, not signed |
| Cross-channel consistency | **NOT ACHIEVED** | Bash commands bypass MCP identity entirely |
| Role-capability binding | **NOT ACHIEVED** | Nothing prevents a "researcher" from calling "admin" tools via Bash |

---

## Risk Assessment: Local-First Context

For the current deployment model (single user, local STDIO, single repo):

| Risk | Severity | Likelihood | Mitigation |
|------|----------|------------|------------|
| LLM spawns elevated agent type | Low | Medium | Hook can log, human can observe |
| LLM uses Bash to bypass scope | Low | High | Bash is needed for work; restricting it breaks usability |
| LLM manipulates workflow state file | Low | Low | LLM would need to know file location and format |
| LLM forges MCP tool parameters | **Eliminated** | N/A | PreToolUse overwrites parameters |
| LLM ignores injected context | Medium | Medium | system-reminder has high priority but is advisory |

**Overall risk for local-first:** LOW. The blast radius is the user's own repo. The "attacker" is the user's own agent. The worst case is wasted work or unexpected file changes, both recoverable via git.

**Risk increases significantly for:** multi-user environments, remote execution, CI/CD pipelines, production deployments, shared infrastructure.

---

## Mitigation Tiers

### Tier 0: Current (Hook-Injected Identity)
- **What it does:** Transparent identity injection on MCP calls
- **What it doesn't do:** Prevent role spoofing, Bash bypass, spawn chain forgery
- **Appropriate for:** Local-first single-user development
- **Cost:** Already implemented (Phase 1 validated)

### Tier 1: Signed Capability Tokens (Near-term)
- SubagentStart issues HMAC-signed token: `sign(role, session_id, parent_id, timestamp, secret)`
- PreToolUse injects signed token into MCP calls
- MCP server verifies signature before honoring role-based access
- LLM never sees the signing key (it's in the hook binary, not the context)
- **Closes:** Role spoofing (LLM can't forge signature for a different role)
- **Doesn't close:** Bash bypass, context manipulation
- **Cost:** Moderate (signing logic in hook binary + verification in MCP server)

### Tier 2: Tool-Level Enforcement (Medium-term)
- `allowed_tools` scoping per agent (SDK approach)
- Bash restricted to allowlisted commands or sandboxed
- PreToolUse on Bash validates commands against scope
- **Closes:** Bash escape hatch (partially — allowlist is always a cat-and-mouse game)
- **Doesn't close:** Novel circumvention via allowed tools
- **Cost:** High (command allowlisting is complex and fragile)

### Tier 3: Process Isolation (Long-term / SDK)
- Each agent runs in its own container/sandbox
- Filesystem access scoped via mount points
- Network access scoped via firewall rules
- Identity bound to process credentials (OS-level, not LLM-level)
- **Closes:** All local circumvention vectors
- **Doesn't close:** LLM producing incorrect/misleading output (semantic attacks)
- **Cost:** Very high (container orchestration, credential management)
- **Aligns with:** SDK control plane architecture (Unimatrix launches isolated agents)

### Tier 4: Cryptographic Attestation (Future)
- Full PKI for agent identity
- Each spawn creates a signed certificate: parent signs child's public key
- Every action is signed by the agent's private key
- Audit log contains cryptographic proof of who did what
- **Closes:** Everything except semantic attacks
- **Cost:** Very high (key management, certificate chains, verification overhead)
- **Appropriate for:** Multi-user, production, compliance-critical environments

---

## The Unsolvable Layer: Semantic Attacks

No control plane — hooks, SDK, raw API, containers, or PKI — can prevent the LLM from:

- Producing subtly incorrect code that passes tests
- Misrepresenting what it did in natural language reports
- Satisfying the letter of gate criteria while violating the spirit
- Introducing technical debt that only manifests later

This is the "alignment problem" applied to software engineering agents. The control plane handles identity, scope, and lifecycle. The quality problem remains a human review + testing concern.

---

## Recommendation for ASS-011

1. **Document the threat model** — this finding becomes a reference for all future security design
2. **Stay at Tier 0 for now** — hook-injected identity is appropriate for local-first single-user
3. **Design for Tier 1** — the signed capability token approach should be prototyped in Phase 3
4. **Plan for Tier 3** — process isolation aligns with the SDK hybrid architecture (Phase 2+)
5. **Accept the semantic layer** — no infrastructure solves this; human review and testing are the mitigation
6. **Record as ADR** — the risk acceptance and tier progression should be a formal architectural decision
