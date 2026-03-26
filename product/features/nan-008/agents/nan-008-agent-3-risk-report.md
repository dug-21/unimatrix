# Agent Report: nan-008-agent-3-risk

## Output

- `/workspaces/unimatrix-nan-008/product/features/nan-008/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 1 |
| High | 7 |
| Medium | 5 |
| Low | 1 |
| **Total** | **13** |

## Top Risks

**R-01 (Critical)**: Dual type copy divergence — `runner/output.rs` and `report/mod.rs` updated
out of sync; `serde(default)` silences the gap. Required coverage: round-trip integration test
`test_report_round_trip_cc_at_k_icd_fields_and_section_6` with non-zero, non-round field values
(ADR-003). Pattern #3512 confirms this is a documented recurring failure for this codebase.

**R-02 (High)**: Section-order regression — section 6 misplaced or duplicated in `render.rs`.
Required coverage: position assertion `pos(## 1.) < ... < pos(## 6.)` on the full rendered
string. Pattern #3426 (formatter overhaul → golden-output test mandatory) directly applies.

**R-05 (High)**: NaN propagation from `ln(0)` in `compute_icd` — if zero-count categories are
included in the entropy sum, `0.0 * ln(0.0) = NaN` propagates silently through aggregation and
JSON. Required coverage: implementation must iterate only over categories with count > 0; unit
tests at ICD = 0.0 (single category) and ICD = ln(n) (uniform) verify boundaries.

**R-07 (High)**: Backward-compat break — pre-nan-008 result JSON without new fields causes
deserialization error if any new field is missing `#[serde(default)]`. Required coverage:
one test deserializing a stripped JSON asserting 0.0 defaults and exit 0.

**R-08 (High)**: `ScoredEntry.category` mapping gap — if `se.entry.category` is never assigned
in `replay.rs`, all entries have `category = ""` and metrics compute silently wrong values.
Required coverage: integration test with multi-category fixture asserting non-empty category
strings in output JSON and non-zero CC@k and ICD.

## Novel Edge Case Identified

The CC@k formula as stated in SCOPE.md (FR-04) counts distinct categories in the result set
that match any configured category. If a result entry carries a category string *not* in
`configured_categories`, the formula behaviour depends on whether the numerator filters to
`configured_categories` ∩ result-categories or counts all distinct result categories. This is
underspecified and CC@k could exceed 1.0 in the former interpretation. This edge case must be
confirmed in the specification and tested explicitly.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #3493 (bugfix-383 retro), #141 (glass-box validation), #167 (gate result handling); no directly applicable prior failures.
- Queried: `/uni-knowledge-search` for "risk pattern" (category: pattern) — found #3426 (section-order regression → golden-output test, directly applied to R-02), #3512 (dual-type eval harness constraint, directly applied to R-01).
- Queried: `/uni-knowledge-search` for "eval harness dual type copy" — #3512 and #2806 confirmed; #3472 (atomic update for duplicated structures, col-027) corroborated R-01 severity.
- Queried: `/uni-knowledge-search` for "Shannon entropy floating point" — found #3521 (ADR-002 nan-008 ICD, confirming scope decision); #3472 (pre-post differential atomic update pattern).
- Queried: `/uni-knowledge-search` for "serde default backward compatibility" — found #646, #320, #923, #652 confirming `#[serde(default)]` is the established pattern; no novel findings.
- Stored: entry #3525 "Shannon entropy implementations must skip zero-count categories to avoid ln(0) = -inf NaN propagation" via `/uni-store-pattern` (novel — not previously captured; generalises beyond nan-008 to any future entropy-based metric).
