# Agent Report: crt-010-agent-3-risk

## Task
Architecture-risk analysis for crt-010 (Status-Aware Retrieval)

## Output
- `product/features/crt-010/RISK-TEST-STRATEGY.md`

## Risk Summary
- **12 risks** identified (2 Critical, 4 High, 6 Medium)
- **41 test scenarios** mapped across all risks
- **9 scope risks** (SR-01 through SR-09) traced to architecture risks

## Top 3 Risks

1. **R-08 (Critical)**: Post-compaction supersession injection unreachable — C6 prunes deprecated entries from HNSW, but C2 supersession injection only triggers when deprecated entries appear in HNSW results. After compaction, the injection trigger is disabled. Design tension between pruning and injection.

2. **R-01 (Critical)**: hnsw_rs `get_point_data` API availability is unverified — the entire successor similarity computation (ADR-002) depends on an undocumented internal API. If unavailable, fallback to re-embedding adds ONNX inference to the hot search path.

3. **R-02 (High)**: Penalty multiplier correctness — 0.7x/0.5x values are arbitrary. At extreme similarity deltas (>30%), penalties may be insufficient to enforce the ranking invariant (Active > Deprecated at comparable similarity).

## Key Findings

- **R-08 is a design gap**: Compaction and supersession injection have contradictory goals. The architect should clarify expected behavior post-compaction — either (a) accept that injection becomes inactive after compaction (successors must be findable via own embeddings), or (b) keep deprecated entries with `superseded_by` in HNSW during compaction.
- All 9 scope risks traced. SR-03/SR-06 confirmed as architecture-resolved. SR-01/SR-04 mapped to concrete architecture risks with test scenarios.
- Security surface is unchanged — no new external inputs.

## Status
Complete
