# Agent Report: nxs-012-agent-2-spec

## Task
Write SPECIFICATION.md for nxs-012: Export/Import Complete Persistent State Coverage.

## Artifacts Produced
- `/workspaces/unimatrix/product/features/nxs-012/specification/SPECIFICATION.md`

## Key Decisions

1. **FR-12 drop_all_data cascade ordering**: Specified that `observation_phase_metrics` and `observation_metrics` (derived tables not part of the export contract) must be deleted during `--force` import to prevent stale aggregates. Order: `observation_phase_metrics` -> `observation_metrics` -> `graph_edges` -> `observations` -> `cycle_events`. This addresses SR-06 from the risk assessment.

2. **format_version validation range**: Specified that both `format_version: 0` and `format_version >= 3` are rejected (FR-07, FR-08). SCOPE.md mentioned rejecting "version 0, 3+" — split into separate testable requirements.

3. **CycleEventRow goal_embedding handling**: Specified that the struct omits `goal_embedding` entirely (rather than modeling it as `Option<()>`). The export SELECT excludes the column, so the deserialization struct need not reference it. The architect may choose either approach; the specification requires only that `goal_embedding` is absent or null in the JSONL output.

4. **Provenance detail string expansion**: Added FR-17 requiring `record_provenance` to include counts for the 3 new tables in the audit log detail string. The current implementation only mentions entries, tags, co_access, and counters.

## Open Questions

1. **observation_metrics clearance on force import**: The specification requires clearing `observation_metrics` and `observation_phase_metrics` during `--force` import (SR-06). These tables have no FK relationship to `observations` (the FK is `observation_phase_metrics` -> `observation_metrics` on `feature_cycle`). The architect should confirm whether clearing them is necessary for correctness or merely for cleanliness. If observation metrics are not cleared, a post-import metrics pipeline run could see stale aggregates that don't match the imported observations.

2. **CycleEventRow.goal_embedding in format.rs**: SCOPE.md suggests `Option<()>` (always null, ignored on deser). The architect should decide whether to use `Option<()>` for forward-compatible deserialization or simply omit the field with `#[serde(default)]` on the struct. Both satisfy the AC.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned 19 entries. Key relevant entries: #4607 (nxs-012 feature context), #1143 (nan-002 format.rs ADR), #343 (JSONL intermediate file pattern), #1166 (nan-002 lessons learned). Pattern #1103 (NaN fallback convention) referenced in SR-01 was not in the top results but is documented in the risk assessment.
