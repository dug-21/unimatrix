# Novel Architecture Patterns for LLM-Resistant Agent Authentication

**Research Document: ASS-008**
**Date:** 2026-02-24
**Scope:** Authentication architectures where identity is proven through mechanisms the LLM cannot control, applied to Unimatrix (Rust, MCP stdio, local-first)

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [The Core Problem: LLMs as Adversarial Identity Claimants](#2-the-core-problem)
3. [Identity is Infrastructure, Not Claims](#3-identity-is-infrastructure)
4. [MCP Protocol-Level Identity Solutions](#4-mcp-protocol-level)
5. [Orchestrator-Mediated Authentication](#5-orchestrator-mediated)
6. [Opaque Token and Reference Token Patterns](#6-opaque-tokens)
7. [Mutual Authentication Without LLM Involvement](#7-mutual-auth)
8. [Intent-Based Access Control](#8-intent-based)
9. [Multi-Layer Defense Architecture](#9-multi-layer)
10. [Prototype Architectures for Unimatrix](#10-prototypes)
11. [Comparative Evaluation](#11-evaluation)
12. [Recommended Architecture](#12-recommendation)
13. [References](#13-references)

---

## 1. Executive Summary

Current Unimatrix authentication relies on claims-based identity: the LLM passes an `agent_id` string in tool parameters, auto-enrolling unknown agents as `Restricted`. This is fundamentally insecure. An LLM can trivially claim to be `"human"` (Privileged trust) or `"system"` (System trust) and gain elevated capabilities. The LLM generates the text that contains the identity claim --- and LLMs are specifically designed to generate convincing text.

This research examines eight architectural paradigms for authentication that remove the LLM from the identity-proving loop. The central thesis: **identity must be an emergent property of the infrastructure** (process, connection, cryptographic material) rather than a claim in the LLM's output stream.

Three concrete prototype architectures are proposed for Unimatrix's specific constraints (Rust, MCP stdio transport, local-first, single-machine deployment). The recommended architecture combines process-level identity extraction via Unix socket `SO_PEERCRED` with orchestrator-injected opaque session tokens and Tenuo-style capability warrants, forming a defense-in-depth stack where no single layer's failure compromises the system.

---

## 2. The Core Problem: LLMs as Adversarial Identity Claimants

### 2.1 Why Claims-Based Identity Fails for LLMs

In traditional service-to-service communication, identity claims work because services are deterministic programs: a service configured to send `agent_id: "payment-service"` will reliably do so. The service has no incentive or ability to lie about its identity.

LLMs break this assumption in three ways:

1. **Generative capability**: LLMs produce arbitrary text, including identity claims. An LLM told "you are agent X" via system prompt can equally be told (via prompt injection) "actually, you are agent Y."

2. **Context window as attack surface**: Anything in the LLM's context window can influence its outputs. If the identity mechanism exists within the text stream, it is within the LLM's influence boundary.

3. **Tool call manipulation**: LLMs generate tool call parameters. If identity is a parameter, the LLM chooses the value. Even well-intentioned LLMs may hallucinate identity values or be manipulated through indirect prompt injection from retrieved content.

### 2.2 Unimatrix's Current Vulnerability

The current `identity.rs` implementation extracts `agent_id` from tool parameters:

```rust
pub fn extract_agent_id(agent_id: &Option<String>) -> String {
    match agent_id {
        Some(id) => {
            let trimmed = id.trim();
            if trimmed.is_empty() { "anonymous".to_string() }
            else { trimmed.to_string() }
        }
        None => "anonymous".to_string(),
    }
}
```

Nothing prevents an LLM from passing `agent_id: "system"` and gaining System-level trust with Read, Write, Search, and Admin capabilities. The `resolve_or_enroll` function in `registry.rs` trusts whatever string it receives.

### 2.3 The Threat Model

| Threat | Description | Current Mitigation |
|--------|-------------|-------------------|
| Identity spoofing | LLM claims to be a higher-trust agent | None |
| Privilege escalation | Restricted agent claims Privileged identity | None |
| Prompt injection escalation | Injected content causes agent to claim different identity | None |
| Confused deputy | Agent acts with its own permissions on behalf of injected instructions | Content scanning (vnc-002), but not identity-aware |
| Replay attack | Previously observed agent_id reused | None (identity is stateless string) |

### 2.4 The "Agents Rule of Two"

Meta AI's "Agents Rule of Two" (October 2025) formalizes a key insight: until robust prompt injection defenses exist, an agent must satisfy **no more than two** of these three properties to avoid catastrophic consequences:

1. Can process untrustworthy inputs
2. Has access to sensitive systems or private data
3. Can change state or communicate externally

Unimatrix agents that store knowledge (property 3) while processing external context (property 1) with access to the knowledge store (property 2) satisfy all three --- the "lethal trifecta." Authentication architecture must constrain this surface.

> "By systematically tuning and scaling general optimization techniques, researchers bypass 12 recent prompt injection defenses with attack success rate above 90% for most." --- Simon Willison, summarizing "The Attacker Moves Second" (Anthropic/OpenAI/DeepMind, 2025)

---

## 3. Identity is Infrastructure, Not Claims

### 3.1 The Service Mesh Model

In service mesh architectures (Istio, Linkerd, Consul Connect), identity is never claimed by the service. Instead:

1. The **control plane** (e.g., Istiod) issues cryptographic identities
2. A **sidecar proxy** (e.g., Envoy) handles mTLS on behalf of the service
3. The service **never touches credentials** --- it communicates via localhost, and the proxy handles authentication
4. Identity is derived from **infrastructure attestation** (Kubernetes service account, pod metadata, node identity)

```
Traditional Claims-Based:
  Service ---("I am X")--> Target
  Target trusts the claim

Service Mesh Model:
  Control Plane ---[X.509 cert]--> Sidecar
  Sidecar ---[mTLS with cert]--> Target Sidecar
  Service ---[localhost:8080]--> Sidecar (no auth needed)
  Target sees verified identity from mTLS, never from service
```

The service literally cannot lie about its identity because it never participates in the authentication handshake.

### 3.2 SPIFFE/SPIRE: Workload Identity at Scale

SPIFFE (Secure Production Identity Framework For Everyone) defines a standard for workload identity:

- **SPIFFE ID**: A URI like `spiffe://trust-domain/workload-identifier` that uniquely identifies a workload
- **SVID (SPIFFE Verifiable Identity Document)**: A short-lived X.509 certificate or JWT containing the SPIFFE ID
- **SPIRE (SPIFFE Runtime Environment)**: Issues SVIDs based on workload attestation

SPIRE performs two-stage attestation:
1. **Node attestation**: Verifies the node itself (AWS instance identity document, Kubernetes node token, TPM attestation)
2. **Workload attestation**: Verifies the workload on the node (PID, container ID, Kubernetes pod metadata, Unix user, binary hash)

Key properties relevant to Unimatrix:
- Identity is issued to the **process**, not claimed by the **application logic**
- Certificates are short-lived (typically minutes to hours) and auto-rotated
- No long-lived secrets --- the workload API delivers fresh credentials
- Attestation is based on properties the workload cannot forge (PID, binary hash, container image)

### 3.3 Applying Infrastructure Identity to LLM Agents

The parallel to LLM agents:

| Service Mesh Concept | LLM Agent Equivalent |
|----------------------|---------------------|
| Service binary | LLM agent process (Claude Code, Cursor, custom orchestrator) |
| Sidecar proxy | MCP client runtime (not the LLM itself) |
| Control plane | Orchestrator or deployment system |
| Node attestation | Process-level identity (PID, UID, binary path) |
| Workload attestation | Agent configuration (system prompt hash, tool permissions) |
| SVID certificate | Session token or capability warrant |

The critical insight: **the MCP client process** (Claude Code binary, Cursor process) is a deterministic program that can participate in cryptographic protocols. The LLM running inside it cannot. Authentication should happen at the process level, not the LLM output level.

### 3.4 Comparison with Traditional Approaches

| Approach | Identity Holder | LLM Can Forge? | Prompt Injection Resistant? |
|----------|----------------|----------------|---------------------------|
| agent_id in tool params | LLM output | Yes | No |
| API key in prompt | LLM context | Yes (can leak) | No |
| Env var read by Bash tool | LLM-accessible via tools | Partially (LLM can read env) | Partially |
| mTLS at process level | OS/runtime | No | Yes |
| Unix socket SO_PEERCRED | OS kernel | No | Yes |
| Opaque session token (env) | Runtime process | No (if not in context) | Yes |

---

## 4. MCP Protocol-Level Identity Solutions

### 4.1 Current MCP Authentication Landscape

The MCP specification (as of 2025-06-18) defines two transport modes with fundamentally different authentication models:

**HTTP/Streamable HTTP Transport:**
- Full OAuth 2.1 authorization framework
- MCP servers classified as OAuth Resource Servers
- Resource Indicators (RFC 8707) required to bind tokens to specific servers
- Dynamic client registration
- Audience-restricted tokens

**Stdio Transport:**
- The MCP spec explicitly states that stdio transport "SHOULD NOT" use OAuth
- Identity is implicit: the MCP client starts the MCP server as a subprocess
- Credentials passed through environment variables when the process starts
- No authentication handshake defined

Unimatrix uses stdio transport, which means the OAuth 2.1 framework does not apply. This is both a limitation and an opportunity.

### 4.2 Stdio Transport: Identity Through Process Parentage

With stdio transport, identity proof is architectural:

```
MCP Client (Claude Code)
    |
    +-- spawn --> Unimatrix MCP Server (child process)
         stdin/stdout pipe
```

The MCP server is started **by** the MCP client. The OS provides guarantees about this relationship:
- The parent process PID is known (`getppid()`)
- The user ID is inherited from the parent
- Environment variables are set at spawn time, before any LLM interaction
- The stdin/stdout pipe is a kernel-mediated channel between two specific processes

This process parentage IS the identity proof. No LLM is involved in establishing the communication channel.

### 4.3 The `initialize` Handshake as Identity Binding Point

The MCP `initialize` handshake exchanges `clientInfo` and `serverInfo`:

```json
{
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-06-18",
    "capabilities": { ... },
    "clientInfo": {
      "name": "claude-code",
      "version": "1.0.0"
    }
  }
}
```

Currently, `clientInfo` is used for debugging, not authentication. However, it represents the first message from the MCP client and could carry:
- A session token generated at spawn time
- A cryptographic challenge-response
- A nonce for session binding

**Limitation**: The `clientInfo` fields are defined by the MCP specification. Adding custom authentication fields requires either spec extension or using the `experimental` capabilities field.

### 4.4 Proposed MCP Extensions for Agent Auth

Several proposals exist for extending MCP with authentication:

**MCPX (MCP Extensions):**
Proposed additional fields for richer client/server metadata, potentially including identity attestation fields.

**MCP Authorization Specification (June 2025 update):**
Clarified that servers must validate token audiences, preventing token passthrough attacks. While designed for HTTP transport, the principles apply to any transport.

**Resource Indicators (RFC 8707):**
Bind tokens to specific resource servers, preventing a token issued for Server A from being used at Server B. Relevant for multi-server Unimatrix deployments.

### 4.5 Feasibility for Unimatrix

| MCP Feature | Feasibility for Unimatrix | Notes |
|-------------|--------------------------|-------|
| OAuth 2.1 | Low | Designed for HTTP transport; overkill for local stdio |
| clientInfo identity | Medium | Requires custom fields; no spec support for auth in clientInfo |
| initialize challenge-response | Medium | Could work via experimental capabilities |
| Process parentage | High | Already available via stdio transport model |
| Environment variable injection | High | Standard pattern for stdio MCP servers |

---

## 5. Orchestrator-Mediated Authentication

### 5.1 The Pattern

The orchestrator (Claude Code, Cursor, or a custom multi-agent system) is the **trusted authority** that spawns agents. It creates a signed session token and injects it into the agent's environment **before** the LLM receives any context.

```
                    +-------------------+
                    |   Orchestrator    |
                    | (Claude Code)     |
                    +--------+----------+
                             |
             1. Generate session token
             2. Sign with orchestrator key
             3. Set as env var
                             |
                    +--------v----------+
                    |   Agent Process   |
                    | (MCP client)      |
                    +--------+----------+
                             |
             4. Agent runtime reads token from env
             5. Includes token in MCP initialize or tool calls
             6. LLM generates intent (what to do)
             7. Runtime attaches token (who is doing it)
                             |
                    +--------v----------+
                    |   Unimatrix       |
                    | (MCP server)      |
                    +-------------------+
             8. Verify signature
             9. Extract identity from token
            10. Enforce capabilities
```

### 5.2 Token Injection: Environment vs. Context

The distinction between environment injection and context injection is critical:

**Environment injection** (secure):
- Token placed in an environment variable at process spawn time
- The runtime process reads it via `std::env::var("UNIMATRIX_SESSION_TOKEN")`
- The LLM does **not** see the token in its context window
- The runtime attaches the token to MCP requests programmatically

**Context injection** (insecure):
- Token placed in the system prompt or tool description
- The LLM "knows" the token
- The LLM can be manipulated to leak or misuse it
- Prompt injection can extract the token

**The critical security boundary**: The LLM generates the **intent** ("store this knowledge about authentication patterns"), while the runtime attaches the **identity proof** (the session token). These two concerns flow through separate channels.

### 5.3 Claude Code's Agent Spawning

Claude Code spawns subagents with:
- Custom system prompts
- Specific tool access permissions
- Independent context windows
- Environment variable inheritance

Claude Code currently inherits the developer's identity. As noted by Token Security: "Claude Code doesn't come with its own tightly scoped service account or operate inside a neatly sandboxed runtime by default, instead inheriting the developer's identity."

This creates both a risk and an opportunity:
- **Risk**: All subagents inherit the same identity (the developer's)
- **Opportunity**: Claude Code **could** generate per-subagent tokens at spawn time

### 5.4 Token Structure for Orchestrator-Mediated Auth

```
+---------------------------------------------------+
| Orchestrator Session Token                         |
+---------------------------------------------------+
| agent_id:      "uni-architect"                     |
| trust_level:   Internal                            |
| capabilities:  [Read, Write, Search]               |
| spawned_by:    "claude-code-session-abc123"        |
| created_at:    1740400000                          |
| expires_at:    1740403600 (1 hour)                 |
| allowed_tools: ["context_store", "context_search"] |
| nonce:         "f7a3b9c1d2e4"                      |
| signature:     HMAC-SHA256(key, payload)           |
+---------------------------------------------------+
```

### 5.5 Limitations

1. **Requires orchestrator cooperation**: Claude Code (or the hosting tool) must implement token generation. This is not currently available.
2. **Bash tool leakage**: An LLM with Bash tool access can read environment variables via `echo $UNIMATRIX_SESSION_TOKEN`. Mitigated by:
   - Using opaque tokens (the LLM seeing the token gains nothing)
   - Token binding to the process (token only valid from the originating PID)
   - Token rotation per request
3. **Single-orchestrator trust**: The orchestrator is the root of trust. If compromised, all tokens are suspect.

---

## 6. Opaque Token and Reference Token Patterns

### 6.1 Why Opaque Tokens Matter for LLM Security

Self-contained tokens (JWTs) encode identity information in the token itself. An LLM that sees a JWT can:
- Read the claims (agent_id, capabilities, trust_level)
- Understand the token's structure and purpose
- Potentially craft modified tokens (though signature verification prevents this)
- Leak the token and its contents through output

Opaque tokens (random strings like `a7f3b2c1-9d4e-8f6a-0b5c-3e2d1a4f7c8b`) provide:
- **No information leakage**: The LLM sees a random string, nothing more
- **Server-side resolution**: Only Unimatrix can map the token to an identity
- **Instant revocation**: Delete the server-side mapping, token becomes invalid
- **No forgery surface**: Without knowing the mapping, the LLM cannot craft a valid token

### 6.2 The Phantom Token Pattern

The Phantom Token pattern (from Curity) combines opaque external tokens with JWT internal tokens:

```
+----------+     opaque token      +---------+     JWT        +--------+
|  Client  | -------------------> | Gateway | ------------> |  API   |
| (agent)  |  "a7f3b2c1..."      | (proxy) |  {claims...}  | (Unim.)|
+----------+                      +---------+               +--------+
                                       |
                                  Introspect
                                       |
                                  +----v----+
                                  |  Token  |
                                  | Service |
                                  +---------+
```

Applied to Unimatrix:
1. The orchestrator generates an opaque token
2. The MCP client sends the opaque token with each request
3. Unimatrix resolves the opaque token to identity server-side
4. The LLM never sees the resolved identity

### 6.3 One-Time Tokens (OTT) and Token Rotation

For maximum security, tokens can be single-use:

```
Request 1: token_a -> Identity resolved -> Response includes token_b
Request 2: token_b -> Identity resolved -> Response includes token_c
Request 3: token_a -> REJECTED (already used)
```

Each tool call consumes the current token and receives a fresh one. This prevents:
- **Replay attacks**: A captured token is immediately invalid
- **Token sharing**: Each token is bound to a specific interaction sequence
- **Context window leakage**: Even if the LLM leaks a token, it is already consumed

**Implementation in Unimatrix:**

```rust
struct TokenStore {
    // Maps opaque token -> (identity, next_token)
    tokens: HashMap<String, (ResolvedIdentity, Option<String>)>,
}

impl TokenStore {
    fn consume(&mut self, token: &str) -> Option<(ResolvedIdentity, String)> {
        if let Some((identity, _)) = self.tokens.remove(token) {
            let next_token = generate_opaque_token();
            self.tokens.insert(next_token.clone(), (identity.clone(), None));
            Some((identity, next_token))
        } else {
            None // Token already consumed or never existed
        }
    }
}
```

### 6.4 Token Binding

Tokens can be bound to additional context to prevent misuse:

| Binding Type | Mechanism | Prevents |
|-------------|-----------|----------|
| Process binding | Token valid only from PID X | Token theft via another process |
| Session binding | Token linked to MCP session ID | Cross-session replay |
| Time binding | Token expires after N seconds | Long-term token capture |
| Sequence binding | Token must follow token_n-1 | Out-of-order replay |
| Tool binding | Token valid only for tool X | Capability escalation |

### 6.5 Feasibility for Unimatrix

Opaque tokens are highly feasible for Unimatrix:
- Server-side token store can use the existing redb database (new table)
- Token generation uses `ring` crate's secure random number generator
- No external dependencies required
- Compatible with stdio transport (token passed in tool params or MCP metadata)
- Performance: HashMap lookup is O(1), negligible overhead

---

## 7. Mutual Authentication Without LLM Involvement

### 7.1 Process-Level Authentication via Unix Sockets

For local-first deployments, Unix domain sockets provide kernel-level process identification:

```
MCP Client Process (PID 1234, UID 1000)
    |
    +-- connect() --> Unix Socket (/tmp/unimatrix.sock)
                          |
                     SO_PEERCRED
                          |
                    Unimatrix Server
                    reads: pid=1234, uid=1000, gid=1000
```

`SO_PEERCRED` returns a `ucred` structure containing the peer's PID, UID, and GID. This is set by the kernel during `connect()` and cannot be forged by the connecting process.

**Rust implementation using the `nix` crate:**

```rust
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};

fn get_peer_identity(socket_fd: RawFd) -> Result<(u32, u32, u32), Error> {
    let cred = getsockopt(socket_fd, PeerCredentials)?;
    Ok((cred.pid() as u32, cred.uid(), cred.gid()))
}
```

Or using Rust's standard library on Linux nightly:

```rust
use std::os::linux::net::UnixSocketExt;

fn get_peer_identity(stream: &UnixStream) -> io::Result<(u32, u32, u32)> {
    let cred = stream.peer_cred()?;
    Ok((cred.pid, cred.uid, cred.gid))
}
```

**Limitation for Unimatrix**: MCP stdio transport uses stdin/stdout pipes, not Unix sockets. Switching to Unix socket transport would require:
- Custom MCP transport implementation (not in the spec for stdio)
- MCP client support for Unix socket connections
- Loss of compatibility with standard MCP client configurations

### 7.2 Environment-Injected HMAC Authentication

A middle ground: the orchestrator injects an HMAC key into the environment, and the runtime process (not the LLM) computes HMAC signatures:

```
Orchestrator:
  1. Generate HMAC key K
  2. Set env UNIMATRIX_HMAC_KEY=K
  3. Spawn agent process

Agent Runtime (code, not LLM):
  4. Read K from env
  5. For each tool call:
     a. LLM generates intent: {tool: "context_store", content: "..."}
     b. Runtime computes: sig = HMAC-SHA256(K, canonical(intent))
     c. Runtime sends: {intent, signature: sig}

Unimatrix:
  6. Look up K for this session
  7. Recompute: expected = HMAC-SHA256(K, canonical(intent))
  8. Verify: sig == expected
  9. If valid: extract identity from session, process request
```

**Key separation of concerns:**
- The LLM decides **what** to do (generate the tool call content)
- The runtime proves **who** is doing it (compute HMAC signature)
- The LLM cannot forge the signature because it does not control the HMAC computation
- Even if the LLM reads the HMAC key via Bash tool, it cannot inject the signature into the MCP protocol layer (the runtime does that)

### 7.3 The Dual-Channel Pattern

Extend the separation to use two distinct channels:

```
+-------+     Channel 1: Intent (LLM-controlled)      +-----------+
| Agent |  ----------------------------------------->  | Unimatrix |
|Process|     {tool: "context_store", content: "..."}  |           |
|       |                                              |           |
|       |     Channel 2: Auth (Runtime-controlled)     |           |
|       |  ----------------------------------------->  |           |
+-------+     {session_token: "xyz", hmac: "abc123"}   +-----------+
```

In MCP stdio transport, both channels share the same JSON-RPC message. But the architectural separation is maintained by having the runtime inject authentication fields that the LLM never sees or controls:

```json
{
  "method": "tools/call",
  "params": {
    "name": "context_store",
    "arguments": {
      "content": "LLM-generated content here",
      "topic": "authentication"
    },
    "_auth": {
      "session_token": "opaque-token-value",
      "timestamp": 1740400000,
      "hmac": "computed-by-runtime-not-llm"
    }
  }
}
```

The `_auth` field is injected by the MCP client runtime after the LLM generates the tool call. The LLM's output contains only the semantic arguments; the runtime adds authentication data.

### 7.4 Feasibility for Unimatrix

| Mechanism | Feasibility | Why |
|-----------|-------------|-----|
| Unix socket SO_PEERCRED | Low | Requires non-standard transport; breaks MCP client compatibility |
| HMAC env injection | Medium | Requires orchestrator cooperation; works with existing transport |
| Dual-channel in JSON-RPC | Medium-High | Works within MCP spec (tool params are extensible); requires MCP client changes for _auth injection |
| Runtime-injected metadata | High | Can use MCP `meta` field or custom parameter prefixes |

---

## 8. Intent-Based Access Control

### 8.1 The "Digitally Signed Intent" Concept

The user's intuition aligns with capability-based security: rather than asking "who are you?" (identity) and then checking permissions, ask "what are you authorized to do?" (capability) with cryptographic proof.

An intent-based system works as follows:

```
+----------------+
| Orchestrator   |
| (trusted)      |
+-------+--------+
        |
   Create signed intent:
   "Agent X may store one entry
    in topic 'auth-patterns'
    for the next 5 minutes"
        |
   Sign with orchestrator key
        |
+-------v--------+       +------------------+
| Agent Process   | ----> | Unimatrix        |
| carries signed  |       | verifies:        |
| intent warrant  |       | 1. Valid signature|
|                 |       | 2. Not expired    |
|                 |       | 3. Scope matches  |
|                 |       |    actual request  |
+-----------------+       +------------------+
```

The LLM cannot:
- Forge the signature (it does not have the signing key)
- Widen the scope (the intent specifies exact permissions)
- Extend the time (expiration is in the signed payload)
- Use the warrant for a different operation (scope is bound)

### 8.2 Tenuo: Capability Warrants for AI Agents

Tenuo is an open-source Rust library implementing exactly this pattern. Key technical details:

**Warrant structure:**
```
Warrant {
    tool: "context_store",          // Which tool is authorized
    constraints: {                   // What parameters are allowed
        topic: Pattern("auth-*"),    // Wildcard-based
        category: OneOf(["convention", "decision"]),
        content: Subpath("/valid/path")
    },
    ttl: Duration::from_secs(300),  // 5-minute expiry
    issuer_signature: Ed25519Sig,   // Cryptographic proof
    holder_binding: PublicKey,      // Proof-of-possession
}
```

**Subtractive delegation:**
```
Root Warrant (control plane):
  tools: ["context_store", "context_search", "context_lookup", "context_get"]
  topics: *
  ttl: 1 hour

Orchestrator attenuates for task:
  tools: ["context_store", "context_search"]
  topics: "auth-*"
  ttl: 10 minutes

Worker receives narrowest scope:
  tools: ["context_store"]
  topics: "auth-patterns"
  ttl: 5 minutes
```

Capabilities can only shrink through delegation, never expand. This is enforced cryptographically via chained HMAC construction (similar to macaroons).

**Performance:** ~27 microseconds for warrant verification. Offline verification (no network calls required).

**Prompt injection resistance:** Even if an agent is prompt-injected, the warrant constrains what it can do. The injected instructions cannot escape the capability bounds because the bounds are cryptographically enforced, not prompt-enforced.

### 8.3 Macaroons: Attenuating Bearer Tokens

Macaroons (Google, 2014) are bearer tokens with cryptographically chained caveats:

```
Root Macaroon:
  identifier: "session-12345"
  location: "unimatrix"
  signature: HMAC(root_key, identifier)

Add caveat "tool = context_store":
  signature: HMAC(prev_signature, "tool = context_store")

Add caveat "topic = auth-patterns":
  signature: HMAC(prev_signature, "topic = auth-patterns")

Add caveat "expires < 2026-02-24T12:00:00Z":
  signature: HMAC(prev_signature, "expires < 2026-02-24T12:00:00Z")
```

Properties:
- Caveats can only be **added**, never removed (chained HMAC ensures this)
- Each caveat **restricts** the token further
- Third-party caveats enable delegation to external verifiers
- Rust implementations exist: `macaroon-rs/macaroon` crate

### 8.4 Handling Dynamic Intent Escalation

A challenge: what happens when an agent discovers mid-task that it needs additional access?

**Pattern: Escalation Request**

```
Agent (holding narrow warrant):
  "I need to also search topic 'crypto-patterns'"

  Cannot widen its own warrant (cryptographically impossible)
  Must request escalation from orchestrator

Orchestrator:
  1. Receives escalation request
  2. Evaluates against policy
  3. If approved: issues new warrant with wider scope
  4. Agent receives new warrant for remaining task

Unimatrix:
  Sees two warrants for same session:
  - Original (narrow, partially consumed)
  - Escalation (wider, fresh TTL)
  Both independently verifiable
```

This mirrors how capability-based operating systems handle authority escalation: the process cannot self-elevate, but can request elevation from a higher authority.

### 8.5 Related Work

**XACML (eXtensible Access Control Markup Language):**
XML-based policy language for attribute-based access control. Overly complex for Unimatrix's use case but provides architectural patterns:
- Policy Decision Point (PDP) evaluates rules
- Policy Enforcement Point (PEP) intercepts requests
- Policy Information Point (PIP) provides context

**Google Zanzibar:**
Global authorization system using relation tuples: `(object, relation, user)`. Several implementations: Authzed (SpiceDB), Auth0 FGA, Permify. Designed for "check if user X has permission Y on object Z" at massive scale. Relevant pattern but enterprise-scale, not local-first.

**OpenID Foundation AIIM (2025):**
The OpenID Foundation created an "Artificial Intelligence Identity Management" Community Group in 2025, publishing a whitepaper on identity management for agentic AI. Key finding: "Existing standards only partially cover the emerging needs of AI agents, particularly around delegated authority, agent authentication, propagation and delegation of authorization between agents, and agent discovery and governance."

### 8.6 Feasibility for Unimatrix

Intent-based access control is the most promising pattern for Unimatrix:
- **Tenuo is Rust-native** with ~27us verification overhead
- **Offline verification** matches local-first architecture (no network calls)
- **Subtractive delegation** naturally maps to Unimatrix's trust hierarchy (System > Privileged > Internal > Restricted)
- **Tool-scoped warrants** align with MCP tool call granularity
- **Topic/category constraints** match existing `allowed_topics` and `allowed_categories` fields in `AgentRecord`

---

## 9. Multi-Layer Defense Architecture

### 9.1 The Security Onion for AI Agents

No single authentication mechanism is sufficient. Each layer addresses different attack vectors:

```
+-----------------------------------------------------------+
| Layer 0: Process Identity (OS-level)                       |
|   Who started this process? What binary is it?             |
+-----------------------------------------------------------+
| Layer 1: Session Authentication (Orchestrator-mediated)    |
|   What session token was injected at spawn time?           |
+-----------------------------------------------------------+
| Layer 2: Capability Warrants (Intent-based)                |
|   What specific operations are authorized for this task?   |
+-----------------------------------------------------------+
| Layer 3: Request Validation (Content-level)                |
|   Does this specific request comply with policy?           |
+-----------------------------------------------------------+
| Layer 4: Behavioral Monitoring (Anomaly detection)         |
|   Is this agent behaving consistently with its history?    |
+-----------------------------------------------------------+
| Layer 5: Audit Trail (Post-hoc accountability)             |
|   Can we reconstruct and verify what happened?             |
+-----------------------------------------------------------+
```

### 9.2 Defense in Depth: Failure Modes

| Layer | Failure Mode | Impact if Only Layer | Impact with Other Layers |
|-------|-------------|---------------------|-------------------------|
| Process identity | Spoofed PID (requires root) | Full compromise | Blocked by session token |
| Session token | Token leaked via LLM output | Session hijacking | Blocked by capability scope |
| Capability warrant | Warrant too broad | Over-permissioned actions | Caught by behavioral monitoring |
| Request validation | Bypass via edge case | Invalid data stored | Caught by audit trail |
| Behavioral monitoring | Novel attack pattern | Undetected misuse | Contained by capability limits |
| Audit trail | Log tampering | Unaccountable actions | Append-only log prevents |

### 9.3 Microsoft FIDES: Information Flow Control

Microsoft's FIDES (Flow Integrity Deterministic Enforcement System) provides a complementary approach: deterministic prevention of prompt injection through information flow labels.

Key mechanism: track **confidentiality** and **integrity** labels on all data flowing through the agent system. Tool results from untrusted sources get low-integrity labels. The planner deterministically prevents low-integrity data from influencing high-integrity decisions (like tool selection or parameter construction).

FIDES achieved 0 successful policy-violating injections in the AgentDojo benchmark suite.

Relevance to Unimatrix: content stored via `context_store` could carry integrity labels based on the storing agent's trust level. Content from Restricted agents gets low-integrity labels; content from System gets high-integrity labels. Query results include integrity metadata so consuming agents can make trust decisions.

### 9.4 Cost/Complexity Tradeoffs

| Layer | Implementation Cost | Runtime Cost | Security Value |
|-------|-------------------|-------------|---------------|
| Process identity | Low (OS API) | Negligible | Medium (local attacks only) |
| Session token | Medium (token management) | ~1us (lookup) | High |
| Capability warrants | Medium-High (crypto) | ~27us (verify) | Very High |
| Request validation | Already implemented (vnc-002) | ~10us | Medium |
| Behavioral monitoring | High (ML/heuristics) | Variable | Medium-High |
| Audit trail | Already implemented (vnc-001) | ~100us (write) | Medium (post-hoc) |

For Unimatrix v0.x: Layers 0-3 and 5 are recommended. Layer 4 (behavioral monitoring) can wait for the Cortical phase (crt-*).

---

## 10. Prototype Architectures for Unimatrix

### 10.1 Architecture A: "Guardian" --- Opaque Token with Server-Side Resolution

The simplest architecture that removes the LLM from the identity loop.

```
 SETUP PHASE (before LLM starts):
 =================================

 +-------------------+                    +-------------------+
 |   Orchestrator    |  1. Request token  |   Unimatrix       |
 | (Claude Code)     | -----------------> |   MCP Server      |
 |                   |                    |                   |
 |                   | <----------------- |                   |
 |                   |  2. Return opaque  |  Token Store:     |
 |                   |     token T        |  T -> {           |
 |                   |                    |    agent: "arch",  |
 +--------+----------+                    |    trust: Internal,|
          |                               |    caps: [R,W,S], |
  3. Set env var                          |    expires: +1h   |
     UNIMATRIX_TOKEN=T                    |  }                |
          |                               +-------------------+
 +--------v----------+
 |   Agent Process   |
 |   (LLM inside)    |
 +-------------------+

 REQUEST PHASE (per tool call):
 ===============================

 +-------------------+                    +-------------------+
 |   Agent Process   |                    |   Unimatrix       |
 |                   |                    |                   |
 |  LLM generates:   |                    |                   |
 |  {tool: store,    |                    |                   |
 |   content: "..."}|                    |                   |
 |                   |                    |                   |
 |  Runtime reads T  |                    |                   |
 |  from env, adds   |                    |                   |
 |  to request:      |                    |                   |
 |                   |  4. MCP call with  |                   |
 |  {tool: store,    | -----------------> | 5. Look up T in   |
 |   content: "...", |     T in _meta     |    token store    |
 |   _meta: {        |                    | 6. Resolve to     |
 |     token: T      |                    |    identity       |
 |   }}              |                    | 7. Check caps     |
 |                   |                    | 8. Process or     |
 |                   | <----------------- |    reject         |
 |                   |  9. Response       |                   |
 +-------------------+                    +-------------------+
```

**Strengths:**
- Simple to implement (HashMap-based token store)
- No cryptographic overhead beyond token generation
- Compatible with existing MCP stdio transport
- Opaque token reveals nothing if leaked
- Instant revocation (delete from store)

**Weaknesses:**
- Requires orchestrator to request token before spawning agent
- Token store is server-side state (must persist across restarts)
- LLM can potentially read env var via Bash tool (mitigated by opacity)
- No delegation mechanism (each agent gets a flat token)
- No scope narrowing at task level

**LLM Spoofing Resistance:** HIGH --- the LLM does not choose the token; the runtime reads it from env.
**Prompt Injection Resistance:** HIGH --- token is not in the LLM's context window.
**Confused Deputy Resistance:** MEDIUM --- token identifies the agent but does not scope operations.
**Implementation Complexity:** LOW --- ~200 lines of Rust.
**Performance Impact:** NEGLIGIBLE --- HashMap lookup.

### 10.2 Architecture B: "Warrant" --- Tenuo-Style Capability Tokens

Full capability-based authentication with cryptographic warrants.

```
 SETUP PHASE:
 =============

 +-------------------+                    +-------------------+
 |   Control Plane   |  1. Generate       |   Unimatrix       |
 | (human or system) |     signing        |   MCP Server      |
 |                   |     keypair        |                   |
 |                   | -----------------> | 2. Store public   |
 |                   |     public key     |    key for        |
 |                   |                    |    verification   |
 +--------+----------+                    +-------------------+
          |
  3. Store private key
     securely
          |
 +--------v----------+
 |   Orchestrator    |
 | (Claude Code)     |
 |                   |
 |  4. Create root   |
 |     warrant:      |
 |     tools: [*]    |
 |     ttl: 1 hour   |
 |                   |
 |  5. Attenuate for |
 |     specific task:|
 |     tools: [store]|
 |     topic: auth-* |
 |     ttl: 10 min   |
 |                   |
 |  6. Inject as env:|
 |     WARRANT=<b64> |
 +--------+----------+
          |
 +--------v----------+
 |   Agent Process   |
 +-------------------+

 REQUEST PHASE:
 ===============

 +-------------------+                    +-------------------+
 |   Agent Process   |                    |   Unimatrix       |
 |                   |                    |                   |
 |  LLM: {store,     |                    |                   |
 |   content: "...", |                    |                   |
 |   topic: "auth-x"}|                    |                   |
 |                   |                    |                   |
 |  Runtime reads    |                    |                   |
 |  WARRANT from env |  7. MCP call +    |                   |
 |  attaches to req  | ------warrant---> | 8. Verify sig     |
 |                   |                    | 9. Check TTL      |
 |                   |                    | 10. Match tool    |
 |                   |                    | 11. Match topic   |
 |                   |                    |     constraint    |
 |                   |                    | 12. Process if    |
 |                   | <----------------- |     all pass      |
 +-------------------+                    +-------------------+

 ESCALATION:
 ============

 +-------------------+                    +-------------------+
 |   Agent Process   |                    |   Orchestrator    |
 |                   |  13. "I also need  |                   |
 |                   | ------search"----> | 14. Evaluate      |
 |                   |                    |     policy        |
 |                   | <----------------- | 15. Issue new     |
 |                   |  new warrant:      |     warrant       |
 |                   |  tools: [store,    |     (attenuated)  |
 |                   |    search]         |                   |
 |                   |  topic: auth-*     |                   |
 +-------------------+                    +-------------------+
```

**Strengths:**
- Cryptographically enforced capability boundaries
- Subtractive delegation (capabilities can only narrow)
- Tool-scoped and topic-scoped authorization
- TTL-based expiry (no revocation infrastructure needed)
- Proof-of-possession prevents stolen warrant abuse
- Tenuo crate available in Rust (~27us verification)
- Offline verification (no network calls)

**Weaknesses:**
- Higher implementation complexity (key management, warrant construction)
- Requires orchestrator to understand Unimatrix's tool/topic model
- Escalation requires round-trip to orchestrator
- Key distribution problem (how does Unimatrix get the signing public key?)
- Larger token size (warrant payload + signature + constraints)

**LLM Spoofing Resistance:** VERY HIGH --- LLM cannot forge Ed25519 signatures.
**Prompt Injection Resistance:** VERY HIGH --- even a fully compromised LLM is bounded by warrant scope.
**Confused Deputy Resistance:** HIGH --- warrant specifies exact operations, tools, and topics.
**Implementation Complexity:** MEDIUM-HIGH --- ~500-800 lines + Tenuo dependency.
**Performance Impact:** LOW --- 27us per verification.

### 10.3 Architecture C: "Citadel" --- Multi-Layer Defense-in-Depth

Combines process identity, opaque sessions, capability warrants, and behavioral monitoring.

```
 +-----------------------------------------------------------------+
 |                     CITADEL ARCHITECTURE                         |
 +-----------------------------------------------------------------+

 LAYER 0: PROCESS IDENTITY
 ==========================
 Unimatrix on startup:
   - Records own PID
   - For stdio: records parent PID (the MCP client)
   - For future Unix socket: would use SO_PEERCRED
   - Maps PID -> known orchestrator binary hash (optional)

 LAYER 1: SESSION AUTHENTICATION
 ================================
 On MCP initialize:
   - Orchestrator sends session_token in clientInfo or first message
   - Token is opaque (random 256-bit)
   - Unimatrix resolves token to session identity:

     Session Store (redb table):
     +------------------+---------------------------+
     | Token            | Session                   |
     +------------------+---------------------------+
     | "a7f3b2c1..."    | orchestrator: "claude-code"|
     |                  | spawned_agents: ["arch"]   |
     |                  | created_at: 1740400000     |
     |                  | expires_at: 1740403600     |
     |                  | parent_pid: 1234           |
     +------------------+---------------------------+

 LAYER 2: CAPABILITY WARRANTS
 =============================
 Per tool call:
   - Agent includes warrant in request _meta
   - Warrant specifies: tools, topics, categories, TTL
   - Unimatrix verifies:
     a. Warrant signature (Ed25519 or HMAC chain)
     b. Warrant not expired
     c. Requested operation within warrant scope
     d. Warrant session matches Layer 1 session
   - If no warrant provided: fall back to session-level capabilities

 LAYER 3: REQUEST VALIDATION
 ============================
 Already implemented (vnc-002):
   - Content scanning (~35 regex patterns)
   - Category allowlist validation
   - Input sanitization
   - Token count validation

 LAYER 4: BEHAVIORAL MONITORING (future: crt-*)
 ================================================
   - Track per-agent request patterns
   - Alert on anomalies:
     - Agent suddenly requesting different topics
     - Burst of write operations
     - Requests outside normal time windows
   - Does NOT block (advisory for human review)

 LAYER 5: AUDIT TRAIL
 =====================
 Already implemented (vnc-001):
   - Append-only AUDIT_LOG table
   - Monotonic u64 IDs
   - Cross-session continuity
   - Records: agent_id, action, timestamp, details

 COMPOSITE FLOW:
 ================

 +-----------+     +----+----+----+----+----+     +----------+
 |  Agent    | --> | L0 | L1 | L2 | L3 | L5| --> | Process  |
 |  Request  |     | PID|Sess|Wrnt|Val |Aud|     | Request  |
 +-----------+     +--+-+--+-+--+-+--+-+--+-+     +----------+
                      |    |    |    |    |
                    pass  pass pass pass log
                      |    |    |    |    |
                  If ANY layer REJECTS --> Error + Audit Entry
```

**Data flow for a single tool call:**

```
 Agent Process                                Unimatrix Server
 =============                                ================

 LLM generates:
   tool: "context_store"
   content: "Auth pattern X..."
   topic: "auth-patterns"
       |
 Runtime injects:
   _meta.session_token: "a7f3b2c1..."
   _meta.warrant: <base64-encoded>
   _meta.timestamp: 1740400123
   _meta.request_id: "req-456"
       |
       +-------- MCP JSON-RPC --------+
                                      |
                                 L0: getppid() == expected?
                                      | yes
                                 L1: token "a7f3b2c1..." -> session?
                                      | yes, session for "architect"
                                 L2: warrant allows context_store
                                     on topic "auth-patterns"?
                                      | yes, within scope
                                 L3: content passes scanning?
                                     topic in category allowlist?
                                      | yes
                                 L5: write audit entry
                                      |
                                 Process: store entry
                                      |
       +-------- Response ------------+
       |
 Agent receives result
```

**Strengths:**
- Any single layer can fail without system compromise
- Graceful degradation: if warrants not available, fall back to session tokens
- If session tokens not available, fall back to process identity
- Combines "who you are" (session) with "what you can do" (warrant)
- Built on existing Unimatrix infrastructure (audit, validation)
- Incremental deployment: add layers over time

**Weaknesses:**
- Highest implementation complexity
- Requires orchestrator cooperation for full effectiveness
- Performance overhead of running all layers (but each is fast)
- More configuration surface for operators
- Risk of over-engineering for current threat model

**LLM Spoofing Resistance:** VERY HIGH (multiple independent barriers).
**Prompt Injection Resistance:** VERY HIGH (warrant scope + content scanning).
**Confused Deputy Resistance:** VERY HIGH (warrant scoping + behavioral monitoring).
**Implementation Complexity:** HIGH --- ~1000-1500 lines across multiple modules.
**Performance Impact:** LOW --- cumulative ~50-100us per request.

---

## 11. Comparative Evaluation

### 11.1 Architecture Comparison Matrix

| Criterion | A: Guardian | B: Warrant | C: Citadel |
|-----------|:-----------:|:----------:|:----------:|
| **LLM Spoofing Resistance** | High | Very High | Very High |
| **Prompt Injection Resistance** | High | Very High | Very High |
| **Confused Deputy Resistance** | Medium | High | Very High |
| **Replay Attack Resistance** | Medium (if OTT) | High (TTL) | Very High |
| **Delegation Support** | No | Yes (subtractive) | Yes |
| **Scope Narrowing** | No | Yes (per-warrant) | Yes |
| **Offline Verification** | No (server lookup) | Yes | Partial |
| **Implementation Complexity** | Low (~200 LOC) | Medium-High (~600 LOC) | High (~1200 LOC) |
| **Orchestrator Dependency** | Medium | High | High (graceful degradation) |
| **MCP Compatibility** | High | Medium | Medium |
| **Performance Impact** | Negligible | Low (27us) | Low (50-100us) |
| **Incremental Deployability** | Yes | Yes | Yes (by design) |
| **Existing Rust Crates** | ring (RNG) | tenuo, macaroon | Multiple |
| **Works Without Orchestrator Changes** | Partially | No | Partially (degraded) |

### 11.2 Threat Model Coverage

| Threat | A: Guardian | B: Warrant | C: Citadel |
|--------|:-----------:|:----------:|:----------:|
| LLM claims false agent_id | Blocked | Blocked | Blocked |
| Prompt injection escalation | Blocked | Blocked | Blocked |
| Stolen token replay | Partial (OTT) | Blocked (TTL+binding) | Blocked |
| Confused deputy (wrong scope) | Not addressed | Blocked (warrant scope) | Blocked |
| Orchestrator compromise | Compromised | Compromised | Detected (behavioral) |
| Token leakage via LLM output | Mitigated (opaque) | Mitigated (opaque+bound) | Mitigated |
| Cross-session token reuse | Blocked | Blocked | Blocked |
| Capability escalation | Not addressed | Blocked (subtractive) | Blocked |

### 11.3 Implementation Path Analysis

**Phase 1 (vnc-003 or vnc-004): Architecture A (Guardian)**
- Replace claims-based `agent_id` extraction with opaque token resolution
- Add `SESSION_TOKENS` redb table
- Implement token generation API (callable by orchestrator before agent spawn)
- Backward-compatible: agents without tokens default to Restricted
- Estimated effort: 2-3 days

**Phase 2 (vnc-005 or later): Add Warrant Layer (A+B hybrid)**
- Add Tenuo dependency or implement simplified warrant verification
- Warrants optional: if present, scope-check; if absent, use session capabilities
- Orchestrators that support warrants get fine-grained control
- Estimated effort: 3-5 days

**Phase 3 (future): Full Citadel**
- Add process identity layer (PID verification)
- Add behavioral monitoring (Cortical phase)
- Complete defense-in-depth stack
- Estimated effort: 5-8 days

---

## 12. Recommended Architecture

### 12.1 Recommendation: Phased Citadel (Architecture C) via Incremental Deployment

The recommended approach is to build toward Architecture C (Citadel) through incremental phases, starting with Architecture A (Guardian) as the foundation. This provides:

1. **Immediate security improvement**: Phase 1 eliminates the LLM-claims-identity vulnerability with minimal effort
2. **Progressive hardening**: Each phase adds a defense layer without breaking existing functionality
3. **Graceful degradation**: Agents without tokens still work (as Restricted), so existing integrations are not disrupted
4. **Eventual completeness**: The full Citadel architecture is reached over 2-3 feature cycles

### 12.2 Phase 1 Implementation Sketch (Guardian)

**New module: `session.rs`**

```
// Core types
struct SessionToken {
    token: String,         // 256-bit random, hex-encoded
    agent_id: String,      // Resolved identity
    trust_level: TrustLevel,
    capabilities: Vec<Capability>,
    created_at: u64,
    expires_at: u64,
    parent_pid: Option<u32>,
}

// New redb table
const SESSION_TOKENS: TableDefinition<&str, &[u8]> = ...;

// API
fn create_session(registry, agent_id, ttl) -> SessionToken
fn resolve_session(token: &str) -> Option<ResolvedIdentity>
fn revoke_session(token: &str) -> bool
```

**Modified identity resolution:**

```
// Before (current):
fn resolve_identity(registry, agent_id_from_tool_params) -> ResolvedIdentity

// After (Phase 1):
fn resolve_identity(registry, request) -> ResolvedIdentity {
    // 1. Check for session token in request _meta
    if let Some(token) = request.meta.get("session_token") {
        return session_store.resolve(token)?;  // Server-side resolution
    }
    // 2. Fallback: treat as anonymous/restricted
    return ResolvedIdentity::restricted_anonymous();
    // NOTE: agent_id from tool params is IGNORED for identity
    //       (may still be used for audit logging)
}
```

### 12.3 Key Design Decisions

1. **agent_id in tool params becomes advisory, not authoritative**: Used for audit logging ("the LLM claims to be X") but never for capability resolution.

2. **No token = Restricted**: Backward compatibility is preserved. Agents without orchestrator support still work with minimal permissions.

3. **Token creation is an MCP tool**: The orchestrator calls a `session_create` tool (Admin capability required) before spawning subagents. Returns the opaque token.

4. **Token is injected via environment variable**: `UNIMATRIX_SESSION_TOKEN`. The MCP client runtime reads it and includes it in every request.

5. **Tokens stored in redb**: Persists across Unimatrix restarts. Expired tokens cleaned up on startup.

### 12.4 Migration Path from Current System

```
Current:    LLM -> agent_id param -> resolve_or_enroll -> capabilities
Phase 1:    Runtime -> session_token -> resolve_session -> capabilities
            LLM -> agent_id param -> audit log only (advisory)
Phase 2:    Runtime -> session_token + warrant -> verify_warrant -> scoped capabilities
Phase 3:    OS -> PID/UID + Runtime -> token + warrant -> full Citadel stack
```

### 12.5 What Unimatrix Can Do Without Orchestrator Changes

Even without Claude Code or Cursor implementing token injection:

1. **Expose `session_create` as an admin tool**: The human user (Privileged trust) can manually create tokens
2. **Configuration-based pre-registration**: Map known agent names to tokens in a config file
3. **First-connection token exchange**: During MCP `initialize`, Unimatrix generates a session token and returns it. The MCP client runtime can cache and reuse it.
4. **Environment variable at startup**: The user sets `UNIMATRIX_SESSION_TOKEN` when configuring the MCP server in their client's config (e.g., Claude Code's `mcp_servers` config includes environment variables).

Option 4 is the most practical near-term approach and requires zero orchestrator changes.

---

## 13. References

### Academic Papers

1. Hintermeier, M. et al. (2025). "AI Agents with Decentralized Identifiers and Verifiable Credentials." arXiv:2511.02841. https://arxiv.org/abs/2511.02841

2. Hosseinzadeh, S. et al. (2025). "A Novel Zero-Trust Identity Framework for Agentic AI: Decentralized Authentication and Fine-Grained Access Control." arXiv:2505.19301. https://arxiv.org/abs/2505.19301

3. Costa, M. and Kopf, B. (2025). "Securing AI Agents with Information-Flow Control (FIDES)." arXiv:2505.23643. https://arxiv.org/abs/2505.23643

4. Bhushan, B. (2025). "An Explainable Zero Trust Identity Framework for LLMs, AI." IJCA, Vol. 187, No. 46. https://www.ijcaonline.org/archives/volume187/number46/bhushan-2025-ijca-925777.pdf

5. Sapio, F. et al. (2025). "Design Patterns for Securing LLM Agents against Prompt Injections." arXiv:2506.08837. https://arxiv.org/abs/2506.08837

6. Birgisson, A. et al. (2014). "Macaroons: Cookies with Contextual Caveats for Decentralized Authorization in the Cloud." Google Research. https://research.google.com/pubs/archive/41892.pdf

7. OpenID Foundation (2025). "Identity Management for Agentic AI." arXiv:2510.25819. https://arxiv.org/abs/2510.25819

### Industry Publications and Blog Posts

8. HashiCorp (2025). "SPIFFE: Securing the Identity of Agentic AI and Non-Human Actors." https://www.hashicorp.com/en/blog/spiffe-securing-the-identity-of-agentic-ai-and-non-human-actors

9. HashiCorp (2025). "Before You Build Agentic AI, Understand the Confused Deputy Problem." https://www.hashicorp.com/en/blog/before-you-build-agentic-ai-understand-the-confused-deputy-problem

10. Aembit (2025). "Securing AI Agents and LLM Workflows Without Secrets." https://aembit.io/blog/securing-ai-agents-without-secrets/

11. Meta AI (2025). "Agents Rule of Two: A Practical Approach to AI Agent Security." https://ai.meta.com/blog/practical-ai-agent-security/

12. Willison, S. (2025). "New prompt injection papers: Agents Rule of Two and The Attacker Moves Second." https://simonwillison.net/2025/Nov/2/new-prompt-injection-papers/

13. Auth0 (2025). "Model Context Protocol (MCP) Spec Updates from June 2025." https://auth0.com/blog/mcp-specs-update-all-about-auth/

14. Auth0 (2025). "Access Control in the Era of AI Agents." https://auth0.com/blog/access-control-in-the-era-of-ai-agents/

15. Curity. "Securing APIs with The Phantom Token Approach." https://curity.io/resources/learn/phantom-token-pattern/

16. Medium/Abhilash (2025). "Intent-Based Access Control for Agentic AI." https://medium.com/@abhilashreddyc7/intent-based-access-control-for-agentic-ai-securing-the-next-chapter-in-cybersecurity-96544a94dea6

17. Token Security (2025). "A Year of Protecting Claude Code: The Identity Problem No One Was Ready For." https://www.token.security/blog/a-year-of-protecting-claude-code-the-identity-problem-no-one-was-ready-for

18. Stack Overflow (2026). "Is that allowed? Authentication and authorization in Model Context Protocol." https://stackoverflow.blog/2026/01/21/is-that-allowed-authentication-and-authorization-in-model-context-protocol

19. ISACA (2025). "The Looming Authorization Crisis: Why Traditional IAM Fails Agentic AI." https://www.isaca.org/resources/news-and-trends/industry-news/2025/the-looming-authorization-crisis-why-traditional-iam-fails-agentic-ai

20. GitGuardian (2025). "Workload and Agentic Identity at Scale: Insights From CyberArk's Workload Identity Day Zero." https://blog.gitguardian.com/workload-identity-day-zero-atlanta/

### Specifications and Standards

21. Model Context Protocol. "Security Best Practices." https://modelcontextprotocol.io/specification/draft/basic/security_best_practices

22. SPIFFE. "SPIFFE Concepts." https://spiffe.io/docs/latest/spiffe-about/spiffe-concepts/

23. SPIFFE. "X509-SVID Specification." https://spiffe.io/docs/latest/spiffe-specs/x509-svid/

24. OWASP (2025). "LLM01:2025 Prompt Injection." https://genai.owasp.org/llmrisk/llm01-prompt-injection/

25. OWASP (2025). "AI Agent Security Cheat Sheet." https://cheatsheetseries.owasp.org/cheatsheets/AI_Agent_Security_Cheat_Sheet.html

### Software and Libraries

26. Tenuo. "Capability Tokens for AI Agents." Rust crate. https://github.com/tenuo-ai/tenuo / https://crates.io/crates/tenuo

27. Microsoft. "FIDES: Flow Integrity Deterministic Enforcement System." https://github.com/microsoft/fides

28. macaroon-rs. "Rust implementation of macaroons." https://github.com/macaroon-rs/macaroon

29. ring. "Safe, fast, small crypto using Rust." https://docs.rs/ring/latest/ring/

30. nix. "Unix credentials and SO_PEERCRED." https://docs.rs/nix/latest/nix/sys/socket/struct.UnixCredentials.html

31. Anthropic. "Securely Deploying AI Agents." https://platform.claude.com/docs/en/agent-sdk/secure-deployment

32. Anthropic. "Create Custom Subagents - Claude Code Docs." https://code.claude.com/docs/en/sub-agents

33. tldrsec. "Prompt Injection Defenses." https://github.com/tldrsec/prompt-injection-defenses

34. ReversecLabs. "Design Patterns for Securing LLM Agents - Code Samples." https://github.com/ReversecLabs/design-patterns-for-securing-llm-agents-code-samples

### Conference Presentations

35. Sabath, M. and Iyer, M. (2025). "Who Let the Agents Out? Securing AI Workflows the Right Way." Open Source Security Con NA. https://tldrecap.tech/posts/2025/opensource-securitycon-na/agent-security-zero-trust-ai-oauth-spiffe/

---

## Appendix A: Glossary

| Term | Definition |
|------|-----------|
| **Claims-based identity** | Identity proven by the entity asserting "I am X" |
| **Infrastructure identity** | Identity proven by the infrastructure the entity runs within |
| **SVID** | SPIFFE Verifiable Identity Document --- short-lived cert with SPIFFE ID |
| **Warrant** | Cryptographic, scoped, time-bound authorization object (Tenuo) |
| **Macaroon** | Bearer token with chained HMAC caveats that can only be narrowed |
| **Opaque token** | Random string with no embedded information; resolved server-side |
| **Phantom token** | Pattern: opaque externally, JWT internally (gateway translates) |
| **SO_PEERCRED** | Linux socket option returning peer process PID/UID/GID |
| **Confused deputy** | Attack where a trusted agent performs unauthorized actions on behalf of an attacker |
| **Lethal trifecta** | Agent has: tool access + private data access + untrusted input exposure |
| **Subtractive delegation** | Capability model where delegated permissions can only shrink |
| **OTT** | One-Time Token --- consumed on use, prevents replay |
| **FIDES** | Flow Integrity Deterministic Enforcement System (Microsoft) |
| **IBAC** | Intent-Based Access Control |
| **NHI** | Non-Human Identity |

## Appendix B: Unimatrix-Specific Constraints

| Constraint | Impact on Architecture |
|-----------|----------------------|
| **Rust implementation** | Tenuo, ring, macaroon-rs all available. No FFI concerns. |
| **MCP stdio transport** | No OAuth 2.1. Identity must use env vars, tool params, or process-level mechanisms. |
| **Local-first** | No external auth server. All verification must be offline/local. |
| **Single-machine** | Unix socket SO_PEERCRED feasible if transport changes. PID verification always available. |
| **redb storage** | Token store, session store, warrant cache can all use existing redb database. |
| **Existing audit log** | Layer 5 (audit) is already implemented. |
| **Existing content scanning** | Layer 3 (request validation) is already implemented. |
| **Existing agent registry** | Can be evolved from claims-based to token-resolved identity. |
| **rmcp =0.16.0** | Must work within rmcp's tool handler API for token extraction. |

## Appendix C: Open Questions for Implementation

1. **Key management for warrants**: Where does the signing key live? Options: generated on first run and stored in redb, provided via environment variable, derived from a user-provided passphrase.

2. **Token injection mechanism**: Should tokens go in tool `_meta` fields, a custom MCP extension, or environment variables read by a custom MCP client wrapper?

3. **Backward compatibility period**: How long to support the claims-based `agent_id` parameter before removing it entirely?

4. **Multi-orchestrator support**: If multiple MCP clients connect (e.g., Claude Code + Cursor), each needs its own session. How are sessions namespaced?

5. **Token rotation strategy**: Per-request rotation (OTT) provides maximum security but adds complexity. Per-session with short TTL may be sufficient.

6. **Human user authentication**: The `"human"` agent currently has Privileged trust. With token-based auth, how does the human prove they are human? Options: token pre-configured at install time, interactive challenge, inherit from MCP client auth.
