# Agent Report: nan-010-agent-3-risk

## Role
Architecture-Risk Strategist (architecture-risk mode)

## Output
`product/features/nan-010/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 1 |
| High | 7 |
| Med | 5 |
| Low | 3 |

Total: 15 identified risks, 20+ required test scenarios.

## Key Risks for Human Attention

### Escalation Required — Architecture/Scope Conflict (R-07)

ARCHITECTURE.md Component 7 (`run_report` sidecar load) specifies WARN+fallback to empty map on
corrupt `profile-meta.json`. SCOPE.md §Design Decisions #8 specifies abort + exit non-zero on
corrupt sidecar. These are directly contradictory. The delivery agent will implement one or the
other; whichever they choose, the other behavior is untested. A vision guardian or Design Leader
must resolve this before implementation wave 1 begins, by either:

- Updating ARCHITECTURE.md Component 7 to match SCOPE.md Design Decision #8 (abort), or
- Reverting SCOPE.md Design Decision #8 to match ARCHITECTURE.md (WARN+fallback).

The SCOPE.md position (abort) is the more correct one from a correctness standpoint — silent
fallback for a corrupt sidecar produces wrong gate mode with no operator signal. Recommend
updating ARCHITECTURE.md to match.

### R-01 (Critical) — Pre-split order must be the first implementation step

`render.rs` at 499 lines and `aggregate.rs` at 488 lines. nan-009 had gate-3b rejection for
this exact class of violation (entry #3580). The delivery agent must establish both boundary
modules before any other change. CI line-count enforcement is the backstop.

### R-11 (High) — Mandatory test file absence risk

nan-009 (entry #3579) had entire test modules absent at gate-3b. `eval/report/tests_distribution_gate.rs`
is a new file — not extended from an existing file. It will not exist unless explicitly created.
Non-negotiable test names listed in the Coverage Summary must be pre-declared in the test plan.

### R-05 (High) — Four-state gate semantics must be structurally encoded

`DistributionGateResult` must carry `diversity_passed` and `mrr_floor_passed` as separate booleans.
If the implementation uses only `overall_passed`, the two failure modes (AC-10) are unrenderable.
This is a structural type-level concern, not a pure rendering concern.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for risk patterns — found #3579, #3580 (nan-009 gate failures), #2758 (non-negotiable test names), #3583 (render split), #3585 (atomic sidecar), #3582 (sidecar pattern), #3574 (dual-type constraint)
- Stored: nothing novel to store — all relevant patterns already stored from this feature's architecture phase
