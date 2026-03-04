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

Of 70 identified signals in the taxonomy, **26 (37%) are already tracked**. The remaining 44 operate on data already present in existing JSONL records and redb tables.

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

The redb single-writer constraint is handled cleanly: signals go to JSONL (no redb contention), knowledge writes batch into single transactions (~10/hour marginal increase).

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

## 8. Decision Criteria Summary

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

## 9. Recommended Next Steps

1. **Scope a feature** — Create a feature cycle (likely `crt-006` or a new phase prefix) for passive knowledge acquisition
2. **Start with Priority 1** — Knowledge gaps, structural conventions, implicit conventions, dead knowledge detection. These are rule-based, zero-risk, and immediately useful.
3. **Build the observation buffer** — Fixed-size ring of per-feature signal observations. File-based for v1, redb table for v2.
4. **Validate with retrospective data** — Unimatrix has 20+ completed features. Run the extraction rules against historical observation data to test precision before deploying live.
5. **Add LLM tier after rules prove out** — Only invest in the LLM extraction path after rule-based extraction demonstrates the value proposition.

---

## References

Full research in this directory:
- `existing-signals.md` — Inventory of current signal and observation infrastructure
- `novel-approaches.md` — State-of-the-art research (45+ papers and systems)
- `signal-taxonomy.md` — 70 signals, 10 extraction patterns, quality assessment
- `architecture-patterns.md` — 5 architecture options, redb analysis, recommended hybrid
- `competitive-landscape.md` — 12 competitor systems, comparative matrix, strategic positioning
