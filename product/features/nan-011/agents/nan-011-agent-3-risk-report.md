# Agent Report: nan-011-agent-3-risk

## Deliverable

`product/features/nan-011/RISK-TEST-STRATEGY.md` — produced.

## Risk Summary

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 3 | R-01, R-02, R-04 |
| High | 4 | R-03, R-05, R-06, R-07 |
| Medium | 6 | R-08, R-09, R-10, R-11, R-12, R-15 |
| Low | 2 | R-13, R-14 |

**Total**: 15 risks, 38+ test scenarios.

## Top Risks for Human Attention

**R-01/R-02 (Critical)**: config.toml default accuracy is the single highest-stakes deliverable. Pattern #3817 (dual-site config defaults) confirms this is a recurring failure mode. The specific serde-vs-Default discrepancy for `boosted_categories` and `adaptive_categories` (both default to `["lesson-learned"]` via serde but `[]` via Rust Default) must be shown with the serde value — an implementer who reads the Default impl rather than the serde fn will write the wrong value.

**R-04 (Critical)**: Dual-copy protocol maintenance. Edits applied to `.claude/protocols/uni/` must be re-applied to `protocols/`. Copy-last ordering is mandatory; copy-first creates a guaranteed mismatch. The diff verification step in NFR-4 is the only control and must be enforced at gate.

**R-05 (High)**: The two-pass grep pattern for bare MCP invocations (ADR-004) must be used. A single-pass grep misses bare invocations inside spawn-prompt strings (uni-retro lines ~146, ~161). Both passes must return zero matches, and the npm-distributed copy of uni-retro must be checked independently.

## Coverage Gaps

- **rayon_pool_size formula** (R-03): No automated check can verify the formula is shown — it requires a human read of the field comment. The tester must read the comment and confirm the dynamic formula appears.
- **ConfidenceWeights sum validation** (R-06 edge case): If the config.toml shows a custom weights example, the six values must sum to exactly 0.92. A TOML parse check will not catch an invalid sum.
- **npm pack output recording** (R-08): The dry-run output must be recorded in the PR — not just run and discarded. This is a process requirement, not a code check.
- **SR-07 minimum version note**: The scope risk that distributing uni-retro creates a versioning contract is accepted but not mitigated by a minimum-version annotation in the skill. This is noted as a follow-on concern, not a gate blocker for nan-011.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection documentation drift" — #3611, #4169, #4198 returned; none directly pattern on documentation-only features
- Queried: /uni-knowledge-search for "risk pattern configuration default value mismatch" (category: pattern) — #3817 and #4044 directly inform R-01/R-02 severity elevation (High/High confirmed by historical data)
- Queried: /uni-knowledge-search for "npm package distribution packaging verification dry-run" — #1196, #1193, #4267 returned; #4267 is ADR-003 for this feature itself
- Stored: nothing novel to store — #3817 already captures the dual-site config default pattern; no new cross-feature pattern observable from nan-011's single-feature data
