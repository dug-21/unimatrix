# Agent Report: col-023-agent-1-architect

## Status: COMPLETE

## Deliverables

### Files Produced
- `/workspaces/unimatrix/product/features/col-023/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/col-023/architecture/ADR-001-observation-event-type-generalization.md`
- `/workspaces/unimatrix/product/features/col-023/architecture/ADR-002-domain-pack-registry.md`
- `/workspaces/unimatrix/product/features/col-023/architecture/ADR-003-rule-evaluator-dsl.md`
- `/workspaces/unimatrix/product/features/col-023/architecture/ADR-004-hooktype-refactor-wave-plan.md`
- `/workspaces/unimatrix/product/features/col-023/architecture/ADR-005-source-domain-guard-contract.md`
- `/workspaces/unimatrix/product/features/col-023/architecture/ADR-006-universal-metrics-canonical-representation.md`
- `/workspaces/unimatrix/product/features/col-023/architecture/ADR-007-ingest-security-bounds.md`

### Unimatrix Entries Stored
- #2903: ADR-001 — HookType → string fields
- #2904: ADR-002 — Domain pack TOML registry
- #2905: ADR-003 — Rule evaluator DSL
- #2906: ADR-004 — Wave-based refactor plan
- #2907: ADR-005 — source_domain guard contract
- #2908: ADR-006 — UniversalMetrics canonical representation
- #2909: ADR-007 — Ingest security bounds

## Key Decisions Made

1. **HookType replaced with two string fields** (`event_type`, `source_domain`) on `ObservationRecord`. String values are identical to HookType variant names — no behavioral regression.
2. **source_domain is always server-side inferred** — hook ingress always sets "claude-code", never client-declared.
3. **TOML config-only domain registration** — no new MCP tool; Admin runtime re-registration removed from W1-5 scope.
4. **Bounded two-kind rule DSL** (threshold + temporal_window) — no new crate dependencies; built-in 21 rules remain Rust implementations.
5. **Four-wave atomically-merged refactor** — wave-gated compilation checkpoints for the 25-file blast radius.
6. **Mandatory source_domain guard** in every DetectionRule::detect() — spec-level contract, not suggestion.
7. **UniversalMetrics typed struct canonical (Option A)** — no HashMap migration; schema v14 adds only `domain_metrics_json TEXT NULL` column; MetricVector gains `domain_metrics: HashMap<String, f64>`.
8. **Four ingest security bounds** at parse_observation_rows() — 64KB, depth-10, domain regex, rule sandboxing.

## Open Questions for Spec Writer

- **OQ-1**: AC-08 (Admin runtime re-registration) must be explicitly removed from acceptance criteria or a target tool and schema delta must be defined before implementation.
- **OQ-2**: W3-1 "fully functional" gate — does it require multi-domain detection rules or just pipeline acceptance of multi-domain events?
- **OQ-3**: source_domain is not stored in the `observations` table (only event_type is). Acceptable for W3-1 training signal queries?
- **OQ-4**: startup failure vs. warning for invalid `rule_file` in domain pack config — current ADR-007 decision is startup failure; may be too strict for optional packs.
- **OQ-5**: `HookType` constants module — public re-export from `unimatrix-core` or internal to claude-code pack only?
