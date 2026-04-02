# ASS-038: Multi-Signal Edge Generation — Graph Density and PPR Validation

**Date**: 2026-04-01  
**Spike type**: Empirical — generate, measure, decide  
**Depends on**: ASS-037 FINDINGS.md, ASS-037 snapshot.db, ASS-037 harness infrastructure  
**Informed by**: ASS-037 (PPR zero-effect due to sparsity, not architecture), ASS-035 (cosine validated)

---

## Context

ASS-037 reached a definitive conclusion: PPR is zero-effect today. The root cause is not the
architecture — it is signal starvation. The graph has one real edge type (CoAccess), 85
partially-blocked Informs edges, and nothing else. PPR cannot propagate cross-category signals
over a graph this thin.

The hypothesis for this spike: **a graph populated from multiple weak, noisy signal sources
will produce a topology dense enough for PPR to demonstrate measurable retrieval improvement.**
We do not need clean edges. We need enough edges. Noise is filtered by PPR score propagation
and, ultimately, by GNN weighting. The noise-filtering problem is preferable to the
signal-starvation problem.

The secondary goal: produce a labeled edge set (edge type + signal origin) that serves as
training data specification for W3-1 (GNN). The GNN learns which signal sources are
predictive of useful knowledge relationships — we do not hand-tune weights.

**All work is harness-only. No production app changes. Move fast.**

---

## Central Question

**Can a multi-source edge generation framework — built entirely from existing corpus data and
agent consumption patterns — produce a graph dense enough for PPR to demonstrate measurable
retrieval lift? And which signal sources are worth delivering?**

---

## The "Ten Messy Sources" Strategy

Do not filter at generation time. Generate edges from every viable signal source. Tag each
edge with its origin signal. Let density build. The evaluation will determine which sources
contribute to PPR lift and which are pure noise. The GNN will learn the weights.

Target: a synthetic graph with 10x current Informs edge density minimum. Current baseline is
~85 Informs + 1,000 CoAccess = 1,085 total edges. Target synthetic graph: 3,000–8,000 edges
across multiple typed sources before PPR evaluation.

---

## Signal Sources to Inventory and Generate

For each source: generate candidate edges against `snapshot.db`, count edges produced,
characterize the distribution (degree, coverage percentage, cross-category ratio).

### S1 — Tag Co-occurrence (existing data)
Entries sharing ≥3 tags (Jaccard threshold configurable). No model required — pure SQL
against `entry_tags`. Noisy by design; the noise is acceptable.

Edge type: `Informs` (weak domain clustering)  
Expected yield: high — 83% tag coverage across 1,134 active entries  
Generation: SQL only

### S2 — Structural Vocabulary Overlap (configurable)
A configured list of domain structural terms. Entries sharing ≥2 terms from the list are
domain-adjacent. The list is domain-agnostic by design — software engineering deployment
provides one vocabulary; SRE or legal compliance deployments provide their own.

Starting vocabulary for this corpus (Rust/software engineering):
`Trait`, `impl`, `Handler`, `Service`, `Repository`, `Arc`, `RwLock`, `async`, `tokio`,
`migration`, `schema`, `compaction`, `WAL`, `HNSW`, `embedding`, `tick`, `cycle`

Edge type: `Informs` (structural role adjacency)  
Expected yield: medium — coverage depends on vocabulary match rate against entry bodies  
Generation: keyword scan against entry bodies + keywords table

### S3 — Keyword Set Overlap (existing data)
Entries sharing ≥3 keywords from the stored keywords table (col-022). Different from
structural vocabulary: these are corpus-derived terms, not administrator-configured.
Keyword overlap captures same-concept entries phrased differently across feature cycles.

Edge type: `Informs` (concept thread)  
Expected yield: medium  
Generation: SQL against keywords table

### S4 — Lexical Citation Detection (existing data)
Scan entry bodies for references to other entries: title matches, tag matches, or
keyword cluster matches. "As established in the graph compaction ADR" → extract citation,
write directed edge. This is a prerequisite/reference signal, not semantic similarity.

Edge type: `Prerequisite` (explicit reference) or `Informs` (implicit domain reference)  
Expected yield: low-medium — depends on how explicitly entries reference each other  
Generation: text scan against entry bodies

### S5 — Supersession Chain Topology (existing data)
Supersession chains (A → superseded by B → superseded by C) define topic lineage threads.
Entries within the same chain are connected by topic even if textually dissimilar. Entries
in different chains that were written in the same feature cycle AND share keywords are
probably related.

Edge type: `Informs` (topic thread)  
Expected yield: low — depends on chain depth  
Generation: SQL traversal of existing supersession records

### S6 — Outcome Co-retrieval (existing data)
Entries co-accessed within successful feature cycle sessions. Stronger than plain CoAccess
because outcome-gated: the co-access happened in a session that ended successfully.
Requires joining OUTCOME_INDEX with CO_ACCESS and cycle session records.

Edge type: `Informs` (outcome-correlated, strong signal)  
Expected yield: unknown — audit OUTCOME_INDEX and CO_ACCESS join feasibility first  
Generation: SQL join; establish whether session-level co-access is reconstructible

### S7 — Briefing Selection Signal (existing data if logged)
When `context_briefing` returns N entries and the agent subsequently calls `context_get`
on a subset M, the selected M entries are implicit endorsements in that task context.
Entries selected together from the same briefing call are task-context-adjacent.

Feasibility gate: determine whether briefing calls and subsequent get calls are
correlatable in the AUDIT_LOG. If not logged with session correlation, this source
is UNTESTABLE — document and close.

Edge type: `Informs` (task-context endorsement)  
Expected yield: unknown pending feasibility audit

### S8 — Repeated Search Term Clustering (existing data if logged)
Search terms that co-occur across multiple sessions from different agents define a topic
cluster. Entries that consistently surface in results for the same term cluster are
central to that topic — more reliably than any single retrieval event.

Feasibility gate: same as S7 — requires search log with term + result correlation.

Edge type: `Informs` (topic centrality)  
Expected yield: unknown pending feasibility audit

### S9 — Cross-Feature Temporal Clustering (existing data)
Entries from different feature cycles that share domain keywords and compatible categories.
This catches the same concept appearing in different feature contexts — the "it came up
again" signal. An entry about async task management from crt-004 and an entry about the
same topic from col-031 are related even if they don't reference each other.

Edge type: `Informs` (concept recurrence)  
Expected yield: medium  
Generation: SQL; group by keyword cluster × category, cross-join across feature cycles

### S10 — Graph Centrality (structural, derived)
After generating S1-S9, compute degree centrality for the combined graph. High-degree
entries are structurally foundational — they connect otherwise separate clusters.
This is not an edge source; it is a node weight that should feed into PPR initialization
and, later, GNN node features.

Output: centrality scores per entry_id, saved as metadata alongside the synthetic graph.

---

## Utilization Use Cases to Test

The ASS-037 eval harness measured semantic retrieval — queries where cosine wins by
construction. This spike must test the scenarios the graph is actually built for.

Design at minimum 20 new scenarios targeting:

**UC1 — Cross-category bridging**
A query about a procedure or convention that should also surface the lesson-learned that
motivated it. The lesson-learned and the procedure have low cosine similarity (problem
description vs. solution prescription). Only graph topology can connect them.

Example structure: query matches entry B (decision), ground truth also includes entry A
(lesson-learned that informed B), where cosine(query, A) < 0.4.

**UC2 — Dormant foundational knowledge**
A query about modifying or extending a fundamental structure defined in an early feature.
The foundational entry has low recent access, low confidence boost, but high structural
centrality. Tests whether the graph surfaces what cosine + confidence would bury.

**UC3 — Prerequisite surfacing**
A query about applying a pattern that requires understanding a prior ADR. The ADR is
prerequisite to the pattern — not semantically similar to the query, but required for
correct application.

**UC4 — Same-concept, different-cycle**
A query that matches an entry from a recent feature cycle. Ground truth includes an
older entry from a different cycle that covers the same concept with different vocabulary.
Tests whether cross-feature temporal clustering (S9) bridges the vocabulary gap.

These scenarios require manual construction against the snapshot. Build them before
running the PPR evaluation — they are the measurement instrument for the hypothesis.

---

## Evaluation Design

### Phase 1 — Edge Generation Audit
For each signal source S1-S10:
- Count: how many candidate edges pass the generation criteria?
- Coverage: what percentage of active entries gain at least one new edge?
- Cross-category ratio: what fraction connect entries of different categories?
- Document yield and characterization in a table

If any source yields < 20 edges, mark INSUFFICIENT and exclude from combined graph.

### Phase 2 — Combined Synthetic Graph
Inject all edges from viable sources into `snapshot-combined.db` (copy of snapshot.db).
Tag each edge row with `signal_origin` (S1, S2, ... S9) for later ablation.

Build the combined graph in passes:
1. Preserve existing CoAccess (1,000 edges) and Informs (83 edges)
2. Inject S1-S9 synthetic edges
3. Compute S10 centrality scores, store as entry metadata

Target: verify combined graph achieves ≥3,000 active→active edges before proceeding.

### Phase 3 — PPR Validation (the primary hypothesis test)

Run against the combined snapshot, using ASS-037 UC scenarios + new UC1-UC4 scenarios:

**Profile A**: `w_sim=0.50, w_conf=0.35, ppr_blend_weight=0.15`  
**Profile B**: `w_sim=0.50, w_conf=0.35, ppr_blend_weight=0.00` (PPR disabled)

If Profile A > Profile B on the new UC scenarios: **graph topology hypothesis CONFIRMED**.
The multi-source graph enables retrieval that cosine alone cannot provide.

If Profile A = Profile B: density is insufficient, or the UC scenarios are not
discriminating. Document which UC types show zero delta and which show partial lift.

### Phase 4 — Per-Source Ablation
Disable one signal source at a time (remove its edges from the combined graph) and
re-run the PPR comparison. Identify which sources contribute to PPR lift and which
are noise.

| Ablation | Edges removed | ΔP@5 | ΔMRR | Verdict |
|----------|--------------|------|------|---------|
| S1 removed | tag co-occurrence | ... | ... | signal / noise |
| S2 removed | structural vocabulary | ... | ... | signal / noise |
| ... | | | | |

Sources that show zero delta when removed are noise at current density.
Sources that show negative delta when removed are load-bearing signal.

### Phase 5 — GNN Readiness Assessment
Does the combined labeled edge set meet the minimum requirements for GNN training?

- Edge count: ≥ 2,000 labeled edges across ≥ 4 signal origins
- Coverage: ≥ 60% of active entries have ≥ 1 non-CoAccess edge
- Label quality: each edge tagged with signal_origin (required for GNN feature construction)
- Node features available: entry category, confidence, age, keyword count, tag count

If these thresholds are met: the combined graph is W3-1 training data ready.
Document the feature vector specification (what GNN input looks like per node and edge).

---

## What Passes, What Fails

**PASS — Graph topology adds retrieval value**
- Combined graph reaches ≥3,000 edges
- Profile A MRR > Profile B MRR on UC scenarios (any positive delta)
- ≥2 signal sources identified as load-bearing (non-noise) in per-source ablation

**PARTIAL — Density achieved, topology not yet contributing**
- Combined graph reaches ≥3,000 edges
- Profile A = Profile B on all scenarios
- Interpretation: edge density threshold not yet reached, or UC scenarios need refinement
- Action: document minimum density estimate; deliver edge generation sources anyway

**FAIL — Signal sources insufficient**
- Cannot reach ≥3,000 edges from available sources
- Most signal sources yield < 20 edges
- Action: reassess source definitions; the corpus may be too small for this approach

---

## Output

1. **Signal source inventory** — yield, coverage, and cross-category ratio per source (S1-S10)
2. **Combined graph statistics** — total edges, coverage percentage, degree distribution
3. **PPR validation result** — Profile A vs. B on both semantic and UC scenario sets
4. **Per-source ablation table** — which sources are signal vs. noise
5. **GNN readiness verdict** — does the labeled edge set meet W3-1 training data requirements?
6. **Feature vector specification** — what GNN input looks like per node and edge type
7. **Delivery sequence recommendation** — which signal sources to ship first based on yield + contribution

---

## Constraints

- **Harness only.** No production app changes. All edge injection is against snapshot copies.
  `snapshot.db` is never modified — always work from named copies.
- **Use the ASS-037 harness.** Extend `eval run` and `eval report` with the new profiles.
  Do not rebuild infrastructure.
- **S6, S7, S8 feasibility gates.** Before spending time on behavioral source implementation,
  audit what is actually in AUDIT_LOG and CO_ACCESS. If session-level correlation is not
  available, mark those sources UNTESTABLE and move on. Do not block the spike on log
  infrastructure that doesn't exist yet.
- **UC scenario construction is pre-work.** Build the cross-category and dormant-knowledge
  scenarios before running Phase 3. Evaluating PPR contribution without purpose-built
  scenarios repeats the ASS-037 mistake of measuring the wrong thing.
- **Tag each synthetic edge with signal_origin.** This is required for per-source ablation
  and GNN feature construction. Do not inject untagged edges.
- **Do not implement delivery features.** This spike recommends which sources to ship.
  Implementation belongs in a delivery session.

---

## What This Is Not

This spike does not implement any of the signal sources in the production app. It does not
design the GNN architecture (that belongs to W3-1 scoping). It does not clean up or
restructure existing tags. It answers one question: does a multi-source graph make PPR
testable, and which sources are worth delivering?
