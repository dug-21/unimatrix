# Agent Report: col-026-agent-4-recommendation-fix

**Component**: compile_cycles recommendation fix (report.rs)
**ADR**: ADR-005
**AC**: AC-19, AC-13, AC-17

---

## Changes Made

### Primary change: `crates/unimatrix-observe/src/report.rs`

Replaced the `compile_cycles` match arm in `recommendation_for()`:

- **Old action**: `"Consider incremental compilation or targeted cargo test invocations"`
- **New action**: Batching/iterative-compilation framing describing iterative per-field struct changes as root cause
- **Old rationale**: `"{:.0} compile cycles detected (threshold: 10) -- consider narrowing test scope"` (contained threshold language)
- **New rationale**: Describes compile-check-fix loop cost, recommends batching to logical units — no `(threshold:` language

The `permission_retries` arm is confirmed unchanged — it retains "allowlist" framing (correct per ADR-005).

Updated `test_recommendation_compile_cycles_above_threshold` assertion:
- Removed: `assert!(recs[0].action.contains("incremental"))`
- Added: asserts `batch` or `iterative` present; asserts `allowlist` absent; asserts `settings.json` absent

Added four new tests:
- `test_compile_cycles_action_no_allowlist` (T-CC-01, AC-19)
- `test_permission_friction_recommendation_independence` (T-CC-02, AC-19)
- `test_compile_cycles_rationale_no_threshold_language` (T-CC-03, AC-13)
- `test_compile_cycles_at_threshold_boundary` (edge case: measured == 10.0 produces no recommendation)

### Compile-time migration: construction sites

Added `goal: None, cycle_type: None, attribution_path: None, is_in_progress: None, phase_stats: None` to all `RetrospectiveReport` struct literals not yet updated by the linter:

- `crates/unimatrix-observe/src/phase_narrative.rs` — 2 test fixtures
- `crates/unimatrix-server/src/mcp/tools.rs` — 1 production site (cached path), 3 test fixtures

Note: `FeatureKnowledgeReuse` construction sites and `retrospective.rs` `make_report()` were already fixed by the auto-linter from the Component 1 agent's changes (committed in HEAD~1).

---

## Test Results

**`cargo test -p unimatrix-observe -- report`**: 39 passed / 0 failed

Named tests confirmed passing:
- `test_recommendation_compile_cycles_above_threshold` — PASS (updated assertion)
- `test_recommendation_compile_cycles_below_threshold` — PASS (unchanged)
- `test_recommendation_permission_retries` — PASS (allowlist still present)
- `test_compile_cycles_action_no_allowlist` — PASS (new)
- `test_permission_friction_recommendation_independence` — PASS (new)
- `test_compile_cycles_rationale_no_threshold_language` — PASS (new)
- `test_compile_cycles_at_threshold_boundary` — PASS (new)

**`cargo test --workspace`**: all suites passed, 0 failures

---

## Files Modified

- `crates/unimatrix-observe/src/report.rs` (included in HEAD~1 via linter auto-stage)
- `crates/unimatrix-observe/src/phase_narrative.rs` (committed in this agent's commit)
- `crates/unimatrix-server/src/mcp/tools.rs` (committed in this agent's commit)

---

## Issues / Blockers

None.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-observe` — no results returned (anonymous agent, no read results surfaced)
- Stored: attempted to store pattern "unimatrix-observe: RetrospectiveReport struct extension requires compile-time migration at all construction sites" via `/uni-store-pattern` — **FAILED** with `MCP error -32003: Agent 'anonymous' lacks Write capability`. Pattern not stored. Recommend coordinator stores this after the swarm completes.

### Pattern to store (for coordinator)

> **Title**: `unimatrix-observe: RetrospectiveReport struct extension requires compile-time migration at all construction sites`
>
> **Content**: When new fields are added to `RetrospectiveReport` (or `FeatureKnowledgeReuse`) in `types.rs`, ALL struct literals across the workspace must be updated — not just the production `build_report()` path. In col-026, construction sites existed in 6 files (phase_narrative.rs, types.rs, report.rs, tools.rs, knowledge_reuse.rs, retrospective.rs). Cargo requires all struct fields explicitly initialized since the struct does not use `#[non_exhaustive]`. All new optional fields use `#[serde(default, skip_serializing_if = "Option::is_none")]` — set to `None` in test fixtures. The linter auto-fixed most sites on save, but not all — always run `cargo build --workspace` to catch remaining gaps before committing.
>
> **Category**: `pattern` **Topic**: `unimatrix-observe`
