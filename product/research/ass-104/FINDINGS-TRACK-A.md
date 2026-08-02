# FINDINGS: ass-104 Track A — Internal retrospective: the OPTION LEDGER

**Spike**: ass-104 (Track A of 4) | **Date**: 2026-07-20 | **Approach**: investigation (document mining) | **Confidence**: DIRECTIONAL

Mined all 34 spikes named in SCOPE.md + ass-103, every `.md` read directly (alt-format dirs opened by filename). Supplemented with Unimatrix reads — several dispositions exist **only** in Unimatrix (ADRs/capability nodes), not in any spike file.

**Legend**: ADOPTED (chosen/shipped) · DEFERRED (explicitly postponed) · REJECTED (ruled out) · **LATENT** (designed/present as a knob but not exposed/wired/active) · UNSTATED (source states none — never inferred).

**Coverage exceptions**: **ass-042**, **ass-079**, **ass-090** are SCOPE-only — **no FINDINGS exist**. ass-041/051/061 are on-theme only at the margins.

---

## 1. Graph / edges

### 1a. Edge model and typing

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| Typed graph — `RelationEdge{relation_type, weight, created_at, created_by, source}` in `GRAPH_EDGES` | ass-022/04 §1 | ADOPTED | "Yes — pays off immediately" |
| Five relation types: Supersedes/Contradicts/Supports/CoAccess/Prerequisite | ass-022/04; UM #2417 | ADOPTED | Replaced `StableGraph<u64,()>`; per-type weight semantics |
| Penalty logic filters to **Supersedes only**; other edges structurally invisible to penalty | UM #2417 §2 | ADOPTED | "until W3-1 GNN integrates them" — GNN never arrived, **exclusion still live** |
| `graph_edges.metadata` TEXT column for **GNN per-edge feature vectors** | UM #2417 §4 | **LATENT** | Added so W3-1 needs no migration. W1-1 writes no values. GNN never built |
| `bootstrap_only=1` edges excluded at construction, promoted only by NLI | UM #2417 §3/§5; ass-030 §F9 | **LATENT/orphaned** | "no documented process for when bootstrap_only edges are 'complete'… permanent underclass" — and the NLI promoter was later deleted as dead code |
| `RelationType::Informs` (cosine 0.45–0.65 + category-pair + cross-feature + temporal) | ass-034 F4–F7 | ADOPTED | Isolation projected 79.5%→~50-55%; live at 83–85 edges |
| `Prerequisite` reused as `depends_on`, stored A→B | ass-055 Q1 | ADOPTED | "PPR direction is already correct and tested" |
| `RelatedTo` edge type | ass-074 SCOPE (vnc-015) | **LATENT** | In the PPR positive set; **0 edges written**. "cheapest diversity lever" — DEFERRED |
| Broad taxonomy (InformedBy, ImplementsDecision, CausedBy, AppliedIn, Mentions, RelatedWork…) | ass-034 RQ-1 | REJECTED (collapsed) | "collapsed the taxonomy to one new edge type (Informs)" |
| Ten research-domain types (Advances, Cites, Asserts, Refutes, Tests, DerivedFrom, Motivates, About…) | ass-057 §2/§7 | DEFERRED (W3 Ph.1) | "~40 lines total in graph.rs. No schema migration" |
| Config-extensible taxonomy (open `RelationType::Extended(String)`) | ass-034 RQ-3 | REJECTED/DEFERRED | "Requires RelationType to become open… bigger architectural change" |
| Explicit `context_relate` tool | ass-034; ass-055 Q2 | REJECTED | "Adds a 13th tool… potential for omission" |
| `depends_on` param on `context_store`/`context_correct` | ass-055 Q2 | ADOPTED (Go, W2) | GRAPH_EDGES-only, no migration; "co-located with entry definition" |
| Unified `edges` parameter generalizing to all typed edges | ass-057 §2b | DEFERRED (W3 Ph.1, "ships first") | "Without it, both SDLC and research graphs stay sparse" |
| Bidirectional storage for symmetric edges | ass-057 §2; UM #4987 | ADOPTED (Contradicts) / DEFERRED (RelatedTo) | Contradicts shipped via `validate_and_write_edges` |
| Bidirectional `Prerequisite` (closes a `graph_expand` gap) | ass-055 OSD#1 | DEFERRED | "Low-effort additive change if the gap proves significant" |
| `strength`/`confidence` mapped onto existing `weight` | ass-057 §2 | ADOPTED (design) | "zero JSON parsing" |
| Edge weight **not used** in `context_get` edge ranking | UM #5054 (ADR-006 vnc-037) | ADOPTED | Authored-first, then by target-entry confidence |
| Source-ownership + confidence-floor guard on agent edge writes | ass-055 Q5; ass-057 §2b | ADOPTED (design) | "PPR seeded on A then surfaces B, inflating B's apparent relevance" |
| Status-as-typed-edge mechanism | ass-093 Q4 | REJECTED | "status is a property of one node, not a relation… pollutes traversal/PPR graph" |

### 1b. Edge inference sources (S-series)

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| **S1** tag co-occurrence (≥3 shared tags) → Informs | ass-038; ass-040 Grp3 | ADOPTED | 1,052 edges, 30.5% cross-category |
| S1 threshold ≥3 **hardcoded in SQL, not configurable** | ass-057 FINDINGS-C §3 | **LATENT** | Named as a tunable that isn't tunable |
| **S2** structural vocabulary (≥2 domain terms) → Informs | ass-038; ass-040 Grp3 | ADOPTED | 1,830 edges, 62.8% cross-category |
| S2 `s2_vocabulary` **defaults empty** → no-op out of the box | ass-103 op 9b; ass-057 §6 | **LATENT** | "Zero-code-change automatic Informs edges — *populate s2_vocabulary*" |
| **S3** keyword overlap | ass-038; ass-040 | REJECTED→DEFERRED | 19 sessions/47 entries; "re-evaluate at corpus ≥3,000" |
| **S4** lexical citation detection | ass-038 | REJECTED | 4–9 pairs |
| **S5** supersession chain topology | ass-038 | REJECTED | Only 2 active→active links survive |
| **S6** outcome co-retrieval | ass-038; ass-040 | REJECTED (UNTESTABLE)→DEFERRED | 0 pairs — outcome_index all quarantined |
| **S7** briefing-selection endorsement | ass-038; ass-040 | REJECTED (UNTESTABLE)→DEFERRED | `audit_log.session_id` empty for all briefing calls |
| **S8** search co-retrieval → CoAccess @0.25 | ass-038; ass-040 | ADOPTED | 2,770 edges, 21.3% cross-category; watermark batch |
| **S9** cross-feature temporal clustering | ass-038 | REJECTED (redundant) | "SUBSET OF S1" — 52% overlap, zero additive edges |
| **S9′** convergent-citation (research domain) | ass-057 OSC-1 | DEFERRED | "after Cites edges confirmed written" |
| **S10** graph centrality as node weight | ass-038 | UNSTATED | "NOT COMPUTED" |
| All generated edges carry `signal_origin` | ass-038; ass-040 | ADOPTED (methodology) | "do not inject unlabeled edges" |
| Text-reference/ID-mention regex (`Mentions`) | ass-034 RQ-2 | **LATENT** | "Zero ML required… precision very high… coverage low" |
| Correction-chain reason-field keyword scan | ass-034 RQ-2 | **LATENT** | Proposed only |
| LLM annotation at store-time | ass-034 RQ-2 | REJECTED (architecturally) | "Unimatrix itself is LLM-agnostic" |
| Relation-extraction model (REBEL) | ass-034 | REJECTED | "Overkill now… Deferred" |
| Automated dependency detection from content | ass-055 OSD#2 | DEFERRED | "W3 opportunity in the NLI/structural detection pipeline" |
| Behavioral Informs from `context_cycle_review`, outcome-weighted (1.0/0.5), 200-cap | ass-039; ass-040 Grp6 | ADOPTED | "closes the self-sustaining loop" |
| Behavioral Informs **first-write-wins weight freeze** | ass-079 thesis #4 | **UNSTATED — spike never ran** | Flagged as a problem to test; never tested |
| Retire `Informs` / keep-and-fix (accumulate-decay) / redefine / status quo | ass-079 RQ-5 a–d | **UNSTATED — spike never ran** | **Four ranked options, no verdict.** Largest unresolved graph question |

### 1c. Traversal and retrieval use

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| Personalized PageRank over Supports+CoAccess+Prerequisite | ass-032 Task5; #398 | ADOPTED | HippoRAG "20% multi-hop"; gate passed (CC@5 0.4244≥0.3659, ICD 0.6381≥0.5341) |
| PPR as re-ranker within k=20 | ass-037 Q2 | REJECTED (at that scale) | `ppr_blend_weight=0.0` — disabling produced identical P@5/MRR; graph 63% isolated |
| Increasing edge density alone as the PPR fix | ass-038 | **REJECTED (disproved)** | 6.2× density (1,086→6,738) → **zero PPR delta** on 2,376 scenarios |
| PPR redesigned as **candidate-set expander** (BFS from HNSW seeds) | ass-038 Rec#2; crt-042 | ADOPTED (built+enabled) | "PPR can only change relative ranking of entries already in k=20"; 6/10 ground-truth outside k=20 |
| Enable `ppr_expander_enabled=true` in prod | ass-074 Q2 | ADOPTED | Human capped-risk decision 2026-06-10 |
| Zero `ppr_blend_weight`/`ppr_max_expand` (ass-037's literal rec) | ass-074 Q1 | **REJECTED — overturned by measurement** | "contradicted by the run… drops co-access" |
| Weight CoAccess/Informs above `cosine_supports` in the walk | ass-074 Q1 | DEFERRED | "would move CC@5/ICD, which cosine_supports cannot" |
| Add Advances/Motivates to PPR positive set | ass-057 §5; ass-074 | DEFERRED / REJECTED-for-SDLC | "~10 lines"; kept out per vnc-018 lesson #4495 |
| Contradicts excluded from traversal/PPR | ass-092 Q1/Q2 | ADOPTED | Confirmed, kept as-is |
| Contradicts collision suppression (post-scoring filter) | ass-032 #395 | ADOPTED | "CC@5 and ICD both improved" |
| Leiden/Louvain community detection | ass-032 Task5 | DEFERRED | "worthwhile above ~500 active entries" — trigger never re-checked |
| `graphrag-rs` crate | ass-032 Option6 | REJECTED | "Low adoption; documentation reads promotional" |
| Consolidated `context_graph` (7 modes) | ass-057 §3 | DEFERRED (W3 Ph.2) | "Eight traversal APIs collapse to one" |
| Filtered-PPR probe tool | ass-037 Q3 | **LATENT (built, unused)** | At `ass-037/tools/informs_probe/`; gated on corpus ≥5K or density ≥1.0 |
| Surface depth-1 typed edges on `context_get`, default-on, cap 10 | ass-076 RQ-1/RQ-5 | ADOPTED | Opt-in rejected — "defeats zero-edge-becomes-visible" |
| Distinguish inferred vs authored via `graph_edges.source` | ass-076 RQ-4 | ADOPTED | Read-path only |
| DOT/GraphViz export | ass-022/04 | UNSTATED | Listed as "unlocked", no commitment |

### 1d. Edge consistency under correction — ass-088 full option set

| Option | Disposition | Stated reason |
|---|---|---|
| **Synchronous-at-write redirect** as the guarantee | **REJECTED as sole guarantee; demoted to optional fast-path** | "cannot own correctness for hot nodes (its ceiling is the defect)" |
| Raise/configure `REDIRECT_CEILING` | **REJECTED** | "that only defers the same failure" |
| Keep vs delete write-side `run_redirect_loop` | **UNSTATED** — flagged for the ADR | Leans "delete to avoid two mechanisms" |
| **Read-time resolution** (vnc-042 default-on) | **ADOPTED as interim cover — insufficient alone** | "a permanent tax… does not repair the stored graph" |
| `context_graph.resolve_supersessions` **default-OFF** | **LATENT — flip recommended** | "must be completed" |
| NG-1 stale edge-target labels on `context_get` | ADOPTED (ships with the ADR) | Read-side gap must close for read-cover to hold |
| **Background-tick convergence** (repoint-then-compact, extends Job 2) | **ADOPTED as owner of stored-graph correctness** | "extension of proven infra, not greenfield" |
| Scan-driven bounded `LIMIT B` | ADOPTED | "Never a full-corpus scan; drains over ticks" — answers #625 |
| Work-queue-driven convergence | DEFERRED | "the enterprise-scale optimization if the scan's cost ever dominates" |
| New `EdgeConvergenceJob` vs extending Job 2 | **UNSTATED** — "choose in the ADR" | Two viable shapes |
| Convergence **before compaction** (repoint precedes delete) | ADOPTED | Load-bearing correction to its own hypothesis |
| Convergence **before `typed_graph_rebuild`** | ADOPTED | "no separate invalidation, no extra rebuild" |
| SLO in **ticks, not wall-time** | ADOPTED | "wall-time SLOs flake (cf. #790/#833)" |
| Per-edge audit logging | REJECTED | "no per-edge audit… optional per-tick summary" only |
| **#744** inbound-orphan ceiling standalone | ADOPTED (retired/folded in) | Ceiling stops being the guarantee |
| **#745** outbound historical drops | **DEFERRED** | "UNRECOVERABLE by any convergence scan… separate optional pass, or product-accept" |
| Compaction **deletes rather than repoints** non-Active edges | **LATENT (flagged defect)** | "a latent integrity defect" — independently re-found as ass-103 I-1/I-2 |
| Quarantine: deletion-only convergence | ADOPTED | "deletion-only is correct" |
| Accept eventual staleness window | ADOPTED | Conditional on read-cover completion |
| Batch budget `B` value | DEFERRED | Pending correction-rate measurement |

**Verdict**: hybrid, no single winner — *"write stays cheap · reads resolve on demand · a bounded per-tick sweep converges the stored graph."*

### 1e. Edge demotion — the standing hole

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| Any count- or score-aware **edge deleter** | ass-103 I-4, B-2 | **DOES NOT EXIST** | "No module in the crate deletes an edge when its count or score falls back below threshold." B-2: "The problem is not scattering — it is the **absence** of a count-aware deleter" |
| Co-access GC deletes source `co_access` row at 365d, never the derived `graph_edges` row | ass-103 I-4 | **LATENT defect** | Promoted edges persist forever with no owner |
| Dedup pre-filters exclude already-written pairs from rescoring | ass-103 I-4 | ADOPTED (with this consequence) | Once written, never re-evaluated |
| Informs weight accumulate/decay vs first-write-wins | ass-079 RQ-6 | **UNSTATED — never ran** | The question that would have owned this |
| Tune co_access promotion threshold (31,124 raw → 444 edges) | ass-074 OSD | DEFERRED | "a second density/mix lever" |
| Near-threshold oscillation must be specified in ACs | UM #3822 | ADOPTED (rule) | ass-103 I-12 finds auto-quarantine still undamped |

---

## 2. Learning signal

### 2a. Fused retrieval formula

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| `w_sim=0.50`, `w_conf=0.35` (Wilson composite) — "conf-boost-c" | ass-037; ass-039; ass-040 | ADOPTED | +0.0241 MRR (+9%) on valid ground truth |
| Formula holds at scale — no re-ablation | ass-074 Q4 | ADOPTED | "no drift signal" |
| `w_nli` | ass-037 Q7; ass-039 | **REJECTED (zeroed)** | Zero contribution, then net-negative (−0.0029 MRR) |
| `w_util`, `w_prov` | ass-037; ass-039 | REJECTED (zeroed) | "Redundant: subsumed by confidence composite" |
| `w_coac` direct co-access boost | crt-032 #3785; ass-040 | ADOPTED→REJECTED | "moved entirely to PPR graph topology". **ass-074 warns: do not zero PPR without addressing this** |
| `w_phase_explicit` | ass-037; ass-032 #414 | LATENT → **ACTIVATED** | Reserved placeholder always 0.0 ("must not be removed, ADR-003"); activated by the learned frequency table. +0.0029 MRR |
| `w_phase_histogram` (real-time) | ass-037; UM #3175 | ADOPTED | 0.02, full session signal budget |
| Static hand-authored `phase_affinity[phase][category]` | ass-032 §4.2 | REJECTED/superseded | Superseded by the learned table shipped |
| Keep the 6-factor **formula** fixed; learn only the **weights** | ass-022/04 §2 | ADOPTED (constraint) | "do not replace the formula. Learn the weight vector" |
| Signal fusion as sequential sort passes | UM #2964 | REJECTED (anti-pattern) | "later additive passes can override earlier semantic signal" |

### 2b. Co-access — the four-way overlap

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| **MicroLoRA** adaptation (pre-HNSW, rank 2–8, ~3072 params) | UM #181, #702 | ADOPTED then transitional | Shipped crt-006 (Option D: ship-then-evaluate). Architecture explicitly **transitional** |
| **Co-access scalar boost** (+0.03 search / +0.01 briefing, log-transform) | UM #702 | ADOPTED then absorbed | "The 0.03 cap and log-transform should be considered **provisional**" |
| **Episodic augmentation** (embedding-space blending) | UM #188, #702 | **REJECTED — shipped as a no-op stub, then removed** | "No distinct signal exists to drive it independently of co-access… would triple-count if activated" |
| `co_access_affinity()` + W_COAC | UM #702 | REJECTED (dead code, removed) | Dead path |
| MicroLoRA-vs-boost overlap evaluation (col-015, GH #50) | UM #702 | **DEFERRED — never closed** | "remains an open question" |
| Contrastive (InfoNCE) learning from co-access pairs | UM #181; ass-015 | ADOPTED (unimatrix-adapt) | "Co-access data from crt-004 is the contrastive training signal" |
| Co-access staleness 30d→365d | ass-032 #408 | ADOPTED (hotfix) | "System is ~30 days old" |
| Co-access promotion (≥3) as a cross-feature bridge | ass-034 F6 | REJECTED (insufficient alone) | "fundamentally cannot bridge the 'never co-accessed' gap" |

### 2c. Confidence, effectiveness, voting

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| Wilson composite, Beta(3,3) prior → neutral 0.5 on zero votes | ass-057 §5/D | ADOPTED | "No crash, no invalid state" |
| Empirical Bayesian priors + adaptive `w_conf` spread (crt-019) | ass-022/01 | ADOPTED | "an excellent generic design" |
| Confidence asymmetry — auto-positive, flag-negative, **never auto-downweight** | UM #203 (ASS-014) | ADOPTED | Standing decision |
| Split objective quality from popularity in the composite | ass-032 §3.2 | DEFERRED | "Keep but reframe… Consider decoupling" — not executed |
| Circular usage_score feedback loop | ass-032 §3.1 | UNSTATED (fix proposed) | "94% of lesson-learned entries have never been accessed… confidence gap 0.43" |
| Exposure bias — never-surfaced entries can't bootstrap feedback | UM #3429 | ADOPTED (named hazard) | Disqualified the ass-031 GNN's label source |
| Observation-only confidence window (5 features post-extraction) | ass-015 §5.5 | ADOPTED | Breaks the feedback loop |
| Novelty bonus — successful deviation lowers the convention's confidence | ass-015 §5.2 | ADOPTED | Mitigation for "Echo Chamber Effect" |
| Self-referential access-rate metric (>80% flags a loop) | ass-015 §5.5 | **LATENT** | No confirmation of instrumentation |
| Implicit votes: write votes **before** marking applied | UM #1616, #1614 | ADOPTED | Double-write is "the lesser evil"; silent loss is "undetectable and uncorrectable" |
| Explicit read logging (`context_get`/`context_lookup`) | ass-040 Grp9 (crt-049) | ADOPTED | "entries returned in a search result set were not necessarily opened or used" |
| access_weight recalibration (briefing=0, get=2, lookup weighted) | ass-032 | UNSTATED | Values not confirmed shipped |
| Signal weight hierarchy (search=1, get=3, lookup=2, helpful=10) | ass-032 OQ-G | **UNSTATED** | "needs formal specification" — never given |
| `confirmed_entries: HashSet<u64>` ("chosen" vs "shown") | ass-032 OQ-F | UNSTATED | Not in ROADMAP Completed |
| `context_correct` **hard-resets** confidence/access_count/helpful/unhelpful | ass-093 Q2 | **UNSTATED (flagged)** | "silent learning-history loss on every content edit may be under-considered" |
| In-place mutation preserves learning signal | ass-093 Q2 | ADOPTED | "the decisive argument for in-place" |
| Salience re-ranking by **applied-causality** not served-frequency | ass-091 Q2 | ADOPTED (headline) | Validated against retro bugfix-891 |
| Applied-attribution from GH stewardship prose | ass-091 → ass-090 | DEFERRED | Content-opaque per #5030; feeds ass-090 (**never ran**) |

### 2d. Neural / ML

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| W3-1 GNN — graph attention over candidate graph | ass-029 Q1 | REJECTED (architecture) | ass-031 chose MLP: "too small to benefit from learned message passing" |
| **The ass-031 GNN** — 3-layer ~5121-param MLP, full tick integration, training loop, 3-slot promotion, cold-start blend | ass-031 (all 5 docs) | **LATENT — fully designed, never built** | Superseded by ass-032's de-scoping |
| De-scope GNN: keep Mode-3 MLP, **replace Mode 1/2 with Thompson Sampling** | ass-032 §2.2/2.4 | ADOPTED (redirection) | Feedback loop: "training labels from entries the current formula surfaced only" |
| GNN training-data readiness | ass-038; UM #3991 | DEFERRED ("NEAR-PASS") | 6,738 labeled edges, 5 origins; non-CoAccess coverage 57.8% vs 60% target |
| GNN confidence learning (learn 6-factor weights + freshness half-life) | ass-022/04 §2 | DEFERRED | "not enough signal at local scale… only meaningful after 100+ helpfulness votes" |
| Learned (phase,category) weights as GNN cold-start init | ass-040 Grp10 | **LATENT** | Accessor available; GNN deferred |
| `GnnFeatureCache`/`EntryFeatureCache` rebuilt per tick | ass-031 §6 | **LATENT** | Fully designed, never built |
| Graph degree features split by edge type; 17-dim entry vector | ass-031 §2.2 | **LATENT** | Designed, never wired |
| `days_since_access` normalized feature (30-day clip) | ass-031 §2.1 | **LATENT** | Designed for GNN input, never wired |
| Two-headed architecture | ass-029 OQ-02; ass-031 OQ-04 | DEFERRED | "revisit if Mode 3 quality is materially worse" |
| `session_category_snapshots` table | ass-031 OQ-01 (**BLOCKER**); ass-030 §O4 | **LATENT — blocker never resolved** | "Without it, training samples will have zero category histograms" |
| Thompson Sampling (Beta-Bernoulli) | ass-032; ass-040 | DEFERRED | "add if ICD < 1.5 nats" after PPR expander ICD measured |
| Neural Thompson Sampling | ass-032 Stage3 | DEFERRED | "After TS baseline ICD is measured" |
| Epinet epistemic uncertainty | ass-032 Task3 | DEFERRED | "premature until query diversity is sufficient" |
| IPS/SNIPS debiasing | ass-032 OQ-D | DEFERRED | "Add to W3-1 scope" — W3-1 deferred |
| **EWC++** regularization | ass-015; ass-031 §5.3 | ADOPTED then **REJECTED** | ass-032: "produces vanishing FIM… paradoxically loses patterns it should keep" |
| EWC from ruvector-gnn | ass-052 §7-8 | DEFERRED (→eval protocol) | "Evaluation protocol IS the deliverable, not a build recommendation" |
| DER++ replacing EWC++ | ass-032 Task6 | DEFERRED | "Replay-based methods consistently outperform regularization" |
| Focal loss | ass-032 Task6 | DEFERRED | Bundled with DER++ |
| SimCSE contrastive fine-tuning | ass-032 Task2 | DEFERRED | "Corpus too small… HIGH at 2,000+ entries"; "blocked by ONNX inference-only constraint" |
| BM25 sparse-dense hybrid + RRF | ass-032 Loop1 | **LATENT** | "Pursue" recommended; not tracked to an issue |
| Additional cross-encoder reranker (ms-marco-MiniLM) | ass-032 | **LATENT** | "Pursue immediately"; not confirmed shipped |
| MS-MARCO reranker as NLI alternative | ass-035 F6 | REJECTED | 3/7 same-feature hits vs cosine's 5/7 |
| Instruction-tuned small LLM (<1B) as zero-shot scorer | ass-032 §6.3 | UNSTATED | No verdict |
| Five ruv-fann models | ass-015 | DEFERRED (partial) | Only Classifier + Convention Scorer into crt-007; other three "deferred to crt-009" |
| "Neural-first, LLM-optional" | ass-015 §8 | ADOPTED | "GREAT without an API key, EXCEPTIONAL with one" |
| Shadow-mode validation, 3 slots (Production/Shadow/Previous) | ass-015; UM #425 | ADOPTED | Explicit promotion criteria |
| Auto-rollback (accuracy −5%, category −10%, NaN/Inf) | ass-015 §5.4 | ADOPTED | Explicit rollback criteria |
| Promotion guard is on **accuracy, not ranking quality** | UM #5578 | ADOPTED-with-caveat | "the guard is on model ACCURACY, not ranking quality (the outcome that matters)" |
| DPO for pairwise feedback | ass-015 §6.5 | DEFERRED | "Phase 5… if applicable" |
| RLVR | ass-015 §6.6 | DEFERRED | Phase 5 |
| Post-hoc/online calibration (temperature, isotonic) | ass-015 §2.2/§6.3 | DEFERRED | Phase 4 |
| Nested Learning (multi-timescale) | ass-015 §1.2 | UNSTATED | Recommended, never referenced again |
| Shrink & Perturb warm-restart | ass-015 §3.3 | UNSTATED | Not restated in feature scoping |
| Burn / Candle frameworks | ass-015 §4.4 | UNSTATED/superseded | Superseded by ruv-fann; no reconciliation |
| Goal clustering / goal-conditioned briefing | ass-032; ass-040 Grp5/6 | ADOPTED | "goal clustering real (2.7–9.5× effect)" |
| Goal-cluster store | ass-039 | DEFERRED | Gated on "H1 and H3 validation with proper embeddings" |
| H1 / H2 / H3 hypothesis tests | ass-039; ass-040 | DEFERRED (all three) | Triggers: goal embedding live; ≥10 rework cycles; agent_role on ≥50 sessions |
| Gap-signal capture from rework cycles | ass-039 H2 | REJECTED/BLOCKED | 162/163 sessions (99.4%) outcome=success; "0 rework cycles with sufficient data" |
| Evidence-driven confidence for research Theses | ass-057 §6/OSC-2 | DEFERRED (W3 Ph.3) | "helpfulness formula directly repurposable" |
| Research-domain ConfidenceParams preset (w_help=0.7) | ass-057 OSC-2 | DEFERRED | "Achievable via the existing struct without code changes" |

---

## 3. Knowledge lifecycle

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| Active/Deprecated/Quarantined/Proposed status model | ass-052; ass-030 §4.1 | ADOPTED | "well-served"; transitions affect HNSW membership |
| Three-tier auto-knowledge status (≥0.6 / 0.4–0.6 / <0.4) | ass-015 §5 | ADOPTED | Carried into col-013 |
| `trust_source` tiers for auto-extracted entries | ass-015 §5 | ADOPTED (refined) | Collapsed to single `"auto"` at 0.35 |
| Correction chain (supersedes + SHA-256 + `find_terminal_active`) | ass-030 §F1; ass-022/02 | ADOPTED | "solid"; a shipped differentiator |
| Supersession kept as an **entry field**, not GRAPH_EDGES-first | ass-057 §4 | ADOPTED (status quo affirmed) | Graph derives Supersedes from `entries.supersedes` only |
| GRAPH_EDGES Supersedes rows written but **skipped by the graph builder** | ass-057 §2, UQ-4 | **UNSTATED (flagged)** | "revision_reason… accessible only via direct SQL. Invisible to all graph traversal logic" |
| Dual structure documented as an ADR | ass-030 §F1/§O1 | ADOPTED (design) / ADR **DEFERRED** | "complementary, not redundant" — ADR never written |
| Chain capped at 50 hops via CTE, fallback on cycle | ass-088 F8; UM #4475 | ADOPTED | Confirmed bounded/safe |
| Chain dead-ends on non-Active terminal → convergence skips | ass-088 F9 | ADOPTED | "skip when no valid terminal" |
| Restore fallback to Active when `pre_quarantine_status` NULL/invalid | UM #601 | ADOPTED | Existing |
| Restore conditionally re-inserts into VectorIndex | UM #3764 | ADOPTED | bugfix-444 |
| `context_quarantine` requires `Capability::Admin` | UM #4413 | ADOPTED | "gate is intentional" |
| **Auto-quarantine on effectiveness** (`auto_quarantine_cycles` dflt 3) | ass-037 Q7; ass-103 op 1l | ADOPTED | "effectiveness-based quarantine is sufficient" |
| NLI auto-quarantine guard (`nli_auto_quarantine_threshold=0.85`) | UM #2716 (ADR-007 crt-023) | ADOPTED then **REJECTED (removed)** | "0 Contradicts edges → guard always returns Allowed; never blocks anything" |
| Restore the NLI auto-quarantine guard | ass-092 Q2 | **REJECTED** | "the guard would be dead code" |
| Auto-deprecation on zero access after N cycles (access-cliff) | ass-015 §4.4/§5.1 | ADOPTED | Became col-013 "Dead Knowledge" rule |
| `pinned` vs `adaptive` CategoryPolicy → entry auto-deprecation | ass-032 #445; ass-040 Grp8 | ADOPTED | #445 is "prerequisite for entry auto-deprecation in enhanced #409" |
| Cross-feature validation gate before promotion (2–5 features) | ass-015 Part 4.1 | ADOPTED | In the col-013 quality gate |
| Dedup/merge near-duplicates at extraction (cosine ≥0.92) | ass-015 §5 | ADOPTED (0.92) | The 0.85 figure elsewhere in the same spike is superseded within it |
| **Category budgets** capping auto-entry growth (conventions 50, procedures 30, patterns 30, gaps 20, lessons 20) | ass-015 §5.4 | **LATENT** | Only the 10/hour rate limit carried forward |
| `context_review` MCP tool for human accept/reject/modify | ass-015 Feature 5 | DEFERRED | Scoped to crt-009 (last in the chain) |
| `context_extract` manual trigger tool | ass-015 §5 | **UNSTATED / dropped** | Absent from final col-013 scope, no reason given |
| **Knowledge synthesis** — distil 3+ entries, supersede lowest-confidence, deprecate originals | ass-022/04 §4 | **DEFERRED** | "premature at project scale… only at >200 clustered entries" — trigger never re-checked |
| ACE-style delta-update / "grow-and-refine" | ass-015 §5/§8 | DEFERRED | Real gap — "no 'evolve'… only replace" — not in the roadmap |
| A-Mem Zettelkasten auto-linking + memory evolution | ass-015 §5 | DEFERRED | Rated "CRITICAL", never scoped |
| Autonomous taxonomy/schema evolution | ass-015 §3/§8 | DEFERRED | Priority "P2" gap; absent from roadmap |
| Cascading auto-transfer of dependency edges on supersession | ass-055 Q3 | **REJECTED** | "if A' meaningfully changes the decision, copied edges may be semantically incorrect" |
| `stale_dependency_edges` + `DependencyOnDeprecated` rule (surface, don't auto-fix) | ass-055 Q3; ass-057 | ADOPTED | Surface-only chosen over auto-transfer |
| In-place mutation of non-content fields (tags/status) | ass-093 | ADOPTED | Wins comparison 29/30 |
| Route capability status transitions through tags, not `context_correct` | ass-093 Q4/Q5 | ADOPTED | "it zeroes the learning vector every transition" |
| General mutable-metadata key/value lane distinct from tags | ass-093 Q4 | REJECTED | "unjustified; today it duplicates what `entry_tags` already is" |
| Reserved status-tag namespace with controlled vocabulary | ass-093 | **LATENT** | "optionally validate" — not settled |
| Thesis lifecycle mapped 1:1 onto Status | ass-057 FINDINGS-A §1 | REJECTED | "refuted has no Status equivalent at all" |
| `tags` convention as interim Thesis-status carrier | ass-057 §1 | ADOPTED (interim) | "not indexed separately and typo-prone" |
| `deprecated_at` column + `as_of` queries | ass-057 UQ-2 | DEFERRED (Ph.3+) | "5–7 days of schema migration. Correctly deferred" |
| `context_cycle_review` purge capability **removed entirely** | ass-091 headline | ADOPTED | Human decision 2026-07-04: "a destructive purge on a review has no natural caller" |
| Optional explicit `purge:true` flag | ass-091 Q3 | REJECTED | "dropped as dead surface" |
| Reclamation delegated to TTL/cap/session-close backstops | ass-091 | ADOPTED | Load-bearing simplification |
| **`context_purge` — atomic DB row + VECTOR_MAP + HNSW point delete** | ass-040 Grp7 | **LATENT — named as a gap, never built** | "~2,491 quarantined phantom entries… **can be deleted but no atomic delete + HNSW removal exists**" |
| Quarantine guard at co_access **write** time | ass-040 Grp7 (#477) | **LATENT** | `analytics.rs` comment explicitly defers it |
| Sensor "knowledge inheritance" correction chain | ass-022/03 §5 | DEFERRED | Prototype bridge, not committed |

---

## 4. Integrity

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| SHA-256 hash chain (`content_hash`, `previous_hash`, `version`, audit_log) | ass-030 §4.1; ass-052 | ADOPTED | "tamper-evident provenance chain… well-served" |
| Hash chain covers **only title+content** | ass-093 Q1 (`hash.rs:7-16`) | ADOPTED (empirically settled) | Load-bearing for ass-093 |
| `previous_hash` written as empty string — **not actually a chain** | ass-057 FINDINGS-B §2 | **UNSTATED (factual gap)** | "The field exists in schema but is unused as a chain" |
| Verify the hash chain **on read** | ass-020 P5 | **DEFERRED** | "written but never checked on retrieval; tampered entries pass undetected" |
| Cross-version chain tamper-evidence | UM #5558/#5563 | ADOPTED (KI-CHAIN-XV) / **MISSING** (XV-STRONG) | Strong variant vs raw-DB adversary `delivery:missing`; #912 unwired |
| Tag/status integrity via capability gating + audit, **not** crypto chain | ass-093 Q1 | ADOPTED (posture) | "consistent with tags being volatile metadata" |
| Append-only DDL triggers (BEFORE UPDATE/DELETE RAISE ABORT) | ass-050 §4; UM #4359 | ADOPTED (recommended) | "application-level convention only" today — SOC2 CC7.1 |
| Audit schema additions (`credential_type`, `capability_used`, `agent_attribution`, `metadata`) | ass-050 §4 | ADOPTED (recommended) | "one migration that gets this right… do not defer" |
| `agent_id` as attribution metadata, **never** a capability gate | ass-050 Inv.7 | ADOPTED | "gating must be driven by ResolvedIdentity" |
| Invariants: `audit_log.detail` untruncated; `cycle_events.goal_embedding` reachable; `observations.phase` indexed | ass-050 Inv.1–3 | ADOPTED (constraints) | Preserve joins for behavioral-provenance analysis |
| Two-hop provenance chain (audit_log → sessions → cycle_events) | ass-050 Seam 5 | ADOPTED (design) / **LATENT (impl)** | "context_cycle MCP handler does NOT write cycle_events… breaking provenance for Codex/Gemini" (#574) |
| Future `task_log` table | ass-050 Inv.4 | **LATENT** | "tasks are currently invisible to Unimatrix" |
| `EnterpriseAuditWriter` trait (SIEM dual-write) | ass-050 §3 | **LATENT** | Additive interface spec'd |
| ABAC `allowed_topics`/`allowed_categories` columns, dormant | ass-050 OSD#4 | **LATENT** | "must not be dropped — seam for future ABAC" |
| `Capability::SessionWrite` defined, never used | ass-050 OSD#3 | **LATENT** | "must be documented as reserved and not removed" |
| Route tick writes through `SecurityGateway::validate_write()` | ass-020 P3 | RECOMMENDED (GH #273) | Tick calls `store.insert()` directly |
| Scan `tags`/`topic`/`source` through `scan_content()` | ass-020 P4 | RECOMMENDED (GH #274) | Currently unscanned |
| `[KNOWLEDGE DATA]` output framing (OWASP ASI06) | ass-020 P1/P2 | RECOMMENDED (GH #271/#272) | Actionable now |
| Sensitive-path blocklist + path-only extraction | ass-015 §5.3 | ADOPTED (design rule) | Mitigation for "Sensitive Information Capture" |
| Staged identity model (self-reported → env var → OAuth) | ass-020 Appendix ADR | ADOPTED | Current stage declared intentional |
| Full capability hierarchy enforcement | ass-020 Tier 3 | DEFERRED | "Blocked on verifiable agent_id" |
| Per-agent write rate limiting vs bulk poisoning | ass-042 §5 | **UNSTATED — no findings exist** | Question posed, never answered |
| Fold a tag-mutate op into the SLN1 write-rate budget | ass-093 OSD | ADOPTED (requirement) | "a poison-budget gap to close deliberately, not by omission" |
| SLN1 poisoning posture | UM #5528 | **research-gated, `delivery:asserted`** | "defensive measures were added early; threat model + current posture are **uncharacterized**" |
| In-crate SHA-256 model-integrity verification | ass-092 OSD; ass-061 OSD#1 | DEFERRED | "SLN1 defense depends on it" |

### 4a. Contradiction / NLI — the full arc

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| Embedded NLI cross-encoder replacing the cosine heuristic | ass-022/04 §3 | ADOPTED | "Yes — small async model, writes are rare" |
| NLI entailment for **Supports** | ass-035 | **REJECTED** | Task mismatch — max entailment 0.255, zero pairs cross 0.45 |
| **Cosine ≥0.65** for Supports | ass-035 | ADOPTED | 6/8 TP Group A, 1/1 Group B, **0/10 FP** (max false cosine 0.247) |
| DeBERTa-v3-small as a drop-in swap | ass-035 F4 | REJECTED | False pair P16 scored 0.722 entailment |
| **GGUF / local LLM** (Phi-3-mini Q4, Llama-3.2-1B) | ass-036 | **REJECTED (hard FAIL)** | 11/25 (44%) correct, 70% FP; 24,077ms mean Form-A; Llama-1B 1/25 |
| Post-store NLI (`run_post_store_nli`) | ass-037 Q7 | REJECTED (removed) | 30 Supports edges total, 27 endpoints later quarantined; **0 Contradicts ever written** |
| Bootstrap NLI promotion | ass-037 Q7 | REJECTED (dead code) | 0 bootstrap Contradicts rows |
| Decouple structural inference from `nli_enabled` | ass-037 Rec#1; ass-040 Grp2 | ADOPTED | The coupling was "a category error" |
| Raise `nli_informs_cosine_floor` 0.3→0.5 | ass-037 Rec#1 | ADOPTED | Compensates for the removed NLI guard |
| Separate periodic `contradiction_tick` | ass-037 Q8 | DEFERRED | "Blocked until a domain-adapted model is available" |
| Heuristic contradiction scan (cache-only, every 4 ticks) | ass-057; ass-103 op 6 | **LATENT** | "Does NOT write Contradicts edges… feeds `context_status` output only" |
| C-13/AC-10a hard gate discarding all NLI contradiction scores | ass-057 FINDINGS-C §1 | ADOPTED for SDLC; lift for `claim` proposed | Added because SNLI produced FPs on SDLC text |
| `contradicts_category_pairs` scoping NLI to Claim↔Claim | ass-057 §5 | DEFERRED (Ph.3) | "~20-line addition" |
| **Restore the Contradicts-edge write inside the existing gate** | ass-092 Q2 | **ADOPTED (recommended)** | "one branch inside the existing gate"; small, low-risk, no schema change |
| Literal as-was restoration (revert crt-038) | ass-092 Q2 | REJECTED | "duplicates the NLI scoring the tick already does" |
| Reverse NLI-removal-from-ranking | ass-092 Non-Goals | REJECTED | "presumed-sound decision" |
| Split `nli_enabled` into two gates | ass-092 OSD | DEFERRED | "a future split gives per-domain cost control" |
| Generalized small-ONNX-model substrate | ass-092 Q3 | DEFERRED | "a separate future `unimatrix-embed` refactor" |
| Orphaned NLI config trio | ass-092 OSD | **UNSTATED** | "decide re-use vs removal" |
| `query_contradicts_edges_for_entry` dead code | ass-092 OSD | DEFERRED | "deprecate this store method" |
| **KI-CONTRADICT capability status** | UM #5548 | **`delivery:missing` — REGRESSED** | "operational, found nothing in the SDLC corpus, and was removed… **Domain-conditional: high value in a research domain (competing claims), null in SDLC. Do NOT prune on single-domain utility.**" |
| Factor a single-entry `check_entry_contradiction()` out of the batch scan | ass-015 col-013 | ADOPTED | "~30 lines refactored, no logic change" |
| Restrict the O(N²) scan from every tick / every `context_status` | ass-020 ARCH-1 | RECOMMENDED | "Option 3 is most impactful" |

---

## 5. Relevance / decay — *where the corpus contradicts itself*

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| **Time-based expiry of knowledge** | UM #5581 (RETAIN, `delivery:proven`) | **REJECTED — ratified as an NFR** | "Time-based expiry would discard still-useful knowledge; retention must be governed by **learning utility**… un-reviewed entries are never pruned" |
| **Freshness half-life = 168h, exponential decay, a confidence sub-component** | ass-032 PIPELINE-AUDIT §3.3/§7 | **ADOPTED — and hardcoded** | Shipped. **This is time-based decay, live, in direct tension with RETAIN** |
| Freshness half-life as a **global hardcoded constant** | ass-022/01 §3.1 | **RECOMMENDED (Critical, ~1h) — never externalized** | "not a weight issue — a **dimensional mismatch**"; top of the "Effort to Full Domain Agnosticism" table |
| **Per-category configurable half-life** (regulatory 1–2yr, seasonal 90d, source-attribution 30d, anomaly 14d, calibration 7d) | ass-022/03 §3/§9 | **DEFERRED** | "A single Unimatrix instance cannot currently support multiple freshness half-lives per category. **This is a feature gap**" — 2–3 days, never built |
| Configurable `freshness_half_life_hours` for research corpora | ass-057 toml sketch | ADOPTED (instance level) | "Research corpora evolve quickly" |
| GNN to learn the half-life rather than hardcode it | ass-022/04 §2 | DEFERRED | Bundled with the deferred GNN |
| `CLEAN_REPLACEMENT_PENALTY` (0.40) on deprecated entries at PPR depth 1 | ass-055 Q3 | ADOPTED (shipped) | Topology-derived penalty |
| Confidence bootstrap tiers by category (convention 0.55 … lesson 0.40) | ass-015 App.B | ADOPTED | Carried through |
| Access-cliff / staleness as the "dead knowledge" trigger | ass-015 §7 | ADOPTED | Became a col-013 rule |
| Configurable `K` retention (co_access, query_log, audit_log) | ass-040 Grp8 (crt-036) | ADOPTED | Complete |
| `query_log_lookback_days` (30d, PhaseFreqTable) | ass-034 F8 | ADOPTED | Truncation degrades PPR "gracefully — quality degradation, not a correctness failure" |
| Isolated-entry **age** as staleness indicator | ass-034 F2 | UNSTATED (rationale only) | Isolated entries 1.5–3.5× older — motivated Informs, not decay |
| **Temporal ordering** (source older than target) in Informs detection | ass-034 F5/F6 | ADOPTED | **Age used as signal, not as decay** |
| Query-drift / trending analysis | ass-015 §7 | DEFERRED | Opportunity gap, not roadmapped |
| Emergent topic clusters (KE-10) | ass-015 Part7 | DEFERRED | "Priority 4: Defer… requires sufficient query volume" |
| Search-to-action correlation (KE-08) | ass-015 Part7 | DEFERRED | "Priority 4: Defer… correlation-not-causation" |
| Explicit time-decay of the **confidence score** by recency/access | ass-040/041/042/047/050/051 | **UNSTATED — not addressed in any of those six** | — |
| Reconcile HNSW retaining stale vector IDs vs live corpus | ass-074 OSD | DEFERRED | "worth a one-off reconciliation" |

---

## 6. Storage governance

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| Two-database split (`knowledge.db` / `analytics.db`) + mpsc write queue | ass-022/05 §3–4 | ADOPTED | "Phase 0… resolves the write contention problem now and forever" |
| `observations` SQLite table replacing dual JSONL+SQLite | ass-015 §3/§9 | ADOPTED | Became col-012 |
| File-based JSONL observation buffer for v1 | ass-015 §8.1 | **SUPERSEDED within the spike** | A later doc argues to go straight to SQLite |
| 60-day retention on `observations`, cleaned by the tick | ass-015 §3 | ADOPTED | Explicit tick item |
| Retention for observations/query_log/injection_log/shadow_evaluations/audit_log | ass-030 §F4/§O3 | UNSTATED | "not an immediate crisis but is a known gap" |
| Intelligence-driven retention (co_access activity, query_log last-K, audit_log 180d) | ass-032 #409; ass-040 Grp8 | ADOPTED | Complete (crt-036) |
| `observations` rows possibly deleted before provenance analysis can use them | ass-050 UQ1 | **UNSTATED** | "not determined by this spike" |
| On-demand rules must not source from tables an EveryTick destructive pass prunes | UM #5431 (bugfix-891) | ADOPTED (rule) | "observable-state lifetime collapses to one tick" |
| Preserve `co_access` from pruning | ass-034 F8 | REJECTED as prune target | "It IS the learning signal… pruning it prunes the learning signal" |
| Truncate `observations`/`injection_log` | ass-034 F8 | ADOPTED (safe) | "Nothing reads it for learning" |
| `compact()` (WAL checkpoint + VACUUM) doesn't remove rows | ass-030 §F4 | UNSTATED | Flagged insufficient, no fix committed |
| **HNSW build-new-then-swap compaction** | ass-052 (ADR-004) | ADOPTED | "hnsw_rs has no point deletion API. Full rebuild required" |
| RuVector tombstone-only HNSW deletion | ass-052 | REJECTED | "'graph structure remains'… no compaction" |
| **HNSW heal pass** (re-embed `embedding_dim=0`, cap 20/tick) | ass-032 #444 | ADOPTED | "healed 20 foundational entries"; NLI max score 0.147→0.383 |
| **VECTOR_MAP prune pass** for quarantined entries | ass-032 #444 | ADOPTED | "pruned 209 stale HNSW points" |
| Maintenance-tick index-active-set invariant | UM #3761 | ADOPTED (lesson) | The invariant the tick must enforce |
| Graph compaction gated on `graph_stale_ratio > 0.10` | ass-015 col-013; ass-103 op 1i | ADOPTED | Relocated from `crt-005 maintain=true` into the tick |
| `usearch` as HNSW replacement | ass-032 Option7 | DEFERRED | "revisit at >500K entries" |
| ColBERT late-interaction storage | ass-032 Option8 | REJECTED | "Memory footprint disproportionate to corpus size" |
| SPLADE sparse expansion | ass-032 Option5 | DEFERRED | "meaningful only at corpus > 100K entries" |
| Vector `passthrough` mode for pre-computed `Vec<f32>` (non-text domains) | ass-022/03 §4/§9 | **DEFERRED** | Explicit structural gap; "~1 day", never built |
| `write_max_connections` default 1 (hard cap 2) | ass-047 Q1 | ADOPTED | "to prevent SQLITE_BUSY_SNAPSHOT under concurrent WAL deferred transactions" |
| Async analytics queue (1000-cap, ≤50 events/500ms) | ass-047 Q1; ass-057 | ADOPTED | 100 events/sec, "not the bottleneck at N≤20" |
| Audit writes bypass the batched path | ass-047 Q1 | ADOPTED | "correct for SOC 2 integrity" — intentional |
| Dedicated write connection for the audit log | ass-047 OSD#1 | **LATENT/DEFERRED** | "Wave 2 consideration" |
| Replace `Mutex<Connection>` with a pool or client-server DB | ass-020 SCALE-1 | DEFERRED | Pool = one feature; DB swap = "multi-sprint" |
| Separate read-only from write connection | ass-020 ARCH-4 | RECOMMENDED (high) | "would eliminate the primary source of MCP request blocking" |
| SQLite WAL + `read_max=6` to 50 agents; **PostgreSQL above 50 / >300 writes/sec** | ass-047 Q2/Q3 | ADOPTED (recommendation) | Ship PG as a documented option from day one |
| SQLite-only constructs blocking a PG port | ass-047 Q3 | **LATENT (catalogued, unfixed)** | "not a rewrite" but ~1–3 engineer-weeks |
| Lazy loading + **LRU eviction** of per-repo structures (30-min inactivity) | ass-047 Q5 | ADOPTED (recommendation) | "required above ~20 active repos on 8GB RAM" |
| **LRU residency/eviction inside the tick work-unit seam** | UM #5167 (ADR-004 crt-056) | **EXPLICITLY REJECTED for crt-056** | "no bounded worker pool, no LRU residency/eviction, no cadence signals… Shipping half a scheduler inflates the feature and risks shipping the wrong half" |
| TypedRelationGraph clone-per-search vs Arc-snapshot | ass-047 OSD#5 | **LATENT/DEFERRED** | "worth profiling" |
| Per-org write queue keying | ass-047 Q7 | REJECTED | "per-repo drain task architecture already achieves per-org isolation" |
| `context_batch_write` atomic bulk write | ass-057 OSC-6 | REJECTED | "HNSW atomicity problem… no clean resolution without significant architectural work" |
| Missing indexes on `entries.supersedes`/`superseded_by` | ass-057 OQ-B-2 | DEFERRED | "Add in the traversal feature migration" |
| Composite GRAPH_EDGES indexes | ass-057 | **LATENT** | "add when context_neighbors-style queries become common" |
| **`observations.hook` has no index → full-table scan every PhaseFreqTable rebuild** | ass-051 OSD#1 | **LATENT** | "worth a follow-up issue" |
| Canonical hook-event-name migration (v24→v25) | ass-051 | REJECTED | "no behavioral benefit… migration cost is real" |
| Missing FK constraints (co_access, graph_edges, feature_entries, injection_log, outcome_index) | ass-030 §F2/§O7 | DEFERRED/UNSTATED | "Moderate risk as KB grows" |
| `counters` aggregates can drift from `entries.status` | ass-030 §F8 | UNSTATED | "Low Risk"; no fix proposed |
| `sessions.keywords` inert field | ass-030 §F7/§O5 | **UNSTATED** | "dead schema… implement or remove" — never answered |
| `query_log_results` junction table | ass-030 §O2 | UNSTATED | Opportunity only |
| `topic_delivery_phases` junction table | ass-030 §O6 | UNSTATED | Opportunity only |
| Tenant isolation: separate DB files (Path A) vs tenant column (Path B) | ass-030 §O10 | Path A ADOPTED (implicit) | "No schema changes needed if separate-DB-per-project confirmed" |
| Scalability tiers (read replicas → topic-sharded → federated) | ass-022/04–05 | DEFERRED | "Design now, ship at scale" |
| GGUF via separate volume + init-container; ONNX baked in | ass-061 Q4 | ADOPTED | 1–8 GB would "10–50× the model portion" |
| Cargo feature flags reserved exclusively for GGUF | ass-061 Q1 | ADOPTED | "Never separate crates for core engine features" |
| `unimatrix snapshot` via `VACUUM INTO` + `--anonymize` | ass-025 Del.1 | ADOPTED | "eval needs analytics tables that the existing export excludes" |
| Transcript backstops: 4 MiB ring-tail, 64-session cap, 24h TTL | ass-091 Q1 | ADOPTED (protected by SCOPE) | Existing fidelity ceiling |
| Raise transcript fidelity caps | ass-091 Q4 | **LATENT** | "a human risk-posture call, not a free consequence" |
| Persist transcript to disk to survive restart | ass-091 Q4 | **REJECTED** | "explicit NG-1/Principle-8 breach requiring a conscious human decision, never smuggled" |

---

## 7. Monitoring / observability

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| Graph cohesion metrics (`isolated_entry_count`, `cross_category_edge_count`, `supports_coverage`, `mean_entry_degree`) | ass-032 #413 | ADOPTED | "primary health view for graph inference" |
| CC@k, ICD beyond P@K/MRR | ass-032 #399 | ADOPTED | Shipped |
| NEER (Novel Entry Exposure Rate) | ass-032; ass-040 | DEFERRED | "after session-level eval designed" |
| PRP@k, temporal confidence improvement, ILD | ass-032 §2.3 | UNSTATED | Proposed, not confirmed built |
| Distribution gate replacing zero-regression check | ass-032 #402 | ADOPTED | "must be in place before #398 ships" |
| `unimatrix eval run/scenarios/report` + `eval live` + hook-IPC sim client | ass-025 Del.2–6 | ADOPTED | "the artifact the human reviews to decide" |
| Behavioral eval scenarios replacing null-ground-truth | ass-039 Out.1 | ADOPTED | Old ones measured "self-consistency… not actual retrieval quality"; 1,585 new made canonical |
| **Platform cannot measure its own graph-relational retrieval quality** | ass-074 primary discovery | **DEFERRED — gap named, unfixed** | "steering its graph features **blind**" |
| Relational-relevance labeled corpus | ass-074 Future work | DEFERRED | "highest-value follow-up" |
| SL-METRIC trust calibration for live corpus | UM #5572 | **`delivery:partial`, keystone** | "not yet trusted for live-corpus interpretation" (#803) |
| Per-`trust_source` coherence lambda diagnostics | ass-015 col-013 | ADOPTED | Explicit StatusReport field |
| LAMBDA-HONEST — fail-loud on empty, never a fake 0 | UM #5555 | `delivery:partial` | "'measures reality' arithmetic tested against a stub only" |
| Five-dimension "GREAT bar" (precision <10% FP, coverage >60%, freshness 1–2 cycles, relevance >70%, trust parity) | ass-015 §2 | ADOPTED (framework) | Governing measurement framework |
| Knowledge pollution rate (<20% target) | ass-015 §5.1 | ADOPTED (target) | Tied to pollution-risk mitigation |
| Convention deviation rate (healthy 5–15%) | ass-015 §5.2 | ADOPTED (target) | "Below 5% suggests lock-in; above 15% suggests the convention is wrong" |
| **Prometheus endpoint** (queue depth, `shed_events_total`, pool latency, **tick completion time**, audit write latency) | ass-047 Q7 | **ADOPTED (recommendation), ~2 days — not built** | "operators **cannot run in production** without it" |
| Structured logging with org_id/project_id spans | ass-047 Q7 | ADOPTED (recommendation, ~1 day) | "required for SaaS log routing" |
| `shed_events_total` sole queue-pressure signal, no HTTP endpoint | ass-047 OSD#4 | **LATENT** | "insufficient for production operations" |
| `[dashboard]`-gated embedded SPA | ass-061 Q2 | DEFERRED | "read-only knowledge browser and health status page" |
| `blend_alpha` logged in tick metadata | ass-031 OQ-05 | **LATENT** | Designed hook, never activated |
| Shadow promotion gate (CC@k≥0.7 AND MRR_delta≥0 AND P@K_delta≥−0.02) | ass-032 §4.2 | **LATENT** | Designed for the deferred W3-1 |
| `stale_dependency_edges` in `context_status` | ass-055 Q3 | ADOPTED (design) | "ADR dependency health visibility" |
| Degree-distribution baseline (max in-degree 110, avg 8.07 @ ~4k entries) | ass-088 OSD | ADOPTED (recorded) | "a useful capacity-planning datum" |
| One-line log when an injected PPR entry is deprecated | ass-074 Q2 | **LATENT** | "to make the capped risk observable" |
| `search_complete`/`elided_bytes`/`provenance` loss-propagation | ass-091 Q3 | ADOPTED (hard requirement) | Prevents silent false negatives |
| Content-opaque integer fold as the sole durable transcript signal | ass-091 Q1 | ADOPTED | crt-054 #5030 — "counts, never prose" |
| N4 — healthy deployment emits no false alarms | UM #5576 | `delivery:partial` | "ERROR-level false alarms erode trust" |
| Replace the naked `.unwrap()` on `JoinError` that can permanently kill the tick | ass-020 P1/FIX-1 | RECOMMENDED (critical) | "The highest-priority fix" |
| Supervisor auto-restarting the tick on panic | ass-020 FIX-6 | RECOMMENDED → **ADOPTED** | Exists today (ass-103 I-19) |
| Idle timeout/keepalive on rmcp stdio transport | ass-020 P11 | **UNSTATED** | Real thread-leak risk; no fix proposed |

---

## 8. Background processing (the tick itself)

| Idea | Source | Disposition | Stated reason |
|---|---|---|---|
| **One unified maintenance tick — no second scheduler** | ass-015 col-013; ass-103 | ADOPTED | Extraction triggers "piggyback on the same maintenance-timer infrastructure" |
| Batch (not real-time/streaming) processing for passive acquisition | ass-015 Part6.1 | ADOPTED | "This is a **deliberate choice**" — batch chosen for accuracy/noise |
| 15-min cycle, 120s per-op timeout, fixed step order | ass-031 §1; ass-047 Q4 | ADOPTED | Existing production pattern |
| 120s `TICK_TIMEOUT` is **per-op, not per-pass** | ass-103 OSD | **UNSTATED (structural)** | Worst case 9 jobs × 120s × N slugs vs a 900s interval; no pass-level budget |
| **Registry order IS the ordering invariant** | ass-040 Grp2; UM #5167; ass-103 | ADOPTED | "jobs are not reordered" |
| Tick decomposition: Phase 4b unconditional; NLI legs gated | ass-037 Q8; crt-039 | ADOPTED | "zero behavioral change — condition and ordering preserved" |
| Contradiction scan as its own periodic block, EveryN(4) | ass-040 Grp2; ass-057 | ADOPTED | `CONTRADICTION_SCAN_INTERVAL_TICKS = 4` |
| **`BackgroundJob` work-unit seam** — trait + static registry, `Cadence::{EveryTick, EveryN}` | UM #5167 | ADOPTED | "future background math is additive — implement the trait + register" |
| **"Step B" scheduler: bounded worker pool, LRU residency/eviction, cadence *signals*, concurrent rayon** | UM #5167 | **EXPLICITLY DEFERRED — largest deferred architecture item** | "Shipping half a scheduler inflates the feature and risks shipping the wrong half." Contained ~1–2 week follow-up needing zero work-unit changes |
| `ResourceClass{Io, Rayon}` | UM #5167; ass-103 OSD | **LATENT — dead weight by design** | "a **self-tag only** — nothing in crt-056 reads it; a forward hook so Step B's semaphore can group jobs" |
| Serial per-slug loop, per-slug `tick_counter`, serialized rayon | UM #5168 | ADOPTED | Explicitly no cross-slug concurrency |
| **Shard the tick into independently-scheduled tasks** (confidence 5min, effectiveness 60min, contradiction 120min, co-access 30min, session GC 60min, extraction 5min) | ass-020 SCALE-3 | **RECOMMENDED — never built** | "At 3–5× current volume, the single 15-minute tick is too coarse" |
| Streaming extraction tick | ass-020 SCALE-2 | RECOMMENDED | "Eliminates the 10,000-row single-lock-hold issue entirely" |
| Cap extraction batch (10,000 → 500/1,000) | ass-020 FIX-3 | RECOMMENDED → ADOPTED | Batch is 1000 today |
| LIMIT + batching on session-GC DELETE CASCADE | ass-020 FIX-5 | RECOMMENDED | Medium priority |
| Cache effectiveness aggregates with TTL | ass-020 ARCH-5 | RECOMMENDED | "decouples the effectiveness analysis cost from interactive calls" |
| `context_status` read-only; `maintain=true` → emergency override | ass-015; UM #178, #3767 | ADOPTED (design) — **superseded** | "prune pass and heal pass are also unconditional since bugfix-444" |
| `compute_report()` as tick loader → `MaintenanceDataSnapshot` | UM #1777 (bugfix-280) | ADOPTED (fix) | ~20–40s of tick budget wasted. **ass-103 I-17: a narrower version of the same inflation survived** |
| Fire-and-forget `spawn_blocking` per MCP request | ass-020 P9 | RECOMMENDED (fix) | "no backpressure or cap" |
| Bounded mpsc + batching consumer | ass-020 ARCH-3 | RECOMMENDED → ADOPTED | The analytics queue exists today |
| Hold (not increment) `consecutive_bad_cycles` on error | UM #1544 | ADOPTED | Error-handling posture |
| Define error semantics for consecutive counters **before** implementation | UM #1542 | ADOPTED (rule) | Standing pattern |
| GRAPH_EDGES tick writes use `write_pool_server()`, not the analytics queue | UM #3883, #4124 | ADOPTED | NLI/behavioral edges "must never be shed" |
| Independent per-type budgets in NLI Phase 5 | UM #3971 | ADOPTED | Replaced a shared budget |
| `max_co_access_promotion_per_tick` (200) | UM #3826 | ADOPTED | Throttle |
| `max_cycles_per_tick` in RetentionConfig | UM #3916 | ADOPTED | Config placement |
| **GNN tick steps 10–12** (sample ingestion 50/tick; training gate + rayon 32-sample batches; promotion + shadow eval + cache rebuild) | ass-031 §2.1–2.3 | **LATENT — fully specified with latency/thread budgets, never implemented** | ass-032 redirected effort to PPR/frequency-table/TS |
| GNN training cadence: every 24h OR 25 new samples | ass-031 §5 | **LATENT** | "batches large enough for stable gradient estimates" |
| `gnn_training_cursor` table vs `signal_queue.gnn_processed` | ass-031 OQ-08 | UNSTATED | Deferred to delivery |
| `EntryFeatureCache` full vs incremental rebuild | ass-031 OQ-07 | UNSTATED | "switch to incremental if >50ms" |
| `TranscriptSummaryJob` beside `GraphInferenceJob` | ass-091 Q4 | **LATENT** | "documented seam… built by no one here" |
| Local GGUF inference at review-time or background, never the query hot path | ass-091 Q4 | ADOPTED (constraint) | Principle 7 |
| Dedicated rayon lane (`gguf_rayon_pool`) | ass-091 Q4 | **LATENT — `TODO(W2-4)`** | Flagged as an anti-stub tension |
| `unimatrix run` **event-driven** session host | ass-066 Q3/Q6 | DEFERRED/LATENT | "2–3 weeks for observation parity; 5–8 weeks for full knowledge-aware runtime" |
| Daily-cadence compatibility of behavioral/goal-cluster/S8 watermark processing | ass-057 FINDINGS-C §5 | ADOPTED (validated) | "infrastructure was designed for this session-bounded, non-continuous pattern" |
| In-memory TypedRelationGraph staleness window | ass-057 OQ-B-4 | **UNSTATED — options given, no choice** | "an architectural decision affecting all traversal APIs" — accept / fallback-SQL / partial-refresh all open |
| Per-slug scope for the convergence tick | ass-088 F10 | ADOPTED | Inherently per-slug by store isolation |
| Eval harness cold-start fix (`from_profile()` calls `TypedGraphState::rebuild()`) | ass-040 Grp4 (crt-045) | ADOPTED | "eval always ran cold… baseline and expander profiles bit-identical" |

---

# Cross-cutting observations

**O-1. The corpus contradicts itself on time-based decay.** RETAIN (#5581, `delivery:proven`) rejects time-based expiry as a ratified NFR. Yet a **168h exponential freshness half-life is live in the confidence composite**, globally hardcoded. ass-022/01 rated externalizing it **Critical, ~1 hour**, calling it *"not a weight issue — a **dimensional mismatch**."* ass-022/03 specified the per-category form, called it *"a feature gap,"* costed it at 2–3 days — never built. **The campaign's canonical example is already documented, already costed, and already deferred twice.**

**O-2. A very large LATENT surface exists — mostly forward-hooks that outlived their consumer.** `graph_edges.metadata` (for a GNN never built), `bootstrap_only` (for an NLI promoter later deleted), `ResourceClass` (for a Step-B scheduler never built), `Capability::SessionWrite`, ABAC columns, `s2_vocabulary` (empty → op 9b is a no-op), `RelatedTo` (0 edges), `blend_alpha` logging, the shadow promotion gate, `gguf_rayon_pool`'s `TODO(W2-4)`, the entire ass-031 GNN tick design. **The dominant cause is not oversight — the consumer was deferred and the hook shipped anyway.**

**O-3. Deferrals are gated on scale triggers nobody re-checks.** Leiden ">~500 entries"; synthesis ">200 clustered"; SimCSE "≥2,000"; S3/S4 "≥3,000"; SPLADE ">100K"; usearch ">500K"; the filtered-PPR probe "≥5K or density ≥1.0". Several have plausibly been crossed. **No mechanism re-evaluates a scale-gated deferral when the gate opens.**

**O-4. NLI is the only full adopt → measure → remove → regress → restore-proposed cycle.** Adopted (ass-022) → Supports leg disproved (ass-035) → LLM alternative hard-failed (ass-036) → four use sites removed (ass-037) → ranking weight zeroed (ass-039) → capability records **REGRESSED, `delivery:missing`** (#5548) → restore scoped as one branch inside the existing gate (ass-092). The capability note carries the campaign framing verbatim: *"Domain-conditional… **Do NOT prune on single-domain utility.**"*

**O-5. The GNN was fully designed twice and built zero times.** ass-029 scoped, ass-031 delivered a complete design, ass-032 re-examined against live data and **recommended de-scoping** on an exposure-bias argument (#3429 — labels come only from what the current formula already surfaced). ass-038 rated training data "NEAR-PASS" (3/4). The surviving phase/category machinery was the *interpretable substitute* ass-032 shipped precisely because it *"ships before GNN exists."*

**O-6. Edge demotion has never had an owner, and the spike scoped to fix it never ran.** Every promotion path is threshold-driven; no deleter is count- or score-aware (ass-103 I-4, B-2). ass-079 posed the four options and produced **no findings**. Largest single unresolved question this track surfaces.

**O-7. Structural REMOVE has been a named gap since ass-040 and remains unbuilt.** *"~2,491 quarantined phantom entries… **no atomic delete + HNSW removal exists**."* Now ~2,504. Separately, ass-091 deliberately **removed** the only purge verb as "dead surface" — correct locally, leaving no purge lifecycle at all.

**O-8. Retain-on-error was never established as a convention.** ass-103 OSD: TypedGraph/PhaseFreq/Effectiveness/Contradiction retain on error; ConfidenceState overwrites with degenerate values (I-3); PhaseFreq overwrites with empty on a *successful* thin rebuild (I-5). No spike ever decided what a Principle-7 cache does when its rebuild fails.

**O-9. Measurement blindness is documented and accepted.** ass-074: P@K/MRR collapse to a cosine proxy, so the platform is *"steering its graph features blind."* SL-METRIC (#5572) says the same at capability level. **Every graph-side disposition citing an MRR delta inherits that caveat.**

**O-10. Where a measurement gate existed, the corpus self-corrected.** ass-074 overturned ass-037 Q3b; ass-038 disproved the density hypothesis its own work was built to test; ass-039 invalidated the entire prior eval scenario set; ass-088 corrected its own hypothesis twice; ass-032 corrected ass-031 with live data. **Where none existed, dispositions rest on code reading alone.**

---

## Unanswered Questions

1. **What is the disposition of behavioral `Informs`?** ass-079 posed four ranked options and **never ran**; also flagged first-write-wins weight-freeze as untested. *Needs another spike — direct owner of the edge-demotion hole (O-6).*
2. **Do the scale-gated deferrals now qualify?** Current corpus size against each numeric trigger not measured. *Out of scope for document mining; needs a measurement pass.*
3. **Was the MicroLoRA-vs-scalar-boost overlap ever evaluated?** Deferred to col-015 / GH #50; architecture called "transitional," the 0.03 cap "provisional." No later spike revisits it. *Blocked — no findings resolve it.*
4. **What is the disposition of ass-042's questions?** (write attribution, cross-agent contradiction cadence, poisoning-based privilege escalation, per-agent rate limiting). *SCOPE-only, no findings. Overlaps SLN1 (#5528), itself `delivery:asserted` with an "uncharacterized" threat model.*
5. **Does the GRAPH_EDGES-Supersedes dual-source gap matter?** Supersedes rows written but skipped by the graph builder; `revision_reason` "invisible to all graph traversal logic." *Out of scope for Track A; belongs to B/C.*
6. **What replaces `sessions.keywords`?** "dead schema… implement or remove" — never answered anywhere. *Unresolved across the whole corpus.*
7. **Does `context_correct`'s hard reset of learning columns cause material signal loss?** ass-093 flagged it "under-considered." *Needs another spike.*

---

## Out-of-Scope Discoveries

- **→ Track B:** ass-088's option set is already a near-complete answer (§1d). Two decisions were explicitly left to the ADR: keep-or-delete the write-side fast-path, and new `EdgeConvergenceJob` vs extending Job 2. ass-057 OQ-B-4 additionally leaves the TypedRelationGraph staleness window fully open ("accept / fallback-SQL / partial-refresh").
- **→ Track C (latent params):** the LATENT rows here are a first-pass inventory. Highest-value and cheapest: `s2_vocabulary` (empty → whole op no-op), the hardcoded S1 `≥3`, `freshness_half_life_hours` (global not per-category), `RelatedTo`'s absent write path, `resolve_supersessions` default-OFF, `ResourceClass`, `graph_edges.metadata`.
- **→ Track C (structural gaps):** all three have prior art. Purge — ass-040 Grp7 (named, costed, unbuilt). HNSW heal — **partially exists** (ass-032 #444 heal + prune shipped), so the gap is *repair breadth*, not absence. Monitoring — ass-047 Q7 already specifies the Prometheus surface at ~2 days and calls it a production blocker.
- **Two capability nodes are `delivery:proven` on an unrecorded condition** — RETAIN (#5581) and C5 (#5550) rest entirely on the tick completing (ass-103). Worth checking whether other `proven` nodes carry unrecorded liveness conditions.
- **`unimatrix run` (ass-066) would reshape the signal supply.** Event-driven session host assessed feasible (2–3 weeks observation parity), deferred. If it lands, several tick-based observation paths become redundant.
- **ass-020's critical tick findings recur verbatim in ass-103** — `compute_report` inflation (#1777 → I-17) and panic-kills-the-tick (P1 → I-19). Old audit findings re-emerging in new locations.

---

## Recommendations Summary

- **Open the synthesis on freshness half-life.** It is the universal-vs-domain-tunable thesis already documented, already costed (~1h to externalize; 2–3d per-category), already deferred twice, and in direct tension with a ratified `delivery:proven` NFR.
- **Classify the LATENT rows before proposing anything new** — much of the "target state" already exists unwired. Separate *activate/expose* from *build new* using §1–§8 LATENT rows as the input set.
- **Carry ass-088's option set into Track B intact — do not re-derive it.** Most complete architectural analysis in the corpus; already reached a hybrid verdict with two decisions reserved for an ADR.
- **Treat edge demotion (O-6) as an open question with no owner, not a gap with a known answer.** Any target state adding edge sources without a demotion owner compounds ass-103 I-18.
- **Do not re-litigate the GNN from first principles.** Two complete designs, one reasoned de-scoping (exposure bias #3429). The live question is keep-the-staging vs retire-the-staging; evidence favours retire-with-reason unless the exposure-bias objection is answered.
- **Treat NLI/contradiction as domain-conditional by ratified decision, not an open cost/value question.** #5548 already says "Do NOT prune on single-domain utility"; ass-092 already scoped the restore as one branch.
- **Discount every graph-side MRR-based disposition by the measurement caveat (O-9).**
- **Re-check the scale-gated deferrals as a batch (O-3).** Several triggers may already be open; nothing re-evaluates them.

---

**Confidence**: DIRECTIONAL. Every row sourced to a spike file (+section) or Unimatrix entry ID; dispositions quoted or tightly paraphrased, never inferred — sources stating none are marked UNSTATED. Coverage of the 34 named spikes + ass-103 is complete, with ass-042/079/090 flagged SCOPE-only. O-1…O-10 are patterns reasoned *from* the ledger, not measured. No Unimatrix writes were made.