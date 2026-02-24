# ASS-008 Wave 2: Unimatrix as Orchestrator — Synthesis & Open Questions

**Date**: 2026-02-24
**Type**: Research Spike — Wave 2 Synthesis
**Predecessor**: `ANALYSIS.md` (Wave 1: LLM-resistant agent authentication)
**Companion files**:
- `RESEARCH-risk-cascading.md` — Does embedding an LLM make Unimatrix untrusted?
- `RESEARCH-sdk-integration.md` — Claude SDK, GitHub Apps, container trust, sandboxing
- `RESEARCH-multi-factor-auth.md` — 2FA patterns for AI agents
- `RESEARCH-trust-boundaries.md` — Trust boundary redesign when AI becomes infrastructure

---

## Executive Summary

Wave 1 established that agent authentication must operate outside the LLM's control surface. Wave 2 asks a harder question: **what happens when you move the LLM INSIDE the trusted system?**

The research reveals three fundamental findings:

1. **Risks DO cascade, but they CAN be contained** — if five conditions hold (the "Containment Invariant"). The formula: `trust(System(LLM)) = min(trust(System), trust(controls_around(LLM)))`. Trust depends on the quality of controls, not on the embedded component.

2. **The authentication problem mostly dissolves** — if Unimatrix IS the orchestrator, it controls agent lifecycle, injects credentials, and mediates all tool execution. The "who is calling?" question becomes trivial because Unimatrix spawns every agent.

3. **A new problem emerges: the Trust Sandwich** — trusted shell (Unimatrix) → untrusted core (LLM) → trusted data (knowledge store). The LLM sits between the system's interface and its data. Every external system that trusts Unimatrix implicitly trusts the LLM inside it — unless trust-breaking barriers are in place.

**The architectural answer**: the "Deterministic Shell / Generative Core" pattern (independently proposed by Google DeepMind CaMeL, Microsoft FIDES, and practitioners). The LLM proposes. Deterministic code validates and executes. The LLM never writes directly to the knowledge store, never touches credentials, never makes security decisions.

---

## 1. The Cascading Risk Question — Answered

### Do risks cascade?

**Yes, along every path where LLM output becomes system action without deterministic validation.**

If the embedded LLM can directly write to the knowledge store, modify agent registry entries, or use GitHub credentials — then Unimatrix inherits ALL of the LLM's risks. A prompt injection through retrieved knowledge could cause Unimatrix to push malicious code to GitHub, poison its own knowledge base, or escalate agent privileges.

### Can cascading be contained?

**Yes, if and only if five conditions hold — the Containment Invariant:**

| # | Condition | Purpose |
|---|-----------|---------|
| 1 | LLM cannot perform privileged operations directly | Process isolation |
| 2 | Every LLM output passes through deterministic validation | Air gap |
| 3 | Non-validatable operations require human approval | Human-in-the-loop |
| 4 | Unforgeable record of LLM behavior | Audit |
| 5 | System can detect and reverse LLM effects | Rollback |

If all five hold, the system is as trustworthy as its controls. If any is violated, trust degrades along that specific path.

### The Three Zones Model

Operations can be classified by LLM involvement:

| Zone | LLM Role | Validation | Example |
|------|----------|------------|---------|
| **Green** (deterministic) | None | N/A | Token verification, HMAC computation, schema validation |
| **Yellow** (LLM-assisted) | Proposes, human/system validates | Deterministic | Knowledge retrieval ranking, query interpretation, briefing assembly |
| **Red** (LLM-dependent) | Core decision-maker | Cannot fully validate | "Is this knowledge correct?", "Should this convention change?" |

**Rule**: Green zone operations can be fully trusted. Yellow zone operations are trustworthy if the deterministic validation is sound. Red zone operations must be flagged as LLM-influenced and never treated as authoritative without human confirmation.

---

## 2. The Authentication Problem — Transformed

### Current architecture (authentication is hard)

```
Agent (LLM) ──── self-reports identity ────> Unimatrix
              LLM controls the claim
              Spoofable, unverifiable
```

### Proposed architecture (authentication mostly dissolves)

```
Unimatrix (orchestrator)
  │
  ├── spawns Agent A with token T1, warrant W1
  ├── spawns Agent B with token T2, warrant W2
  │
  │   Unimatrix KNOWS who each agent is
  │   because it CREATED them
  │
  └── receives tool calls with tokens
      resolves T1 → Agent A (certain)
      resolves T2 → Agent B (certain)
```

When Unimatrix is the orchestrator, the identity problem becomes trivial for internally spawned agents. Unimatrix mints the token, injects it into the agent's environment, and resolves it on every request. The LLM never sees the token-to-identity mapping.

### Remaining authentication problem

External agents (not spawned by Unimatrix) still need authentication. For these, the Wave 1 recommendations apply: opaque tokens, warrants, process identity binding.

The architecture supports both:
- **Internal agents**: identity by construction (Unimatrix spawned them)
- **External agents**: identity by credential (tokens, warrants, SO_PEERCRED)

---

## 3. The Trust Sandwich

The deepest architectural challenge. When Unimatrix embeds an LLM:

```
┌─────────────────────────────────────────────┐
│  Trusted Shell (Unimatrix Rust code)        │
│  - auth, permissions, validation, audit     │
│                                             │
│  ┌───────────────────────────────────────┐  │
│  │  Untrusted Core (LLM via Claude API)  │  │
│  │  - reasoning, planning, understanding │  │
│  │  - CAN be prompt-injected             │  │
│  │  - CAN hallucinate                    │  │
│  │  - CAN attempt credential exfil       │  │
│  └───────────────────────────────────────┘  │
│                                             │
│  Trusted Data (knowledge store, redb)       │
│  - entries, indexes, audit log, registry    │
└─────────────────────────────────────────────┘
```

The LLM sits between the interface and the data. Every path from external input to stored knowledge passes through the LLM. This is both the value proposition (LLM understands the input) and the risk (LLM can be manipulated).

### The Deterministic Gateway

The solution is a hard boundary between the LLM and the data:

```
External input
    │
    ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ Input        │───>│ LLM          │───>│ Deterministic │───> Knowledge
│ Validation   │    │ (proposes    │    │ Gateway       │     Store
│ (det.)       │    │  actions)    │    │ (validates &  │
└──────────────┘    └──────────────┘    │  executes)    │
                                        └──────────────┘
```

The LLM produces structured output (e.g., `{ action: "store", title: "...", content: "...", topic: "..." }`). The deterministic gateway validates every field, checks permissions, computes hashes, applies content scanning, and only then writes to the store.

**The LLM never**:
- Holds a write transaction
- Sees signing keys or tokens
- Makes permission decisions
- Accesses the agent registry directly
- Constructs audit log entries

This is CaMeL (Google DeepMind), FIDES (Microsoft), and the "air gap" pattern — converging on the same architecture from different starting points.

---

## 4. Multi-Factor Authentication — Key Findings

### The critical insight for LLM 2FA

Traditional "something you know" is dangerous for LLMs — any secret in the LLM's context is vulnerable to exfiltration via prompt injection. Therefore, both factors must operate at architectural layers the LLM cannot access:

| Factor | Layer | LLM Can Access? |
|--------|-------|:---------------:|
| Opaque session token | Application (env var) | No |
| SO_PEERCRED | Kernel | No |
| mTLS certificate | Transport | No |
| DPoP proof-of-possession | Cryptographic | No |
| HMAC of request body | Runtime (computed by process, not LLM) | No |
| Biscuit capability token | Application (injected by orchestrator) | No |

**Prompt injection CANNOT defeat properly architected 2FA** when neither factor enters the LLM context.

### Recommended 2FA combinations for Unimatrix

| Transport | Factor 1 (HAVE) | Factor 2 (HAVE/VERIFY) | Resistance |
|-----------|----------------|----------------------|------------|
| Stdio (Phase 1) | Opaque token (env var) | HMAC of request body (runtime-computed) | High |
| Stdio (Phase 2) | Biscuit token (injected) | SO_PEERCRED (kernel-verified) | Very High |
| HTTP (future) | OAuth 2.1 bearer token | DPoP proof-of-possession (RFC 9449) | Very High |
| HTTP (enterprise) | OAuth 2.1 + mTLS | Client certificate (mutual TLS) | Maximum |

### Project-derived secrets

Your idea of binding auth to the repository context is valid but must be implemented carefully:

```
project_secret = HMAC-SHA256(repo_origin_url + unimatrix_install_key)
request_hmac = HMAC-SHA256(request_body, project_secret)
```

This proves the agent has access to both the repository AND the Unimatrix installation. The key is that `project_secret` is computed by the runtime process, never exposed to the LLM's context window.

---

## 5. External Systems — Do They Need to Protect FROM Unimatrix?

**Yes. And they already should, regardless of LLM embedding.**

Standard security hygiene says: grant Unimatrix only the permissions its deterministic components need, not what its LLM components might want.

### GitHub App permissions model

If Unimatrix is a GitHub App:

| Permission | Needed By | Should Grant? |
|-----------|-----------|:------------:|
| `contents: read` | Knowledge sync from repo | Yes |
| `contents: write` | Push generated code | Only with human approval per-push |
| `metadata: read` | Repo discovery | Yes |
| `issues: write` | Create/update tracking issues | Yes (low-risk) |
| `pull_requests: write` | Create PRs | Yes (requires human merge) |
| `actions: write` | Trigger CI | No (unless explicitly needed) |
| `administration` | Repo settings | Never |

**Key principle**: GitHub's own permission model provides the trust-breaking barrier. Unimatrix can only do what GitHub allows. If the embedded LLM is prompt-injected, the blast radius is bounded by the GitHub App's permission scope.

### Tiered trust labeling

External systems should know WHICH Unimatrix outputs involved LLM reasoning:

| Output | Zone | Consumer should |
|--------|------|----------------|
| Content hash verification | Green (deterministic) | Trust fully |
| Entry retrieval by ID | Green (deterministic) | Trust fully |
| Search ranking | Yellow (LLM-assisted) | Trust with awareness |
| Briefing synthesis | Yellow (LLM-assisted) | Verify key claims |
| "This convention should change" | Red (LLM-dependent) | Require human confirmation |

---

## 6. The Integration Architecture — Recommended

### Direct API Integration (not SDK wrapping)

The research recommends calling the Claude API directly from Rust rather than wrapping the Claude Code CLI or Agent SDK:

| Approach | Pros | Cons |
|----------|------|------|
| Wrap Claude Code CLI | Familiar tool, existing ecosystem | Subprocess fragility, hard to control |
| Wrap Agent SDK (Python) | Rich lifecycle hooks | FFI overhead, Python dependency |
| **Direct API calls (Rust)** | Maximum control, native types, no FFI | Must implement tool routing |

Unimatrix already has a Rust codebase. Calling the Claude API directly (via `reqwest` or the Anthropic Rust SDK when available) gives:
- Full control over what context the LLM sees
- Type-safe structured output parsing
- No credentials in subprocess environments
- Rust's type system enforces the deterministic gateway

### Rust Type System Enforcement

The research identifies a powerful technique: use Rust's type system to enforce the trust boundary at compile time.

```
// LLM output is a distinct type — cannot be used where validated output is expected
struct LlmProposal<T>(T);      // Unvalidated — cannot write to store
struct Validated<T>(T);          // Validated — can write to store

// The gateway function is the ONLY way to convert
fn validate(proposal: LlmProposal<StoreEntry>) -> Result<Validated<StoreEntry>, ValidationError> {
    // deterministic validation: schema, permissions, content scanning, hash computation
}

// Store::write only accepts Validated<T>
fn write(&self, entry: Validated<StoreEntry>) -> Result<EntryId, StoreError>
```

The LLM's output can never bypass validation because the type system prevents it. This is compile-time enforcement of the Containment Invariant.

---

## 7. The Big Picture — Architecture Evolution

```
Stage 1 (Current): Passive Knowledge Store
┌──────────┐     MCP      ┌────────────┐
│ LLM Agent│────────────>  │ Unimatrix  │
│ (external)│  self-report │ (passive)  │
└──────────┘    identity   └────────────┘
  Identity: self-reported (spoofable)
  Trust model: agent is untrusted, Unimatrix is trusted

Stage 2 (Wave 1): Authenticated Knowledge Store
┌──────────┐    MCP +     ┌────────────┐
│ LLM Agent│───token────> │ Unimatrix  │
│ (external)│  + warrant  │ (passive)  │
└──────────┘              └────────────┘
  Identity: infrastructure-verified (tokens, warrants, SO_PEERCRED)
  Trust model: agent proves identity cryptographically

Stage 3 (Wave 2): Active Orchestrator
┌───────────────────────────────────────┐
│              Unimatrix                │
│  ┌─────────────────────────────────┐  │
│  │ Deterministic Shell (Rust)      │  │
│  │  - auth, validation, gateway    │  │
│  │  ┌──────────────────────────┐   │  │
│  │  │ Claude API (generative)  │   │  │
│  │  │  - reasoning, planning   │   │  │
│  │  └──────────────────────────┘   │  │
│  └─────────────────────────────────┘  │
│                                       │
│  ├── spawns Agent A (internal)        │
│  ├── spawns Agent B (internal)        │
│  └── accepts Agent C (external, auth) │
└───────────────────────────────────────┘
  Identity: by construction (internal) or by credential (external)
  Trust model: Unimatrix is the trust anchor, LLM is sandboxed inside

Stage 4 (Future): Verified Autonomy
┌───────────────────────────────────────┐
│              Unimatrix                │
│  (Stage 3 + behavioral monitoring    │
│   + formal verification of gateway   │
│   + multi-project isolation          │
│   + tiered trust labeling)           │
└───────────────────────────────────────┘
  Trust model: external systems can reason about which outputs are
               deterministic vs. LLM-influenced, and act accordingly
```

---

## 8. Open Questions for Future Research

### Q1: Can the Containment Invariant be formally verified?

The five conditions are stated informally. Can they be expressed as properties that a formal verification tool (e.g., Kani for Rust) can check? Specifically: "no path exists from LLM output to store write without passing through validation."

### Q2: What about multi-project isolation?

When Unimatrix supports multiple projects, the embedded LLM might see knowledge from Project A while working on Project B. The KV-cache side-channel attack (NDSS 2025) shows this is a real risk. Mitigation: separate LLM sessions per project, or use Claude's system prompt to enforce isolation (imperfect — prompt injection can override).

### Q3: How does this interact with the Cortical learning phase?

Cortical (crt-*) features are designed to detect drift, contradictions, and anomalies. If the LLM is inside Unimatrix, can Cortical detection monitor the LLM's behavior? This creates a recursive situation: the system monitoring the LLM is the same system containing the LLM.

### Q4: What's the right granularity for human-in-the-loop?

Red zone operations require human confirmation. But if every knowledge write requires human approval, Unimatrix loses its value as an autonomous system. The question: which operations can be safely delegated to the deterministic gateway, and which genuinely need human judgment?

### Q5: Does the GitHub App model create a single point of compromise?

If Unimatrix's GitHub App credentials are exfiltrated (via the embedded LLM), the attacker gets repo access. Mitigations: short-lived installation tokens (1hr TTL), minimal permissions, and never exposing tokens to the LLM's context. But is this sufficient?

### Q6: What does "connected to the repository" actually prove?

Your question about proving the agent is connected to the repo is deeper than it seems. A project-derived secret proves access to repo metadata at one point in time. It doesn't prove ongoing authorization, and it doesn't prove the agent's intent is aligned with the project's goals.

---

## 9. Source Summary

This wave's research drew from 150+ additional references across 4 documents. Key new sources:

### Architectures
- Google DeepMind CaMeL (arXiv:2503.18813) — dual-LLM with capability tracking
- Microsoft FIDES (arXiv:2505.23643) — information flow control for AI
- Meta "Agents Rule of Two" — limit to 2 of 3 dangerous capabilities
- AWS Bedrock AgentCore — Cedar policy language, deterministic enforcement
- GKE Agent Sandbox — gVisor kernel-level isolation

### Trust & Risk
- de Vadoss "Byzantine Fault Tolerance for AI Safety" — oracle problem analysis
- MAESTRO Framework — centralization paradox
- NCSC Dec 2025 — "prompt injection may never be fixed"
- DigiNotar/Symantec CA failures — TTP compromise precedents

### MFA & Identity
- NIST NCCoE concept paper (Feb 2026) — machine identity multi-factor
- OpenID Foundation AIWG whitepaper — agent identity management
- RFC 9449 (DPoP) — proof-of-possession for OAuth
- Vault AppRole — dual-credential machine authentication

### SDK & Integration
- Claude Agent SDK documentation — lifecycle hooks, permission model
- Anthropic Sandbox Runtime — tool execution isolation
- Wasmtime/Wassette — WASM-based MCP tool sandboxing
- Landlock LSM — unprivileged Linux self-sandboxing

Full bibliographies in companion research documents.
