# Agent Report: crt-057-agent-3-risk-v3 (uni-risk-strategist, architecture-risk mode)

MAJOR REWORK of the risk strategy for the ass-091 redesign. Both risk files rewritten to the read-only
scoped-retrieval axis; boolean-era rows retired.

## Deliverables (updated in place)
- `product/features/crt-057/RISK-TEST-STRATEGY.md` — full rewrite (18 risks, ~61 min scenarios).
- `product/features/crt-057/SCOPE-RISK-ASSESSMENT.md` — refreshed to the new axis (SR-01..SR-14, <100 lines).
- GH #894 comment: https://github.com/dug-21/unimatrix/issues/894#issuecomment-4883332396

## New / changed / retired rows — see the "Retired / Superseded Boolean-Era Risk Rows" table and
Scope Risk Traceability in RISK-TEST-STRATEGY.md. Headline:
- New Critical top risk R-01 (silent false negative). New High R-05 (clock/skew), R-06 (orphan-deletion +
  exhaustive-match re-home), R-09 (scoped-filter). R-07 reframed to fold-read four-site gating.
- Retired: memo-hit-flag-threading (superseded by R-07), force-vs-extract purge precedence, purge
  granularity, purge-keying, OQ-2 warning (demoted), post-purge stale-verbatim (reframed to R-16).
- Coverage delta: 17/44 → 18/61; Critical set now R-01..R-04.

## Note for the synthesizer / spec writer
`specification/SPECIFICATION.md` (12:56) predates the SCOPE rework (17:07) and architecture (17:48) — it is
boolean-era and stale. I anchored traceability to SCOPE AC-01..AC-17, not the spec. Confirm the spec is
being reworked in parallel before the acceptance map binds to it.

## Flag for delivery leader
- ReDoS surface: the caller-supplied `match` regex runs over potentially large candidate blocks — R-05/Security
  recommends a compile-complexity bound or size guard.
- C-8 rebase: confirm no live conflict on `distill_handler.rs` before delivery.

## Knowledge Stewardship
- Queried: /uni-knowledge-search (context_search) for silent-false-negative + loss propagation, cross-plane
  clock skew, protocol/orphan-deletion blast radius. Applied #3385/#3372, #4195/#4236, #5383, #4831, #4044/#4915;
  carried #4879/#4585/#4452/#3548/#5089.
- Stored: nothing novel to store yet — candidate cross-feature pattern ("read-only-retrieval redesign relocates
  risk from destructive-gate-fired to negative-result-honest + sole-backstop-reclamation-correct") deferred
  pending a 2nd-feature confirmation, per the patterns-across-2+-features rule.
