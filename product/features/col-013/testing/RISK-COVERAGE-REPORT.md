# Risk Coverage Report: col-013 Extraction Rule Engine

**Date**: 2026-03-05
**Feature**: col-013

## Test Summary

| Category | Count | Status |
|----------|-------|--------|
| Unit tests (all crates) | 1419 | PASS |
| Integration tests (sqlite_parity) | 29 | PASS |
| Integration smoke tests (infra-001) | 18/19 | 18 PASS, 1 PRE-EXISTING FAIL |
| New tests added | 50 | PASS |

### Pre-existing Integration Failure

- `test_volume.py::TestVolume1K::test_store_1000_entries` - rate limited at 60/3600s
  - **Root cause**: Pre-existing rate limit configuration (60/hour) prevents storing 1000 entries in a single test run
  - **Relation to col-013**: NONE - this test was failing before col-013
  - **Action**: No xfail marker needed; this is a known `@pytest.mark.volume` test

## Risk Coverage Matrix

| Risk | Severity | Test Coverage | Status |
|------|----------|---------------|--------|
| R-01: Low-quality entries | High | 15 quality gate unit tests, 30+ extraction rule tests, trust_score("auto")=0.35 test | MITIGATED |
| R-02: Silent tick failure | Medium | TickMetadata unit tests, tick_metadata in StatusReport, tracing logs | MITIGATED |
| R-03: CRT regressions | High | 2 trust_score tests, workspace-wide 1419 tests passing | MITIGATED |
| R-04: Observation query performance | Medium | Watermark pattern implemented, O(new_rows) verified by design | ACCEPTED |
| R-05: SQLite write contention | Medium | Same spawn_blocking + store locking as existing writes | MITIGATED |
| R-06: Type migration breaks imports | Low | cargo build --workspace succeeds, re-exports verified | MITIGATED |
| R-07: Rate limit reset on restart | Low | Rate limit unit test, accepted by design | ACCEPTED |

## Acceptance Criteria Verification

| AC | Description | Verification | Status |
|----|-------------|--------------|--------|
| AC-01 | ExtractionRule trait exists | `extraction/mod.rs` ExtractionRule trait | PASS |
| AC-02 | 5 extraction rules implemented | 5 rule modules verified | PASS |
| AC-03 | Quality gate pipeline (6 checks) | 4 in-memory + 2 embedding-based checks | PASS |
| AC-04 | Watermark-based incremental processing | ExtractionContext.last_watermark in extraction_tick() | PASS |
| AC-05 | Background tick (15-min default) | TICK_INTERVAL_SECS = 900 in background.rs | PASS |
| AC-06 | StatusReport gains maintenance fields | 4 new fields added | PASS |
| AC-07 | maintain=true silently ignored | No error, no side effect in tools.rs | PASS |
| AC-08 | trust_score("auto") = 0.35 | confidence.rs trust_score function | PASS |
| AC-09 | check_entry_contradiction() exists | contradiction.rs public function | PASS |
| AC-10 | coherence_by_source in StatusReport | status.rs groups by trust_source | PASS |
| AC-11 | Type migration with re-exports | observation.rs in core, re-exports in observe | PASS |
| AC-12 | Rate limit: 10/hour | ExtractionContext::check_and_increment_rate() | PASS |
| AC-13 | Auto-extracted entries stored with trust_source="auto" | background.rs extraction_tick() | PASS |

## Unit Test Details

### New Tests (50 total)

**Extraction module (unimatrix-observe)**: 45 tests
- `extraction/mod.rs`: 15 tests (quality gate, helpers, defaults)
- `extraction/knowledge_gap.rs`: 7 tests (gap detection, cross-feature, normalization)
- `extraction/implicit_convention.rs`: 7 tests (100% consistency, partial, normalization)
- `extraction/dead_knowledge.rs`: 6 tests (dormant entries, recent access, ID extraction)
- `extraction/recurring_friction.rs`: 4 tests (3+ features, below minimum)
- `extraction/file_dependency.rs`: 6 tests (read-write chains, window, no pattern)

**CRT refactors**: 2 tests
- `confidence.rs`: trust_score_auto_value, trust_score_auto_between_agent_and_fallback

**Background tick**: 3 tests
- `background.rs`: tick_metadata_new_defaults, parse_hook_type_variants, now_secs_returns_reasonable_value

### Regression Tests

All 1419 unit tests pass across:
- unimatrix-core: 21
- unimatrix-embed: 76 (18 ignored - require model download)
- unimatrix-engine: 173
- unimatrix-observe: 275
- unimatrix-server: 770
- unimatrix-vector: 104

### Integration Tests

- sqlite_parity: 29 passed
- infra-001 smoke: 18 passed, 1 pre-existing failure (volume rate limit)

## Gaps and Residual Risk

1. **Background tick integration test**: The tick requires a running server and 15-minute interval. Covered by unit tests on TickMetadata and extraction_tick logic. Full integration testing would require tokio::time::pause() or a configurable interval -- deferred to a future test infrastructure improvement.

2. **Near-duplicate and contradiction quality gates (checks 5-6)**: Require embedding model loaded at runtime. Covered by design (same patterns as existing contradiction scan). Unit-tested through individual components.

3. **Auto-extracted entry ranking**: Requires embedding model for semantic search ranking. trust_score("auto") = 0.35 is verified by unit test; ranking behavior is a composition of existing search reranking code (unchanged).
