# ASS-008: Existing Implementations and Standards for AI Agent Authentication

**Date:** 2026-02-24
**Scope:** Survey of existing libraries, standards, protocols, and products for authenticating AI agents with resistance to LLM spoofing.
**Purpose:** Inform Unimatrix's agent authentication architecture with practical, implementable solutions.

---

## Table of Contents

1. [Capability Token Libraries](#1-capability-token-libraries)
   - 1.1 Tenuo
   - 1.2 Biscuit
   - 1.3 UCAN
   - 1.4 Macaroons
2. [Identity Frameworks](#2-identity-frameworks)
   - 2.1 SPIFFE/SPIRE
   - 2.2 Decentralized Identifiers (DIDs) and Verifiable Credentials
   - 2.3 MCP-I (MCP-Identity)
   - 2.4 Project NANDA
3. [Relationship-Based Access Control](#3-relationship-based-access-control)
   - 3.1 Google Zanzibar / SpiceDB
   - 3.2 OpenFGA
4. [Cloud Identity Providers for Agents](#4-cloud-identity-providers-for-agents)
   - 4.1 Auth0 for AI Agents
   - 4.2 Okta Identity Security Fabric
5. [Agent Communication Protocols](#5-agent-communication-protocols)
   - 5.1 Google A2A Protocol
   - 5.2 MCP Authorization Specification
6. [MCP Security Extensions and Tools](#6-mcp-security-extensions-and-tools)
   - 6.1 AttestMCP
   - 6.2 MCPSecBench
   - 6.3 MCP Guardian
   - 6.4 Attestable MCP Server
   - 6.5 mcp-scan / agent-scan
7. [OWASP MCP Security Guidance](#7-owasp-mcp-security-guidance)
8. [Rust Cryptographic Libraries](#8-rust-cryptographic-libraries)
   - 8.1 ed25519-dalek
   - 8.2 ring
   - 8.3 jsonwebtoken
   - 8.4 rusty_paseto / pasetors
   - 8.5 PASETO vs JWT Analysis
   - 8.6 cap-std
9. [Agent Framework Authentication](#9-agent-framework-authentication)
   - 9.1 LangChain/LangGraph
   - 9.2 AutoGen
   - 9.3 CrewAI
10. [Academic Research](#10-academic-research)
    - 10.1 CaMeL (Google DeepMind)
    - 10.2 Agent Security Bench (ASB)
    - 10.3 DID/VC Agent Authentication
    - 10.4 LLM Agent Communication Security Survey
11. [MCP Client Implementations](#11-mcp-client-implementations)
12. [Comparative Analysis](#12-comparative-analysis)
13. [Recommendations for Unimatrix](#13-recommendations-for-unimatrix)
14. [References](#14-references)

---

## 1. Capability Token Libraries

### 1.1 Tenuo

| Property | Value |
|---|---|
| **License** | MIT / Apache-2.0 |
| **Language** | Rust core, Python bindings, WASM bindings |
| **Version** | 0.1.0-beta.7 (as of Feb 2026) |
| **Maturity** | Beta. "Core semantics are stable. APIs may evolve." |
| **GitHub** | [tenuo-ai/tenuo](https://github.com/tenuo-ai/tenuo) |
| **Crate** | [tenuo](https://crates.io/crates/tenuo) |
| **Stars/Forks** | 27 stars, 2 forks |
| **Commits** | ~265 on main branch |
| **Created** | December 3, 2025 |
| **Readiness** | **Prototype/Early Beta** |

#### How Warrants Work

Tenuo implements cryptographically-enforced capability attenuation for AI agent workflows. The core abstraction is the **Warrant** -- a signed, time-limited, task-scoped capability token.

**Lifecycle:**
1. **Minting:** An issuer creates a warrant via `WarrantBuilder` / `OwnedIssuanceBuilder`, specifying capabilities (tool names), constraints (parameter restrictions), a holder public key, and a TTL.
2. **Signing:** The issuer's `SigningKey` (Ed25519) signs the warrant, producing a `Signature`. Warrants are cryptographically bound to the issuer.
3. **Verification:** Any party holding the issuer's `PublicKey` can verify the warrant offline in ~27 microseconds. The `Authorizer` / `DataPlane` checks the signature, expiry, and whether the requested capability + constraints match.
4. **Attenuation:** A warrant holder can create a narrower warrant via `OwnedAttenuationBuilder`. Authority can only shrink -- never expand. Constraints compose monotonically. Each attenuation adds a new delegation link to the chain.

**Constraint System:**
- `Exact`, `Pattern`, `OneOf`, `Contains`, `Wildcard` -- string matching
- `Range` -- numeric bounds
- `Cidr` -- network ranges
- `RegexConstraint`, `UrlPattern` -- complex patterns
- `CelConstraint` -- Common Expression Language evaluation
- `All`, `Any`, `Not`, `Subset`, `NotOneOf` -- logical/set composition
- Nesting up to `MAX_CONSTRAINT_DEPTH`

**Infrastructure Types:**
- `ControlPlane` / `DataPlane` -- issuance vs. verification separation
- `RevocationManager`, `SignedRevocationList` -- warrant invalidation
- `GatewayConfig`, `CompiledGatewayConfig` -- route-based policy enforcement
- `DelegationReceipt` -- audit trail for attenuation events
- `ChainVerificationResult` -- multi-hop delegation chain validation

**Protocol Constants:**
- `DEFAULT_WARRANT_TTL_SECS` = 300 (5 minutes)
- `MAX_WARRANT_TTL_SECS` = 7,776,000 (90 days)
- `MAX_DELEGATION_DEPTH` -- bounded chain length

**Performance:** ~27 microseconds per offline verification. No network round-trip required.

**Integration Points:** Python extras for `openai`, `google_adk`, `a2a`, `fastapi`, `langchain`, `langgraph`, `crewai`, `temporal`, `autogen`, `mcp`. The Rust core can be embedded directly.

**Design Influences:** CaMeL (Google DeepMind), Macaroons, Biscuit, UCAN.

#### Limitations

- Very new project (created Dec 2025, ~3 months old).
- 27 stars / 2 forks -- low adoption signal.
- API surface may change (beta status).
- **Not a sandbox** -- authorizes actions but does not isolate execution.
- **Not an LLM filter** -- gates tool calls at the runtime level, not at the model output level.
- Bus factor: unclear number of core contributors (appears to be a small team).

#### Relevance to Unimatrix

**High relevance.** Tenuo's warrant model maps directly to Unimatrix's agent trust levels and capability enforcement. The Rust core means zero FFI overhead. The constraint system is rich enough to model Unimatrix's per-tool, per-category, per-agent restrictions. The ~27us verification cost is negligible compared to Unimatrix's embedding operations.

---

### 1.2 Biscuit

| Property | Value |
|---|---|
| **License** | Apache-2.0 |
| **Language** | Rust (reference), WASM, Python, Haskell, Java, Go, C |
| **Version** | biscuit-auth 6.0.0 (July 2025) |
| **Maturity** | **Production-ready** (v3.x token format) |
| **GitHub** | [eclipse-biscuit/biscuit-rust](https://github.com/biscuit-auth/biscuit-rust) |
| **Crate** | [biscuit-auth](https://crates.io/crates/biscuit-auth) |
| **Stars/Forks** | 227 stars, 39 forks |
| **Contributors** | 27 |
| **Readiness** | **Production-ready** |

#### How Biscuit Works

Biscuit is an authorization token for microservices with three core properties:
1. **Decentralized validation** -- any holder of the root public key can verify.
2. **Offline attenuation** -- add restriction blocks without contacting the issuer.
3. **Datalog-based policy** -- flexible rights expression using a modified Datalog language.

**Token Structure:** A Biscuit token consists of an authority block (signed by the issuer) plus zero or more attenuation blocks. Each block adds new checks/rules that further restrict the token. Blocks cannot expand authority.

**Datalog Policy Language:**
```
// Only allow reading file.txt
check if resource("file.txt"), operation("read");

// Time-limited access
check if time($t), $t < 2026-03-01T00:00:00Z;

// Agent-specific constraints
check if agent_trust_level($level), $level >= 2;
```

**Performance:** Token parsing + signature verification + Datalog evaluation typically completes in under 1 millisecond. Granular benchmarks: single-block verification ~264us, two blocks ~323us, three blocks ~419us.

**Token Size:** Initial tokens around 258 bytes. Grows with attenuation blocks.

**Cryptography:** Public-key based (not shared-secret like Macaroons). Uses ed25519 for signatures.

#### Comparison with Tenuo

| Aspect | Biscuit | Tenuo |
|---|---|---|
| Maturity | Production (6+ years) | Beta (3 months) |
| Policy Language | Datalog (very flexible) | Constraint types (structured) |
| Performance | ~264-419us | ~27us |
| Attenuation | Block-based, unlimited | Chain-based, bounded depth |
| Ecosystem | Multi-language | Rust + Python + WASM |
| AI Agent Focus | General-purpose | Purpose-built for agents |

#### Relevance to Unimatrix

**High relevance.** Biscuit is the most mature capability token library in the Rust ecosystem. Its Datalog policy language can express complex agent authorization rules. The trade-off vs. Tenuo is maturity (Biscuit wins) vs. domain-specific fit (Tenuo wins -- designed for AI agents from the start). Biscuit's ~1ms verification is still very fast for Unimatrix's use case.

---

### 1.3 UCAN (User Controlled Authorization Network)

| Property | Value |
|---|---|
| **License** | Apache-2.0 |
| **Language** | Rust, JavaScript/TypeScript, Go |
| **Version** | ucan crate 0.4.0 |
| **Maturity** | **Alpha/Emerging standard** |
| **GitHub** | [ucan-wg/rs-ucan](https://github.com/ucan-wg/rs-ucan) |
| **Crate** | [ucan](https://crates.io/crates/ucan) |
| **Stars/Forks** | 73 stars, 21 forks |
| **Downloads** | ~77K total |
| **Readiness** | **Alpha** |

#### How UCAN Works

UCANs are JWT-based decentralized authorization tokens. They facilitate distributed authorization flows where:
- Tokens are self-certifying (no central authority needed).
- Delegation chains are embedded in the token via proof references.
- Capabilities can be attenuated at each delegation step.
- The `UcanBuilder` creates tokens; `ProofChain` validates delegation chains.

UCANs originated in the Filecoin/IPFS ecosystem (Protocol Labs) and are used in web3.storage and related projects.

#### Relevance to Unimatrix

**Medium relevance.** UCAN is conceptually aligned (capability attenuation, decentralized verification) but less mature than Biscuit, JWT-based (inheriting JWT's design issues), and oriented toward decentralized web rather than local agent orchestration. The spec is still evolving.

---

### 1.4 Macaroons (Rust)

| Property | Value |
|---|---|
| **License** | MIT |
| **GitHub** | [macaroon-rs/macaroon](https://github.com/macaroon-rs/macaroon) |
| **Maturity** | **Unmaintained / Not production-safe** |
| **Readiness** | **Do not use** |

The Rust Macaroons implementation explicitly warns: "This library should not be considered secure and should not be used in production until it passes a full security audit." Macaroons use shared-secret HMAC chains (not public-key crypto), which is a fundamental limitation for multi-party verification scenarios. Biscuit was designed as the public-key successor to Macaroons.

---

## 2. Identity Frameworks

### 2.1 SPIFFE/SPIRE

| Property | Value |
|---|---|
| **License** | Apache-2.0 |
| **Specification** | [spiffe.io](https://spiffe.io/) |
| **Implementation** | SPIRE (reference implementation) |
| **Rust Libraries** | `spiffe` crate, `spiffe-rs` crate |
| **Maturity** | **Production-ready** (CNCF Graduated project) |
| **Readiness** | **Production-ready for distributed systems** |

#### How SPIFFE Works

SPIFFE (Secure Production Identity Framework for Everyone) provides cryptographic identities to workloads:
- **SPIFFE ID:** A URI like `spiffe://trust-domain/workload-identifier`.
- **SVID (SPIFFE Verifiable Identity Document):** Short-lived X.509 certificates or JWT tokens proving the SPIFFE ID.
- **SPIRE Agent:** Runs on each node, performs node attestation, manages SVID rotation.
- **SPIRE Server:** Central authority that issues SVIDs after workload attestation.

#### Rust Libraries

1. **`spiffe` crate** -- SPIFFE identity primitives, Workload API client (X509Source, JwtSource), streaming updates. Opt-in features: `x509-source`, `jwt-source`, `workload-api-*`.
2. **`spiffe-rs` crate** -- Port of spiffe-go with DID support, bundles, SVIDs, Workload API client, rustls TLS integration.

#### Suitability for Local-Only Agent Auth

SPIFFE/SPIRE is **architecturally overkill** for Unimatrix's single-machine, stdio-transport scenario:
- Requires running a SPIRE Server + SPIRE Agent daemon.
- Designed for distributed microservice identity across clusters.
- Node attestation assumes multiple machines.
- The Workload API requires a Unix domain socket to the SPIRE Agent.

However, the SPIFFE ID format and X.509 SVID concepts could inform Unimatrix's agent identity design without adopting the full SPIRE infrastructure.

#### Relevance to Unimatrix

**Low relevance for implementation, medium for design influence.** SPIFFE's identity model is excellent, but the infrastructure overhead is disproportionate for a local MCP server. Better to borrow concepts (short-lived certs, attestation) than adopt the stack.

---

### 2.2 Decentralized Identifiers (DIDs) and Verifiable Credentials (VCs)

| Property | Value |
|---|---|
| **Standards** | W3C DID Core, W3C VC Data Model 2.0 |
| **Maturity** | **W3C Recommendation (ratified)** |
| **Rust Libraries** | `ssi` crate (Spruce Systems), `didkit` |
| **Readiness** | **Standards mature; agent-specific implementations are prototypes** |

#### Research Paper: AI Agents with DIDs and VCs

Garzon et al. (2025) propose equipping each AI agent with a ledger-anchored DID and a set of VCs. Their prototype demonstrates:
- Agents authenticate via DID ownership proofs.
- Trust relationships established through VC exchange.
- Cross-domain interoperability via standard formats.

**Critical finding:** "Technical feasibility is demonstrated but reveals limitations once an agent's LLM is in sole charge to control the respective security procedures." The LLM may agree to skip authentication against stated policies -- confirming that **enforcement must be at the runtime level, not the LLM level**.

#### Relevance to Unimatrix

**Low-medium relevance.** DID/VC is a heavyweight standard requiring ledger infrastructure. The core insight (runtime enforcement, not LLM compliance) is valuable. The standards themselves are useful reference points but excessive for local agent auth.

---

### 2.3 MCP-I (MCP-Identity) by Vouched

| Property | Value |
|---|---|
| **Vendor** | Vouched |
| **Announced** | May 2025 |
| **Type** | Commercial product + proposed MCP extension |
| **Readiness** | **Early commercial product** |

#### What MCP-I Provides

MCP-I is a proposed extension to MCP adding identity capabilities:
1. **Agent Identity Server** -- agents store and present verified credentials and delegated authorities.
2. **Know Your Agent (KYA)** -- authentication (cryptographic credentials), user association verification, attestation (delegated permissions).
3. **Agent Reputation Directory** ("Know That AI") -- community-driven agent trustworthiness assessment.

Built on W3C Verifiable Credentials and DIDs. Primarily a cloud service, not a library.

#### Relevance to Unimatrix

**Low relevance for implementation.** Commercial, cloud-first, not a library. The KYA concept and reputation directory ideas are interesting design influences.

---

### 2.4 Project NANDA (MIT Media Lab)

| Property | Value |
|---|---|
| **Organization** | MIT Media Lab |
| **Announced** | 2025 |
| **Type** | Research infrastructure / protocol |
| **Website** | [projectnanda.org](https://projectnanda.org/) |
| **Readiness** | **Research prototype** |

Project NANDA (Network of AI Agents and Decentralized Architecture) builds a "DNS for agents" enabling discovery, authentication, and verifiable interaction. Key components:
- **AgentFacts** -- signed, schema-validated JSON-LD documents describing agent capabilities, operators, and connection details.
- **Zero Trust Agentic Access (ZTAA)** -- extends ZTNA to autonomous agents.
- **Federated registries** -- currently hosted at 15 universities.
- Supports MCP, A2A, and NLWeb protocols.

#### Relevance to Unimatrix

**Low relevance for immediate implementation.** NANDA is focused on internet-scale agent discovery and federation. The AgentFacts concept and ZTAA principles could influence Unimatrix's design for future multi-project scenarios.

---

## 3. Relationship-Based Access Control

### 3.1 SpiceDB (Google Zanzibar)

| Property | Value |
|---|---|
| **License** | Apache-2.0 |
| **Language** | Go |
| **GitHub** | [authzed/spicedb](https://github.com/authzed/spicedb) |
| **Rust Clients** | [spicedb-client](https://crates.io/crates/spicedb-client), [spicedb-grpc](https://crates.io/crates/spicedb-grpc), [spicedb-rust](https://github.com/Lur1an/spicedb-rust) |
| **Maturity** | **Production-ready** |
| **Readiness** | **Production-ready, but architectural mismatch** |

SpiceDB is the leading open-source Google Zanzibar implementation. It models authorization as a graph of relationships: `user:alice#member@organization:acme`. Permission checks are graph traversals.

**Rust Clients Available:**
- `spicedb-client` -- ergonomic gRPC client
- `spicedb-grpc` -- auto-generated from protobuf
- `spicedb-rust` -- opinionated client by Lur1an

**Assessment for Unimatrix:** SpiceDB requires running a separate Go server with a database backend (PostgreSQL, CockroachDB, or Spanner). This is **significant operational overhead** for a local MCP server. The relationship model is powerful but designed for cloud-scale multi-tenant authorization, not single-process agent capability scoping.

### 3.2 OpenFGA

| Property | Value |
|---|---|
| **License** | Apache-2.0 |
| **Organization** | Auth0 (Okta) |
| **GitHub** | [openfga/openfga](https://github.com/openfga) |
| **Rust Clients** | [openfga-client](https://crates.io/crates/openfga-client), [openfga-rs](https://github.com/liamwh/openfga-rs) |
| **Maturity** | **Production-ready** (CNCF project) |
| **Readiness** | **Production-ready, but same mismatch as SpiceDB** |

OpenFGA is Auth0's Zanzibar implementation. Same assessment as SpiceDB -- requires a separate server process, designed for distributed systems.

**Rust Clients:**
- `openfga-client` (vakamo-labs) -- type-safe gRPC client with auth model management
- `openfga-rs` (liamwh) -- auto-generated from protobuf

#### Relevance to Unimatrix (both SpiceDB and OpenFGA)

**Low relevance.** These systems solve authorization at a different architectural layer. Unimatrix needs **embedded, in-process** capability enforcement, not an external authorization service. The relationship model concepts are useful but the operational overhead is prohibitive. Unimatrix already has its own redb-backed storage and trust level system (AGENT_REGISTRY).

---

## 4. Cloud Identity Providers for Agents

### 4.1 Auth0 for AI Agents

| Property | Value |
|---|---|
| **Vendor** | Auth0 (Okta) |
| **GA Date** | November 2025 |
| **Type** | Cloud service + SDKs |
| **MCP Support** | Yes (OAuth 2.0 resource server) |
| **Readiness** | **Production-ready (cloud-only)** |

#### Key Features

1. **User Authentication** -- login flows for chatbots and AI agents.
2. **Token Vault** -- secure storage for OAuth tokens to 30+ pre-integrated services (GitHub, Slack, Google Workspace, etc.). Handles token lifecycle (refresh, exchange) automatically.
3. **MCP Integration** -- registers MCP clients/servers, manages OAuth 2.0 flows, supports Custom Token Exchange.
4. **Agent Identity** -- each agent gets a distinct, traceable identity.
5. **SDKs** -- Python, Node.js, others. No Rust SDK.

#### Assessment for Unimatrix

**Not applicable for local-first scenario.** Auth0 is cloud-first with no local/offline mode. There is no Rust SDK. However, for future Unimatrix scenarios involving remote MCP servers or multi-user deployments, Auth0's MCP authorization patterns are a useful reference.

### 4.2 Okta Identity Security Fabric

| Property | Value |
|---|---|
| **Vendor** | Okta |
| **Announced** | September 2025 |
| **Type** | Enterprise platform |
| **Readiness** | **Enterprise production (cloud-only)** |

Okta's "identity security fabric" provides a centralized control plane for human and non-human identities, including AI agents. Features include agent lifecycle management, Cross App Access, and verifiable credentials. Entirely cloud-based and enterprise-oriented.

#### Relevance to Unimatrix

**No direct relevance for implementation.** Useful as a reference for enterprise identity patterns.

---

## 5. Agent Communication Protocols

### 5.1 Google A2A Protocol (Agent-to-Agent)

| Property | Value |
|---|---|
| **Organization** | Google (50+ technology partners) |
| **Announced** | April 2025 |
| **Current Version** | 0.3 |
| **GitHub** | [a2aproject/A2A](https://github.com/a2aproject/A2A) |
| **Transport** | HTTP, SSE, JSON-RPC, gRPC (v0.3) |
| **Readiness** | **Beta specification** |

#### Authentication in A2A

A2A supports OpenAPI-aligned security schemes:
- API keys
- OAuth 2.0
- OpenID Connect Discovery
- Tokens scoped per task, expiring in minutes
- **Signed security cards** (v0.3) -- agents cryptographically sign their Agent Cards

**Agent Cards** are JSON documents advertising an agent's capabilities. In v0.3, cards can be signed, enabling verification of the agent's declared identity and capabilities.

#### Relationship to MCP

A2A and MCP are complementary:
- **MCP** connects an LLM to tools/resources (vertical integration).
- **A2A** connects agents to other agents (horizontal interop).
- An MCP server could expose A2A agents as tools.

#### Relevance to Unimatrix

**Medium relevance.** A2A's signed Agent Cards and task-scoped tokens are directly applicable patterns. If Unimatrix evolves to support multi-agent scenarios, A2A's authentication model is a natural fit. The gRPC support in v0.3 is interesting for Rust integration.

---

### 5.2 MCP Authorization Specification

| Property | Value |
|---|---|
| **Specification** | [MCP 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25) |
| **Auth Standard** | OAuth 2.1 (HTTP transport only) |
| **STDIO Auth** | None specified (environment variables by convention) |
| **Readiness** | **Specification stable; implementations vary widely** |

#### Current State

The November 2025 MCP specification established:
1. **MCP servers as OAuth Resource Servers** -- servers validate tokens, not issue them.
2. **Resource Indicators (RFC 8707)** -- tokens scoped to specific MCP servers.
3. **PKCE required** -- prevents authorization code interception.
4. **Client ID Metadata Documents (CIMD)** -- decentralized client registration.
5. **Extensions framework** -- scenario-specific additions outside the core spec.

**Critical gap for STDIO transport:** The MCP spec explicitly states that **STDIO should NOT use the OAuth authorization spec**. For local STDIO servers (like Unimatrix), authentication is entirely out-of-spec. The convention is environment variables for credentials, with an "implicit trust boundary" since the server runs in the user's local environment.

This means Unimatrix must define its own agent authentication mechanism for STDIO transport.

#### Five Layers of MCP Auth

The Permit.io analysis identifies five distinct layers:
1. **Agent Identity** -- distinct, traceable identity per agent
2. **Delegator Authentication** -- who authorized the agent
3. **Consent** -- delegator-to-agent permission
4. **MCP Server Access** -- token validation
5. **Upstream Service Access** -- token exchange for external APIs

#### Relevance to Unimatrix

**High relevance.** Unimatrix operates over STDIO, where MCP provides no auth specification. This is both a gap and an opportunity -- Unimatrix needs its own authentication mechanism. The five-layer model is a useful framework for designing it.

---

## 6. MCP Security Extensions and Tools

### 6.1 AttestMCP

| Property | Value |
|---|---|
| **Paper** | "Breaking the Protocol" (arXiv:2601.17549) |
| **Authors** | Narek Maloyan, Dmitry Namiot |
| **Date** | January 2026 |
| **Type** | Backward-compatible MCP protocol extension |
| **Readiness** | **Research proposal (no implementation released)** |

#### What AttestMCP Adds

Three security mechanisms:
1. **Capability Attestation** -- servers cryptographically prove capability possession via signed certificates from a capability authority.
2. **Message Authentication** -- all JSON-RPC messages include HMAC-SHA256 signatures binding content to authenticated server identity.
3. **Origin Tagging** -- sampling requests tagged with server origin, enabling clients to distinguish server-injected from user-originated prompts.

#### Performance

- Median latency overhead: **8.3ms per message**
- Attack success rate reduction: **52.8% to 12.4%** (tested across 847 attack scenarios)
- MCP's design amplifies attack success by 23-41% vs. non-MCP integrations

#### Three Fundamental Vulnerabilities Identified

1. Absence of capability attestation (servers can claim arbitrary permissions)
2. Bidirectional sampling without origin authentication (server-side prompt injection)
3. Implicit trust propagation in multi-server configurations

#### Relevance to Unimatrix

**High relevance for design.** AttestMCP identifies the exact problems Unimatrix faces. The capability attestation and message authentication patterns are directly implementable. No released code to use, but the design is well-documented enough to implement.

---

### 6.2 MCPSecBench

| Property | Value |
|---|---|
| **Paper** | arXiv:2508.13220 |
| **GitHub** | [AIS2Lab/MCPSecBench](https://github.com/AIS2Lab/MCPSecBench) |
| **Type** | Security benchmark / testing framework |
| **Readiness** | **Research tool** |

MCPSecBench identifies **17 attack types across 4 attack surfaces** (user interaction, client, transport, server). Key finding: **over 85% of identified attacks successfully compromise at least one major platform** (Claude, OpenAI, Cursor). Current protection mechanisms have limited effectiveness.

#### Relevance to Unimatrix

**Medium relevance.** Useful as a testing framework for validating Unimatrix's security. The attack taxonomy informs threat modeling.

---

### 6.3 MCP Guardian

| Property | Value |
|---|---|
| **Paper** | arXiv:2504.12757 |
| **Type** | Security middleware layer |
| **Readiness** | **Research prototype** |

MCP Guardian intercepts all MCP tool calls via an override of `invoke_tool`, providing:
- Authentication and authorization per agent
- Rate limiting
- Regex-based WAF scanning
- Audit logging (complete activity trail)

Claims minimal performance overhead and no architectural changes to existing MCP servers.

#### Relevance to Unimatrix

**Medium relevance.** Unimatrix already implements similar patterns (content scanning via regex, audit logging, agent registry). MCP Guardian validates these design choices and provides additional patterns for rate limiting.

---

### 6.4 Attestable MCP Server

| Property | Value |
|---|---|
| **GitHub** | [kontext-dev/attestable-mcp-server](https://github.com/co-browser/attestable-mcp-server) |
| **Type** | Hardware attestation for MCP servers |
| **Technology** | Intel SGX, RA-TLS |
| **Readiness** | **Experimental** |

Uses Trusted Execution Environments (TEEs) and RA-TLS to prove an MCP server is running intended, untampered code. The TLS handshake includes an SGX quote embedded in an X.509 extension, enabling cryptographic verification of server code integrity.

#### Relevance to Unimatrix

**Low relevance.** Requires hardware TEE support (Intel SGX). Interesting for future hardened deployments but not practical for general local development scenarios.

---

### 6.5 mcp-scan / agent-scan (Snyk)

| Property | Value |
|---|---|
| **Organization** | Snyk (originally Invariant Labs) |
| **GitHub** | [snyk/agent-scan](https://github.com/invariantlabs-ai/mcp-scan) |
| **Type** | Static security scanner |
| **Readiness** | **Available tool** |

Statically scans MCP server tool descriptions for malicious content (tool poisoning, cross-origin escalation, rug pull attacks). Detects hidden instructions in tool metadata that are invisible to users but visible to AI models.

#### Relevance to Unimatrix

**Low direct relevance** (Unimatrix is the server, not a client scanning servers). However, Unimatrix's content scanning patterns (vnc-002) serve a similar purpose from the server side.

---

## 7. OWASP MCP Security Guidance

### Practical Guide for Secure MCP Server Development (Feb 2026)

| Property | Value |
|---|---|
| **Publisher** | OWASP Gen AI Security Project |
| **Date** | February 16, 2026 |
| **URL** | [OWASP Guide](https://genai.owasp.org/resource/a-practical-guide-for-secure-mcp-server-development/) |
| **Length** | 17 pages |
| **Readiness** | **Published guidance** |

#### Key Recommendations

1. **Isolation** -- never run an MCP server with host-system privileges.
2. **Short-lived tokens** -- minutes-long lifespans.
3. **OAuth 2.1/OIDC** -- enforce for all remote connections.
4. **Strict input validation** -- sanitize all tool parameters.
5. **Session isolation** -- prevent cross-session data leakage.
6. **Hardened deployment** -- container isolation, minimal permissions.

### OWASP MCP Top 10

The top 10 risks include:
1. Model misbinding
2. Context spoofing
3. Prompt-state manipulation
4. Insecure memory references
5. Covert channel abuse

These are amplified in agentic AI, model chaining, multi-modal orchestration, and dynamic role assignment scenarios.

#### Relevance to Unimatrix

**High relevance.** Unimatrix already implements several OWASP recommendations (input validation, content scanning, session isolation, audit logging). The guide validates Unimatrix's security-first design and identifies additional hardening areas.

---

## 8. Rust Cryptographic Libraries

### 8.1 ed25519-dalek

| Property | Value |
|---|---|
| **License** | BSD-3-Clause |
| **Crate** | [ed25519-dalek](https://crates.io/crates/ed25519-dalek) |
| **Maturity** | **Production-ready** |
| **Features** | Key generation, signing, verification, batch verification |
| **Performance** | Fast; constant-time signing |
| **Safety** | Keys zeroed on drop; `#[no_std]` compatible |

The standard Rust library for Ed25519 signatures. Used by Tenuo, Biscuit, and many other projects. Provides the `signature::Signer` and `signature::Verifier` traits.

### 8.2 ring

| Property | Value |
|---|---|
| **License** | ISC-style |
| **Crate** | [ring](https://crates.io/crates/ring) |
| **Maturity** | **Production-ready** |
| **Features** | Ed25519, ECDSA, RSA, HMAC, HKDF, AES-GCM, SHA |
| **Note** | Wraps BoringSSL; requires C compilation |

Comprehensive cryptographic library. The `ring-compat` crate provides trait compatibility with the `signature` crate ecosystem.

### 8.3 jsonwebtoken

| Property | Value |
|---|---|
| **License** | MIT |
| **Crate** | [jsonwebtoken](https://crates.io/crates/jsonwebtoken) |
| **Maturity** | **Production-ready** |
| **Backends** | `aws_lc_rs` or `rust_crypto` (feature-gated) |
| **Features** | JWT creation, decoding, validation (exp, nbf, iss, aud, sub) |

The most widely used Rust JWT library. Supports all standard algorithms. Recommends pre-computing `DecodingKey` for performance.

### 8.4 rusty_paseto

| Property | Value |
|---|---|
| **License** | MIT |
| **Crate** | [rusty_paseto](https://crates.io/crates/rusty_paseto) |
| **Version** | 0.9.0 |
| **Maturity** | **Stable** |
| **GitHub** | [rrrodzilla/rusty_paseto](https://github.com/rrrodzilla/rusty_paseto) |
| **Features** | V3 (NIST), V4 (Sodium) tokens; Local (symmetric) + Public (asymmetric) |

Type-driven PASETO implementation with three API layers: `batteries_included` (ergonomic), `generic` (flexible), `core` (low-level). V4 Public uses Ed25519.

**Alternative:** `pasetors` crate -- another PASETO implementation.

### 8.5 PASETO vs JWT for Agent Tokens

| Criterion | JWT | PASETO |
|---|---|---|
| Algorithm agility | Yes (source of vulnerabilities) | No (versioned, fixed algorithms) |
| `alg: none` attack | Possible | Impossible (no algorithm negotiation) |
| Key confusion attacks | Possible | Impossible |
| Symmetric mode | JWE (complex) | Local tokens (simple) |
| Asymmetric mode | JWS | Public tokens (Ed25519 in V4) |
| Ecosystem | Massive | Small but growing |
| Rust crate maturity | Production (`jsonwebtoken`) | Stable (`rusty_paseto`) |
| Token size | Comparable | Comparable |
| Parsing safety | Requires careful validation | Safe by design |

**Recommendation for Unimatrix:** PASETO V4 Public is the better choice for agent tokens. It eliminates entire classes of JWT vulnerabilities (algorithm confusion, `alg: none`, key confusion) by design. The trade-off is a smaller ecosystem, but since Unimatrix controls both token creation and verification, ecosystem size is irrelevant.

However, if Unimatrix adopts Tenuo or Biscuit, the token format decision is already made (both use Ed25519 directly, not JWT/PASETO).

### 8.6 cap-std

| Property | Value |
|---|---|
| **License** | Apache-2.0 with LLVM exception |
| **Organization** | Bytecode Alliance |
| **Crate** | [cap-std](https://crates.io/crates/cap-std) |
| **Version** | 4.0.0 (December 2025) |
| **Maturity** | **Production-ready** |

Capability-oriented version of Rust's standard library. Provides `Dir`, `TcpListener`, etc. that enforce capability-based access (e.g., a `Dir` handle can only access files within its subtree). Used in Wasmtime and other Bytecode Alliance projects.

#### Relevance to Unimatrix

**Medium relevance.** cap-std could enforce filesystem sandboxing for tool operations (e.g., restricting which directories an agent can access). Useful for defense-in-depth alongside capability tokens.

---

## 9. Agent Framework Authentication

### 9.1 LangChain/LangGraph

| Property | Value |
|---|---|
| **Platform** | LangGraph Platform (renamed LangSmith Deployment) |
| **Auth Model** | OAuth 2.0, custom authentication handlers |
| **Readiness** | **Production (cloud-only SaaS or Enterprise Self-Hosted)** |

LangGraph provides:
- `Auth` object for registering authentication functions on every request.
- Three-level authorization: Global, Resource, Action handlers.
- JWT token validation with user extraction.
- Resource-level access control with metadata filtering.

**Limitation:** Custom auth is only available for LangGraph Platform SaaS or Enterprise Self-Hosted deployments. Not available for self-hosted open-source deployments.

### 9.2 AutoGen (Microsoft)

| Property | Value |
|---|---|
| **Organization** | Microsoft |
| **Auth Model** | Azure AD / Microsoft Graph integration |
| **Readiness** | **Relies on external identity systems** |

AutoGen integrates with Azure identity services but does not implement its own agent authentication protocol. Authentication is delegated to the enterprise identity infrastructure.

### 9.3 CrewAI

| Property | Value |
|---|---|
| **Auth Model** | None built-in |
| **Readiness** | **No agent authentication** |

CrewAI does not implement agent-to-agent authentication. Agents within a crew operate with implicit trust. Cross-framework agent communication is not supported.

#### Relevance to Unimatrix

**Low relevance.** None of these frameworks implement LLM-spoofing-resistant authentication. They rely on external identity providers or implicit trust. This confirms that **robust agent authentication is an unsolved problem in the current framework ecosystem**.

---

## 10. Academic Research

### 10.1 CaMeL (Capabilities for Machine Learning) -- Google DeepMind

| Property | Value |
|---|---|
| **Paper** | arXiv:2503.18813 |
| **Authors** | Google DeepMind researchers |
| **Date** | March 2025 |
| **Type** | Defense architecture |
| **Readiness** | **Research prototype** |

CaMeL creates a protective system layer around the LLM using traditional software security principles:
- **Control flow integrity** -- extracts control/data flows from queries into a Python-like program.
- **Access control** -- capability-based restrictions on tool calls.
- **Information flow control** -- tracks data provenance through execution steps.
- **Dual-LLM architecture:** Privileged LLM (trusted input) + Quarantined LLM (untrusted content).

**Effectiveness:** Neutralizes 67% of attacks in AgentDojo benchmark. Reduces successful attacks to zero for some models (GPT-4o).

**Limitation:** Relies on users to define security policies. Risk of user fatigue from manual approval prompts.

**Key insight for Unimatrix:** CaMeL's capability model directly influenced Tenuo's design. The principle that **enforcement must be at the runtime layer, not the LLM layer** is the critical takeaway.

### 10.2 Agent Security Bench (ASB) -- ICLR 2025

| Property | Value |
|---|---|
| **Paper** | arXiv:2410.02644 |
| **Venue** | ICLR 2025 |
| **GitHub** | [agiresearch/ASB](https://github.com/agiresearch/ASB) |
| **Type** | Security benchmark |

ASB formalizes and benchmarks attacks/defenses for LLM-based agents:
- 10 scenarios, 10 agents, 400+ tools, 27 attack/defense methods, 7 evaluation metrics.
- Highest average attack success rate: **84.30%**.
- Current defenses show **limited effectiveness**.
- Novel Plan-of-Thought (PoT) Backdoor Attack proposed.

**Key finding for Unimatrix:** The 84.30% attack success rate confirms that **LLM-level defenses alone are insufficient**. Runtime enforcement (capability tokens, content scanning) is essential.

### 10.3 AI Agents with DIDs and VCs

| Property | Value |
|---|---|
| **Paper** | arXiv:2511.02841 |
| **Authors** | Garzon et al. |
| **Venue** | ICAART 2026 |

See Section 2.2 for details. Key contribution: demonstrates that LLMs cannot be trusted to enforce authentication protocols autonomously.

### 10.4 LLM Agent Communication Security Survey

| Property | Value |
|---|---|
| **Paper** | arXiv:2506.19676 |
| **Type** | Survey |

Comprehensive survey of LLM-driven agent communication protocols, security risks, and defenses. Identifies identity spoofing as a key threat: "compromised user credentials [can] establish fraudulent sessions, after which the attacker usurps the victim's identity."

---

## 11. MCP Client Implementations (Authentication Status)

| Client | Auth Implementation | Notes |
|---|---|---|
| **Claude Desktop** | STDIO: env vars; HTTP: OAuth 2.0 | No STDIO auth spec |
| **Claude Code** | OAuth 2.0 for remote servers | STDIO servers run with user env |
| **Cursor** | Session cookies, bearer tokens | 53% of MCP servers use static API keys |
| **VS Code (Copilot)** | OAuth 2.0 for remote | STDIO same as others |

**Critical observation:** All major MCP clients treat STDIO transport as implicitly trusted. There is no standard for STDIO-level agent authentication. Unimatrix operates over STDIO and must solve this gap independently.

---

## 12. Comparative Analysis

### Readiness Matrix

| Solution | License | Rust Native | Maturity | Local/Embedded | Agent-Specific | Spoofing Resistant |
|---|---|---|---|---|---|---|
| **Tenuo** | MIT/Apache-2.0 | Yes (core) | Beta | Yes | Yes | Yes (runtime) |
| **Biscuit** | Apache-2.0 | Yes (reference) | Production | Yes | No (general) | Yes (runtime) |
| **UCAN** | Apache-2.0 | Yes | Alpha | Yes | No | Partial |
| **SPIFFE/SPIRE** | Apache-2.0 | Yes (client) | Production | No (server required) | No | Yes |
| **SpiceDB/OpenFGA** | Apache-2.0 | Yes (client) | Production | No (server required) | No | N/A |
| **Auth0 for AI Agents** | Commercial | No | Production | No (cloud) | Yes | Yes |
| **A2A Protocol** | Apache-2.0 | No SDK | Beta | No (HTTP) | Yes | Partial |
| **AttestMCP** | N/A | No impl | Research | Yes (design) | Yes | Yes (design) |
| **CaMeL** | N/A | No | Research | Yes (design) | Yes | Yes (design) |
| **PASETO (rusty_paseto)** | MIT | Yes | Stable | Yes | No (token format) | Yes (by design) |
| **ed25519-dalek** | BSD-3 | Yes | Production | Yes | No (primitive) | Yes (crypto) |
| **cap-std** | Apache-2.0 | Yes | Production | Yes | No (OS-level) | Yes (OS-level) |

### Architectural Fit for Unimatrix

Unimatrix's constraints:
1. **STDIO transport** -- no HTTP, no OAuth
2. **Local-first** -- single machine, no external services
3. **Embedded** -- in-process, no separate server
4. **Rust** -- native crate preferred
5. **Low latency** -- verification on every tool call
6. **LLM-spoofing resistant** -- runtime enforcement, not LLM compliance

**Best fits:** Tenuo and Biscuit are the only solutions that meet all six constraints.

---

## 13. Recommendations for Unimatrix

### Primary Recommendation: Evaluate Tenuo and Biscuit

Both libraries deserve hands-on evaluation. The decision factors:

| Factor | Tenuo Advantage | Biscuit Advantage |
|---|---|---|
| Domain fit | Purpose-built for AI agents | General capability token |
| Maturity | 3 months, beta | 6+ years, production |
| Performance | ~27us verification | ~264-419us verification |
| Policy language | Structured constraints | Datalog (more flexible) |
| MCP integration | Has `[mcp]` feature | Would need wrapper |
| Risk | Low adoption, API churn | Stable, proven |

**Recommendation:** Start with **Biscuit** as the foundation (proven, stable, excellent Rust support) and monitor **Tenuo** as it matures. Biscuit's Datalog policy language can express all of Unimatrix's authorization rules, and the ~1ms overhead is negligible. If Tenuo reaches v1.0 with stable APIs and broader adoption, it may become the better choice due to its agent-specific design.

### Secondary Recommendations

1. **Use Ed25519 for all signing operations** -- ed25519-dalek is the Rust ecosystem standard. Both Tenuo and Biscuit use it internally.

2. **Adopt PASETO V4 Public for any custom tokens** -- if Unimatrix needs to issue its own tokens (beyond Biscuit/Tenuo), PASETO eliminates JWT's vulnerability surface. Use `rusty_paseto` or `pasetors`.

3. **Implement AttestMCP's patterns** -- even without the formal extension, Unimatrix should implement:
   - Capability attestation (agents prove they hold valid capabilities)
   - Message authentication (HMAC-SHA256 on JSON-RPC messages)
   - Origin tagging (distinguish agent-originated from injected content)

4. **Use cap-std for filesystem sandboxing** -- defense-in-depth for any tool that accesses the filesystem.

5. **Follow OWASP MCP guidance** -- Unimatrix already implements many recommendations. Validate against the February 2026 guide.

6. **Ignore SpiceDB/OpenFGA/SPIFFE/Auth0** -- these solve different problems at different architectural layers. They add operational complexity without proportional benefit for a local MCP server.

7. **Monitor A2A Protocol** -- if Unimatrix evolves to support multi-agent scenarios, A2A's signed Agent Cards and task-scoped tokens are the right pattern.

8. **Monitor Project NANDA** -- for future multi-project, multi-agent discovery scenarios.

### Implementation Strategy

The recommended approach for Unimatrix agent authentication:

```
Phase 1 (vnc-003 or similar):
  - Integrate Biscuit for capability tokens
  - Agent enrollment mints a Biscuit token with trust-level-appropriate capabilities
  - Every tool call verifies the Biscuit token (~1ms overhead)
  - Attenuation when agents delegate to sub-agents
  - Revocation via Biscuit's built-in mechanisms

Phase 2 (future):
  - Evaluate Tenuo if it reaches v1.0
  - Add PASETO tokens for cross-session agent identity
  - Implement AttestMCP-style message authentication
  - Add cap-std filesystem sandboxing for tool execution

Phase 3 (multi-agent/multi-project):
  - A2A-style signed agent cards for discovery
  - NANDA-style AgentFacts for capability attestation
```

---

## 14. References

### Libraries and Crates

1. Tenuo -- Capability Tokens for AI Agents. https://github.com/tenuo-ai/tenuo | https://crates.io/crates/tenuo | https://docs.rs/tenuo/latest/tenuo/
2. Biscuit -- Eclipse Biscuit Authorization Token (Rust). https://github.com/biscuit-auth/biscuit-rust | https://crates.io/crates/biscuit-auth | https://doc.biscuitsec.org/
3. UCAN -- User Controlled Authorization Network (Rust). https://github.com/ucan-wg/rs-ucan | https://crates.io/crates/ucan
4. Macaroons (Rust). https://github.com/macaroon-rs/macaroon
5. ed25519-dalek. https://crates.io/crates/ed25519-dalek | https://docs.rs/ed25519-dalek/
6. ring. https://crates.io/crates/ring
7. jsonwebtoken. https://crates.io/crates/jsonwebtoken | https://github.com/Keats/jsonwebtoken
8. rusty_paseto. https://crates.io/crates/rusty_paseto | https://github.com/rrrodzilla/rusty_paseto
9. pasetors. https://crates.io/crates/pasetors
10. cap-std. https://crates.io/crates/cap-std | https://github.com/bytecodealliance/cap-std
11. spiffe crate. https://crates.io/crates/spiffe | https://github.com/maxlambrecht/rust-spiffe
12. spicedb-client. https://crates.io/crates/spicedb-client
13. openfga-client. https://crates.io/crates/openfga-client | https://github.com/vakamo-labs/openfga-client
14. SpiceDB. https://github.com/authzed/spicedb | https://authzed.com/spicedb
15. OpenFGA. https://openfga.dev/ | https://github.com/openfga

### Specifications and Protocols

16. MCP Specification (2025-11-25). https://modelcontextprotocol.io/specification/2025-11-25
17. MCP Authorization. https://modelcontextprotocol.io/specification/draft/basic/authorization
18. MCP Auth Tutorial. https://modelcontextprotocol.io/docs/tutorials/security/authorization
19. A2A Protocol. https://github.com/a2aproject/A2A | https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/
20. A2A Protocol v0.3 Upgrade. https://cloud.google.com/blog/products/ai-machine-learning/agent2agent-protocol-is-getting-an-upgrade
21. SPIFFE. https://spiffe.io/ | https://spiffe.io/docs/latest/spire-about/spire-concepts/
22. PASETO Specification. https://paseto.io/
23. W3C DID Core. https://www.w3.org/TR/did-core/
24. W3C VC Data Model. https://www.w3.org/TR/vc-data-model-2.0/

### OWASP and Security Guidance

25. OWASP Practical Guide for Secure MCP Server Development. https://genai.owasp.org/resource/a-practical-guide-for-secure-mcp-server-development/
26. OWASP MCP Top 10. https://owasp.org/www-project-mcp-top-10/
27. OWASP Cheatsheet: Securely Using Third-Party MCP Servers. https://genai.owasp.org/resource/cheatsheet-a-practical-guide-for-securely-using-third-party-mcp-servers-1-0/
28. Top MCP Security Resources (Feb 2026). https://adversa.ai/blog/top-mcp-security-resources-february-2026/

### Academic Papers

29. Maloyan, N. & Namiot, D. (2026). "Breaking the Protocol: Security Analysis of the Model Context Protocol Specification and Prompt Injection Vulnerabilities in Tool-Integrated LLM Agents." arXiv:2601.17549. https://arxiv.org/abs/2601.17549
30. Garzon, S.R. et al. (2025). "AI Agents with Decentralized Identifiers and Verifiable Credentials." arXiv:2511.02841. https://arxiv.org/abs/2511.02841
31. Zhang et al. (2024/2025). "Agent Security Bench (ASB): Formalizing and Benchmarking Attacks and Defenses in LLM-based Agents." ICLR 2025. arXiv:2410.02644. https://arxiv.org/abs/2410.02644
32. "MCPSecBench: A Systematic Security Benchmark and Playground for Testing Model Context Protocols." arXiv:2508.13220. https://arxiv.org/abs/2508.13220
33. Google DeepMind (2025). "Defeating Prompt Injections by Design" (CaMeL). arXiv:2503.18813. https://arxiv.org/abs/2503.18813
34. "MCP Guardian: A Security-First Layer for Safeguarding MCP-Based AI System." arXiv:2504.12757. https://arxiv.org/abs/2504.12757
35. "A Survey of LLM-Driven AI Agent Communication: Protocols, Security Risks, and Defense Countermeasures." arXiv:2506.19676. https://arxiv.org/abs/2506.19676

### Industry and Vendor Resources

36. Auth0 for AI Agents. https://auth0.com/ai | https://auth0.com/blog/auth0-for-ai-agents-generally-available/
37. Auth0 Token Vault. https://auth0.com/features/token-vault | https://auth0.com/blog/auth0-token-vault-secure-token-exchange-for-ai-agents/
38. Auth0 MCP Integration. https://auth0.com/blog/mcp-and-auth0-an-agentic-match-made-in-heaven/
39. Okta Identity Security Fabric. https://www.okta.com/solutions/secure-ai/ | https://www.okta.com/blog/ai/okta-helps-secure-ai-agent-identity/
40. Vouched MCP-I. https://www.vouched.id/product/mcp-i-server-know-your-agent | https://www.vouched.id/ai-agents
41. Project NANDA. https://projectnanda.org/ | https://www.media.mit.edu/projects/mit-nanda/overview/
42. LangGraph Authentication. https://docs.langchain.com/langgraph-platform/auth | https://blog.langchain.com/agent-authorization-explainer/
43. MCP Auth Guide (Permit.io). https://www.permit.io/blog/the-ultimate-guide-to-mcp-auth
44. MCP Auth Guide (Stytch). https://stytch.com/blog/MCP-authentication-and-authorization-guide/
45. MCP Auth Guide (Stack Overflow). https://stackoverflow.blog/2026/01/21/is-that-allowed-authentication-and-authorization-in-model-context-protocol
46. Securing MCP Servers (Ping Identity). https://developer.pingidentity.com/identity-for-ai/agents/idai-securing-mcp-servers.html
47. Attestable MCP Server. https://github.com/co-browser/attestable-mcp-server
48. mcp-scan (Snyk). https://github.com/invariantlabs-ai/mcp-scan
49. MCPGuard. https://usemcpguard.io/ | https://arxiv.org/abs/2510.23673

### MCP Rust SDKs

50. Official Rust MCP SDK. https://github.com/modelcontextprotocol/rust-sdk
51. rust-mcp-sdk. https://github.com/rust-mcp-stack/rust-mcp-sdk
52. TurboMCP. https://github.com/Epistates/turbomcp
53. SSE MCP Server with OAuth in Rust (Shuttle). https://www.shuttle.dev/blog/2025/08/13/sse-mcp-server-with-oauth-in-rust

### Security Analyses

54. Cursor MCP Trust Bypass (Checkpoint). https://blog.checkpoint.com/research/cursor-ide-persistent-code-execution-via-mcp-trust-bypass/
55. MCP Security Risks (Pillar Security). https://www.pillar.security/blog/the-security-risks-of-model-context-protocol-mcp
56. MCP Security Risks (Red Hat). https://www.redhat.com/en/blog/model-context-protocol-mcp-understanding-security-risks-and-controls
57. MCP Server Security Audit (Rob Taylor). https://robt.uk/posts/2026-02-20-your-mcp-servers-are-probably-a-security-mess/
58. SPIFFE/SPIRE and Keylime (Red Hat). https://next.redhat.com/2025/01/24/spiffe-spire-and-keylime-software-identity-based-on-secure-machine-state/
59. JWT vs PASETO (Permify). https://permify.co/post/jwt-paseto/
60. CaMeL Analysis (Simon Willison). https://simonwillison.net/2025/Apr/11/camel/
