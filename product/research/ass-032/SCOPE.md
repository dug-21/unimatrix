# ASS-032: Best-Possible Knowledge Surfacing Pipeline for a Self-Learning Knowledge Engine

**Status**: Revised (2026-03-25) — expanded scope; Parts 1–5 are background; Part 6 is the active research mandate
**Supersedes**: ASS-032 v1 (2026-03-25, feedback loop audit) — absorbed as Part 1; ASS-032 v2 (same day, software-dev framing) — corrected by Part 6
**Feeds**: W3-1 delivery scoping; possible redesign of W3-1 learning objective; broader self-learning architecture
**Predecessor**: ASS-031 (W3-1 pre-implementation research spike)

---

## Corrections to Previous Research

Two material corrections to the prior ASS-032 report before reading further.

### Correction 1: Corpus size assumption was wrong

The previous ASS-032 stated the corpus as "53 active entries, nearly all decision." That figure came from stale project memory about a single bootstrapped deployment. It was wrong in two ways.

First, Unimatrix is a deployable engine that runs in any repository. The corpus size and category distribution are unknown and variable per deployment. No assumption about corpus shape is valid in architectural recommendations.

Second, querying the actual live database for this deployment (2026-03-25):

| Category | Active entries | Entries with access_count > 0 | Avg confidence | Avg access_count |
|---|---|---|---|---|
| lesson-learned | 2263 | 135 (6%) | 0.096 | 0.40 |
| decision | 320 | 308 (96%) | 0.528 | 8.68 |
| pattern | 247 | 216 (87%) | 0.516 | 8.47 |
| convention | 73 | 9 (12%) | 0.082 | 3.12 |
| procedure | 41 | 36 (88%) | 0.526 | 12.0 |
| outcome | 24 | 6 (25%) | 0.122 | 0.42 |
| duties | 14 | 14 (100%) | 0.551 | 5.64 |

**Total active: 2982. Total deprecated: 244.**

This is a 56x larger corpus than the stale memory assumed, with a category distribution that directly validates the feedback loop concern: lesson-learned entries (76% of the corpus) have only 6% access penetration and near-floor average confidence (0.096), while decision and pattern entries (19% of the corpus) dominate access counts and confidence. The structural bias is not theoretical — it is measurably present in production.

All prior architectural analysis that assumed a small "decision-dominated" corpus was directionally correct about the bias mechanism but quantitatively wrong about its severity. The actual situation is worse: 2263 lesson-learned entries are largely frozen at low confidence with minimal access, while 320 decision entries are circularly boosting each other. The confidence gap between these categories is 0.43 confidence points on average.

**Architectural mandate going forward**: The corpus is deployment-variable. Architectures must be correct across the range of small (tens of entries) through large (tens of thousands) with unknown category distributions. The live deployment demonstrates that extreme category imbalance in access patterns is the rule, not the exception.

### Correction 2: The mandate is not "fix the feedback loop"

The previous ASS-032 converged on a staged fix (epsilon injection + phase-affinity matrix + W3-1 with IPS). That was one option derived from one framing of the problem. The actual question is: **what is the best possible knowledge surfacing pipeline for Unimatrix's use case?** This document answers that question from first principles, then evaluates the current architecture and the W3-1 design against the answer.

---

## Part 1: The Use Case, Precisely Stated

### 1.0 What Unimatrix Actually Is

**Unimatrix is a general-purpose, workflow-aware knowledge management engine.** Its schema (knowledge categories) and workflow topology (phase definitions) are deployment-configurable via domain packs. There is no hard coupling to software development.

**Structural invariants across all domain packs:**
1. **Categories** — typed knowledge containers. Every deployment defines a category taxonomy. Categories encode the *type* of knowledge and its appropriate context of use. The number, names, and semantics of categories are domain-specific.
2. **Phases** — discrete workflow states. Every deployment defines a phase topology. Phases encode *where in a workflow* an agent is operating, which changes which categories of knowledge are most relevant.

**The software development domain pack** (the current default deployment) uses categories: `decision/ADR`, `convention`, `pattern`, `procedure`, `lesson-learned`, `outcome` — and phases: `scope`, `design`, `delivery`, `review`, `retro`. These are *examples*, not invariants. A legal research deployment might use categories `precedent`, `statute`, `procedure`, `brief-fragment` with phases `intake`, `discovery`, `drafting`, `review`. A medical deployment might use `guideline`, `contraindication`, `protocol`, `case-note` with phases `triage`, `diagnosis`, `treatment`, `followup`.

**Architectural mandate**: All pipeline designs must be correct and meaningful for any (categories, phases) configuration. No hardcoded category names, no hardcoded phase names, no assumptions about their semantics. The surfacing pipeline must reason over whatever the deployment defines.

### 1.1 The General Use Case

Unimatrix surfaces knowledge to agents in a multi-agent orchestration system operating within a structured workflow. The precise requirements:

- **Agents have roles**: roles are deployment-specific (e.g. architect, spec writer, tester for software dev; or case analyst, drafter, reviewer for legal). Each role has different knowledge needs.
- **Agents operate in known workflow phases**: phases are defined by the domain pack and are discrete, detectable (via `context_cycle`). The software-dev phases (scope, design, delivery, review, retro) are one instance.
- **Knowledge is categorized**: categories are defined by the domain pack. Categories encode the type of knowledge and its appropriate use context. The software-dev categories (decision, pattern, procedure, etc.) are one instance.
- **Agents do not know what they need**: They cannot formulate precise queries for unknown unknowns. The system must proactively surface relevant knowledge without a query.
- **Human feedback is sparse**: Helpful/unhelpful votes are infrequent. Most entries receive no explicit feedback. Implicit behavioral signals (session outcomes, rework events) provide more signal but are noisy.
- **The system must improve over time**: It is not a static retrieval system. Self-improvement is a design requirement.
- **No dedicated ML team, no external services**: The system runs in-process, maintains itself, and must be operationally simple.

**The precise definition of "surfacing quality"** (derived from the use case):

An entry is **well-surfaced** if: agents who receive it are more likely to produce correct, non-reworked outputs than agents who do not. An entry is **under-surfaced** if: it is semantically relevant to the session, exists in the knowledge base, but consistently ranks below the top-k due to signals unrelated to its semantic relevance (low access_count, low freshness, no co-access). The pipeline is **self-improving** if the distribution of surfaced entries converges toward the distribution of semantically relevant entries, not toward the distribution of historically popular entries.

---

## Part 2: What Does the Best Pipeline Look Like? External Research

### 2.1 How Agent Memory Systems Handle Knowledge Surfacing

**Letta (MemGPT)**: Uses explicit memory blocks — agents directly edit what stays in-context vs. archival. Retrieval is agent-directed: the agent calls tools to read/write memory. This is powerful for single-agent personalization but requires agents to know what they need. The "proactive surfacing" problem is not solved — it is delegated to the agent.

**Mem0**: Extracts salient facts from interactions, stores them, retrieves top-s semantically similar memories at query time using dense embeddings. Ranking is purely semantic (no confidence, no co-access, no phase signal). No exploration mechanism. Temperature-0 deterministic operation for reproducibility. Mem0 solves the "remember what was discussed" problem, not the "surface what the agent needs but hasn't asked for" problem.

**Zep**: Temporal knowledge graph tracking how facts change over time. Combines graph traversal and vector search. Strong at temporal queries ("what was the decision about X three months ago"). Does not address exploration or category coverage. Graph construction is expensive (600,000+ token memory footprint per conversation) and retrieval quality degrades immediately post-ingestion.

**MemOS**: Hierarchical memory OS with MemScheduler for active lifecycle management. Combines recency, access frequency, semantic alignment, and contextual relationships. Explicitly addresses the cold-start / underutilized entry problem via MemGovernance. This is the closest to Unimatrix's use case, but MemOS is designed for conversational memory, not for structured, categorized knowledge with typed entries and workflow phase awareness.

**Key finding**: No existing agent memory system solves the full Unimatrix use case. They all solve subproblems (temporal memory, personalization, conversational grounding). The proactive, phase-aware, category-diverse surfacing of structured knowledge to role-specific agents in a multi-phase workflow is a distinct problem that Unimatrix must solve for itself. The closest relevant body of knowledge is **recommender systems with diversity/coverage objectives**, not agent memory.

### 2.2 The Right Learning Objective

The ASS-031 W3-1 design frames the learning objective as supervised binary classification: predict whether a surfaced entry received a helpful label. This is Interpretation B (session-conditioned relevance). Four alternatives exist with different properties.

**Option 1 — Supervised MLP (W3-1 as designed)**

Learn: `P(helpful | entry_features, session_context)` from logged feedback.

Strengths: Well-understood, uses existing `unimatrix-learn` infrastructure, leverages full feature richness.

Weaknesses: **Feedback loop**: training data contains only entries that the current formula already surfaced. The model learns to replicate the existing distribution, not to improve it. Requires MIN_TRAIN_SIZE=50 samples before activation (months of cold start in practice). EWC++ may lock in biased early patterns — see Section 3.4.

**Option 2 — Contextual Bandit (Thompson Sampling)**

Learn: A per-entry Beta distribution (or contextual linear model) over expected helpfulness. At serving time, sample from each arm's posterior and select the highest-sampled arm.

Strengths: Exploration is built-in. New entries have high uncertainty → high sampling probability → get surfaced and labeled sooner. Works from day zero (no cold-start gate). Naturally handles sparse feedback. Category coverage improves automatically because underrepresented categories have high uncertainty.

Weaknesses: Pure per-arm Thompson Sampling (Beta-Bernoulli) ignores session context — it treats all sessions identically. A contextual bandit (LinUCB or Neural Thompson Sampling) captures context but is more complex. LinUCB assumes linear reward models, which may not hold for complex entry×session interactions. Neural Thompson Sampling adds computational weight.

**Practical fit for Unimatrix**: For the injection/briefing surface (Modes 1/2), a **per-entry Beta-Bernoulli Thompson Sampling policy** is a strong fit. The action space is bounded (top-k candidates from HNSW or phase-filtered), rewards come from explicit votes and implicit session signals, and uncertainty naturally forces exploration of new entries. Per-entry Beta priors (α₀=3, β₀=3, matching the existing Bayesian helpfulness prior) provide the warm start. This is not complex to implement and produces principled exploration without any training gate.

For Mode 3 (search re-ranking), the query anchors the action, and a bandit is less natural. A pointwise scoring model with IPS debiasing is more appropriate here.

**Option 3 — Preference Learning / Ranking Loss**

Instead of predicting absolute helpfulness, learn pairwise preferences: "was entry A more useful than entry B in session context C?" This requires pairs, which are harder to generate from sparse feedback but potentially richer.

**Practical fit for Unimatrix**: No existing infrastructure for pairwise labels. Requires co-occurrence of two surfaced entries with differential feedback, which is rare at current corpus/feedback volumes. Defer.

**Option 4 — Mixture of Experts / Category-Role Routing**

Learn a separate scoring head per (agent_role, workflow_phase) pair. Each head is a small MLP trained only on sessions from that role/phase combination.

Strengths: Each expert is trained on a homogeneous distribution. No cross-contamination. Naturally interpretable ("this is the procedure-delivery model").

Weaknesses: Requires labeled data per role×phase combination. With few agents and few phases, some combinations may never have enough data. Adds model management complexity (one model per combination vs. one shared model). Does not naturally handle unknown phases.

**Practical fit for Unimatrix**: The phase-affinity matrix (Option C from prior research) is a lookup-table approximation of this without ML. If training data eventually justifies per-phase models, this is the right evolution path. Not the right starting point.

**Recommendation on learning objective**: A **hybrid learning objective** is optimal:

- **Injection/briefing (Modes 1/2)**: Thompson Sampling bandit over HNSW candidates, conditioned on phase and category signals. Per-entry Beta-Bernoulli posteriors, updated from explicit votes and implicit session outcomes. Exploration is built-in.
- **Search re-ranking (Mode 3)**: Supervised MLP (W3-1 architecture) with IPS-weighted training, as query context makes the scoring problem well-defined and the bandit formulation less natural.
- **Phase-category affinity**: Static configurable lookup table (`phase_affinity[phase][category]`) activating the `w_phase_explicit` placeholder immediately. This provides deterministic, zero-training phase signal that improves the training data distribution for the MLP.

### 2.3 Evaluation Metrics for Knowledge Surfacing Quality

The current eval harness (W1-3, nan-007) computes P@K and MRR against ground-truth entry lists. These metrics have a structural defect for this use case: they measure "did the right entry score highly?" but not "did the result set cover the right knowledge types?" A system that returns the same five decision entries for every query will score perfectly on P@5/MRR if those are the ground-truth entries, while failing to surface the lesson-learned or procedure entry that would have prevented a rework.

The field has validated the inadequacy of pure precision/recall metrics for diversity-critical retrieval. The recommender systems literature (Herlocker et al. 2004 "Beyond Accuracy", Celma 2010 serendipity, RecMetrics library) has studied catalog coverage, intra-list diversity, novelty, and serendipity for two decades. Production systems (Spotify, Netflix, YouTube) all track diversity metrics alongside accuracy metrics because accuracy-maximizing systems reliably collapse to popular-item recommendation.

**Metrics that should replace or augment P@5/MRR:**

**Metric 1 — Category Coverage@k (CC@k)**

Definition: Across a set of N test queries, what fraction of configured categories appear at least once in the top-k results?

Formula: `CC@k = |{cat : ∃ entry ∈ top-k results with entry.category = cat}| / |configured_categories|`

This is measured across a query set, not per-query. A value of 1.0 means all categories appear in the top-k results across the test suite. A value of 0.11 (1/9 = only "decision" appears) represents the saturation failure.

Why this matters: The live corpus has 2263 lesson-learned entries with 0.096 average confidence. If CC@k for the lesson-learned category is near zero in the eval harness, the system is systematically failing to surface 76% of its knowledge base. P@5 would not detect this.

**Metric 2 — Intra-Session Category Diversity (ICD)**

Definition: Across a full feature cycle session, what is the Shannon entropy of the category distribution of surfaced entries?

Formula: `ICD = -Σ_cat p(cat) * log(p(cat))` where `p(cat)` = fraction of entries surfaced in this session with that category.

Maximum entropy (uniform distribution across k categories): `log(k)`. Minimum (single category): 0.

Why this matters: ICD measures whether the pipeline surfaces a representative mix of knowledge types across a session. A session where every injection is a "decision" entry has ICD ≈ 0 regardless of how high the P@5 for those decisions is.

**Metric 3 — Novel Entry Exposure Rate (NEER)**

Definition: Fraction of entries surfaced in this session (injection or search) that have never been surfaced to this agent (session_id) before.

Formula: `NEER = |surfaced_entries - previously_surfaced_entries| / |surfaced_entries|`

Requires the `injection_log` and session-scoped history. Already partially tracked via `injection_history` in session state.

Why this matters: NEER measures exploration rate. A NEER near 1.0 means every surfacing is a new entry (aggressive exploration). A NEER near 0.0 means the agent sees the same entries repeatedly (no learning). The target is a NEER that starts high and decreases as the session progresses (early exploration, later exploitation).

**Supporting metrics** (important but secondary):

- **Phase-relevance precision (PRP@k)**: Of entries surfaced during phase X, what fraction are in categories that have a non-zero phase affinity for phase X? Requires a ground-truth affinity matrix (the configurable lookup table from Option C).
- **Temporal confidence improvement**: Does the average confidence of surfaced entries increase over sessions for underrepresented categories? Tracks whether the pipeline is learning.
- **Intra-list semantic diversity (ILD)**: Average pairwise cosine distance between surfaced entries' embeddings. Prevents retrieval of near-duplicate entries under different surface forms.

**What metrics the current eval harness should NOT drop:**

P@K and MRR are still valid for evaluating semantic search precision. They are necessary but not sufficient. The harness should add CC@k, ICD, and NEER as co-equal first-class metrics. If CC@k or ICD degrades in a shadow model promotion evaluation, the shadow model should not be promoted even if P@K improves.

### 2.4 The Exploration Problem

**Epsilon-greedy vs. UCB1 vs. Thompson Sampling for knowledge surfacing at corpus size 10–10,000:**

| Approach | Behavior at small corpus (<100 entries) | Behavior at large corpus (>1,000 entries) | Implementation complexity |
|---|---|---|---|
| Epsilon-greedy (fixed ε) | Simple, predictable, ε * k slots are random. Risk: random slot may surface irrelevant entry (semantic similarity < threshold). | Same behavior regardless of corpus size — does not adapt exploration rate to uncertainty. | Hours. |
| UCB1 | Explicit exploration bonus: `x̄ + sqrt(2 ln(N) / n_i)`. Ensures every entry is explored proportional to its uncertainty. Does not require semantic similarity floor (can surface any entry). | Same formula, bounded exploration over time. Approaches greedy as n_i grows. | Half-day for per-entry UCB state. |
| Thompson Sampling (Beta-Bernoulli) | Per-entry Beta posterior (α_i, β_i). New entries have (α₀=3, β₀=3) → high variance → high exploration probability. Updates continuously with each observation. | Same behavior, scales to any corpus size. Warm start from helpfulness priors means no truly random surfacing. | Half-day for per-entry state management. |

**Thompson Sampling is the right choice for Unimatrix's injection pipeline.** Key reasons:

1. Unimatrix already maintains per-entry Bayesian helpfulness state (α, β derived from helpful_count/unhelpful_count). Thompson Sampling is the direct action policy built on top of this existing state — it requires no new state, only a sampling step at serving time.
2. New entries (α₀=3, β₀=3) have high variance → they are naturally explored without a dedicated "exploration slot." The warm start (prior = 0.5) prevents surfacing of known-irrelevant entries.
3. As entries accumulate votes, their posteriors sharpen → exploration rate decreases naturally → no hyperparameter tuning of exploration decay.
4. Counterfactual evaluation: entries not sampled in a session can still have their posterior evaluated for "would this have been good?" analysis, enabling offline evaluation of the policy.

**Forced exposure for signal gathering**: For entries with very low access counts (access_count < 5) and high semantic similarity to the current session (similarity > 0.4), the Thompson Sample draws from a more diffuse posterior than the raw (helpful_count + α₀) / (helpful_count + unhelpful_count + α₀ + β₀) estimate. This is equivalent to confidence interval widening for underexplored entries — a form of UCB within the Thompson Sampling framework. This mechanism is already partially present in the existing Bayesian helpfulness prior; it just needs to be activated as a serving-time policy rather than just a confidence dimension.

---

## Part 3: Honest Evaluation of the Current Architecture

### 3.1 What Is the Confidence System Actually Modeling?

Reading `confidence.rs` and the ASS-031 audit, a "high confidence" entry in the current system means: **an entry that was created by a trusted source, was frequently accessed (recently), has been corrected at least once, and has received helpful votes.** What it does NOT mean: "this entry is semantically useful for the current session." Confidence is a historical popularity and quality signal, not a relevance signal.

This distinction matters because confidence gets w_conf=0.15 in the fused score — the third-largest weight after NLI (0.35) and similarity (0.25). It acts as a persistent popularity bias on top of the semantic similarity signal.

**The 94-point confidence gap** (from live corpus data): Decision entries average 0.528 confidence; lesson-learned entries average 0.096. This is not because lesson-learned entries are less valuable — it is because 94% of lesson-learned entries have never been accessed (only 135/2263 have access_count > 0). The confidence system has correctly computed that these entries have no demonstrated utility — but the reason they have no demonstrated utility is that they were never surfaced, not that they are irrelevant.

### 3.2 Component Evaluation

| Mechanism | Keep / Replace / Add | Rationale |
|---|---|---|
| Semantic similarity (HNSW cosine + NLI) | **Keep, w_sim + w_nli = 0.60 (dominant)** | Unbiased toward historical popularity. The strongest signal. The NLI cross-encoder is the precision layer. Do not reduce weights. |
| Confidence composite (w_conf = 0.15) | **Keep but reframe** | Useful as a quality/trust signal. Problem is the usage_score dimension circularly amplifies surfaced entries. Consider decoupling: keep base_score + trust_score + correction_score (objective quality), separate usage_score and freshness_score into a separate "freshness" signal with lower weight. |
| Co-access boosting (w_coac = 0.10) | **Keep, ceiling is appropriate** | The MAX_CO_ACCESS_BOOST = 0.03 ceiling limits cumulative drift. Co-access captures genuine structural relatedness (entries that appear together are often complementary). The feedback loop concern is real but bounded by the ceiling. |
| WA-2 category histogram (w_phase_histogram = 0.02) | **Keep as complement to Option C** | Within-session signal is weak but not harmful. With the phase-affinity matrix activating w_phase_explicit, this remains useful as an in-session momentum signal. |
| Phase-affinity matrix (w_phase_explicit, currently 0.0) | **Add immediately** | Zero-training, auditable, immediate improvement. Activates the reserved placeholder. Must be added before W3-1 to improve training data distribution. |
| Thompson Sampling injection policy | **Add** | Replaces the current greedy top-k injection with principled exploration. No training required. Operates on existing helpfulness state. |
| EWC++ | **Keep, but defer activation** | EWC++ has a known failure mode: Fisher Information Matrix computation produces vanishing gradients for high-confidence predictions (EWC Done Right, 2026), causing it to over-protect early learned patterns. In a biased training setting, EWC++ preserves the bias. Recommendation: do not activate EWC++ until the training reservoir has a CC@k of ≥ 0.7 (all major categories represented). Use L2 regularization instead for the first training window. |
| MicroLoRA (unimatrix-adapt) | **Keep, unchanged** | Infrastructure asset awaiting a training pipeline. MicroLoRA is for embedding adaptation; not the right tool for scoring. |
| W3-1 MLP RelevanceScorer | **Modify** | The architecture is sound (ASS-031). The learning objective and training data pipeline need the corrections in Section 3.3. Mode 3 (search re-ranking) is the priority. Mode 1/2 injection should use Thompson Sampling instead of GNN scoring. |
| IPS-weighted training | **Add to W3-1 scope** | Required to correct selection bias in the training reservoir. Propensity estimate: `P(entry surfaced | session) ≈ (access_count_in_topic) / (total_active_entries_in_topic)`. Self-normalized IPS to control variance. |

### 3.3 What W3-1 Should Actually Be

W3-1 was designed as a unified model for all three modes. Given the analysis above, the best design separates the modes:

**Mode 3 (search re-ranking)**: Keep W3-1 MLP as designed. The query anchor makes this a well-defined supervised problem. Add IPS debiasing. This is the Mode 3 that benefits most from learned ranking because the explicit query provides a relevance anchor.

**Modes 1/2 (proactive injection and briefing)**: Replace the planned GNN-driven proactive scoring with Thompson Sampling over HNSW candidates. This provides immediate exploration without a training gate, naturally handles cold-start for new categories, and solves the category saturation problem the MLP cannot solve without diverse training data. The MLP can be added on top of Thompson Sampling later (Neural Thompson Sampling) if the bandit alone is insufficient.

**Phase-category routing**: The phase-affinity matrix (Option C) handles this deterministically. W3-1's phase one-hot features capture fine-grained phase signals that the static matrix cannot, but only after the model has seen sufficient diverse training data. The matrix is the bridge.

### 3.4 EWC++ and Biased Training: The Critical Risk

The 2026 "EWC Done Right" paper identifies that EWC++ produces near-zero Fisher Information Matrix values when the model achieves high confidence on predictions. In a biased training set (decision entries dominate labels), the model quickly learns "decision entries are relevant" with high confidence → vanishing FIM → EWC++ fails to protect the weights that encode decision-entry recognition → paradoxically, the model loses the decision-entry patterns it should keep, while also being unable to learn new category patterns. The net result is instability.

**Recommendation**: Use L2 regularization (weight decay) instead of EWC++ for the first W3-1 training window. Activate EWC++ only after the first CC@k ≥ 0.7 checkpoint. This matches the EWC++ lifecycle intent: it was designed to prevent forgetting of a representative learned model, not to bootstrap learning from biased data.

---

## Part 4: Proposed Best Pipeline

### 4.1 Architecture

The best pipeline for Unimatrix's use case is a **three-layer hybrid** combining deterministic routing, principled exploration, and learned re-ranking:

**Layer 1 — Candidate Retrieval (unchanged)**
HNSW approximate nearest-neighbor search (bi-encoder cosine similarity) over the active corpus. EF=32, filtered to Active status only. Returns top-200 candidates. This layer is unbiased toward historical popularity — every entry with semantic similarity to the query/briefing topic is a candidate.

**Layer 2 — Phase-Category Gate (new)**
Apply the configurable `phase_affinity[current_phase][category]` matrix as a multiplicative gate on candidates. Entries in categories with `phase_affinity[phase][cat] = 0.0` are moved to a secondary pool (not discarded — available for exploration). This is the `w_phase_explicit` placeholder being activated.

**Layer 3A — Briefing/Injection Scoring (Thompson Sampling + fused score)**
For proactive injection (Mode 1) and briefings (Mode 2):
1. Score each candidate with the current fused formula (NLI + similarity + confidence + co-access + phase signals).
2. For each candidate, sample from its Beta posterior: `score_ts = Beta(α_i + helpful_count, β_i + unhelpful_count).sample()`.
3. Final injection score: `0.7 * fused_score + 0.3 * ts_sample`. The 0.7/0.3 blend starts exploitation-heavy and can be tuned.
4. Apply Category Coverage Guarantee: if any configured category with ≥1 active entry in the candidate pool has zero representation in the top-k results, force-include the highest fused_score entry from that category (replacing the lowest-scored result). This is the "floor" mechanism.

**Layer 3B — Search Re-ranking (fused formula + W3-1 MLP at blend_alpha > 0)**
For reactive search (Mode 3), the existing fused formula remains primary. W3-1 MLP adds the `phase_explicit_norm` term as blend_alpha ramps from 0 to 1 as training data accumulates. IPS-weighted training corrects selection bias.

**Layer 4 — Learning Signal (three channels)**
1. Explicit helpfulness votes → update Beta(α_i, β_i) per entry + add to W3-1 RelevanceSample reservoir with weight=1.0.
2. Implicit session outcome (success → positive, rework → negative) → update Beta posteriors for entries in that session + add to reservoir with weight=0.4.
3. Coverage feedback: track which categories were surfaced in each session. This feeds the CC@k metric and informs the phase-affinity matrix recalibration.

### 4.2 Migration Path

**Stage 0 (immediate, no feature gate)**: Live corpus data confirms the bias is severe. The `lesson-learned` category has 2263 entries with 0.096 average confidence and 6% access rate. No code change is needed for Stage 0 — it is the baseline characterization.

**Stage 1 (pre-W3-1, days of work)**:
- Implement phase-affinity matrix in `InferenceConfig`, populate `w_phase_explicit`. Map: scope→{lesson-learned: 0.8, pattern: 0.7, decision: 0.5}, design→{decision: 0.8, pattern: 0.8, procedure: 0.3}, delivery→{procedure: 0.9, decision: 0.5}, review→{lesson-learned: 0.9}, retro→{lesson-learned: 0.8, outcome: 0.7}.
- Implement Category Coverage Guarantee in `IndexBriefingService::index()`: after normal top-k selection, check which categories have zero representation, force-include one entry per missing category from the candidate pool.
- Implement Thompson Sampling serving for Mode 1/2: read `helpful_count` and `unhelpful_count` from each candidate, sample Beta(α₀+helpful, β₀+unhelpful), blend with fused score.

**Stage 2 (W3-1 delivery, modified from ASS-031 design)**:
- Mode 3 MLP as designed in ASS-031/GNN-ARCHITECTURE.md. Add IPS weighting to training samples.
- Replace EWC++ with L2 regularization for the first training window.
- Add CC@k, ICD, and NEER as eval harness metrics (required for shadow model promotion gate).
- Shadow promotion gate: requires `CC@k ≥ 0.7` AND `MRR_delta ≥ 0` AND `P@K_delta ≥ -0.02` (allow small P@K regression if coverage improves).

**Stage 3 (post-W3-1, if bandit proves insufficient)**:
- If Thompson Sampling alone does not achieve target ICD (< 1.5 nats for 7-category corpus), implement Neural Thompson Sampling (NTS): add an uncertainty head to the MLP that predicts posterior variance rather than point estimates. Sample from this predicted distribution.
- Activate EWC++ after first CC@k ≥ 0.7 checkpoint.

---

## Part 5: Assessment of W3-1 as Designed in ASS-031

**Should W3-1 as designed in ASS-031 proceed, be modified, or be reconsidered?**

**Answer: Proceed with modifications. The W3-1 architecture is sound for Mode 3 (search re-ranking). It should not be the primary mechanism for Modes 1/2 (injection/briefing) — that role should go to Thompson Sampling + Category Coverage Guarantee.**

Specific modifications required:

1. **Mode 1/2 scoring**: Do not replace the injection/briefing pipeline with W3-1 GNN scoring until Thompson Sampling has been in production for at least one month and its CC@k and ICD metrics have been measured. The GNN cannot improve on what Thompson Sampling already solves better at zero training cost.

2. **IPS debiasing**: Add self-normalized IPS weighting to the `RelevanceSample` training pipeline. Propensity estimate: `P(entry surfaced | session) ≈ count(sessions where entry appeared in top-k) / count(sessions with matching topic)` from `injection_log` and `query_log`. Clip max weight at 10.0 to control variance.

3. **EWC++ timing**: Replace EWC++ with L2 regularization (weight_decay=0.001) for the first 150 training samples. Enable EWC++ after first shadow model achieves CC@k ≥ 0.7.

4. **Shadow promotion gate**: The W1-3 eval harness must include CC@k, ICD, and NEER before W3-1 can be promoted to production. P@K alone is insufficient.

5. **Mode 1/2 blend_alpha**: At `blend_alpha > 0`, Mode 1/2 scores are replaced by the GNN. This should only happen if the Thompson Sampling baseline CC@k is stable (≥ 0.6) — otherwise the GNN takes over before it has seen diverse training data and may regress coverage.

The 5-6 day effort estimate from ASS-031 is still valid for the modified scope. Stage 1 (phase-affinity + Category Coverage Guarantee + Thompson Sampling) adds approximately 1-2 days before W3-1 delivery begins.

---

---

## Part 6: Expanded Research Mandate — Self-Learning Knowledge Engine

Parts 1–5 derived the best-possible pipeline *within the constraints of the current architecture* and the feedback loop problem. This part opens the problem from first principles: **what would the best self-learning knowledge engine look like if we were designing it today?** The current architecture is an input to the answer, not a constraint on it.

### 6.1 What "Self-Learning" Means

A self-learning knowledge engine:

1. **Improves retrieval quality over time** without requiring manual curation or explicit retraining triggers.
2. **Discovers what it does not know** — identifies gaps in its knowledge base relative to the domains it is queried on.
3. **Adapts to the domain it is deployed in** — the phase-affinity and category-relevance relationships are learned, not configured.
4. **Generalizes across category/phase configurations** — the learning pipeline works regardless of what domain pack is installed.
5. **Operates at inference-time with minimal latency** — in-process, no external service calls on the hot path.

This is distinct from the "feedback loop fix" framing of Parts 1–5. The feedback loop fix is one small piece of a self-learning engine. The full picture requires answering:

- What signals feed learning? (explicit votes, implicit session outcomes, co-access, query patterns, outcome correlations)
- What is being learned? (relevance model, phase-affinity weights, category importance scores, embedding quality)
- How is the learned state maintained? (continual learning, periodic retraining, online updates)
- How is the domain-agnostic invariant preserved? (no hardcoded categories/phases in any learned artifact)

### 6.2 Rust-Native ML Ecosystem Survey (Required)

We are a Rust-first system. The question is: what neural model capabilities are available in-process in Rust, without FFI to Python, without external service calls?

**Candidate libraries/frameworks to evaluate:**

- **[ruvnet/ruvector](https://github.com/ruvnet/ruvector)** — Rust vector/neural engine; understand what it provides and whether it fits our use case
- **[burn-rs/burn](https://github.com/burn-rs/burn)** — full deep learning framework in Rust; Wgpu/CUDA/CPU backends; model export; could replace unimatrix-learn's ndarray MLP
- **[huggingface/candle](https://github.com/huggingface/candle)** — minimalist ML framework; GGML/GGUF model loading; runs transformer models in-process
- **[ort-rs/ort](https://github.com/pykeio/ort)** — ONNX Runtime bindings for Rust; already used for embedding (ONNX boundary); could also run neural re-rankers
- **[hora (hora-search)](https://github.com/hora-search/hora)** — Rust ANN library; HNSW + other index types; compare to current hnsw_rs
- **[usearch](https://github.com/unum-cloud/usearch)** — HNSW + multi-index + quantization in Rust (via C bindings); supports custom distance metrics
- **[linfa](https://github.com/rust-ml/linfa)** — sklearn-style ML in Rust; includes clustering, regression, dimensionality reduction; useful for unsupervised learning over entries
- **[smartcore](https://github.com/smartcorelib/smartcore)** — comprehensive ML in Rust; gradient boosting, SVMs, etc.
- **[tract](https://github.com/sonos/tract)** — ONNX/NNEF inference in pure Rust; alternative to ort

**For each candidate, evaluate:**
1. Can it run inference in-process, on a CPU-only server, without Python or external services?
2. What is the latency for a forward pass on a small model (< 5M params) at serving time?
3. Does it support quantized models (INT8/FP16) to reduce memory footprint?
4. Can it be trained (or fine-tuned) in-process, or inference-only?
5. What is the ecosystem maturity (release cadence, GitHub stars, production usage evidence)?

### 6.3 Novel Approaches to Research (Beyond Parts 1–5)

Parts 1–5 covered: Thompson Sampling bandits, IPS debiasing, EWC++ continual learning, MLP re-ranking, recommender system coverage metrics. These are all valid. The following are *additional* directions to investigate that may produce better results:

**Direction 1 — Late Interaction Models (ColBERT-style)**

ColBERT stores per-token embeddings and defers interaction to retrieval time. For a knowledge engine where documents are short (knowledge entries, not full documents), late interaction could give significantly better re-ranking precision than a single dense vector. The question is: does this work at our corpus scale (10K entries) with in-process Rust compute?

**Direction 2 — Splade / Sparse-Dense Hybrid Retrieval**

SPLADE expands sparse token weights using a masked language model, then uses inverted-index retrieval. Dense vectors for recall, sparse for precision. The hybrid achieves near-perfect recall on out-of-domain queries. For Unimatrix, this is relevant because some knowledge entries use domain-specific terminology that pure dense retrieval misses.

**Direction 3 — Graph-Augmented Retrieval**

The existing co-access graph captures which entries are accessed together. This is one form of graph signal. A fuller knowledge graph could encode: entry A contradicts entry B, entry C supersedes entry D, entry E is a prerequisite for understanding entry F. Retrieval could traverse this graph (multi-hop) to surface prerequisite and contextual knowledge alongside the directly matched entry. Relevant prior art: GraphRAG (Microsoft), KG-RAG.

**Direction 4 — Instruction-Tuned Relevance Scoring**

Rather than a trained MLP, a small instruction-tuned LLM (running via candle or ort in-process, < 1B params, quantized) could score entry-query pairs directly. The model prompt: "Given that an agent is in phase X performing role Y, is the following knowledge entry relevant? Entry: {text}. Answer: yes/no/partial". This eliminates the need for labeled training data — the model's instruction-following transfers zero-shot.

**Direction 5 — Self-Supervised Contrastive Learning for Embedding Quality**

The current embedding pipeline uses a frozen sentence-transformers ONNX model. A self-supervised contrastive learning step could fine-tune embeddings using in-corpus pairs: (entry, its correction chain) as positive pairs, (entry, contradicted entry) as negative pairs. This would make the embedding space more semantically precise for the actual knowledge domain without requiring labeled data.

**Direction 6 — Active Learning for Knowledge Gap Detection**

Rather than passively waiting for queries to reveal underrepresented knowledge, an active learning loop could: (a) cluster existing queries by topic, (b) identify clusters with low coverage (few high-confidence entries), (c) surface these clusters to the human operator for targeted knowledge addition. This turns the system from reactive (learn from what happens) to proactive (identify what's missing).

### 6.4 Research Framing

This is a **research-only** expanded mandate. The deliverable is not an implementation plan — it is a research report that answers:

1. **Current capabilities audit**: What does the current search/scoring pipeline actually do, end-to-end? Every component, every weight, every feedback channel. No assumptions.
2. **Rust ML options**: For each direction in 6.3, which Rust libraries could enable it? What are the realistic implementation costs?
3. **Novel approach assessment**: For each direction in 6.3, does external research (papers, production systems) support it as a better path than the current Thompson Sampling + MLP hybrid? What is the evidence?
4. **Recommended next steps**: Given the full picture, what is the best architecture for a self-learning, domain-agnostic Unimatrix? This may or may not look like the Part 4 proposed pipeline.

---

## Goals

1. Correct the corpus size assumption in all downstream design artifacts.
2. Identify the best pipeline architecture for Unimatrix's use case, with external research backing.
3. Evaluate the current architecture honestly against the identified best pipeline.
4. Produce a concrete component evaluation table (keep/replace/add with rationale).
5. Define the evaluation framework (what metrics, what constitutes improvement).
6. Determine whether W3-1 as designed should proceed, be modified, or be reconsidered.
7. **[New]** Fully audit the current search/scoring pipeline end-to-end: every component, weight, feedback channel, and path through the code.
8. **[New]** Survey the Rust ML ecosystem for viable in-process neural model options (burn, candle, ort, ruvector, usearch, etc.).
9. **[New]** Research novel self-learning knowledge engine approaches (late interaction, sparse-dense hybrid, graph-augmented retrieval, active learning, self-supervised embeddings) with evidence from papers and production systems.
10. **[New]** Produce a domain-agnostic framing — all designs must work for any (categories, phases) configuration, not just the software development domain pack.

---

## Non-Goals

- This spike does not implement any of the proposed changes. All implementation is downstream delivery work.
- This spike does not evaluate MicroLoRA or embedding adaptation — those are out of scope (separate pipeline).
- This spike does not define the exact IPS propensity estimation implementation — that is a W3-1 delivery detail.
- This spike does not redesign the confidence score formula — that is a separate crt-phase feature.
- This spike does not propose removing the existing confidence system — it proposes decoupling usage_score from the exploration-critical path.

---

## Acceptance Criteria

- AC-01: Corpus size assumption from stale MEMORY.md is corrected with live database query results.
- AC-02: External research covers agent memory systems (Letta, Mem0, Zep, MemOS), learning objectives (bandit vs. supervised vs. preference learning), evaluation metrics beyond P@5/MRR, and the exploration problem.
- AC-03: Evaluation framework proposes at minimum three metrics that are not P@K or MRR, each with a definition, formula, and rationale tied to the use case.
- AC-04: Component evaluation table covers all eight mechanisms (semantic similarity, confidence, co-access, EWC++, MicroLoRA, W3-1 GNN, phase-affinity routing, exploration epsilon).
- AC-05: The proposed pipeline is opinionated — a concrete recommendation, not a menu of options.
- AC-06: W3-1 go/no-go decision is explicit with specific conditions for the modifications required before delivery.
- AC-07: The feedback loop concern from the original ASS-032 is quantified with live corpus data (not assumed corpus state).
- AC-08: EWC++ risk in biased training setting is evaluated with reference to external research on EWC failure modes.
- **AC-09 [New]**: Current search pipeline is documented end-to-end: candidate retrieval path, every scoring component, re-ranking, serving modes (injection/briefing/search), and all feedback channels back into learning state.
- **AC-10 [New]**: At least 5 Rust-native ML/neural libraries are evaluated for feasibility against the criteria in Section 6.2 (in-process inference, latency, quantization, training support, maturity). ruvector must be included.
- **AC-11 [New]**: Each of the 6 novel directions in Section 6.3 is assessed: summarize what the approach does, cite external evidence for/against, evaluate Rust implementation path, and give a recommendation (pursue/defer/reject) with rationale.
- **AC-12 [New]**: All proposed designs are framed in terms of the configurable (categories, phases) abstraction — no hardcoded category names or phase names in any recommendation.
- **AC-13 [New]**: A final synthesis section gives an opinionated answer to "what is the best architecture for a self-learning, domain-agnostic Unimatrix?" — may supersede or extend the Part 4 proposed pipeline.

---

## Constraints

- **Schema version v16 (post-col-025)**: Any new tables require a migration. Stage 1 changes (phase-affinity matrix, Thompson Sampling) require no schema changes. Stage 2 (W3-1 delivery) requires `session_category_snapshots` table per ASS-031/OQ-01.
- **No additional ML libraries**: All options use existing infrastructure (`unimatrix-learn`, ndarray, bincode). Thompson Sampling over Beta posteriors requires no additional libraries.
- **In-process operation**: No external service dependencies. All ML and bandit state maintained in-process, persisted to SQLite.
- **Rayon pool**: Shared across NLI, embedding, and GNN training. Thompson Sampling sampling at serving time is CPU-trivial (single Beta sample per candidate). No rayon involvement needed.
- **`w_phase_explicit` is currently 0.0**: Stage 1 activates this placeholder. The field name is stable (ADR-003). W3-1 will eventually replace the static matrix with learned scores.
- **Corpus is deployment-variable**: All architectures must be correct across small (10s) through large (10,000s+) corpora. The live deployment (2982 active entries) is a mid-range example. The lesson-learned saturation problem is likely to be common across deployments as agents generate lesson-learned entries during retros.

---

## Open Questions

**OQ-F [New, High Priority]**: `context_get` is the strongest implicit positive signal in the system — it means the agent selected a specific entry from search/briefing results and fetched it in full. Yet `current_phase` is always `None` for gets (policy in `usage.rs:69`). The phase-conditioned frequency table cannot be built from get-signals because the phase dimension is stripped. Fix: snapshot `current_phase` from `SessionState` at get-time (same pattern as `context_store`, tools.rs:519–531) and raise `access_weight` for gets to differentiate them from search appearances. Add `confirmed_entries: HashSet<u64>` to `SessionState` to track "agent chose this" vs. "agent was shown this."

**OQ-G [New]**: The signal weight hierarchy needs formal specification. A `context_get` should contribute more to the phase-conditioned frequency table than a search appearance, and less than an explicit `helpful=true` vote. Proposed: search_appearance=1, context_get=3, context_lookup=2, explicit_helpful=10. These weights feed both the phase-conditioned table and the per-(phase, entry) Beta posteriors in Thompson Sampling.

**OQ-A**: The phase-affinity matrix initial values are illustrative. What is the correct calibration for a software development orchestration workflow? Recommendation: extract from the live `FEATURE_ENTRIES` table and `observations` table — which phases correlate with which category stores? This is a data-driven calibration step, not a design question.

**OQ-B**: The 0.7/0.3 blend between fused_score and Thompson Sampling in the injection pipeline is a proposed default. Should this be configurable? Recommendation: yes, expose as `[injection] ts_blend = 0.3` in `InferenceConfig`. Start at 0.3 and tune based on ICD metrics.

**OQ-C**: The Category Coverage Guarantee (force-include one entry per missing category) may surface irrelevant entries if a category has no semantically relevant entries for the current session. A similarity floor (e.g., cosine > 0.2) should gate inclusion. Define this threshold.

**OQ-D**: IPS weight clipping at 10.0 is a heuristic. The self-normalized IPS variant (SNIPS) divides weights by their sum, which automatically controls for extreme weights. Recommend SNIPS over naive clipping.

**OQ-E**: The `session_category_snapshots` table (OQ-01 from ASS-031) is a prerequisite for W3-1 training. Stage 1 does not require it. Stage 2 does. This dependency is unchanged.

---

## Background Research

### Codebase Findings

- **ASS-031**: Full W3-1 design spike; GNN architecture is a graph-feature-enriched MLP RelevanceScorer (5121 params); feature vector is 49 dims (k=7); training design uses BCE loss with EWC++ and reservoir sampling; cold-start blend_alpha ramp from 50 to 150 samples.
- **Live corpus (2026-03-25)**: 2982 active entries across 7 categories. Lesson-learned: 2263 entries (76% of corpus), only 6% accessed, avg confidence 0.096. Decision: 320 entries (11%), 96% accessed, avg confidence 0.528. The confidence gap between categories is 0.43 — a direct consequence of the feedback loop operating on the actual corpus for an extended period.
- **search.rs**: `compute_fused_score` with `FusionWeights` — `w_phase_explicit = 0.0` is a live production placeholder.
- **index_briefing.rs**: `IndexBriefingService::index()` — no category diversity enforcement, pure score-sorted top-k. Default k=20.
- **eval/runner/metrics.rs**: P@K and MRR only. No coverage or diversity metrics.
- **EWC failure mode (EWC Done Right, 2026)**: Vanishing FIM when model achieves high confidence → over-protects early patterns → biased training amplified. Resolved by Logits Reversal, but the simpler fix for Unimatrix is to delay EWC++ activation.

### External Research Findings

- **Mem0 (2025)**: Pure semantic similarity retrieval, temperature-0 deterministic, no exploration mechanism. Solves recall of past interactions, not proactive surfacing.
- **Zep**: Temporal knowledge graph + vector search. No exploration. 600K+ token memory footprint. Post-ingestion retrieval fails immediately.
- **MemOS (2025)**: Hierarchical memory OS with active lifecycle management. Most similar to Unimatrix's use case. MemScheduler combines recency, frequency, semantic, and contextual signals. Explicitly addresses cold-start via MemGovernance.
- **Facebook Reels Epinet (2024)**: Thompson Sampling via epistemic neural networks for content cold start. Deployed on billions of impressions. Key finding: new content (< 10,000 impressions) with sparse feedback requires uncertainty-aware exploration to avoid the cold-start popularity trap. Epinet = base MLP + uncertainty head. Direct analog to W3-1 Neural Thompson Sampling proposal.
- **Scalable Interpretable Contextual Bandits (2025)**: Thompson Sampling matches or exceeds UCB in practice. LinUCB is theoretically optimal in linear settings. Epsilon-greedy is simplest. For Unimatrix corpus size (1K-10K entries), Thompson Sampling per-entry is tractable.
- **IPS in production (2025)**: Self-normalized IPS (SNIPS) is more stable than raw IPS due to variance control. IPS-weighted BPR in e-commerce shows significant improvement in long-tail item coverage without degrading top-item metrics.
- **Category coverage as a metric (recommender systems literature)**: `Coverage = unique_recommended / total_available`. Essential complement to precision/recall. Low coverage indicates popular-item collapse. Formula: `CC@k = |{cat with ≥1 entry in top-k}| / |all_categories|`.
- **Diversity metrics**: Intra-list diversity (average pairwise dissimilarity of results), catalog coverage, novelty (1 - popularity), serendipity (unexpected + relevant). Shannon entropy of category distribution is a practical implementation of intra-list category diversity.
- **EWC++ limitations (2024-2026)**: Standard EWC underperforms in class-imbalanced continual learning by prioritizing dominant classes. The Logits Reversal fix (EWC-DR) achieves 53% improvement. For Unimatrix, the simpler mitigation is to delay EWC++ until the training data is representative.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for knowledge surfacing pipeline, retrieval ranking, feedback loop, contextual bandit, exploration exploitation. MCP server tools were not available via tool call; server is running (PID 10893) but not reachable from this session's tool interface. The query intent was executed by reading the codebase directly and conducting external web research.
- Stored: Deferring to post-review pattern storage. Three patterns are candidates for `/uni-store-pattern`:
  1. "Category Coverage Guarantee as injection diversity floor" — reusable pattern for any injection service.
  2. "Thompson Sampling over Beta priors for knowledge surfacing exploration" — reusable pattern for agent memory systems.
  3. "Delay EWC++ until training data is representative (CC@k gate)" — reusable pattern for continual learning with biased training distributions.
  These will be stored after the human reviews this research and a delivery path is confirmed.

---

## Sources

- [Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory (arXiv 2504.19413)](https://arxiv.org/html/2504.19413v1)
- [MemOS: A Memory OS for AI System (July 2025)](https://statics.memtensor.com.cn/files/MemOS_0707.pdf)
- [Zep: A Temporal Knowledge Graph Architecture for Agent Memory](https://www.researchgate.net/publication/388402077_Zep_A_Temporal_Knowledge_Graph_Architecture_for_Agent_Memory)
- [Epinet for Content Cold Start (arXiv 2412.04484) — Facebook Reels Thompson Sampling deployment](https://arxiv.org/html/2412.04484v1)
- [Scalable and Interpretable Contextual Bandits: A Literature Review and Retail Offer Prototype (arXiv 2505.16918)](https://arxiv.org/html/2505.16918v1)
- [Elastic Weight Consolidation Done Right for Continual Learning (arXiv 2603.18596)](https://arxiv.org/html/2603.18596)
- [Counterfactual Risk Minimization with IPS-Weighted BPR (arXiv 2509.00333)](https://arxiv.org/html/2509.00333v1)
- [Unbiased Learning to Rank: Counterfactual and Online Approaches (WWW 2020 tutorial)](https://ilps.github.io/webconf2020-tutorial-unbiased-ltr/WWW2020handout.pdf)
- [Beyond Accuracy: Evaluating Recommender Systems by Coverage and Serendipity (Herlocker et al. 2004)](https://www.researchgate.net/publication/221140976_Beyond_accuracy_Evaluating_recommender_systems_by_coverage_and_serendipity)
- [On (Normalised) Discounted Cumulative Gain as an Off-Policy Evaluation Metric (arXiv 2307.15053)](https://arxiv.org/abs/2307.15053) — nDCG inconsistency in off-policy evaluation
- [Bias and Debias in Recommender System: A Survey and Future Directions (ACM TOIS)](https://dl.acm.org/doi/10.1145/3564284)
- [Reducing Popularity Influence by Addressing Position Bias (RecSys 2024)](https://ceur-ws.org/Vol-3924/short4.pdf)
- [Graph Memory for AI Agents (Mem0, January 2026)](https://mem0.ai/blog/graph-memory-solutions-ai-agents)
