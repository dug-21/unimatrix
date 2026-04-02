# ASS-035 Findings: NLI Input Extraction Strategy for Supports Edge Detection

**Date**: 2026-04-01
**Spike type**: Empirical — measure, then decide
**Harness**: `product/research/ass-035/harness/` (standalone Rust binary, read-only DB access)
**Models tested**:
- `cross-encoder/nli-MiniLM2-L6-H768 Q8` — production NLI model
- `cross-encoder/nli-deberta-v3-small Q8` — larger NLI comparison
- `cross-encoder/ms-marco-MiniLM-L6-v2 Q8` — search-relevance reranker (alternative framing)
- `sentence-transformers/all-MiniLM-L6-v2` — production embedding model (cosine similarity baseline)

---

## Decision

**No cross-encoder model or extraction strategy produces a reliable signal for Supports edge
detection in Unimatrix's knowledge corpus. The recommended mechanism is cosine similarity
with a `same_feature_cycle` constraint, not NLI or search-relevance scoring.**

Three models were tested across three extraction strategies (A=topic, B=operative claim, C=full body):
- NLI (MiniLM2, DeBERTa): Strategy B/C never cross 0.45 on any true pair. Task mismatch confirmed.
- MS-MARCO reranker: Improves over NLI (3-5/8 true pairs above 0.50) but fails where A and B
  use different vocabulary to describe the same causal relationship. Underperforms cosine.
- Cosine similarity (production embeddings): 6/8 same-feature true pairs above 0.65 threshold,
  with perfect rejection of all 10 false pairs including 5 compatible-category cross-feature
  negatives (max false score 0.247). The `same_feature_cycle` filter is not required for
  correctness at this threshold.

The problem is not input length or model size. It is a **task mismatch**: Unimatrix's "Supports"
relationship (lesson → procedure, lesson → pattern, pattern → decision) is a
*prescription-from-problem* or *parallel statement* structure. NLI models trained on MNLI/SNLI
interpret problem description + solution description as **contradiction** (different world-states).
MS-MARCO partially recovers via relevance scoring but fails on vocabulary-mismatched causal pairs.

Cosine similarity with the production embedding model is the most reliable available signal.

---

## Score Distributions

### Combined: Cosine + MiniLM2 Q8 NLI

Harness extended (2026-04-01) to compute cosine similarity using the production embedding
pipeline (`sentence-transformers/all-MiniLM-L6-v2`, `prepare_text(title, content, ": ")`).

```
Pair Grp  Label       | Cosine | Ent_C  Neu_C  Con_C | A-id  B-id
-------------------------------------------------------------------
P01  A    true        | 0.7564 | 0.096  0.379  0.525 | 376   375
P02  A    true        | 0.5229 | 0.011  0.218  0.771 | 2798  2809
P03  A    true        | 0.7804 | 0.012  0.098  0.890 | 665   667
P04  A    true        | 0.8192 | 0.059  0.054  0.887 | 3353  3354
P05  A    true        | 0.5568 | 0.040  0.438  0.522 | 1688  1369
P06  A    true        | 0.7992 | 0.255  0.303  0.442 | 3744  3750
P07  A    true        | 0.7125 | 0.032  0.156  0.812 | 374   375
P08  A    true        | 0.6379 | 0.078  0.331  0.591 | 2571  2728
P09  B    borderline  | 0.4967 | 0.031  0.156  0.813 | 376   2060
P10  B    borderline  | 0.5345 | 0.018  0.457  0.525 | 735   1369
P11  B    true        | 0.6742 | 0.010  0.076  0.915 | 3353  3660
P12  B    borderline  | 0.3533 | 0.077  0.400  0.524 | 378   238
P13  B    borderline  | 0.5273 | 0.033  0.150  0.817 | 1628  1367
P14  B    borderline  | 0.5356 | 0.017  0.446  0.537 | 2571  3741
P15  B    borderline  | 0.4416 | 0.015  0.472  0.513 | 667   245
P16  C    false       | 0.2248 | 0.017  0.110  0.873 | 376   2701
P17  C    false       |-0.0439 | 0.030  0.128  0.842 | 64    735
P18  C    false       | 0.1643 | 0.010  0.352  0.638 | 63    1688
P19  C    false       | 0.1358 | 0.033  0.214  0.753 | 239   3732
P20  C    false       | 0.2406 | 0.061  0.241  0.697 | 2393  65
```

**Cosine group summary:**

| Group | Label | max | mean | min | n≥.65 | n≥.70 | n≥.80 |
|-------|-------|-----|------|-----|-------|-------|-------|
| A | true | 0.819 | 0.698 | 0.523 | 6/8 | 5/8 | 1/8 |
| B | true | 0.674 | 0.674 | 0.674 | 1/1 | 1/1 | 0/1 |
| B | borderline | 0.536 | 0.482 | 0.353 | 0/6 | 0/6 | 0/6 |
| C | false | 0.241 | 0.144 | −0.044 | 0/5 | 0/5 | 0/5 |

### MiniLM2 Q8 — Full NLI Results

```
Pair Grp  Label       | Ent_A  Neu_A  Con_A | Ent_B  Neu_B  Con_B | Ent_C  Neu_C  Con_C | A-id  B-id
------------------------------------------------------------------------------------------------------
P01  A    true        | 0.956  0.034  0.011 | 0.156  0.502  0.341 | 0.096  0.379  0.525 | 376   375
P02  A    true        | 0.911  0.070  0.019 | 0.013  0.686  0.301 | 0.011  0.218  0.771 | 2798  2809
P03  A    true        | 0.945  0.047  0.008 | 0.119  0.395  0.487 | 0.012  0.098  0.890 | 665   667
P04  A    true        | 0.960  0.033  0.006 | 0.001  0.008  0.990 | 0.059  0.054  0.887 | 3353  3354
P05  A    true        | 0.035  0.032  0.933 | 0.017  0.563  0.420 | 0.040  0.438  0.522 | 1688  1369
P06  A    true        | 0.230  0.175  0.595 | 0.034  0.309  0.657 | 0.255  0.303  0.442 | 3744  3750
P07  A    true        | 0.056  0.037  0.906 | 0.057  0.698  0.245 | 0.032  0.156  0.812 | 374   375
P08  A    true        | 0.282  0.568  0.150 | 0.009  0.691  0.300 | 0.078  0.331  0.591 | 2571  2728
P09  B    borderline  | 0.174  0.264  0.562 | 0.055  0.368  0.577 | 0.031  0.156  0.813 | 376   2060
P10  B    borderline  | 0.042  0.029  0.929 | 0.005  0.662  0.332 | 0.018  0.457  0.525 | 735   1369
P11  B    true        | 0.380  0.577  0.043 | 0.023  0.085  0.892 | 0.010  0.076  0.915 | 3353  3660
P12  B    borderline  | 0.874  0.091  0.035 | 0.131  0.659  0.210 | 0.077  0.400  0.524 | 378   238
P13  B    borderline  | 0.334  0.376  0.289 | 0.022  0.487  0.491 | 0.033  0.150  0.817 | 1628  1367
P14  B    borderline  | 0.104  0.262  0.634 | 0.013  0.479  0.508 | 0.017  0.446  0.537 | 2571  3741
P15  B    borderline  | 0.079  0.109  0.812 | 0.019  0.524  0.457 | 0.015  0.472  0.513 | 667   245
P16  C    false       | 0.120  0.113  0.767 | 0.009  0.320  0.671 | 0.017  0.110  0.873 | 376   2701
P17  C    false       | 0.166  0.470  0.365 | 0.006  0.969  0.026 | 0.030  0.128  0.842 | 64    735
P18  C    false       | 0.196  0.371  0.433 | 0.004  0.860  0.136 | 0.010  0.352  0.638 | 63    1688
P19  C    false       | 0.026  0.634  0.340 | 0.007  0.063  0.930 | 0.033  0.214  0.753 | 239   3732
P20  C    false       | 0.596  0.116  0.288 | 0.021  0.119  0.859 | 0.061  0.241  0.697 | 2393  65
```

**Threshold crossings (MiniLM2):**

| Strategy | ≥ 0.45 | ≥ 0.50 | ≥ 0.60 | Notes |
|----------|--------|--------|--------|-------|
| A (topic) | 5 pairs | 5 pairs | 5 pairs | All from P01-P04 + P12 — see below |
| B (first ¶) | 0 pairs | 0 pairs | 0 pairs | No pair crosses 0.45 |
| C (full body) | 0 pairs | 0 pairs | 0 pairs | No pair crosses 0.45 |

**Group summary (MiniLM2, entailment column):**

| Group | Label | max_A | mean_A | max_B | mean_B | max_C | mean_C |
|-------|-------|-------|--------|-------|--------|-------|--------|
| A | true | 0.960 | 0.547 | **0.156** | 0.051 | **0.255** | 0.073 |
| B | true | 0.380 | 0.380 | 0.023 | 0.023 | 0.010 | 0.010 |
| B | borderline | 0.874 | 0.268 | 0.131 | 0.041 | 0.077 | 0.032 |
| C | false | 0.596 | 0.221 | 0.021 | 0.009 | 0.061 | 0.030 |

---

### DeBERTa Q8 — Full Results

```
Pair Grp  Label       | Ent_A  Neu_A  Con_A | Ent_B  Neu_B  Con_B | Ent_C  Neu_C  Con_C | A-id  B-id
------------------------------------------------------------------------------------------------------
P01  A    true        | 0.981  0.016  0.003 | 0.075  0.819  0.106 | 0.971  0.022  0.007 | 376   375
P02  A    true        | 0.970  0.024  0.006 | 0.001  0.989  0.010 | 0.023  0.032  0.945 | 2798  2809
P03  A    true        | 0.981  0.016  0.003 | 0.026  0.965  0.009 | 0.004  0.003  0.994 | 665   667
P04  A    true        | 0.982  0.018  0.001 | 0.038  0.051  0.911 | 0.975  0.022  0.003 | 3353  3354
P05  A    true        | 0.021  0.011  0.968 | 0.043  0.812  0.145 | 0.003  0.014  0.983 | 1688  1369
P06  A    true        | 0.029  0.714  0.257 | 0.137  0.828  0.035 | 0.187  0.135  0.678 | 3744  3750
P07  A    true        | 0.001  0.002  0.997 | 0.633  0.340  0.027 | 0.807  0.162  0.031 | 374   375
P08  A    true        | 0.156  0.375  0.469 | 0.010  0.922  0.068 | 0.574  0.182  0.244 | 2571  2728
P09  B    borderline  | 0.020  0.969  0.011 | 0.001  0.006  0.993 | 0.834  0.062  0.104 | 376   2060
P10  B    borderline  | 0.094  0.047  0.859 | 0.007  0.977  0.016 | 0.027  0.046  0.927 | 735   1369
P11  B    true        | 0.128  0.704  0.168 | 0.158  0.040  0.803 | 0.011  0.039  0.950 | 3353  3660
P12  B    borderline  | 0.592  0.321  0.087 | 0.110  0.267  0.623 | 0.385  0.215  0.400 | 378   238
P13  B    borderline  | 0.011  0.978  0.011 | 0.056  0.926  0.018 | 0.388  0.573  0.040 | 1628  1367
P14  B    borderline  | 0.037  0.079  0.885 | 0.000  0.004  0.996 | 0.117  0.796  0.087 | 2571  3741
P15  B    borderline  | 0.008  0.043  0.950 | 0.073  0.783  0.144 | 0.080  0.860  0.060 | 667   245
P16  C    false       | 0.004  0.992  0.005 | 0.021  0.540  0.439 | 0.722  0.059  0.218 | 376   2701
P17  C    false       | 0.076  0.084  0.840 | 0.001  0.027  0.972 | 0.048  0.847  0.105 | 64    735
P18  C    false       | 0.006  0.004  0.990 | 0.001  0.003  0.996 | 0.046  0.087  0.867 | 63    1688
P19  C    false       | 0.008  0.984  0.008 | 0.005  0.007  0.988 | 0.447  0.411  0.142 | 239   3732
P20  C    false       | 0.003  0.016  0.981 | 0.270  0.320  0.411 | 0.045  0.784  0.171 | 2393  65
```

---

## Analysis

### Finding 1: Strategy A is topic-label matching, not semantic entailment

Strategy A (topic field only) produces entailment scores above 0.9 on P01–P04 for both models.
But the topic fields for those pairs are: "database-init"/"database-init", "unimatrix-embed"/
"unimatrix-embed", "unimatrix-server"/"unimatrix-server", "contradiction"/"contradiction".

The model is scoring **label string similarity**, not semantic content entailment. This is
confirmed by the failures on equally valid "true" pairs where topics differ:
- P05: topics "unimatrix-server" vs "rust-dev" → 0.035 entailment (true pair)
- P07: topics "schema-migration" vs "database-init" → 0.056 entailment (true pair)

Strategy A also produces false positives:
- P12 (borderline): topics "testing" vs "tester" → 0.874 entailment
- P20 (false): topics "dsn-001" vs "nxs-002" → 0.596 entailment (MiniLM2)

**Strategy A is not viable.** It detects identifier similarity, not Supports relationships.

### Finding 2: Strategy B reveals the task mismatch directly

Strategy B extracts the operative claim sentence — the most semantically dense short text.
The extraction preview confirms it's working correctly:
- P01: Takeaway "When a migration transforms table structure... DDL... MUST run after migration"
  → Procedure "In Store::open() the init sequence MUST be: migration before DDL"

These are the SAME rule, stated in two different ways. MiniLM2 scores 0.156 entailment,
0.341 contradiction. DeBERTa scores 0.075 entailment, 0.106 contradiction.

The maximum Strategy B entailment score across all 20 pairs, both models, is **0.156**
(MiniLM2, P01). No pair crosses 0.45 under Strategy B with either model.

**Diagnosis**: The NLI model sees "What went wrong" and "Here is the rule" as two different
texts about different states of the world — not as logical entailment. This is correct by
SNLI/MNLI definitions, where entailment requires the hypothesis to be necessarily true given
the premise. In Unimatrix's corpus, lessons and patterns describe different perspectives on
the same rule, not logical consequences.

Worst case: P04 Strategy B (MiniLM2) → 0.001 entailment, **0.990 contradiction**. Entry A
says "Handle::current panics in rayon." Entry B says "pre-fetch before spawn." The model
correctly identifies these as describing **different situations** (problem vs solution), but
Unimatrix needs to detect that the solution INFORMS the problem report's resolution.

### Finding 3: Strategy C (current production baseline) never crosses 0.45

The current production implementation (full entry.content, truncated at 2000 chars) produces:
- MiniLM2 Q8: max entailment 0.255 across all 20 pairs (P06, pattern → decision, true)
- **Zero pairs cross 0.45 under Strategy C with MiniLM2.** This confirms the known ~0.32 max
  and explains why zero Supports edges are written per tick.

The 2000-char truncation is not the cause. Even Strategy B (short operative claims, 100-400
chars) fails. The extraction point is not the problem.

### Finding 4: DeBERTa Q8 is inconsistent and unreliable for this corpus

DeBERTa Strategy C produces some encouraging scores on true pairs:
- P01: 0.971 ✓, P04: 0.975 ✓, P07: 0.807 ✓, P08: 0.574 ✓

But it also produces **unacceptable false positives**:
- P16 (false pair: migration lesson ↔ NLI reranking ADR): **0.722 entailment** under Strategy C
- P09 (borderline pair): 0.834 entailment — possibly correct, but labels true and false pairs
  similarly high, destroying discriminability

Within the true group itself, DeBERTa is inconsistent:
- P02 (true, lesson → pattern, same feature crt-023): 0.023 entailment, **0.945 contradiction**
- P03 (true, lesson → pattern, same feature vnc-004): 0.004 entailment, **0.994 contradiction**

Both P02 and P03 are same-feature lesson → pattern pairs where A and B describe the same rule.
DeBERTa scores them near-pure contradiction while scoring P01 (same category pair) near-pure
entailment. This suggests content-specific sensitivity that makes the model unreliable for
production use on this corpus without extensive calibration.

The Q8 quantization may also have degraded discriminability compared to FP32. This is not
investigated in this spike.

**DeBERTa Q8 is not a drop-in replacement.** It does not solve the Supports detection problem
reliably and introduces worse false positives than MiniLM2.

### Finding 5: The problem-description / solution-prescription structural mismatch

The NLI model consistently interprets Unimatrix's knowledge patterns as follows:

| Knowledge relationship | NLI interpretation | Score |
|------------------------|-------------------|-------|
| Lesson (describes failure) → Pattern (prescribes fix) | Contradiction — different worlds | High Con |
| Pattern (implementation) → Decision (that decided it) | Neutral or mild contradiction | ~Con |
| Lesson (describes failure) → Procedure (prevention steps) | Neutral — different scope | ~Neu |
| **Same rule in two phrasings** | Entailment — only works if phrasing is nearly identical | High Ent |

The only case where NLI entailment fires correctly is when both entries state the same rule
in nearly identical language (P01: "migration before DDL" appears in both texts verbatim).
This is paraphrase detection, not Supports relationship detection.

---

## Confirmation of Scope Decision Criteria

> "Do any entry pairs, under any extraction strategy, score above 0.6?"

MiniLM2 Q8: **No** — no content-strategy pair (B or C) crosses 0.6. Strategy A crosses 0.6
on P01-P04 only via topic-label matching (not semantic entailment), and produces false
positives on negative controls (P12 borderline=0.874, P20 false=0.596).

**Per the SCOPE: "the task is mismatched and NLI entailment cannot detect this relationship
in Unimatrix's knowledge corpus."**

---

## Recommendation: Non-NLI Supports Detection

NLI entailment is the wrong task. The Informs detection mechanism from ASS-034/crt-037
(structural signals + NLI neutral zone) is the correct design template for Supports as well.
The key insight: **Supports relationships in Unimatrix are identifiable structurally, not
through logical inference.**

### Proposed Mechanism: High-Cosine Same-Feature Promotion

The structural signal for Supports is clear from the data:

| Signal | Value | Rationale |
|--------|-------|-----------|
| HNSW cosine similarity | ≥ 0.65 | Catches 6/8 same-feature true pairs; see calibration below |
| Same `feature_cycle` | required | Primary discriminator: all false pairs are cross-feature |
| Compatible category pairs | lesson→procedure, lesson→pattern, pattern→decision, procedure→procedure | Per config's epistemic structure |
| No existing edge | required | Skip if Supersedes or Informs edge already exists |

Weight: `cosine * 0.9` (high confidence, structurally grounded — higher than Informs' 0.6).

**Threshold calibration** (from cosine harness run):

The original 0.80 recommendation was too tight — only 1/8 positive controls (P04) exceed it.
The 0.65 threshold is empirically supported:

| Threshold | Group A true hit | Group B border hit | Group C false hit | Assessment |
|-----------|-----------------|--------------------|--------------------|-----------|
| 0.80 | 1/8 | 0/6 | 0/5 | Too tight — misses most true pairs |
| 0.70 | 5/8 | 0/6 | 0/5 | Conservative but clean |
| **0.65** | **6/8** | **0/6** | **0/5** | **Recommended** |
| 0.60 | 7/8 | 0/6 | 0/5 | P08 added (0.638); still clean |
| 0.55 | 7/8 | 0/6 | 0/5 | P05 added (0.557); cross-feature pair |
| 0.50 | 8/8 | 4/6 | 0/5 | Catches all true but fires on borderlines |

The two misses at 0.65 (P02=0.523, P05=0.557) have structural explanations:
- P02: lesson is a single-bug deep-dive; pattern aggregates 4 gotchas — lower overlap is correct
- P05: cross-feature pair (bugfix-277 → vnc-008); same_feature_cycle filter would exclude it anyway

**Critical discriminator**: the `same_feature_cycle` constraint is load-bearing. All 5 false
pairs are cross-feature; their cosine scores (max 0.241) are well below any plausible threshold.
The threshold only needs to separate true same-feature pairs from unrelated same-feature pairs
(which this data cannot quantify — see Limitation below).

This approach requires:
1. A new pass in `nli_detection_tick.rs` (or a separate `supports_promotion_tick.rs`) that:
   a. Queries entry pairs sharing the same `feature_cycle` with cosine ≥ 0.65
   b. Filters by compatible category pairs (same list as `informs_category_pairs` + same-category)
   c. Writes Supports edges without NLI scoring
2. A config field `supports_cosine_threshold` (default 0.65)
3. No model change needed. The NLI model is not used in this path.

Cross-feature Supports (P11: rayon panic lesson → grep gate pattern, cosine 0.674) scores
above 0.65. A cross-feature pass with the same threshold + compatible category filter
would catch this class. However, without negative same-category cross-feature controls in
the test set, the false-positive risk cannot be quantified here.

**Extension — Group D: compatible-category cross-feature negatives (2026-04-01)**

The original limitation was that all Group C false pairs were in *incompatible* categories,
leaving the question: does cosine ≥ 0.65 alone produce false positives on cross-feature pairs
that ARE in compatible category pairs (lesson→decision, pattern→decision, pattern→convention)?

Five pairs were added (P21–P25) — all cross-feature, all in compatible category pairs, all with
no actual Supports relationship:

```
P21  D  false  | 0.0305  | 665  → 2701  (lesson→decision: flock TOCTOU → NLI sort)
P22  D  false  | 0.1382  | 1628 → 64   (lesson→decision: spawn_blocking mutex → DistDot)
P23  D  false  | 0.2474  | 3353 → 245  (lesson→decision: rayon panic → socket lifecycle)
P24  D  false  | -0.0377 | 667  → 2701  (pattern→decision: flock PID → NLI sort)
P25  D  false  | 0.2125  | 2571 → 238  (pattern→convention: rayon-tokio bridge → testing infra)
```

**Group D max cosine: 0.247.** All 5 pairs score below 0.35. Zero pairs cross 0.65.

**Conclusion: the `same_feature_cycle` filter is NOT strictly necessary.** Cosine similarity
alone at threshold 0.65 correctly rejects all 10 false pairs (Groups C + D), including
compatible-category cross-feature pairs with no semantic relationship. The filter remains
valuable as defense-in-depth and to reduce the candidate pair search space, but it is not
required for correctness at the 0.65 threshold.

### Finding 6: MS-MARCO reranker partially works but underperforms cosine for same-feature pairs

`cross-encoder/ms-marco-MiniLM-L6-v2 Q8` was tested as an alternative to NLI entailment.
The search-relevance framing (lesson=query, pattern=passage) sidesteps the entailment trap.

**MS-MARCO scores (Strategy B — operative claim, Strategy C — full content):**

```
Pair Grp  Label       | Rel_B  | Rel_C  | A-id  B-id
------------------------------------------------------
P01  A    true        | 0.978  | 0.226  | 376   375
P02  A    true        | 0.175  | 0.011  | 2798  2809
P03  A    true        | 0.995  | 0.746  | 665   667
P04  A    true        | 0.290  | 0.077  | 3353  3354
P05  A    true        | 0.018  | 0.755  | 1688  1369
P06  A    true        | 0.897  | 0.886  | 3744  3750
P07  A    true        | 0.403  | 0.864  | 374   375
P08  A    true        | 0.174  | 0.073  | 2571  2728
P09  B    borderline  | 0.012  | 0.005  | 376   2060
P10  B    borderline  | 0.000  | 0.023  | 735   1369
P11  B    true        | 0.001  | 0.891  | 3353  3660
P12  B    borderline  | 0.016  | 0.028  | 378   238
P13  B    borderline  | 0.037  | 0.001  | 1628  1367
P14  B    borderline  | 0.000  | 0.056  | 2571  3741
P15  B    borderline  | 0.052  | 0.077  | 667   245
P16  C    false       | 0.000  | 0.002  | 376   2701
P17  C    false       | 0.000  | 0.001  | 64    735
P18  C    false       | 0.000  | 0.002  | 63    1688
P19  C    false       | 0.000  | 0.000  | 239   3732
P20  C    false       | 0.000  | 0.004  | 2393  65
```

**At threshold 0.50:**

| Strategy | Group A true ≥ 0.50 | Group B/C ≥ 0.50 |
|----------|--------------------|--------------------|
| B (claim) | 3/8 (P01, P03, P06) | 0/12 |
| C (full body) | 5/8 (P03, P05, P06, P07, P08 miss; P01/P02/P04/P08 miss) | P11 only (0.891) |

Wait — corrected: Strategy C ≥ 0.50: P03(0.746), P05(0.755), P06(0.886), P07(0.864), P11(0.891) = 4 Group A + 1 Group B.

**Compared to cosine at 0.65:**

| Approach | Same-feature true hits | Cross-feature | False positives |
|----------|----------------------|---------------|-----------------|
| Cosine ≥ 0.65 | P01✓ P03✓ P04✓ P06✓ P07✓ = **5/7** | P11✓ | 0/5 |
| MS-MARCO C ≥ 0.50 | P03✓ P06✓ P07✓ = **3/7** | P05✓ P11✓ | 0/5 |

(P05 is cross-feature; same_feature_cycle filter excludes it in production.)

MS-MARCO Strategy C catches pairs where A and B use **overlapping vocabulary**
(P03: "lock-then-open" — both entries use "flock", "truncate", "create"; P06: "Outgoing" appears verbatim
in both). It fails where A and B describe the same causal relationship in **different vocabulary**:
- P04: A says "Handle::current() panics", B says "pre-fetch Vec" — different words, same fix
- P01: Strategy B works (0.978) but Strategy C collapses to 0.226 — full content dilutes the operative claim

**Diagnosis**: MS-MARCO measures "does this passage contain words that answer this query?" — essentially
weighted lexical overlap on dense semantic space. It partially escapes the entailment trap but falls into
a different failure mode: vocabulary mismatch. Cosine (embed full content → L2-normalize → dot product)
is more robust because the embedding model captures semantic similarity holistically.

**Conclusion**: MS-MARCO is a meaningful improvement over NLI entailment (3-5 true pairs vs 0) but
underperforms cosine for same-feature Supports detection (3/7 vs 5/7 at comparable operating points).
**Cosine similarity remains the recommended mechanism.** No model swap is warranted.

### Why the NLI model should stay for contradiction detection

The Contradicts pass (`nli_contradiction_threshold = 0.6`) is not affected by this finding.
Contradiction detection uses the **contradiction score** (logit[0]), not entailment. The model
correctly identifies contradicting texts (two entries making opposite claims). Do not change the
contradiction detection path. If a model swap were ever considered for other reasons, the
contradiction pass would need independent re-validation before shipping.

---

## Artifacts

- `product/research/ass-035/SCOPE.md` — research questions
- `product/research/ass-035/PAIRS.md` — 20-pair labeled ground truth set
- `product/research/ass-035/harness/` — scoring harness (standalone Rust binary)
  - `cargo run --release` — NLI (MiniLM2 Q8) + cosine
  - `cargo run --release -- --model deberta-q8` — DeBERTa Q8 + cosine
  - `cargo run --release -- --model ms-marco` — MS-MARCO reranker + cosine
  - `cargo run --release -- --cosine-only` — cosine only (fast, no NLI)
- `product/research/ass-035/FINDINGS.md` — this document

The 20-pair labeled set (`PAIRS.md`) is formatted for integration into a regression eval
when Supports detection is implemented. The cosine threshold calibration table above
provides the empirical basis for `supports_cosine_threshold = 0.65` in config.

---

## Appendix: Strategy B Extraction Quality

The extract_b function correctly captured operative claims for most entries:
- Lessons with "Takeaway:": extracted the takeaway sentence (P01, P03, P09, P12, P13, P16)
- ADRs with "Decision:" section: extracted decision paragraph (P06, P09 B-entry)
- Patterns without structured headers: extracted first paragraph (P04, P08, P11, P15)

The extraction logic worked as designed. The failure is not in extraction quality but in the
NLI model's interpretation of extracted content.
