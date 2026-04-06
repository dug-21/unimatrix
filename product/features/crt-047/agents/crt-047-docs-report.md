# Agent Report: crt-047-docs

## Agent ID
crt-047-docs

## Task
Update README.md for feature crt-047 (Curation Health Metrics, GH #529).

## Sections Modified

### MCP Tool Reference

Two rows updated in the MCP Tool Reference table:

**`context_status`** — Added description of the new `curation_health` aggregate block to both the description column and the "When to Use" column. The block surfaces per-cycle correction rate mean/stddev, source breakdown (agent%, human%), orphan deprecation ratio mean/stddev, and trend direction (available when at least 6 cycles of snapshot data exist). The "When to Use" guidance was extended to include reviewing curation behavior trends.

**`context_cycle_review`** — Added the `curation_health` block to the report contents list in the "When to Use" column. Documented: raw correction counts (total, agent-attributed, human-attributed), orphan deprecation count, σ deviation from the rolling 10-cycle baseline when at least 3 prior cycles have snapshot data (annotated with history length), and raw-counts-only cold-start behavior.

## Sections Not Modified

- **Architecture Overview / Storage** — Schema version and table count fields in this section were not updated. These figures predate crt-047 in the existing README and were not the subject of prior documentation-agent updates. Updating them is out of scope for a targeted crt-047 edit; they would require a dedicated pass to reconcile all intervening schema bumps.
- All other README sections were unaffected by crt-047.

## Sources

All claims trace to SPECIFICATION.md:
- FR-11 (curation_health on context_cycle_review, raw + σ, cold start)
- FR-13, FR-14 (curation_health on context_status, rate/breakdown/orphan/trend)
- OQ-04 resolved (trend requires 6 cycles)
- OQ-05 resolved (history annotation format: "2.1σ (4 cycles of history)")
- SCOPE.md Goals 4 and 5 (parallel confirmation)

## Commit

`1e148f16` — `docs: update README for crt-047 curation health metrics (#529)`
