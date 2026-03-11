# Agent Report: vnc-011-agent-2-spec

## Task
Write SPECIFICATION.md for vnc-011 Retrospective ReportFormatter.

## Status
COMPLETE

## Artifacts Produced
- `/workspaces/unimatrix/product/features/vnc-011/specification/SPECIFICATION.md`

## Key Decisions

1. **SR-03 resolved as global default change**: SCOPE explicitly states `evidence_limit` defaults to 0. Specification follows SCOPE. Documented as C-03 constraint with rationale that callers wanting truncation can pass `evidence_limit: 3` explicitly.

2. **SR-01 addressed via AC-16 + C-04**: Mandated exhaustive None-handling tested against a minimal report (all Optional fields None). Formatter omits sections for None fields rather than rendering empty sections.

3. **entries_analysis excluded from markdown**: The SCOPE target structure does not include entries_analysis. It remains accessible via JSON format. Listed in NOT in scope.

4. **Collapsed Finding as formatter-internal struct**: Defined an intermediate grouping structure in Domain Models. This is not a public type -- it exists only within the formatter function to organize the grouping logic.

5. **Actionability tagging deferral acknowledged**: SR-04 from risk assessment noted the SCOPE/VISION mismatch. Specification explicitly defers this in C-05 and NOT in scope.

6. **Duration formatting from metrics**: Total duration in the header comes from `metrics.universal.total_duration_secs`, not from summing session durations, since MetricVector is always present.

## Open Questions

1. **Baseline sample count display**: FR-06 references showing the baseline sample count in the section heading. The `BaselineComparison` struct does not directly carry `sample_count` -- it has `mean` and `stddev` but not the count from the underlying `BaselineEntry`. The architect should determine whether to derive this from the data (e.g., from the `BaselineSet` if accessible) or omit the count from the heading.

2. **Random selection mechanism**: NFR-02 notes that `rand` should only be used if already in the dependency tree. The architect should verify whether `rand` is available or choose the modular arithmetic fallback approach for k=3 selection.

3. **Session window formatting**: FR-04 formats session start times as HH:MM. The `started_at` field is epoch millis. The formatter needs a timestamp-to-time conversion. The architect should decide whether to use `chrono` (if available) or manual epoch arithmetic for UTC time display.
