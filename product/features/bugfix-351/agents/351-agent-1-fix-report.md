# Agent Report: 351-agent-1-fix

**Bug**: GH #351 — fix(observe): extraction pipeline creates noise instead of deprecating dead knowledge
**Branch**: `bugfix/351-extraction-noise`
**Commit**: `4ef1246`

## Files Modified

- `/workspaces/unimatrix-bugfix-351/crates/unimatrix-observe/src/extraction/dead_knowledge.rs` — Removed `DeadKnowledgeRule` struct and `ExtractionRule` impl. Refactored detection logic into public `detect_dead_knowledge_candidates()` free function. Updated tests.
- `/workspaces/unimatrix-bugfix-351/crates/unimatrix-observe/src/extraction/mod.rs` — Removed `DeadKnowledgeRule` from `default_extraction_rules()` (5 → 4 rules). Removed dead-knowledge 5-session minimum from `min_features_for_rule`. Updated count assertions and added `test_dead_knowledge_rule_removed_from_defaults`.
- `/workspaces/unimatrix-bugfix-351/crates/unimatrix-observe/src/extraction/recurring_friction.rs` — Renamed `_store` → `store` (now used). Added `existing_entry_with_title()` dedup guard. Replaced raw UUID session-list content with enriched `remediation_for_rule()` text. Updated test runtime flavors to `multi_thread` (required by `block_in_place` inside dedup guard).
- `/workspaces/unimatrix-bugfix-351/crates/unimatrix-observe/tests/extraction_pipeline.rs` — Removed `("dead-knowledge", 5)` from cross-rule minimum features test.
- `/workspaces/unimatrix-bugfix-351/crates/unimatrix-server/src/background.rs` — Added `dead_knowledge_deprecation_pass()` (async, cap 50/tick), `run_dead_knowledge_migration_v1()` (COUNTERS-gated one-shot, cap 200), `fetch_recent_observations_for_dead_knowledge()`. Called both from `maintenance_tick()`. Added `counters` and `detect_dead_knowledge_candidates` imports.
- `/workspaces/unimatrix-bugfix-351/product/test/infra-001/suites/test_lifecycle.py` — Added `test_dead_knowledge_entries_deprecated_by_tick` (xfail, GH#291).

## New Tests

**dead_knowledge.rs**:
- `test_dead_knowledge_rule_removed_from_defaults` — verifies DeadKnowledgeRule not in extraction defaults
- `test_dead_knowledge_deprecation_pass_caps_at_50` — verifies detection returns all 60 candidates (cap enforced by caller)
- `test_recently_accessed_entry_not_a_candidate` — entry referenced in recent session snippet excluded
- `test_detect_returns_none_with_insufficient_sessions` — < 5 sessions → None
- `test_detect_returns_empty_with_no_accessed_entries` — empty store → Some([])

**recurring_friction.rs**:
- `test_recurring_friction_skips_if_existing_entry` — dedup guard suppresses duplicate proposals
- `test_recurring_friction_content_has_remediation_not_uuids` — content has "Remediation:", no "[…]" session list
- `remediation_for_permission_retries_is_actionable` — mentions settings.json
- `remediation_for_unknown_rule_returns_default` — non-empty fallback

**background.rs**:
- `test_dead_knowledge_deprecation_pass_unit` — 3 stale entries deprecated, no lesson-learned inserted
- `test_dead_knowledge_migration_v1_deprecates_legacy_entries` — legacy entry deprecated, marker set
- `test_dead_knowledge_migration_v1_is_idempotent` — marker present → migration skips

**test_lifecycle.py**:
- `test_dead_knowledge_entries_deprecated_by_tick` (xfail, GH#291)

## Tests: pass/fail count

- `cargo test -p unimatrix-observe -p unimatrix-server`: **2424 passed / 0 failed**
- `cargo test --workspace`: all suites green, 0 failures

## Issues / Blockers

None. The fix implemented as specified. One deviation from brief: `existing_entry_with_title` in `recurring_friction.rs` handles both multi-thread and single-thread tokio runtime (matching the `dead_knowledge.rs` pattern), and existing tests were updated to `flavor = "multi_thread"` since `block_in_place` requires it.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-observe` (ExtractionRule, maintenance action, deprecation, background tick) — found ADR-005 source_domain guard, tick loop error recovery pattern, and auto-quarantine consecutive counter patterns. None covered the specific ExtractionRule-as-maintenance-action antipattern.
- Stored: entry #3254 "ExtractionRule vs maintenance action: additive rules must not signal deprecation (GH #351)" via `/uni-store-pattern` — documents the antipattern that caused the noise loop and the correct maintenance-pass pattern with COUNTERS-gated one-shot migration.
