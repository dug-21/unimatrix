# Agent Report: crt-018b-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Feature**: crt-018b: Effectiveness-Driven Retrieval

---

## Output

All seven test plan files created under `product/features/crt-018b/test-plan/`:

| File | Lines | Component |
|------|-------|-----------|
| `OVERVIEW.md` | ~130 | Strategy, risk mapping, integration harness plan |
| `effectiveness-state.md` | ~70 | EffectivenessState types, cold start, generation counter |
| `background-tick-writer.md` | ~90 | Write path, hold-on-error semantics, tick_skipped audit |
| `search-utility-delta.md` | ~110 | utility_delta fn, all 4 rerank_score sites, generation cache skip |
| `briefing-tiebreaker.md` | ~100 | Constructor, injection history sort, convention sort |
| `auto-quarantine-guard.md` | ~110 | consecutive_bad_cycles increment/reset/remove, threshold trigger |
| `auto-quarantine-audit.md` | ~90 | Audit event schema, 9-field verification |

---

## Risk Coverage Mapping

| Risk ID | Priority | Covered By | Component File |
|---------|----------|-----------|----------------|
| R-01 | Critical | effectiveness-state (lock ordering), search-utility-delta (read-guard-before-mutex) | both |
| R-02 | Critical | search-utility-delta (all 4 sites + step7/step8 pair tests) | search-utility-delta.md |
| R-03 | Critical | auto-quarantine-guard (per-entry error isolation, 5-entry bulk test) | auto-quarantine-guard.md |
| R-04 | High | background-tick-writer (context_status non-writing integration test) | background-tick-writer.md |
| R-05 | High | search-utility-delta (inside vs outside penalty multiplication, numeric assertion) | search-utility-delta.md |
| R-06 | High | effectiveness-state (Arc<Mutex<>> snapshot), search-utility-delta (clone sharing), briefing-tiebreaker (clone sharing) | multiple |
| R-07 | High | search-utility-delta (utility_delta(None)==0.0), briefing-tiebreaker (priority(None)==0) | multiple |
| R-08 | Medium | background-tick-writer (tick_skipped event on error), auto-quarantine-audit (tick_skipped fields) | multiple |
| R-09 | Medium | briefing-tiebreaker (confidence is primary, effectiveness is tiebreaker) | briefing-tiebreaker.md |
| R-10 | Medium | effectiveness-state (SETTLED_BOOST < 0.03 constant test) | effectiveness-state.md |
| R-11 | Medium | auto-quarantine-guard (Settled/Unmatched entries not quarantined) | auto-quarantine-guard.md |
| R-12 | Low | auto-quarantine-guard (auto_quarantined_this_cycle populated) | auto-quarantine-guard.md |
| R-13 | Critical | background-tick-writer (write lock released before SQL, concurrency test), auto-quarantine-guard (try_read while quarantine runs) | multiple |
| R-14 | Medium | search-utility-delta (test at both spread extremes), OVERVIEW.md integration test for non-zero spread | multiple |

---

## Integration Suite Plan

**Mandatory gate**: `smoke` subset — must pass before any feature-specific suites run.

**Primary suite**: `test_lifecycle.py` — 5 new tests planned:
1. `test_effectiveness_search_ordering_after_tick` — AC-17 item 1
2. `test_briefing_effectiveness_tiebreaker` — AC-17 item 2
3. `test_auto_quarantine_after_consecutive_bad_ticks` — AC-17 item 3 (may need xfail if tick is not test-drivable)
4. `test_crt019_spread_nonzero_prerequisite` — AC-17 item 4
5. `test_context_status_does_not_advance_consecutive_counters` — R-04

**Secondary suite**: `test_security.py` — 1 new test:
- `test_auto_quarantine_env_var_validation` — validates UNIMATRIX_AUTO_QUARANTINE_CYCLES rejects > 1000

**Known gap**: AC-17 item 3 (integration-level auto-quarantine) depends on whether the background tick can be driven externally. The 15-minute interval prevents deterministic test timing. If not test-drivable, this scenario is covered entirely by unit tests and the integration test is marked xfail with a GH Issue for "test-mode tick trigger" (env var to shorten interval for CI).

---

## Open Questions

1. **Background tick test-drivability (AC-17 item 3)**: Can `UNIMATRIX_TICK_INTERVAL_SECONDS` or a `UNIMATRIX_TICK_IMMEDIATE` env var be added to allow integration tests to trigger tick behavior? This unblocks the auto-quarantine integration test. If yes, Stage 3b implementer should add this env var. If no, Stage 3c will mark AC-17 item 3 as integration-untestable and file a GH Issue.

2. **AuditEvent struct shape**: The audit event field for per-entry ID is labeled `target_ids: Vec<u64>` in the architecture (Component 6 example). The auto-quarantine-audit test plan assumes `target_ids` or an equivalent field exists. Stage 3b implementer should confirm the exact AuditEvent field name used for the quarantined entry ID. If the struct shape differs, the test plan assertions need updating at Stage 3c.

3. **`quarantine_entry` mock approach**: The bulk quarantine unit tests (R-03) require injecting a mock that returns an error for one entry while succeeding for others. The existing `TestDb` infrastructure should be checked for mock store support. If `Store::quarantine_entry` is not easily mockable, the test may need a trait abstraction or test-specific store variant.

4. **`compute_report()` error injection (R-08)**: Injecting a `compute_report()` error for the tick-skipped audit event test requires either (a) a mock StatusService, or (b) an environment-configurable failure mode. Stage 3b should identify the cleanest injection point.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for testing procedures — skill invocation succeeded but no MCP tool was available in the current context to call. Proceeded without results.
- Queried: pattern bank via documentation review of existing tests — findings: `confidence.rs` test module is the canonical pattern for `Arc<RwLock<_>>` state tests; `tests_classify.rs` is the canonical location for engine-level constant/pure-function tests; `test_lifecycle.py` is the correct home for multi-step integration scenarios including background-tick-dependent flows.
- Stored: nothing novel to store — the lock-ordering test pattern (R-01, R-13) and clone-sharing Arc<Mutex<>> pattern (R-06) are already documented in Unimatrix per the RISK-TEST-STRATEGY knowledge stewardship note (patterns #1366 and related). The concrete test scenarios here are crt-018b-specific instantiations of those patterns, not novel patterns in their own right.
