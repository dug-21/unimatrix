# Agent Report: vnc-013-agent-0-scope-risk

## Output
- Produced: `product/features/vnc-013/SCOPE-RISK-ASSESSMENT.md` (39 lines)

## Risk Summary
- High severity: 3 (SR-01, SR-03, SR-07, SR-08 — SR-07 and SR-08 both High)
- Medium severity: 4 (SR-02, SR-04, SR-05, SR-06, SR-09)
- Low severity: 1 (SR-04 likelihood low; SR-09 likelihood low)

## Top 3 Risks for Architect/Spec Writer Attention
1. **SR-07 + SR-08** (High/High): 6-file blast radius across 3 crates with a critical silent-failure point — `mcp_context.tool_name` promotion before `build_cycle_event_or_fallthrough()`. This is the single most likely source of rework. Needs explicit per-site AC coverage in the architecture doc.
2. **SR-01** (High/High): Codex `--provider` flag is required for correct write-path attribution but the scope's backward-compatible default silently mislabels Codex events as Claude Code. The design decision (required vs. optional flag) has downstream implications for the Codex reference config.
3. **SR-03** (High/Med): DB-read-path source_domain derivation via `resolve_source_domain()` without an explicit fallback contract will silently change `source_domain` from `"claude-code"` to `"unknown"` for non-listed event types, potentially breaking existing consumers. Spec writer must decide and codify this contract before implementation.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned failures/gate-rejection — found 5 entries on gate validation process issues; none specific to hook normalization domain
- Queried: `/uni-knowledge-search` for outcome/rework in hook/ingest domain — found entry #4298 (existing vnc-013 pattern, directly applicable) and #3475 (ADR-003 col-027 PostToolUseFailure normalization precedent, directly applicable)
- Queried: `/uni-knowledge-search` for risk patterns — no directly applicable cross-feature risk patterns found
- Stored: nothing novel to store — entry #4298 already captures the ingest-boundary normalization pattern for vnc-013; the blast-radius + multi-site-coverage-ownership risk is an instance of the existing ADR-004 col-023 pattern already in Unimatrix
