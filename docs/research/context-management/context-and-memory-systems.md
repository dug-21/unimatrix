# Context and Memory Systems for Multi-Project Agentic Development

**Research Date**: February 2026
**Status**: Comprehensive Research Document
**Relevance**: Critical -- this is the #1 technical challenge for Unimatrix

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [The Context Delivery Problem](#the-context-delivery-problem)
3. [Memory Architecture Patterns](#memory-architecture-patterns)
4. [Phase-Aware Context Delivery](#phase-aware-context-delivery)
5. [Token Optimization Strategies](#token-optimization-strategies)
6. [Storage Technologies Comparison](#storage-technologies-comparison)
7. [Existing Solutions Assessment](#existing-solutions-assessment)
8. [Multi-Project Context Isolation and Sharing](#multi-project-context-isolation-and-sharing)
9. [Knowledge Evolution and Pruning Strategies](#knowledge-evolution-and-pruning-strategies)
10. [Recommended Architecture Approach for Unimatrix](#recommended-architecture-approach-for-unimatrix)
11. [Open Questions](#open-questions)

---

## Executive Summary

Managing context for AI agents across multiple projects and development phases is the defining infrastructure challenge for agentic development platforms. The core tension is between **comprehensiveness** (agents need enough context to make correct decisions) and **parsimony** (every additional token competes for the model's attention, degrading performance and increasing cost).

Research from Anthropic, Google, JetBrains, and the broader academic community converges on several key principles:

1. **Context is a finite resource with diminishing returns.** Stuffing a 200K context window degrades performance. The goal is finding the *smallest set of high-signal tokens* that maximizes the desired outcome.

2. **Hierarchical, tiered memory architectures are the consensus approach.** Systems should separate working memory (in-context), short-term recall (session-scoped), and long-term knowledge (cross-session), with explicit mechanisms for promotion and demotion between tiers.

3. **Hybrid retrieval (vector + graph + structured) outperforms any single approach.** Vector search handles semantic similarity; knowledge graphs capture relationships and temporal validity; structured stores hold deterministic rules and conventions.

4. **Phase-aware delivery is an unsolved but critical capability.** No production system today dynamically adjusts context based on SDLC phase. This represents a significant opportunity for differentiation.

5. **Simple approaches often outperform sophisticated ones.** JetBrains research found that observation masking (simple placeholder replacement) outperformed LLM-based summarization in 4 of 5 test settings while being 52% cheaper. File-based approaches (CLAUDE.md, AGENTS.md) remain the pragmatic foundation.

6. **The Model Context Protocol (MCP) is the emerging standard** for how agents access external context, with 97M+ monthly SDK downloads and adoption across all major AI platforms.

**Recommended path for Unimatrix**: A layered architecture combining file-based context foundations (AGENTS.md hierarchy), a lightweight knowledge store (graph + vector hybrid), phase-aware context assembly, and MCP as the delivery protocol. Start simple, measure, and add sophistication only where token analysis proves the need.

---

## The Context Delivery Problem

### Problem Statement

Unimatrix manages 3-5 concurrent projects, each with distinct:

- **Architecture patterns** (microservices vs. monolith, different frameworks)
- **Coding conventions** (naming, structure, error handling)
- **Testing strategies** (unit, integration, E2E, different frameworks)
- **CI/CD pipelines** (different providers, deployment targets)
- **Security requirements** (compliance standards, auth patterns)

Agents working across these projects need the right context at the right time -- and critically, **not** the wrong context. An agent coding a React frontend should not have its attention diluted by backend deployment patterns. An agent writing tests should not be loaded with architecture decision records.

### The Five Dimensions of the Problem

```
                    ┌─────────────────────────┐
                    │    CONTEXT DELIVERY      │
                    │      CHALLENGE           │
                    └────────┬────────────────┘
                             │
        ┌────────────┬───────┼───────┬────────────┐
        ▼            ▼       ▼       ▼            ▼
   ┌─────────┐ ┌─────────┐ ┌────┐ ┌──────┐ ┌──────────┐
   │ SCOPE   │ │  PHASE  │ │TIME│ │VOLUME│ │FRESHNESS │
   │         │ │         │ │    │ │      │ │          │
   │ Which   │ │ What    │ │When│ │ How  │ │ Is it    │
   │ project?│ │ stage?  │ │now?│ │much? │ │ current? │
   └─────────┘ └─────────┘ └────┘ └──────┘ └──────────┘
```

1. **Scope**: Which project's conventions apply? Which are global?
2. **Phase**: Architecture, coding, testing, deployment -- each needs different knowledge
3. **Timing**: What's relevant to the immediate task vs. background knowledge?
4. **Volume**: How much can fit without degrading performance? (Token budget)
5. **Freshness**: Has this pattern been superseded? Is this convention still valid?

### Why This Is Hard

Anthropic's context engineering research articulates the fundamental tension: "Every token added to the context window is competing for the model's attention. Stuffing a hundred thousand tokens of history into the window actually degrades the model's ability to reason about what actually matters."

This is not just a retrieval problem -- it is a **curation** problem. The system must act as an intelligent librarian, not a search engine.

### Cost Implications

Without context management, a naive approach to multi-project context delivery would:

- Load all project conventions (~5-10K tokens per project x 5 projects = 25-50K tokens)
- Include all phase-relevant documentation (~10-20K tokens)
- Maintain full conversation history (~10-50K tokens per session)
- Include tool definitions (~5-10K tokens)

**Total: 50-130K tokens per request**, most of which is noise for any given task. At $15/M input tokens (Claude Opus), this costs $0.75-$1.95 per request -- and produces worse results than a targeted 10-15K token context.

---

## Memory Architecture Patterns

### Taxonomy of Agent Memory

Research converges on two complementary taxonomies that together provide a complete picture:

#### Cognitive-Inspired Taxonomy (CoALA Framework)

| Memory Type | Human Analogy | Agent Implementation | Persistence |
|-------------|---------------|---------------------|-------------|
| **Working Memory** | "What I'm thinking about now" | Current context window contents | Ephemeral (single inference) |
| **Semantic Memory** | "Facts I learned in school" | Factual knowledge in external stores | Long-term, updateable |
| **Episodic Memory** | "Things I did" | Past interactions, decisions, outcomes | Long-term, append-only |
| **Procedural Memory** | "How to ride a bike" | System prompts, rules, conventions | Long-term, versioned |

#### Architecture-Focused Taxonomy (Letta Framework)

| Layer | Contents | Access Pattern | Storage |
|-------|----------|---------------|---------|
| **Message Buffer** | Recent conversation turns | Always in context | In-context |
| **Core Memory** | Agent-managed key facts | Always in context, self-edited | In-context + persistent |
| **Recall Memory** | Full conversation history | Retrieved on demand | Database |
| **Archival Memory** | Structured knowledge base | Searched when relevant | Database |

### Pattern 1: Tiered Memory Architecture

The consensus architecture across Anthropic, Google ADK, and academic research is a **three-tier system**:

```
┌─────────────────────────────────────────────┐
│              WORKING CONTEXT                 │
│  (Ephemeral, recomputed per inference)       │
│                                              │
│  System prompt + current task + active       │
│  memory blocks + tool definitions +          │
│  recent conversation                         │
│                                              │
│  Budget: 10-30K tokens                       │
├─────────────────────────────────────────────┤
│              SESSION MEMORY                  │
│  (Persists within a task/session)            │
│                                              │
│  Conversation history, intermediate          │
│  results, tool outputs, agent notes          │
│                                              │
│  Access: Summarized/retrieved into working   │
│  Budget: Unlimited (stored externally)       │
├─────────────────────────────────────────────┤
│              LONG-TERM KNOWLEDGE             │
│  (Persists across sessions)                  │
│                                              │
│  Project conventions, architecture           │
│  decisions, learned patterns, team           │
│  preferences, past solutions                 │
│                                              │
│  Access: Semantically retrieved on demand    │
│  Budget: Unlimited (stored externally)       │
└─────────────────────────────────────────────┘
```

**Key principle from Google ADK**: Working context is an *ephemeral, recomputed projection* built fresh for each model invocation -- not a mutable text buffer. Context is *compiled* from the tiers below.

### Pattern 2: RAG-Based Context Assembly

Retrieval-Augmented Generation retrieves relevant documents/chunks from a vector store based on semantic similarity to the current query. For code/development context:

**Strengths**:
- Handles unstructured knowledge well (documentation, past conversations, code comments)
- Scales to large knowledge bases without token cost increase
- Semantic matching finds relevant context even with different terminology

**Weaknesses**:
- Retrieval quality varies -- may miss relevant context or include irrelevant content
- No understanding of relationships between retrieved chunks
- Temporal awareness is weak (doesn't know if a pattern is outdated)
- Code-specific challenges: code semantics differ from natural language semantics

**2025/2026 Advances**:
- **Granularity-Aware Retrieval**: Optimizing retrieval unit size from full documents to semantically aligned segments
- **Context Sufficiency Detection**: ICLR 2025 research enabling systems to know when they have enough context for a correct answer
- **Retrieval-Augmented Thoughts (RAT)**: Iteratively refining chain-of-thought reasoning with retrieved information

### Pattern 3: Knowledge Graph-Based Context

Knowledge graphs represent information as entities (nodes) and relationships (edges), enabling structured reasoning about interconnected facts.

**Strengths**:
- Captures relationships explicitly (e.g., "ServiceA depends on ServiceB", "Pattern X replaced Pattern Y")
- Supports temporal reasoning (validity periods for facts)
- Enables traversal-based retrieval (follow relationships to find related context)
- Natural fit for code architecture (dependencies, inheritance, API contracts)

**Weaknesses**:
- Requires upfront schema design and maintenance
- Construction is expensive (entity extraction, relationship mapping)
- Less effective for unstructured, narrative knowledge
- Query complexity can be high

**Key Technology**: Graphiti (by Zep) -- a framework for building temporally-aware knowledge graphs specifically for AI agents, supporting incremental updates and efficient retrieval in dynamic environments.

### Pattern 4: Hybrid Architecture (Recommended)

The most effective production systems combine multiple approaches. Research from NVIDIA and academic sources confirms that **hybrid retrieval outperforms any single approach**.

```
┌────────────────────────────────────────────────────┐
│                  CONTEXT ASSEMBLER                  │
│  (Compiles working context per inference)           │
│                                                     │
│  Inputs:                                            │
│  ├── File-based rules (AGENTS.md, project configs)  │
│  ├── Vector retrieval (semantic search)             │
│  ├── Graph traversal (relationship-based)           │
│  ├── Structured lookup (conventions, patterns)      │
│  └── Session state (recent history, notes)          │
│                                                     │
│  Output: Optimized context window                   │
└────────────────────────────────────────────────────┘
```

**Reciprocal Rank Fusion (RRF)** is the standard technique for combining results from multiple retrieval methods, merging rankings from vector similarity, graph traversal, and keyword search into a unified relevance score.

---

## Phase-Aware Context Delivery

### The SDLC Phase Problem

Different development phases require fundamentally different context:

| Phase | Needs | Does NOT Need |
|-------|-------|---------------|
| **Architecture** | System design docs, ADRs, dependency maps, capacity requirements, tech stack decisions | Unit test patterns, CI/CD configs, code style rules |
| **Implementation** | Coding conventions, API specs, type definitions, related code examples, error handling patterns | Architecture rationale, capacity planning, deployment topology |
| **Testing** | Test frameworks, coverage requirements, mock patterns, fixture conventions, E2E setup | Architecture decisions, deployment configs |
| **Code Review** | Style guide, security checklist, performance patterns, common anti-patterns | Test fixtures, deployment procedures |
| **Deployment** | CI/CD config, environment variables, rollback procedures, monitoring setup | Code style rules, test patterns |
| **Debugging** | Error logs, system topology, recent changes, monitoring dashboards | Architecture rationale, code style rules |

### Phase Detection Strategies

No production system today implements robust automatic phase detection. Approaches under investigation:

1. **Explicit Declaration**: Agent or user declares the current phase (simplest, most reliable)
2. **Task Analysis**: Infer phase from the task description ("write tests for..." = testing phase)
3. **File Context**: Infer from files being touched (*.test.ts = testing, Dockerfile = deployment)
4. **Conversation Analysis**: Detect phase shifts from conversation patterns
5. **Workflow Position**: If part of a defined workflow, phase is known from the workflow step

**Recommended approach for Unimatrix**: Combine explicit workflow positioning (when available) with file-context inference as fallback, allowing manual override.

### Phase-Aware Context Assembly

```
Phase Detection
      │
      ▼
┌─────────────────────────┐
│   CONTEXT POLICY ENGINE  │
│                          │
│  Phase: "implementation" │
│  Project: "project-alpha" │
│  Task: "add user API"    │
│                          │
│  INCLUDE:                │
│  ├── project-alpha/      │
│  │   conventions         │
│  ├── API patterns        │
│  ├── Type definitions    │
│  ├── Error handling      │
│  │   conventions         │
│  └── Related code        │
│      examples            │
│                          │
│  EXCLUDE:                │
│  ├── Architecture docs   │
│  ├── Test patterns       │
│  ├── Deployment configs  │
│  └── Other projects'     │
│      conventions         │
└─────────────────────────┘
```

### Context Policies as Configuration

Phase-aware delivery can be implemented as declarative policies:

```yaml
# Example: context-policy.yaml
phases:
  architecture:
    include:
      - knowledge_types: [adr, system_design, dependency_map, tech_decisions]
      - scope: [global, project]
    exclude:
      - knowledge_types: [code_style, test_patterns, ci_cd]
    token_budget: 15000

  implementation:
    include:
      - knowledge_types: [coding_conventions, api_specs, type_defs, examples]
      - scope: [project, shared_libraries]
    exclude:
      - knowledge_types: [architecture_rationale, deployment, capacity]
    token_budget: 12000

  testing:
    include:
      - knowledge_types: [test_conventions, mock_patterns, fixtures, coverage_reqs]
      - scope: [project]
    exclude:
      - knowledge_types: [architecture, deployment, code_style]
    token_budget: 10000
```

---

## Token Optimization Strategies

### Strategy 1: Context Compression

#### Observation Masking (Simple, Effective)

JetBrains Research (December 2025) found that **observation masking** -- replacing older tool outputs with placeholders while preserving agent reasoning -- outperformed LLM summarization in 4 of 5 test settings:

- **Cost reduction**: 50%+ compared to unmanaged contexts
- **Performance**: Matched or exceeded summarization
- **Why it works**: Preserves the agent's reasoning chain while eliminating redundant data
- **Why summarization underperformed**: Agents ran ~15% longer (trajectory elongation), summary generation consumed 7%+ of costs, and summaries obscured stop signals

**Implementation**: Replace tool outputs older than N turns with `[Output truncated -- see archival memory for details]`.

#### LLM-Based Summarization (Selective Use)

Best used selectively for:
- Compressing conversation history at session boundaries
- Generating session summaries for long-term storage
- Creating condensed versions of large retrieved documents

**Key lesson**: Use summarization at **tier boundaries** (session-to-long-term), not within the active working context.

#### ACON Framework (Adaptive Compression)

The ACON framework (2025) provides systematic, adaptive context compression:
- Reduces memory usage by 26-54% (peak tokens)
- Preserves >95% accuracy when using distilled compressors
- Uses failure-driven, task-aware compression guideline optimization

### Strategy 2: Smart Retrieval

| Technique | Description | Token Savings |
|-----------|-------------|--------------|
| **Granular retrieval** | Retrieve paragraphs/functions, not whole documents | 60-80% |
| **Re-ranking** | Score retrieved chunks for relevance before inclusion | 30-50% |
| **Context sufficiency** | Stop retrieving when enough context is gathered | 20-40% |
| **Deduplication** | Remove redundant information across retrieved chunks | 10-30% |
| **Adaptive depth** | Retrieve more for complex queries, less for simple ones | Variable |

### Strategy 3: Prompt Caching

Prompt caching is the single most impactful cost optimization for applications with repeated static content:

| Provider | Cost Reduction | Latency Reduction | Cache Lifetime | Minimum Tokens |
|----------|---------------|-------------------|----------------|----------------|
| **Anthropic** | Up to 90% | Up to 85% | 5 minutes | 1,024 |
| **OpenAI** | 50% | Variable | Automatic | 1,024 |
| **Google** | ~75% | Variable | Configurable | Variable |

**Best practice for Unimatrix**: Structure context with **static prefixes** (system instructions, project conventions, phase-specific rules) and **dynamic suffixes** (current task, recent conversation). This maximizes cache hit rates.

```
┌──────────────────────────────────────┐  ← Cached (static)
│ System instructions                   │
│ Project conventions                   │
│ Phase-specific rules                  │
│ Tool definitions                      │
├──────────────────────────────────────┤  ← Dynamic (per-request)
│ Retrieved context                     │
│ Recent conversation                   │
│ Current task                          │
└──────────────────────────────────────┘
```

### Strategy 4: Data Format Optimization

Format choice significantly impacts token count:

| Format | Relative Token Cost | Best For |
|--------|-------------------|----------|
| **Custom compact** | 1x (baseline) | Known schemas, internal transport |
| **CSV** | 1.2-1.5x | Tabular data |
| **YAML** | 1.5-2x | Configuration, human-readable |
| **JSON** | 2-2.5x | API responses, structured data |
| **Markdown** | 2-3x | Documentation, mixed content |
| **XML** | 3-4x | Legacy systems |

**Key finding**: CSV outperforms JSON by 40-50% for tabular data. For context delivery, prefer compact representations over verbose formats.

### Strategy 5: Progressive Disclosure

Rather than pre-loading all potentially relevant context, provide agents with **navigation capabilities**:

1. Start with a lightweight overview (file paths, summaries, identifiers)
2. Agent requests details as needed via tool calls
3. Detailed content is loaded just-in-time
4. After use, detailed content can be summarized or evicted

This mirrors Anthropic's recommended pattern: "Agents navigate and retrieve data autonomously, maintaining only what's necessary in working memory."

---

## Storage Technologies Comparison

### Vector Databases

| Feature | **Chroma** | **Qdrant** | **Weaviate** | **Pinecone** |
|---------|-----------|-----------|-------------|-------------|
| **Type** | Open source | Open source | Open source | Managed service |
| **Language** | Python/Rust (2025 rewrite) | Rust | Go | Proprietary |
| **Self-hosted** | Yes | Yes | Yes | No |
| **Managed option** | Limited | Yes (Qdrant Cloud) | Yes (Weaviate Cloud) | Yes (only option) |
| **Hybrid search** | Basic | Advanced filters | Native hybrid (vector + keyword) | Sparse-dense vectors |
| **Scale** | Small-medium | Large (billions) | Large | Large |
| **Latency** | Good | Excellent (<50ms) | Good | Excellent (<50ms) |
| **Best for** | Prototyping, small projects | Production, cost-sensitive | Feature-rich production | Zero-ops enterprise |
| **Pricing** | Free (OSS) | Free (OSS) / pay for cloud | Free (OSS) / pay for cloud | Pay per use (higher cost) |
| **Notable** | 4x faster after Rust rewrite | ACID transactions, advanced filtering | Built-in ML module support | Auto-scaling, multi-region |

**Recommendation for Unimatrix**: Start with **Qdrant** (self-hosted). It provides the best balance of performance, cost, production-readiness, and advanced filtering (critical for project/phase-scoped retrieval). Its Rust implementation offers excellent performance and small resource footprint. Chroma is a viable alternative for rapid prototyping.

### Graph Databases

| Feature | **Neo4j** | **Amazon Neptune** | **Memgraph** | **Graphiti** (library) |
|---------|-----------|-------------------|-------------|----------------------|
| **Type** | Native graph DB | Managed service | In-memory graph DB | KG framework (uses Neo4j/others) |
| **Query Language** | Cypher | Gremlin/SPARQL/openCypher | Cypher | Python API |
| **Temporal support** | Via properties | Via properties | Via properties | Native (first-class) |
| **LLM integration** | GraphRAG patterns | Neptune Analytics | Basic | Built for AI agents |
| **Best for** | Production knowledge graphs | AWS-native stacks | Real-time, high-throughput | Agent memory specifically |
| **Relevance to Unimatrix** | High (mature, well-documented) | Medium (if on AWS) | Medium (performance) | High (purpose-built for agents) |

**Recommendation for Unimatrix**: Use **Graphiti** (Zep's open-source KG framework) for the knowledge graph layer. It is purpose-built for AI agents with native temporal awareness, supporting incremental updates and efficient retrieval. It can use Neo4j as its backing store for production deployments.

### File-Based Systems

| Approach | Examples | Strengths | Weaknesses |
|----------|----------|-----------|------------|
| **Project root files** | CLAUDE.md, AGENTS.md, .cursorrules | Simple, version-controlled, universal | Flat, no phase-awareness, grows unwieldy |
| **Hierarchical files** | Nested CLAUDE.md files per directory | Scoped to subsystems, composable | Manual maintenance, no retrieval logic |
| **Strategy documents** | implementation-strategy.md, architecture.md | Rich, detailed, save tokens in subsequent sessions | Static, no dynamic adaptation |
| **Context folders** | .context/, .ai/ directories | Organized, can include session history | Manual structure, no search |

**Key insight**: File-based approaches are the **pragmatic foundation** that every system should start with. They are version-controlled, human-readable, and require no infrastructure. More sophisticated systems should augment, not replace, file-based context.

### Hybrid Approach Comparison

| Architecture | Retrieval Quality | Setup Complexity | Maintenance | Cost | Best For |
|-------------|-------------------|-----------------|-------------|------|----------|
| **Files only** | Low-medium | Minimal | Low | Free | Small projects, starting out |
| **Vector only** | Medium | Low-medium | Low | Low | Semantic search, documentation |
| **Graph only** | Medium-high (structured) | High | High | Medium | Relationship-heavy domains |
| **Vector + files** | Medium-high | Low-medium | Low | Low | Most development contexts |
| **Vector + graph** | High | High | Medium-high | Medium | Complex multi-project systems |
| **Vector + graph + files** | Highest | High | Medium | Medium | Enterprise multi-project (Unimatrix target) |

---

## Existing Solutions Assessment

### Mem0

**What it is**: A universal memory layer for AI agents, providing dynamic memory extraction, consolidation, and retrieval.

| Aspect | Details |
|--------|---------|
| **Architecture** | Extraction -> Update -> Storage pipeline with async summary generation |
| **Storage** | Vector store + optional graph variant (Mem0g) |
| **Graph variant** | Directed labeled graphs with entities as nodes, relationships as edges |
| **Performance** | 26% improvement over OpenAI baseline; 91% lower p95 latency; 90%+ token cost savings |
| **Latency** | 0.71s median, 1.44s p95 (base); 1.09s median, 2.59s p95 (graph variant) |
| **Integration** | Python/JavaScript SDKs; native in CrewAI, Flowise, LangFlow |
| **Backing** | $24M Series A; AWS selected as exclusive memory provider for Agent SDK |
| **Strengths** | Production-proven, excellent latency, broad integration ecosystem |
| **Weaknesses** | Generic (not development-specific), limited phase awareness, memory extraction is opaque |
| **Relevance to Unimatrix** | **Medium** -- good general memory layer but would need significant customization for development-specific context management |

Source: [Mem0 Research Paper](https://arxiv.org/abs/2504.19413), [Mem0 Documentation](https://docs.mem0.ai/introduction)

### Letta (formerly MemGPT)

**What it is**: A platform for building stateful agents with self-editing memory, based on the MemGPT research paper's OS-inspired memory management.

| Aspect | Details |
|--------|---------|
| **Architecture** | Core memory (RAM) + Archival memory (disk) + Recall memory (search) |
| **Key innovation** | Self-editing memory -- agents manage their own memory via tool calls |
| **Memory blocks** | Discrete functional units of context that agents can read/write |
| **Context Repositories** | New (Feb 2026): Git-based versioning for programmatic context management |
| **Agent architecture** | V1 architecture optimized for frontier reasoning models (GPT-5, Claude 4.5) |
| **Open source** | Yes (GitHub), with managed cloud offering |
| **Strengths** | Most sophisticated memory management; self-editing is powerful; active research lineage |
| **Weaknesses** | Complex to operate; memory management overhead can be significant; agent must spend tokens managing memory |
| **Relevance to Unimatrix** | **High** -- the memory block abstraction and self-editing pattern are directly applicable; Context Repositories align with version-controlled knowledge |

Source: [Letta Documentation](https://docs.letta.com/concepts/memgpt/), [Letta Blog](https://www.letta.com/blog/agent-memory)

### Zep

**What it is**: A context engineering and agent memory platform built on temporal knowledge graphs.

| Aspect | Details |
|--------|---------|
| **Architecture** | Temporal knowledge graph (Graphiti engine) + vector embeddings |
| **Key innovation** | Temporally-aware facts with validity periods; non-lossy knowledge graph updates |
| **Performance** | 18.5% accuracy improvement over MemGPT on DMR benchmark; 90% latency reduction |
| **Latency** | <200ms retrieval, optimized for real-time |
| **Graph engine** | Graphiti (open source) -- incrementally builds and queries temporal KGs |
| **Integration** | Amazon Neptune integration (Sep 2025) |
| **Strengths** | Best temporal reasoning; handles knowledge evolution natively; fast retrieval |
| **Weaknesses** | Graph construction overhead; less mature ecosystem than Mem0; focused on conversational memory |
| **Relevance to Unimatrix** | **High** -- temporal knowledge graphs directly address the knowledge evolution problem; Graphiti is available as a standalone library |

Source: [Zep Research Paper](https://arxiv.org/abs/2501.13956), [Graphiti GitHub](https://github.com/getzep/graphiti)

### LangMem (LangChain)

**What it is**: A library for building long-term memory in agent systems, part of the LangChain/LangGraph ecosystem.

| Aspect | Details |
|--------|---------|
| **Architecture** | Hot path (in-conversation tools) + background memory manager |
| **Memory types** | Semantic (facts), Episodic (experiences), Procedural (rules/instructions) |
| **Key innovation** | Procedural memory -- agents update their own system prompts based on learning |
| **Storage** | Works with any storage system; native LangGraph integration |
| **Background processing** | Automatic extraction, consolidation, and updating between sessions |
| **Strengths** | Framework-agnostic storage; good cognitive memory model; procedural memory is unique |
| **Weaknesses** | Tied to LangChain ecosystem; less production-hardened than Mem0/Zep; no temporal graph |
| **Relevance to Unimatrix** | **Medium** -- procedural memory (self-updating instructions) is valuable for convention evolution; the cognitive memory model is well-designed but can be replicated |

Source: [LangMem GitHub](https://github.com/langchain-ai/langmem), [LangMem Documentation](https://langchain-ai.github.io/langmem/)

### AWS AgentCore Long-Term Memory

**What it is**: AWS's managed memory service for AI agents, launched in 2025.

| Aspect | Details |
|--------|---------|
| **Architecture** | Managed service with namespace isolation, semantic extraction |
| **Key feature** | Namespace-based memory isolation (useful for multi-project) |
| **Storage** | Managed (AWS infrastructure) |
| **Integration** | Native Mem0 integration; works with any agent framework |
| **Strengths** | Zero-ops; namespace isolation; AWS ecosystem integration |
| **Weaknesses** | Vendor lock-in; less customizable; black-box extraction |
| **Relevance to Unimatrix** | **Low-Medium** -- useful if Unimatrix is AWS-native, but too opaque for a platform that needs fine-grained context control |

### Comparative Summary

| Solution | Memory Sophistication | Temporal Awareness | Dev-Specific Features | Production Readiness | OSS | Fit for Unimatrix |
|----------|----------------------|-------------------|----------------------|---------------------|-----|-------------------|
| **Mem0** | Medium | Low | None | High | Yes | Medium |
| **Letta** | High | Low | None | Medium | Yes | High |
| **Zep/Graphiti** | High | High | None | Medium-High | Partial | High |
| **LangMem** | Medium | Low | None | Low-Medium | Yes | Medium |
| **AWS AgentCore** | Low-Medium | Low | None | High | No | Low-Medium |

**Critical observation**: None of these solutions are development-specific. They are all general-purpose memory systems. Unimatrix's opportunity is to build a development-aware context layer that understands SDLC phases, project conventions, architecture patterns, and code relationships.

---

## Multi-Project Context Isolation and Sharing

### The Isolation Problem

When an agent works on Project A, it must not be polluted by Project B's conventions. But some knowledge is shared (global standards, team preferences, cross-project libraries).

### Isolation Architectures

#### Namespace-Based Isolation

```
┌─────────────────────────────────────────────┐
│              GLOBAL KNOWLEDGE               │
│  Team conventions, shared libraries,        │
│  cross-cutting standards                    │
├──────────┬──────────┬──────────┬────────────┤
│Project A │Project B │Project C │ Shared Libs│
│          │          │          │            │
│Conventions│Conventions│Conventions│ APIs      │
│Patterns  │Patterns  │Patterns  │ Types     │
│ADRs      │ADRs      │ADRs      │ Contracts │
│History   │History   │History   │           │
└──────────┴──────────┴──────────┴────────────┘
```

**Implementation**: Each project gets its own namespace/partition in the knowledge store. Retrieval queries are scoped to `[global + current_project + shared_libraries]`.

#### Monorepo vs. Multi-Repo Considerations

Research from Nx and Augment Code reveals important trade-offs:

| Aspect | Monorepo | Multi-Repo |
|--------|----------|-----------|
| **Agent context** | Full codebase visibility in one repo | Must bridge silos across repos |
| **Convention consistency** | Enforced via shared config | Must be synchronized manually |
| **Context scoping** | Path-based (agents see only relevant directories) | Repo-based (natural isolation) |
| **Cross-project reasoning** | Easy (everything in one tree) | Hard (requires explicit context assembly) |
| **AGENTS.md** | Single hierarchical file set | Must be maintained per repo |

**For Unimatrix**: Whether projects are in a monorepo or multi-repo, the context system must provide **logical isolation with controlled sharing**. The knowledge store should support:

1. **Project-scoped queries**: "What are Project A's API conventions?"
2. **Global queries**: "What is the team's error handling standard?"
3. **Cross-project queries**: "How does Project A's API contract affect Project B?"

### Context Sharing Patterns

| Pattern | When to Use | Implementation |
|---------|------------|----------------|
| **Inheritance** | Global conventions that all projects follow | Global namespace, always included |
| **Explicit import** | Shared library documentation | Cross-project reference with explicit inclusion |
| **On-demand retrieval** | Cross-project dependencies | Agent queries with cross-project scope when needed |
| **Never share** | Project-specific implementation details | Strict namespace isolation |

### Agent Isolation via Sub-Agents

Google ADK's research demonstrates that **many agents with isolated contexts outperform single-agent implementations**:

- Each sub-agent gets a focused context window for its specific sub-task
- Sub-agents return condensed results (1,000-2,000 tokens) to a coordinator
- Context translation on handoff prevents hallucination of prior agent actions
- This pattern naturally supports multi-project work: one sub-agent per project concern

---

## Knowledge Evolution and Pruning Strategies

### The Staleness Problem

Development knowledge evolves constantly:
- Architecture patterns get replaced
- Conventions get updated
- Dependencies get upgraded
- APIs get deprecated
- Team preferences shift

An agent using outdated patterns is worse than an agent with no patterns -- it will confidently produce wrong code.

### Evolution Mechanisms

Research identifies three core mechanisms for memory evolution:

#### 1. Consolidation (Merge + Generalize)

Combine related memories into higher-level patterns:
- Multiple instances of "use async/await for API calls" across sessions -> consolidated rule
- Several similar bug fixes -> generalized debugging pattern

#### 2. Updating (Revise + Replace)

When new information contradicts existing knowledge:
- Mark the old knowledge as **historical** (not deleted -- maintains audit trail)
- Create new knowledge entry with validity timestamp
- Archive previous version with metadata: `{superseded_by: "new_id", superseded_at: "2026-02-15"}`

**Zep's approach**: Graphiti maintains a timeline of facts and relationships including their periods of validity, enabling the system to answer "what was the convention in January?" while using current conventions for new work.

#### 3. Forgetting (Prune + Deprecate)

Deliberately remove low-value information:

| Signal | Action |
|--------|--------|
| Not accessed in N sessions | Mark as cold, exclude from active retrieval |
| Explicitly superseded | Archive with historical flag |
| Low relevance scores consistently | Reduce retrieval priority |
| Contradicted by recent evidence | Flag for review, reduce confidence |
| Domain no longer active | Move to cold storage |

### Pruning Strategies

#### Time-Based Decay

```
relevance_score = base_score * decay_factor^(days_since_last_access)
```

Knowledge that hasn't been accessed or reinforced gradually loses retrieval priority.

#### Usage-Based Reinforcement

Track which knowledge entries are actually used by agents:
- Entries that lead to successful outcomes get reinforced
- Entries that lead to errors or corrections get flagged for review
- Entries never retrieved get deprioritized

#### Conflict Resolution

When new knowledge conflicts with existing knowledge:

1. **Detect**: Embedding similarity + explicit contradiction detection
2. **Evaluate**: Compare timestamps, sources, confidence levels
3. **Resolve**: Newer, higher-confidence information wins; old version archived
4. **Notify**: Flag significant changes for human review

#### Evo-Memory Findings (2025)

Research on the Evo-Memory benchmark provides quantitative guidance:
- **Diverse domains** exhibit 36.8% pruning rates (more redundancy across heterogeneous tasks)
- **Concentrated domains** show 10-17% pruning rates (higher task similarity means more relevant knowledge)
- **Implication for Unimatrix**: Multi-project systems will need higher pruning rates due to cross-project knowledge overlap and divergence

### Recommended Evolution Pipeline for Unimatrix

```
New Information Arrives
        │
        ▼
┌──────────────────┐
│  Extract entities │
│  and relationships│
└────────┬─────────┘
         │
         ▼
┌──────────────────┐     ┌───────────────────┐
│ Check for         │────▶│ No conflict:      │
│ existing conflicts│     │ ADD new knowledge │
└────────┬─────────┘     └───────────────────┘
         │ Conflict found
         ▼
┌──────────────────┐     ┌───────────────────┐
│ Compare recency,  │────▶│ New wins:         │
│ confidence, source│     │ Archive old,      │
└────────┬─────────┘     │ store new         │
         │               └───────────────────┘
         │ Ambiguous
         ▼
┌──────────────────┐
│ Flag for human    │
│ review            │
└──────────────────┘
```

---

## Recommended Architecture Approach for Unimatrix

### Design Principles

1. **Start simple, add sophistication only where measured token analysis proves the need**
2. **File-based foundations first** -- AGENTS.md hierarchy is the pragmatic starting point
3. **Context is compiled, not accumulated** -- recompute working context per inference
4. **Isolation by default** -- projects see only their own knowledge unless explicitly shared
5. **Phase-awareness is a policy, not a model** -- declarative policies determine context inclusion
6. **MCP as the delivery protocol** -- standardized, ecosystem-compatible
7. **Measure everything** -- track token usage, cache hit rates, retrieval relevance, agent performance

### Proposed Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    AGENT RUNTIME                         │
│                                                          │
│  ┌─────────────┐  ┌───────────┐  ┌──────────────────┐  │
│  │ Working      │  │ Tool      │  │ MCP Servers       │  │
│  │ Context      │◀─│ Execution │◀─│ (context access)  │  │
│  │ (compiled)   │  │           │  │                   │  │
│  └──────┬───────┘  └───────────┘  └──────────────────┘  │
│         │                                                │
└─────────┼────────────────────────────────────────────────┘
          │ Compiled from:
          ▼
┌─────────────────────────────────────────────────────────┐
│               CONTEXT ASSEMBLY ENGINE                    │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────┐ │
│  │ Phase Policy  │  │ Token Budget │  │ Cache Manager │ │
│  │ Engine        │  │ Allocator    │  │               │ │
│  └──────┬───────┘  └──────┬───────┘  └───────┬───────┘ │
│         │                 │                    │         │
│         ▼                 ▼                    ▼         │
│  ┌──────────────────────────────────────────────────┐   │
│  │              CONTEXT COMPILER                     │   │
│  │                                                   │   │
│  │  1. Load static rules (AGENTS.md, project config)│   │
│  │  2. Apply phase policy (include/exclude)         │   │
│  │  3. Retrieve relevant knowledge (hybrid search)  │   │
│  │  4. Inject session state (recent history, notes) │   │
│  │  5. Optimize for token budget (compress, rank)   │   │
│  │  6. Structure for cache efficiency (static       │   │
│  │     prefix + dynamic suffix)                     │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
└──────────┬───────────────────────────────────────────────┘
           │ Reads from:
           ▼
┌─────────────────────────────────────────────────────────┐
│                   KNOWLEDGE STORES                       │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────┐ │
│  │ File Store    │  │ Vector Store │  │ Knowledge     │ │
│  │ (AGENTS.md,  │  │ (Qdrant)     │  │ Graph         │ │
│  │  configs,    │  │              │  │ (Graphiti +   │ │
│  │  strategies) │  │ Embeddings   │  │  Neo4j)       │ │
│  │              │  │ of code,     │  │              │ │
│  │ Git-versioned│  │ docs, convos │  │ Entities,    │ │
│  │              │  │              │  │ relationships,│ │
│  │              │  │ Namespaced   │  │ temporal      │ │
│  │              │  │ per project  │  │ validity      │ │
│  └──────────────┘  └──────────────┘  └───────────────┘ │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │              SESSION STORE                        │   │
│  │                                                   │   │
│  │  Conversation history, agent notes, tool outputs  │   │
│  │  Compacted on threshold, archived on session end  │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Implementation Phases

#### Phase 1: File-Based Foundation (Weeks 1-2)

**Goal**: Get value immediately with minimal infrastructure.

- Implement hierarchical AGENTS.md structure per project
- Create phase-specific context templates (architecture.md, implementation.md, testing.md)
- Build a simple context assembler that concatenates relevant files based on project + phase
- Implement basic token counting and budget enforcement
- Deliver context via MCP server

**Token budget**: Static file inclusion, ~5-10K tokens per request
**Infrastructure**: None (files in git)

#### Phase 2: Vector-Enhanced Retrieval (Weeks 3-5)

**Goal**: Enable semantic search across project knowledge.

- Deploy Qdrant (self-hosted, single node)
- Index project documentation, code comments, ADRs, past conversations
- Implement namespace isolation per project with global namespace for shared knowledge
- Build retrieval pipeline: query -> embed -> search -> re-rank -> inject
- Add context sufficiency detection (stop retrieving when enough context gathered)

**Token budget**: Dynamic retrieval adds 2-5K tokens of relevant context
**Infrastructure**: Qdrant instance, embedding model (local or API)

#### Phase 3: Knowledge Graph Layer (Weeks 6-9)

**Goal**: Capture relationships and enable temporal reasoning.

- Deploy Graphiti with Neo4j backing store
- Model development entities: projects, services, APIs, conventions, patterns, decisions
- Build incremental knowledge extraction from agent interactions
- Implement temporal validity for evolving knowledge
- Hybrid retrieval: combine vector similarity with graph traversal using RRF

**Token budget**: Graph-based retrieval adds structured, high-signal context
**Infrastructure**: Neo4j instance, Graphiti

#### Phase 4: Intelligent Context Assembly (Weeks 10-13)

**Goal**: Automated phase detection and adaptive context delivery.

- Implement phase detection (file-based inference + explicit declaration)
- Build context policy engine with declarative phase-aware policies
- Implement observation masking for long-running sessions
- Add prompt caching optimization (static prefix structuring)
- Build token budget allocation across context sources
- Implement context quality metrics and feedback loops

**Token budget**: Optimized to 10-15K total with high relevance
**Infrastructure**: Context assembly service

#### Phase 5: Learning and Evolution (Weeks 14+)

**Goal**: Knowledge that improves over time.

- Implement knowledge consolidation pipeline (merge related learnings)
- Add conflict detection and resolution for contradicting knowledge
- Build time-based decay and usage-based reinforcement
- Implement pruning pipeline with human-in-the-loop for ambiguous conflicts
- Add cross-project learning: patterns that work in Project A may apply to Project B

### Technology Stack Summary

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| **File-based context** | AGENTS.md + Git | Universal, version-controlled, zero infrastructure |
| **Vector store** | Qdrant (self-hosted) | Performance, cost, advanced filtering, Rust-based |
| **Knowledge graph** | Graphiti + Neo4j | Temporal awareness, agent-native, incremental updates |
| **Embedding model** | Local (e.g., nomic-embed-text) or API | Cost vs. quality trade-off; start with API, move local |
| **Context delivery** | MCP Server | Standard protocol, universal agent compatibility |
| **Session store** | SQLite or PostgreSQL | Simple, reliable, supports compaction |
| **Cache layer** | Prompt caching (provider-native) | 50-90% cost reduction on static context |

### Expected Outcomes

| Metric | Without System | With System (Target) |
|--------|---------------|---------------------|
| **Tokens per request** | 50-130K | 10-15K |
| **Context relevance** | ~30% (lots of noise) | >80% (targeted) |
| **Cost per request** | $0.75-$1.95 | $0.15-$0.30 |
| **Cross-project contamination** | Frequent | Rare (namespace isolation) |
| **Stale knowledge usage** | Undetectable | Tracked + pruned |
| **Phase-appropriate context** | Manual | Automatic |

---

## Open Questions

### Architecture Questions

1. **Where does the context assembly engine live?** As a standalone service, embedded in the agent runtime, or as an MCP server? Each has trade-offs for latency, coupling, and scalability.

2. **How granular should phase detection be?** Is "implementation" one phase, or should we distinguish "implementing a new feature" from "refactoring existing code" from "fixing a bug"?

3. **Should agents manage their own memory or should it be managed for them?** Letta's self-editing memory is powerful but costs tokens. Background management (LangMem style) is cheaper but less responsive. The right answer may be a hybrid.

4. **How do we handle cross-project conventions that diverge?** If Project A uses REST and Project B uses GraphQL, an agent working on integration code needs both contexts without confusion.

### Technical Questions

5. **What embedding model best captures code semantics?** General-purpose embeddings may not distinguish `async function fetchUser()` from `async function deleteUser()` effectively. Code-specific embedding models (CodeBERT, StarCoder embeddings) may be needed.

6. **How do we measure context quality?** What metrics tell us whether the context we're delivering is actually helping agents perform better? Options: task success rate with/without specific context, token efficiency ratio, agent self-reported confidence.

7. **What is the optimal token budget allocation across context sources?** How much should go to system instructions vs. retrieved knowledge vs. conversation history? This likely varies by phase and task.

8. **How do we handle context for multi-agent workflows?** When an architect agent hands off to a coding agent, how much of the architecture context should transfer? All of it? A summary? Just the relevant decisions?

### Operational Questions

9. **Who maintains the knowledge stores?** Automated extraction from agent interactions? Human curation? A combination? What is the human review burden?

10. **How do we detect when knowledge is outdated?** Can we detect "convention drift" automatically (e.g., agents are consistently not following a stored pattern)?

11. **What is the cold-start experience?** When a new project is added, how does the system bootstrap its knowledge? Manual seeding? Analysis of existing codebase? Learning from initial sessions?

12. **How do we handle catastrophic forgetting?** If a pruning cycle removes knowledge that later turns out to be important, how do we recover? Version-controlled knowledge stores (like Letta's Context Repositories) may help.

### Research Questions

13. **Can we build a feedback loop between agent performance and context quality?** If agents with Context Set A consistently outperform those with Context Set B, can we automatically optimize context assembly?

14. **Is there a universal "context sufficiency" signal?** Can we detect in real-time when an agent has enough context to complete a task well, and stop adding more?

15. **How do phase-aware context policies generalize?** Do policies developed for one team/stack transfer to others, or must each team calibrate from scratch?

---

## References

### Primary Sources

- [Effective Context Engineering for AI Agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) -- Anthropic
- [Context Engineering for Coding Agents](https://martinfowler.com/articles/exploring-gen-ai/context-engineering-coding-agents.html) -- Martin Fowler / Thoughtworks
- [Cutting Through the Noise: Smarter Context Management for LLM-Powered Agents](https://blog.jetbrains.com/research/2025/12/efficient-context-management/) -- JetBrains Research
- [Architecting Efficient Context-Aware Multi-Agent Framework for Production](https://developers.googleblog.com/architecting-efficient-context-aware-multi-agent-framework-for-production/) -- Google Developers
- [Memory in the Age of AI Agents: A Survey](https://arxiv.org/abs/2512.13564) -- arXiv
- [ACON: Optimizing Context Compression for Long-horizon LLM Agents](https://arxiv.org/abs/2510.00615) -- arXiv

### Solution Documentation

- [Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory](https://arxiv.org/abs/2504.19413)
- [Zep: A Temporal Knowledge Graph Architecture for Agent Memory](https://arxiv.org/abs/2501.13956)
- [Letta Documentation](https://docs.letta.com/concepts/memgpt/)
- [LangMem SDK](https://github.com/langchain-ai/langmem)
- [Graphiti: Build Real-Time Knowledge Graphs for AI Agents](https://github.com/getzep/graphiti)

### Context Standards

- [Model Context Protocol Specification](https://modelcontextprotocol.io/specification/2025-11-25)
- [AGENTS.md: One File to Guide Them All](https://layer5.io/blog/ai/agentsmd-one-file-to-guide-them-all/)
- [Agentic Coding Context Guide](https://softcery.com/lab/softcerys-guide-agentic-coding-best-practices)

### Technology Comparisons

- [Vector Database Comparison 2025](https://liquidmetal.ai/casesAndBlogs/vector-comparison/)
- [Best Vector Databases 2026](https://www.firecrawl.dev/blog/best-vector-databases-2025)
- [HybridRAG: Integrating Knowledge Graphs and Vector Retrieval](https://arxiv.org/abs/2408.04948)
- [From RAG to Context: 2025 Year-End Review](https://ragflow.io/blog/rag-review-2025-from-rag-to-context)

### Knowledge Evolution

- [Memory Evolution: How AI Agent Knowledge Systems Improve Over Time](https://fourweekmba.com/memory-evolution-how-ai-agent-knowledge-systems-improve-over-time/)
- [Evo-Memory: Benchmarking LLM Agent Test-time Learning](https://arxiv.org/html/2511.20857v1)
- [Making Sense of Memory in AI Agents](https://www.leoniemonigatti.com/blog/memory-in-ai-agents.html)

### Multi-Project Context

- [Monorepos & AI](https://monorepo.tools/ai)
- [Context from Internal Git Repos](https://elite-ai-assisted-coding.dev/p/context-from-internal-git-repos)
- [Container Use: Isolated Parallel Coding Agents](https://www.infoq.com/news/2025/08/container-use/)
