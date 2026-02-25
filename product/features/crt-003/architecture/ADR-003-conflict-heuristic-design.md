# ADR-003: Conflict Heuristic Design

## Status

Accepted

## Context

After identifying entry pairs with high embedding similarity (>0.85), we need to determine which pairs actually contain conflicting content. Two similar entries could be complementary (both valid, covering different aspects) or contradictory (one says "do X", the other says "don't do X").

A trained NLI model (e.g., bart-large-mnli) would classify pairs as entailment/neutral/contradiction with ~90% accuracy but requires a second ONNX model (~1.5GB), tokenizer, and inference pipeline. This is rejected per SCOPE.md non-goals.

## Decision

Use a multi-signal rule-based heuristic with three weighted signals and a tunable sensitivity threshold.

### Signal 1: Negation Opposition (weight: 0.6)

Extract directive phrases from each entry and check for negation pairs. Directive phrase patterns:

```
Affirmative: "use X", "always X", "prefer X", "should X", "must X", "enable X"
Negative:    "avoid X", "never X", "do not X", "don't X", "should not X", "must not X", "disable X"
```

The detector extracts the subject after the directive verb and compares subjects across entries. If entry A says "use serde" and entry B says "avoid serde", the subject "serde" matches with opposing directives. Score = 1.0 for exact subject match, 0.5 for partial (substring) match.

### Signal 2: Incompatible Directives (weight: 0.3)

Both entries prescribe specific choices for the same category of decision. Detected by:
- Both entries contain "use X" or "prefer X" patterns
- The subjects X and Y are different
- The entries share a topic and category

Example: "Use reqwest for HTTP clients" vs "Use ureq for HTTP clients" -- same decision category (HTTP client), different choices.

Score = 1.0 when different subjects are prescribed for the same topic+category.

### Signal 3: Opposing Sentiment (weight: 0.1)

One entry frames a practice positively and the other negatively. Detected by presence of positive markers ("recommended", "best practice", "preferred") in one entry and negative markers ("anti-pattern", "discouraged", "problematic", "risky") in the other, when both entries share a topic.

Score = 1.0 when opposing sentiment markers are present.

### Composite Score

```
conflict_score = clamp(
    sig1_score * 0.6 + sig2_score * 0.3 + sig3_score * 0.1,
    0.0, 1.0
)
```

### Sensitivity Threshold

The `conflict_sensitivity` parameter (default 0.5) controls the minimum composite score required to flag a pair. The threshold is applied as: flag if `conflict_score >= (1.0 - sensitivity)`.

- sensitivity = 0.9 -> flag if score >= 0.1 (very sensitive, many flags)
- sensitivity = 0.5 -> flag if score >= 0.5 (moderate, default)
- sensitivity = 0.1 -> flag if score >= 0.9 (very specific, few flags)

## Rationale

- **Weighted signals**: Different signal types have different reliability. Negation opposition is the strongest signal (explicit contradiction). Incompatible directives are weaker (could be valid alternatives). Sentiment is weakest (subjective).
- **Tunable threshold**: Avoids hardcoding a binary decision. Users can adjust based on their tolerance for false positives.
- **Extractable patterns**: All three signals use regex-based pattern matching, which is already in the dependency tree (vnc-002 content scanning).
- **Composable**: Signals can be added, removed, or reweighted without structural changes. Future NLI model integration could be added as Signal 4 with high weight.

## Consequences

- False positives will occur for entries that discuss the same topic from different valid perspectives
- False negatives will occur for subtle contradictions that don't use directive language (e.g., implicit assumptions that conflict)
- The pattern library needs periodic review as the knowledge base grows
- The sensitivity parameter provides a user-tunable dial but the default (0.5) should work for most cases
