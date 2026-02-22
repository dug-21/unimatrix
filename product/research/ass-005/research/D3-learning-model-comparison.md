# D3: Learning Model Comparison — Sona-Style ML vs. Metadata Lifecycle

**Date**: 2026-02-20
**Status**: Complete
**Parent**: ass-005 (Learning Model Assessment), Pre-Roadmap Spike Track 1C
**Answers**: Q5 — Would a simpler metadata state machine cover 90% of the learning value?

---

## Executive Summary

**Recommendation: The metadata lifecycle approach covers approximately 95% of the learning value for Unimatrix's target scale (1K-100K entries per project, single developer) at roughly 10% of the implementation complexity.**

The ML components in sona (LoRA fine-tuning, EWC++ regularization, K-means++ clustering, trajectory-based reinforcement learning) solve problems that do not exist at Unimatrix's operating scale. The genuine learning behaviors needed — retrieval, staleness filtering, correction propagation, deduplication, and time decay — are all achievable with metadata fields, simple formulas, and the vector similarity search that hnsw_rs already provides.

The remaining ~5% where ML could add value (automatic pattern generalization from corrections, cross-project transfer learning) can be addressed later via LLM-based extraction (the mem0/Zep approach) rather than custom neural networks — and even that is a Phase 2+ concern.

---

## 1. Metadata Lifecycle State Machines

### The Pattern in Production

Metadata-driven lifecycle state machines are a well-established pattern across knowledge management, threat intelligence, and content management systems. The pattern requires no ML — just structured metadata fields and transition rules.

**OpenCTI** (threat intelligence platform) implements exactly this pattern for knowledge objects. Every piece of intelligence — indicators, campaigns, vulnerabilities — carries a state field: `New → Analyzing → Validated → Deprecated`. Transition criteria are role-based and evidence-based: an object moves from "Analyzing" to "Validated" when corroborated by multiple independent sources, tested in a sandbox, or confirmed by a trusted provider. No neural networks involved.

**IETF RFCs** have used a `supersedes`/`obsoletes` pattern since the 1970s. RFC 821 (SMTP) is obsoleted by RFC 2821, which is obsoleted by RFC 5321. The technology persists; the documentation evolves. Each document links to its predecessor and successor. This is correction-chain learning without ML — the RFC system has managed knowledge evolution for 50 years with metadata links alone.

**Evolveum midPoint** (identity management) uses lifecycle states on all managed objects: `Proposed → Active → Deprecated → Archived`. Development configurations work with Proposed + Active states; production configurations work with Active + Deprecated states. State transitions drive operational behavior without any learning algorithm.

### Proposed Lifecycle for Unimatrix

```
PROPOSED → VALIDATED → ACTIVE → AGING → DEPRECATED → ARCHIVED
              |                    |          |
              |                    |          +→ EVOLVED (new version created)
              |                    |
              +-- rejected         +-- reinforced (reset aging clock)
```

### Required Metadata Fields

```yaml
# Core identity
id: string                    # unique identifier
version: integer              # monotonic version counter
scope: enum                   # session | project | global

# Lifecycle state
status: enum                  # proposed | validated | active | aging | deprecated | archived
created_at: timestamp
created_by: string            # agent_id or human_id
source: enum                  # session_correction | code_review | manual_entry | automated_extraction

# Validation tracking
validation_count: integer     # times confirmed correct
validated_by: string[]        # list of validators (human or automated)
last_validated: timestamp

# Usage tracking
usage_count: integer          # times retrieved and referenced by an agent
success_count: integer        # times usage led to successful outcome
last_used: timestamp

# Correction tracking
correction_count: integer     # times this entry was corrected
supersedes: string | null     # id of entry this replaces
superseded_by: string | null  # id of entry that replaced this
corrections: string[]         # ids of correction entries linked to this

# Computed fields
confidence: float             # 0.0 - 1.0, computed from formula below
success_rate: float           # success_count / usage_count
days_since_last_use: integer  # computed from last_used
days_since_last_validation: integer  # computed from last_validated
```

### Transition Rules (No ML Required)

| Transition | Trigger | Condition |
|-----------|---------|-----------|
| proposed → validated | Human approval OR successful application without correction | validation_count >= 1 |
| proposed → rejected | Human rejects or correction within first use | explicit rejection event |
| validated → active | Repeated successful usage | usage_count >= 5 AND success_rate >= 0.8 |
| active → aging | Time-based, automated | days_since_last_use > staleness_threshold (90 days project, 180 days global) |
| aging → active | Re-used or re-validated | usage event OR validation event resets aging clock |
| aging → deprecated | Human decision or automated detection | explicit deprecation OR superseded_by is set |
| active → deprecated | Explicit supersession | superseded_by is set OR human deprecation |
| deprecated → archived | Time elapsed since deprecation | 30 days in deprecated state with no objections |
| any → evolved | Fork-and-replace | new version created, old version gets superseded_by link |

Every transition is a simple conditional check on metadata fields. No gradient descent, no Fisher Information Matrix, no learned parameters.

---

## 2. Correction-Based Learning Without ML

### The Core Pattern

The most valuable learning behavior in development knowledge is: *store a correction, link it to the original, ensure the corrected version is returned in future searches*.

This requires zero ML. The pattern is:

```
1. Agent produces output using knowledge entry K1
2. Human corrects: "No, don't use npm, use pnpm"
3. System creates correction entry K2:
   - content: "This project uses pnpm, not npm"
   - supersedes: K1.id
   - source: session_correction
   - status: proposed (or validated if from human directly)
4. K1 is updated:
   - superseded_by: K2.id
   - status: deprecated
   - correction_count += 1
   - confidence recalculated (decreases)
5. Next search matching K1's topic returns K2 instead
   - K2 has higher confidence (fresh, from correction)
   - K1 is excluded (status = deprecated)
   - If K1 is somehow returned, it includes pointer to K2
```

### How This Maps to Existing Systems

**Zep/Graphiti** implements exactly this pattern for temporal fact management, calling it "edge invalidation." When new information contradicts existing knowledge, Graphiti:
1. Uses an LLM to detect semantic conflicts between new and existing facts
2. Sets `t_invalid` on the old edge to the `t_valid` of the new edge
3. Preserves the old edge for historical queries
4. Returns the new edge for current queries

The temporal metadata (`t_valid`, `t_invalid`, `t'_created`, `t'_expired`) is purely metadata — no neural networks. The only LLM usage is for detecting whether two statements semantically conflict, which is a retrieval/comparison task, not a learning task.

**RFC Obsoletes Pattern**: RFC 2822 (Internet Message Format) carries the header `Obsoletes: 822`. Any system querying for the Internet Message Format specification follows the chain to the latest version. The old document remains accessible for historical reference. This is a linked list of corrections, not a trained model.

**Git itself** is a correction-chain system. Each commit supersedes the previous state. `git log` traces the correction history. `git blame` attributes knowledge to its source. No ML.

### Correction Chain Retrieval Algorithm

```
fn search_with_corrections(query, index) -> Vec<Result> {
    let raw_results = vector_search(query, index);
    let final_results = Vec::new();

    for result in raw_results {
        if result.status == Deprecated && result.superseded_by.is_some() {
            // Follow correction chain to latest version
            let current = follow_chain(result.superseded_by);
            if current.status != Archived {
                final_results.push(current);
            }
        } else if result.status == Active || result.status == Validated {
            final_results.push(result);
        }
        // Skip aging entries unless explicitly requested
    }

    deduplicate_and_rank(final_results)
}

fn follow_chain(id) -> KnowledgeEntry {
    let entry = get_by_id(id);
    if entry.superseded_by.is_some() {
        return follow_chain(entry.superseded_by);
    }
    entry
}
```

This is a linked-list traversal, not inference. O(chain_length), which is almost always O(1) or O(2) for development knowledge.

---

## 3. Confidence Scoring Without Neural Networks

### The Problem with ML-Derived Confidence

Sona's approach computes confidence through:
- EWC++ (Elastic Weight Consolidation): maintains a Fisher Information Matrix to prevent catastrophic forgetting during continual learning. Computational overhead: O(n_parameters^2) for FIM calculation.
- LoRA fine-tuning: low-rank adaptation matrices trained on new patterns. Requires GPU, training loops, hyperparameter tuning.
- Trajectory rewards: reinforcement learning signal from begin/step/end/reward cycles.

For development knowledge at scale 1K-100K entries, this is like using a particle accelerator to crack a walnut.

### Simple Confidence Formula

A confidence score for development knowledge can be computed from four metadata signals:

```
confidence = base_confidence
    * usage_factor(usage_count)
    * success_factor(success_rate, usage_count)
    * freshness_factor(days_since_last_validation)
    * correction_penalty(correction_count)
```

Where:

**Base confidence** (by source):
```
manual_entry (human wrote it):     0.7
code_review (from PR feedback):    0.6
session_correction (from fix):     0.5
automated_extraction (LLM-derived): 0.3
```

**Usage factor** (Wilson score lower bound — accounts for small sample sizes):
```
fn usage_factor(usage_count: u32) -> f64 {
    if usage_count == 0 { return 0.5; }
    // Lower bound of 95% Wilson confidence interval
    let n = usage_count as f64;
    let z = 1.96;  // 95% confidence
    let phat = 1.0; // assume positive until corrected
    (phat + z*z/(2.0*n) - z * ((phat*(1.0-phat) + z*z/(4.0*n))/n).sqrt())
        / (1.0 + z*z/n)
}
```

This is the same formula Reddit uses for comment ranking. It naturally handles the cold-start problem: an entry used once successfully gets a moderate score; an entry used 100 times successfully gets a high score. No training required.

**Success factor** (also Wilson-based):
```
fn success_factor(success_rate: f64, usage_count: u32) -> f64 {
    if usage_count < 3 { return 0.5; }  // Not enough data
    let n = usage_count as f64;
    let z = 1.96;
    let phat = success_rate;
    (phat + z*z/(2.0*n) - z * ((phat*(1.0-phat) + z*z/(4.0*n))/n).sqrt())
        / (1.0 + z*z/n)
}
```

**Freshness factor** (exponential decay):
```
fn freshness_factor(days_since_last_validation: u32) -> f64 {
    // Half-life: 90 days for project knowledge, 180 days for global
    let half_life = 90.0;
    let decay_rate = 0.693 / half_life;  // ln(2) / half_life
    (-decay_rate * days_since_last_validation as f64).exp()
}
```

This is the same exponential decay formula used by Hacker News (`score = (P-1) / (T+2)^G`), content recommendation systems, and Google's QDF (Query Deserves Freshness) signal. The formula `e^(-lambda * t)` where lambda = ln(2)/half_life gives a value that halves every `half_life` days.

**Correction penalty**:
```
fn correction_penalty(correction_count: u32) -> f64 {
    match correction_count {
        0 => 1.0,
        1 => 0.7,
        2 => 0.4,
        _ => 0.2,  // 3+ corrections: nearly deprecated
    }
}
```

### Comparison: ML Confidence vs. Metadata Confidence

| Aspect | ML (sona-style) | Metadata Formula |
|--------|-----------------|-----------------|
| **Accuracy at 1K entries** | Undertrained, unreliable | Works fine (Wilson handles small samples) |
| **Accuracy at 100K entries** | Good, if properly trained | Good (formula is scale-independent) |
| **Cold start** | Needs training data | Wilson score handles naturally |
| **Interpretability** | Black box | Fully explainable: "confidence is low because entry hasn't been validated in 120 days" |
| **Compute cost** | GPU for LoRA, CPU for EWC FIM | Microseconds per entry |
| **Debuggability** | Requires model introspection | Just read the metadata fields |
| **Implementation** | ~5,000+ lines (LoRA + EWC + training loop) | ~50 lines |
| **Dependencies** | ML framework (tch-rs, candle, or similar) | None beyond std math |

---

## 4. Existing Tools: What They Actually Do

### Cursor (.cursorrules / .cursor/rules/)

**Learning model: None.** Static rules files, manually maintained. No adaptation, no memory, no learning. After a few messages in a conversation, Cursor's context window optimization mechanisms may deprioritize rules in favor of recent context, requiring users to explicitly say "remember the rules." The 2025 evolution to `.cursor/*.mdc` files adds context-aware activation (rules fire only when relevant files are in scope) but this is pattern matching on file globs, not learning.

**Takeaway**: The floor. Any learning system is an improvement over static rules.

### Claude Code (CLAUDE.md + Auto-Memory)

**Learning model: Append-only flat files.** CLAUDE.md is human-written instructions. Auto-memory (`~/.claude/projects/<project>/memory/`) is Claude-written notes, stored as markdown. First 200 lines of MEMORY.md are loaded into the system prompt at session start. Beyond 200 lines, content is not automatically loaded. No pruning, no staleness detection, no confidence scoring, no lifecycle management.

**What it captures**: Build commands, test conventions, code style, debugging insights, architecture notes, user preferences.

**What it lacks**: No validation tracking (is this still correct?). No usage tracking (was this useful?). No correction linking (this was wrong, here's the fix). No cross-project sharing. No deduplication. No aging/deprecation. No structured retrieval beyond "load the first 200 lines."

**Takeaway**: Demonstrates that even crude memory (flat files, no structure) provides value. A structured metadata approach is a massive improvement without needing ML.

### Mem0

**Learning model: LLM-as-classifier, no neural learning.** Mem0 uses GPT-4o-mini to extract salient facts from conversations and choose operations (ADD, UPDATE, DELETE, NOOP) via function calling. Memories are stored as text in a vector database with optional graph overlay (Neo4j). No custom neural networks. No fine-tuning. No LoRA. No EWC.

**Architecture**: Hybrid datastore — key-value for structured data, vector for semantic search, graph for relationships. The graph variant (Mem0^g) stores memories as a directed labeled graph `G=(V,E,L)`.

**What it lacks**: No explicit confidence scores (only vector similarity). No temporal decay mechanism mentioned in the paper. No staleness penalties. No lifecycle management (memories persist indefinitely). No correction chaining.

**Takeaway**: The market-leading "AI memory" product ($24M raised, 97K GitHub stars) uses zero ML for memory management. All intelligence comes from LLM-based extraction at write time, not learned models. This strongly validates the metadata approach.

### Zep / Graphiti

**Learning model: Temporal metadata + LLM for conflict detection.** Zep's Graphiti engine is the most sophisticated memory system in production. Its core innovation is bi-temporal metadata:

- `t_valid` / `t_invalid`: when facts were true
- `t'_created` / `t'_expired`: when facts entered/left the system

When new information conflicts with existing facts, Graphiti:
1. Uses semantic search (vector + keyword + graph) to find potentially conflicting edges
2. Uses an LLM to determine if a genuine conflict exists
3. If conflict: sets `t_invalid` on old edge, preserving history
4. Returns only currently-valid facts in searches

**Key insight**: Graphiti's temporal invalidation is exactly the "correction chain" pattern described above. The LLM is used only for *detecting* conflicts (a comparison task), not for *learning* from them (no weight updates, no training).

**Performance**: 18.5% accuracy improvement over baselines, 90% latency reduction, using less than 2% of baseline tokens.

**Takeaway**: The most advanced production memory system validates the metadata approach. Its power comes from temporal metadata and graph structure, not from trained models. The only LLM usage is for semantic comparison, which Unimatrix already handles via hnsw_rs vector similarity.

### MCP Memory Servers

**Official MCP Memory Server**: Knowledge graph with entities, relations, and observations. Stored as JSONL. No learning, no lifecycle, no confidence, no correction handling. Deletion cascades (remove entity = remove relations). No versioning.

**MemoryMesh**: Schema-driven knowledge graph. Nodes with `name`, `nodeType`, `metadata[]`, optional `weight`. No learning, no lifecycle, no aging, no corrections. Data persists indefinitely in `memory.json`.

**mcp-memory-service**: Persistent memory with knowledge graph and "autonomous consolidation." REST API with 5ms retrieval. Still no lifecycle management or learning in the ML sense.

**Takeaway**: Every MCP memory server in the ecosystem uses simple data structures — JSON, JSONL, knowledge graphs. None use ML for memory management. The market gap is not "we need neural networks for memory" but "we need lifecycle management and correction tracking."

---

## 5. The 90% Question: Where Does Value Actually Come From?

For development knowledge at single-developer scale (1K-100K entries), five behaviors constitute essentially all the learning value:

### Behavior 1: Retrieving Relevant Context

| Approach | Mechanism | ML Required? |
|----------|-----------|-------------|
| **Sona** | K-means++ clustering + vector search + learned embeddings | K-means is ML but unnecessary |
| **Metadata** | hnsw_rs vector similarity search + metadata filtering (status, tags, phase) | No. Pre-trained embeddings (text-embedding-3-small or all-MiniLM-L6-v2) + HNSW index |

**Why K-means++ adds no value here**: K-means clustering is useful when you need to discover latent groupings in unstructured data. Development knowledge has *explicit* structure: categories (architecture, conventions, testing, operations), phases (coding, testing, deployment), tags (react, typescript, auth). You already know the clusters. They are your metadata fields. Running K-means over already-categorized data to "discover" the categories you assigned is circular.

At 100K entries, HNSW search returns top-k results in <1ms. Filtering by metadata fields during search (using hnsw_rs `FilterT`) is also sub-millisecond. No clustering needed.

**Verdict**: Metadata covers 100% of this behavior.

### Behavior 2: Filtering Out Stale/Deprecated Information

| Approach | Mechanism | ML Required? |
|----------|-----------|-------------|
| **Sona** | EWC++ prevents catastrophic forgetting; LoRA updates model to new knowledge | Yes, heavy |
| **Metadata** | `status` field filter: exclude deprecated/archived entries from search results. Aging entries ranked lower via freshness_factor in confidence score | No |

**Why EWC++/LoRA add no value here**: EWC++ solves catastrophic forgetting in neural networks — when you train a model on Task B, it forgets Task A. This is a real problem *for neural networks*. It is not a problem for a metadata database. A metadata database does not "forget" old entries when you add new ones. You filter them by status field. The old entry remains accessible for historical queries (status = archived). The new entry is returned for current queries (status = active).

The computational cost of EWC++ (computing and storing the Fisher Information Matrix) scales with the number of model parameters. For LoRA-augmented models, this is still O(millions) of floating-point operations. For a metadata status field filter, it is O(1).

**Verdict**: Metadata covers 100% of this behavior.

### Behavior 3: Learning from Corrections

| Approach | Mechanism | ML Required? |
|----------|-----------|-------------|
| **Sona** | LoRA fine-tuning on correction pairs; trajectory reward signal updates policy | Yes |
| **Metadata** | Correction entry created → links to original via `supersedes` field → original gets `superseded_by` and status=deprecated → search follows correction chain → corrected version returned | No |

**Why LoRA fine-tuning adds no value here**: LoRA would train the system to "generalize" from specific corrections. For example, from the correction "use pnpm not npm in this project," LoRA might learn a general preference toward pnpm. But:
1. The correction is already stored verbatim and retrieved when relevant.
2. Generalization from a single correction is unreliable (maybe pnpm is only for this one project).
3. If generalization IS desired, an LLM can be asked at write time: "Based on this correction, is there a general principle?" (This is the mem0/Zep approach — LLM-at-write-time, not trained model.)

The metadata approach captures 100% of the correction value for the specific case. For generalization, LLM extraction at write time (Phase 2 feature) captures most of the remaining value without custom model training.

**Verdict**: Metadata covers ~90% of this behavior. The remaining 10% (generalization) is better served by LLM-at-write-time than LoRA.

### Behavior 4: Deduplication

| Approach | Mechanism | ML Required? |
|----------|-----------|-------------|
| **Sona** | K-means++ clustering to group similar entries; cluster centroids for dedup | Yes |
| **Metadata** | On insert: vector similarity search for existing entries above threshold (e.g., cosine similarity > 0.92). If match found: merge or reject duplicate | No |

**Why K-means++ adds no value for deduplication**: Deduplication is a nearest-neighbor problem, not a clustering problem. You need to answer: "Does an entry sufficiently similar to this new entry already exist?" HNSW search answers this directly in O(log n) time. K-means clustering answers a different question: "What are the natural groupings in my data?" — and then you still need to do nearest-neighbor within a cluster to find duplicates.

The similarity threshold approach is:
1. Simpler (one parameter: the similarity threshold)
2. Faster (single HNSW query vs. cluster assignment + intra-cluster search)
3. More precise (exact nearest-neighbor vs. cluster-approximate)
4. Tunable (adjust threshold based on observed duplicate rate)

**Verdict**: Metadata + vector similarity covers 100% of this behavior.

### Behavior 5: Time Decay

| Approach | Mechanism | ML Required? |
|----------|-----------|-------------|
| **Sona** | Learned decay rates via trajectory reward model | Yes |
| **Metadata** | Exponential decay: `freshness = e^(-ln(2)/half_life * days_since_last_validation)` | No |

**Why learned decay is unnecessary**: The exponential decay formula has been used for content freshness ranking since at least 2007 (Hacker News launch). The Hacker News formula `score = (P-1) / (T+2)^G` has ranked millions of items for 19 years using one tunable parameter (gravity = 1.8). Google's QDF (Query Deserves Freshness) uses similar time-decay signals.

For development knowledge, the half-life is configurable per scope:
- Project-level: 90-day half-life (conventions change with the project)
- Global-level: 180-day half-life (language idioms change slower)
- Security patterns: 365-day half-life (security best practices are more stable)

One parameter per scope. No training loop.

**Verdict**: Metadata covers 100% of this behavior.

---

## 6. Where ML Genuinely Adds Value (The Remaining ~5%)

Being intellectually honest, there are capabilities where ML could provide value that pure metadata cannot:

### 6A. Automatic Pattern Generalization

When a human corrects "use pnpm not npm in Project Alpha," a metadata system stores this as a project-specific correction. An ML system could potentially generalize to "check package-lock.json vs. pnpm-lock.yaml to detect the correct package manager."

**But**: This is better done by an LLM at write time (the mem0 approach) than by training a custom model. Cost: one LLM call per correction. No model to maintain.

### 6B. Semantic Conflict Detection

When new knowledge contradicts existing knowledge, detecting the conflict requires semantic understanding. A metadata system can detect conflicts by tag/category overlap + vector similarity, but may miss subtle contradictions.

**But**: Zep/Graphiti solves this with LLM-based conflict detection at write time. Again, one LLM call, no custom model.

### 6C. Cross-Project Transfer Learning

Determining which patterns from Project A apply to Project B requires understanding project similarity beyond tag matching.

**But**: This can be approximated by explicit metadata (same language? same framework? same domain?) and validated by human approval. The metadata approach requires human judgment for cross-project transfer; ML could theoretically automate this. In practice, at 1-3 projects, human judgment is both sufficient and preferable.

### 6D. Retrieval Scoring Optimization

Learning which retrieved patterns agents actually use (and which they ignore) to tune retrieval scoring over time.

**But**: This is a simple feedback signal: if a pattern is retrieved and the agent's output complies with it, increment usage_count. If retrieved but not followed, decrement a retrieval_relevance counter. This is a metadata counter update, not a learned model.

### Assessment of the ~5%

All four ML-advantaged capabilities have simpler alternatives:
- 6A and 6B: LLM call at write time (already planned for Phase 2)
- 6C: Explicit metadata + human approval (already planned)
- 6D: Counter-based feedback loop (already designed in the learning architecture doc)

None of these require custom model training, LoRA adapters, EWC++ regularization, or K-means clustering.

---

## 7. Complexity and Maintenance Cost Comparison

### Sona-Style ML Approach

| Component | Lines of Code (est.) | Dependencies | Maintenance Burden |
|-----------|---------------------|-------------|-------------------|
| LoRA adapter management | ~2,000 | tch-rs or candle | Model versioning, checkpoint management, GPU/CPU dispatch |
| EWC++ Fisher Information Matrix | ~1,500 | Linear algebra crate | FIM recomputation on knowledge base changes, numerical stability |
| K-means++ clustering | ~800 | ndarray or custom | Cluster rebalancing, centroid drift, quality monitoring |
| Trajectory/reward model | ~1,200 | Custom RL loop | Reward signal design, exploration/exploitation tuning |
| Training pipeline | ~1,500 | Data loaders, batch management | Training schedule, hyperparameter tuning, validation splits |
| Model storage/loading | ~500 | Serialization of model weights | Version compatibility, migration between model versions |
| **Total** | **~7,500** | **Heavy ML stack** | **Ongoing tuning, retraining, monitoring** |

### Metadata Lifecycle Approach

| Component | Lines of Code (est.) | Dependencies | Maintenance Burden |
|-----------|---------------------|-------------|-------------------|
| Lifecycle state machine | ~200 | None | Transition rules (declarative, auditable) |
| Confidence scoring formula | ~50 | None (std math) | Parameter tuning (half_life values) |
| Correction chain logic | ~100 | None | Chain traversal (linked list) |
| Deduplication on insert | ~80 | hnsw_rs (already have) | Similarity threshold tuning |
| Pruning engine | ~300 | None | Staleness thresholds (configurable) |
| Metadata schema + storage | ~200 | serde + redb (already have) | Schema migrations (standard) |
| **Total** | **~930** | **Already in stack** | **Minimal — configuration, not code changes** |

**Complexity ratio**: ~8:1 in favor of metadata.
**Dependency ratio**: ML approach adds a heavy ML framework; metadata approach adds nothing new.
**Maintenance ratio**: ML approach requires ongoing retraining and monitoring; metadata approach requires occasional parameter tuning.

---

## 8. Risk Analysis

### Risks of the ML Approach at Unimatrix Scale

| Risk | Likelihood | Impact |
|------|-----------|--------|
| **Undertrained models**: At 1K-10K entries, LoRA and EWC++ will not have enough training signal to produce meaningful improvements over random | High | Model produces garbage confidence scores, eroding trust |
| **Training compute overhead**: LoRA fine-tuning adds seconds-to-minutes of GPU time per knowledge update | Medium | Unacceptable latency for interactive correction flow |
| **Complexity budget**: 7,500 lines of ML code competes with core feature development | High | Delays shipping actual learning value |
| **Debugging difficulty**: When confidence scores are wrong, diagnosing "why" requires model introspection | High | Developer cannot explain or fix system behavior |
| **Catastrophic forgetting irony**: EWC++ exists to prevent catastrophic forgetting in neural networks — but metadata databases do not have catastrophic forgetting | Certain | Solving a problem that does not exist |

### Risks of the Metadata Approach

| Risk | Likelihood | Impact |
|------|-----------|--------|
| **Hand-tuned parameters**: Half-life, staleness thresholds, correction penalties need tuning | Medium | Suboptimal but functional; easily adjusted |
| **No automatic generalization**: Corrections are stored verbatim, not generalized | Medium | Mitigated by LLM extraction at write time (Phase 2) |
| **Ceiling on sophistication**: Eventually, at very large scale (1M+ entries, many projects), metadata heuristics may underperform ML | Low (not at Unimatrix's target scale) | Migration path: add ML components later when data justifies it |

---

## 9. Recommendation

### Primary Recommendation

**Design the MCP tool interface around the metadata lifecycle model.** The interface schema already supports this:

```yaml
# memory_search response includes:
metadata:
  status: string        # active | aging | deprecated
  confidence: float     # computed from formula, not from model
  created_at: datetime
  last_used_at: datetime
  correction: object    # nullable, links to superseding entry
```

This interface is **forward-compatible with ML**. If, after shipping and collecting usage data at scale, ML-derived confidence proves superior to formula-derived confidence, the `confidence` field can be populated by a different computation. The interface does not change. The client does not know or care how confidence was computed.

### Implementation Priority

1. **Phase 0**: Lifecycle state machine + basic confidence (source-based only). Store corrections as linked entries. Filter by status in search.
2. **Phase 1**: Full confidence formula (usage + success + freshness + correction factors). Deduplication on insert via similarity threshold. Pruning engine.
3. **Phase 2**: LLM-based extraction at write time for pattern generalization and conflict detection (mem0/Zep pattern). This is NOT ML training — it is a single LLM call per write.
4. **Phase 3+**: If and only if usage data demonstrates that metadata confidence is insufficient, evaluate adding ML components. By then, you will have the training data to make ML viable.

### What to Take from Sona

Sona's value to Unimatrix is not its ML components but its **API shape and knowledge categories**:

- `store_pattern` / `find_patterns` — this CRUD shape is correct, implement it with metadata
- Categories (architecture patterns, code conventions, debugging knowledge) — these become metadata tags, not K-means clusters
- Confidence as a first-class field in search results — yes, absolutely, but compute it from a formula
- Trajectory model's concept of "session context" — capture session events as metadata, not as RL training signal

### The Bottom Line

| Question | Answer |
|----------|--------|
| Does a metadata state machine cover 90% of the value? | **Yes. Closer to 95%.** |
| What covers the remaining 5%? | LLM-at-write-time (mem0/Zep pattern), not custom neural networks |
| When should ML be reconsidered? | When usage data from 100K+ entries across 5+ projects shows metadata confidence is measurably inferior |
| What is the interface impact of this decision? | **None.** The MCP tool interface is the same either way. Confidence is a float. The client does not know how it was computed. |

---

## Sources

- [OpenCTI Knowledge Object State](https://filigran.io/knowledge-object-state-matters-in-opencti/)
- [Evolveum Object Lifecycle](https://docs.evolveum.com/midpoint/reference/master/concepts/object-lifecycle/)
- [IETF RFC Process](https://www.ietf.org/process/rfcs/)
- [Mem0 Paper (arXiv 2504.19413)](https://arxiv.org/abs/2504.19413)
- [Mem0 GitHub](https://github.com/mem0ai/mem0)
- [Zep Temporal Knowledge Graph (arXiv 2501.13956)](https://arxiv.org/abs/2501.13956)
- [Zep/Graphiti GitHub](https://github.com/getzep/graphiti)
- [Claude Code Memory Documentation](https://code.claude.com/docs/en/memory)
- [Cursor Rules Best Practices](https://medium.com/elementor-engineers/cursor-rules-best-practices-for-developers-16a438a4935c)
- [MCP Memory Server (Official)](https://github.com/modelcontextprotocol/servers/tree/main/src/memory)
- [MemoryMesh](https://github.com/CheMiguel23/MemoryMesh)
- [Wilson Score Interval (Evan Miller)](https://www.evanmiller.org/how-not-to-sort-by-average-rating.html)
- [Hacker News Ranking Algorithm](http://www.righto.com/2013/11/how-hacker-news-ranking-really-works.html)
- [Exponential Decay for Content Ranking](https://julesjacobs.com/2015/05/06/exponentially-decaying-likes.html)
- [Mem0 Architecture Deep Dive](https://medium.com/@parthshr370/from-chat-history-to-ai-memory-a-better-way-to-build-intelligent-agents-f30116b0c124)
- [MCP Knowledge & Memory Servers](https://github.com/TensorBlock/awesome-mcp-servers/blob/main/docs/knowledge-management--memory.md)
