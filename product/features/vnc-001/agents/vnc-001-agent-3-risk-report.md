# Agent Report: vnc-001-agent-3-risk

## Artifacts Produced

- `/workspaces/unimatrix/product/features/vnc-001/RISK-TEST-STRATEGY.md`

## Risk Summary

- 16 risks identified (6 Critical, 7 High, 2 Medium, 0 Low -- note: some bridge priority boundaries)
- 60 test scenarios defined
- 5 integration risks (IR-01 through IR-05)
- 7 edge cases (EC-01 through EC-07)
- 4 security risks (SR-01 through SR-04)
- 5 failure modes (FM-01 through FM-05)

## Key Risks for Human Attention

1. **R-04 (Critical)**: AGENT_REGISTRY table creation breaking existing Store::open(). This is the highest-risk change because it touches the foundation crate.
2. **R-08 (High)**: Shutdown compact() failure due to leaked Arcs. The likelihood is high because Arc lifecycle across many subsystems is complex.
3. **R-06 (Critical)**: Auto-enrollment capability assignment. Incorrect defaults could undermine the entire security model.
4. **R-14 (Critical)**: Server panics on malformed input. MCP receives arbitrary JSON from agents.

## Open Questions

None.
