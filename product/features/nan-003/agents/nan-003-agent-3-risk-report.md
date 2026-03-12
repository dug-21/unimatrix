# Agent Report: nan-003-agent-3-risk (Architecture Risk)

## Output

- **Produced**: `product/features/nan-003/RISK-TEST-STRATEGY.md`
- **Mode**: architecture-risk

## Risk Summary

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 1 | R-01 (STOP gate failure) |
| High | 2 | R-02 (quality gate bypass), R-03 (wrong category) |
| Medium | 6 | R-04 (sentinel miss), R-05 (MCP mid-session), R-06 (dry-run violated), R-07 (depth bypass), R-08 (approval inversion), R-12 (CLAUDE.md corruption) |
| Low | 4 | R-09 (pre-flight false success), R-10 (near-dup re-run), R-11 (scan false neg), R-13 (prerequisites gap) |

**Total**: 13 risks, 13 risks mapped to at least one test scenario each.

## Key Risks for Human Attention

1. **R-01 (Critical)**: The STOP gate is the architectural heart of this feature. All quality control depends on the model respecting explicit STOP instructions. Since skills are markdown (not code), there is no enforcement mechanism beyond instruction phrasing. Every gate transition must be manually tested.

2. **R-02 + R-03 (High)**: The quality gate (What/Why/Scope) and category restriction are the primary defenses against repeating the uni-init prototype failure. Both depend entirely on model instruction-following — same platform constraint as R-01. These must be stress-tested with sparse/ambiguous repos.

3. **R-06 (Medium)**: Dry-run mode violation is high-impact in practice: a user reviewing changes before committing receives false safety assurance. The skill must clearly distinguish Write (overwrite) from Edit (append) semantics and must not write in dry-run mode.

## All SR-XX Scope Risks Traced

All 7 scope risks (SR-01 through SR-07) are resolved in the Scope Risk Traceability table. SR-05 (name collision) has no architecture-level R-XX — addressed at specification level via FR-07/AC-12.

## Knowledge Stewardship

- Queried: `/knowledge-search` for "lesson-learned failures gate rejection" — no directly applicable entries; gate result handling patterns found (#167, #1006) but are workflow-level, not skill quality level.
- Queried: `/knowledge-search` for "risk pattern skill markdown instruction following" — #550 (Markdown-Only Delivery Pattern) confirms the platform constraint is known.
- Queried: `/knowledge-search` for "idempotency sentinel duplicate MCP store" — found #1091 (ADR-002 itself) as the primary reference.
- Stored: nothing novel — instruction-fidelity risk is feature-specific to markdown-only delivery. Pattern #550 already captures the broader context. No 2+ feature evidence for a new risk pattern.
