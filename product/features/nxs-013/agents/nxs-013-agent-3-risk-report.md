# Agent Report: nxs-013-agent-3-risk

## Task
Architecture-risk analysis for nxs-013 (co-locate per-project config.toml with project data directory).

## Artifacts Produced
- `/workspaces/unimatrix/product/features/nxs-013/RISK-TEST-STRATEGY.md`

## Risk Summary
- **High priority**: 3 risks (R-01 container cold start, R-06 explicit UNIMATRIX_CONFIG override, R-07 config template corruption)
- **Medium priority**: 4 risks (R-02 control flow alteration, R-03 log label untestability, R-04 doc edit scope creep, R-08 YAML syntax)
- **Low priority**: 1 risk (R-05 README merge conflict)
- **Total**: 8 risks, 18 test scenarios

## Top Risks for Attention
1. **R-01**: Docker build + run is mandatory verification — static review insufficient per lesson #4582
2. **R-07**: DEFAULT_CONFIG_TOML header edit must not corrupt TOML template body — caught by existing parsing tests
3. **R-03**: AC-03/AC-10 log label verification is manual-only — accepted as code review + inspection per lesson #4147

## Scope Risk Traceability
All 7 SR-XX risks traced. SR-05 fully resolved (all OQs answered in architecture). Remaining risks mapped to architecture-level R-XX risks.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" -- found #4473 (warn+continue masks missing tests), #4147 (log-level AC testability), #4582 (Dockerfile requires build verification). #4582 and #4147 directly informed R-01 and R-03.
- Queried: /uni-knowledge-search for "risk pattern" -- found #3426 (formatter regression), #4041 (write_graph_edge), #1616 (dedup flags). No direct applicability to nxs-013.
- Queried: /uni-knowledge-search for "Dockerfile container ENV config" -- found #4573 (ADR-005), #4635 (ADR-003 nxs-013), #4582 (Docker build lesson). Confirmed ADR-005 HOME=/data dependency.
- Stored: nothing novel to store -- nxs-013 is a documentation/config/labeling feature with no new risk patterns. Existing lessons #4582 and #4147 already capture the relevant patterns.
