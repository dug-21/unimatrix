# Agent Report: nan-009-agent-3-risk

## Output

RISK-TEST-STRATEGY.md written to:
`product/features/nan-009/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 3 (R-01, R-02, R-04) |
| High | 4 (R-03, R-05, R-06, R-07) |
| Med | 5 (R-08, R-09, R-12, EC-03, SEC-01) |
| Low | 2 (R-10, R-11) |

Total tracked risks: 12 (R-01 through R-12) plus 6 edge cases, 3 security risks, 5 failure modes.

## Blocker

**R-01 / SR-04: "(none)" vs "(unset)" null-phase label** — UNRESOLVED.

- ADR-003 (architect): canonical label is `"(unset)"`, citing SCOPE.md Constraint 5
- SPECIFICATION.md (spec writer): canonical label is `"(none)"`, citing SCOPE.md Goals §5 / AC-05

Both artifacts claim to resolve SR-04. They disagree. Delivery cannot start until a human designates one artifact as authoritative and the other is updated. The round-trip test (AC-11, ADR-002) asserts a specific string literal; an internally inconsistent test suite will produce false passes or false failures.

Recommendation: adopt `"(unset)"` (ADR-003 reasoning is stronger — `"(none)"` is ambiguous with a phase value of the string `"none"`). Update SPECIFICATION.md Constraint 5, FR-07, AC-05, AC-07, Domain Models, and Phase Vocabulary section accordingly.

## Other Risks Requiring Delivery Attention

- **R-04** (Critical): `insert_query_log_row` test helper must be updated to accept `phase: Option<&str>` — lesson #3543 documents the exact same failure from col-028 where the helper was not updated and AC-level tests could not exercise the non-null path.
- **R-02** (Critical): Section renumbering requires explicit `!content.contains("## 6. Distribution Analysis")` assertion — pattern #3426.
- **R-03** (High): Round-trip test must use a non-trivial phase value (`Some("delivery")`) — a `None` or default value allows a partial dual-type update to pass silently.
- **R-05** (High): Serde annotations differ by copy (runner: no `skip_serializing_if`; scenarios/types.rs: has it; report/mod.rs: `default` only). Two tests with opposite assertions required.
- **R-06** (High): Phase must NOT be forwarded to `ServiceSearchParams` — measurement purity. Requires explicit verification in `replay.rs`.

## Knowledge Stewardship

- Queried: /uni-knowledge-search "lesson-learned failures gate rejection" — found #3543, #885, #3548
- Queried: /uni-knowledge-search "risk pattern eval harness section order golden output" — found #3426, #3526, #3522
- Queried: /uni-knowledge-search "serde dual-type ScenarioResult round-trip" — found #885, #3526, #3522
- Queried: /uni-knowledge-search "eval harness SQLite query_log column extraction rework" — found #3543, #3555
- Stored: nothing novel to store — all patterns already captured in existing Unimatrix entries (#3426, #3526, #3543, #3550); the null-label conflict is feature-specific
