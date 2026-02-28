# ASS-013: Compound Signal Correlation

## The Observation

During crt-006 analysis, a human observed that unusually long Session 2 duration AND high post-PR iteration effort were correlated — and hypothesized "scope too large" as the root cause. Supporting data was then found: 9 new source files, 35 design artifacts, 4 ADRs, a session timeout, a cold restart, and 2 follow-up issues filed post-delivery.

## The Honest Problem

This correlation was discovered by a human who already suspected the conclusion. The data was then searched for confirming evidence. That's a valid form of analysis, but it's not automated detection.

A system that could discover this correlation autonomously would need to:
1. Know which metrics to track (solved — rule-based collection)
2. Know which metrics to compare against each other (partially solved — present them together)
3. Recognize when multiple metrics are co-elevated (unsolved for novel correlations)
4. Attribute meaning to the correlation (requires LLM or human reasoning)

## What We Can Do vs. What We Can't

### Can do now (rule-based):
- Collect per-feature metric vectors (source files, artifacts, durations, restarts, post-completion work, issues)
- Present current feature metrics alongside historical baselines
- Flag individual metrics that exceed thresholds

### Can do with accumulated data (statistical):
- After N features, compute pairwise correlation between metrics
- Surface "metrics that tend to move together" (no causal claim, just co-occurrence)
- This is thin with <20 data points and noisy with non-independent variables

### Requires LLM:
- Attributing meaning: "these co-elevated metrics suggest scope was too large"
- Recommending action: "consider splitting features with >N source files into sub-features"
- Distinguishing correlation from coincidence in small datasets

### Cannot do (fundamental limitation):
- Hard-code compound signal thresholds from 1 data point
- Prove causation from observational data
- Know in advance which correlations matter

## Design Approach: Metric Table + LLM Reasoning

Rather than building a compound signal detector, build a **feature metric table** that accumulates over iterations. The `/retrospective` report presents:

1. **Current feature metrics** — the atoms collected during this cycle
2. **Historical comparison** — same metrics for previous N features (table format)
3. **Anomaly flags** — individual metrics that exceed their evolving threshold
4. **Correlation prompt** — explicitly ask the LLM: "Given this feature's metrics compared to historical baselines, do you see any correlated patterns that suggest systemic issues?"

The LLM does the reasoning. Unimatrix provides the structured data.

### Example Metric Table (accumulated over features)

```
Feature    | Files | Artifacts | 3b-min | Restarts | Post-% | Issues | Timeouts
-----------|-------|-----------|--------|----------|--------|--------|---------
crt-001    |     2 |        12 |     15 |        0 |     3% |      0 |        0
crt-002    |     3 |        14 |     20 |        0 |     5% |      0 |        0
crt-003    |     4 |        18 |     25 |        0 |     4% |      1 |        0
crt-004    |     2 |        10 |     12 |        0 |     2% |      0 |        0
crt-005    |     3 |        16 |     18 |        0 |     6% |      0 |        0
crt-006    |     9 |        35 |     40 |        1 |    12% |      2 |        1  ← outlier
```

With this table, even without statistical analysis, the LLM (or human) can see: crt-006 is an outlier on every dimension. The correlation is visible by inspection.

### Promoted Compound Signals

When the LLM identifies a correlation across multiple retrospectives, the human can choose to promote it:
- "Yes, scope-too-large is a real pattern — track it"
- Unimatrix stores it as a known compound signal with the contributing metrics
- Future retrospectives check for it explicitly
- The compound signal starts with broad thresholds and converges like individual metrics

This makes compound signals **learned from evidence and confirmed by humans**, not invented by engineers.

## Metrics to Collect Per Feature (v1)

### Scope Metrics
- New source files created (Write tool, *.rs filter)
- Design artifacts produced (files in feature directory)
- ADR count
- Component count (distinct pseudocode files)

### Execution Metrics
- Phase durations (from task state transitions)
- Compile cycles per phase
- Total tool calls per phase
- Coordinator respawn count

### Context Engineering Metrics
- Agent hotspot count (from hotspot detection)
- Cold restart events
- Total context loaded (KB read, discounting edit echo-back)
- Session timeouts

### Outcome Metrics
- Post-completion tool calls as % of total
- Follow-up issues created
- Knowledge entries stored
- Permission friction events

### Dismissed Hotspot Feedback
- Which hotspots the human reviewed and dismissed
- Which hotspots led to action

## Open Questions

1. Where to store the feature metric table — Unimatrix entries, a dedicated table, or a file?
2. How to handle features with very different natures (research spike vs. full implementation)?
3. Minimum iteration count before statistical correlation is meaningful?
4. Should the LLM correlation prompt be standardized or customized per project?
5. How to capture the human's "promote this correlation" decision durably?
