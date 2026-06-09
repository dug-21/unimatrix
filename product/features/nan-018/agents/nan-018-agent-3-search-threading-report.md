# nan-018 Agent Report — Search Threading (Wave 4)

**Agent**: nan-018-agent-3-search-threading
**Component**: `SearchService.graph_penalty_params` + the two penalty-application sites
**Commit**: f9068862 `impl(search-threading): thread resolved GraphPenaltyParams through the two penalty sites (#716)`

## Files Modified
- `crates/unimatrix-server/src/services/search.rs` — import swap, `SearchService.graph_penalty_params` field, `new()` param, body assign, the two penalty sites swapped, 7 new tests.
- `crates/unimatrix-server/src/services/mod.rs` — one line: the sole `SearchService::new` caller (`with_rate_config`) passes `GraphPenaltyParams::default()`.

## What was implemented (WARN-1 two-site list)
- SITE 1 (fallback branch): `FALLBACK_PENALTY` const → `self.graph_penalty_params.fallback`.
- SITE 2 (normal branch): `graph_penalty(..)` → `graph_penalty_with(.., &self.graph_penalty_params)`.
- `background.rs:583` NOT touched (confirmed it is a `tracing` log string; no penalty applied, no `penalty_map`).
- At default config, resolved params == `GraphPenaltyParams::default()` == consts ⇒ byte-identical penalties (R-01/AC-01).

## Tests (all pass)
7 new in `services::search::tests`:
- `test_enumerated_penalty_sites_route_through_config` — R-01 grep guard (source scan of production half).
- `test_background_rs_not_a_penalty_site` — WARN-1 false-positive guard (background.rs log-string-only).
- `test_with_rate_config_default_resolves_to_const_params` — config-resolution bit-for-bit.
- `test_with_rate_config_multiplier_scales_severities_only` — R-13 shape-param exclusion.
- `test_with_rate_config_per_field_override_wins_over_multiplier` — R-13 precedence.
- `test_with_rate_config_multiplier_none_is_noop` — R-13 no-op.
- `test_fallback_branch_value_equals_resolved_field` — :727 reads resolved field.

Result: search module `test result: ok. 133 passed; 0 failed`. `cargo fmt` applied; `cargo clippy` clean on my files; lib builds with zero errors/new warnings attributable to my change.

## Scope deviation (IMPORTANT — needs Leader routing)
The brief said "modify search.rs ONLY", but the architecture/test-plan put resolution in `with_rate_config` (which lives in `services/mod.rs`, not search.rs). Adding the required `graph_penalty_params` parameter to `SearchService::new` forced a one-line change to its sole caller in `mod.rs`. That is the full extra footprint — no other files touched.

**Live-lever wiring is NOT complete (hand-off):** `with_rate_config` receives only `Arc<InferenceConfig>`, not the full `UnimatrixConfig`, so `config.graph_penalty.resolve_params()` cannot be threaded there without extending the `ServiceLayer::{new, with_rate_config}` constructor chain (cascades to `mod.rs`, `eval/profile/layer.rs`, `test_support.rs`, `server.rs`). I passed `GraphPenaltyParams::default()` at the call site, which is bit-for-bit correct for production (ADR-006 eval-only boundary) but means the **swept eval lever is not yet live** — AC-14 condition 3 (non-zero penalty delta from a swept profile) requires a follow-up that threads `profile.config_overrides.graph_penalty.resolve_params()` from `eval/profile/layer.rs` through `with_rate_config` into `SearchService::new`. This crosses files outside my single-file scope and should be a dedicated wiring task (the SR-08 checklist site (b)/(c) in ARCHITECTURE §88).

## Shared-checkout race observed
Sibling Wave-1 agents (cost-metric, trust-metric, report-extensions) had uncommitted in-flight edits in this checkout. The `unimatrix-server` test target was intermittently broken by their `ProfileResult`/`TrustOutcome`/`RegressionRecord` changes; it converged mid-session and my tests then ran green. One sibling test (`eval::report::tests::test_zero_regression_check_both_regression`, `eval/report/tests.rs:204`) was still failing at commit time — it is report-extensions work, unrelated to penalty threading (my change does not touch `eval/report/`). Flagging for the Leader's integration build.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern + decision) + read ADR-001 #4897, ADR-006 #4894 — confirmed clamp/multiplier coupling, eval-only boundary, two-site list, dual-default discipline (#4064). Briefing surfaced #1480 (parameter-passing over shared state when promoting engine consts) which matched the `GraphPenaltyParams` Copy-field-on-service approach.
- Stored: entry #4907 "Source-grep enumerated-site guard trips on comment/string mentions of the banned token" via context_store (pattern, topic unimatrix-server) — a genuine gotcha hit during implementation (the grep guard failed on my own `// (was FALLBACK_PENALTY const)` comment until reworded).
