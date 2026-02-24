# Cryptographic and Process-Level Agent Authentication Resistant to LLM Spoofing

**Research Document ASS-008 | February 2026**
**Status**: Research Complete

---

## Table of Contents

1. [Problem Statement](#1-problem-statement)
2. [Capability-Based Security / Object Capabilities (OCAP)](#2-capability-based-security--object-capabilities-ocap)
3. [Process-Level Identity Binding](#3-process-level-identity-binding)
4. [Trusted Intermediary / Proxy Patterns](#4-trusted-intermediary--proxy-patterns)
5. [Challenge-Response / Proof-of-Knowledge](#5-challenge-response--proof-of-knowledge)
6. [Hardware and TEE-Based Approaches](#6-hardware-and-tee-based-approaches)
7. [Signed Intent / Warrant Patterns](#7-signed-intent--warrant-patterns)
8. [Novel and Emerging Approaches (2025-2026)](#8-novel-and-emerging-approaches-2025-2026)
9. [Comparison Matrix](#9-comparison-matrix)
10. [Recommendations for Unimatrix](#10-recommendations-for-unimatrix)
11. [References](#11-references)

---

## 1. Problem Statement

Unimatrix is an MCP server (Model Context Protocol) that serves as a knowledge engine for multi-agent AI development orchestration. The current identity model is **self-reported**: agents pass an `agent_id` string in tool parameters. This creates a fundamental vulnerability:

- Any LLM can claim to be any agent
- A prompt-injected LLM can impersonate a higher-privilege agent
- There is no cryptographic binding between the claimed identity and the actual calling process
- The MCP protocol itself does not carry user/agent context from Host to Server [1]

The core requirement is authentication that LLMs **literally cannot spoof**, even under prompt injection. This means the authentication mechanism must operate **outside the LLM's control surface** -- the LLM must neither possess nor be able to fabricate the credential.

### Threat Model

| Threat | Description | Severity |
|--------|-------------|----------|
| **Identity Spoofing** | LLM claims a different `agent_id` | Critical |
| **Prompt Injection Escalation** | Injected prompt instructs LLM to claim privileged identity | Critical |
| **Token Exfiltration** | LLM is tricked into revealing/forwarding a capability token | High |
| **Replay Attack** | Previously valid credential reused after expiry/revocation | Medium |
| **Confused Deputy** | High-privilege agent tricked into acting on behalf of low-privilege request | Critical |
| **Credential Caching** | Agent stores credentials in memory/context that can be extracted | High |

---

## 2. Capability-Based Security / Object Capabilities (OCAP)

### 2.1 Theoretical Foundation

The object-capability model (OCAP) was first proposed by Dennis and Van Horn in 1966 and formalized for programming languages by Mark Miller in his 2006 PhD thesis, *Robust Composition: Towards a Unified Approach to Access Control and Concurrency Control* [2]. In OCAP, a **capability** is an unforgeable reference that simultaneously designates an object and authorizes operations on it. Authority flows only along explicit paths in the reference graph.

Key properties:
- **No ambient authority**: A process can only exercise authority it was explicitly granted
- **Principle of least privilege**: Capabilities can be attenuated (narrowed) but never amplified
- **Confused deputy prevention**: Authority is tied to the capability token, not to the identity of the caller

The E programming language (1997, Mark Miller et al.) was the first language built around capability security, where object references serve as capabilities. All interprocess communication is encrypted by the runtime, and lexical scoping limits the code that must be audited for security effects [3].

### 2.2 Cap'n Proto RPC

Cap'n Proto implements a capability-based RPC protocol based on CapTP (the distributed capability protocol from E). Capabilities are first-class types in Cap'n Proto's schema, embedded as special pointers within messages. The runtime enforces that only held capabilities can be exercised. Features include three-way vat introductions, promise pipelining, and SturdyRefs for persistent capabilities [4].

**Relevance to Unimatrix**: Cap'n Proto demonstrates that capability-based RPC can be practical and performant. However, MCP uses JSON-RPC over stdio/SSE, not Cap'n Proto's binary protocol, so the capability model would need to be layered on top rather than built into the transport.

### 2.3 Tenuo: Cryptographic Warrants for AI Agents

Tenuo is an open-source capability engine (Rust core, Python bindings) that provides cryptographically attenuated warrants for AI agent authorization. It represents the most directly applicable OCAP implementation for the LLM agent domain [5].

**Architecture**:
```
Control Plane --> Orchestrator --> Worker
Full scope   -->  Narrower    --> Narrowest
```

**Key properties**:
- **Warrants** are cryptographic, scoped, time-bound authorization objects
- **Monotonic attenuation**: capabilities only shrink through delegation chains, never expand
- **Proof-of-possession**: warrants are bound to signing keys; stolen tokens are useless without the key
- **Offline verification**: ~27 microseconds, no network calls required
- **Semantic constraints**: the system parses inputs "the way the system will" (addressing CVE-2025-66032 where validators and shells interpreted strings differently)

**Constraint types**:
- `Exact` (precise value), `Pattern` (wildcard), `Range` (numeric bounds)
- `OneOf` (enumerated), `Regex`, `CIDR` (network blocks)
- `UrlPattern`, `Subpath` (filesystem traversal protection)
- `UrlSafe` (SSRF prevention), `Shlex` (shell command parsing)
- `CEL` (Common Expression Language for custom logic)

**Integration with MCP**: Tenuo provides explicit MCP client integration. The guard decorator intercepts tool calls at execution time, not by parsing model outputs:

```python
from tenuo import configure, SigningKey, mint_sync, guard, Capability, Pattern

configure(issuer_key=SigningKey.generate(), dev_mode=True)

@guard(tool="send_email")
def send_email(to: str) -> str:
    return f"Sent to {to}"

with mint_sync(Capability("send_email", to=Pattern("*@company.com"))):
    send_email(to="alice@company.com")  # Allowed
    send_email(to="attacker@evil.com")  # Denied -- constraint violation
```

**LLM spoofing resistance**: HIGH. Tenuo gates tool calls at execution time, not at the LLM prompt level. If an agent is prompt-injected, the warrant's constraints still apply -- the LLM cannot fabricate a warrant with broader permissions because it would require the issuer's signing key. The proof-of-possession binding means stolen warrants are useless without the associated private key.

**Limitation**: Tenuo explicitly states it is "not a sandbox" and should be paired with containers or VMs for defense in depth. It also requires a control plane to mint root warrants, which adds deployment complexity.

### 2.4 Biscuit Authorization Tokens

Biscuit (Eclipse Foundation) is a specification for cryptographically verified authorization tokens with offline attenuation, using a Datalog-based authorization language [6].

**Key technical properties**:
- **Ed25519 cryptographic foundation**: any entity with the root public key can verify tokens
- **Datalog-based policy**: authorization rules expressed in a logic language, embedded in tokens
- **Offline attenuation**: holders generate derivative tokens with reduced permissions without server involvement
- **Revocation identifiers**: unique IDs that can reject both original and all downstream attenuated tokens
- **Rust reference implementation**: `biscuit-auth` crate on crates.io

**Comparison to Tenuo**: Biscuit is lower-level (general authorization token) while Tenuo is purpose-built for AI agents. Biscuit's Datalog policy language is more expressive but requires more expertise. Biscuit lacks Tenuo's semantic constraint parsing and LLM-specific integrations.

**LLM spoofing resistance**: HIGH. Same cryptographic guarantees as Tenuo -- the LLM cannot forge Ed25519 signatures.

### 2.5 Macaroons

Macaroons (Google Research, NDSS 2014) are bearer credentials with chained-HMAC construction enabling contextual caveats and decentralized delegation [7].

**Key properties**:
- Caveats attenuate permissions and are cryptographically chained
- Adding a caveat is computationally trivial; removing one is computationally infeasible
- Carry their own proof of authorization
- Verification without centralized database

**Comparison**: Macaroons are simpler than Biscuit (HMAC vs Ed25519, no Datalog) but lack holder binding. A leaked macaroon can be used by anyone, making them less suitable for LLM environments where token exfiltration via prompt injection is a threat.

**LLM spoofing resistance**: MEDIUM. Macaroons are bearer tokens -- if an LLM is tricked into forwarding one, the recipient can use it. No proof-of-possession binding.

### 2.6 Can an LLM Leak a Capability Token?

This is the critical question for any OCAP approach. Research shows that prompt injection attacks can cause LLMs to exfiltrate data through tool calls, including tokens and credentials [8][9]. Mitigations:

1. **Proof-of-possession binding** (Tenuo, Biscuit): The token alone is insufficient; the private key must sign the request. The key lives in the process runtime, not in the LLM's context window.

2. **Short TTLs**: Warrants that expire in seconds minimize replay windows.

3. **Opaque tokens**: The LLM never sees the raw token -- the middleware injects it at the transport layer (see Section 4).

4. **Output filtering**: Guardrails that scan LLM outputs for token-shaped strings before transmission.

5. **Holder-binding at the network layer**: The warrant is tied to a specific TLS session or Unix socket credential, making it non-transferable even if the raw bytes leak.

---

## 3. Process-Level Identity Binding

### 3.1 Unix Domain Sockets with SO_PEERCRED

`SO_PEERCRED` is a Linux socket option that returns the credentials (PID, UID, GID) of the peer process connected to a Unix domain socket. These credentials are retrieved from the kernel's internal management data at `connect(2)` time and **cannot be spoofed by the client process** [10].

```c
struct ucred {
    pid_t pid;  // Process ID
    uid_t uid;  // User ID
    gid_t gid;  // Group ID
};

// Server side, after accept():
struct ucred cred;
socklen_t len = sizeof(cred);
getsockopt(client_fd, SOL_SOCKET, SO_PEERCRED, &cred, &len);
```

**Security properties**:
- Credentials come from the kernel, not from the client
- Cannot be spoofed by userspace processes
- Available on Linux (SO_PEERCRED), FreeBSD (LOCAL_PEERCRED), macOS (LOCAL_PEERPID)

**PID reuse vulnerability**: If the connecting process terminates after establishing the socket connection, the PID could be reassigned to a different process. SPIRE mitigates this with `watcher.IsAlive()` checks [11].

**Relevance to MCP stdio transport**: The current MCP stdio transport uses stdin/stdout pipes between parent and child processes. Converting to Unix domain sockets would enable `SO_PEERCRED`-based identity verification. The Unimatrix server could:

1. Listen on a Unix domain socket instead of (or in addition to) stdio
2. Accept connections and immediately verify `SO_PEERCRED`
3. Map (UID, PID) to a registered agent identity
4. Reject connections from unregistered processes

**LLM spoofing resistance**: VERY HIGH. The LLM has zero ability to influence PID, UID, or GID -- these are kernel-enforced properties of the hosting process. Even a fully compromised LLM prompt cannot change the process identity reported by `SO_PEERCRED`.

**Limitation**: Requires Unix domain socket transport (not supported by MCP stdio out-of-the-box). Also, PID alone is ephemeral -- needs mapping to a persistent identity via registration.

### 3.2 Process Attestation with SPIFFE/SPIRE

SPIFFE (Secure Production Identity Framework For Everyone) is a CNCF-graduated standard for workload identity. SPIRE (SPIFFE Runtime Environment) implements SPIFFE through a node attestation and workload attestation pipeline [12].

**How SPIRE workload attestation works**:

1. **Node attestation**: SPIRE Agent proves its node identity to the SPIRE Server (via cloud metadata, TPM, join token, etc.)
2. **Workload registration**: Administrator registers workloads with selectors (unix:uid, unix:gid, docker:image_id, k8s:pod-name, etc.)
3. **Workload attestation**: When a workload requests identity, the SPIRE Agent verifies the caller's PID, UID, GID, and cgroup membership against registered selectors
4. **SVID issuance**: Verified workloads receive a SPIFFE Verifiable Identity Document (X.509 or JWT) with a SPIFFE ID like `spiffe://trust-domain/agent/orchestrator`

**Applying to AI agents**: Each agent process could be registered with SPIRE using process-level selectors. The Unimatrix MCP server would verify the caller's SVID before processing any tool call.

```
spiffe://unimatrix.local/agent/scrum-master
spiffe://unimatrix.local/agent/architect
spiffe://unimatrix.local/agent/implementer
```

**Per-instance identity** (critical for AI agents): Standard SPIRE gives all replicas of a workload the same identity. For AI agents, per-instance identity is needed. Solo.io's analysis proposes extending SPIFFE IDs with instance discriminators [13]:

```
spiffe://acme.com/ns/trading/sa/trading-agent-sa/instance/001
```

**Integration with Vault**: HashiCorp Vault Enterprise 1.21+ natively supports SPIFFE authentication, enabling AI agents to authenticate to Vault via SVID and receive dynamic, short-lived secrets [14].

**LLM spoofing resistance**: VERY HIGH. SPIRE attestation is based on kernel-level process properties (PID, UID, cgroup) that the LLM cannot influence. The SVID is issued by SPIRE infrastructure, not by the LLM.

**Limitation**: Heavy infrastructure requirement (SPIRE Server + Agent). Overkill for single-machine development setups. The SPIFFE ID must be mapped to Unimatrix's trust levels (System/Privileged/Internal/Restricted).

### 3.3 Cgroup-Based Identity

On Linux, every process belongs to a cgroup hierarchy. Container runtimes (Docker, containerd) assign unique cgroup paths to containers. SPIRE uses cgroup membership as a workload selector for Kubernetes attestation [12].

For non-containerized agent processes, cgroup v2 allows creating custom slices:
```
/sys/fs/cgroup/unimatrix.slice/orchestrator.scope
/sys/fs/cgroup/unimatrix.slice/architect.scope
```

The Unimatrix server could read `/proc/<pid>/cgroup` to determine the caller's cgroup path and map it to an agent identity.

**LLM spoofing resistance**: VERY HIGH (same as PID/UID -- kernel-enforced).

### 3.4 Spawner-Injected Identity

In the MCP stdio model, the **Host application** (e.g., Claude Desktop, VS Code) spawns the MCP server as a child process. The spawning process can inject identity information that the LLM cannot override:

1. **Environment variables**: Set `UNIMATRIX_AGENT_ID` and `UNIMATRIX_AGENT_SECRET` before spawning. The child process reads these, but the LLM's context window does not contain them.

2. **Command-line arguments**: Pass identity as CLI args that the server reads at startup.

3. **File descriptor passing**: The spawner opens a pre-authenticated channel and passes the fd to the child.

4. **Configuration file**: Write a per-session config file with a nonce/token, readable only by the spawned process's UID.

**Critical insight**: In MCP stdio transport, the LLM model and the MCP server process are **different processes**. The LLM writes JSON-RPC messages to the server's stdin, but it cannot modify the server's environment variables, memory, or file descriptors. The identity injection happens at process spawn time, before any LLM interaction.

**LLM spoofing resistance**: HIGH. The LLM can include a fake `agent_id` in tool parameters, but the server ignores it and uses the spawner-injected identity instead. The LLM has no mechanism to modify environment variables or process memory of the MCP server.

**Limitation**: Requires the MCP host to participate in identity provisioning. Each spawned MCP server instance has a fixed identity for its lifetime.

---

## 4. Trusted Intermediary / Proxy Patterns

### 4.1 MCP Gateway / Proxy Architecture

An MCP gateway sits between LLM clients and MCP servers, providing centralized security controls. The gateway strips self-reported identity from tool call parameters and injects verified identity based on the authenticated session [15][16].

**Architecture**:
```
LLM Client --> [MCP Gateway] --> Unimatrix MCP Server
                   |
            Identity Provider
            (OIDC, SPIFFE, etc.)
```

**How identity injection works**:
1. Agent's MCP client connects to the gateway (not directly to Unimatrix)
2. Gateway authenticates the client via SSO/OAuth/mTLS
3. Gateway extracts verified identity from the authentication token
4. Gateway creates a signed JWT "wristband" containing the agent identity and permitted tools
5. Gateway injects the JWT as a header (e.g., `x-authorized-tools`) into the forwarded request
6. Unimatrix validates the JWT using the gateway's trusted public key

**Key security property**: The MCP server never sees a raw OAuth token. It sees a clean, signed identity assertion from the gateway. The LLM never possesses or controls the identity credential [16].

**Real-world implementations**:
- **Pomerium**: Zero-trust access for MCP with per-user, per-tool authorization [15]
- **Red Hat MCP Gateway**: Advanced authentication with OIDC, x-authorized-tools JWT injection [16]
- **Traefik Hub**: MCP gateway with getting-started documentation [17]
- **Obot MCP Gateway**: Open-source MCP gateway [18]

**LLM spoofing resistance**: VERY HIGH. The LLM never sees, possesses, or controls its identity credential. Identity is injected by infrastructure that the LLM cannot influence.

**Limitation**: Adds a network hop and deployment complexity. Not suitable for local stdio-based MCP without architectural changes.

### 4.2 Service Mesh Identity Sidecar Patterns

Service meshes (Istio/Envoy, Linkerd) provide identity sidecars that transparently inject mTLS credentials into network traffic:

**Istio/Envoy pattern**:
- Envoy sidecar proxy intercepts all inbound/outbound traffic
- Kubernetes admission webhook automatically injects the sidecar
- Identity derived from Kubernetes Service Account token
- mTLS enforced between all services [19]

**Linkerd pattern**:
- Rust-based "micro-proxy" (Linkerd2-proxy)
- Automatic proxy injection via mutating webhook
- mTLS identity from root trust anchor + intermediate issuer certificate [19]

**Sidecar identity vulnerability**: If an attacker reads the Service Account token from the shared volume, they can perform their own Certificate Signing Request and get valid mTLS certificates [19]. This is relevant for AI agents -- if the LLM process has access to the same filesystem as the sidecar, token theft is possible.

**Adaptation for Unimatrix**: A "Unimatrix sidecar" could be a lightweight process that:
1. Accepts connections from the LLM's MCP client on localhost
2. Authenticates itself to Unimatrix using a pre-provisioned credential
3. Forwards tool calls with injected identity
4. Strips any self-reported identity from the tool call parameters

**LLM spoofing resistance**: HIGH, provided the sidecar's credentials are not accessible to the LLM process.

### 4.3 Intent-Describing Intermediary Pattern

This pattern (aligned with the user's intuition) separates intent from execution:

1. **LLM describes intent** in natural language (e.g., "I need to store a knowledge entry about Rust testing conventions")
2. **Trusted intermediary** (not the LLM) resolves:
   - Which agent is requesting (from process-level identity)
   - What tool to call (from intent parsing)
   - What permissions apply (from capability policy)
3. **Intermediary generates an opaque, signed request** and forwards to Unimatrix
4. **Unimatrix verifies** the intermediary's signature and processes the request

**Advantage**: The LLM never directly invokes tools. It expresses intent, and the trusted intermediary translates intent into authorized, signed tool calls. This completely decouples the LLM from the authentication mechanism.

**LLM spoofing resistance**: VERY HIGH. The LLM literally cannot forge a signed request because it does not possess the signing key. Even if prompt-injected to claim a different identity, the intermediary overrides with the verified identity.

**Limitation**: Adds latency (intent parsing step). Requires the intermediary to understand tool semantics well enough to translate natural language intent into structured tool calls.

---

## 5. Challenge-Response / Proof-of-Knowledge

### 5.1 Why Pure Challenge-Response Fails for LLMs

Traditional challenge-response protocols prove knowledge of a secret by computing a response to a random challenge. For LLMs, this approach has fundamental problems:

- LLMs are **probabilistic text generators**, not deterministic computation engines
- An LLM cannot reliably compute HMAC-SHA256 or Ed25519 signatures
- But: if the secret is in the LLM's context window, a prompt injection could extract it
- If the computation happens in the LLM's runtime (not the model), the LLM cannot be tricked into revealing the key

### 5.2 Runtime-Level HMAC with Environment Secrets

A practical hybrid approach:

1. A **signing key** is stored in the MCP client process's environment (environment variable or config file), NOT in the LLM's context/prompt
2. When the LLM generates a tool call, the **MCP client runtime** (not the LLM model) computes `HMAC-SHA256(key, nonce + tool_name + timestamp + params)`
3. The HMAC is attached to the tool call at the transport layer
4. Unimatrix verifies the HMAC using the pre-shared key associated with the registered agent

**Critical design point**: The signing key exists in the process's memory space. The LLM model generates text that the runtime framework interprets as tool calls. The HMAC computation happens in the framework code, not in the LLM's generation. The LLM never "sees" the key.

```
LLM generates:  {"tool": "context_store", "params": {...}}
                              |
                     MCP Client Runtime
                              |
                     Reads HMAC key from env
                     Computes signature
                              |
                     Sends: {"tool": "context_store",
                             "params": {...},
                             "_sig": "hmac-sha256:abc123...",
                             "_nonce": "random-value",
                             "_ts": 1708800000}
```

**Replay prevention**: Nonce + timestamp + server-side nonce tracking. Unimatrix rejects requests with:
- Timestamps older than 30 seconds
- Previously-seen nonces
- Invalid HMAC signatures

**LLM spoofing resistance**: HIGH. The LLM cannot compute the HMAC because it does not have the key. Even if a prompt injection instructs the LLM to include a fake `_sig` field, the MCP client runtime overwrites it with the real signature.

**Limitation**: Requires every MCP client to implement HMAC signing. Pre-shared keys must be distributed securely.

### 5.3 Nonce-Based Protocols with Process-Level Keys

A more sophisticated variant using asymmetric cryptography:

1. Each agent process generates an Ed25519 keypair at startup
2. The public key is registered with Unimatrix (via a registration protocol bootstrapped with a one-time token)
3. For each tool call, the MCP client runtime signs the request with the private key
4. Unimatrix verifies the signature against the registered public key

**Visa Trusted Agent Protocol (TAP)** implements exactly this pattern for AI agent authentication in commerce, using RFC 9421 HTTP Message Signatures with Ed25519. Signatures cover method, path, and headers; any modification invalidates the signature. Signatures are time-bound and replay-resistant [20].

An open-source Rust implementation (`tap-mcp-bridge`) bridges Visa TAP with MCP, enabling Claude-compatible agents to authenticate with merchants using cryptographic signatures [21].

**LLM spoofing resistance**: VERY HIGH. The Ed25519 private key is in process memory. The LLM cannot extract it or compute valid signatures.

---

## 6. Hardware and TEE-Based Approaches

### 6.1 Trusted Execution Environments (TEEs)

A TEE is a hardware-encrypted zone within a processor where sensitive computations run isolated from the OS, hypervisor, and administrators [22].

**Major implementations**:

| Technology | Vendor | Isolation Granularity | Status (2025) |
|------------|--------|-----------------------|---------------|
| Intel SGX | Intel | Process-level enclaves | **Discontinued** from latest CPUs |
| AMD SEV | AMD | Full VM encryption | Active, SEV-SNP latest |
| ARM TrustZone | ARM | Secure/Normal world split | Active |
| Intel TDX | Intel | VM-level (SGX successor) | Active |

**Intel SGX deprecation**: Intel has removed SGX from latest CPU lines, forcing organizations to pivot to AMD SEV or Intel TDX. SGX enclaves provided the finest-grained isolation (individual process), which was ideal for agent key storage. Its removal is a significant loss [23].

### 6.2 TEE for Agent Key Storage

An agent's signing key could be stored inside a TEE enclave:
1. Key generation occurs inside the enclave
2. The private key never leaves the encrypted memory region
3. Signing operations happen inside the enclave
4. Remote attestation proves the enclave is genuine and unmodified

**LLM spoofing resistance**: MAXIMUM. Even if the entire host OS is compromised, the key cannot be extracted from the TEE. The LLM has absolutely zero access to enclave memory.

**Limitation**: Requires TEE-capable hardware. The SGX deprecation narrows options. AMD SEV operates at VM granularity, which is coarser than needed for per-agent key isolation. Confidential VMs (Azure, GCP) provide VM-level attestation but not per-process attestation.

### 6.3 TPM-Based Attestation

The Trusted Platform Module (TPM) is a dedicated cryptoprocessor present in most modern hardware [24].

**TPM attestation flow for agents**:
1. TPM stores agent signing keys in tamper-resistant hardware
2. At startup, the agent process requests an attestation quote from the TPM
3. The quote covers Platform Configuration Registers (PCRs) that reflect the boot chain and loaded software
4. Unimatrix verifies the quote, confirming the agent process is running expected code on trusted hardware
5. The agent's signing key (stored in TPM) signs subsequent tool calls

**SPIRE + TPM**: SPIRE can use TPM-based node attestation, combining workload identity with hardware root of trust [25]. The attestation agent collects TPM quotes and measurement logs, and the verifier validates signatures and checks against approved whitelists.

**LLM spoofing resistance**: MAXIMUM. TPM keys are hardware-bound and cannot be extracted by any software, including the LLM.

**Limitation**: TPM is primarily a server/workstation technology. Not available in all development environments. Complex setup and management.

---

## 7. Signed Intent / Warrant Patterns

### 7.1 Digitally Signed Intent

The orchestrator creates a cryptographically signed description of what the agent should do. Unimatrix verifies the signature before executing.

**Flow**:
```
1. Human/Orchestrator defines task: "Research Rust testing patterns"
2. Orchestrator creates intent document:
   {
     "agent_id": "implementer-003",
     "task": "context_store",
     "scope": {"category": "conventions", "topic": "rust-testing"},
     "issued_at": "2026-02-24T10:00:00Z",
     "expires_at": "2026-02-24T10:30:00Z",
     "nonce": "abc123"
   }
3. Orchestrator signs with Ed25519 private key
4. Signed intent given to worker agent as opaque blob
5. Worker presents signed intent with tool call
6. Unimatrix verifies signature, checks constraints, processes if valid
```

**Anti-replay**: Nonce + expiry + server-side nonce tracking. Each signed intent is single-use (or limited-use with a counter).

**LLM spoofing resistance**: VERY HIGH. The worker LLM receives the signed intent as an opaque blob. It cannot modify the intent (signature would break), forge a new intent (no signing key), or expand the scope. Even under prompt injection, the worst case is the LLM refuses to present the intent, not that it can abuse it.

### 7.2 Tenuo Warrants as Signed Intent

Tenuo warrants (Section 2.3) are a production implementation of the signed intent pattern. The control plane mints a root warrant, the orchestrator attenuates it for the specific task, and the worker presents the attenuated warrant when calling tools.

Tenuo's constraint system adds semantic enforcement: even if the worker has a valid warrant for `context_store`, the constraints might restrict it to `category: "conventions"` only, preventing abuse of the store capability for other categories.

### 7.3 Authorization Code Pattern

Modeled after OAuth 2.0 Authorization Code flow:

1. Orchestrator requests an **authorization code** from Unimatrix for a specific agent + task
2. Unimatrix generates a one-time code and returns it to the orchestrator
3. Orchestrator gives the code to the worker agent
4. Worker presents the code to Unimatrix
5. Unimatrix validates the code (single-use, time-limited, scope-matched) and processes the request

**Replay prevention** (from OAuth 2.0 best practices [26]):
- PKCE (Proof Key for Code Exchange): orchestrator generates `code_verifier`, sends `code_challenge = SHA256(code_verifier)` to Unimatrix. Worker must present both code and verifier.
- Single-use enforcement: code invalidated after first redemption
- Short TTL: codes expire in seconds

**LLM spoofing resistance**: HIGH. The LLM receives the authorization code as an opaque string. It can present it to Unimatrix (legitimate use) or potentially leak it (via prompt injection). PKCE mitigates this: the code alone is insufficient without the `code_verifier`, which lives in the orchestrator's process memory.

### 7.4 Comparison of Warrant Approaches

| Pattern | Forgery Resistance | Replay Resistance | Scope Control | Implementation Complexity |
|---------|-------------------|-------------------|---------------|---------------------------|
| Signed Intent (Ed25519) | Very High | High (nonce+expiry) | Manual | Medium |
| Tenuo Warrants | Very High | High (TTL+binding) | Rich constraints | Low (library) |
| Biscuit Tokens | Very High | High (revocation IDs) | Datalog policies | Medium |
| Authorization Codes | High | Very High (single-use) | Per-code scope | Medium |
| Macaroons | High | Medium | Caveat-based | Low |

---

## 8. Novel and Emerging Approaches (2025-2026)

### 8.1 Academic Research

#### Binding Agent ID (BAID) -- arXiv:2512.17538 (Dec 2025)

BAID proposes a comprehensive identity infrastructure with three orthogonal mechanisms [27]:

1. **Local binding**: Biometric authentication for operator (human) identity
2. **Decentralized on-chain identity**: Blockchain-anchored DID management
3. **Code-Level Authentication**: zkVM-based protocol that treats the program binary as identity

The zkVM (zero-knowledge virtual machine) approach provides cryptographic guarantees for operator identity, agent configuration integrity, and execution provenance. It prevents unauthorized operation and code substitution by making the binary itself part of the identity proof.

**Key insight**: "While existing agent identity systems successfully establish 'agent-to-system' trust, they fall short of establishing 'human-to-agent' liability binding."

#### AI Agents with DIDs and VCs -- arXiv:2511.02841 (Nov 2025)

Proposes equipping AI agents with W3C Decentralized Identifiers (DIDs) and W3C Verifiable Credentials (VCs) [28]:

- Each agent receives a ledger-anchored DID and a set of third-party-issued VCs
- Agents prove DID ownership at dialog onset for mutual authentication
- VCs carry delegations, certifications, and capability attestations

**Critical limitation discovered**: The evaluation "reveals limitations once an agent's LLM is in sole charge to control the respective security procedures" -- confirming that LLMs managing their own cryptographic operations is inherently risky.

#### MiniScope -- arXiv:2512.11147 (Dec 2025)

UC Berkeley's framework for least-privilege enforcement in tool-calling agents [29]:

- **Mechanically enforces** permissions (not via LLM prompting)
- Automatically reconstructs permission hierarchies from tool call relationships
- Mobile-style permission model (grant/deny per capability category)
- 1-6% latency overhead vs vanilla tool calling
- Outperforms LLM-based baselines in permission minimization

**Key contribution**: "First to rigorously define and enforce least privilege principles for tool calling agentic tasks. Unlike prior works that enforce least privilege through prompting the LLM, their enforcement is mechanical."

#### Progent -- arXiv:2504.11703 (Apr 2025)

Programmable privilege control framework for LLM agents [30]:

- Domain-specific language (JSON-based) for fine-grained tool privilege policies
- Runtime deterministic enforcement with provable security guarantees
- Reduces attack success rate from 41.2% to 2.2% (AgentDojo benchmark)
- Reduces attack success rate from 70.3% to 7.3% (ASB benchmark)
- LLMs can automatically generate effective Progent policies

#### AgentCrypt -- arXiv:2512.08104 / NeurIPS 2025

Three-tiered framework for secure agent communication [31]:

- Level 1: Unrestricted data exchange
- Level 2: Context-aware masking
- Level 3: Fully encrypted computation using Homomorphic Encryption

"Unlike prompt-based defenses, AgentCrypt guarantees that tagged data privacy is strictly preserved even when the underlying model errs." Security is decoupled from the agent's probabilistic reasoning.

#### Zero-Trust Identity Framework -- arXiv:2505.19301 (May 2025)

Proposes a modular, Zero-Trust architecture supporting [32]:

- Decentralized identity provisioning via DIDs
- Context-aware authentication
- Real-time access control
- Behavioral monitoring
- Explainability feedback loops

"Traditional Identity and Access Management infrastructures are not equipped to manage entities that spawn sub-processes, adapt dynamically, or require ephemeral trust boundaries."

### 8.2 Standards and Industry Frameworks

#### OpenID Connect for Agents (OIDC-A) 1.0 -- arXiv:2509.25974 (Sep 2025)

Extends OpenID Connect with purpose-built constructs for AI agent identity [33]:

**New claims**:
- Core identity: `agent_type`, `agent_model`, `agent_version`, `agent_provider`, `agent_instance_id`
- Delegation: `delegator_sub`, `delegation_chain`
- Trust: `agent_capabilities`, `agent_trust_level`, `agent_attestation`

**New endpoints**:
- Agent Attestation Endpoint: validates attestation evidence, returns verification status with cryptographic signatures
- Agent Capabilities Endpoint: declares agent capabilities and constraints

**Delegation chain validation**: Each step includes issuer, delegator, delegatee, timestamp, scope. Validation requires chronological ordering, issuer trustworthiness, audience-to-subject matching, and scope reduction (each step's scope is a subset of the delegator's).

#### A2A Protocol (Google, Linux Foundation)

Agent2Agent protocol (originally Google, now under Linux Foundation AAIF) for agent interoperability [34]:

- Agent Card (JSON) for capability advertisement
- HTTP + SSE + JSON-RPC transport
- Supports OAuth 2.0, OpenID Connect, API keys for authentication
- Short-lived tokens (minutes) replacing static secrets

#### OWASP Top 10 for Agentic Applications (2026)

OWASP released their agentic AI security guidance in December 2025 [35]:

- **ASI01**: Agent Goal Hijack (prompt injection leading to goal modification)
- **ASI02**: Tool Misuse (legitimate tools abused within granted privileges)
- **ASI03**: Identity & Privilege Abuse (credential inheritance, confused deputy, delegation without scoping)
- **ASI04**: Supply Chain Risks (malicious tools, MCP servers, agent cards)
- **ASI05**: Unexpected Code Execution
- **ASI06**: Memory & Context Poisoning
- **ASI07**: Insecure Inter-Agent Communication
- **ASI08**: Cascading Failures
- **ASI09**: Human-Agent Trust Exploitation
- **ASI10**: Rogue Agents

**ASI03 mitigations**: Short-lived credentials, task-scoped permissions, policy-enforced authorization on every action, isolated identities for agents.

Three of the top four risks (ASI02, ASI03, ASI04) are identity-focused, confirming agent identity as a critical security concern [36].

#### Agentic AI Foundation (AAIF) -- Linux Foundation (Dec 2025)

Formed December 9, 2025, with founding contributions of MCP (Anthropic), goose (Block), and AGENTS.md (OpenAI). Platinum members include AWS, Anthropic, Block, Bloomberg, Cloudflare, Google, Microsoft, and OpenAI [37].

Working on the **SLIM protocol** (Secure Low Latency Interactive Messaging) for agent identity verification and secure inter-agent communication.

#### MCP Security Specification Updates (June 2025)

The MCP specification now requires [38]:
- **Resource Indicators** (RFC 8707): prevents malicious servers from using tokens to access other resources
- **OAuth 2.1 with PKCE**: mandatory for all authorization flows
- **Token exchange** (RFC 8693): replaces token passthrough to prevent confused deputy

### 8.3 Industry Solutions

#### Aembit -- Workload Identity for AI Agents

Aembit introduced IAM for Agentic AI with "Blended Identity" and "MCP Identity Gateway" [39]:

- **Secretless authentication**: Applications use placeholder credentials; Aembit intercepts HTTPS requests, validates workload identity, retrieves temporary credentials, injects into Authorization header
- **The agent never sees or stores the actual API key**
- **Host-based credential injection**: Single agent codebase, different credentials per destination host
- Cut 85% of credential issuance, rotation, and auditing overhead

#### Okta -- AI Agent Identity

Okta announced "Auth0 for AI Agents" (GA October 2025) and "Okta for AI Agents" (early 2026) [40]:
- Complete authentication for building AI agents
- Enterprise-grade token management, async approvals, fine-grained access controls
- Cross App Access (XAA) for identity-first security

#### CyberArk -- Privileged Access for AI

CyberArk CORA AI analyzes agent behavior, detects emerging threats, and recommends automated responses. Integration with Accenture's AI Refinery for Zero Trust management of AI agents [40].

#### HashiCorp Vault -- Dynamic Secrets for AI Agents

Validated pattern for AI agent identity with Vault [14]:
- User identity attribution through AI tool authentication flows
- Just-in-time credentials scoped by role-based access
- SPIFFE auth (Vault Enterprise 1.21+) for non-human identity workloads
- Complete audit logging of every secret request and access

### 8.4 MCP-Specific Security Research

#### "Securing the Model Context Protocol" -- arXiv:2511.20920 (Nov 2025)

Comprehensive risk analysis proposing [41]:
- Per-user authentication with scoped authorization
- Provenance tracking across agent workflows
- Containerized sandboxing with input/output checks
- Inline policy enforcement with DLP and anomaly detection
- Centralized governance using private registries or gateway layers

#### Defense-First MCP Architecture (Christian Schneider, 2025)

Four-layer defense model [42]:
1. Sandboxing & Isolation (containers/VMs, default-deny egress)
2. Authorization Boundaries (OAuth 2.1 + PKCE, token scoping)
3. Tool Integrity Verification (description auditing, version pinning)
4. Runtime Monitoring (audit trails with user attribution)

Critical confused deputy mitigation: RFC 8693 token exchange where MCP server exchanges user token for a downstream-scoped token with `subject` (original user), `actor` (MCP server), and reduced `scope`.

---

## 9. Comparison Matrix

### 9.1 LLM Spoofing Resistance

| Approach | LLM Spoofing Resistance | Token Leak Resistance | Confused Deputy | Implementation Complexity | Infrastructure Required |
|----------|------------------------|----------------------|-----------------|---------------------------|------------------------|
| **Self-reported agent_id** (current) | NONE | N/A | NONE | None | None |
| **SO_PEERCRED (Unix socket)** | MAXIMUM | N/A (no token) | HIGH | Low | Unix socket transport |
| **SPIFFE/SPIRE** | MAXIMUM | HIGH (X.509 in process) | HIGH | High | SPIRE Server + Agent |
| **Tenuo Warrants** | VERY HIGH | VERY HIGH (proof-of-possession) | VERY HIGH | Low | Tenuo control plane |
| **Biscuit Tokens** | VERY HIGH | HIGH (Ed25519) | HIGH | Medium | Public key distribution |
| **MCP Gateway (proxy)** | VERY HIGH | VERY HIGH (LLM never sees token) | VERY HIGH | Medium | Gateway infrastructure |
| **Spawner-injected identity** | HIGH | HIGH (env vars) | MEDIUM | Low | MCP host cooperation |
| **Runtime HMAC signing** | HIGH | HIGH (key in process memory) | MEDIUM | Medium | Key distribution |
| **Signed Intent (orchestrator)** | VERY HIGH | HIGH (opaque blob) | VERY HIGH | Medium | Orchestrator signing |
| **Authorization Code + PKCE** | HIGH | MEDIUM (code leakable) | HIGH | Medium | Auth server |
| **TEE / TPM** | MAXIMUM | MAXIMUM | VERY HIGH | Very High | Special hardware |
| **Macaroons** | HIGH | LOW (bearer token) | MEDIUM | Low | HMAC key distribution |
| **DID + VC** | HIGH | HIGH | HIGH | High | DID infrastructure |

### 9.2 Suitability for Unimatrix

| Approach | Local Dev Fit | Production Fit | MCP Compat | Rust Ecosystem | Incremental Adoption |
|----------|---------------|----------------|------------|----------------|---------------------|
| **SO_PEERCRED** | Excellent | Good | Needs transport change | Native | Easy (additive) |
| **Spawner-injected identity** | Excellent | Good | Compatible (env vars) | Native | Very Easy |
| **Tenuo Warrants** | Good | Excellent | Has MCP integration | Rust core | Medium |
| **Biscuit Tokens** | Good | Excellent | Layerable | `biscuit-auth` crate | Medium |
| **MCP Gateway** | Poor (overhead) | Excellent | Designed for MCP | Various | Requires infra |
| **SPIFFE/SPIRE** | Poor (heavy) | Excellent | Layerable | Rust client exists | Requires infra |
| **Runtime HMAC** | Good | Good | Compatible | Native | Easy |
| **TEE/TPM** | Poor (hw req) | Niche | Layerable | ort crate for ONNX | Very Hard |
| **Signed Intent** | Good | Excellent | Compatible | Ed25519 crate | Medium |

### 9.3 Defense-in-Depth Stack (Recommended Layers)

```
Layer 0: Process-Level Identity (SO_PEERCRED or spawner-injected)
   |  -- Establishes "which process is calling" irrefutably
   |
Layer 1: Capability Tokens (Tenuo warrants or Biscuit)
   |  -- Constrains "what this process can do" with cryptographic proof
   |
Layer 2: Request Signing (Ed25519 or HMAC)
   |  -- Ensures "this specific request is authentic" and replay-resistant
   |
Layer 3: Runtime Policy Enforcement (Progent-style DSL)
   |  -- Deterministically blocks unauthorized tool calls regardless of LLM intent
   |
Layer 4: Audit Trail (existing Unimatrix AUDIT_LOG)
   |  -- Records all actions for post-hoc verification
```

---

## 10. Recommendations for Unimatrix

### 10.1 Phase 1: Immediate (Low Infrastructure)

**Spawner-Injected Identity + Runtime HMAC Signing**

1. Define `UNIMATRIX_AGENT_TOKEN` environment variable protocol
2. MCP host sets this before spawning Unimatrix server
3. Server reads token at startup, maps to agent identity
4. Server **ignores** self-reported `agent_id` in tool parameters when a verified identity exists
5. All tool calls are HMAC-signed by the MCP client runtime (not by the LLM)

This requires zero infrastructure changes. The MCP host (Claude Desktop, VS Code, etc.) sets an environment variable. Unimatrix reads it. Done.

**Spoofing resistance**: The LLM cannot modify environment variables. The HMAC key is in process memory, not in the LLM's context.

### 10.2 Phase 2: Capability Tokens

**Integrate Tenuo or Biscuit for task-scoped authorization**

1. Orchestrator agent receives a root warrant from the control plane
2. Orchestrator attenuates warrants per task and passes to worker agents
3. Workers present warrants with tool calls
4. Unimatrix verifies warrants (offline, ~27 microseconds with Tenuo)

Tenuo is the recommended choice for Unimatrix because:
- Rust core (same language as Unimatrix)
- Purpose-built for AI agent authorization
- Explicit MCP integration
- Rich semantic constraints matching Unimatrix's domain (categories, topics, etc.)
- Proof-of-possession prevents token theft

### 10.3 Phase 3: Process-Level Binding (Optional, Production)

**Unix Domain Socket Transport with SO_PEERCRED**

1. Add UDS transport option alongside stdio
2. Verify PID/UID/GID via `SO_PEERCRED` on connection
3. Map verified process identity to Unimatrix agent identity
4. Register agent processes during orchestrator startup

This provides maximum spoofing resistance for production deployments where processes can be pre-registered.

### 10.4 Key Architecture Decision

**The LLM must never control its own identity credential.**

Every recommended approach shares this principle: identity is established by infrastructure (process environment, kernel, cryptographic tokens minted by a trusted party) that the LLM cannot influence. The LLM can express intent, but identity is verified independently.

This aligns with the broader industry consensus: OWASP ASI03 recommends isolated agent identities [35], Aembit's "secretless" model never exposes credentials to agents [39], and Tenuo's warrants gate authority at the execution layer, not the generation layer [5].

---

## 11. References

[1] "Security Best Practices - Model Context Protocol." MCP Specification (Draft). https://modelcontextprotocol.io/specification/draft/basic/security_best_practices

[2] Mark S. Miller. "Robust Composition: Towards a Unified Approach to Access Control and Concurrency Control." PhD thesis, Johns Hopkins University, 2006. https://scholar.google.com/citations?user=PuP2INoAAAAJ

[3] "E (programming language)." Wikipedia. https://en.wikipedia.org/wiki/E_(programming_language)

[4] "Cap'n Proto: RPC Protocol." https://capnproto.org/rpc.html

[5] Tenuo -- Capability Tokens for AI Agents. https://github.com/tenuo-ai/tenuo / https://tenuo.ai/ / https://crates.io/crates/tenuo

[6] Biscuit -- Authorization Token with Decentralized Verification. https://www.biscuitsec.org/ / https://github.com/eclipse-biscuit/biscuit-rust

[7] Birgisson et al. "Macaroons: Cookies with Contextual Caveats for Decentralized Authorization in the Cloud." NDSS 2014. https://research.google/pubs/macaroons-cookies-with-contextual-caveats-for-decentralized-authorization-in-the-cloud/

[8] "From prompt injections to protocol exploits: Threats in LLM-powered AI agents workflows." ScienceDirect, 2025. https://www.sciencedirect.com/science/article/pii/S2405959525001997

[9] "Log-To-Leak: Prompt Injection Attacks on Tool-Using LLM Agents via Model Context Protocol." OpenReview. https://openreview.net/forum?id=UVgbFuXPaO

[10] "unix(7) - Linux manual page." https://man7.org/linux/man-pages/man7/unix.7.html / "joeshaw/peercred - Go wrapper for SO_PEERCRED." https://github.com/joeshaw/peercred

[11] SPIFFE/SPIRE documentation on workload attestation. https://spiffe.io/docs/latest/spire-about/spire-concepts/

[12] SPIFFE -- Secure Production Identity Framework For Everyone. https://spiffe.io/ / "Registering workloads." https://spiffe.io/docs/latest/deploying/registering/

[13] "Agent Identity and Access Management - Can SPIFFE Work?" Solo.io. https://www.solo.io/blog/agent-identity-and-access-management---can-spiffe-work

[14] "Secure AI agent authentication using HashiCorp Vault dynamic secrets." HashiCorp Developer. https://developer.hashicorp.com/validated-patterns/vault/ai-agent-identity-with-hashicorp-vault / "SPIFFE: Securing the identity of agentic AI and non-human actors." HashiCorp Blog. https://www.hashicorp.com/en/blog/spiffe-securing-the-identity-of-agentic-ai-and-non-human-actors

[15] "MCP Security: Zero Trust Access for Agentic AI." Pomerium. https://www.pomerium.com/blog/secure-access-for-mcp

[16] "Advanced authentication and authorization for MCP Gateway." Red Hat Developer. https://developers.redhat.com/articles/2025/12/12/advanced-authentication-authorization-mcp-gateway

[17] "Getting Started with MCP Gateway." Traefik Hub. https://doc.traefik.io/traefik-hub/mcp-gateway/guides/getting-started

[18] Obot MCP Gateway. https://obot.ai/

[19] "Sidecar Siphon: Stealing Identities in Service Mesh." InstaTunnel Blog. https://instatunnel.my/blog/the-sidecar-siphon-exploiting-identity-leaks-in-service-mesh-architectures / "Linkerd vs. Istio." Solo.io. https://www.solo.io/topics/istio/linkerd-vs-istio

[20] Visa Trusted Agent Protocol. https://developer.visa.com/capabilities/trusted-agent-protocol / RFC 9421 -- HTTP Message Signatures. https://datatracker.ietf.org/doc/html/rfc9421

[21] tap-mcp-bridge -- Rust bridge for Visa TAP + Anthropic MCP. https://github.com/bug-ops/tap-mcp-bridge

[22] "Trusted execution environment." Wikipedia. https://en.wikipedia.org/wiki/Trusted_execution_environment

[23] "Rethinking Confidential Compute for Private AI Model Training in 2025." AI 2 Work. https://ai2.work/technology/ai-tech-intel-sgx-discontinuation-impact-2025/

[24] "Trusted Platform Module." Wikipedia. https://en.wikipedia.org/wiki/Trusted_Platform_Module / "TPM remote attestation." Infineon Community. https://community.infineon.com/t5/Blogs/TPM-remote-attestation-How-can-I-trust-you/ba-p/452729

[25] "Securing Cloud-Native Workloads from the Metal Up - TPM + SPIRE." OpenSource SecurityCon. https://tldrecap.tech/posts/2025/opensource-securitycon-na/secure-cloud-native-infrastructure-tpm-spire/

[26] "OAuth2 Cheat Sheet." OWASP. https://cheatsheetseries.owasp.org/cheatsheets/OAuth2_Cheat_Sheet.html / "Transaction Authorization Cheat Sheet." OWASP. https://cheatsheetseries.owasp.org/cheatsheets/Transaction_Authorization_Cheat_Sheet.html

[27] "Binding Agent ID: Unleashing the Power of AI Agents with accountability and credibility." arXiv:2512.17538, Dec 2025. https://arxiv.org/abs/2512.17538

[28] "AI Agents with Decentralized Identifiers and Verifiable Credentials." arXiv:2511.02841, Nov 2025. https://arxiv.org/abs/2511.02841

[29] Zhu et al. "MiniScope: A Least Privilege Framework for Authorizing Tool Calling Agents." arXiv:2512.11147, Dec 2025. https://arxiv.org/abs/2512.11147

[30] "Progent: Programmable Privilege Control for LLM Agents." arXiv:2504.11703, Apr 2025. https://arxiv.org/abs/2504.11703

[31] "AgentCrypt: Advancing Privacy and (Secure) Computation in AI Agent Collaboration." arXiv:2512.08104 / NeurIPS 2025. https://arxiv.org/abs/2512.08104

[32] "A Novel Zero-Trust Identity Framework for Agentic AI." arXiv:2505.19301, May 2025. https://arxiv.org/abs/2505.19301

[33] "OpenID Connect for Agents (OIDC-A) 1.0: A Standard Extension for LLM-Based Agent Identity and Authorization." arXiv:2509.25974, Sep 2025. https://arxiv.org/abs/2509.25974

[34] "Announcing the Agent2Agent Protocol (A2A)." Google Developers Blog. https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/ / A2A Protocol. https://a2a-protocol.org/latest/

[35] "OWASP Top 10 for Agentic Applications for 2026." OWASP GenAI Security Project. https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/ / "Securing Agentic Applications Guide 1.0." https://genai.owasp.org/resource/securing-agentic-applications-guide-1-0/

[36] "OWASP Agentic Top 10 and the Case for Agentic Identity." Descope. https://www.descope.com/blog/post/owasp-agentic-top-10-identity / "OWASP's Top 10 Agentic AI Risks Explained." HUMAN Security. https://www.humansecurity.com/learn/blog/owasp-top-10-agentic-applications/

[37] "Linux Foundation Announces the Formation of the Agentic AI Foundation (AAIF)." Linux Foundation, Dec 2025. https://www.linuxfoundation.org/press/linux-foundation-announces-the-formation-of-the-agentic-ai-foundation

[38] "Model Context Protocol (MCP) Spec Updates from June 2025." Auth0. https://auth0.com/blog/mcp-specs-update-all-about-auth/ / "What the New MCP Specification Means to You." Lakera. https://www.lakera.ai/blog/what-the-new-mcp-specification-means-to-you-and-your-agents

[39] Aembit -- Securing AI Agents Without Secrets. https://aembit.io/blog/securing-ai-agents-without-secrets/ / "Aembit Introduces Identity and Access Management for Agentic AI." https://aembit.io/press-release/aembit-introduces-identity-and-access-management-for-agentic-ai/

[40] Okta -- Govern and Secure Identity for AI Agents. https://www.okta.com/solutions/secure-ai/ / "CyberArk, Okta, Google target AI agent security." Biometric Update. https://www.biometricupdate.com/202504/cyberark-okta-google-target-ai-agent-security

[41] "Securing the Model Context Protocol (MCP): Risks, Controls, and Governance." arXiv:2511.20920, Nov 2025. https://arxiv.org/abs/2511.20920

[42] "Securing MCP: a defense-first architecture guide." Christian Schneider. https://christian-schneider.net/blog/securing-mcp-defense-first-architecture/

[43] "Capabilities Are the Only Way to Secure Agent Delegation." niyikiza.com. https://niyikiza.com/posts/capability-delegation/

[44] "AI Agents Need Identity and Zero-Knowledge Proofs Are the Solution." CoinDesk, Nov 2025. https://www.coindesk.com/opinion/2025/11/19/ai-agents-need-identity-and-zero-knowledge-proofs-are-the-solution

[45] "SPIFFE Meets OAuth2: Current Landscape for Secure Workload Identity in the Agentic AI Era." Riptides. https://riptides.io/blog-post/spiffe-meets-oauth2-current-landscape-for-secure-workload-identity-in-the-agentic-ai-era

[46] "MCP Security: TOP 25 MCP Vulnerabilities." Adversa AI. https://adversa.ai/mcp-security-top-25-mcp-vulnerabilities/

[47] "A Timeline of Model Context Protocol (MCP) Security Breaches." AuthZed. https://authzed.com/blog/timeline-mcp-breaches

[48] "Prompt Injection Attacks in Large Language Models and AI Agent Systems." MDPI Information, 2025. https://www.mdpi.com/2078-2489/17/1/54

[49] "SEP: Capability-based authorization." A2A Project Discussion #1404. https://github.com/a2aproject/A2A/discussions/1404

[50] "Identity Management for Agentic AI." OpenID Foundation, Oct 2025. https://openid.net/wp-content/uploads/2025/10/Identity-Management-for-Agentic-AI.pdf

[51] "Establishing Workload Identity for Zero Trust CI/CD: From Secrets to SPIFFE-Based Authentication." arXiv:2504.14760, Apr 2025. https://arxiv.org/html/2504.14760v1

[52] "An Explainable Zero Trust Identity Framework for LLMs." IJCA, 2025. https://www.ijcaonline.org/archives/volume187/number46/bhushan-2025-ijca-925777.pdf

[53] "MCP-Guard: A Multi-Stage Defense-in-Depth Framework for Securing Model Context Protocol in Agentic AI." arXiv:2508.10991. https://arxiv.org/html/2508.10991

[54] dckc/awesome-ocap -- Awesome Object Capabilities and Capability Security. https://github.com/dckc/awesome-ocap
