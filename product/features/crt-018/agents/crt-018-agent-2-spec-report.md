# Agent Report: crt-018-agent-2-spec

## Agent Role
Specification Writer

## Status
COMPLETE

## Artifacts Produced
- `/workspaces/unimatrix/product/features/crt-018/specification/SPECIFICATION.md`

## Key Decisions

1. **Classification priority order**: Defined Noisy > Ineffective > Unmatched > Settled > Effective as the tiebreaker when an entry could match multiple categories. Noisy takes priority because it is the most actionable signal (auto-extracted, zero quality signal).

2. **NULL topic handling (SR-06)**: Entries with NULL/empty topic are assigned to an "(unattributed)" bucket rather than silently dropped. Sessions with NULL feature_cycle are excluded from topic activity but still contribute to entry-level effectiveness via injection_log JOIN. This addresses the known failure mode from Unimatrix #981.

3. **NULL session outcomes**: Sessions with NULL outcome (e.g., still active, timed out) are excluded from weighted success rate computation entirely (neither numerator nor denominator). This prevents in-progress sessions from diluting effectiveness scores.

4. **Calibration bucket boundaries**: Lower-inclusive, upper-exclusive for buckets 0.0-0.9. The final bucket (0.9-1.0) is inclusive on both ends to capture confidence=1.0.

5. **Data window indicator**: Added a "data window" indicator (session count + span in days) to all output formats per SR-02 recommendation, so consumers understand the coverage of the analysis.

6. **Noisy filter extensibility (SR-05)**: Specified that the architect should consider making the trust_source filter for Noisy configurable, but the default for this feature is "auto" only.

7. **StatusAggregates consolidation (SR-07)**: Deferred to architect whether to fold into existing StatusAggregates pattern or add independent methods. Spec requires only that performance budget is met.

## Open Questions

1. **Entries with injections but only NULL-outcome sessions**: An entry injected into sessions that all have NULL outcome (active/timed-out) has no outcome data. Current spec would leave these as Effective (default). The architect should confirm whether a sixth state like "Pending" is warranted or if defaulting to Effective is acceptable for this edge case.

2. **Settled entries with mixed topic spelling**: entries.topic is free-text. If the same logical topic is spelled differently (e.g., "crt-018" vs "crt-018-effectiveness"), the topic activity check may produce false "Settled" classifications. This is a pre-existing data quality issue not unique to effectiveness analysis, but worth noting for the architect.

## Self-Check
- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through AC-15, plus AC-16 for NULL handling and AC-17 for constant naming)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (500ms, 500 entries, 10k injection rows)
- [x] Domain Models section defines key terms
- [x] NOT in scope section is explicit (10 exclusions)
- [x] Output file is in `product/features/crt-018/specification/` only
- [x] No placeholder or TBD sections
