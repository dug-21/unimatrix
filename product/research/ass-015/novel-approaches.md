# ASS-015: Passive/Implicit Knowledge Acquisition — State of the Art

Research spike for Unimatrix. Explores whether Unimatrix can build its knowledge base
passively from agent signals (search queries, access patterns, tool call sequences) rather
than requiring explicit "store this knowledge" calls.

Date: 2026-03-03

---

## Table of Contents

1. [Signal-Based Knowledge Extraction](#1-signal-based-knowledge-extraction)
2. [Implicit Feedback Loops in AI Systems](#2-implicit-feedback-loops-in-ai-systems)
3. [Autonomous Knowledge Base Construction](#3-autonomous-knowledge-base-construction)
4. [Query-Log Mining for Knowledge Discovery](#4-query-log-mining-for-knowledge-discovery)
5. [Self-Organizing Knowledge Systems](#5-self-organizing-knowledge-systems)
6. [Observation-Driven Learning in Multi-Agent Systems](#6-observation-driven-learning-in-multi-agent-systems)
7. [Knowledge Distillation from Tool Calls](#7-knowledge-distillation-from-tool-calls)
8. [Synthesis: What This Means for Unimatrix](#8-synthesis-what-this-means-for-unimatrix)
9. [Recommended Architecture](#9-recommended-architecture)

---

## 1. Signal-Based Knowledge Extraction

### Key Concepts and Approaches

Signal-based knowledge extraction builds knowledge structures from usage signals — query
logs, access patterns, event streams, interaction sequences — rather than from explicitly
authored content. The core insight: **users reveal what matters by how they search, what
they access together, and what sequences they follow**.

Three primary paradigms have emerged:

**Log-to-Knowledge-Graph pipelines.** Systems like LEKG (Log Extraction to Knowledge Graph)
construct knowledge graphs from system logs using a local-to-global strategy: temporary log
instance graphs are built from grouped log entries, relation inference is performed against
each instance graph, and then triples are merged into a background knowledge graph for
global reasoning. The approach handles weakly-structured, non-natural-language log data
through template-based parsing followed by rule-based relation linking and constraint-based
validation.

**LLM-powered extraction from unstructured signals.** The 2024-2025 paradigm shift uses LLMs
to reframe information extraction as a generative task. Few-shot prompting with GPT-4 or
Claude achieves accuracy roughly equivalent to fully supervised traditional models without
requiring labeled training examples. This is the key enabler: Unimatrix can pass raw agent
interaction traces to an LLM and get structured knowledge back.

**Enterprise signal aggregation.** Glean ($7.2B valuation, 2025) builds unified Enterprise
Knowledge Graphs by connecting to all company apps, indexing every document and conversation,
and using vector embeddings plus behavioral signals (who accessed what, when, with whom) to
make everything searchable through a single, permission-aware interface. Their system learns
from search patterns and access patterns to improve relevance ranking over time.

### Notable Systems/Papers

| System | Year | Key Contribution |
|--------|------|------------------|
| LEKG | 2021 | Log-to-KG pipeline with rule-based inference |
| Glean | 2023-2025 | Enterprise signal aggregation at scale |
| GraphRAG (Microsoft) | 2024 | LLM-built knowledge graphs from text with community summarization |
| KGGen | 2025 (NeurIPS) | Open-source LLM KG extraction with entity clustering, 18% improvement over GraphRAG |

### Relevance to Unimatrix

**HIGH.** Unimatrix already records usage telemetry (crt-001) and co-access patterns
(crt-004). The gap is extraction: turning raw signals into knowledge entries. The LEKG
pattern (local instance graphs merged into a global graph) maps directly to Unimatrix's
architecture — tool call sequences form "local instance graphs" per session, and cross-session
merging builds the global knowledge base.

### Feasibility Assessment

**HIGH.** Unimatrix already has the signal collection infrastructure (usage tracking,
co-access boosting, outcome tracking). The missing piece is an extraction layer that
periodically synthesizes signals into candidate knowledge entries.

### Can This Be GREAT?

**Yes, if** the extraction is continuous and the quality signal is closed-loop. The
difference between good and great: good systems extract once; great systems extract, measure
whether the extraction was useful (via subsequent access patterns), and refine the extraction
heuristics. Unimatrix's existing confidence system (crt-002) provides the measurement layer.

---

## 2. Implicit Feedback Loops in AI Systems

### Key Concepts and Approaches

Implicit feedback is any signal that reveals user preference without explicit instruction.
In traditional IR: clicks, dwell time, query reformulations, abandonment. In LLM agent
systems: **tool call sequences, search-then-not-store patterns, repeated queries for similar
concepts, query reformulations, access-without-helpful-vote patterns**.

Key developments in 2024-2025:

**MemOS (Memory Operating System).** Released May 2025, MemOS elevates memory to a
first-class operational resource with a three-layer architecture: memory API, memory
scheduling/management, and memory storage/infrastructure. Three memory types — parametric
(model weights), activation (KV-cache/hidden states), and plaintext (external documents) —
are unified under a "Memory Cube" abstraction. MemOS v2.0 (Dec 2025) added tool memory for
agent planning and memory feedback mechanisms. The key insight: **memory is not just storage;
it needs scheduling, lifecycle management, and cross-agent composition**.

**Reinforcement Learning from AI Feedback (RLAIF).** The RLHF paradigm has been extended to
use AI evaluators instead of human annotators. This is directly applicable: Unimatrix can
use an LLM to evaluate whether a candidate knowledge entry (extracted from signals) meets
quality thresholds, closing the feedback loop without human intervention.

**Implicit short-term memory in LLMs.** Transient intermediate representations during
inference (KV-caches, hidden states) continuously influence attention distributions and
behavioral strategies. This means agent behavior already encodes implicit knowledge about
task context — the challenge is externalizing it.

**Experience-Following Behavior (2025).** An empirical study by Xiong et al. demonstrated
that LLM agents display an "experience-following property": high similarity between a task
input and a retrieved memory record results in highly similar outputs. This confirms that
**memory contents directly shape agent behavior**, making knowledge quality critical.
Selective memory addition + deletion strategies yielded 10% absolute performance gains over
naive memory growth.

### Notable Systems/Papers

| System/Paper | Year | Key Contribution |
|-------------|------|------------------|
| MemOS | 2025 | Memory as first-class OS resource with lifecycle management |
| MemoryOS (EMNLP 2025 Oral) | 2025 | Personalized agent memory operating system |
| Experience-Following (Xiong et al.) | 2025 | Empirical proof that memory shapes agent behavior |
| RLAIF | 2024-2025 | AI-evaluator feedback loops replacing human annotation |

### Relevance to Unimatrix

**CRITICAL.** Unimatrix's existing helpful/unhelpful vote system is explicit feedback.
Implicit feedback signals that Unimatrix could capture today but does not:

- **Search-not-found patterns**: Agent searches, gets no results, then manually stores
  knowledge. The search query itself reveals a knowledge gap.
- **Repeated similar queries**: Multiple agents searching for similar concepts in a short
  window reveals high-demand knowledge that should be promoted.
- **Access-then-reformulate**: Agent accesses an entry, then searches again with different
  terms — suggests the entry was not helpful or was incomplete.
- **Co-access temporal proximity**: Entries accessed within the same tool-call chain are
  implicitly related, even if never explicitly linked.
- **Store-then-never-access**: Knowledge that was stored but never subsequently accessed —
  either low quality or poorly indexed.

### Feasibility Assessment

**HIGH.** Most of these signals are already partially captured by Unimatrix's telemetry. The
missing piece is a signal interpreter that detects these patterns and acts on them (boosting
confidence, creating implicit links, flagging stale entries, or synthesizing new entries).

### Can This Be GREAT?

**Yes.** This is potentially the highest-impact area. The experience-following research
proves that memory quality directly determines agent performance. A system that automatically
curates its own knowledge based on usage signals — boosting what works, pruning what does not,
filling gaps where agents struggle — is a genuine competitive advantage. The key differentiator
over MemOS: Unimatrix is a shared knowledge base across a swarm, not per-agent memory, so
the implicit signals from one agent's behavior benefit all agents.

---

## 3. Autonomous Knowledge Base Construction

### Key Concepts and Approaches

The 2024-2025 landscape saw autonomous KB construction reach production maturity. The shift
from rule-based and statistical pipelines to LLM-driven generative frameworks has
fundamentally changed what is possible.

**AutoSchemaKG (HKUST, 2025).** A framework for fully autonomous KG construction that
eliminates predefined schemas. It processes 50M+ documents to build ATLAS — a knowledge graph
family with 900M+ nodes and 5.9B edges. Schema induction achieves 92% semantic alignment
with human-crafted schemas with zero manual intervention. The key innovation: treating events
(not just entities) as basic semantic units captures temporal relationships, causality, and
procedural knowledge. This is directly relevant to Unimatrix, where agent workflows are
fundamentally event-driven.

**KARMA (NeurIPS 2025 spotlight).** A multi-agent framework using nine collaborative LLM
agents for KG enrichment: ingestion, reading, summarizing, entity extraction, relationship
extraction, schema alignment, and conflict resolution. Tested on 1,200 PubMed articles, it
identified 38,230 new entities with 83.1% LLM-verified correctness and reduced conflict
edges by 18.6% through multi-layer assessments. The conflict resolution via LLM-based debate
is particularly relevant — Unimatrix already has contradiction detection (crt-003) but could
use a more sophisticated resolution mechanism.

**KGGen (NeurIPS 2025).** Open-source framework combining LLM-based entity/relation
extraction with iterative clustering to reduce sparsity and redundancy. Achieves 18%
improvement over GraphRAG and 36% over OpenIE. Available as `pip install kg-gen`.

**GraphRAG (Microsoft, 2024-2025).** Extracts entity knowledge graphs from source documents,
builds community hierarchies, generates community summaries, and leverages these for
retrieval. Substantial improvements over naive RAG for global sensemaking questions. Now
available through Microsoft Discovery.

**Ontogenia (2025).** Uses Metacognitive Prompting for ontology generation — the model
performs self-reflection and structural correction during synthesis. The CQbyCQ framework
demonstrated that LLMs can translate competency questions and user stories into OWL-compliant
schemas.

### Relevance to Unimatrix

**HIGH.** The AutoSchemaKG event-centric approach aligns with Unimatrix's workflow-oriented
knowledge. KARMA's multi-agent architecture with specialized extraction agents maps to
Unimatrix's agent swarm pattern. KGGen provides an immediately usable tool for extracting
knowledge graphs from agent conversation logs.

The most relevant insight: **schema induction can be autonomous.** Unimatrix currently uses
a fixed category/topic/tag taxonomy. An AutoSchemaKG-inspired approach could let the taxonomy
evolve based on what agents actually store and search for.

### Feasibility Assessment

**MEDIUM-HIGH.** The extraction technology exists and works. The challenge is adaptation:
these systems were built for natural language text, not tool-call traces. Unimatrix would
need to either (a) convert agent traces to natural language summaries first, then extract, or
(b) build domain-specific extraction prompts for tool-call patterns. Option (a) is more
feasible with current technology.

### Can This Be GREAT?

**Yes, with domain adaptation.** Generic KG construction is good. Domain-specific extraction
tuned to software development patterns (architecture decisions, bug patterns, deployment
procedures, testing strategies) is great. The key: Unimatrix knows the domain vocabulary
and workflow structure — this prior knowledge makes extraction far more precise than
domain-agnostic approaches.

---

## 4. Query-Log Mining for Knowledge Discovery

### Key Concepts and Approaches

Query-log mining extracts structured knowledge from search behavior. The field is
experiencing a renaissance due to the shift from keyword search to conversational queries.

**LLM Query Mining.** A VLDB 2025 workshop survey cataloged 210 published studies on
LLM-based intent mining between 2015 and 2025, with 70%+ appearing in the last four years.
Key insight: LLM prompts are longer, more conversational, and closer to how people actually
think about problems compared to traditional search logs. This makes them richer signals for
knowledge extraction.

**Intent Mining from Conversational Queries.** 54% of consumers shifted toward conversational
search habits by 2025. Users' queries are vague and ambiguous, with exploratory needs that
diverge from stated intent. Mining these patterns reveals not just what users want, but what
they do not yet know they need — a form of knowledge gap detection.

**Grounding LLMs for Database Exploration (VLDB 2025).** Intent scoping and paraphrasing for
robust query understanding. The approach decomposes vague queries into structured intents,
applicable to Unimatrix's search queries.

**Intent-Based Query Rewriting (VLDB 2025).** Rewrites user queries based on inferred intent
rather than literal terms. Applied to Unimatrix: when an agent's search query does not match
stored knowledge, the system could infer intent and either find related entries or flag the
gap for knowledge creation.

### Relevance to Unimatrix

**HIGH and directly actionable.** Every `context_search` call is a query log entry. Every
`context_lookup` is a structured filter. Every `context_briefing` request reveals what role
and task combination needs knowledge. These logs already exist in Unimatrix's usage tracking.

Specific mining opportunities:

- **Query clustering**: Group similar search queries to identify recurring knowledge needs.
  If 5 different agents search for variations of "how to handle schema migration", that is a
  knowledge entry waiting to be created.
- **Failed query analysis**: Searches that return zero results or low-confidence results
  reveal knowledge gaps. These can be aggregated into a "knowledge gap report" or
  auto-generated as candidate entries.
- **Query-result satisfaction**: When an agent searches, accesses a result, and then does
  NOT reformulate — the entry satisfied the need. When they DO reformulate — it did not.
  This implicit signal is more reliable than explicit votes.
- **Temporal query patterns**: Knowledge needs that spike during certain workflow phases
  (e.g., many architecture queries during design phases) can be used to preemptively surface
  relevant knowledge in briefings.

### Feasibility Assessment

**HIGH.** This requires only analysis of data Unimatrix already collects. No new
infrastructure needed — just an analysis pipeline that runs periodically (or could be
triggered by `context_status maintain=true`).

### Can This Be GREAT?

**Yes.** The key insight from VLDB 2025 research: **conversational queries encode intent,
not just information needs.** Unimatrix's search queries from agents carry rich intent
signals because agents operate within known workflow contexts (design, implementation,
bugfix). Combining query analysis with workflow context (from outcome tracking, col-001) is
unique to multi-agent development orchestration. No existing system does this.

---

## 5. Self-Organizing Knowledge Systems

### Key Concepts and Approaches

Self-organizing knowledge systems automatically categorize, link, and evolve knowledge
entries based on usage patterns rather than manual curation. The field is transitioning from
static taxonomies to adaptive "knowledge fabrics."

**A-Mem (NeurIPS 2025).** Agentic Memory for LLM Agents, inspired by the Zettelkasten
method. When a new memory is added, a comprehensive note is generated with structured
attributes (contextual descriptions, keywords, tags). The system analyzes the historical
memory repository to establish connections based on semantic similarity and shared attributes.
The "memory evolution" mechanism updates contextual information of linked older notes when
new memories are integrated — **new knowledge reshapes understanding of past knowledge**.
Outperforms baselines across six foundation models on long-term conversational tasks.

**ACE — Agentic Context Engineering (Stanford/SambaNova/Berkeley, Oct 2025).** Treats
contexts as evolving "playbooks" maintained by three roles: Generator, Reflector, Curator.
Performs delta updates — localized edits that accumulate insights while preserving prior
knowledge. A "grow-and-refine" mechanism merges or prunes context items based on semantic
similarity. Addresses two critical problems: brevity bias (dropping domain insights for
concise summaries) and context collapse (iterative rewriting eroding details). Results:
+10.6% on agents, +8.6% on finance. **Adapts effectively without labeled supervision by
leveraging natural execution feedback.**

**ML-powered clustering for knowledge organization.** Industry practice (2025) shows that
ML-powered clustering identifies more meaningful content relationships than manual
categorization while reducing taxonomy maintenance costs. Advanced analytics tools
automatically categorize and tag data based on relevance and usage patterns.

**Knowledge fabrics.** AI technologies are turning static repositories into adaptive systems
that continuously index, interpret, and surface organizational intelligence at the moment of
need.

### Notable Systems/Papers

| System/Paper | Year | Key Contribution |
|-------------|------|------------------|
| A-Mem | 2025 (NeurIPS) | Zettelkasten-inspired dynamic memory with evolution |
| ACE | 2025 | Delta-update playbooks with Generator/Reflector/Curator |
| AutoSchemaKG | 2025 | 92% schema alignment with zero manual intervention |

### Relevance to Unimatrix

**CRITICAL.** This is the closest match to Unimatrix's architecture and goals.

A-Mem's approach maps directly: Unimatrix entries already have structured attributes
(title, content, category, topic, tags). The missing piece is **automatic linking based on
semantic similarity when entries are stored** and **evolution of existing entries when related
new knowledge arrives.**

ACE's three-role architecture (Generator, Reflector, Curator) maps to a pipeline that could
run within Unimatrix's maintenance cycle:
- Generator: Extract candidate knowledge from agent signals
- Reflector: Evaluate candidate quality against existing knowledge
- Curator: Merge, prune, or promote based on semantic similarity and usage signals

The delta-update pattern solves a real problem: Unimatrix currently either stores a new entry
or corrects an existing one. There is no "evolve an existing entry with new context" —
only replace. ACE's grow-and-refine mechanism is a better model.

### Feasibility Assessment

**MEDIUM-HIGH.** A-Mem's linking mechanism could be implemented using Unimatrix's existing
vector index for similarity matching. ACE's delta-update pattern requires new mutation
semantics beyond store/correct. The Reflector role needs LLM evaluation, which adds latency
and cost.

### Can This Be GREAT?

**Yes.** Combining A-Mem's evolution mechanism (new knowledge reshapes old) with ACE's
delta-update pattern (grow-and-refine without collapse) and Unimatrix's confidence system
(usage-weighted quality signals) creates something none of these systems achieve alone:
**a shared, multi-agent knowledge base that organizes itself based on collective agent
behavior, evolves entries based on new discoveries, and maintains quality through implicit
usage signals.** This is the core thesis.

---

## 6. Observation-Driven Learning in Multi-Agent Systems

### Key Concepts and Approaches

Multi-agent observation-driven learning encompasses systems where agents learn from observing
each other's behavior — tool usage, decision patterns, success/failure outcomes — rather than
from explicit communication.

**LLM-Based Multi-Agent Systems for Software Engineering.** A comprehensive literature
review (ACM TOSEM, 2025) covers the state of multi-agent systems for SE. Key systems:
ChatDev (waterfall), AgileCoder (agile with sprints). Both demonstrate that agent swarms can
produce software through collaborative workflows, but neither implements learning from
observation — they execute fixed protocols.

**AgentTrace (Feb 2025).** A structured logging framework that instruments agents at runtime
with minimal overhead across three surfaces:
- **Operational**: What the agent did (tool calls, actions, state changes)
- **Cognitive**: Why the agent did it (reasoning traces, decision rationale)
- **Contextual**: Environment state, available resources, constraints

AgentTrace captures not just actions but the reasoning behind them. This is directly relevant:
if Unimatrix can capture cognitive traces alongside operational traces, it can extract not
just "what patterns emerge" but "why agents make certain decisions."

**Voyager's Skill Library (2023, continued influence through 2025).** Voyager builds an
ever-growing library of executable skills from agent experience in Minecraft. Skills are
temporally extended, interpretable, and compositional. The agent stores successful code
fragments and retrieves them for similar future tasks. Results: 3.3x more unique items,
15.3x faster tech tree progression. The key insight: **successful execution traces become
reusable knowledge.** In Unimatrix's domain: successful agent workflows (sequences of
tool calls that resolved a bug, completed a design review, etc.) are the equivalent of
Voyager's skill library entries.

**ReflecTool (2024) and ToolMem (2025).** Two complementary approaches to tool-usage
learning:
- ReflecTool progressively enlarges long-term memory by saving successful solving processes
  and tool-wise experience. Uses an optimization/inference cycle with iterative refinement.
- ToolMem records empirical performance signals for each tool, enabling accuracy-aware tool
  selection. Explicitly tracks tool-specific strengths and weaknesses.

Both demonstrate that **agents can learn which tools work for which situations by observing
their own tool usage outcomes.** Applied to Unimatrix: tool call success/failure patterns
across agents could inform a "tool effectiveness knowledge base."

**SiriuS.** Extends experience replay to multi-agent dialogues. Failed trajectories are
post-hoc repaired by another agent or offline process and added as positive examples. Models
bootstrap improvement by learning to evaluate and enhance outputs from initially imperfect
attempts.

### Notable Systems/Papers

| System/Paper | Year | Key Contribution |
|-------------|------|------------------|
| Voyager | 2023 | Skill library from execution traces |
| AgentTrace | 2025 | Three-surface structured observability |
| ReflecTool | 2024 | Iterative tool-wise experience accumulation |
| ToolMem | 2025 | Tool capability memory from interaction history |
| SiriuS | 2024-2025 | Failed trajectory repair and replay |
| LLM-MAS for SE (ACM TOSEM) | 2025 | Comprehensive survey of SE multi-agent patterns |

### Relevance to Unimatrix

**CRITICAL.** Unimatrix already has the observation infrastructure:
- `unimatrix-observe` crate (col-002) with 21 detection rules
- Outcome tracking (col-001) with structured tags
- Co-access boosting (crt-004)

What is missing:
1. **Cognitive trace capture**: AgentTrace's three-surface model suggests capturing not just
   operational traces but reasoning context. Currently, Unimatrix sees tool calls but not
   why an agent made those calls.
2. **Skill library pattern**: Successful workflow sequences are not extracted and stored as
   reusable knowledge. The Voyager model suggests they should be.
3. **Failed trajectory learning**: SiriuS's approach of repairing failed trajectories maps
   to Unimatrix's lesson-learned category — but currently lessons must be explicitly stored.
   They could be auto-generated from failed outcome signals.

### Feasibility Assessment

**MEDIUM.** Operational trace capture exists. Cognitive trace capture requires either changes
to agent prompt structure (to emit reasoning alongside tool calls) or LLM-based inference of
reasoning from tool call sequences. Skill library extraction requires defining "success" for
workflow sequences — partially available via outcome tracking but needs refinement.

### Can This Be GREAT?

**Yes, if** Unimatrix combines observation across the swarm. Individual agent learning from
own traces is good (Voyager, ReflecTool). Cross-agent observation learning — where one
agent's successful bug-fix pattern becomes available to all agents — is great. This is
Unimatrix's unique position: it is the shared memory layer for a multi-agent swarm, so
observations naturally aggregate across agents.

---

## 7. Knowledge Distillation from Tool Calls

### Key Concepts and Approaches

This is the most novel and directly applicable area. Tool call sequences in LLM agent systems
encode implicit knowledge about task decomposition, problem-solving strategies, and domain
patterns.

**Agent Distillation (May 2025, open-sourced).** Transfers not just reasoning but full
task-solving behavior from LLM agents into smaller models. The distillation includes
supervised imitation of tool invocation, code execution, retrieval, and explicit sub-task
decompositions. Key insight: **tool call sequences are a learnable pattern**, not random
noise.

**Telemetry-Aware Development (MCP, 2025).** AI-first IDEs should be "telemetry-aware,"
treating agent interactions with the same rigor as code. The Model Context Protocol (MCP)
provides a standardized interface for tool interactions. The convergence on OpenTelemetry
(OTEL) standards for agent telemetry means structured tool-call data is becoming standardized.

**Self-Evolving Agents Survey (2025-2026).** Two comprehensive surveys catalog the field:
- "What, When, How, Where to Evolve" (Jan 2026): Taxonomy covering single-agent, multi-agent,
  and domain-specific optimization
- "Bridging Foundation Models and Lifelong Agentic Systems" (Aug 2025): Domain-specific
  evolution for biomedicine, programming, and finance

Key finding: self-evolving agents adjust non-parametric components (memory, tools) as well as
parametric ones. **Memory and tool evolution can happen without model retraining.**

**ACE's Natural Execution Feedback.** ACE adapts effectively without labeled supervision by
leveraging natural execution feedback. This is the closest existing approach to what Unimatrix
needs: using the outcomes of agent tool calls as implicit training signals for knowledge
quality.

### What Can Be Extracted from Tool Calls

For Unimatrix specifically, tool call sequences encode:

1. **Problem-solving patterns**: When agents consistently search for X, then lookup Y, then
   store Z — that sequence is a reusable procedure.
2. **Knowledge dependency graphs**: If agents always access entries A and B together before
   accessing C — there is an implicit dependency that should be explicit.
3. **Workflow effectiveness**: Tool call sequences that lead to successful outcomes (positive
   outcome records) versus sequences that lead to failures — this is labeled training data
   for "what works."
4. **Knowledge gaps**: Tool calls to `context_search` that return empty or low-confidence
   results, especially when followed by `context_store` — the agent is creating knowledge
   that should have existed.
5. **Concept emergence**: When agents start using new terms in searches that do not match
   existing entries — new concepts are emerging in the codebase that the knowledge base has
   not captured.
6. **Agent specialization patterns**: Which agents access which categories most — reveals
   implicit agent-knowledge affinities that can improve briefing relevance.

### Notable Systems/Papers

| System/Paper | Year | Key Contribution |
|-------------|------|------------------|
| Agent Distillation (Nardien et al.) | 2025 | Tool-call sequence learning for smaller models |
| ACE | 2025 | Execution feedback as implicit supervision |
| Self-Evolving Agents Survey | 2025-2026 | Taxonomy of non-parametric agent evolution |
| AgentTrace | 2025 | Structured telemetry for cognitive + operational traces |
| Mind the Metrics (MCP) | 2025 | Telemetry-aware IDE patterns |

### Relevance to Unimatrix

**HIGHEST.** This is the most novel and directly actionable area. Unimatrix is an MCP server
that processes tool calls — it is literally the system through which tool calls flow. Every
`context_search`, `context_lookup`, `context_get`, `context_store`, `context_correct`,
`context_briefing` call passes through Unimatrix. This is a unique vantage point that no
other system in the architecture shares.

### Feasibility Assessment

**HIGH for basic extraction, MEDIUM for advanced.** Basic patterns (knowledge gaps from
failed searches, co-access from sequential calls, access frequency for confidence) are
implementable with existing infrastructure. Advanced patterns (procedure extraction from
tool-call sequences, concept emergence detection) require LLM-based analysis of aggregated
trace data, which adds cost and latency but is technically straightforward.

### Can This Be GREAT?

**This is the GREAT opportunity.** No production system today extracts reusable knowledge
from MCP tool call patterns in a multi-agent development context. The combination of:
- Tool call telemetry (MCP server vantage point)
- Cross-agent observation (shared knowledge base)
- Outcome correlation (col-001 tracking)
- Confidence evolution (crt-002)
- Existing contradiction detection (crt-003)

...creates a closed loop that no other system has. This is where Unimatrix should invest.

---

## 8. Synthesis: What This Means for Unimatrix

### The Core Thesis

Unimatrix can evolve from a **passive knowledge store** (agents explicitly store and
retrieve) to an **active knowledge fabric** (the system observes, extracts, organizes, and
evolves knowledge from agent signals). The research confirms this is feasible with current
technology and would be genuinely novel in the multi-agent development orchestration domain.

### What Already Exists in Unimatrix

| Capability | Current State | Research Analog |
|-----------|---------------|-----------------|
| Usage tracking (crt-001) | Records access events | AgentTrace (operational surface) |
| Co-access boosting (crt-004) | Boosts co-accessed entries | Knowledge graph link prediction |
| Confidence evolution (crt-002) | Additive weighted composite | ToolMem's performance signals |
| Contradiction detection (crt-003) | Heuristic-based | KARMA's LLM-based debate |
| Outcome tracking (col-001) | Structured success/failure | Voyager's skill library labels |
| Observation pipeline (col-002) | 21 detection rules | AgentTrace (pattern detection) |
| Coherence gate (crt-005) | Lambda health metric | ACE's quality control |

### What Is Missing

| Gap | Research Solution | Priority |
|-----|-------------------|----------|
| Knowledge gap detection from failed searches | Query-log mining | P0 |
| Auto-extraction of procedures from tool-call sequences | Voyager skill library + agent distillation | P0 |
| Implicit linking based on co-access patterns | A-Mem's Zettelkasten linking | P1 |
| Entry evolution (delta updates) | ACE's grow-and-refine | P1 |
| Schema/taxonomy evolution | AutoSchemaKG's dynamic schema induction | P2 |
| Cognitive trace capture | AgentTrace's three-surface model | P2 |
| Failed trajectory repair | SiriuS multi-agent replay | P3 |

### Three Tiers of Implementation

**Tier 1 — Signal Interpretation (low cost, high impact, existing infrastructure)**
- Knowledge gap detection: Analyze `context_search` calls with zero/low results
- Query clustering: Group similar searches to identify recurring needs
- Access pattern analysis: Detect entries that are frequently accessed together but not linked
- Staleness detection: Flag entries stored but never accessed
- Demand signals: Identify topics with high search volume but low entry counts

**Tier 2 — Passive Extraction (medium cost, high impact, requires LLM calls)**
- Procedure extraction: When outcome tracking records a successful workflow, use LLM to
  summarize the tool-call sequence into a reusable procedure entry
- Lesson extraction: When outcome tracking records a failure, use LLM to extract the lesson
  from the preceding tool-call sequence
- Entry enrichment: When multiple agents access the same entry and then search for related
  concepts, use LLM to add missing context to the entry (ACE delta-update pattern)
- Concept emergence: When new search terms appear that do not match existing entries, flag
  for automated extraction or creation

**Tier 3 — Active Organization (higher cost, transformative impact)**
- A-Mem-style evolution: New entries trigger re-evaluation and potential updates of related
  entries
- ACE-style Generator/Reflector/Curator pipeline for candidate knowledge
- Automatic taxonomy evolution based on usage patterns
- Cross-agent pattern synthesis: Identify that different agents solving similar problems
  converge on the same knowledge patterns — extract the common pattern

---

## 9. Recommended Architecture

### Passive Knowledge Acquisition Pipeline

```
  Agent Tool Calls
        |
        v
  [Signal Collector] ---- already exists (crt-001, col-001)
        |
        v
  [Signal Interpreter] ---- NEW: Tier 1
        |                    Detects patterns: gaps, clusters, co-access,
        |                    staleness, demand
        |
        +---> [Gap Report]         (knowledge needs)
        +---> [Demand Signals]     (topic priorities)
        +---> [Implicit Links]     (co-access relationships)
        +---> [Staleness Flags]    (candidates for deprecation)
        |
        v
  [Knowledge Extractor] ---- NEW: Tier 2
        |                     LLM-based extraction from traces
        |                     Triggered by: successful outcomes, failures,
        |                     recurring gaps
        |
        +---> [Candidate Entries]  (proposed knowledge)
        |
        v
  [Quality Gate] ---- extends crt-005
        |              Evaluates candidates against existing knowledge
        |              Checks for contradictions (crt-003)
        |              Checks for duplicates (vector similarity)
        |
        +---> [Accepted Entries]   (auto-stored with low initial confidence)
        +---> [Rejected Candidates] (logged for analysis)
        |
        v
  [Evolution Engine] ---- NEW: Tier 3
        |                  Delta-updates to existing entries
        |                  Re-linking based on new knowledge
        |                  Taxonomy evolution
        |
        v
  [Unimatrix Knowledge Base]
```

### Key Design Decisions to Make

1. **Batch vs. streaming**: Signal interpretation can run in batch (during maintenance) or
   streaming (per-request). Batch is simpler; streaming catches gaps faster.

2. **LLM cost management**: Tier 2 extraction requires LLM calls. Options: (a) run only
   during explicit maintenance cycles, (b) use lightweight local models for candidate
   screening and full models only for extraction, (c) rate-limit extraction to N entries
   per maintenance cycle.

3. **Confidence bootstrapping**: Auto-extracted entries should start with low confidence
   (e.g., 0.2) and earn confidence through subsequent access and helpfulness signals. This
   prevents low-quality auto-generated entries from polluting high-confidence manual entries.

4. **Human-in-the-loop option**: Auto-extracted entries could be flagged as "proposed" status
   (extending the current active/deprecated model) requiring human or senior-agent approval
   before becoming active. This adds a safety layer at the cost of automation.

5. **Attribution**: Auto-extracted entries should record their provenance (which signals,
   which tool-call sequences, which agents contributed to extraction) for auditability.

---

## Sources

### Signal-Based Knowledge Extraction
- [LEKG: Log Extraction Knowledge Graph Construction](https://dl.acm.org/doi/10.1145/3502223.3502250)
- [From LLMs to Knowledge Graphs: Production-Ready Systems in 2025](https://medium.com/@claudiubranzan/from-llms-to-knowledge-graphs-building-production-ready-graph-systems-in-2025-2b4aff1ec99a)
- [LLM-empowered Knowledge Graph Construction: A Survey](https://arxiv.org/abs/2510.20345)
- [KGGen: Extracting Knowledge Graphs from Plain Text (NeurIPS 2025)](https://arxiv.org/abs/2502.09956)
- [GraphRAG — Microsoft Research](https://www.microsoft.com/en-us/research/project/graphrag/)

### Implicit Feedback and Memory Systems
- [MemOS: A Memory OS for AI System](https://arxiv.org/abs/2507.03724)
- [MemOS GitHub Repository](https://github.com/MemTensor/MemOS)
- [MemoryOS: Personalized AI Agent Memory (EMNLP 2025 Oral)](https://github.com/BAI-LAB/MemoryOS)
- [How Memory Management Impacts LLM Agents (Xiong et al., 2025)](https://arxiv.org/abs/2505.16067)
- [Reinforcement Learning from Human Feedback Survey](https://arxiv.org/html/2504.12501v3)
- [Glean: Enterprise AI Search and Knowledge Discovery](https://www.glean.com)

### Autonomous Knowledge Base Construction
- [AutoSchemaKG: Autonomous KG Construction via Dynamic Schema Induction](https://arxiv.org/abs/2505.23628)
- [KARMA: Multi-Agent LLMs for KG Enrichment (NeurIPS 2025)](https://arxiv.org/abs/2502.06472)
- [Knowledge Base Construction from Pre-Trained Language Models (Workshop 2024)](https://lm-kbc.github.io/workshop2024/)
- [LLM-TEXT2KG 2025: 4th International Workshop](https://aiisc.ai/text2kg2025/)

### Query-Log Mining
- [LLM Query Mining: Extracting Insights from AI Search Questions](https://www.singlegrain.com/blog-posts/analytics/llm-query-mining-extracting-insights-from-ai-search-questions/)
- [Grounding LLMs for Database Exploration (VLDB 2025)](https://www.vldb.org/2025/Workshops/VLDB-Workshops-2025/AIDB/AIDB25_5.pdf)
- [Intent-Based Query Rewriting (VLDB 2025)](https://www.vldb.org/2025/Workshops/VLDB-Workshops-2025/DATAI/DATAI25_5.pdf)
- [Automated Mining of Structured Knowledge (KDD 2024)](https://dl.acm.org/doi/10.1145/3637528.3671469)

### Self-Organizing Knowledge Systems
- [A-Mem: Agentic Memory for LLM Agents (NeurIPS 2025)](https://arxiv.org/abs/2502.12110)
- [ACE: Agentic Context Engineering (Stanford/SambaNova/Berkeley, 2025)](https://arxiv.org/abs/2510.04618)
- [A Survey of Knowledge Organization Systems](https://arxiv.org/abs/2409.04432)

### Observation-Driven Multi-Agent Learning
- [LLM-Based Multi-Agent Systems for Software Engineering (ACM TOSEM)](https://dl.acm.org/doi/10.1145/3712003)
- [AgentTrace: Structured Logging for Agent System Observability](https://arxiv.org/abs/2602.10133)
- [Voyager: Open-Ended Embodied Agent with LLMs](https://arxiv.org/abs/2305.16291)
- [ToolMem: Learnable Tool Capability Memory](https://arxiv.org/abs/2510.06664)
- [ReflecTool: Reflection-Aware Tool-Augmented Clinical Agents](https://arxiv.org/abs/2410.17657)
- [Memory in LLM-based Multi-Agent Systems Survey](https://www.techrxiv.org/users/1007269/articles/1367390/master/file/data/LLM_MAS_Memory_Survey_preprint_/LLM_MAS_Memory_Survey_preprint_.pdf)

### Knowledge Distillation and Self-Evolution
- [Distilling LLM Agent into Small Models with Tools](https://arxiv.org/abs/2505.17612)
- [Self-Evolving Agents Survey: What, When, How, Where](https://arxiv.org/abs/2507.21046)
- [Comprehensive Survey of Self-Evolving AI Agents](https://arxiv.org/abs/2508.07407)
- [Self-Evolving Agents Cookbook — OpenAI](https://cookbook.openai.com/examples/partners/self_evolving_agents/autonomous_agent_retraining)
- [Mind the Metrics: Telemetry-Aware IDE Development via MCP](https://arxiv.org/html/2506.11019v1)
- [OpenTelemetry AI Agent Observability](https://opentelemetry.io/blog/2025/ai-agent-observability/)

### Agent Memory and Learning
- [Agent Memory: How to Build Agents that Learn and Remember (Letta)](https://www.letta.com/blog/agent-memory)
- [Memory for AI Agents: Context Engineering (The New Stack)](https://thenewstack.io/memory-for-ai-agents-a-new-paradigm-of-context-engineering/)
- [Self-Improving Data Agents (Powerdrill)](https://powerdrill.ai/blog/self-improving-data-agents)
- [Yohei Nakajima: Better Ways to Build Self-Improving AI Agents](https://yoheinakajima.com/better-ways-to-build-self-improving-ai-agents/)
