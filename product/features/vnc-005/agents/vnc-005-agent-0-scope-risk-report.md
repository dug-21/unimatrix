# Agent Report: vnc-005-agent-0-scope-risk

## Output

- Produced: `product/features/vnc-005/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary

| Severity | Count |
|----------|-------|
| High     | 4 (SR-01, SR-02, SR-04, SR-07) |
| Medium   | 5 (SR-03, SR-05, SR-06, SR-08, SR-09) |
| Low      | 0 |

Total: 9 risks across 3 categories.

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-02 (High/High) — Tokio+fork ordering**: `nix` fork called in a tokio binary is undefined behavior if the runtime is already initialized. This must be resolved as the first architecture question — it constrains the entire daemon startup sequence. Pattern #1910 stored.

2. **SR-07 (High/High) — graceful_shutdown decoupling**: The current shutdown is triggered by transport close; daemon mode requires session close to be independent of daemon lifetime. This touches the same write-drain paths implicated in lesson #735 (spawn_blocking saturation). Highest implementation risk in the feature.

3. **SR-04 (High/Med) — Default invocation behavior change**: Changing no-subcommand from "start server" to "bridge mode" is a user-visible breaking change. The spec must define the full failure matrix (timeout, unhealthy daemon, auto-start failure) before implementation to avoid field breakage.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures, outcome rework, risk patterns, UDS transport, rmcp shutdown, background tick patterns — found entries #735, #1688, #300, #1897, #1898, #1560, #1542 as relevant historical context
- Stored: entry #1910 "Tokio+fork ordering is a recurring pre-architecture risk in daemon features" via context_store (pattern)
