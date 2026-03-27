# Agent Report: col-031-agent-3-risk

**Mode**: architecture-risk
**Feature**: col-031 — Phase-Conditioned Frequency Table
**Output**: `/workspaces/unimatrix/product/features/col-031/RISK-TEST-STRATEGY.md`

## Summary

14 risks identified, 4 Critical, 4 High, 4 Medium, 2 Low.

## Risk Count by Priority

| Priority | Count |
|----------|-------|
| Critical | 4 |
| High | 4 |
| Medium | 4 |
| Low | 2 |

## Top Risks for Human Attention

**R-01 (Critical)** — `json_each` integer cast produces no rows or wrong rows. If the SQL form is wrong, the frequency table is silently empty on every rebuild and the feature is inert without any error. Must be validated by AC-08 integration test against a real SQLite TestDb. Historical pattern #3678 confirms this is a recurring trap.

**R-04 / R-05 (Critical)** — Eval gate vacuous PASS and non-separable deliverable violation. If AC-16 (`extract.rs` phase fix) is not complete when AC-12 is gated, the gate measures pre-col-031 behavior and passes trivially. This is the highest process risk. Historical gate failures in nan-009 (#3579, #3580) show this pattern recurs.

**R-02 (Critical)** — Cold-start semantic drift: `use_fallback=true` returns `1.0` (neutral), not `0.0`. When `current_phase` is set and the table is cold-start, all candidates receive a uniform `+0.05` absolute score shift. Ranking is preserved but scores are not bit-for-bit identical to pre-col-031. AC-11 must test these two paths separately (`None` path: score identity; `Some` + cold-start: ranking preservation only).

**R-03 (High)** — Lock ordering violation risk. If the `PhaseFreqTableHandle` read guard is held into the scoring loop, the background tick write is blocked for the full scoring pass. Code review must confirm the guard is dropped before the scoring loop begins.

## Open Questions for Human Review

- OQ-03 (NFR-08): Should a `tracing::debug!` line be emitted when `current_phase` has no match in the frequency table? The spec requires at minimum a code comment. A log line aids operator diagnosis.
- OQ-04 (NFR-03 cold-start): The spec documents the cold-start `1.0` behavior but notes the architect must confirm whether this is intended or whether `0.0` is preferable. ADR-005 implicitly accepts `1.0` as correct (flat additive constant). Confirm before implementation.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #3579, #3580, #2758. Gate failure patterns from nan-009 elevated R-04/R-05 to Critical.
- Queried: `/uni-knowledge-search` for "risk pattern scoring weight calibration regression" — found #3207, #3206, #2985. Informed R-02, R-08.
- Queried: `/uni-knowledge-search` for "SQLite json_each query_log background tick" — found #3678, #3681. Confirmed R-01 as Critical.
- Queried: `/uni-knowledge-search` for "RwLock Arc hot path lock ordering" — found #3682. Confirmed R-03 is architecturally addressed.
- Queried: `/uni-knowledge-search` for "eval harness extract scenario phase regression gate vacuous" — found #3555, #3683. Confirmed R-04/R-05 grounded in documented gap.
- Stored: nothing novel to store — all recurring patterns identified already exist in Unimatrix (#3678, #3682, #3579, #3580).
