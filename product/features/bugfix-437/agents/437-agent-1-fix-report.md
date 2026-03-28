# Agent Report: 437-agent-1-fix

**Feature**: bugfix-437
**Task**: Implement fix for recurring_friction wrong write path (Steps 1–6, GH #437)

## Files Modified

1. `/workspaces/unimatrix/crates/unimatrix-observe/src/extraction/recurring_friction.rs`
2. `/workspaces/unimatrix/crates/unimatrix-observe/src/extraction/dead_knowledge.rs`
3. `/workspaces/unimatrix/crates/unimatrix-observe/src/extraction/mod.rs`
4. `/workspaces/unimatrix/crates/unimatrix-server/src/background.rs`
5. `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/tools.rs`

## Changes by Step

**Step 1 — recurring_friction.rs:**
- Removed `ExtractionRule` impl (`RecurringFrictionRule` struct, `evaluate()`, `existing_entry_with_title()`).
- Added `compute_friction_recommendations(observations: &[ObservationRecord]) -> Vec<String>`.
- Rewrote all tests to use `compute_friction_recommendations()` instead of `RecurringFrictionRule::evaluate()`. Tests verify: empty on <3 sessions, recommendations contain "fired in" and "sessions" text, empty input returns empty.
- `remediation_for_rule()` preserved as `pub(crate)` — same content, still used.

**Step 2 — dead_knowledge.rs:**
- Added `compute_dead_knowledge_recommendations(observations, store) -> Vec<String>`.
- Uses `detect_dead_knowledge_candidates()` with window=5. Returns empty on insufficient data, one recommendation string when stale entries are detected.
- Added 3 tests: empty on insufficient sessions, empty with no accessed entries, surfaces stale entries.

**Step 3 — extraction/mod.rs:**
- Removed `RecurringFrictionRule` from `default_extraction_rules()` (4→3 rules). Updated doc comment.
- Removed `"recurring-friction"` from `min_features_for_rule()` match arm (falls through to default=3 — unchanged behavior).
- Updated tests: `default_extraction_rules_returns_four` → `default_extraction_rules_returns_three`, added `test_recurring_friction_rule_removed_from_defaults`, updated `default_extraction_rules_names` and `min_features_for_each_rule` comments.

**Step 4 — background.rs:**
- Added `friction_signals: Vec<String>` and `dead_knowledge_signals: Vec<String>` to `TickMetadata` with doc comments.
- Added imports: `compute_friction_recommendations`, `compute_dead_knowledge_recommendations`.
- Changed `extraction_tick()` return type from `Result<ExtractionStats, ServiceError>` to `Result<(ExtractionStats, Vec<String>, Vec<String>), ServiceError>`.
- Inside `spawn_blocking` closure: added calls to both compute functions after `run_extraction_rules`. Signals computed from `obs_for_rules` already in scope (architect F-1 fix — no second DB fetch).
- Updated early return in gate error path to `Ok((ctx.stats.clone(), friction_recs, dead_knowledge_recs))`.
- Updated match arm in `run_single_tick` to destructure tuple and assign `meta.friction_signals` and `meta.dead_knowledge_signals`.

**Step 5 — mcp/tools.rs:**
- Appended both signal vecs to `report.maintenance_recommendations` inside the tick_meta lock block.
- Added comment: "friction_signals are unconditional — they report agent workflow patterns, not KM graph health".

**Step 6 — Regression test in background.rs:**
- Added `test_extraction_tick_does_not_write_recurring_friction_to_entries`.
- Creates 3-session orphaned-call observations, asserts RecurringFrictionRule is absent from `default_extraction_rules()`, asserts `compute_friction_recommendations()` returns non-empty, asserts `run_extraction_rules()` produces zero `process-improvement` proposals.

## New Tests

- `extraction::recurring_friction::tests::recurring_friction_from_three_sessions_returns_recommendations`
- `extraction::recurring_friction::tests::no_friction_from_two_sessions`
- `extraction::recurring_friction::tests::empty_observations_returns_empty`
- `extraction::recurring_friction::tests::recommendations_contain_remediation_text`
- `extraction::recurring_friction::tests::remediation_for_orphaned_calls_is_actionable`
- `extraction::recurring_friction::tests::remediation_for_tool_call_retries_is_actionable`
- `extraction::recurring_friction::tests::remediation_for_session_rollbacks_is_actionable`
- `extraction::recurring_friction::tests::remediation_for_unknown_rule_returns_default`
- `extraction::dead_knowledge::tests::test_compute_dead_knowledge_recommendations_empty_with_insufficient_sessions`
- `extraction::dead_knowledge::tests::test_compute_dead_knowledge_recommendations_empty_with_no_stale_entries`
- `extraction::dead_knowledge::tests::test_compute_dead_knowledge_recommendations_surfaces_stale_entries`
- `extraction::tests::default_extraction_rules_returns_three`
- `extraction::tests::test_recurring_friction_rule_removed_from_defaults`
- `background::tests::test_extraction_tick_does_not_write_recurring_friction_to_entries`

## Tests: pass/fail count

- `cargo test -p unimatrix-observe -p unimatrix-server`: all pass, 0 failed
- `cargo test --workspace` (second run): all pass, 0 failed
- First workspace run had 2 intermittent failures in `uds::listener::tests::col018_topic_signal_*` — pre-existing non-deterministic tokio runtime shutdown under parallel test execution (passes in isolation and in subsequent runs, unrelated to this fix).

## Issues

None. Build clean, no new clippy warnings introduced (pre-existing `auth.rs` collapsible-if warning unchanged on main).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #3252 (ExtractionRule must not perform side effects) confirming the fix approach. No conflicts with existing ADRs.
- Stored: Nothing novel — the root cause pattern is already in entry #3252. The fix is a direct application of the existing constraint. The architect's F-1 finding (observations not available at run_single_tick call site) is fix-specific and not generalizable beyond this refactor.
