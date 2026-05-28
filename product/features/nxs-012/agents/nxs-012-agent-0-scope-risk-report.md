# Agent Report: nxs-012-agent-0-scope-risk

## Task
Scope-level risk assessment for nxs-012 (Export/Import: Complete Persistent State Coverage).

## Output
- `/workspaces/unimatrix/product/features/nxs-012/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- **High severity**: 1 (SR-06: FK cascade ordering in drop_all_data)
- **Medium severity**: 5 (SR-01, SR-03, SR-04, SR-05, SR-07)
- **Low severity**: 3 (SR-02, SR-08, SR-09)
- **Total**: 9 risks identified

## Top 3 Risks for Architect/Spec Writer Attention
1. **SR-06** (High/Med): `observation_metrics` and `observation_phase_metrics` have FOREIGN KEY cascades on `observations`. `drop_all_data` must DELETE derived tables before observations, even though they are not exported. Missing this causes FK constraint violations on --force import.
2. **SR-03** (Med/High): Excluding `goal_embedding` BLOB from export means goal-cluster affinity scoring is unavailable post-import until first cycle completion. Architect must confirm `context_briefing` handles NULL gracefully.
3. **SR-01** (Med/Med): `graph_edges.weight` is REAL (f64). NaN/Infinity values require the same `Number::from_f64` fallback pattern used for `entries.confidence`. Existing pattern #1103 covers this.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection export import" -- found nan-002 retrospective (#1166) confirming established export/import patterns are stable; audit_log counter race lesson (#4396) relevant to import transaction safety
- Queried: /uni-knowledge-search for "risk pattern export import serialization" -- found SQL-to-JSONL pattern (#1103), JSONL intermediate format (#343), shared deserialization structs (#1161) all validated by nan-002
- Stored: nothing novel to store -- risks identified are feature-specific, not cross-feature patterns; established export/import patterns already well-documented in Unimatrix
