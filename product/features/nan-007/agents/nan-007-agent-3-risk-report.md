# Agent Report: nan-007-agent-3-risk

## Deliverable

`/workspaces/unimatrix/product/features/nan-007/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 1 |
| High     | 7 |
| Med      | 8 |
| Low      | 2 |
| **Total** | **18** |

## Top Testing Priorities

1. **R-01 (Critical)** — `AnalyticsMode::Suppressed` must be verified via SHA-256 snapshot integrity test (AC-05/NFR-04). This is the single highest-risk item: a missing suppression causes silent snapshot corruption and confidence score pollution. Four test scenarios required.

2. **R-04 / R-05 (High)** — Both Python client framing contracts are fragile at the byte level. `UnimatrixUdsClient` must use newline-delimited JSON (not length-prefixed); `UnimatrixHookClient` must use 4-byte big-endian (not little-endian). Raw byte-capture tests are required for both, not just end-to-end AC verification.

3. **R-08 (High)** — P@K dual-mode semantics: the `expected` vs. `baseline.entry_ids` dispatch must be explicitly tested for both branches. An inversion here produces reports that are numerically plausible but semantically wrong — no runtime error fires.

## Key Findings

- SR-07 risk (analytics suppression) has been correctly addressed by ADR-002 but requires a SHA-256 snapshot integrity test as its verification artifact — not just a unit test of `AnalyticsMode`.
- SR-01 (rmcp exact pin) is already resolved by the existing `=0.16.0` pin; the test obligation is a smoke integration test that exercises the UDS `serve()` path.
- The offline/live acceptance split (SR-04) must be enforced at test-file granularity: Group 1 (D1–D4) tests must run without a daemon; Group 2 (D5–D6) must be isolated so fixture failure does not block Group 1.
- R-06 (symlink bypass on snapshot path) is a security-class risk that must not be skipped despite its Low likelihood — the blast radius is full database loss.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — entries #1203, #1204, #2577 (missing boundary tests are a recurring gate-failure pattern, directly informing R-12 and R-16)
- Queried: `/uni-knowledge-search` for "risk pattern analytics suppression SQLite read-only" — entries #2125, #2130, #2582 (analytics drain fire-and-forget failure modes; WAL SQLITE_BUSY_SNAPSHOT; MCP UDS framing confirmed newline-delimited)
- Queried: `/uni-knowledge-search` for "migration read-only integration test" — entry #2060 (migration connection sequencing, informs R-02)
- Queried: `/uni-knowledge-search` for "VACUUM INTO WAL snapshot" — entries #1097, #329, #2270 (snapshot isolation ADR, WAL auto-checkpoint, dual-pool architecture)
- Stored: nothing novel to store — the two-socket/two-framing distinction is already captured in entry #2582; analytics suppression is feature-specific. Cross-feature pattern promotion deferred to retro.
