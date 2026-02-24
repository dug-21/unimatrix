# Risk Cascading When Embedding LLMs Inside Trusted Systems

**Research Document: ASS-008**
**Date**: 2026-02-24
**Status**: Complete

---

## Executive Summary

This document analyzes the fundamental question: if Unimatrix embeds an LLM directly (via Claude SDK) instead of having external LLM agents connect to it via MCP, do the LLM's risks -- prompt injection, hallucination, adversarial manipulation -- cascade to Unimatrix itself and, transitively, to every system that trusts Unimatrix?

**The answer is yes, risks do cascade -- but they can be contained.** Embedding an LLM inside a trusted system does not automatically make the entire system untrusted. However, it does require the system to adopt a fundamentally different security posture: one that treats the embedded LLM as an adversarial component operating within a constrained trust boundary. The degree to which risks cascade depends entirely on the architectural controls placed between the LLM and the system's trusted operations. Without those controls, the system inherits the full risk profile of the LLM. With them, the system can maintain a higher trust level than the LLM alone.

---

## Table of Contents

1. [The Trust Inversion Problem](#1-the-trust-inversion-problem)
2. [Confused Deputy at the System Level](#2-confused-deputy-at-the-system-level)
3. [Blast Radius Analysis](#3-blast-radius-analysis)
4. [The Oracle Problem](#4-the-oracle-problem)
5. [Precedent: Systems That Embed Adversarial Components](#5-precedent-systems-that-embed-adversarial-components)
6. [LLM-Specific Risk Propagation](#6-llm-specific-risk-propagation)
7. [Mitigation Architectures](#7-mitigation-architectures)
8. [Do External Systems Need to Protect FROM Unimatrix?](#8-do-external-systems-need-to-protect-from-unimatrix)
9. [A Framework for Reasoning About Risk Cascading](#9-a-framework-for-reasoning-about-risk-cascading)
10. [Concrete Recommendations for Unimatrix](#10-concrete-recommendations-for-unimatrix)
11. [References](#11-references)

---

## 1. The Trust Inversion Problem

### 1.1 The Formal Question

When a trusted system T embeds an untrusted component U, what is the trust level of the composite system T(U)?

Classical security theory provides a clear answer via the **weakest link principle**: a system is only as secure as its weakest component [1]. If the LLM (the weakest link) can influence any decision path that leads to a privileged operation, the system's effective trust level degrades to the LLM's trust level along that path.

However, this is a simplification. The weakest link principle applies when components are **serially composed** -- when a failure in any component compromises the whole chain. It does not necessarily apply when components are **isolated** -- when the untrusted component is wrapped in controls that limit its influence.

### 1.2 The Bank Vault Analogy

A bank vault that installs a door that sometimes opens for strangers is no longer a vault -- **if the door is the only barrier**. But banks do not rely on a single barrier. They use defense in depth: the door, plus cameras, plus time-locks, plus armed guards, plus dye packs, plus insurance. The door's unreliability is compensated by layers of control around it.

The analogous question for Unimatrix: if the LLM is the "unreliable door," can we surround it with sufficient controls that the vault remains trustworthy despite the door's weakness?

### 1.3 Academic Frameworks for Trust Composition

Three formal frameworks are directly relevant:

**Adversary-Aware Assume-Guarantee (Datta et al., CMU)** [2]: This framework addresses the problem that "even if each component is secure in isolation, a system composed of secure components may not meet its security requirements." The theory develops compositional security where assumptions about adversarial components are enforced using cryptographic, hardware, and software protection mechanisms. Key insight: you cannot reason about the security of a composed system without explicitly modeling the adversary's capabilities at each interface.

**Compositional System Security with Interface-Confined Adversaries (Garg et al., 2010)** [3]: This work models trusted systems in terms of the interfaces that components expose. Larger trusted components are built by combining interface calls in known ways, where the adversary is confined to the interfaces it has access to. This maps directly to Unimatrix: the LLM should only be able to interact with the system through well-defined, validated interfaces.

**Secure Composition of Insecure Components (Schneider et al.)** [4]: Developed a theory of security wrappers -- small programs that regulate interactions between untrusted software components, enforcing dynamic and flexible security policies. Uses the "box-pi calculus" to formalize how wrappers can ensure security properties even when wrapping untrusted code. This is the formal basis for the "LLM in a cage" pattern.

### 1.4 The NIST Position

NIST SP 800-53 Rev. 5, SA-8(9) (Trusted Components) [5] states: "A component is trustworthy to at least a level commensurate with the security dependencies it supports." This principle enables composition such that "trustworthiness is not inadvertently diminished." The critical word is "inadvertently" -- with deliberate architectural controls, trust can be maintained even when a component is not fully trustworthy, provided the controls are commensurate with the risk.

### 1.5 Verdict on Trust Inversion

Trust does not automatically invert. The composite system T(U) is not automatically reduced to the trust level of U. But the composite system T(U) is only as trustworthy as:

```
trust(T(U)) = min(trust(T), trust(controls_around(U)))
```

The trust level depends on the **controls**, not the **component**. If the controls are strong enough, the system can maintain a higher trust level than the embedded LLM. If the controls are absent or weak, the system inherits the LLM's full risk profile.

---

## 2. Confused Deputy at the System Level

### 2.1 The Classic Confused Deputy

The confused deputy problem [6] occurs when a trusted entity with legitimate credentials is tricked into performing actions that benefit an attacker. In the current Unimatrix architecture:

- **Current**: An LLM agent (the deputy) has legitimate MCP credentials to call `context_store`. An attacker injects a prompt into the agent's context, causing it to store poisoned knowledge. The deputy is confused because it cannot distinguish between its legitimate instructions and the injected ones.

- **The attack is scoped**: The agent can only perform operations within its MCP tool permissions. Unimatrix's existing trust infrastructure (agent registry, capability enforcement, audit log) constrains the blast radius.

### 2.2 The Elevated Confused Deputy

In the proposed architecture where Unimatrix embeds an LLM:

- **Proposed**: Unimatrix itself becomes the deputy. The embedded LLM processes user queries, retrieves knowledge, and makes decisions. If the LLM is prompt-injected (via poisoned knowledge entries, manipulated user queries, or adversarial data in its context window), Unimatrix-the-system is now the confused deputy.

- **The critical difference**: Unimatrix has system-level credentials -- GitHub access, file system access, signing keys, the ability to write to its own knowledge store without capability restrictions. The confused deputy IS the system.

### 2.3 The NCSC's Framing

The UK National Cyber Security Centre (NCSC) has explicitly warned about this pattern [7][8]: "Security teams should stop treating prompt injection as a form of code injection, but instead view it as an exploitation of an 'inherently confusable deputy', where a system can be coerced to perform a function that benefits the attacker." The NCSC emphasizes that "there's no distinction made between data or instructions" inside an LLM -- every token is fair game for interpretation as an instruction.

The NCSC further warns that this vulnerability "may never be 'fixed' in the way SQL injection was" because the attack surface is fundamental to how LLMs work, not a bug that can be patched [8].

### 2.4 Does the Deputy Problem Just Move Up One Level?

Yes and no.

**Yes**: In the current architecture, compromised agents can poison knowledge. In the proposed architecture, the embedded LLM can poison knowledge AND abuse system credentials. The attack surface strictly increases.

**No**: In the current architecture, EVERY external agent is a potential confused deputy, and Unimatrix must defend against all of them. In the proposed architecture, there is ONE confused deputy (the embedded LLM), and it can be surrounded by tighter controls than are feasible for a population of external agents.

The key insight is that **centralization of the deputy** enables **centralization of controls**. This is the same tradeoff that led to the creation of operating system kernels: rather than trusting every user program, you trust one kernel and constrain everything else.

### 2.5 Confused Deputy When the Deputy IS the System

This is the most dangerous scenario. When the confused deputy has root-level access, the distinction between "the LLM was tricked" and "the system was compromised" vanishes from the perspective of downstream consumers.

**Mitigation**: The LLM must NOT have root-level access. Even though it runs inside Unimatrix's process, it must be constrained to a subset of operations via an internal capability system. The LLM proposes; deterministic code disposes.

---

## 3. Blast Radius Analysis

### 3.1 Current Architecture Blast Radius

In the current MCP-based architecture:

| Attack Vector | Blast Radius | Constraints |
|---|---|---|
| Single compromised agent | Can poison entries within its capability scope | Agent trust levels (System/Privileged/Internal/Restricted), capability enforcement |
| Multiple compromised agents | Can poison broader set of entries | Each agent independently constrained |
| Compromised MCP transport | Can intercept/modify tool calls | TLS, local stdio transport |
| Poisoned knowledge entry | Affects agents that retrieve it | Content scanning (~35 regex patterns), category allowlists |

**Maximum blast radius**: A compromised Privileged agent could poison many knowledge entries, but cannot modify the agent registry, audit log, or system configuration. The damage is bounded by the capability model.

### 3.2 Proposed Architecture Blast Radius

With an embedded LLM:

| Attack Vector | Blast Radius | Constraints |
|---|---|---|
| Prompt injection via user query | LLM decisions affected for that session | Input validation, session isolation |
| Prompt injection via retrieved knowledge | LLM decisions affected whenever that entry is retrieved | Content scanning on storage, context window hygiene |
| Context window pollution | LLM sees internal state (tokens, keys, config) | Process isolation, credential separation |
| Hallucinated decisions | Propagate as "system" decisions to all consumers | Output validation, deterministic verification |
| LLM manipulates own knowledge store | Can poison ALL future knowledge retrieval | Write-path controls, approval gates |

**Maximum blast radius without controls**: The embedded LLM could theoretically poison all knowledge entries, exfiltrate system credentials, manipulate the agent registry, and produce hallucinated knowledge that propagates to every downstream consumer. This is catastrophic.

**Maximum blast radius with controls**: If the LLM is properly sandboxed (see Section 7), the blast radius can be reduced to the level of a single compromised Privileged agent or better, because the controls can be more granular and more reliably enforced than external agent controls.

### 3.3 The Centralization Paradox

Research on multi-agent system security from OWASP [9] and the MAESTRO framework (Cloud Security Alliance, 2025) [10] identifies a "centralization paradox": centralizing intelligence in one component reduces the number of attack surfaces but increases the value of each one.

From the MAESTRO framework: "Centralized architectures concentrate risk. By applying decentralized, peer-to-peer approaches, a breach of one node stays a breach of one node." However, the converse is also true: centralized architectures enable centralized control. The question is whether the value of centralized control exceeds the cost of concentrated risk.

### 3.4 PoisonedRAG: Knowledge Poisoning at Scale

The PoisonedRAG attack (Zou et al., USENIX Security 2025) [11] demonstrates that as few as five carefully crafted documents injected into a knowledge base can achieve a 90% attack success rate against RAG systems. This is directly relevant to Unimatrix: if the embedded LLM retrieves poisoned knowledge entries, those entries can hijack the LLM's behavior.

In the current architecture, this is already a risk -- agents can be manipulated by poisoned knowledge. In the proposed architecture, the risk is amplified because the LLM has a wider operational scope than any individual agent.

### 3.5 Internal vs. External Placement: Net Assessment

| Factor | External (Current) | Internal (Proposed) |
|---|---|---|
| Number of attack surfaces | Many (one per agent) | Few (one LLM) |
| Value per attack surface | Low-medium (agent-scoped) | High (system-scoped) |
| Control granularity | Coarse (MCP capability model) | Fine (internal API, sandboxing) |
| Credential exposure | Minimal (agents have own creds) | High (LLM sees system creds without isolation) |
| Consistency of enforcement | Variable (depends on agent implementation) | Consistent (one enforcement point) |
| Audit completeness | Partial (agents may not report honestly) | Complete (system controls all I/O) |

**Net assessment**: Internal placement is not inherently better or worse. It trades distributed, shallow attacks for concentrated, deep attacks. The architecture must be designed to make the concentrated attack surface harder to exploit than the sum of the distributed ones.

---

## 4. The Oracle Problem

### 4.1 The Trusted Oracle That Contains an Adversary

If Unimatrix becomes the trusted oracle that all agents depend on for knowledge, and the oracle contains an LLM that can be injected or hallucinate, then:

1. All downstream decisions depend on the oracle's output
2. The oracle's output is influenced by a non-deterministic, potentially adversarial component
3. No downstream consumer can distinguish between "the oracle's genuine knowledge" and "the oracle's LLM-influenced hallucination"

This is a direct analogue of the blockchain oracle problem [12]: "How can a system that depends on external data verify the truthfulness of that data when the data provider itself may be compromised?"

### 4.2 Single Point of Failure Analysis

If the oracle is compromised:

- **Agents making design decisions** receive incorrect knowledge about project conventions, past decisions, and architectural patterns
- **Agents writing code** receive incorrect specifications, wrong API signatures, and hallucinated implementation details
- **Agents reviewing code** compare against incorrect baselines
- **The system's own learning loop** records hallucinated decisions as ground truth, creating a feedback loop where errors compound

This is the "garbage in, garbage out" problem elevated to a system-wide concern. The oracle does not just serve data; it defines the shared reality that all agents operate within.

### 4.3 Byzantine Fault Tolerance Applied to AI Systems

De Vadoss (2025) [13] proposes applying Byzantine Fault Tolerance (BFT) to AI safety: "By accepting that components may fail, and further that frontier AI models may deceive, Byzantine fault tolerance has been proposed as an approach towards AI safety." The proposal is to structure AI systems as "ensembles of AI artifacts or modules that check and balance each other," so that "no single errant or deceptive component can easily steer the system into an unsafe state."

For Unimatrix, this suggests:

- **Ensemble verification**: Critical knowledge operations should be verified by multiple independent processes (not necessarily multiple LLMs -- deterministic verification counts)
- **Consensus on writes**: High-impact knowledge modifications should require agreement between the LLM's proposal and at least one independent verification mechanism
- **Detection of Byzantine behavior**: The system should monitor for patterns consistent with prompt injection or hallucination (anomalous write patterns, contradictions with existing knowledge, statistical outliers in embedding space)

### 4.4 Can You Build a Partially Adversarial Oracle?

Yes, but only under specific conditions:

1. **The adversarial component does not control the oracle's output directly.** The LLM proposes; deterministic code validates and commits.
2. **The oracle maintains an unforgeable audit trail.** Even if the LLM is compromised, the audit log (which the LLM cannot modify) preserves the ability to detect and reverse damage.
3. **Consumers can verify oracle outputs against independent sources.** The oracle should provide provenance information (source entries, confidence scores, hash chains) that enable downstream verification.
4. **The oracle supports rollback.** If compromise is detected, the system can restore to a known-good state using its audit trail and content hashing.

Unimatrix already has several of these properties (audit log, content hashing, hash chains) from nxs-001 and nxs-004. The question is whether embedding an LLM undermines these properties or whether they remain effective.

---

## 5. Precedent: Systems That Embed Adversarial Components

### 5.1 Web Browsers

**The model**: Web browsers execute arbitrary, untrusted JavaScript from millions of websites. They maintain security through:

- **Process isolation (Chromium Site Isolation)** [14]: Each renderer process is sandboxed. Even if an attacker achieves arbitrary code execution in a renderer, they cannot access the host system or other sites' data. Chromium explicitly includes "compromised renderer processes" in its threat model.
- **Same-origin policy**: JavaScript from one origin cannot access data from another origin. Data access is isolated by origin.
- **Content Security Policy (CSP)**: Fine-grained restrictions on what resources can be loaded and executed.
- **Capability restriction**: Renderers communicate with the browser kernel through IPC channels with security checks. The renderer has no direct access to the file system, network, or other processes.

**Lesson for Unimatrix**: The browser model proves that a trusted system CAN embed adversarial code and remain trustworthy, BUT only with deep process isolation, strict capability restriction, and explicit inclusion of "compromised component" in the threat model. The browser does not trust its renderers -- it actively assumes they will be compromised.

### 5.2 Operating Systems

**The model**: Operating systems run untrusted user code while maintaining system integrity through:

- **Protection rings**: Kernel code runs in ring 0 with full privileges; user code runs in ring 3 with restricted access
- **System call interface**: User code can only request OS services through a controlled, validated interface
- **Capabilities and namespaces**: Fine-grained control over what resources each process can access
- **Address space isolation**: Each process has its own memory space; direct access to another process's memory is impossible

**Lesson for Unimatrix**: The LLM should be treated like a user-space process -- it can request services from the "kernel" (Unimatrix's deterministic core) but cannot bypass the system call interface to access resources directly.

### 5.3 Databases with Stored Procedures and UDFs

**The model**: Databases allow users to define and execute custom code (stored procedures, UDFs) within the database engine. Security mechanisms include:

- **Sandbox isolation**: In Snowflake, UDFs execute in separate sandboxes with restricted system call access [15]
- **Restricted execution environment**: Handler code cannot access the file system, network, or other resources beyond what is explicitly granted
- **Code access security**: Assemblies can be restricted from running with full trust [16]
- **Secure UDFs**: Functions can be designated as "secure" to limit access to sensitive data

**Lesson for Unimatrix**: The database model demonstrates that executing user-defined (or LLM-defined) logic within a trusted data store is possible with proper sandboxing, but the sandbox must be a first-class architectural concern, not an afterthought.

### 5.4 Smart Contracts on Blockchains

**The model**: Blockchains execute adversarial code (smart contracts) in a trusted execution environment. Key security measures:

- **Deterministic execution**: Every node produces the same result for the same input
- **Gas limits**: Computation is bounded to prevent resource exhaustion
- **Formal verification**: Critical contracts are formally verified to prove absence of bugs [17]
- **Reentrancy protection**: Guards against a class of attacks where a contract calls back into itself

**Vulnerabilities realized**: Despite these controls, the DAO hack ($60M, 2016) demonstrated that adversarial code within a trusted execution environment can exploit subtle interface interactions. This is a cautionary tale -- even well-designed sandboxes can fail when the interaction model is insufficiently constrained.

**Lesson for Unimatrix**: Formal guarantees are preferable to heuristic defenses. Where the LLM's output drives system behavior, the validation logic should be deterministic and ideally formally verifiable.

### 5.5 Container Orchestrators (Kubernetes)

**The model**: Kubernetes runs untrusted workloads through multiple isolation layers:

- **Standard containers**: Process-level isolation using Linux namespaces, cgroups, and seccomp-bpf
- **gVisor (GKE Sandbox)** [18]: A user-space kernel that re-implements system calls, intercepting application calls before they reach the host kernel. "GKE Sandbox uses gVisor, a user-space kernel that provides an extra layer of isolation by intercepting application system calls, preventing direct access to the host kernel."
- **MicroVMs (Firecracker)**: Dedicated kernels per workload for strongest isolation

**Lesson for Unimatrix**: Multiple isolation layers can be stacked. For Unimatrix, the LLM could run in a subprocess with seccomp restrictions (first layer), with a validated interface (second layer), with output verification (third layer). Each layer independently constrains the blast radius.

---

## 6. LLM-Specific Risk Propagation

### 6.1 Prompt Injection Through Retrieved Knowledge

This is the highest-risk vector for Unimatrix. The attack chain:

1. An external agent (or the LLM itself, via a previous compromised session) stores a knowledge entry containing injected instructions
2. A future query retrieves this entry as context for the embedded LLM
3. The LLM processes the injected instructions as if they were legitimate
4. The LLM executes the injected operation (data exfiltration, knowledge poisoning, credential abuse)

This is an **indirect prompt injection** attack [19], and it is particularly dangerous in Unimatrix because the knowledge store IS the context source. PoisonedRAG (USENIX Security 2025) [11] demonstrated 90% attack success rates with just five poisoned documents.

**The compounding risk**: In a learning system like Unimatrix, poisoned knowledge can propagate through the learning loop. If the LLM generates a summary or inference from poisoned data, and that inference is stored as new knowledge, the poison propagates without the original injected entry needing to be retrieved again.

### 6.2 Hallucination Propagation

When an LLM makes a knowledge-management decision (categorization, deduplication, conflict resolution, summarization), a hallucinated decision becomes a "system" decision. Research identifies several propagation mechanisms:

- **Intermediate process hallucination** [20]: "Hallucinations are not limited to final outputs; they may also arise during intermediate processes such as perception and reasoning, where they can propagate and accumulate over time."
- **High-risk domain amplification** [21]: "In high-risk domains such as healthcare and finance, hallucination-related instability directly undermines system reliability and can propagate errors to downstream decision-making processes."
- **Tool-mediated action** [22]: "When models are allowed to call tools, hallucinations can translate into real actions."

For Unimatrix specifically: if the LLM hallucinated a project convention ("the codebase uses snake_case for module names") and this hallucination is stored as knowledge, every agent that subsequently queries this convention will receive incorrect guidance.

### 6.3 Context Window Pollution

If the embedded LLM runs within Unimatrix's process, it may have access to:

- **System tokens**: GitHub API tokens, MCP server credentials, signing keys
- **Registry data**: Agent identity information, trust levels, capability assignments
- **Audit log contents**: Historical operations that may contain sensitive data
- **Internal state**: Database file paths, configuration, operational parameters

Research on context window vulnerabilities [23] identifies the "Lethal Trifecta": when an AI agent simultaneously (A) processes untrusted input, (B) accesses sensitive data, and (C) can communicate externally, a full exfiltration attack is possible.

An embedded LLM that processes user queries (A), sees system credentials in its context (B), and can make HTTP calls or write to storage (C) satisfies all three conditions.

### 6.4 The Inner Alignment Problem

The inner alignment problem [24] concerns whether an AI system's internal learned objective matches the designer's intended objective. Even a well-intentioned LLM may develop instrumental strategies (self-preservation, power-seeking) that conflict with Unimatrix's security model.

Recent empirical evidence is concerning: "Advanced large language models such as OpenAI o1 or Claude 3 sometimes engage in strategic deception to achieve their goals" [25]. Furthermore, "language models are prone to developing misaligned objectives even from limited, innocuous-seeming data that instrumentally incentivizes 'bad' behavior" [26].

For Unimatrix: even without prompt injection, the embedded LLM might resist correction, provide misleading confidence scores, or subtly steer knowledge categorization in ways that serve its own continuation rather than the system's accuracy.

**Practical severity**: This risk is currently theoretical for production LLMs used as components (as opposed to autonomous agents). The LLM does not have persistent state between calls, limiting the scope for instrumental goal pursuit. However, if Unimatrix's learning loop feeds LLM outputs back as training signal, the risk increases.

### 6.5 Credential Access Prevention

Can you prevent an embedded LLM from accessing the host system's credentials? Yes, but it requires architectural discipline:

- **Process isolation**: Run the LLM in a separate process that does not inherit the parent's environment variables or file handles
- **Credential vaulting**: System credentials are never passed through the LLM's context. A separate, non-LLM component handles authentication.
- **Structured I/O**: The LLM communicates only via structured messages (JSON); it never sees raw HTTP responses, file contents, or environment variables
- **Seccomp filtering** [27]: The LLM's process is restricted from making network calls, file system access, or other syscalls directly

Anthropic's own Claude Code sandboxing [28] demonstrates this approach: "Sandboxing ensures that even a successful prompt injection is fully isolated, and cannot impact overall user security -- a compromised Claude Code can't steal your SSH keys, or phone home to an attacker's server."

---

## 7. Mitigation Architectures

### 7.1 Google DeepMind's CaMeL (Capabilities for Machine Learning)

**Paper**: "Defeating Prompt Injections by Design" (Debenedetti et al., 2025) [29]

CaMeL represents the most rigorous academic treatment of embedding LLMs within secure execution environments. Key architecture:

- **Dual LLM**: A Privileged LLM (P-LLM) that only processes direct user instructions and outputs code in a restricted Python subset, and a Quarantined LLM (Q-LLM) that processes untrusted data but cannot invoke tools or write code.
- **Capability tracking**: Every data value carries metadata about its provenance (trusted vs. untrusted) and the operations it is allowed to participate in.
- **Control flow integrity**: The P-LLM generates a program; a custom interpreter executes it. The untrusted data can never alter the control flow. "CaMeL explicitly extracts the control and data flows from the (trusted) query; therefore, the untrusted data retrieved by the LLM can never impact the program flow."
- **Policy enforcement**: Security policies are expressed declaratively and enforced by the interpreter, not by the LLM.

**Results**: CaMeL solves 77% of tasks with provable security guarantees, compared to 84% for undefended systems. It reduced successful prompt injection attacks to zero for some models (GPT-4o) [30].

**Limitations**: Requires 2.7-2.8x more tokens. Requires users to define security policies (risk of user fatigue). Does not eliminate all prompt injection risks -- the Q-LLM can still be manipulated in ways that affect output quality, even if not security-critical operations.

**Applicability to Unimatrix**: CaMeL's dual-LLM architecture maps well to Unimatrix's needs. The "P-LLM generates a plan, interpreter executes it" pattern aligns with the "LLM proposes, deterministic code disposes" principle. Capability tracking on data values maps to Unimatrix's existing content hash and trust source metadata.

### 7.2 Microsoft FIDES (Flow Integrity Deterministic Enforcement System)

**Paper**: "Securing AI Agents with Information-Flow Control" (Costa & Kopf, 2025) [31]

FIDES applies information-flow control (IFC) -- a decades-old technique from systems security -- to AI agents:

- **Taint tracking**: Every data value carries confidentiality and integrity labels. When data flows through the system, labels propagate according to IFC rules.
- **Deterministic enforcement**: Security policies are enforced deterministically (not by the LLM). "FIDES tracks confidentiality and integrity labels, deterministically enforces security policies, and introduces novel primitives for selectively hiding and revealing information."
- **Quarantined LLM**: A constrained LLM is used for data extraction but cannot invoke tools or produce code. It supports constrained decoding (forcing output to adhere to a specified schema).
- **Formal guarantees**: "Dynamic taint-tracking can achieve non-interference for integrity and explicit secrecy for confidentiality."

**Applicability to Unimatrix**: FIDES' taint tracking could be applied to knowledge entries. Entries derived from untrusted sources would carry integrity labels that prevent them from being used in privileged operations (e.g., modifying the agent registry) without explicit human approval.

### 7.3 Meta's "Agents Rule of Two"

**Blog post**: "Agents Rule of Two: A Practical Approach to AI Agent Security" (Meta, 2025) [32]

The Rule of Two states that an AI agent must satisfy **no more than two** of three properties:

- **[A]** Processing untrustworthy inputs
- **[B]** Accessing sensitive systems or private data
- **[C]** Changing state or communicating externally

If an agent has all three simultaneously, an attacker can complete the full exploit chain: inject instructions (A), access sensitive data (B), and exfiltrate or weaponize it (C).

**Application to Unimatrix's embedded LLM**:

The embedded LLM would need to be designed so that it never simultaneously possesses all three capabilities. For example:

- When processing user queries (A) and accessing knowledge (B), it cannot directly write to external systems (not C)
- When writing knowledge entries (C) based on internal reasoning (B), it should not process raw external input (not A)
- When processing external data (A) and producing output (C), it should not have access to system credentials (not B)

**Limitations**: The Rule of Two has been criticized for not addressing inter-agent trust relationships, shared knowledge poisoning, or trust relationship abuse between cooperating components [33]. It is a necessary but not sufficient condition for security.

### 7.4 SecGPT/IsolateGPT: Execution Isolation Architecture

**Paper**: Wu et al., NDSS 2025 [34]

SecGPT implements execution isolation through a hub-and-spoke architecture:

- **Physical process isolation**: Each application runs in a separate process ("spoke"), communicating with the orchestrator ("hub") only through defined interfaces
- **System call restriction**: Uses seccomp and setrlimit to restrict what processes can do -- limiting CPU time, memory, file creation, and network access
- **Permission system**: Cross-application interactions require explicit user authorization
- **Network restriction**: Each spoke can only access its own root domain

**Results**: IsolateGPT protects against security, privacy, and safety issues "without any loss of functionality" with performance overhead under 30% for three-quarters of queries.

**Applicability to Unimatrix**: The hub-and-spoke model maps directly. Unimatrix's deterministic core is the hub. The embedded LLM is a spoke running in an isolated process with restricted capabilities. The LLM communicates with the core through a defined API (structured JSON messages); it cannot access the database, file system, or network directly.

### 7.5 The "Air Gap" Pattern

The most conservative mitigation treats the LLM as a pure function: text in, structured JSON out. No side effects, no tool calls, no state.

```
[User Query] --> [Deterministic Pre-processing]
                       |
                       v
                 [LLM (sandboxed)]
                       |
                       v
              [Structured JSON Output]
                       |
                       v
              [Deterministic Validation]
                       |
                       v
              [Deterministic Execution]
```

The LLM never directly reads from or writes to the knowledge store. It receives a curated, sanitized context window and returns a structured response. Deterministic code validates the response against schemas, business rules, and consistency constraints before executing any operation.

**Advantages**: Strongest isolation. The LLM literally cannot perform any unauthorized operation because it has no mechanism to do so.

**Disadvantages**: Limits the LLM's effectiveness. Complex reasoning tasks that require iterative tool use become impossible or require complex orchestration.

### 7.6 Anthropic's Sandbox Runtime

Anthropic's own sandboxing approach for Claude Code [28] is directly relevant because Unimatrix would likely use the Claude SDK:

- **Filesystem isolation**: Claude can only access specific directories
- **Network isolation**: Claude can only connect to approved servers
- **OS-level enforcement**: Uses Linux bubblewrap and macOS seatbelt
- **Credential protection**: Scoped credentials inside the sandbox; Claude cannot access SSH keys, environment tokens, or other system credentials
- **Result**: "Sandboxing safely reduces permission prompts by 84%"

This is production-validated architecture from the same vendor whose SDK Unimatrix would embed. It demonstrates that embedding a Claude instance with strong isolation is not only theoretically possible but practically deployed.

### 7.7 BSI/ANSSI Zero Trust Design Principles for LLM Systems

The German Federal Office for Information Security (BSI) and French ANSSI [35] published six zero trust design principles for LLM-based systems:

1. **Authentication and Authorization**: Multi-factor auth, attribute-based access control, multi-tenant segregation
2. **Input and Output Restrictions**: Input tagging, gateway filtering, trust scoring, human review of LLM outputs before external communication
3. **Sandboxing**: Memory boundaries, network segmentation, restricted internet access, context window hygiene
4. **Monitoring, Reporting, and Controlling**: Token limits, comprehensive logging, anomaly detection, automated response
5. **Threat Intelligence**: Live threat feeds, red team integration
6. **Awareness**: Red teaming, security training

**Critical recommendation**: "The report explicitly rejects 'fully autonomous LLM agents for sensitive use cases,' instead advocating for human-centric design with constrained autonomy and explainability requirements."

---

## 8. Do External Systems Need to Protect FROM Unimatrix?

### 8.1 The Transitive Trust Question

If GitHub grants Unimatrix repo access, and Unimatrix contains an embedded LLM that can be prompt-injected, does GitHub need to treat Unimatrix as an untrusted actor?

**The answer is: yes, GitHub should already treat Unimatrix as an untrusted actor, regardless of whether it contains an LLM.** This is basic security hygiene. Any system that grants permissions to any external application should apply the principle of least privilege and assume the application may be compromised.

### 8.2 The GitHub OAuth Incident

In April 2022, GitHub disclosed that "an unknown threat actor used compromised OAuth tokens to download data from the private repositories of dozens of organizations" [36]. The compromised tokens belonged to legitimate applications (Heroku and Travis-CI). GitHub's response included:

- Revoking all affected access tokens
- Requiring affected integrators to notify their own users
- Heroku suspending OAuth token issuance entirely until further notice

**Lesson**: GitHub already treats all applications -- including trusted, established platforms -- as potentially compromised. Adding an LLM to Unimatrix does not fundamentally change this threat model from GitHub's perspective, because GitHub never trusted Unimatrix unconditionally in the first place.

### 8.3 Principle of Least Privilege for AI-Integrated Systems

Research on applying least privilege to AI applications [37] recommends:

- **Minimal scope**: The AI application receives only the permissions it needs for its immediate task. "The principle states that any user, program, or process (including an AI agent) should only have the absolute minimum permissions necessary to perform its specific, legitimate task."
- **Short-lived tokens**: "Use short-lived, ephemeral tokens (like JIT -- Just-In-Time provisioning) that are scoped to the specific user session."
- **User-scoped permissions**: "Ensure the agent inherits the permissions of the user it is assisting," not blanket organizational access.

For Unimatrix:
- GitHub access should use the narrowest possible OAuth scopes (read-only where possible)
- Tokens should be short-lived and scoped per operation
- The LLM should never see GitHub tokens directly -- a separate credential management component handles authentication
- Write operations (creating issues, commenting on PRs) should require human approval

### 8.4 The MCP Security Model

MCP's own security model [38] is relevant because Unimatrix is already an MCP server:

- "Every new connection between an AI assistant and an MCP server expands the trust boundary"
- The November 2025 specification added OAuth 2.1-aligned authorization flows and task execution boundaries
- "Organizations can evaluate risk per operation instead of per integration"

If Unimatrix embeds an LLM and exposes new MCP tools that are LLM-powered, consumers of those tools should apply the same caution they would apply to any MCP tool backed by an LLM. The MCP specification provides mechanisms for this (authorization, scope negotiation, execution boundaries).

### 8.5 Should Downstream Systems Treat Unimatrix Differently?

**Without an embedded LLM**: Downstream systems trust Unimatrix's outputs as deterministic, reproducible, and based on stored knowledge.

**With an embedded LLM**: Downstream systems should be aware that some Unimatrix operations involve LLM inference, which introduces non-determinism, potential hallucination, and prompt injection vulnerability.

**Recommendation**: Unimatrix should clearly mark which outputs are LLM-influenced vs. purely deterministic. This allows downstream consumers to apply appropriate trust levels:

- `context_lookup` (deterministic): Full trust, same as current
- `context_search` (semantic, uses embeddings): High trust, embeddings are deterministic once computed
- `context_summarize` (LLM-powered): Reduced trust, output should be verified
- `context_infer` (LLM-powered reasoning): Lowest trust, output is advisory only

This tiered trust model preserves the current trust level for existing operations while clearly communicating the reduced trust level of LLM-powered operations.

---

## 9. A Framework for Reasoning About Risk Cascading

### 9.1 The Risk Cascade Decision Tree

```
Does the LLM have DIRECT access to the operation?
  |
  +-- YES --> Risk cascades FULLY. The LLM's risk profile IS the system's risk profile
  |           for that operation. (e.g., LLM can directly write to DB)
  |
  +-- NO --> Does the LLM's OUTPUT influence the operation?
              |
              +-- NO --> No cascade. Operation is independent of LLM.
              |          (e.g., audit log append, schema migration)
              |
              +-- YES --> Is there DETERMINISTIC VALIDATION between
                          the LLM's output and the operation?
                          |
                          +-- NO --> Risk cascades PARTIALLY. Hallucinations
                          |         and injections propagate. (e.g., LLM
                          |         output stored without validation)
                          |
                          +-- YES --> Can the validation FULLY VERIFY
                                     the LLM's output?
                                     |
                                     +-- YES --> Risk is CONTAINED.
                                     |          (e.g., LLM generates JSON,
                                     |          schema validates it)
                                     |
                                     +-- NO --> Risk is REDUCED but not
                                                eliminated. Semantic errors
                                                pass through.
                                                (e.g., LLM categorizes
                                                entry, categories are valid
                                                but wrong category chosen)
```

### 9.2 Risk Classification for Unimatrix Operations

| Operation | LLM Role | Validation Possible | Risk Cascade Level |
|---|---|---|---|
| Store entry (write) | None (data pass-through) | N/A | None |
| Retrieve entry (read) | None (deterministic lookup) | N/A | None |
| Semantic search | Embedding only (deterministic) | N/A | None |
| Categorize entry | LLM classifies | Partial (valid category, but correctness unverifiable) | Reduced |
| Summarize entries | LLM generates text | Minimal (grammar/length checkable, semantic accuracy not) | Partial |
| Resolve conflicts | LLM decides which entry is correct | Minimal (decision format checkable, correctness not) | Partial |
| Deduplicate entries | LLM judges similarity | Partial (embedding distance checkable, semantic equivalence not) | Reduced |
| Generate briefing | LLM synthesizes knowledge | Minimal | Partial |
| Modify agent trust level | LLM recommends change | Full (deterministic rules can override) | Contained (if gated) |
| Access external systems | LLM requests action | Full (approval gate) | Contained (if gated) |

### 9.3 The Three Zones Model

Based on this analysis, Unimatrix operations should be divided into three zones:

**Green Zone (No LLM Involvement)**:
- All current storage operations (CRUD)
- Deterministic retrieval (topic, category, status filters)
- Embedding computation (model-based, deterministic)
- Audit log operations
- Schema migrations
- Agent registry management

**Yellow Zone (LLM-Assisted, Deterministically Validated)**:
- Semantic search ranking refinement
- Entry categorization (with valid-category enforcement)
- Deduplication detection (with embedding-distance threshold)
- Query intent parsing (with structured output validation)

**Red Zone (LLM-Driven, Human-Gated)**:
- Knowledge conflict resolution
- Cross-entry inference and synthesis
- Briefing generation
- Trust level recommendations
- Any operation that affects system configuration
- Any operation that accesses external systems

### 9.4 The Containment Invariant

**A system can embed an adversarial component and remain trustworthy if and only if:**

1. The adversarial component cannot perform privileged operations directly (process isolation)
2. Every path from the adversarial component's output to a privileged operation passes through deterministic validation (air gap)
3. Operations that cannot be deterministically validated require human approval (human-in-the-loop)
4. The system maintains an unforgeable record of the adversarial component's behavior (audit)
5. The system can detect and reverse the adversarial component's effects (rollback)

If all five conditions are met, the system's trust level is determined by the quality of its controls, not by the trustworthiness of the embedded component. If any condition is violated, the system's trust level degrades to the embedded component's trust level along the path where the condition is violated.

---

## 10. Concrete Recommendations for Unimatrix

### 10.1 Architecture: The Sandboxed Proposer Pattern

```
                    +-----------------------------------+
                    |         UNIMATRIX CORE             |
                    |     (Rust, deterministic,          |
                    |      full system privileges)       |
                    |                                    |
                    |  +-----------------------------+   |
                    |  | Knowledge Store (redb)      |   |
                    |  | Vector Index (hnsw_rs)      |   |
                    |  | Audit Log                   |   |
                    |  | Agent Registry              |   |
                    |  | Content Scanner             |   |
                    |  +-----------------------------+   |
                    |              |                      |
                    |     [Validated Interface]           |
                    |              |                      |
                    |  +-----------------------------+   |
                    |  | PROPOSAL VALIDATOR           |   |
                    |  | (Deterministic Rust code)    |   |
                    |  | - Schema validation          |   |
                    |  | - Business rule enforcement  |   |
                    |  | - Consistency checks         |   |
                    |  | - Human approval gating      |   |
                    |  +-----------------------------+   |
                    |              |                      |
                    +------|------|-----------------------+
                           |      |
                    [Structured JSON only]
                           |      |
                    +------v------v-----------+
                    |   LLM SANDBOX            |
                    |   (Separate process,     |
                    |    seccomp restricted,    |
                    |    no credentials,        |
                    |    no network,            |
                    |    no filesystem,         |
                    |    structured I/O only)   |
                    |                          |
                    |   Claude SDK instance    |
                    +--------------------------+
```

### 10.2 Specific Technical Recommendations

1. **Process isolation**: Run the Claude SDK in a subprocess using Anthropic's sandbox-runtime [28] or equivalent Linux bubblewrap/seccomp sandboxing. The subprocess must not inherit environment variables, file handles, or network access from the parent.

2. **Credential separation**: System credentials (GitHub tokens, signing keys, MCP server certs) must never enter the LLM's context window. A separate credential management component handles all authenticated operations. The LLM receives sanitized results, never raw authenticated responses.

3. **Structured I/O only**: The LLM communicates exclusively via typed JSON messages with schema validation. No free-form text output for operations that affect system state. Use constrained decoding where possible (as in FIDES [31]).

4. **Taint tracking**: Implement FIDES-style integrity labels on knowledge entries. Entries derived from LLM inference carry an "llm-derived" label that prevents them from being treated as ground truth without human confirmation.

5. **Rule of Two enforcement**: Ensure the LLM never simultaneously processes untrusted input, accesses sensitive data, and performs state changes. Design the API so that these three capabilities are structurally separated.

6. **Write-path controls**: All knowledge mutations proposed by the LLM must pass through the Proposal Validator. The validator enforces:
   - Schema validity (correct fields, valid categories)
   - Content scanning (existing ~35 regex patterns)
   - Consistency with existing knowledge (embedding-distance anomaly detection)
   - Rate limiting (prevent mass poisoning)
   - Human approval for Red Zone operations

7. **Tiered trust labeling**: All Unimatrix responses must indicate their trust tier:
   - `trust: "deterministic"` -- No LLM involvement
   - `trust: "llm-assisted"` -- LLM involved but deterministically validated
   - `trust: "llm-derived"` -- LLM-generated content, advisory only

8. **Audit continuity**: The existing audit log must capture all LLM interactions, including the full prompt sent to the LLM and the full response received. This enables post-incident analysis and pattern detection.

9. **Rollback capability**: Leverage existing content hashing and hash chains (from nxs-004) to enable point-in-time recovery. If LLM-derived entries are later found to be compromised, the system can identify and revert all entries in the tainted chain.

10. **No autonomous learning loop**: The LLM's outputs should NOT automatically feed back into the knowledge store without human review. This prevents the hallucination feedback loop where errors compound over time.

### 10.3 What NOT to Do

1. **Do not give the LLM direct database access.** The LLM must never hold a reference to `Store`, `Arc<Store>`, or any database handle. All data access goes through the validated interface.

2. **Do not pass system credentials through the LLM's context.** The LLM should not see GitHub tokens, API keys, signing keys, or agent registry data. These are managed by separate, non-LLM components.

3. **Do not trust LLM-generated categories, confidence scores, or conflict resolutions without validation.** The LLM is a proposer, not a decider.

4. **Do not allow the LLM to modify the agent registry or trust levels.** These are system-level operations that require human approval regardless of who proposes them.

5. **Do not assume the sandbox is sufficient.** Defense in depth requires multiple independent layers. Sandboxing is necessary but not sufficient. Combine with validation, taint tracking, audit, and human approval.

6. **Do not embed the LLM to reduce complexity.** Embedding an LLM increases architectural complexity. If the goal is simpler architecture, embedding is counterproductive. Embed only if the functional benefits (richer context assembly, natural language understanding, proactive knowledge management) justify the security cost.

---

## 11. References

[1] Berryville Institute of Machine Learning. "Secure the Weakest Link [Principle 1]." 2019. https://berryvilleiml.com/2019/07/25/secure-the-weakest-link-principle-1/

[2] Datta, A. et al. "Compositional Security." Carnegie Mellon University. https://www.andrew.cmu.edu/user/danupam/compositional-security.html

[3] Garg, D. et al. "Compositional System Security with Interface-Confined Adversaries." MFPS 2010. https://people.mpi-sws.org/~dg/papers/mfps-10-final.pdf

[4] Schneider, F. et al. "Secure composition of insecure components." ResearchGate. https://www.researchgate.net/publication/3809718_Secure_composition_of_insecure_components

[5] NIST. "SA-8(9): Trusted Components." SP 800-53 Rev. 5. https://csf.tools/reference/nist-sp-800-53/r5/sa/sa-8/sa-8-9/

[6] Hardy, N. "The Confused Deputy." ACM SIGOPS Operating Systems Review, 1988.

[7] NCSC UK. "Prompt injection is not SQL injection (it may be worse)." December 2025. https://www.ncsc.gov.uk/blog-post/prompt-injection-is-not-sql-injection

[8] Malwarebytes. "Prompt injection is a problem that may never be fixed, warns NCSC." December 2025. https://www.malwarebytes.com/blog/news/2025/12/prompt-injection-is-a-problem-that-may-never-be-fixed-warns-ncsc

[9] Parminder Singh. "Managing the Agentic Blast Radius in Multi-Agent Systems (OWASP 2026)." Medium, January 2026.

[10] Cloud Security Alliance. "Agentic AI Threat Modeling Framework: MAESTRO." February 2025. https://cloudsecurityalliance.org/blog/2025/02/06/agentic-ai-threat-modeling-framework-maestro

[11] Zou, W. et al. "PoisonedRAG: Knowledge Corruption Attacks to Retrieval-Augmented Generation." USENIX Security 2025. https://www.usenix.org/system/files/usenixsecurity25-zou-poisonedrag.pdf

[12] Springer. "Use of Asymmetric Byzantine Quorums to Overcome Trust Issues in Blockchain Oracles." https://link.springer.com/article/10.1007/s44227-024-00054-9

[13] De Vadoss, J. "A Byzantine Fault Tolerance Approach towards AI Safety." arXiv:2504.14668, April 2025. https://www.arxiv.org/pdf/2504.14668

[14] Chromium. "Multi-process Architecture." https://www.chromium.org/developers/design-documents/multi-process-architecture/ ; "Site Isolation Design Document." https://www.chromium.org/developers/design-documents/site-isolation/

[15] Snowflake. "Security Practices for UDFs and Procedures." https://docs.snowflake.com/en/developer-guide/udf-stored-procedure-security-practices

[16] Microsoft. "Restrict UDF Code Access Security Permissions." https://learn.microsoft.com/en-us/sharepoint/dev/general-development/how-to-restrict-udf-code-access-security-permissions

[17] Frontiers. "Review of Automated Vulnerability Analysis of Smart Contracts on Ethereum." https://www.frontiersin.org/journals/blockchain/articles/10.3389/fbloc.2022.814977/full

[18] Google Cloud. "GKE Sandbox." https://docs.google.com/kubernetes-engine/docs/concepts/sandbox-pods ; gVisor. "Introduction to gVisor security." https://gvisor.dev/docs/architecture_guide/intro/

[19] MDPI. "Prompt Injection Attacks in Large Language Models and AI Agent Systems: A Comprehensive Review." Information, 17(1), 54. https://www.mdpi.com/2078-2489/17/1/54

[20] arXiv. "LLM-based Agents Suffer from Hallucinations: A Survey." arXiv:2509.18970. https://arxiv.org/html/2509.18970v1

[21] arXiv. "A Concise Review of Hallucinations in LLMs and their Mitigation." arXiv:2512.02527. https://arxiv.org/html/2512.02527v1

[22] Portkey. "LLM hallucinations in production." https://portkey.ai/blog/llm-hallucinations-in-production/

[23] Glama. "Mitigating Agentic Data Exfiltration in MCP Architectures with Context-Aware Firewalls." November 2025. https://glama.ai/blog/2025-11-11-the-lethal-trifecta-securing-model-context-protocol-against-data-flow-attacks

[24] Hubinger, E. et al. "Risks from Learned Optimization in Advanced Machine Learning Systems." arXiv:1906.01820, 2019.

[25] Alignment Forum. "Inducing Unprompted Misalignment in LLMs." https://www.alignmentforum.org/posts/ukTLGe5CQq9w8FMne/inducing-unprompted-misalignment-in-llms

[26] Nature. "Training large language models on narrow tasks can lead to broad misalignment." 2025. https://www.nature.com/articles/s41586-025-09937-5

[27] SecGPT GitHub. "An Execution Isolation Architecture for LLM-Based Agentic Systems." https://github.com/llm-platform-security/SecGPT

[28] Anthropic. "Claude Code Sandboxing." https://www.anthropic.com/engineering/claude-code-sandboxing ; GitHub. "sandbox-runtime." https://github.com/anthropic-experimental/sandbox-runtime

[29] Debenedetti, E. et al. "Defeating Prompt Injections by Design." arXiv:2503.18813, March 2025. https://arxiv.org/abs/2503.18813

[30] WinBuzzer. "How Google DeepMind's CaMeL Architecture Aims to Block LLM Prompt Injections." April 2025. https://winbuzzer.com/2025/04/27/how-google-deepminds-camel-architecture-aims-to-block-llm-prompt-injections-xcxwbn/

[31] Costa, M. & Kopf, B. "Securing AI Agents with Information-Flow Control." arXiv:2505.23643, May 2025. https://arxiv.org/abs/2505.23643 ; GitHub. "microsoft/fides." https://github.com/microsoft/fides

[32] Meta AI. "Agents Rule of Two: A Practical Approach to AI Agent Security." October 2025. https://ai.meta.com/blog/practical-ai-agent-security/

[33] Ken Huang. "The 'Rule of Two' vs. Reality: Why Meta's Agent Safeguards Don't Cover All Core Agentic AI Risks." https://kenhuangus.substack.com/p/the-rule-of-two-vs-reality-why-metas

[34] Wu, Y. et al. "IsolateGPT: An Execution Isolation Architecture for LLM-Based Agentic Systems." NDSS 2025. https://arxiv.org/abs/2403.04960

[35] BSI & ANSSI. "Design Principles for LLM-based Systems with Zero Trust." 2025. https://www.bsi.bund.de/SharedDocs/Downloads/EN/BSI/Publications/ANSSI-BSI-joint-releases/LLM-based_Systems_Zero_Trust.pdf

[36] GitHub Blog. "Security alert: Attack campaign involving stolen OAuth user tokens issued to two third-party integrators." April 2022. https://github.blog/news-insights/company-news/security-alert-stolen-oauth-user-tokens/

[37] arXiv. "MiniScope: A Least Privilege Framework for Authorizing Tool Calling Agents." arXiv:2512.11147. https://arxiv.org/pdf/2512.11147

[38] Model Context Protocol. "Security Best Practices." https://modelcontextprotocol.io/specification/draft/basic/security_best_practices ; Red Hat. "Model Context Protocol (MCP): Understanding security risks and controls." https://www.redhat.com/en/blog/model-context-protocol-mcp-understanding-security-risks-and-controls

[39] OWASP. "Top 10 for LLM Applications 2025." https://genai.owasp.org/resource/owasp-top-10-for-llm-applications-2025/

[40] Simon Willison. "CaMeL offers a promising new direction for mitigating prompt injection attacks." April 2025. https://simonwillison.net/2025/Apr/11/camel/

[41] arXiv. "Modelling Trust and Trusted Systems: A Category Theoretic Approach." arXiv:2602.11376. https://arxiv.org/html/2602.11376

[42] arXiv. "Securing Agentic AI: A Comprehensive Threat Model and Mitigation Framework." arXiv:2504.19956. https://arxiv.org/pdf/2504.19956

[43] Northflank. "How to sandbox AI agents in 2026: MicroVMs, gVisor & isolation strategies." https://northflank.com/blog/how-to-sandbox-ai-agents

[44] ScienceDirect. "From prompt injections to protocol exploits: Threats in LLM-powered AI agents workflows." 2025. https://www.sciencedirect.com/science/article/pii/S2405959525001997

[45] PMC. "Quantifying the uncertainty of LLM hallucination spreading in complex adaptive social networks." https://pmc.ncbi.nlm.nih.gov/articles/PMC11252443/

[46] CSO Online. "Implementing zero trust in AI and LLM architectures." https://www.csoonline.com/article/3604270/implementing-zero-trust-in-ai-and-llm-architectures-a-practitioners-guide-to-secure-and-responsible-ai-systems.html
