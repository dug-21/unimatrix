# ASS-017: Knowledge Base Health Analysis

**Date:** 2026-03-13
**Snapshot:** 1,166 active entries, 188 deprecated, 5,263 co-access pairs, 136 outcomes
**Lambda:** 0.7097

---

## 1. Confidence System Analysis

### 1.1 Score Distribution: Collapsed to Noise

The confidence calibration data shows severe clustering:

| Confidence Bucket | Injections | % of Total |
|-------------------|------------|------------|
| 0.4-0.5 | 3 | 2.3% |
| 0.5-0.6 | 117 | 87.9% |
| 0.6-0.7 | 13 | 9.8% |
| All other buckets | 0 | 0% |

**88% of all injected entries score between 0.5 and 0.6.** The confidence dimension produces a ~0.08 range across the entire knowledge base.

### 1.2 Root Cause: Three Neutral Defaults

The confidence formula is a weighted composite of 6 factors summing to 0.92:

| Factor | Weight | Typical Value | Why It's Stuck |
|--------|--------|--------------|----------------|
| Base (status) | 0.18 | 0.5 | All active entries = 0.5. No differentiation within active entries. |
| Helpfulness | 0.14 | 0.5 (neutral) | Wilson score requires 5+ votes. Almost nothing reaches this threshold. |
| Corrections | 0.14 | 0.5 | 88 correction chains across 1,166 entries. Most entries have 0 corrections. |
| Trust | 0.14 | 0.35-0.50 | agent=0.5 (693 entries), auto=0.35 (604 entries). Binary split, not a gradient. |
| Usage | 0.14 | variable | Log-transformed access count. Some variance, but compressed by log. |
| Freshness | 0.18 | decaying | Exponential decay (168h half-life). Only factor with real movement. |

**Three factors (base, helpfulness, corrections) collectively contribute 0.46 weight but return the same value (0.5) for nearly every entry.** This collapses 50% of the formula to a constant.

### 1.3 Worked Example

Typical active entry with 10 accesses, 1 day old, agent-created, no votes, no corrections:

```
confidence = 0.18*0.5    (base: active)
           + 0.14*0.61   (usage: ln(11)/ln(51))
           + 0.18*0.88   (freshness: exp(-24/168))
           + 0.14*0.5    (helpfulness: neutral, <5 votes)
           + 0.14*0.5    (corrections: 0 corrections)
           + 0.14*0.5    (trust: agent)
           = 0.09 + 0.085 + 0.158 + 0.07 + 0.07 + 0.07
           = 0.543
```

Compare with a 7-day-old entry with 50 accesses:

```
confidence = 0.18*0.5 + 0.14*1.0 + 0.18*0.37 + 0.14*0.5 + 0.14*0.5 + 0.14*0.5
           = 0.09 + 0.14 + 0.067 + 0.07 + 0.07 + 0.07
           = 0.507
```

**A 5x usage difference and 7-day age difference produces a 0.036 score delta.** The compressed factors dominate.

### 1.4 Impact on Search Ranking

Search re-ranking formula: `final_score = 0.85 * similarity + 0.15 * confidence`

With confidence range of ~0.08 (0.52 to 0.60):
- Maximum ranking impact: `0.15 * 0.08 = 0.012`
- This is smaller than the co-access boost cap (0.03) and the provenance boost (0.02)
- **Confidence is the weakest signal in re-ranking despite being the most complex to compute**

### 1.5 The Feedback Loop That Never Fires

The helpfulness factor was designed as the primary quality signal. It uses Wilson score lower bound at 95% confidence, which is a sound statistical approach. But the minimum sample size is 5 votes, and almost no entry reaches this threshold because:

1. Helpful/unhelpful feedback is opt-in (agents pass `helpful: true/false` on retrieval)
2. Most entries are accessed by automated flows that don't provide feedback
3. Even frequently-accessed entries rarely accumulate 5+ explicit votes

The result: the most discriminating factor in the formula is permanently disabled.

---

## 2. Co-Access Pair Analysis

### 2.1 Volume and Growth

- 5,263 active pairs across 1,166 active + 188 deprecated entries
- Theoretical maximum pairs for 1,354 entries: ~916,000
- Actual density: 0.57% — sparse, as expected

### 2.2 Top Pairs Assessment

| Pair | Count | Relationship | Assessment |
|------|-------|-------------|------------|
| #296 (Service Extraction) ↔ #301 (Crate Restructuring) | 11 | Both `unimatrix-server` procedures, used during vnc-008 | True signal |
| #141 (Glass Box Validation) ↔ #167 (Gate Result Handling) | 9 | Both validation/gate workflow concepts | True signal, but both deprecated |
| #239 (Feature Naming) ↔ #241 (Knowledge Categories) | 9 | Both project conventions, human-authored | True signal |
| #261 (AuditSource Security) ↔ #315 (Test Gateway Pattern) | 9 | Security pattern + its test pattern | Strong signal |
| #262 (ServiceError Conversion) ↔ #281 (Caller-Parameterized Service) | 9 | Both service-layer architecture patterns | Strong signal |

**Verdict: Co-access pairs are semantically meaningful.** The system correctly identifies entries that belong together.

### 2.3 Deprecated Entry Accumulation

Co-access pairs involving deprecated entries continue to accumulate counts because:
- Recording layer (`record_co_access_pairs`) has no status filter
- Session dedup (`filter_co_access_pairs`) is status-unaware
- Status filtering only happens at boost computation time (crt-010)

This means:
1. Deprecated entries inflate the "top pairs" view in status reports
2. Co-access counts between deprecated entries are wasted storage writes
3. If a deprecated entry is later quarantined, its co-access counts persist but are fully excluded from boost

**Not a correctness bug** — deprecated entries are excluded from search boost. But it's a hygiene issue that creates misleading status output.

### 2.4 Search Boost Effectiveness

Co-access boost formula: `boost = ln(1 + count) / ln(21) * 0.03`

| Co-Access Count | Boost |
|-----------------|-------|
| 1 | 0.0068 |
| 5 | 0.0176 |
| 9 (top pairs) | 0.0227 |
| 11 (highest) | 0.0245 |
| 20 (saturation) | 0.0300 |

The boost cap of 0.03 is **2.5x larger than the effective confidence range** (0.012). Co-access is a stronger ranking signal than confidence despite being a simpler mechanism.

---

## 3. Coherence (Lambda) Breakdown

| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|----------|
| Freshness | 0.295 | 0.35 | 0.103 |
| Graph Quality | 1.000 | 0.30 | 0.300 |
| Embedding Consistency | 1.000 | 0.15 | 0.150 |
| Contradiction Density | 1.000 | 0.20 | 0.200 |

**Lambda = 0.710** (dragged down entirely by confidence freshness)

822 of 1,166 entries (70.5%) have stale confidence scores (oldest: 14 days). Running `maintain: true` would refresh these and push lambda toward ~0.85+. But given the confidence clustering analysis above, refreshing stale confidence scores updates values that don't meaningfully differentiate entries anyway.

---

## 4. Effectiveness Analysis

| Category | Count | % |
|----------|-------|---|
| Effective | 1,034 | 88.7% |
| Settled | 62 | 5.3% |
| Unmatched | 70 | 6.0% |
| Ineffective | 0 | 0% |
| Noisy | 0 | 0% |

**88.7% effective with 0% ineffective is a strong signal** — the knowledge base is delivering value. The 70 unmatched entries (mostly base-004 and col-020 ADRs) haven't been retrieved in any tracked session. These may be too specialized or their topics don't match common queries.

### 4.1 Source Utility

| Source | Effective | Settled | Unmatched | Utility |
|--------|-----------|---------|-----------|---------|
| agent | 391 | 57 | 70 | 1.00 |
| auto | 604 | 0 | 0 | 0.00 |
| system | 39 | 5 | 0 | 1.00 |

**`auto` source has 0.00 utility** — 604 entries (52% of the KB) sourced from automated processes show zero effectiveness signal. These are likely observation-pipeline entries that get stored but never retrieved through search. They may be dragging down confidence freshness without contributing to agent workflows.

---

## 5. Summary of Findings

### What's Working

1. **Co-access pairs** — semantically meaningful, correctly identifies related entries
2. **Effectiveness tracking** — 88.7% effective rate demonstrates the KB delivers value
3. **Correction chains** — 101 supersession relationships creating clean knowledge evolution
4. **Zero contradictions** — knowledge consistency is maintained
5. **Search re-ranking** — vector similarity (0.85 weight) carries the ranking correctly; supplementary signals add real value

### What's Broken or Weak

1. **Confidence differentiation** — scores collapse to 0.52-0.60 range, providing ~0.012 max ranking impact
2. **Helpfulness feedback loop** — 5-vote minimum never reached, disabling the primary quality signal
3. **Trust factor granularity** — binary split (agent 0.5 vs auto 0.35) with 97% of entries in two buckets
4. **Auto-sourced entry utility** — 604 entries (52% of KB) with zero effectiveness signal
5. **Deprecated co-access accumulation** — misleading status output, wasted writes

### Systemic Issue

The confidence system was designed around a feedback-rich environment where entries would accumulate votes, corrections, and varied trust signals over time. In practice, the knowledge base operates in a **low-feedback, high-automation environment** where:
- Entries are created by agents and auto-processes, not humans
- Retrieval happens in automated pipelines that don't provide quality feedback
- Most entries are young (project is ~2 weeks of active knowledge accumulation)
- The correction mechanism works well but affects a small percentage of entries

The confidence formula needs to be re-evaluated for the actual operating environment, not the theoretical one it was designed for.

---

## 6. Relationship to ASS-017 petgraph Analysis

The petgraph analysis (companion document) proposes graph-derived penalties to replace hardcoded `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY`. This is complementary — graph topology would add structural signal where confidence currently provides noise. The recommendation to start with supersession graph traversal (petgraph Phase 1) addresses one specific weakness without requiring a full confidence system redesign.

However, the confidence clustering problem identified here is **upstream** of petgraph — even with graph-derived penalties, the base confidence score fed into re-ranking will remain in a narrow band unless the formula itself is restructured.
