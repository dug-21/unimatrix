# Agent Report: crt-018-agent-1-architect

## Task
Design architecture for crt-018 (Knowledge Effectiveness Analysis).

## Artifacts Produced

### Architecture Document
- `/workspaces/unimatrix/product/features/crt-018/architecture/ARCHITECTURE.md`

### ADR Files
- `/workspaces/unimatrix/product/features/crt-018/architecture/ADR-001-consolidated-effectiveness-query.md`
- `/workspaces/unimatrix/product/features/crt-018/architecture/ADR-002-null-topic-handling.md`
- `/workspaces/unimatrix/product/features/crt-018/architecture/ADR-003-data-window-indicator.md`
- `/workspaces/unimatrix/product/features/crt-018/architecture/ADR-004-configurable-noisy-trust-sources.md`

## Key Design Decisions

1. **ADR-001: Single consolidated Store method** — One `compute_effectiveness_aggregates()` method with 4 SQL queries inside, following the `StatusAggregates` pattern from crt-013 (Unimatrix #704). Addresses SR-01 (performance) and SR-07 (pattern consistency).

2. **ADR-002: Explicit NULL handling** — Empty/NULL topics mapped to "(unattributed)" sentinel. Sessions with NULL feature_cycle excluded from active_topics but included in injection outcome counts. Addresses SR-06.

3. **ADR-003: Data window indicator** — `DataWindow` struct in every effectiveness report showing session_count, earliest/latest timestamps. Addresses SR-02 (GC sliding window non-determinism).

4. **ADR-004: Configurable noisy trust sources** — `NOISY_TRUST_SOURCES` array constant instead of hardcoded "auto". Addresses SR-05.

## Three-Crate Architecture

- **unimatrix-engine** — New `effectiveness.rs` module with pure computation functions (classify, calibrate, aggregate). Follows `confidence.rs` pattern.
- **unimatrix-store** — Two new Store methods: `compute_effectiveness_aggregates()` and `load_entry_classification_meta()`. Added to `read.rs`.
- **unimatrix-server** — StatusReport extended with `Option<EffectivenessReport>`, Phase 8 in compute_report, all three format outputs.

## Unimatrix Storage Status

ADR storage in Unimatrix failed — agent `crt-018-agent-1-architect` lacks Write capability. All four ADRs need to be stored by a privileged agent using `/store-adr` after this task completes.

## Open Questions

None. All scope risks (SR-01 through SR-08) are addressed in the architecture or ADRs.
