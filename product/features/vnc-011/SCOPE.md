# vnc-011: Retrospective ReportFormatter

## Problem Statement

`context_retrospective` returns a JSON blob that costs ~3,500–27,700 tokens depending on feature complexity. LLM consumers (retro agents, human-in-the-loop) waste most of their context budget parsing noise:

- 34/44 baseline comparisons are "Normal" — no signal
- Evidence arrays repeat what the claim string already says (74 clusters for a single permission narrative in nxs-010)
- Multiple `HotspotFinding`s from the same detection rule (e.g., 4 `output_parsing_struggle` findings for cargo test/build/clippy/test -p) are one logical finding
- Zero-activity phases clutter the output
- Recommendations duplicate across related hotspots
- Raw JSON structure forces the LLM to parse rather than reason

Real-world measurement: nxs-010 retro was 76KB JSON (~27,700 tokens). 5 actionable findings extracted from ~2,000 lines. Signal-to-noise ratio: ~3%.

## Root Cause

The retrospective pipeline was built for completeness and structured machine consumption. No formatter exists between `RetrospectiveReport` and the MCP tool response. The full report struct is serialized directly to JSON.

## Scope

### In Scope

**1. Markdown formatter** (new function in response formatting layer)
- Default output format for `context_retrospective`
- Compact, scannable markdown optimized for LLM consumption
- Session table from `session_summaries` (col-020 data)
- Feature-level knowledge reuse summary (col-020b data)
- Baseline comparisons filtered to Outlier + NewSignal only (skip Normal, NoVariance)
- Zero-activity phase suppression (phases where `tool_call_count <= 1 AND duration_secs == 0`)
- Attribution quality note when `attribution.attributed_session_count < attribution.total_session_count`

**2. Finding collapse** (formatter-side grouping)
- Group `HotspotFinding`s by `rule_name` into logical findings
- Render per-tool/per-entity breakdown within each collapsed finding (e.g., "Bash(12), Read(6), context_store(5)")
- k=3 random examples per collapsed finding, drawn from the combined evidence pools of all findings in the group
- Severity: highest severity in the group

**3. Narrative collapse** (formatter-side)
- Summary line + cluster count per finding, not full cluster listing
- Sequence patterns (e.g., sleep escalation) preserved inline

**4. Recommendation deduplication**
- Deduplicate recommendations by `hotspot_type`
- Render as actionable list at the bottom of the report

**5. Format parameter** (`RetrospectiveParams`)
- Add `format` field: `"markdown"` (default), `"json"` (current behavior, unchanged)
- `evidence_limit` default changed from 3 to 0

**6. Target output structure**
```markdown
# Retrospective: {feature_cycle}
{session_count} sessions | {total_records} tool calls | {total_duration}

## Sessions
| # | Window | Duration | Calls | Knowledge | Outcome |
|---|--------|----------|-------|-----------|---------|
| 1 | 09:52–10:49 | 57m | 312 | 5 served, 2 stored | success |
| 2 | 18:23–19:20 | 57m | 489 | 3 served, 1 stored | success |

## Outliers (vs {baseline_sample_count}-feature baseline)
| Metric | Value | Mean | σ |
|--------|-------|------|---|
| permission_friction | 26 | 8.8 | 7.0 |

## Findings ({count})
### F-01 [warning] Permission friction — 26 events across 2 sessions
Bash(12), Read(6), context_store(5), Edit(3).
Examples:
- PreToolUse for Bash at ts=1710000012000
- PreToolUse for context_store at ts=1710000034000
- PreToolUse for Read at ts=1710000045000

### F-02 [info] Sleep workarounds — 19 events
Escalation pattern: 30s→60s→90s→120s
Examples:
- Sleep command in Bash input at ts=1710000050000
- Sleep command in Bash input at ts=1710000060000
- Sleep command in Bash input at ts=1710000070000

## Phase Outliers
| Phase | Metric | Value | Mean | σ |
|-------|--------|-------|------|---|
| 3b | duration_secs | 6,761 | 2,052 | 2,800 |

## Knowledge Reuse
5 entries delivered | 2 cross-session | Gaps: procedure

## Recommendations
- Add build/test commands to settings.json allowlist
- Use run_in_background instead of sleep polling
```

### Out of Scope

- Changes to `RetrospectiveReport` struct
- Changes to `build_report()` or detection rules
- Actionability tagging (`[actionable]`/`[expected]`/`[informational]`)
- Drill-down tool for per-finding detail
- Changes to narrative synthesis (`synthesis.rs`)
- Changes to baseline computation

### Key Constraint

All collapse, selection, filtering, and formatting logic lives in the formatter. The report generation pipeline is untouched. `format: "json"` returns the exact same output as today.

## Key Stakeholders

- **unimatrix-server**: New formatter function, `RetrospectiveParams` format field, evidence_limit default
- **unimatrix-observe**: No changes (types only consumed by formatter)
- **Retro agents**: Primary consumer — receives markdown instead of JSON by default
- **Human users**: Secondary consumer via `/retro` skill

## Success Criteria

1. `context_retrospective` returns markdown by default, scannable and complete
2. `format: "json"` returns unchanged JSON output
3. Baseline comparisons filtered to Outlier + NewSignal in markdown
4. Hotspot findings collapsed by `rule_name` with per-entity breakdown
5. k=3 random examples per collapsed finding
6. Zero-activity phases suppressed in markdown
7. Recommendations deduplicated by `hotspot_type`
8. `evidence_limit` defaults to 0
9. Session table rendered from col-020 `session_summaries`
10. Token reduction >= 80% vs JSON for typical reports
11. All existing tests pass, new tests cover markdown formatter

## Risks

- SR-01: Random example selection may occasionally pick low-value evidence. Mitigation: acceptable for MVP; can add "pick heaviest cluster" heuristic later.
- SR-02: Finding collapse by `rule_name` may group findings that a human would separate. Mitigation: only 3 rules produce multiple findings (`permission_retries`, `output_parsing_struggle`, `phase_duration_outlier`); grouping is natural for all three.
- SR-03: Markdown table formatting edge cases (long session IDs, missing data). Mitigation: truncate/default in formatter.
- SR-04: `session_summaries` may be None for pre-col-020 retrospectives. Mitigation: omit Sessions section when absent.

## Dependencies

- col-020 (multi-session retrospective) — COMPLETE, in codebase
- col-020b (knowledge reuse refinement) — in progress, but formatter handles None gracefully

## Effort Estimate

Medium. ~300-400 lines of formatter code + ~100 lines of tests + ~20 lines of parameter changes. No schema changes, no report pipeline changes.

## References

- GitHub Issue: #91
- Product Vision: vnc-011 in Activity Intelligence Wave 3
- nxs-010 retro experience: #91 comment (76KB JSON, 3% signal-to-noise)

## Tracking

https://github.com/dug-21/unimatrix/issues/196
