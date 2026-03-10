# Agent Report: col-020-agent-1-architect

## Task
Design architecture for col-020 (Multi-Session Retrospective).

## Artifacts Produced

### Architecture Document
- `/workspaces/unimatrix/product/features/col-020/architecture/ARCHITECTURE.md`

### ADR Files
- `/workspaces/unimatrix/product/features/col-020/architecture/ADR-001-knowledge-reuse-server-side.md` (Unimatrix #864)
- `/workspaces/unimatrix/product/features/col-020/architecture/ADR-002-idempotent-counter-updates.md` (Unimatrix #865)
- `/workspaces/unimatrix/product/features/col-020/architecture/ADR-003-attribution-metadata.md` (Unimatrix #866)
- `/workspaces/unimatrix/product/features/col-020/architecture/ADR-004-file-path-extraction-mapping.md` (Unimatrix #867)

## Key Design Decisions

1. **Knowledge reuse computed server-side** (ADR-001, #864): Multi-table Store joins live in the context_retrospective handler, not in unimatrix-observe. ObservationSource trait stays focused. Scoped exception to the existing observe/server split.

2. **Idempotent counter updates via absolute-set** (ADR-002, #865): New `set_topic_delivery_counters()` method writes absolute values instead of additive deltas. Resolves SR-09 — repeated retrospective runs produce identical counter values.

3. **Attribution metadata on report** (ADR-003, #866): New `AttributionMetadata` struct surfaces attributed vs total session count. Resolves SR-07 — consumers can assess metric trustworthiness when attribution coverage is low.

4. **Explicit tool-to-field mapping for file paths** (ADR-004, #867): Deterministic match-based extraction from known tools. Resolves SR-04 — no heuristic false positives, unknown tools silently skipped.

## Scope Risk Mitigations

| Risk | Mitigation |
|------|-----------|
| SR-07 (attribution quality) | AttributionMetadata on report (ADR-003) |
| SR-08 (observe/server boundary) | Server-side computation, documented exception (ADR-001) |
| SR-09 (idempotent counters) | Absolute-set, not additive (ADR-002) |
| SR-01 (JSON parsing) | serde_json parse with fallback to empty vec, no panic |
| SR-04 (file path extraction) | Explicit mapping (ADR-004) |
| SR-06 (overlapping sessions) | Strict ordering by started_at, concurrent sessions independent |

## Architecture Summary

- **6 components**: session_metrics module (C1), new types (C2), knowledge reuse inline computation (C3), Store API extensions (C4), report builder extension pattern (C5), handler integration (C6)
- **Crate split**: session summaries + reload rate in unimatrix-observe; knowledge reuse in unimatrix-server; batch queries in unimatrix-store
- **Pattern**: post-build mutation (same as narratives/recommendations) — no build_report() signature change
- **Error handling**: best-effort — new step failures log warning and leave field as None, not abort

## Open Questions

1. query_log batch scan with >100 sessions may need chunking (current scale is safe)
2. Helpful-signal-based reuse deferred — no per-session attribution of who marked helpful
