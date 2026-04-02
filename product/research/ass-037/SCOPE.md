# ASS-037: Retrieval Architecture Strategy — Signal Attribution and Graph Topology

**Date**: 2026-04-01 (revised post-ASS-036)
**Spike type**: Strategic empirical — measure, then decide
**Depends on**: ASS-035 FINDINGS.md, ASS-036 FINDINGS.md, W1-3 eval harness (operational), live DB snapshot
**Informed by**: ASS-035 (cosine validated, NLI task mismatch confirmed), ASS-036 (GGUF FAIL at deployable model sizes)

---

## Context

ASS-036 returned a FAIL verdict. GGUF is not the near-term mechanism for relationship
reasoning at deployable model sizes. This closes the "Scenario A" branch from the original
ASS-037 scope — there is no GGUF replacement path in the current planning horizon.

What remains is an honest question: **what retrieval architecture actually works for a
typed knowledge corpus, using the signals we have?**

The current pipeline is structurally broken at the top level:

```
HNSW(k=20, ef=32)
→ fused score: 0.35*nli(≈0) + 0.25*sim + 0.15*conf + 0.0*coac + 0.05*util + 0.05*prov
             + 0.02*phase_hist + 0.05*phase_explicit
→ PPR blend (α=0.85, 20 iter, blend=0.15): re-weight HNSW results + expand up to 50 candidates
→ graph_penalty (superseded/deprecated)
→ contradicts suppression
→ top-k
```

`w_nli=0.35` is the dominant weight and the eval confirmed zero measurable NLI contribution
(P@5 0.3060, MRR 0.4222, WA-0). The effective remaining weight is 0.57 split across sim,
conf, util, prov, and phase signals. Co-access moved off the formula into PPR topology
(w_coac=0.0; CoAccess edges in GRAPH_EDGES drive PPR traversal instead).

PPR is live (crt-030) and consuming the typed graph: CoAccess edges (bidirectional, crt-035),
Informs edges (2-4/tick, structural detection from crt-037), Supports edges (NLI-created, but
NLI entailment never fires — effectively zero), and Contradicts edges. PPR's contribution to
search quality has **never been measured**.

Tags are stored on every entry but contribute zero to any scoring or filtering path.

---

## Domain-Agnostic Framing

**This spike is not tuning the system for this corpus. It is asking which signals are
structurally sound for any typed knowledge corpus.**

A signal is structurally sound if:
- It is grounded in a general property of knowledge organization (semantic proximity,
  co-retrieval affinity, category topology), not in idiosyncrasies of this specific corpus
- It degrades gracefully when the specific signal is sparse (new corpus, low usage)
- Its failure mode does not introduce systematic noise that hurts a different corpus

This distinction matters because the same code will run on an SRE operations knowledge base,
an environmental monitoring corpus, or a legal compliance store. A signal that only works
because our corpus has a specific distribution of lesson-learned categories is not worth
building into the product.

The corpus in active use is the best available evidence. We use it to validate or refute
structural soundness — not to optimize for it.

---

## Central Question

**What is the contribution of each retrieval signal — semantic similarity, graph topology
(PPR with typed edges), and metadata dimensions (category, tags, confidence, phase) — to
actual search quality, and what is the optimal retrieval architecture for a typed knowledge
corpus before W3-1 (GNN)?**

---

## Core Hypothesis (Untested)

Informs edges encode category-to-category relevance relationships: a lesson-learned entry
*informs* a procedure entry not because their texts are similar, but because one category
of knowledge structurally informs another. If this topology is real and dense enough, PPR
traversal over Informs edges should surface relevant entries that HNSW misses — entries
that are categorically related but semantically distant.

This is the "category bridge" hypothesis: the typed graph adds a retrieval dimension that
embedding similarity cannot provide.

---

## Investigation Areas

### Q1 — Graph topology audit: is the current graph meaningful?

Before measuring PPR quality, establish the current state of the typed graph:

- How many GRAPH_EDGES rows exist by type (CoAccess, Informs, Supports, Contradicts, Supersedes)?
- What fraction of active entries participate in at least one non-CoAccess edge?
- What is the degree distribution for Informs edges specifically? Are they clustered
  on a few high-degree nodes, or distributed across the corpus?
- Is the graph connected enough for PPR to propagate meaningfully, or is it dominated
  by isolated CoAccess islands?

The structural answer to this question determines whether the PPR hypothesis is testable
at all given current edge density. If Informs edges number in the single digits, the
hypothesis cannot be tested until crt-037 has accumulated more.

**Instrument**: Direct SQL against analytics.db. No eval harness needed.

---

### Q2 — PPR contribution: does graph traversal add quality over HNSW alone?

Run the eval harness with two profiles against the current DB snapshot:

**Profile A (baseline)**: Current production config. PPR active, `blend_weight=0.15`,
`inclusion_threshold=0.05`, `max_expand=50`.

**Profile B (PPR disabled)**: `ppr_blend_weight=0.0`, `ppr_max_expand=0`. Pure HNSW +
fused formula, no graph expansion.

Measure: P@5, MRR, rank delta per scenario.

If Profile A = Profile B: PPR is not contributing. The graph topology is not shaping
results. The complexity is dead weight.

If Profile A > Profile B: PPR is contributing. Proceed to Q3 to isolate the source.

If Profile A < Profile B: PPR is hurting. The graph contains noise edges that pollute
candidate expansion. Diagnose which edge types are causing harm.

---

### Q3 — Informs edge contribution: does category topology drive PPR lift?

If Q2 shows PPR contributing, isolate the Informs edge contribution specifically.

This requires a modified eval profile that excludes Informs edges from the PPR graph
while keeping CoAccess edges. The eval harness profile system supports per-run
graph configuration changes if the graph can be rebuilt from a filtered edge set —
verify this is achievable before committing to this sub-question.

**Profile C**: PPR active, CoAccess edges only (Informs excluded from graph rebuild).
**Profile D**: PPR active, all edge types (Informs + CoAccess + Supports + Contradicts).

Delta D-C measures Informs edge contribution.

Domain-agnostic interpretation: if Informs edges contribute measurable lift, it means
category topology is a real signal — not just in this corpus, but in any corpus where
the category taxonomy encodes epistemic relationships (which is an explicit design
constraint for Unimatrix's configurable category system).

---

### Q4 — Signal ablation: which fused formula terms are load-bearing?

Run the eval harness with each formula signal zeroed individually, against Profile A
(current config) as baseline.

| Ablation | Config change | Hypothesis |
|----------|--------------|------------|
| NLI removed | `w_nli=0.0`, weights renormalized | ~0% impact (eval confirmed nothing) |
| Confidence removed | `w_conf=0.0` | Unknown — confidence is a composite of 6 factors |
| Utility removed | `w_util=0.0` | Small signal, rarely non-neutral |
| Provenance removed | `w_prov=0.0` | Small signal, binary in practice |
| Phase histogram removed | `w_phase_histogram=0.0` | WA-2 signal, unknown contribution |
| Phase explicit removed | `w_phase_explicit=0.0` | col-031 signal, unknown contribution |

For each: report P@5/MRR delta vs. baseline. Any signal that shows < ±0.5% P@5 delta
when zeroed is not contributing at current weight.

Domain-agnostic interpretation: a signal is load-bearing if it provides information
that cosine similarity does not already capture. Confidence, for example, encodes
usage history — which is orthogonal to semantic content. Phase histogram encodes
session context — orthogonal to entry content.

---

### Q5 — Tags: unused signal or non-viable dimension?

Tags are stored on every entry but contribute zero to any scoring path. This has never
been evaluated.

The structural case for tags:
- Tags are human-specified categorical annotations, not computed signals
- They encode semantic grouping that embeddings may not capture (a "migration"
  tag groups DB migration entries across different feature cycles and phrasings)
- Tag overlap between query-context and entry tags could be a filter or boost signal

Scope-limited evaluation:
- Audit: what is the current tag distribution? How many active entries have ≥1 tag?
  What is the vocabulary size?
- Structural assessment: are tags semantically redundant with category labels
  (same information, just more specific), or do they encode orthogonal grouping?
- Feasibility: tags on entries but not on queries — what would a "query tag" be?
  The scenario where tags are useful as a FILTER requires either: (a) user-specified
  tags in the search request, or (b) tag extraction from the query string.

If tags are sparse (< 30% of active entries have tags) or redundant with categories,
this dimension is not viable and should be documented as such. Do not design an
implementation — this is a feasibility assessment only.

---

### Q6 — Formula redesign: what is the optimal pre-GNN scoring formula?

Based on Q1-Q5 findings, propose a revised `[inference]` config section that:

1. Sets `w_nli` to 0.0 (or removes it from the sum entirely if the NLI model will
   be removed from search re-ranking; preserve only for eval harness backward compat)
2. Redistributes the reclaimed weight (0.35) to validated signals
3. Tests 2-3 formula variants on the eval harness and reports P@5/MRR
4. Selects the best-performing variant as the recommended config

Domain-agnostic constraint: the formula must make sense for a corpus that has no
usage history yet (new deployment, all signals sparse). The cold-start formula should
be sim-dominant, with other signals adding signal only when present.

Candidate redistribution targets:
- Increase w_sim (cosine is the most validated signal)
- Increase PPR blend_weight (if Q2-Q3 confirm graph contribution)
- Increase w_conf (if Q4 shows confidence is load-bearing)
- Maintain or adjust phase signals based on Q4 findings

The recommended formula becomes W3-1's cold-start initialization. Getting it right
reduces the amount of data W3-1 needs to improve from cold-start.

---

### Q7 — NLI infrastructure audit (original ASS-037 Sections 1-4)

Now that GGUF fails and Scenario A is off the table, enumerate every NLI use in the
codebase and produce a keep/remove/stub verdict for each.

Expected uses to audit (verify completeness — do not assume this list is final):
- Search re-ranking: `w_nli` in fused score (eval: zero contribution → remove)
- Post-store NLI: neighbor scoring after `context_store` (Supports/Contradicts edges)
- `nli_detection_tick`: Supports candidate scoring (entailment) → replacing with cosine
- `nli_detection_tick`: Informs candidate scoring (neutral zone filter or entailment?)
- `nli_detection_tick`: Contradicts edge detection (contradiction score — still working)
- Auto-quarantine guard: NLI-origin Contradicts edge threshold check
- Graph edge source attribution: NLI-sourced edges vs. cosine-sourced edges

For each use, characterize:
- What task (entailment / contradiction / neutral)?
- Measured quality at that task (draw on tick logs, ASS-035, eval data)?
- What breaks or degrades if removed?
- Verdict: **keep** (Contradicts path — still valid), **remove** (w_nli in search),
  **replace with cosine** (Supports detection), **stub for future** (GGUF interface point)

The Informs NLI dependency (neutral zone filter): determine whether the 2-4 Informs
edges/tick are driven by the structural detection or by the NLI neutral score. If
structural detection is doing all the work, the NLI call in the Informs path is
removable without precision loss.

---

### Q8 — Tick architecture: decomposition for mixed-mechanism detection

The `nli_detection_tick` is monolithic: one function handling Supports, Informs, and
Contradicts candidates, all gated by `nli_handle.get_provider()`. This creates a
correctness problem: when Supports moves to cosine detection, the whole tick should
not be a no-op when the NLI model is unavailable.

Design the decomposed architecture:
- `cosine_supports_tick`: HNSW cosine ≥ 0.65, compatible category pairs, no NLI.
  Runs regardless of NLI model availability.
- `contradiction_tick`: NLI contradiction score. Runs only when NLI model is present.
  Informs detection folded here or kept separate based on Q7 finding.
- Ordering invariants: compaction → promotion → graph-rebuild → detection ticks.
  Cosine supports tick can run every tick (near-instant); contradiction tick runs
  every N ticks per its existing budget.

Produce a concrete tick composition table: tick name, mechanism, edge type(s) written,
NLI dependency (yes/no), frequency.

---

## What Passes, What Fails

This spike does not have a binary pass/fail verdict. It has four decision outputs:

**D1 — PPR verdict** (from Q2-Q3):
- CONFIRMED: graph traversal produces measurable P@5 lift (≥1%) over HNSW alone
- NEUTRAL: < ±0.5% delta — PPR is overhead-equivalent to HNSW alone
- HARMFUL: PPR decreases P@5 — graph edges are introducing noise

**D2 — Informs hypothesis** (from Q3):
- CONFIRMED: Informs edges contribute measurable lift independent of CoAccess
- NOT CONFIRMED: delta indistinguishable from CoAccess-only PPR
- UNTESTABLE: insufficient Informs edge density for meaningful measurement

**D3 — Signal attribution** (from Q4):
- For each signal: LOAD-BEARING (≥0.5% delta when zeroed) or NOT LOAD-BEARING

**D4 — Tags** (from Q5):
- VIABLE: sufficient density and orthogonal to categories → design in ASS-038
- NOT VIABLE: sparse or redundant → document as non-dimension, close the question

---

## Output

1. **Graph topology report** — edge counts by type, degree distribution, connectivity
   assessment (Q1)

2. **PPR contribution table** — Profile A vs B eval results with P@5/MRR delta (Q2)

3. **Informs hypothesis verdict** — Profile C vs D delta, with interpretation (Q3)

4. **Signal ablation table** — per-signal P@5/MRR delta, structural soundness
   assessment for each (Q4)

5. **Tags viability assessment** — distribution audit and structural feasibility
   verdict (Q5)

6. **Recommended formula** — config section with empirical backing, cold-start
   rationale, W3-1 initialization implications (Q6)

7. **NLI cleanup table** — per-use keep/remove/replace verdict (Q7)

8. **Tick composition table** — decomposed architecture with ordering invariants (Q8)

9. **Delivery sequence** — what ships regardless of PPR verdict (cosine Supports
   delivery, NLI search weight removal) vs. what is gated on PPR findings

---

## Constraints

- **Use the corpus as evidence, not as the target.** Every finding must include a
  domain-agnostic interpretation: would this hold for a different typed knowledge
  corpus? Findings that are corpus-specific should be labeled as such and not
  incorporated into the formula.

- **Eval harness is the measurement tool.** No conclusions about search quality from
  manual inspection or reasoning alone. Ablation results must come from the W1-3
  harness running against a current snapshot.

- **Verify snapshot currency before drawing P@5 conclusions.** The snapshot may be
  stale. Check when it was taken relative to recent feature deliveries that may have
  changed the knowledge base structure. If stale, retake before running ablations.

- **Q3 feasibility gate.** Determining Informs edge contribution (Q3) requires the
  eval harness to support filtered graph configurations. Verify this is achievable
  before designing Q3 experiments in detail — if it requires harness changes, scope
  the change explicitly.

- **Do not change the contradiction detection path.** The NLI contradiction score is
  still the best available signal for Contradicts edges. The cleanup table (Q7) should
  distinguish between what is removed vs. what is preserved for contradiction detection.

- **No production app changes.** All formula variant testing is via eval harness
  profile configs. The recommended formula ships via a config change, not a code change
  (the config system supports full formula override via `[inference]` section).

- **The delivery sequence is an output, not an input.** Do not presuppose what ships
  first. Let the evidence (PPR verdict, signal attribution) determine the sequence.

---

## What This Is Not

This spike does not implement cosine Supports detection (scoped from ASS-035 findings).
It does not design W3-1 architecture (that requires ASS-029). It does not retire the NLI
model from production (the NLI audit produces the recommended cleanup, not the cleanup
itself). It does not evaluate GGUF further — ASS-036 is closed.

It answers one question per investigation area, produces the evidence, and delivers the
empirically grounded architecture recommendation for the pre-GNN retrieval pipeline.
