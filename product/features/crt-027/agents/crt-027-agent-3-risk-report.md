# Agent Report: crt-027-agent-3-risk

## Deliverable

`/workspaces/unimatrix/product/features/crt-027/RISK-TEST-STRATEGY.md` — written.

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 2 |
| High     | 5 |
| Medium   | 5 |
| Low      | 2 |
| **Total** | **14** |

## Top Risks for Tester Attention

**R-01 (Critical)** — `source` field addition to `HookRequest::ContextSearch` wire protocol. All existing struct-literal test constructors must add `source: None` or use `..` spread, or they fail to compile. More critically, the `dispatch_request` observation tagging path must be tested for all three cases: explicit `SubagentStart`, explicit `None`, and deserialized-absent (serde default). Lesson #885 and #699 directly predict this is where silent data corruption lives.

**R-03 (Critical)** — `format_compaction_payload` 11-test rewrite. Every replaced test name must exist and pass. Gate reviewer must grep by name (lesson #2758). Any omitted invariant (budget enforcement, UTF-8 boundary, histogram block, confidence sort) becomes an invisible regression.

**R-07 (High)** — SubagentStart stdout injection is unconfirmed (SR-01). The feature degrades gracefully if Claude Code ignores hook stdout, but the primary value path (knowledge injected into subagent before first token) is unverified. AC-SR01 must be explicitly marked OPEN or CONFIRMED before Gate 3c — this cannot be inferred from automated test results alone.

**R-04 (High)** — `MIN_QUERY_WORDS = 5` off-by-one boundary. Tests for exactly 4 words (→ RecordEvent) and exactly 5 words (→ ContextSearch) are non-negotiable. SubagentStart must be confirmed exempt from this guard.

**R-06 (High)** — Query derivation three-step fallback must use a single shared `derive_briefing_query` helper for both MCP and UDS paths. Code review gate: confirm one function, two call sites. All four derivation paths (task present, synthesized, empty-signals, no-session) must be unit tested against the helper directly.

## Manual Gate Items

- **AC-SR01**: SubagentStart stdout injection — OPEN (unverified). Must be confirmed via manual smoke test or documentation reference before Gate 3c.
- **11 non-negotiable test names**: Must be verified by `grep` at gate review (R-03 / lesson #2758).
- **Static grep gates** for R-09: `parse_semantic_k` and `UNIMATRIX_BRIEFING_K` must not appear in production code paths post-delivery.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned gate failures — found #2758, #699, #885 — all directly elevate R-01, R-03, R-12 severity.
- Queried: `/uni-knowledge-search` for risk patterns — found #2938, #3180, #646 — calibrated R-01 and R-03 likelihoods.
- Queried: `/uni-knowledge-search` for EffectivenessStateHandle BriefingService — found #1546 — confirmed R-02 scenario design.
- Stored: nothing novel to store — relevant patterns already captured in existing Unimatrix entries.
