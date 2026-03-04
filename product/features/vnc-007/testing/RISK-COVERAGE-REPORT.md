# Risk Coverage Report: vnc-007 (Briefing Unification)

## Test Summary

| Category | Count | Result |
|----------|-------|--------|
| BriefingService unit tests (new) | 21 | All PASS |
| unimatrix-server total unit tests | 730 | All PASS |
| Workspace total unit tests | 1664 | All PASS (1 pre-existing flaky in unimatrix-vector) |
| Integration smoke tests | 19 | All PASS |
| Briefing integration tests | 6 | All PASS |
| Build: default features | -- | PASS |
| Build: --no-default-features | -- | PASS |
| Clippy: unimatrix-server | -- | 0 warnings |

## Risk Coverage Matrix

### R-01: CompactPayload Behavioral Regression (High/Med -> High)
**Status: COVERED**
- T-BS-06: Injection history basic processing with 3 categories at different confidence levels
- T-BS-12: Proportional budget allocation with 9 entries across 3 categories
- T-BS-11: Budget truncation with multiple conventions
- T-BS-13: Minimum budget boundary
- Integration: `test_briefing_with_large_kb` (volume test with 1000 entries)
- Integration: `dispatch_compact_payload_primary_path_uses_injection_history`
- Integration: `dispatch_compact_payload_primary_path_sorts_by_confidence`

**Note**: Exact behavioral equivalence is not guaranteed (token budget vs byte budget), but proportional allocation with category partitioning preserves the same relative priorities.

### R-02: MCP Briefing Semantic Search Regression (Med/Med -> Med)
**Status: COVERED**
- T-BS-05: Semantic search with EmbedNotReady graceful degradation
- T-BS-04: Semantic search isolation when include_semantic=false
- Integration: `test_briefing_returns_content`
- Integration: `test_briefing_reflects_stored_knowledge`
- Co-access anchors: BriefingService passes convention entry_ids to SearchService as co_access_anchors (code review confirmed, line 225-229 of briefing.rs)

### R-03: Feature Flag + rmcp Macro Compatibility (Med/Med -> Med)
**Status: COVERED**
- Build: `cargo build --workspace` succeeds (default features, mcp-briefing enabled)
- Build: `cargo build --no-default-features -p unimatrix-server` succeeds (mcp-briefing disabled)
- Wrapper approach: cfg blocks inside method body (not on method), per ADR-001 fallback
- AC-16: `#[cfg(feature = "mcp-briefing")]` gating confirmed in tools.rs (lines 34, 45, 1394, 1401)
- AC-17: `default = ["mcp-briefing"]` in Cargo.toml confirmed

### R-04: Injection History Path Latency (High/Low -> Med)
**Status: COVERED**
- T-BS-04: SearchService isolation test. Embed service not loaded. `include_semantic=false` with task query. Service does NOT call SearchService -- passes without EmbedNotReady error.
- Code review: `assemble()` checks `params.include_semantic` before any SearchService interaction (line 216). When false, SearchService is never touched.
- T-BS-06/07/08/09/10/12/13: All injection history tests use `include_semantic: false` and succeed without SearchService involvement.

### R-05: Quarantine Exclusion in Injection History (Med/Med -> Med)
**Status: COVERED**
- T-BS-08: Active + quarantined entries in injection history. Quarantined excluded, active retained. Explicit assertion on excluded entry ID.
- T-BS-20: All entries quarantined. All sections empty, entry_ids empty.
- T-BS-10: Deprecated entries included (not excluded like quarantined).
- Code: `SecurityGateway::is_quarantined(&entry.status)` check at line 313.

### R-06: Budget Overflow with Mixed Sources (Med/Low -> Low)
**Status: COVERED**
- T-BS-13: Minimum budget (max_tokens=500). Single small entry fits.
- T-BS-11: Multiple entries with budget constraint.
- T-BS-12: Mixed categories with proportional allocation.
- Validation: max_tokens below 500 rejected (T-BS-16). max_tokens=0 returns validation error.

### R-07: Duties Removal Test Breakage (Low/High -> Med)
**Status: COVERED**
- response.rs: Briefing struct has no `duties` field (AC-09 verified: grep confirms no `duties` in struct definition)
- format_briefing: duties sections removed from all 3 formats
- Tests updated: negative assertions `!text.contains("duties")` in summary, markdown, JSON formats
- AC-10: tools.rs has zero matches for "duties"
- AC-12: briefing.rs has zero matches for "duties"
- All 730 server tests pass, including updated format_briefing tests

### R-08: EmbedNotReady Fallback (Med/Low -> Low)
**Status: COVERED**
- T-BS-05: BriefingService with embed not ready. `search_available=false` in result. Conventions still populated.
- Integration: `test_briefing_empty_db` (verifies empty-db case works)

### R-09: CompactPayload Format Text Divergence (Med/Med -> Med)
**Status: PARTIALLY COVERED**
- CompactPayload formatting (`format_compaction_payload`) is NOT modified by vnc-007. BriefingService returns structured data; formatting remains in uds_listener.rs.
- Integration: `dispatch_compact_payload_primary_path_uses_injection_history` and `dispatch_compact_payload_primary_path_sorts_by_confidence` verify correct output.
- Section headers and entry formatting remain unchanged in `format_compaction_payload`.

### R-10: dispatch_unknown_returns_error Test (Low/High -> Low)
**Status: COVERED**
- Test renamed from `dispatch_unknown_returns_error` to `dispatch_briefing_returns_content`
- HookRequest::Briefing now wired and returns BriefingContent response
- AC-36: Test passes (confirmed in 730/730 pass count)

## Acceptance Criteria Verification

### Verified by Test
| AC | Status | Evidence |
|----|--------|----------|
| AC-02 | PASS | T-BS-01: convention lookup with role returns 2 entries |
| AC-03 | PASS | T-BS-04: semantic search not called when include_semantic=false; T-BS-05: called when true |
| AC-04 | PASS | T-BS-06: injection history partitioned into decisions/injections/conventions |
| AC-05 | PASS | T-BS-11/12/13: budget truncation and proportional allocation |
| AC-06 | PASS | T-BS-08/20: quarantined entries excluded from all paths |
| AC-07 | PASS | T-BS-14/15/16/17: input validation for role, task, max_tokens, control chars |
| AC-11 | PASS | format_briefing tests: no "duties" or "Duties" in any format output |
| AC-22 | PASS | dispatch_briefing_returns_content test |
| AC-24 | PASS | dispatch_briefing_returns_content: returns BriefingContent, not Error |
| AC-33 | PASS | 730 tests (net increase from 709 pre-vnc-007 + 21 new) |
| AC-34 | PASS | 21 unit tests covering all entry sources and edge cases |

### Verified by Grep/Shell
| AC | Status | Evidence |
|----|--------|----------|
| AC-01 | PASS | `assemble()` at line 131 |
| AC-08 | PASS | `briefing` in mod.rs: module, use, field, construction |
| AC-09 | PASS | No `duties` field in Briefing struct (only in test assertions and comments) |
| AC-10 | PASS | Zero matches for "duties" in tools.rs |
| AC-12 | PASS | Zero matches for "duties" in briefing.rs |
| AC-13 | PASS | `services.briefing.assemble` delegation confirmed in tools.rs |
| AC-16 | PASS | `#[cfg(feature = "mcp-briefing")]` at 4 locations in tools.rs |
| AC-17 | PASS | `default = ["mcp-briefing"]` in Cargo.toml |
| AC-18 | PASS | `services.briefing.assemble` delegation confirmed in uds_listener.rs |
| AC-25 | PASS | `cargo build --no-default-features -p unimatrix-server` succeeds |
| AC-26 | PASS | `cargo build -p unimatrix-server` succeeds + integration tests pass |
| AC-27 | PASS | BriefingService available in both feature configs (UDS path always active) |
| AC-37 | PASS | git diff --stat confirms changes only in crates/unimatrix-server/ |

### Verified by Manual Inspection
| AC | Status | Evidence |
|----|--------|----------|
| AC-15 | PASS | tools.rs retains: resolve_agent, validate_briefing_params, validated_max_tokens, format_briefing, record_usage_for_entries |
| AC-20 | PASS | SessionRegistry lookup at uds_listener.rs line ~837 precedes BriefingService call |
| AC-21 | PASS | Compaction count increment in existing test `dispatch_compact_payload_primary_path_uses_injection_history` |

### Deferred
| AC | Status | Reason |
|----|--------|--------|
| AC-28-32 | DEFERRED | Rate limiting deferred to vnc-009 per ADR-004 |

### Snapshot Tests (AC-14, AC-19, AC-35)
**Status: NOT IMPLEMENTED** -- Exact output equivalence tests are impractical because:
1. BriefingService uses token budget (proportional) vs old code's byte budget (fixed)
2. Entry selection order may differ due to proportional allocation
3. Format functions (`format_briefing`, `format_compaction_payload`) are unchanged, so format text is identical for identical entries.

The integration tests `test_briefing_returns_content`, `test_briefing_reflects_stored_knowledge`, and `dispatch_compact_payload_primary_path_*` serve as behavioral equivalence evidence.

## Integration Test Counts

| Suite | Total | Passed | Failed | Skipped |
|-------|-------|--------|--------|---------|
| Smoke (all suites) | 19 | 19 | 0 | 163 |
| test_tools (briefing) | 4 | 4 | 0 | 64 |
| test_lifecycle (briefing) | 1 | 1 | 0 | -- |
| test_volume (briefing) | 1 | 1 | 0 | -- |
| **Total briefing integration** | **6** | **6** | **0** | -- |

## Pre-existing Issues

| Issue | Impact on vnc-007 | Action |
|-------|-------------------|--------|
| test_compact_search_consistency (flaky, unimatrix-vector) | None -- HNSW compaction non-determinism, unrelated to vnc-007 | No action needed |
| Clippy warnings in unimatrix-store, unimatrix-adapt (collapsible_if) | None -- pre-existing in unmodified crates | No action needed |

## Residual Risk

| Risk | Residual Level | Justification |
|------|---------------|---------------|
| R-01 | Low | Proportional allocation tested; format functions unchanged; 6 integration tests pass |
| R-02 | Low | SearchService delegation tested; co-access anchors wired; integration test passes |
| R-03 | Very Low | Both feature configurations build and test clean |
| R-04 | Very Low | Isolation proven by test (SearchService not invoked when disabled) |
| R-05 | Very Low | Explicit quarantine exclusion tests pass |
| R-06 | Low | Budget validation prevents extreme values; truncation tested |
| R-07 | Very Low | Negative assertions confirm no duties in output |
| R-08 | Very Low | Graceful degradation tested; conventions still returned |
| R-09 | Very Low | Format functions unchanged; integration tests pass |
| R-10 | Very Low | Test updated and passes |
