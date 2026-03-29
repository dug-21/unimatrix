# Agent Report: crt-030-agent-3-risk

**Role**: Architecture-Risk Strategist
**Feature**: crt-030 — Personalized PageRank for Multi-Hop Relevance Propagation
**Output**: `product/features/crt-030/RISK-TEST-STRATEGY.md`

## Summary

13 risks identified across R-01 through R-13. 2 Critical, 4 High, 7 Med. All 8 scope risks (SR-01 through SR-08) traced in the Scope Risk Traceability table.

## Risk Counts by Priority

| Priority | Count |
|----------|-------|
| Critical | 2 |
| High | 4 |
| Med | 7 |
| Low | 0 |

## Top Risks for Implementation Attention

**R-08 (Critical): Quarantine bypass for PPR-only entries.**
PPR-only entries are fetched in Step 6d and bypass the Step 6 quarantine filter that covers HNSW results. The spec (AC-13) states quarantined entries are skipped, but no named T-PPR-XX test in the spec covers this path. An explicit dedicated test is required — this is a safety/correctness gap that could surface withdrawn or poisoned entries in search results.

**R-01 (Critical): Rayon offload branch coverage.**
The `all_entries.len() >= PPR_RAYON_OFFLOAD_THRESHOLD` branch is scoped in crt-030 (ADR-008) but not required to be exercised by current tests. If written without test coverage, the branch silently rots. Both the offload and inline paths must be exercised. Offload timeout must return an empty map and proceed — this path needs a test.

**R-04 (High): Node-ID sort placement — O(I × N log N) regression.**
If the pre-sorted `Vec<u64>` is constructed inside the power iteration loop instead of outside it, latency at 10K nodes goes from < 1 ms to ~20 ms, violating NFR-01. This is a code-placement error that correctness tests will not catch. Requires either a timing test or a code review gate.

**R-08 / R-12 combined: Prerequisite direction test must exist pre-#412.**
Prerequisite edges use `Direction::Incoming`. No production Prerequisite edges exist yet (#412 not shipped). If the direction constant is accidentally set to `Outgoing`, the error is undetectable until #412 ships — at which point it causes a silent correctness regression. A synthetic unit test for Prerequisite direction must be written now.

## Artifacts

- `/workspaces/unimatrix/product/features/crt-030/RISK-TEST-STRATEGY.md`

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for risk patterns — found #2800 (cap logic gate failure), #1628 (per-query store reads instability), #2964 (signal fusion NLI override), #729 (cross-crate integration test pattern)
- Stored: nothing novel to store — crt-030 risks are feature-specific; quarantine-bypass-for-injected-entries may be stored as a pattern post-ship if the same pattern appears in future pool-expansion features
