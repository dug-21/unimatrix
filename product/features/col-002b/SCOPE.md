# col-002b: Detection Library + Baseline Comparison

## Problem Statement

col-002 ships the observation pipeline — hooks, JSONL parsing, attribution, hotspot framework, metric storage, and the `context_retrospective` MCP tool — with 3 detection rules to prove the pipeline works end-to-end. But 3 rules cover only a fraction of the anomalous behavior that ASS-013 identified as detectable. The retrospective report is sparse: it catches permission retries, session timeouts, and sleep workarounds, but misses monolithic agents, context overloading, edit bloat, cold restarts, scope creep, and rework patterns.

col-002b fills in the detection library: 18 additional rules across all 4 hotspot categories, plus historical baseline comparison so the retrospective report can show how the current feature compares to project norms.

## Prerequisites

- **col-002 complete**: Pipeline infrastructure, hotspot framework, OBSERVATION_METRICS table, `context_retrospective` tool, and the 3 proof-of-concept rules must be shipped and working.

## Goals

### 1. Complete Detection Library

Implement the remaining 18 hotspot detection rules, registering each into the existing framework established by col-002.

**Agent Hotspots (7 rules)**

| Signal | Metric | Starting Threshold | Detection |
|--------|--------|--------------------|-----------|
| Context load | KB read before first Write/Edit | >100 KB | Sum Read response_size until first Write/Edit |
| Lifespan | SubagentStart → SubagentStop duration | >45 min | Timestamp diff |
| File breadth | Distinct files touched (read + write) | >20 files | Unique file paths in tool inputs |
| Re-read rate | Files read 2+ times within agent window | >3 re-reads | File path frequency count |
| Mutation spread | Distinct files written/edited | >10 files | Unique Write/Edit target paths |
| Compile cycles | cargo check/test invocations per phase | >6 per phase | Regex match Bash commands |
| Edit bloat | Average edit response size | >50 KB avg | PostToolUse response_size for Edit tool |

**Friction Hotspots (2 rules)**

| Signal | Metric | Starting Threshold | Detection |
|--------|--------|--------------------|-----------|
| Search-via-Bash | Bash commands matching find/grep/rg patterns | >5% of Bash calls | Regex on Bash command input |
| Output parsing struggle | Same cargo command with different pipe filters within 3 min | >2 filter variations | Command similarity + timestamp proximity |

**Session Hotspots (4 rules)**

| Signal | Metric | Starting Threshold | Detection |
|--------|--------|--------------------|-----------|
| Cold restart | Gap >30 min + burst of reads to already-read files | Any occurrence | Timestamp gap + file path intersection |
| Coordinator respawns | SubagentStart count for coordinator types | >3 per feature | Count by agent_type |
| Post-completion work | Tool calls after final task completion / total | >8% | TaskUpdate completion timestamp as boundary |
| Rework events | Task status completed → in_progress | Any occurrence | TaskUpdate state transition |

**Scope Hotspots (5 rules)**

| Signal | Metric | Starting Threshold | Detection |
|--------|--------|--------------------|-----------|
| Source file count | New *.rs files created via Write | >6 files | Write tool path filter |
| Design artifact count | Files in feature directory | >25 files | Write/Edit paths under product/features/ |
| ADR count | ADR-* files created | >3 ADRs | Write path pattern match |
| Post-delivery issues | GH issues created after final task completion | >0 | Bash commands matching `gh issue create` |
| Phase duration outlier | Any phase >2x its evolving baseline duration | 2x baseline | Compare against stored MetricVector history |

### 2. Historical Baseline Comparison

Add baseline computation to the retrospective report. When `context_retrospective` runs:

1. Load all stored MetricVectors from OBSERVATION_METRICS via `list_all_metrics`
2. Compute per-metric mean and standard deviation across all previous features
3. Include a comparison table in the report showing the current feature's metrics alongside historical baselines
4. Flag metrics that exceed mean + 1.5σ as statistical outliers (separate from hotspot threshold flags)

**Comparison table format** (presented to the LLM in the retrospective report):

```
Metric               | crt-007 | Mean  | Stddev | Status
---------------------|---------|-------|--------|--------
Total tool calls     |     890 |  1050 |    180 | normal
Search miss rate     |     41% |   28% |     8% | ▲ outlier
Edit bloat KB        |    1200 |   900 |    350 | normal
Stage 3b duration    |   55min |  25min |   10min | ▲ outlier
Cold restarts        |       2 |   0.5 |    0.7 | ▲ outlier
```

**Phase-specific baselines**: Computed per phase name. "stage-3b" baseline is derived only from previous "stage-3b" measurements, not from all phases mixed together.

**Minimum data requirement**: Baseline comparison requires at least 3 stored MetricVectors. With fewer, the report notes "insufficient history for baseline comparison" and omits the table.

### 3. Phase Duration Outlier Detection

This rule specifically depends on baseline data and cannot function without historical MetricVectors:

- Compare each phase's duration against the historical mean for that phase name
- Flag as outlier if duration exceeds 2x the evolving baseline mean
- With <3 data points for a phase name, use the bootstrapped absolute threshold instead (defined in the rule)
- This is the only detection rule that requires col-002b's baseline infrastructure

## Non-Goals

- **No threshold convergence.** Baselines are computed and displayed, but thresholds remain bootstrapped. Adapting thresholds based on dismissed hotspot feedback or empirical mean+1.5σ is a future follow-on.
- **No compound signal detection.** The baseline comparison table enables the LLM to spot correlated outliers visually, but automated compound signal detection and promotion remains future work.
- **No new MCP tools or parameters.** col-002b enhances the existing `context_retrospective` tool response — more hotspots in the report, baseline comparison section added. No API changes.
- **No changes to MetricVector structure.** col-002 defines the full MetricVector with all universal and phase metric fields. col-002b populates additional fields that were already computed but had no matching hotspot rules, and adds the baseline comparison to the report.
- **No changes to hooks or collection.** The observation pipeline is unchanged. Same JSONL files, same attribution logic, same file lifecycle.

## Proposed Approach

### 1. Rule Implementation

Each rule implements the hotspot detection trait/interface established by col-002. For each rule:
- Implement the scan logic (iterate records, accumulate state, detect threshold breach)
- Define the bootstrapped threshold as a constant
- Collect evidence records (the specific tool calls that triggered detection)
- Register in the rule engine

Rules can be implemented and tested independently. Each rule is a self-contained module with its own unit tests.

### 2. Baseline Computation Module

Add a `baseline` module to `unimatrix-observe`:
- `compute_baselines(history: &[MetricVector]) -> BaselineSet` — computes mean and stddev per metric
- `compare_to_baseline(current: &MetricVector, baselines: &BaselineSet) -> Vec<BaselineComparison>` — flags outliers
- Phase-specific baseline computation (group by phase name, compute per-group)
- Minimum history check (require 3+ MetricVectors)

### 3. Report Enhancement

Extend `RetrospectiveReport` to include:
- `baseline_comparison: Option<Vec<BaselineComparison>>` — None if insufficient history
- Each `BaselineComparison` includes: metric name, current value, historical mean, stddev, outlier flag

### 4. Server Integration

Update the `context_retrospective` handler in the server crate:
- After analysis, load all MetricVectors via `list_all_metrics`
- Pass to baseline computation
- Include baseline comparison in the report response

## Acceptance Criteria

### Detection Rules
- AC-01: All 7 agent hotspot rules implemented with bootstrapped thresholds
- AC-02: Both remaining friction hotspot rules implemented with bootstrapped thresholds
- AC-03: All 4 remaining session hotspot rules implemented with bootstrapped thresholds
- AC-04: All 5 scope hotspot rules implemented with bootstrapped thresholds
- AC-05: Each rule includes evidence records (concrete tool call data that triggered detection)
- AC-06: Each rule is independently testable with unit tests using synthetic record data
- AC-07: All 18 new rules register into the existing hotspot framework without modifying engine core

### Baseline Comparison
- AC-08: Baseline computation produces per-metric mean and standard deviation from stored MetricVectors
- AC-09: Phase-specific baselines computed per phase name (not aggregated across phases)
- AC-10: Metrics exceeding mean + 1.5σ flagged as statistical outliers in the comparison table
- AC-11: Baseline comparison requires minimum 3 stored MetricVectors; report notes "insufficient history" with fewer
- AC-12: Comparison table included in `context_retrospective` report when baseline data is available
- AC-13: Phase duration outlier rule uses baseline data when available (≥3 data points), falls back to absolute threshold otherwise

### General
- AC-14: No changes to MetricVector structure, OBSERVATION_METRICS table schema, or hook infrastructure
- AC-15: No new MCP tools or tool parameters — enhances existing `context_retrospective` response
- AC-16: All existing tests pass with no regressions (col-002 pipeline tests, store tests, server tests)
- AC-17: Unit tests cover: each of the 18 detection rules, baseline mean/stddev computation, phase-specific baseline grouping, minimum history check, outlier flagging
- AC-18: Integration tests cover: full retrospective with all rules active (verify richer hotspot output), retrospective with baseline comparison (write 3+ MetricVectors, verify comparison table in report)
- AC-19: `#![forbid(unsafe_code)]` maintained on `unimatrix-observe`
- AC-20: No new crate dependencies

## Constraints

- **Additive only.** col-002b adds rules to the existing framework and adds baseline comparison to the existing report. No structural changes to the pipeline, storage, or tool interface.
- **Rules are independent.** Each rule can be implemented, tested, and merged individually. No ordering dependencies between rules (except phase duration outlier, which depends on baseline computation).
- **`unimatrix-observe` remains a pure computation library.** Baseline computation takes `&[MetricVector]` as input — the server loads the data from the store and passes it in.
- **Test infrastructure is cumulative.** Build on col-002's test fixtures (synthetic JSONL generators, test MetricVectors).

## Resolved Decisions

1. **All detection rules and thresholds are defined in col-002's SCOPE.md.** col-002b implements the rules exactly as specified there. No re-scoping of individual rule thresholds or detection methods.

2. **Phase duration outlier is the only rule that depends on baseline data.** All other rules use static bootstrapped thresholds. This rule falls back to an absolute threshold when insufficient baseline data exists.

3. **Baseline comparison uses 1.5σ for outlier flagging.** This is a display threshold (shown in the report), not a hotspot threshold. It tells the LLM "this is statistically unusual for your project" as additional context alongside the bootstrapped hotspot flags.

## Open Questions

_None. All design decisions inherited from col-002 SCOPE.md and ASS-013 research._

## Tracking

https://github.com/dug-21/unimatrix/issues/57
