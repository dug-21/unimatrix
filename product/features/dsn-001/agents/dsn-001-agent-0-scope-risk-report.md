# Agent Report: dsn-001-agent-0-scope-risk

**Role**: Scope-Risk Strategist
**Feature**: dsn-001 — Config Externalization (W0-3)
**Mode**: scope-risk

## Output

- Produced: `product/features/dsn-001/SCOPE-RISK-ASSESSMENT.md` (36 lines, 8 risks SR-01–SR-08)

## Risk Summary

| Severity | Count |
|----------|-------|
| High     | 3 (SR-02, SR-04, SR-05) |
| Med      | 5 (SR-01, SR-03, SR-06, SR-07, SR-08) |
| Low      | 0 |

## Top 3 Risks for Architect/Spec Writer

1. **SR-02 (High/High)** — `freshness_score()` / `compute_confidence()` API break in `unimatrix-engine`. Decide on `ConfidenceParams` struct vs. bare parameter before implementation — a struct is a 10-line hedge against W3-1 churn.

2. **SR-04 (High/Med)** — Scope excludes `[confidence]` and `[cycle]` TOML sections that PRODUCT-VISION W0-3 includes, and that W3-1 depends on for GNN cold-start. If `UnimatrixConfig` is designed without reserved stubs for these sections, W3-1 forces a config format break. Reserve them as empty structs now.

3. **SR-05 (High/High)** — `context_retrospective` rename has blast radius across Rust crates, `unimatrix-observe` types, protocol files, skill files, and CLAUDE.md. Build success is not sufficient to confirm completeness. Spec writer must produce an exhaustive rename checklist before the PR opens.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for risk patterns -- MCP tool not available in this agent thread; proceeded from project memory and direct codebase analysis.
- Stored: nothing novel to store -- risks are feature-specific; cross-feature pattern evidence insufficient at this stage (only one prior config feature observed).
