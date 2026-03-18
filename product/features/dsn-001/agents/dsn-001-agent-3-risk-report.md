# Agent Report: dsn-001-agent-3-risk

## Task
Architecture-risk mode. Produced `RISK-TEST-STRATEGY.md` for dsn-001 (Config Externalization / W0-3).

## Artifacts Read
- `product/features/dsn-001/SCOPE.md`
- `product/features/dsn-001/SCOPE-RISK-ASSESSMENT.md`
- `product/features/dsn-001/architecture/ARCHITECTURE.md`
- `product/features/dsn-001/architecture/ADR-001-confidence-params-struct.md`
- `product/features/dsn-001/architecture/ADR-002-config-type-placement.md`
- `product/features/dsn-001/architecture/ADR-003-two-level-config-merge.md`
- `product/features/dsn-001/architecture/ADR-004-forward-compat-stubs.md`
- `product/features/dsn-001/specification/SPECIFICATION.md`

## Codebase Evidence Gathered
- Confirmed `freshness_score()` in `unimatrix-engine/src/confidence.rs` still uses `FRESHNESS_HALF_LIFE_HOURS` directly (line 148). Not yet parameterized.
- Confirmed 13 files across engine + server crates call `compute_confidence` or `freshness_score` — all require migration to `ConfidenceParams`.
- Confirmed 4 hardcoded `entry.category == "lesson-learned"` comparisons in `search.rs` (lines 413, 418, 484, 489).
- Confirmed `ContentScanner::global()` uses `OnceLock::get_or_init` — safe on first call but warm-up path needs testing.
- Confirmed `PERMISSIVE_AUTO_ENROLL: bool = true` hardcoded in `infra/registry.rs` line 25.
- Confirmed `agent_resolve_or_enroll(agent_id, permissive: bool)` current two-arg signature in `unimatrix-store/src/registry.rs`.
- Confirmed `context_retrospective` matches 231 files total; live callers identified in specification checklist.

## Risks Identified
13 risks: 2 Critical, 4 High, 5 Med, 2 Low.

**Top risks by severity:**
- **R-01 (Critical)**: Rename checklist miss — Python tests, protocol files, skill files not caught by compiler.
- **R-02 (Critical)**: ConfidenceParams migration — 13 call-site files, cross-crate; any miss silently uses stale half-life.
- **R-03 (High)**: Merge false-negative — per-project field explicitly set to default value is not detected as "set" by PartialEq-with-Default approach.
- **R-08 (High)**: AgentRegistry session_caps propagation — `resolve_or_enroll` wrapper may pass None for session_caps even after config wiring.

## Output
`product/features/dsn-001/RISK-TEST-STRATEGY.md`

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for risk patterns — entries #1910, #364, #747 relevant; no directly applicable patterns for config-merge false-negative or startup-validation ordering.
- Stored: nothing novel to store — rename blast-radius (#364) and cross-crate migration (#747) patterns already exist. R-03 merge false-negative is feature-specific until a second config-merging feature appears.
