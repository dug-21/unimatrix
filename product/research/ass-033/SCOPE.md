# ASS-033: Unimatrix Cycle Review — Enhancement Research

**Type**: Research spike
**Date**: 2026-03-24
**Status**: Complete

## Question

What improvements can be made to `context_cycle_review` — rebranded **Unimatrix Cycle Review** — given the new attribution engine (col-024), feature goal signal (col-025), and documented format gaps in GH issues #203 and #320? Additionally, what can the per-phase `cycle_events` timeline unlock?

## Sources Examined

| Source | Type | Key Insight |
|---|---|---|
| `product/features/col-024/` | Feature docs | Cycle-events-first attribution engine; three-path fallback; topic_signal enrichment |
| `product/features/col-025/` | Feature docs | Goal field on cycle_events (v16); SessionState.current_goal; goal-priority briefing |
| GH #203 | Issue + 2 comments | Markdown/JSON comparison across base-004, crt-018b; temporal narrative format proposal |
| GH #320 | Issue | Knowledge reuse counts only intra-cycle entries; cross-feature reuse invisible |
| GH #362 | Issue | Root cause for col-024: bugfix attribution failure patterns A and B |
| `crates/unimatrix-store/src/migration.rs` | Source | cycle_events DDL: id, cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal |
| `crates/unimatrix-server/src/infra/validation.rs` | Source | CYCLE_START_EVENT, CYCLE_PHASE_END_EVENT, CYCLE_STOP_EVENT constants |
| `crates/unimatrix-server/src/uds/listener.rs` | Source | handle_cycle_event; phase/outcome/goal extraction |
| `crates/unimatrix-observe/src/types.rs` | Source | PhaseNarrative, RetrospectiveReport structs |
| `crates/unimatrix-server/src/mcp/tools.rs` | Source | CycleParams, context_cycle_review handler |

## Findings Summary

See `FINDINGS.md` for full analysis.

**Four categories of opportunity:**

1. **New data available (col-024/025)**: Goal in header, attribution provenance, in-progress indicator, goal-contextualized hotspot severity, CycleType classification
2. **Existing data not rendered (markdown gaps from #203)**: Session profile, agents spawned, file zones, positive signals, knowledge category breakdown, temporal burst sketches
3. **Metric correctness fix (#320)**: Knowledge reuse must include all served entries, not just same-cycle entries
4. **Phase timeline breakdown (new)**: `cycle_events` timestamps create per-phase windows; every observation can be placed into a phase — enabling per-phase duration, agents, tool distribution, knowledge served/stored, hotspot scoping, and rework evidence

**The phase timeline is the highest-value new capability** — it makes the retro scannable at a glance and connects duration outliers directly to the hotspots that caused them.

## Branding

Report header changes from `## Retrospective: {id}` to `# Unimatrix Cycle Review — {id}`. MCP tool name `context_cycle_review` unchanged (backward compat).

## Recommended Feature Scope

A single delivery feature (col-026 candidate) covering high/medium items:

**Formatter-only changes (no schema/data changes):**
- Goal + attribution provenance in header
- Session profile section (agents, tools, file zones)
- What Went Well section
- Relative timestamps + timeline burst sketch in findings
- entries_analysis as knowledge health section
- In-progress indicator
- Branding: "Unimatrix Cycle Review"

**Service-layer additions (new computation, no schema changes):**
- Fix knowledge reuse metric (#320) — cross-feature split
- Phase timeline table (new PhaseStats type, slice existing observations by cycle_events windows)
- Per-phase hotspot scoping (annotate each finding with which phase it fired in)
- Rework phase evidence (per-pass diff for repeated phases)

**Deferred:**
- Per-CycleType baseline comparison (requires accumulation of phase-stats data over time)
- Phase velocity trend (same)
- Phase knowledge profile anomaly detection
