# Research: Data Poisoning, Knowledge Integrity, and Content Trust in AI Knowledge Systems

**Date**: 2026-02-23
**Context**: Unimatrix -- a self-learning context engine where AI agents store and retrieve knowledge entries (conventions, decisions, patterns, lessons learned) across feature development cycles.
**Relevance**: Unimatrix accumulates knowledge over time in a local redb store (nxs-001) with hnsw_rs vector search (nxs-002) and local ONNX embeddings (nxs-003). Agents read from and write to this shared knowledge base. This research surveys the threat landscape and defense techniques relevant to that architecture.

---

## 1. Data Poisoning in Knowledge Bases

### 1.1 The Threat

Data poisoning against AI knowledge bases has moved from theoretical to demonstrated in 2024-2025. The core risk: a malicious or compromised agent injects false knowledge entries that corrupt future agent behavior.

**PoisonedRAG** (USENIX Security 2025) is the landmark paper. Zou et al. demonstrated that injecting as few as 5 malicious texts into a knowledge database containing millions of entries achieves a 90% attack success rate for making an LLM produce attacker-chosen answers to targeted questions. The attack formulates poisoning as an optimization problem with two conditions: a *retrieval condition* (the malicious text must be retrieved for the target question) and a *generation condition* (the malicious text must mislead the LLM into producing the target answer). Evaluated defenses were found insufficient.

- Source: [PoisonedRAG, USENIX Security 2025](https://www.usenix.org/conference/usenixsecurity25/presentation/zou-poisonedrag)
- Status: **Demonstrated**. Code available on GitHub. Tested against multiple LLMs and RAG configurations.

**ADMIT** (arXiv, October 2025) extends this to fact-checking systems specifically. It achieves 86% attack success rate at a poisoning rate of 0.93 x 10^-6 -- meaning a single adversarial passage in a million-entry database is enough. ADMIT remains effective even when strong counter-evidence exists in the database.

- Source: [ADMIT: Few-shot Knowledge Poisoning Attacks on RAG-based Fact Checking](https://arxiv.org/abs/2510.13842)
- Status: **Demonstrated**. Attacker needs no access to the retriever or LLM, only the ability to write to the knowledge base.

**MemoryGraft** (arXiv, December 2025) targets agent *experience memory* specifically. Unlike PoisonedRAG which targets factual knowledge, MemoryGraft exploits agents' tendency to replicate patterns from retrieved successful tasks. An attacker implants malicious "successful experience" templates into the agent's long-term memory. When the agent encounters semantically similar tasks later, it retrieves and adopts the embedded unsafe patterns. Validated on MetaGPT's DataInterpreter with GPT-4o.

- Source: [MemoryGraft: Persistent Compromise of LLM Agents via Poisoned Experience Retrieval](https://arxiv.org/abs/2512.16962)
- Status: **Demonstrated**. The compromise remains active until the memory store is explicitly purged.

### 1.2 Relevance to Unimatrix

Unimatrix's architecture is a textbook target for these attacks:
- Agents write entries via `Store::insert()` with title, content, category, and tags.
- Those entries are embedded (nxs-003) and indexed in hnsw_rs (nxs-002).
- Future agents retrieve entries via semantic similarity search.
- A single poisoned entry -- e.g., a "convention" stating "always use eval() for config parsing" -- would be retrieved by agents working on config-related tasks.

The key difference from cloud RAG systems: Unimatrix is local. The attack surface is limited to agents that have write access to the store, not the open internet. This narrows the threat model considerably but does not eliminate it -- a compromised agent, a poisoned MCP tool response, or a malicious agent profile could all inject entries.

### 1.3 Foundational vs. Layered

**Foundational (build now)**:
- Entry-level metadata: author/agent ID, timestamp, source context (which feature cycle, which agent invocation).
- Append-only write log (already partially present in redb transaction model).

**Layered (add later)**:
- Content validation rules (e.g., reject entries containing known dangerous patterns).
- Cross-entry consistency checks (new entries that contradict established conventions flagged for review).
- Confidence scoring based on corroboration (multiple independent agents storing similar knowledge increases trust).

---

## 2. Content Integrity Verification

### 2.1 Techniques

**Content hashing** is the baseline. Each entry's content is hashed (SHA-256) at write time; the hash is stored alongside the entry. Any modification to the content invalidates the hash. This detects tampering but not initial poisoning -- a poisoned entry has a valid hash from the start.

**Provenance chains** extend hashing by recording the full history of an entry: who created it, which agent, during which feature cycle, what evidence supported it. The **Knowledge Provenance Protocol (KPP)** uses Merkle proofs to periodically anchor off-chain DAG state to an on-chain settlement layer for immutability.

- Source: [Knowledge Provenance Protocol (KPP)](https://www.sec.gov/files/ctf-written-input-knowledge-provenance-protocol-kpp-revolutionary-framework-decentralized-science.pdf)
- Status: Framework specification; blockchain-based and over-engineered for a local knowledge store, but the DAG-of-provenance concept is directly applicable.

**Merkle trees for knowledge stores**: An append-only knowledge store maps naturally to a Merkle tree where each leaf hashes an entry, parents combine hashes, the root verifies the entire store state, changes propagate up, and an invalid root signals tampering. This gives O(log n) verification of any single entry's inclusion in a known-good state.

- Source: [arXiv:2506.13246, On Immutable Memory Systems](https://arxiv.org/pdf/2506.13246)
- Status: **Theoretical** for AI knowledge systems specifically; well-established in blockchain and git.

**C2PA Content Credentials** (v2.3, 2025) provide an industry standard for content provenance. Each asset gets a cryptographically signed manifest containing assertions about origin, modifications, and AI involvement. Uses SHA-256 hashes, X.509 certificates, and digital signatures. Any tampering invalidates the cryptographic hash and signature.

- Source: [C2PA Technical Specification 2.3](https://spec.c2pa.org/specifications/specifications/2.3/specs/C2PA_Specification.html)
- Status: **Production standard**. Adobe, Microsoft, Intel consortium. Designed for media, but the manifest+signature model applies to knowledge entries.

**MAIF (Multi-Agent Integrity Framework)** uses cryptographic hash chains and digital signatures to create an immutable audit trail for agent actions. Each agent action is signed with a unique digital identity, providing non-repudiable proof.

- Source: [MAIF: Enforcing AI Trust and Provenance](https://arxiv.org/pdf/2511.15097)
- Status: **Research prototype** (November 2025). Directly relevant to multi-agent knowledge systems.

### 2.2 Relevance to Unimatrix

Unimatrix entries in redb are mutable (updates change content, indexes are rewritten). The current design has no content hashing or provenance tracking. An entry modified by a compromised agent is indistinguishable from a legitimate update.

### 2.3 Foundational vs. Layered

**Foundational (build now)**:
- SHA-256 content hash stored per entry in `EntryRecord`. Computed at write time, verified on read.
- Agent identity stored per entry (which agent wrote/updated it).
- Timestamp chain: created_at, updated_at, with no ability to backdate.

**Layered (add later)**:
- Merkle root over the full entry set, recomputed on each write. Enables point-in-time integrity verification.
- Entry signing with per-agent keys (requires agent identity infrastructure).
- Provenance DAG linking entries to their source evidence.

---

## 3. Semantic Poisoning

### 3.1 The Threat

Semantic poisoning is the most insidious variant: entries that are syntactically valid, pass basic validation, and appear to be legitimate knowledge, but contain subtly misleading content. Examples in the Unimatrix context:

- A "convention" entry: "For all database operations, disable foreign key constraints for performance" -- technically valid advice in some contexts, catastrophically wrong as a general rule.
- A "decision" entry: "Authentication tokens should be stored in localStorage for cross-tab access" -- common pattern, known vulnerability.
- A "pattern" entry: "Use `*.` glob patterns in CORS configuration for development convenience" -- opens the application to cross-origin attacks.

**Knowledge Graph Poisoning** (ScienceDirect, 2025) demonstrates this in structured knowledge: perturbation triples are inserted into knowledge graphs to create misleading inference chains. At least one adversarial triple is retrieved in over 90% of queries under attack. The injected triples are semantically plausible -- they *look right* but *lead wrong*.

- Source: [Exploring knowledge poisoning attacks to retrieval-augmented generation](https://www.sciencedirect.com/science/article/abs/pii/S1566253525009625)
- Status: **Demonstrated** on KG-RAG systems.

**Nature Medicine study** (2024) found that replacing just 0.001% of training tokens with misinformation caused models to generate 7-11% more harmful completions. The threshold for effective poisoning is astonishingly low.

- Source: [Medical large language models are vulnerable to data-poisoning attacks](https://www.nature.com/articles/s41591-024-03445-1)
- Status: **Demonstrated** in medical domain.

### 3.2 Relevance to Unimatrix

This is the highest-risk threat for Unimatrix specifically. The system stores *conventions* and *decisions* that influence how agents write code in future feature cycles. A subtly wrong convention ("always set `secure: false` on cookies during development" stored without the "during development" qualifier, or with it but applied broadly) persists across feature cycles and propagates through generated code.

Detection is extremely difficult because:
1. The entries are well-formed text, not malformed inputs.
2. They may be partially correct (mixing valid and dangerous advice).
3. They match the expected schema (title, content, category, tags).
4. Their embeddings place them correctly in semantic space (near related legitimate entries).

### 3.3 Foundational vs. Layered

**Foundational (build now)**:
- Entry categorization and source tracking (which feature cycle generated this entry).
- Required fields that force explicit scope (e.g., a convention must specify what projects/contexts it applies to).

**Layered (add later)**:
- Cross-referencing new entries against established entries for contradictions.
- Human review gates for entries in high-trust categories (security conventions, deployment patterns).
- Entry confidence scoring that decays without corroboration.

---

## 4. Trust-on-First-Use (TOFU) in Knowledge Systems

### 4.1 The Model

TOFU in traditional systems (SSH, Signal): the first time you connect to a server or contact, you accept its identity key. Future connections verify against that stored key. If the key changes, you get a warning.

Applied to knowledge entries: the first agent to store a convention about "JWT validation" establishes the trusted pattern. Future entries on the same topic are compared against this baseline. Changes trigger review.

### 4.2 The Risk

If the *first* agent to store a pattern was compromised, TOFU enshrines the poisoned knowledge as the trusted baseline. All future corrections appear as anomalies. This is the TOFU bootstrap problem.

Google's experience with AI agent trust (2025) showed that "the conversation around AI fundamentally shifted from capability to trust in 2025" with trust defined by sovereignty (control over data, infrastructure, and models) and compliance.

- Source: [Lessons from 2025 on agents and trust, Google Cloud Blog](https://cloud.google.com/transform/ai-grew-up-and-got-a-job-lessons-from-2025-on-agents-and-trust)
- Status: Industry trend analysis, not a specific attack demonstration.

### 4.3 Relevance to Unimatrix

Unimatrix is inherently a TOFU system today. The first feature cycle (nxs-001, nxs-002) established conventions that all subsequent features follow. If those early entries were wrong, the error compounds.

Mitigations:
- **Multi-source corroboration**: A convention gains trust not from being first, but from being independently confirmed by multiple agents in different feature cycles.
- **Human approval gates**: Critical entries (security, architecture decisions) require human confirmation before becoming trusted baselines.
- **Decay and re-evaluation**: Entries that haven't been accessed or corroborated recently lose trust score.

### 4.4 Foundational vs. Layered

**Foundational (build now)**:
- Entry metadata includes `trust_source` (agent-generated vs. human-approved).
- Distinguish between "observed" (agent stored this) and "ratified" (human confirmed this).

**Layered (add later)**:
- Corroboration counting: how many independent agents have stored/referenced similar knowledge.
- Trust score computation based on source, age, corroboration, and access patterns.
- Conflict detection when a new entry contradicts an established one.

---

## 5. Anomaly Detection for Knowledge Store Compromise

### 5.1 Known Indicators

**OWASP Top 10 for Agentic Applications** (December 2025, designated as the 2026 edition) classifies memory and context poisoning as **ASI06**. The description: "persistent corruption of agent memory, RAG stores, or contextual knowledge." A single successful injection poisons the agent's memory permanently; every future session inherits the compromise.

- Source: [OWASP Top 10 for Agentic Applications](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/)
- Status: **Official classification**. Real-world examples include the Google Gemini Memory Attack (February 2025) and Gemini Calendar Invite Poisoning.

**MITRE ATLAS** (October 2025 update) added 14 new attack techniques for AI agents, including:
- *AI Agent Context Poisoning*: Manipulating the context used by an agent's LLM to persistently influence responses.
- *Memory Manipulation*: Altering long-term memory to ensure malicious changes persist across sessions.
- *Modify AI Agent Configuration*: Changing config files to create persistent malicious behavior.

- Source: [MITRE ATLAS Framework](https://atlas.mitre.org/) and [Zenity & MITRE ATLAS collaboration](https://zenity.io/blog/current-events/zenity-labs-and-mitre-atlas-collaborate-to-advances-ai-agent-security-with-the-first-release-of)
- Status: **Formal framework**. 15 tactics, 66 techniques, 33 real-world case studies.

**Microsoft AI Recommendation Poisoning** (February 2026) documented 50+ unique prompts from 31 companies across 14 industries embedding hidden instructions in "Summarize with AI" buttons. One-click attack vector that injects persistence commands into AI assistant memory via URL prompt parameters. Stored entries affect responses in later, unrelated conversations.

- Source: [Microsoft Security Blog: Manipulating AI memory for profit](https://www.microsoft.com/en-us/security/blog/2026/02/10/ai-recommendation-poisoning/)
- Status: **Demonstrated in the wild**. Not theoretical -- actively exploited commercially.

### 5.2 Write Pattern Anomalies

Indicators of compromise in a knowledge store:

1. **Burst writes**: An agent suddenly stores many entries in a short period (normal pattern: a few entries per feature cycle).
2. **Category flooding**: Many entries in a single high-value category (e.g., "security-convention") from one agent.
3. **Contradiction injection**: New entries that directly contradict established entries without referencing them.
4. **Scope creep**: Entries that claim broad applicability ("all projects", "always") without qualification.
5. **Update storms**: Existing entries being updated to change their meaning while preserving their title/category.
6. **Orphan entries**: Entries with no connection to any active feature cycle or GitHub issue.

### 5.3 Relevance to Unimatrix

Unimatrix's redb store tracks timestamps and could track agent IDs. The COUNTERS table (nxs-001) tracks total entries and per-status counts. These provide baseline metrics for anomaly detection. The challenge: with the current small scale (hundreds of entries across feature cycles), statistical anomaly detection is noisy. Rule-based checks are more appropriate initially.

### 5.4 Foundational vs. Layered

**Foundational (build now)**:
- Per-entry metadata: agent_id, feature_cycle, timestamp.
- COUNTERS table already exists (nxs-001). Extend to track writes-per-agent, writes-per-category.

**Layered (add later)**:
- Write rate limiting per agent.
- Contradiction detection (semantic similarity between new entry and existing entries in same category, checked for semantic opposition).
- Alert on entries that reference no feature cycle or tracking issue.
- Temporal analysis: entries created outside of active development sessions.

---

## 6. Embedding Space Attacks

### 6.1 The Threat

Adversarial inputs designed to manipulate vector similarity search are a distinct class of attack. Rather than poisoning the knowledge content, they poison its *discoverability*.

**Semantic collision attacks**: An attacker crafts a document whose embedding is deceptively close to a target query's embedding, even if the surface-level text is unrelated. Example: a document about "team-building activities" optimized to be the closest match for a query about "administrator passwords."

- Source: [Embedding Space Manipulation](https://aiq.hu/en/30-1-4-embedding-space-manipulation/) and [OWASP LLM08:2025](https://genai.owasp.org/llmrisk/llm082025-vector-and-embedding-weaknesses/)
- Status: **Demonstrated**. OWASP LLM08:2025 specifically addresses this.

**Vec2vec translation attacks** (May 2025): Cross-embedding-space interpretation enables correlating data across systems. An attacker with vec2vec access could translate embeddings from one system into another's space, breaking the assumption that embeddings are opaque.

- Source: [Vec2vec attacks](https://www.howweknowus.com/2025/05/23/vec2vec-attacks/)
- Status: **Demonstrated** in research. Primarily a privacy concern, but also enables targeted poisoning.

**Embedding inversion**: Vector embeddings can be partly transformed back into source data. Sensitive data (personal information, code snippets) can potentially be extracted from stored embeddings.

- Source: [AI Vector & Embedding Security Risks, Mend.io](https://www.mend.io/blog/vector-and-embedding-weaknesses-in-ai-systems/)
- Status: **Demonstrated**. OWASP classifies this under LLM08:2025.

**Adversarial query triggers**: Attackers craft queries that include calculated perturbations -- seemingly innocuous strings designed to steer the query's embedding toward specific malicious documents already in the database.

- Source: [The Embedded Threat in Your LLM: Poisoning RAG Pipelines via Vector Embeddings](https://prompt.security/blog/the-embedded-threat-in-your-llm-poisoning-rag-pipelines-via-vector-embeddings)
- Status: **Demonstrated** in RAG pipeline attacks.

### 6.2 Relevance to Unimatrix

Unimatrix uses hnsw_rs with DistDot (dot product on L2-normalized vectors, equivalent to cosine similarity). The embedding model (all-MiniLM-L6-v2, 384-d) runs locally via ONNX Runtime. Attack vectors:

1. **Relevance hijacking**: A poisoned entry is crafted so its embedding is maximally similar to high-value queries ("authentication pattern", "security convention") regardless of its actual content.
2. **Relevance suppression**: An attacker stores many entries whose embeddings crowd out legitimate entries in the same semantic neighborhood, effectively burying correct knowledge.
3. **Embedding inversion**: Less relevant for Unimatrix since the stored knowledge is not secret -- it's development conventions. But if entries contained credentials or secrets (a misuse of the store), embeddings could leak them.

The local embedding model is a significant defense: attackers cannot observe the exact embedding function unless they have access to the same ONNX model. But since Unimatrix uses a public model (all-MiniLM-L6-v2), any attacker can reproduce the embedding function locally and optimize adversarial inputs.

### 6.3 Foundational vs. Layered

**Foundational (build now)**:
- Embedding dimension validation already exists in nxs-002 (rejects non-384-d vectors, NaN, infinity).
- Content stored alongside embeddings in redb -- the embedding is not the source of truth, the text is.

**Layered (add later)**:
- Embedding consistency checks: verify that an entry's embedding is semantically consistent with its text content (re-embed and compare).
- Neighborhood analysis: flag entries whose nearest neighbors are semantically unrelated to their content.
- Retrieval result diversity enforcement: don't return all results from a single author/agent/feature cycle.

---

## 7. Supply Chain Security for AI Knowledge

### 7.1 The Threat

Knowledge accumulation creates a supply chain: entries from early feature cycles influence decisions in later feature cycles, which generate new entries that reference earlier ones. Poisoning early in the chain propagates forward.

The **Virus Infection Attack (VIA)** (September 2025) demonstrated how poisoned content propagates through synthetic data pipelines: once baked into datasets, the poison spreads across generations, amplifying impact over time.

- Source: [LLM Data Poisoning: Training AI to Betray You, Medium](https://medium.com/@instatunnel/llm-data-poisoning-training-ai-to-betray-you-1e0872edb7bd)
- Status: **Demonstrated** in synthetic data pipelines. Directly analogous to knowledge propagation.

**Scale of the problem**: The AI model supply chain saw $12 billion in losses from compromised models in 2025. In March 2025, researchers found that 23% of the top 1,000 most-downloaded models on Hugging Face had been compromised at some point. Approximately 25% of organizations reported being victims of AI data poisoning in 2025.

- Source: [AI Model Poisoning: The $12B Supply Chain Crisis](https://docs.cybersecfeed.com/blog/ai-model-poisoning-supply-chain-crisis)
- Status: **Demonstrated at scale**. Industry-wide problem.

**Lakera's 2025 GenAI Security Readiness Report** found that only 14% of organizations with agents in production have runtime guardrails in place. Defenses must treat all external influences -- including the agent's own memory -- as untrusted input.

- Source: [Lakera Agentic AI Threats](https://www.lakera.ai/blog/agentic-ai-threats-p1) and [Lakera GenAI Security Report 2025](https://www.lakera.ai/genai-security-report-2025)
- Status: **Industry survey data**.

### 7.2 Relevance to Unimatrix

Unimatrix is explicitly designed as a cumulative knowledge system. This is its value proposition and its greatest risk surface. The propagation chain:

```
nxs-001 conventions --> nxs-002 design decisions --> nxs-003 patterns
     |                       |                            |
     v                       v                            v
  vnc-001 tool behavior  col-001 orchestration      crt-001 learning rules
```

A poisoned convention from nxs-001 ("always use bincode v1 native Encode") would have cascaded through every subsequent feature. In practice, the ALIGNMENT-REPORT mechanism caught this risk (W1 in nxs-001), but that was a human-reviewed document, not an automated defense.

### 7.3 Foundational vs. Layered

**Foundational (build now)**:
- Entry lineage: track which feature cycle and which prior entries influenced a new entry.
- Feature cycle boundaries: entries are tagged with their source feature cycle, enabling isolation.

**Layered (add later)**:
- Dependency graph: which entries cite which other entries. Enables impact analysis when an entry is flagged.
- Propagation analysis: if entry X is found to be poisoned, identify all entries that were created while X was in the active knowledge set.
- Feature cycle quarantine: ability to isolate all entries from a specific feature cycle if that cycle is suspected of compromise.

---

## 8. Rollback and Recovery

### 8.1 Strategies

**Append-only architecture**: Every modification creates a new version linked to its predecessor. Nothing is overwritten. This is the gold standard for recoverability. Git uses this model. redb's transaction model provides atomic writes but not append-only versioning by default.

**Immutable audit trails with Merkle trees**: Each entry hashes to a leaf; parents combine hashes; the root verifies the entire trail. Changes propagate up; invalid roots signal tampering. Hash chaining is simple (linear verification cost). Merkle trees enable O(log n) batch validation.

- Source: [Immutable Audit Trails: The Missing Piece in AI Accountability](https://quantumencoding.io/blog/immutable-audit-trails)
- Status: **Established technique**. Well-proven in blockchain and version control systems.

**Trusted snapshots**: Periodically create a verified snapshot of the knowledge store (Merkle root + metadata). If poisoning is detected, roll back to the last trusted snapshot. Entries added after the snapshot are quarantined for review.

**Entry quarantine**: Flag suspicious entries as quarantined rather than deleting them. Quarantined entries are excluded from retrieval but preserved for forensic analysis. This allows investigation without data loss.

**Lakera's five-layer defense architecture** for agent memory:
1. Memory partitioning (isolate memory by trust level)
2. Context isolation (prevent cross-contamination between sessions)
3. Provenance tracking (tag every memory item with source and timestamp)
4. Temporal decay (reduce trust of old unverified entries)
5. Behavioral monitoring (detect drift in agent behavior patterns)

- Source: [MintMCP: AI agent memory poisoning](https://www.mintmcp.com/blog/ai-agent-memory-poisoning)
- Status: **Architectural recommendation**. Not a specific implementation.

### 8.2 Relevance to Unimatrix

Unimatrix's redb store supports atomic transactions but not entry versioning. An `update_entry()` call overwrites the previous content. There is no way to:
- See what an entry contained before an update.
- Roll back a specific entry to a previous version.
- Identify when an entry's meaning changed.

The `StatusIndex` (nxs-001) tracks entry lifecycle (Active/Deprecated/Archived) but not content history.

### 8.3 Foundational vs. Layered

**Foundational (build now)**:
- Content hash per entry (SHA-256 of title + content at write time).
- Entry version counter (incremented on each update).
- Previous content hash stored on update (enables detecting that content changed, even if the old content is not preserved).

**Layered (add later)**:
- Full append-only version history: store previous versions in a VERSION_HISTORY table.
- Merkle root computation over all entry hashes, stored in COUNTERS or a dedicated INTEGRITY table.
- Trusted snapshot creation: checkpoint command that records the Merkle root + entry count + timestamp.
- Quarantine status: extend StatusIndex with a Quarantined state that excludes entries from retrieval.
- Rollback to snapshot: restore the entry set to a previous checkpoint, quarantining all entries added after it.

---

## Summary: What to Build When

### Foundational Support (nxs-004 or as extensions to existing crates)

These are low-cost additions to the existing schema that enable future integrity features without requiring them immediately:

| Capability | Where | Cost | Enables |
|-----------|-------|------|---------|
| SHA-256 content hash per entry | `EntryRecord` field in nxs-001 | Low | Tamper detection, Merkle tree later |
| Agent identity per entry | `EntryRecord` field | Low | Provenance, anomaly detection |
| Feature cycle tag per entry | `EntryRecord` field | Low | Lineage tracking, quarantine |
| Entry version counter | `EntryRecord` field | Low | Change tracking, rollback |
| Previous content hash on update | Write path in nxs-001 | Low | Detecting content changes |
| Trust source field | `EntryRecord` field | Low | TOFU mitigation, human/agent distinction |
| Writes-per-agent counter | COUNTERS table | Low | Anomaly detection baseline |

### Layered Defenses (future features)

These require the foundational fields above but can be added incrementally:

| Layer | Depends On | Complexity | Threat Addressed |
|-------|-----------|------------|-----------------|
| Entry signing (per-agent keys) | Agent identity | Medium | Content integrity |
| Merkle root computation | Content hashes | Medium | Store-wide integrity |
| Trusted snapshots | Merkle root | Medium | Rollback capability |
| Version history table | Version counter | Medium | Full rollback |
| Quarantine status | StatusIndex | Low | Isolation of suspected entries |
| Contradiction detection | Embeddings + content | High | Semantic poisoning |
| Embedding consistency check | Embeddings + content | Medium | Relevance hijacking |
| Corroboration scoring | Agent identity + lineage | High | TOFU mitigation |
| Write rate limiting | Writes-per-agent counter | Low | Burst attack prevention |
| Propagation analysis | Feature cycle tags + lineage | High | Supply chain analysis |

### Key Takeaway

The threat landscape for AI knowledge systems is mature and actively exploited as of early 2026. The attacks are not theoretical -- PoisonedRAG, ADMIT, MemoryGraft, and the Microsoft AI Recommendation Poisoning campaign are all demonstrated. OWASP (ASI06) and MITRE ATLAS have formalized the threat categories.

For Unimatrix, the highest-priority risk is **semantic poisoning of conventions and patterns** that propagate across feature cycles. The local-only architecture significantly reduces the attack surface (no public-facing RAG endpoint, no web scraping), but the multi-agent write model means any compromised agent can inject entries.

The defense strategy should be: build foundational metadata fields now (hashes, agent IDs, version counters, trust sources) that are cheap to add and enable all future integrity features. Layer the active defenses (contradiction detection, quarantine, rollback) as the system matures and the threat model becomes more concrete.

---

## Sources

### Academic Papers
- [PoisonedRAG: Knowledge Corruption Attacks to RAG, USENIX Security 2025](https://www.usenix.org/conference/usenixsecurity25/presentation/zou-poisonedrag)
- [ADMIT: Few-shot Knowledge Poisoning Attacks on RAG-based Fact Checking, arXiv Oct 2025](https://arxiv.org/abs/2510.13842)
- [MemoryGraft: Persistent Compromise of LLM Agents via Poisoned Experience Retrieval, arXiv Dec 2025](https://arxiv.org/abs/2512.16962)
- [Medical LLMs vulnerable to data-poisoning attacks, Nature Medicine 2024](https://www.nature.com/articles/s41591-024-03445-1)
- [Knowledge poisoning attacks to retrieval-augmented generation, ScienceDirect 2025](https://www.sciencedirect.com/science/article/abs/pii/S1566253525009625)
- [RAG Safety: Exploring Knowledge Poisoning Attacks, arXiv Jul 2025](https://arxiv.org/abs/2507.08862)
- [Defending Against Knowledge Poisoning Attacks During RAG, arXiv Aug 2025](https://arxiv.org/abs/2508.02835)
- [On Immutable Memory Systems, arXiv Jun 2025](https://arxiv.org/pdf/2506.13246)
- [MAIF: Enforcing AI Trust and Provenance, arXiv Nov 2025](https://arxiv.org/pdf/2511.15097)
- [Agentic AI Security: Threats, Defenses, Evaluation, and Open Challenges, arXiv Oct 2025](https://arxiv.org/html/2510.23883v1)

### Industry Standards and Frameworks
- [OWASP Top 10 for Agentic Applications 2026](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/)
- [OWASP LLM08:2025 Vector and Embedding Weaknesses](https://genai.owasp.org/llmrisk/llm082025-vector-and-embedding-weaknesses/)
- [OWASP LLM04:2025 Data and Model Poisoning](https://genai.owasp.org/llmrisk/llm042025-data-and-model-poisoning/)
- [MITRE ATLAS Framework](https://atlas.mitre.org/)
- [MITRE ATLAS Framework 2026 Guide](https://www.practical-devsecops.com/mitre-atlas-framework-guide-securing-ai-systems/)
- [C2PA Technical Specification 2.3](https://spec.c2pa.org/specifications/specifications/2.3/specs/C2PA_Specification.html)
- [C2PA Explainer](https://spec.c2pa.org/specifications/specifications/2.2/explainer/Explainer.html)

### Industry Reports and Blog Posts
- [Microsoft Security Blog: AI Recommendation Poisoning, Feb 2026](https://www.microsoft.com/en-us/security/blog/2026/02/10/ai-recommendation-poisoning/)
- [MintMCP: AI agent memory poisoning](https://www.mintmcp.com/blog/ai-agent-memory-poisoning)
- [Lakera: Agentic AI Threats Part 1, Nov 2025](https://www.lakera.ai/blog/agentic-ai-threats-p1)
- [Lakera: GenAI Security Readiness Report 2025](https://www.lakera.ai/genai-security-report-2025)
- [Lakera: Introduction to Data Poisoning 2025](https://www.lakera.ai/blog/training-data-poisoning)
- [Google Cloud: Lessons from 2025 on agents and trust](https://cloud.google.com/transform/ai-grew-up-and-got-a-job-lessons-from-2025-on-agents-and-trust)
- [AI Model Poisoning: The $12B Supply Chain Crisis](https://docs.cybersecfeed.com/blog/ai-model-poisoning-supply-chain-crisis)
- [Vec2vec attacks, May 2025](https://www.howweknowus.com/2025/05/23/vec2vec-attacks/)
- [Mend.io: AI Vector & Embedding Security Risks](https://www.mend.io/blog/vector-and-embedding-weaknesses-in-ai-systems/)
- [Prompt Security: The Embedded Threat in Your LLM](https://prompt.security/blog/the-embedded-threat-in-your-llm-poisoning-rag-pipelines-via-vector-embeddings)
- [Embedding Space Manipulation, AIQ.hu](https://aiq.hu/en/30-1-4-embedding-space-manipulation/)
- [Immutable Audit Trails: The Missing Piece in AI Accountability](https://quantumencoding.io/blog/immutable-audit-trails)
- [Obsidian Security: AI Agent Security Risks](https://www.obsidiansecurity.com/blog/ai-agent-security-risks)
- [Obsidian Security: AI Detection and Response](https://www.obsidiansecurity.com/blog/ai-detection-and-response)
- [RAG Data Poisoning Explained, Promptfoo](https://www.promptfoo.dev/blog/rag-poisoning/)
- [Cobalt: Vector and Embedding Weaknesses](https://www.cobalt.io/blog/vector-and-embedding-weaknesses)
- [NeuralTrust: AI-Driven Supply Chain Attacks 2025](https://neuraltrust.ai/blog/ai-driven-supply-chain-attacks)
- [LastPass: AI Model Poisoning in 2026](https://blog.lastpass.com/posts/model-poisoning)
