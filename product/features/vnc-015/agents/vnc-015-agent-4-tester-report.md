# Agent Report: vnc-015-agent-4-tester (Stage 3c)

## Task Summary

Executed tests for vnc-015 (Typed Edge Write Path + context_edge Tool). Ran unit tests, integration smoke tests, and all relevant integration suites. Wrote 36 new integration tests. Identified and fixed 2 implementation gaps.

## Unit Test Results

- **Total**: 4,896 passed, 0 failed, 28 ignored
- **vnc-015 specific**: ~75 unit tests covering RelationType (20 round-trip + Pass 2b), DependencyOnDeprecatedRule (6), default_rules (6), edge_write (7), stale_dependency_edges (7)

## Integration Test Results

| Suite | Collected | Pass | XFail | Notes |
|-------|-----------|------|-------|-------|
| smoke | 23 | 23 | 0 | Mandatory gate — PASS |
| protocol | 13 | 13 | 0 | `test_list_tools_returns_thirteen` passes |
| tools | 156 | 156 | 0 | 33 new vnc-015 tests added |
| lifecycle | 59 | 59 | 0 | 3 new vnc-015 tests added |
| security | 20 | 20 | 0 | |
| contradiction | 13 | 13 | 0 | |
| edge_cases | 24 | 22 | 2 | Pre-existing xfail (GH#303, GH#305) |

**Total: 308 executed, 306 passed, 2 xfailed (pre-existing)**

## Implementation Bugs Fixed

### Bug 1: EdgeParams missing agent_id field (AC-21 blocker)

`context_edge` handler called `build_context_with_external_identity(&None, ...)` — ignoring any `agent_id` from params. All `context_edge` calls used the MCP session identity ("human"), bypassing capability enforcement for enrolled agents.

**Fix**: Added `agent_id: Option<String>` and `format: Option<String>` to `EdgeParams`; updated handler to pass `&params.agent_id`.

**Files**: `crates/unimatrix-server/src/mcp/tools.rs`

### Bug 2: stale_dependency_edges not surfaced in context_status (AC-11 blocker)

`GraphCohesionMetrics.stale_dependency_edges` was computed correctly (7 unit tests pass) but not mapped into `StatusReport` or the `StatusReportJson` formatter.

**Fix**: Added field to `StatusReport`, `StatusReportJson`, `Default impl`, and `from()` mapper. Also updated 8 `StatusReport` initializers in `mcp/response/mod.rs` test helpers.

**Files**: `crates/unimatrix-server/src/mcp/response/status.rs`, `crates/unimatrix-server/src/services/status.rs`, `crates/unimatrix-server/src/mcp/response/mod.rs`

## New Integration Tests Added

**test_tools.py** (+33 tests): context_edge tool registration, add/remove/redirect modes, Contradicts bidirectionality, SourceFrozen gate, capability enforcement, no-ownership-check, target validation, self-referential rejection, idempotency, rollback-on-failure.

**test_lifecycle.py** (+3 tests): `test_stale_dependency_appears_in_context_status`, `test_contradicts_query_bidirectional`, `test_edge_survives_server_restart`.

**harness/client.py** additions: `context_edge()` method; `edges` param on `context_store()` and `context_correct()`.

## SR-01 Verification (ADR-007)

All 10×4 cells confirmed: enum body, as_str(), from_str() for all 10 variants. `RelatedTo` in graph_ppr.rs and graph_expand.rs positive sets. `Advances`/`Motivates` absent from code (comments only — negative grep confirms).

## Risk Coverage

All 15 risks covered. No pre-existing xfail markers filed from this feature — the 2 xfailed edge_cases tests (GH#303, GH#305) are pre-existing from prior features.

## AC Coverage

- 25/26 ACs: PASS
- AC-12 (DependencyOnDeprecated end-to-end via `context_cycle_review`): PARTIAL — rule fires in unit tests; full integration path requires complex CYCLE_EVENTS seeding not in scope for this stage.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found 12 entries, ADRs for vnc-015 orientation
- Stored: nothing novel to store — bugs fixed were feature-specific defects, not reusable patterns (agent_id params pattern and GraphCohesionMetrics surfacing are already-established patterns in this codebase)
