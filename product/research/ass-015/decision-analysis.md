# ASS-015: Decision Analysis — Passive Knowledge Acquisition

## The Question

Should Unimatrix evolve from requiring explicit `context_store` calls to passively extracting knowledge from agent behavioral signals? Can this be done at a level that is GREAT, not merely good?

---

## Verdict: YES — Pursue This

The research strongly supports pursuing passive knowledge acquisition. The concept is feasible with current architecture, the competitive landscape is wide open, the academic foundations are solid, and Unimatrix already has 60-70% of the required infrastructure built.

The rest of this document explains why, what "GREAT" means, and how to get there.

---

## 1. Why This Works

### 1.1 The Signal Infrastructure Already Exists

Unimatrix captures signals from 7 distinct sources today:

| Source | Current Use | Passive Acquisition Use |
|--------|------------|------------------------|
| Hook events (Pre/Post ToolUse) | Observation records → hotspots | Tool sequence patterns → procedures |
| Usage tracking (crt-001) | access_count, last_accessed_at | Access cliffs → staleness detection |
| Co-access (crt-004) | Search re-ranking boost | File dependency graphs, concept clusters |
| Helpful/unhelpful (crt-002) | Wilson score → confidence | Signal quality feedback loop |
| Outcome tracking (col-001) | Feature success/rework tags | Outcome-to-knowledge correlation |
| Retrospective (col-002) | 21 detection rules → hotspots | Pattern extraction triggers |
| Confidence pipeline | 6-factor scoring | Quality gate for auto-entries |

Of 70 identified signals in the taxonomy, **26 (37%) are already tracked**. The remaining 44 operate on data already present in existing JSONL records and SQLite tables.

### 1.2 The Competitive Gap is Real and Wide

No production system combines all four of:
1. Passive behavioral signal capture (from tool calls, not conversations)
2. Cross-agent pattern detection (swarm-level, not single-agent)
3. Quality evolution (confidence scoring + contradictions + coherence)
4. Domain specialization (software development orchestration)

The closest competitors each address only a subset:

| System | Passive | Multi-Agent | Quality Evolution | Behavioral (not conversational) |
|--------|---------|-------------|-------------------|-------------------------------|
| Confucius (Meta/Harvard) | Yes | No | No | Yes |
| Mem0 ($24M Series A) | Semi | No | Conflict resolution only | No (conversation mining) |
| Letta sleep-time compute | Semi | No | No | Partially |
| claude-mem | Yes | No | No | Yes |
| Observability platforms | Capture only | Yes | No | Yes (but no extraction) |
| **Unimatrix (proposed)** | **Yes** | **Yes** | **Yes** | **Yes** |

**Key insight:** Conversation-mining is crowded (Mem0, Zep, LangMem). Behavioral-trace mining is open. Unimatrix should not compete on what agents *say* but on what agents *do*.

### 1.3 The Architecture Is Sound

The recommended architecture — **event-sourced hybrid, in-process** — builds directly on proven patterns already in the codebase:

- Signal capture → JSONL append (extends existing EventQueue/observation files)
- Rule-based extraction → detection rule pattern (extends unimatrix-observe's 21 rules)
- LLM extraction → async batch processing (extends fire-and-forget spawn_blocking)
- Quality gates → existing dedup + contradiction + confidence infrastructure
- Provenance → existing `trust_source` field on EntryRecord

The SQLite backend handles this cleanly: WAL mode supports concurrent reads during writes, and knowledge writes batch into single transactions (~10/hour marginal increase).

---

## 2. What "GREAT" Means

The research identified five dimensions that separate GREAT from merely good:

| Dimension | Target | Measurement |
|-----------|--------|-------------|
| **Precision** | < 10% false positive rate | (auto-entries deprecated unused) / (total auto-entries created) |
| **Coverage** | > 60% of knowledge agents would have stored if asked | Manual audit against expected knowledge per feature |
| **Freshness** | New knowledge within 1-2 feature cycles | Latency from pattern emergence to entry creation |
| **Relevance** | > 70% of auto-entries accessed within 10 features | (auto-entries with access_count > 0) / (total auto-entries) |
| **Trust** | Auto-entries used as confidently as explicit entries | Compare helpful_vote rate: auto vs explicit entries |

### What Makes It GREAT (Not Just Good)

Three design choices elevate this from "good auto-extraction" to "great knowledge fabric":

**1. LLM-in-the-loop for semantic understanding.** Rule-based extraction handles structural patterns (knowledge gaps, conventions, co-access). But only an LLM can synthesize: "The agent searched for 'auth error handling', found nothing, spent 45 minutes reading 12 files, then wrote a try-catch pattern that worked. This pattern is a reusable convention." The hybrid tier architecture (rules for 80% + LLM for 20%) is the key differentiator. Cost is negligible: ~$0.01-$2.00/day with Haiku batch processing.

**2. Cross-feature validation gates.** No entry promoted from a single observation. Structural conventions need 2+ features. Procedures need 3+. Phase profiles need 5+. This eliminates one-off noise. Combined with the confidence pipeline (entries start low, must earn their way up through access and votes), the system self-corrects.

**3. Closed-loop quality evolution.** Auto-extracted entries participate in the full confidence pipeline (Wilson score, co-access boosting, decay, contradiction detection). Bad extractions naturally decay. Good ones compound. No other system has this feedback loop on auto-generated knowledge.

---

## 3. The Three-Tier Architecture

```
Signal Buffer (JSONL, append-only)
       |
       v
  +──────────────────────────────────────────────+
  | Tier 1: Rule-Based (auto, immediate)          |
  | - Knowledge gap detection (zero-result queries)|
  | - Structural conventions (file path templates) |
  | - Implicit conventions (100% consistency)      |
  | - Dead knowledge flagging (access cliff)       |
  | Confidence: >= 0.6 → Active status             |
  | Source: uds:auto                               |
  +──────────────────────────────────────────────+
       |
       v (signals not handled by rules)
  +──────────────────────────────────────────────+
  | Tier 2: LLM Batch (periodic, async)           |
  | - Session narrative synthesis                  |
  | - Cross-session pattern detection              |
  | - Procedure extraction from success traces     |
  | - Lesson extraction from failure traces        |
  | Confidence: 0.4-0.6 → Proposed status          |
  | Source: uds:llm                                |
  +──────────────────────────────────────────────+
       |
       v (low-confidence extractions)
  +──────────────────────────────────────────────+
  | Tier 3: Human Review                          |
  | - Surfaced in context_status / review tool    |
  | - Novel domain entries                        |
  | Confidence: < 0.4 → Proposed + review_needed   |
  | Source: uds:propose                            |
  +──────────────────────────────────────────────+
       |
       v
  +──────────────────────────────────────────────+
  | Quality Gates                                 |
  | 1. Near-duplicate check (cosine >= 0.92)      |
  | 2. Contradiction check (crt-003)              |
  | 3. Content validation (min length, allowlist) |
  | 4. Rate limit (max 10 auto-extractions/hour)  |
  | 5. Confidence floor (< 0.2 → discard)         |
  +──────────────────────────────────────────────+
       |
       v
  Unimatrix Knowledge Base
  (confidence evolution handles the rest)
```

---

## 4. Implementation Priorities

### Priority 1: Immediate (High value, low risk)

| Extraction | Signals | Why First |
|------------|---------|-----------|
| Knowledge gaps | Zero-result context_search queries | Already in JSONL. Just count. Zero false positive risk. |
| Structural conventions | File creation paths per feature | File paths in JSONL. Template matching is regex. Verifiable against filesystem. |
| Implicit conventions | 100%-consistent patterns across features | Binary detection. Highest confidence of any extraction type. |
| Dead knowledge | Entries with access cliff | access_count + last_accessed_at already tracked. Pure threshold detection. |

### Priority 2: Next (High value, medium complexity)

| Extraction | Signals | Why Second |
|------------|---------|------------|
| File dependencies | Read-before-edit chains, co-access | Co-access tracking exists (crt-004). Extend from entries to files. |
| Gate failure lessons | Outcome signals + rework events | Gate events in JSONL. Failure reason needs parsing. |
| Recurring friction | Permission retries, sleep workarounds | 21 detection rules exist. Aggregate across features. |

### Priority 3: LLM Path (Medium value, needs API integration)

| Extraction | Signals | Why Third |
|------------|---------|-----------|
| Procedure extraction | Tool sequences from successful outcomes | Requires LLM to synthesize readable procedures from tool call logs. |
| Session narrative | Complete session history + outcome | Requires LLM to identify what went right/wrong and extract lessons. |
| Concept emergence | Novel search terms not matching entries | Requires LLM to understand why a new concept matters. |

### Priority 4: Defer (Research needed)

| Extraction | Why Defer |
|------------|-----------|
| Search-to-action correlation | Correlation-not-causation problem. Needs careful design. |
| Topic cluster detection | Requires sufficient query volume. Small projects may not accumulate enough. |
| Agent learning trajectory | Per-agent measurement needs agent identity improvements first. |

---

## 5. Risk Assessment

| Risk | Severity | Mitigation | Residual Risk |
|------|----------|------------|---------------|
| Knowledge pollution | HIGH | Cross-feature validation gates, rate limiting, confidence floor, auto-deprecation of unused entries | LOW |
| Echo chambers | MEDIUM | Descriptive-not-prescriptive entries, deviation flagging, novelty bonus for successful deviations | LOW |
| Feedback loops | MEDIUM | Source tagging, observation-only confidence window (5 features post-extraction), self-referential access monitoring | LOW |
| LLM hallucination | MEDIUM | Structured output, dedup check, Proposed status, human review tier | LOW |
| Storage bloat | MEDIUM | Category budgets, dedup at extraction, compaction during retrospective, bounded observation buffer (20 features) | LOW |
| Sensitive data capture | HIGH | Path-only extraction for content signals, sensitive path blocklist, no content snippets in entries, content review gate | LOW |
| API key dependency | LOW | Tier 1 works without any API key; Tier 2 degrades gracefully | NEGLIGIBLE |

The mitigations are strong because they layer on top of existing quality infrastructure (confidence pipeline, contradiction detection, coherence gate). The system is designed to be conservative: better to miss some knowledge than to pollute the knowledge base.

---

## 6. What Validates This from Research

### Academic Foundations

| Concept | Source | Application |
|---------|--------|-------------|
| ACE (Stanford/SambaNova/Berkeley) | Delta updates, grow-and-refine, execution feedback as implicit supervision | Entry evolution pattern for Unimatrix |
| A-Mem (NeurIPS 2025) | Zettelkasten-inspired dynamic memory where new knowledge reshapes old | Self-organizing knowledge model |
| Voyager (NVIDIA/Caltech/Stanford) | Skill library from execution traces | Procedure extraction from successful workflows |
| AgentTrace | Three-surface observability (operational, cognitive, contextual) | Signal taxonomy design |
| Self-Evolving Agents Surveys (2025-2026) | Non-parametric agent evolution (memory + tools, no retraining) | Validates knowledge evolution without model changes |
| Sleep-Time Compute (Letta) | Async idle-period processing, 5x compute reduction | Validates async extraction architecture |
| Confucius (Meta/Harvard) | Note-taking agent distills execution trajectories | Validates passive trace distillation hypothesis |

### Production Validation

| System | What It Proves |
|--------|---------------|
| Mem0 ($24M funding, 91% lower latency) | LLM-based extraction from interactions produces useful knowledge |
| Confucius (59% SWE-Bench-Pro) | Passive trace distillation improves agent performance |
| Glean ($7.2B valuation) | Behavioral signals (who accessed what, when) improve relevance |
| Observability platforms | Behavioral signal capture at scale is a solved infrastructure problem |

---

## 7. What This Means for Unimatrix's Roadmap

### Phase Alignment

| Phase | Existing | New with Passive Acquisition |
|-------|----------|------------------------------|
| Cortical (crt) | Usage tracking, confidence, co-access, contradiction, coherence | Signal interpretation, extraction rules, LLM extraction |
| Collective (col) | Outcome tracking, retrospective pipeline | Cross-feature validation, observation buffer, batch extraction triggers |
| Vinculum (vnc) | MCP server, 10 tools | New tools: `context_extract` (manual trigger), `context_review` (proposal review) |

### Estimated Scope

| Component | Lines of Code | Complexity |
|-----------|---------------|------------|
| Signal capture (JSONL events) | ~300 | Low |
| Rule-based extraction (5 rules) | ~500 | Medium |
| LLM extraction (client + prompts) | ~800 | Medium |
| Quality gates + integration | ~400 | Medium |
| **Total** | **~2,000** | **Medium** |

This is 2 feature cycles worth of implementation, roughly comparable to crt-001 + crt-002 combined.

---

## 8. Design Option: Self-Learning Neural Pipeline (ruv-fann + Continuous Retraining)

**Added after initial analysis.** See `self-learning-neural-design.md` for full details.

### The Concept

Replace the LLM extraction tier (Tier 2) with bundled purpose-built neural models via ruv-fann that continuously retrain from Unimatrix's own utilization signals. Fully self-contained — no external API dependency. The one exclusion: lesson extraction from failures remains agent-driven (requires deep causal reasoning).

### Why This Is Compelling

**unimatrix-adapt already proves the pattern.** The crt-006 MicroLoRA adaptation crate implements continuous self-retraining for embeddings using:
- Reservoir sampling (memory-bounded training buffer)
- EWC++ (catastrophic forgetting prevention)
- Fire-and-forget training (non-blocking, threshold-triggered)
- Contrastive learning from co-access pairs
- Versioned persistence with generation tracking

These exact mechanisms apply to extraction models.

### Five Models, ~87MB Total

| Model | Architecture | Size | Retrains Every | CPU Time |
|-------|-------------|------|----------------|----------|
| Signal Classifier | MLP: input→64→32→5(softmax) | ~5MB | 2-3 features | <5s |
| Duplicate Detector | Siamese MLP on 384-dim embeddings | ~10MB | 5-10 features | <10s |
| Convention Scorer | MLP: input→32→1(sigmoid) | ~2MB | 3-5 features | <2s |
| Pattern Merger | Encoder + merger MLP | ~50MB | 10 features | ~30s |
| Entry Writer Scorer | MLP quality scorer for templates | ~20MB | 5 features | ~30s |

### The Self-Learning Loop

```
Agents use Unimatrix → signals captured
    → rules pre-digest signals
    → neural models extract knowledge
    → entries stored with low initial confidence
    → agents interact with entries (helpful/unhelpful/access/ignore)
    → utilization signals become training labels
    → models retrain (fire-and-forget, background)
    → models improve → better extraction → better entries
    → (loop continues, system gets smarter with every feature)
```

### Training Labels Come Free

Unimatrix's existing quality signals ARE training labels:
- Helpful votes → positive label for classifier + writer
- Deprecated entries → negative label for whatever produced them
- Correction chains → ground truth for duplicate detector + classifier
- Access patterns → relevance signal for convention scorer
- Feature outcomes → weak labels for all models

### Self-Learning Timeline

| Maturity | State | Quality |
|----------|-------|---------|
| Features 1-5 | Observation only, models in shadow mode | Rules only |
| Features 6-10 | Models activated, low confidence | Rules + neural (improving) |
| Features 11-20 | Models calibrated, retraining from real feedback | Full pipeline (good) |
| Features 21-50 | Deeply domain-adapted, high precision | Full pipeline (great) |
| Features 50+ | Self-sustaining knowledge fabric | Autonomous |

### Advantage Over LLM Tier

| Dimension | Bundled Neural (ruv-fann) | LLM API (Claude Haiku) |
|-----------|--------------------------|----------------------|
| Dependency | None — fully self-contained | API key + network |
| Latency | Microseconds (classifier) to seconds (merger) | 1-5 seconds per batch |
| Cost | Zero marginal cost | ~$0.01-$2.00/day |
| Domain adaptation | Continuously retrains on YOUR data | Generic, never improves |
| Precision | Higher (specialized, trained on domain) | Lower (general purpose) |
| Coverage | Lower (can't handle novel patterns) | Higher (understands semantics) |
| Lesson extraction | Cannot do this | Can do this |

### Recommendation: Neural-First, LLM-Optional

1. **Default:** Rules (Tier 1) + Neural models (Tier 2) — fully self-contained
2. **Optional enhancement:** LLM API for lesson extraction + novel pattern discovery
3. **The system is GREAT without an API key, EXCEPTIONAL with one**

### Open Risk: ruv-fann Maturity

ruv-fann is v0.2.0 with ~4K downloads. Mitigation: if RPROP implementation proves insufficient, fall back to ndarray + hand-rolled training following unimatrix-adapt's proven approach (which already implements forward/backward passes, gradient computation, and weight updates in pure Rust with no ML framework dependency).

---

## 9. Decision Criteria Summary

| Criterion | Assessment | Evidence |
|-----------|------------|----------|
| Is it feasible? | YES | 37% of signals already tracked; architecture fits existing patterns |
| Can it be GREAT? | YES | Hybrid tiers + cross-feature validation + closed-loop quality evolution |
| Is the market gap real? | YES | No production system combines all four differentiators |
| Is the risk manageable? | YES | Every risk has a concrete mitigation layered on existing infrastructure |
| Does it align with Unimatrix's vision? | YES | "Self-learning context engine" — passive acquisition is the learning |
| Is the scope reasonable? | YES | ~2,000 lines across 4 phases; comparable to existing feature scope |
| Is now the right time? | YES | Infrastructure is built (crt-001–005, col-001–002); this is the natural next step |

---

## 10. Recommended Next Steps

1. **Scope a feature** — Create a feature cycle for passive knowledge acquisition (new Cortical phase feature)
2. **Start with Priority 1 rules** — Knowledge gaps, structural conventions, implicit conventions, dead knowledge detection. Rule-based, zero-risk, immediately useful.
3. **Build the observation buffer** — Fixed-size ring of per-feature signal observations. File-based for v1.
4. **Validate rules against historical data** — Run extraction rules against 20+ completed features to test precision before deploying live.
5. **Integrate ruv-fann** — Add as dependency, implement Signal Classifier and Convention Scorer (smallest models, fastest to validate)
6. **Shadow mode validation** — Run neural models in shadow for 5 features, compare against rule-only extraction
7. **Activate neural pipeline** — After shadow validation, promote models to production
8. **Enable continuous retraining** — Fire-and-forget retraining from utilization signals, following unimatrix-adapt patterns
9. **Optional: LLM tier** — Add API-based extraction for lesson learning + novel patterns if needed

---

## References

Full research in this directory:
- `existing-signals.md` — Inventory of current signal and observation infrastructure
- `novel-approaches.md` — State-of-the-art research (45+ papers and systems)
- `signal-taxonomy.md` — 70 signals, 10 extraction patterns, quality assessment
- `architecture-patterns.md` — 5 architecture options, storage analysis, recommended hybrid
- `competitive-landscape.md` — 12 competitor systems, comparative matrix, strategic positioning
- `self-retraining.md` — Continuous/online retraining patterns, drift detection, Rust ML frameworks
- `self-learning-neural-design.md` — Complete neural pipeline design with ruv-fann + continuous retraining
