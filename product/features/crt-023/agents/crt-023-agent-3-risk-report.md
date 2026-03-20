# Agent Report: crt-023-agent-3-risk

**Role**: Architecture-Risk Strategist
**Feature**: crt-023 — NLI + Cross-Encoder Re-ranking (W1-4)
**Output**: `/workspaces/unimatrix/product/features/crt-023/RISK-TEST-STRATEGY.md`

## Summary

Produced RISK-TEST-STRATEGY.md with 22 risks, 6 classified Critical, 12 High, 4 Med.
All 9 SR-XX scope risks traced to architecture-level R-XX risks in the traceability table.
All 6 architecture stress-tests from the spawn prompt are covered.

## Risk Counts by Priority

| Priority | Count | Risks |
|----------|-------|-------|
| Critical | 5 | R-01, R-03, R-05, R-09, R-10 |
| High | 12 | R-02, R-04, R-06, R-07, R-11, R-13, R-14, R-15, R-16, R-17, R-18, R-19 |
| Med | 4 | R-08, R-12, R-20, R-21 |
| Low | 1 | R-22 |

## Spawn Prompt Stress-Tests → Coverage

| Stress-Test | Risk(s) | Coverage |
|-------------|---------|----------|
| Pool floor raise + startup race condition | R-01, R-02 | Load test (3 concurrent NLI searches) + startup config unit test |
| 60s timeout fires during 20-pair NLI batch | R-04 | Slow-mock `CrossEncoderProvider` + timeout-then-fallback test |
| Hash absent in config / partial file download | R-05, R-06 | Hash mismatch log assertion + truncated file `Failed` transition test |
| Embedding Vec consumed before hand-off | R-07 | Integration test: mock provider records call count + non-empty embedding assertion |
| Circuit breaker `max_contradicts_per_tick` | R-09, R-10 | Cap enforcement across both edge types + cascade-to-auto-quarantine end-to-end test |
| SR-02 rayon pool + embedding degradation | R-01, R-16 | Concurrent store + search load test; fire-and-forget write contention test |

## Top 3 Risks for Human Attention

1. **R-10 (Critical/Med)** — NLI miscalibration cascade: a single noisy store call can write `max_contradicts_per_tick` Contradicts edges; on the next background tick these edges apply graph penalties; entries near the auto-quarantine threshold could be quarantined. The cap is the only upstream gate. Requires an end-to-end cascade test before ship.

2. **R-05 (Critical/Med)** — Hash verification absent by default: `nli_model_sha256` is `Option<String>` with no default; production deployments that omit the field run with no model integrity check. The spec requires a `warn!` in this case but does not require the field. A documentation or operator-guidance gap here is a ship-blocking security issue.

3. **R-03 (Critical/Med)** — Stable sort under identical NLI scores: short/terse ADR entries in the Unimatrix knowledge base are the primary use case; NLI scores on these may cluster near 0.33 (uniform distribution); if the sort is unstable, repeated `context_search` calls return different orderings — direct trust regression.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for risk patterns — key findings: entry #735 (pool saturation from fire-and-forget writes, elevated R-01 and R-16), entry #770 (mutex deadlock from re-acquisition, elevated R-13 to High), entry #2130 (SQLite write_pool max_connections=1 constraint, created R-16), entry #724 (behavior-based ranking test pattern, applied to R-03/R-17 test design).
- Stored: nothing novel to store — crt-023-specific risks identified. Pattern for "shared inference pool + fire-and-forget write tasks under single SQLite write pool" will be stored after implementation confirms whether R-16 materializes.
