# Agent Report: nxs-013-agent-2-spec

## Task
Write SPECIFICATION.md for nxs-013 (co-locate per-project config.toml with project data directory).

## Output
- `/workspaces/unimatrix/product/features/nxs-013/specification/SPECIFICATION.md`

## Key Decisions

1. **SR-06 mitigated**: Verified that the 7 provenance tests in config.rs (line 9219+) assert on `SourceStatus` enum variants (`Loaded`, `NotFound`, `NotApplicable`), not on log message strings. FR-03 log label changes will not break any tests. AC-09 ("existing tests pass unmodified") is consistent with AC-03 (label changes).

2. **OQ-01 resolved**: Include commented `UNIMATRIX_CONFIG` example in docker-compose.yml for advanced operators. Serves both migration path (SR-01) and Kubernetes documentation needs.

3. **OQ-02 resolved**: WAVE2-ROADMAP.md is a living planning document (updated regularly). Correct the volume description with a brief annotation noting the correction reflects the shipped design.

4. **OQ-03 resolved**: Out of scope. Individual source lines are sufficient. Summary line would be a new feature.

5. **Edit boundaries defined** (per SR-03/SR-04): Exact file sections and line ranges specified for each documentation edit to prevent scope creep.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned 16 entries including #2395 (Two-Level TOML Config Merge pattern), #4573 (ADR-005 Container Data Path Resolution), #2286 (dsn-001 config decision). All confirmed no conflict with nxs-013 scope.
