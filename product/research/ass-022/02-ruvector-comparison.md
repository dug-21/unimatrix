# ASS-022/02: Unimatrix vs. RuVector — Architecture Comparison

**Date**: 2026-03-16
**Type**: Competitive / architectural analysis
**Reference**: github.com/ruvnet/ruvector

---

## 1. What RuVector Actually Is

RuVector describes itself as a "self-learning vector database" — but that undersells and misdirects. It is closer to **a complete ML infrastructure platform** built around vector operations. Its capabilities extend well beyond storage and retrieval:

- **Self-learning search**: A Graph Neural Network layer sits atop HNSW, analyzing query patterns and user feedback to continuously reweight results. Updates happen in under 1ms.
- **Local LLM inference**: Full GGUF model support (Metal, CUDA, WebGPU acceleration) — RuVector can *run models*, not just *call them*.
- **Graph database**: Native Cypher query support with hyperedge modeling. Relationships aren't approximated by co-access similarity — they are first-class graph objects.
- **Distributed systems**: Raft consensus, multi-master replication, auto-sharding. RuVector scales horizontally.
- **Edge deployment**: WASM target (browser, IoT, bare metal). A "Cognitive Container" (RVF file format) bundles vectors + model + a bootable Linux microkernel into a single deployable unit that starts in 125ms.
- **PostgreSQL compatibility**: 230+ SQL functions — drop-in pgvector replacement with more features.
- **46 attention mechanisms**: Flash, linear, graph, hyperbolic variants — an ML research sandbox baked into the database.

RuVector is positioning as "the entire ML infrastructure stack in one system."

---

## 2. What Unimatrix Actually Is

Unimatrix describes itself as a "self-learning context engine" — which is also underselling, but in the opposite direction. It is closer to **a knowledge integrity and lifecycle system** that happens to use semantic search as its retrieval mechanism.

Its defining capabilities are not about search performance or scale:

- **Correction chains with cryptographic provenance**: SHA-256 hash per entry, `previous_hash` linking entries into a tamper-evident chain. Every knowledge mutation is traceable.
- **Confidence evolution from real signals**: Six-factor composite (usage, freshness, helpfulness, correction quality, trust, base) that changes as knowledge is used, corrected, and validated.
- **Contradiction detection**: When two knowledge entries conflict, the system detects and surfaces the conflict — not just flags it, but begins decay on the lower-confidence entry.
- **Trust-tiered access control**: Four trust levels (System > Privileged > Internal > Restricted), four capabilities (Read/Write/Search/Admin), immutable audit log.
- **Knowledge lifecycle management**: Active/Deprecated/Proposed/Quarantined states with state restoration, quarantine isolation from search, and structured deprecation reasons.
- **Process intelligence**: The retrospective pipeline observes *how* agents interact with knowledge and surfaces anti-patterns in the workflow itself — not just the knowledge.
- **MCP-native delivery**: 12 tools purpose-built for AI agent consumption. Hook-driven injection means knowledge reaches agents *without the agent asking for it*.

Unimatrix is positioning as "what agents remember should be trustworthy, correctable, and auditable."

---

## 3. Where They Overlap

Both systems share a philosophical orientation: static retrieval is insufficient, systems should learn from usage.

| Capability | RuVector | Unimatrix |
|-----------|---------|-----------|
| HNSW vector index | ✅ (core) | ✅ (core) |
| Semantic similarity search | ✅ | ✅ |
| Local ONNX inference | ✅ (also runs GGUF) | ✅ (ONNX only) |
| Learning from query patterns | ✅ (GNN layer, <1ms) | ✅ (confidence evolution, co-access boosting) |
| Knowledge versioning | ✅ (implicit, graph-level) | ✅ (explicit, correction chains) |
| Local-first deployment | ✅ (edge, WASM, RVF) | ✅ (single binary, embedded SQLite) |
| AI agent integration | ✅ (various protocols) | ✅ (MCP-native, 12 tools) |
| Trust / access control | ✅ (mentioned, not detailed) | ✅ (4-tier RBAC, audit log) |
| No cloud dependency | ✅ | ✅ |

---

## 4. Where They Diverge

This is where the comparison gets interesting. The overlap above is real but shallow — the design philosophies are fundamentally different.

### 4.1 Scale vs. Integrity

**RuVector** is built for **scale and performance**:
- Distributed (Raft, multi-master, auto-sharding)
- Sublinear solvers achieving O(log n) complexity
- 46 attention mechanisms — research-grade performance optimization
- PostgreSQL compatibility for existing high-throughput workloads
- 1M+ element vector indices as baseline

**Unimatrix** is built for **trust and lifecycle**:
- Single-node, embedded, zero-config (not distributed by design)
- ~1M element vector index, no sharding
- 12 purpose-built MCP tools, not a general query layer
- Cryptographic hash chains — integrity, not just versioning
- Knowledge *lifecycle* (Active/Deprecated/Proposed/Quarantined) as a first-class concept

If you are storing sensor readings from 10,000 IoT devices, RuVector wins. If you are managing the *knowledge about* those sensor readings (what anomaly patterns mean, which calibration procedures work, what regulations apply), Unimatrix wins.

### 4.2 Graph Primitives vs. Co-access Inference

**RuVector** has native graph support: Cypher queries, hyperedges, explicit relationship modeling. You define relationships; the graph stores them precisely.

**Unimatrix** infers relationships from usage: co-access pairs emerge when agents retrieve entries together. No explicit relationship definition required. Relationships are discovered, not declared.

These are not equivalent. For domains where relationships are **known** (protein interaction networks, legal citation graphs, organizational hierarchies), RuVector's explicit graph is more powerful. For domains where relationships are **emergent** (what knowledge tends to cluster together in practice, what conventions are actually applied together), Unimatrix's co-access graph surfaces things that nobody explicitly modeled.

### 4.3 Model Execution vs. Model Integration

**RuVector** runs models: full GGUF LLM inference, 46 attention mechanisms, GNN layers trained inside the database. It is partly an ML training and inference runtime.

**Unimatrix** integrates with models: ONNX for embedding (inference only, no training), external LLM via MCP protocol, MicroLoRA adaptation layer (crt-adapt, via unimatrix-adapt). Unimatrix does not run LLMs — it provides structured knowledge context *to* LLMs that live elsewhere.

This is a design philosophy split. RuVector wants to be the inference platform. Unimatrix wants to be the memory layer for inference platforms that already exist.

### 4.4 Correction Chains vs. Implicit Versioning

**RuVector** has versioning at the graph level — vectors and nodes can be updated, and presumably history is tracked. But there is no public API showing correction chain semantics: "entry A was wrong; entry B corrects it; here is A's content, B's content, and the reason for the change."

**Unimatrix** makes correction chains explicit and cryptographically verifiable:
- `context_correct(original_id, content, reason)` creates a new entry, deprecates the original, links them via `supersedes`/`superseded_by`, and chains the SHA-256 hashes.
- Every correction is audited (actor, timestamp, operation).
- Confidence of the corrected entry starts fresh; the correction count affects the score.
- The chain is traversable: "show me the full history of this knowledge, including why it changed."

For regulated domains (pharma, legal, medical devices, compliance), this distinction is decisive. A court does not care that your vector database "versioned" a document. It cares *who changed it, when, and what the original said*.

### 4.5 MCP Depth vs. Protocol Breadth

**RuVector** claims AI agent integration but does not have a defined MCP implementation with specific tools. The integration appears to be via PostgreSQL compatibility and LLM-in-database patterns.

**Unimatrix** is MCP-native: 12 tools with well-defined parameter schemas, behavioral driving via server instructions, hook-driven automatic injection (agents receive context they didn't ask for), and role-specific briefings. The MCP server is the primary interface, not an adapter.

---

## 5. The Complementary Angle

The most interesting strategic insight is that **RuVector and Unimatrix are not head-to-head competitors — they are complementary layers in an architecture**.

```
┌─────────────────────────────────────────────────────┐
│                   AI Agents / LLMs                  │
└─────────────────────┬───────────────┬───────────────┘
                      │               │
           MCP (12 tools)        Cypher/SQL
                      │               │
         ┌────────────▼───┐   ┌───────▼──────────┐
         │   Unimatrix    │   │    RuVector       │
         │  Knowledge     │◄──│  Vector/Graph     │
         │  Lifecycle     │   │  Performance DB   │
         │  Engine        │   │                   │
         │                │   │  - High throughput│
         │  - Trust       │   │  - Distribution   │
         │  - Correction  │   │  - LLM inference  │
         │  - Audit       │   │  - Graph queries  │
         │  - Lifecycle   │   │  - Scale          │
         └────────────────┘   └───────────────────┘
```

**Use pattern**: RuVector handles the high-volume, high-performance retrieval of raw vectors (sensor readings, document embeddings, real-time signals). Unimatrix manages the curated, trustworthy knowledge *about* that data — the interpretations, the calibration notes, the anomaly patterns that have been validated and corrected over time.

A concrete example in environmental monitoring:
- RuVector: stores 6 months of PM2.5 readings from 500 sensors, handles real-time similarity queries ("find readings similar to the current spike")
- Unimatrix: stores what those similar patterns meant in the past, which regulatory thresholds they violated, which calibration procedures were applied, which source attributions were confirmed — with full correction chains and trust attribution

The agent querying both systems gets: *raw pattern match* (RuVector) + *curated knowledge about what that pattern means* (Unimatrix).

---

## 6. When to Choose Each

| Scenario | RuVector | Unimatrix | Why |
|----------|---------|-----------|-----|
| High-volume time-series retrieval | ✅ | ❌ | Scale, sharding, throughput |
| Multi-node distributed knowledge base | ✅ | ❌ | Raft, multi-master |
| Local LLM inference integrated with search | ✅ | ❌ | GGUF model execution |
| Explicit relationship graphs (citation networks, org charts) | ✅ | ❌ | Cypher, hyperedges |
| Knowledge with audit trail requirements | ❌ | ✅ | Cryptographic integrity, immutable log |
| Knowledge that evolves and gets corrected | ❌ | ✅ | Correction chains with provenance |
| Trust-tiered multi-actor knowledge systems | ❌ | ✅ | RBAC, actor attribution |
| Contradiction detection in knowledge base | ❌ | ✅ | Active contradiction scanning |
| AI agent briefing (role-specific context assembly) | ❌ | ✅ | context_briefing, hook injection |
| Knowledge that decays based on usage and time | ❌ | ✅ | Confidence evolution |
| Zero-config single-binary deployment | ❌ | ✅ | Embedded SQLite, one binary |
| Regulated domain compliance (pharma, legal, medical) | ❌ | ✅ | Hash chains, audit, lifecycle |
| Process intelligence (learn from *how* agents work) | ❌ | ✅ | Observation pipeline, retrospective |

---

## 7. What Unimatrix Can Learn from RuVector

1. **Graph primitives**: RuVector's explicit relationship modeling would complement Unimatrix's inferred co-access graph. A future `unimatrix-graph` integration with petgraph (already on the roadmap) would close this gap.

2. **Edge deployment packaging**: RuVector's "Cognitive Container" (RVF) bundling everything into a bootable microkernel is compelling for constrained environments. Unimatrix's single-binary model is good; a container-native packaging would extend reach.

3. **WASM target**: Unimatrix currently has no browser or IoT deployment path. RuVector's WASM support opens entirely new deployment contexts.

4. **Performance at scale**: RuVector's sublinear solvers and horizontal scaling are out of Unimatrix's current scope. If the use case grows beyond single-node, Unimatrix needs a distributed story.

5. **GNN-based query learning**: RuVector's GNN layer that adapts search weights from query patterns is more sophisticated than Unimatrix's current co-access boosting (which is simpler, additive, not graph-neural). This is a potential crt-* feature direction.

---

## 8. Summary

RuVector is a **high-performance ML infrastructure platform** with vector search as its core primitive and scale, inference, and graph capabilities as its differentiators.

Unimatrix is a **knowledge integrity and lifecycle engine** with semantic search as its retrieval mechanism and trust, correction chains, and confidence evolution as its differentiators.

The right framing: RuVector is where you put *data at scale*. Unimatrix is where you put *knowledge that matters*. They occupy adjacent layers of a sophisticated AI data architecture, and the most interesting deployments will use both.
