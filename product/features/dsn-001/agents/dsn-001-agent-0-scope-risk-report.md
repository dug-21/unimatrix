# Agent Report: dsn-001-agent-0-scope-risk (Revised — Preset System Expansion)

**Role**: Scope-Risk Strategist
**Feature**: dsn-001 — Config Externalization (W0-3)
**Mode**: scope-risk
**Revision**: Re-assessment after `[profile]` preset system added to scope

## Output

- Produced: `product/features/dsn-001/SCOPE-RISK-ASSESSMENT.md` (overwritten, 37 lines, risks SR-01–SR-13)

## Risk Summary

| Severity | Count | Risk IDs |
|----------|-------|----------|
| High | 4 | SR-02, SR-05, SR-09, SR-10 |
| Med | 4 | SR-01, SR-11, SR-12, SR-13 |
| Low | 0 | — |

Prior risks resolved by architecture ADRs (retained in table as closed): SR-03, SR-04, SR-06, SR-07, SR-08.

## Top 3 Risks for Architect/Spec Writer

1. **SR-02 (High/High) — ConfidenceParams struct gap**: The architecture-designed `ConfidenceParams` carries only `freshness_half_life_hours`, `alpha0`, `beta0`. The six weight constants (W_BASE, W_USAGE, W_FRESH, W_HELP, W_CORR, W_TRUST) are absent. Every preset sets all six weights — if those values never enter `ConfidenceParams`, preset selection is silently inert. The architect must extend the struct with the six weight fields before delivery begins.

2. **SR-09 (High/Med) — Preset weight values undefined**: Exact numeric values for all four named presets remain "architect deliverable requiring domain science validation" — illustrative, not committed. The delivery team cannot write correctness tests against undefined values. Spec must commit exact values with a rationale, and all six must sum to ≤ 0.92 (the stored-factor invariant from confidence.rs).

3. **SR-10 (High/Med) — collaborative regression with no mechanical guard**: `collaborative` must reproduce current compiled constants exactly (W_BASE=0.16, W_USAGE=0.16, W_FRESH=0.18, W_HELP=0.12, W_CORR=0.14, W_TRUST=0.16, half_life=168.0h). Existing tests do not assert weight values — AC-01 passes even if collaborative ships with wrong numbers. Spec must require a unit test: `ConfidenceParams::from_preset(Preset::Collaborative) == ConfidenceParams::default()`.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for risk patterns -- MCP tool not accessible in this agent context; analysis performed directly from SCOPE.md, ARCHITECTURE.md, and confidence.rs constants.
- Stored: nothing novel to store -- SR-02 (config loads preset weights but struct doesn't carry them to engine) is feature-specific to dsn-001's current design state; not yet a cross-feature pattern.
