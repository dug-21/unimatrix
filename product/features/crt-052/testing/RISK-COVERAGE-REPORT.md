# Risk Coverage Report: crt-052

> Stage: 3c (Test Execution) · Branch: feature/crt-052 (#689) · Date: 2026-06-08
> Inputs: RISK-TEST-STRATEGY.md (R-01..R-20), ACCEPTANCE-MAP.md (AC-01..13, AC-V-SEAM, AC-V-FUZZ),
> test-plan/OVERVIEW.md (integration harness plan), gate-3b-report.md.
> Result: **All unit tests pass; mandatory smoke gate passes; all 6 merge gates green.**

## Execution Summary

| Layer | Command | Result |
|-------|---------|--------|
| Unit — observe | `cargo test -p unimatrix-observe --lib` | **514 passed, 0 failed** |
| Unit — server lib | `cargo test -p unimatrix-server --lib` | **3747 passed, 0 failed, 1 ignored** |
| Integration smoke (MANDATORY gate) | `pytest suites/ -m smoke` | **23 passed, 0 failed** |
| Integration suites | `pytest test_protocol test_tools test_lifecycle test_security test_edge_cases` | **308 passed, 10 xfailed, 2 xpassed, 0 failed** (320 collected) |
| Dependency posture (AC-13) | `cargo audit` | 1 pre-existing CVE (RUSTSEC-2023-0071, rsa via sqlx-mysql) — NOT in any crt-052 chain |

Environment note: full-parallel workspace test runs OOM (SIGKILL) in this environment; unit runs used
`CARGO_BUILD_JOBS=2` with targeted `-p` crate filters per the spawn-prompt constraint. All counts are
the per-crate `cargo test --lib` totals.

## Risk Coverage Matrix (R-01 .. R-20)

| Risk | Pri | Test(s) — verified passing | Result | Coverage |
|------|-----|-----------------------------|--------|----------|
| R-01 wrong-cycle re-adopt | Crit | `continuity_simulated_lifecycle` (loud re-adopt match / fail-loud mismatch), `test_readopt_mismatch_diagnostic_metadata_only` | PASS | Full |
| R-02 unbounded held memory | Crit | `continuity_simulated_lifecycle` (bounded held-count + 3 observable evictions + TTL reclaim w/o review) | PASS | Full |
| R-03 audit not exactly-once | Crit | `continuity_simulated_lifecycle` (exactly-once per held session), `test_audit_detail_content_free`; ADR-009 no-consumer survey recorded CLEAN | PASS | Full |
| R-04 content leak to SQL/log/audit | Crit | `test_candidates_structurally_absent_from_memoized_report`, `test_audit_detail_content_free`, MCP `test_cycle_review_rereview_no_persisted_candidates`, `test_cycle_review_no_candidate_content_in_query_surface` | PASS | Full |
| R-05 unfaithful AC-11 sim | Crit | `continuity_simulated_lifecycle` — 3 drains through production entry points, inter-drain deltas, cross-turn TURN1/2/3 content asserted | PASS | Full |
| R-06 third buffer reader | High | `test_700_reuse_parses_snapshot_bytes_without_contiguous_tail`; single-reader structural by construction (private `data`/`snapshot_block`) | PASS | Full (1 WARN — see Gaps) |
| R-07 not-all-four-returns | Crit | `test_exhaustiveness_fifth_return_fails`, `test_distill_strictly_before_purge_at_each_return` | PASS | Full |
| R-08 parse/match under lock; torn read | High | `test_seam_no_parse_under_lock` (source assertion), `test_concurrent_deltas_during_seam_consistent` (4-writer stress) | PASS | Full |
| R-09 fallback mis-calibrated | High | `test_trigger_elided_above_threshold_falls_back`, `test_trigger_nonempty_no_loss_is_primary`, distill_handler fallback cluster | PASS | Full |
| R-10 parser panic on adversarial | Crit | observe `corpus_tests` (truncated/non-UTF-8/unknown-type/embedded-NUL fuzz), `test_handler_fully_corrupt_snapshot_normal_response`, MCP `test_cycle_review_corrupt_buffer_no_panic` | PASS | Full |
| R-11 Wave B contaminates Wave A | High | `test_seam_wave_a_only_registered_scan`, Wave-A no-transcript_hold dependency assertions (gate-3b verified) | PASS | Full |
| R-12 array-relative byte_offset | High | logical byte_offset in `select_candidates` + overflow accounting (`test_elided_bytes_accounting_exact`); covered in observe select tests | PASS | Full |
| R-13 snapshot/purge scan divergence | High | seam scans registered ∪ held with Arc-identity dedup; `continuity_simulated_lifecycle` post-review no-held-survives; gate-3b R-13 verified | PASS | Full |
| R-14 topic_source as filter | Low | reconstruct stable-sort no-op (topic_source inert for v1, never a filter) — observe reconstruct tests | PASS | Full |
| R-15 non-deterministic aggregate cap | Med | deterministic keep-earliest truncation in select; observe select cap tests | PASS | Full |
| R-16 silent eviction/poison drop | High | `continuity_simulated_lifecycle` eviction audit; poison treat-as-empty + loss surfacing (session_transcript poison tests) | PASS | Full |
| R-17 delta routing regresses hot path | Med | O(1) keyed held-buffer lookup; delta-apply lock-class unchanged (transcript_hold tests) | PASS | Full |
| R-18 RetainDays distills/purges | Low | `test_retention_match_no_wildcard`, `test_validate_rejects_retaindays_enterprise_only`, `test_retention_merge_revalidated_rejects_retaindays` | PASS | Full |
| R-19 content-bearing Debug | Med | `test_snapshot_debug_metadata_only`, `test_heldbuffer_debug_metadata_only`, `test_holeinfo_debug_safe`, `test_debug_output_contains_no_payload_bytes` | PASS | Full |
| R-20 self-fulfilling fixture | High | `test_corpus_provenance_header_present`, `test_independent_corpus_recall_ge_090`, `test_selected_volume_le_10pct` | PASS | Full |

**No risk lacks test coverage.** All 7 Critical risks and all 8 High risks have full coverage with named,
passing tests verified in isolation.

## Merge-Gate Coverage (non-negotiable — all green)

| # | Merge gate | Evidence (verified passing in isolation) | Status |
|---|-----------|------------------------------------------|--------|
| 1 | AC-11 `continuity_simulated_lifecycle` | `infra::transcript_hold::tests::ac11::continuity_simulated_lifecycle` — 3 drains, inter-drain deltas, cross-turn content, loud re-adopt/mismatch, bounded held-count + eviction, TTL reclaim, exactly-once audit | PASS |
| 2 | Content-leak (AC-06) | `test_candidates_structurally_absent_from_memoized_report` (compile-level absence) + metadata-only Debug cluster + `test_audit_detail_content_free` + MCP `test_cycle_review_rereview_no_persisted_candidates` (summary_json candidate-free across restart) | PASS |
| 3 | Four-return exhaustiveness (AC-05) | `test_exhaustiveness_fifth_return_fails` (4 purge == 4 distill == 4 attach), `test_distill_strictly_before_purge_at_each_return` | PASS |
| 4 | AC-V-FUZZ no-panic (R-10) | observe `corpus_tests` (module-level), `test_handler_fully_corrupt_snapshot_normal_response` (handler-level), MCP `test_cycle_review_corrupt_buffer_no_panic` | PASS |
| 5 | AC-V-SEAM single-reader (R-06) | `test_700_reuse_parses_snapshot_bytes_without_contiguous_tail`; single-reader invariant structural by construction | PASS (1 WARN) |
| 6 | AC-01 snapshot-and-release (R-08) | `test_seam_no_parse_under_lock` + `test_concurrent_deltas_during_seam_consistent` | PASS |

## New Integration Tests Added (Stage 3c)

Per the OVERVIEW Integration Harness Plan, 5 MCP-level tests were added and all pass. Rationale: the
in-memory transcript buffer is fed exclusively through the UDS `transcript_delta` hook path, which is NOT
active in the stdio MCP harness — so a populated buffer cannot be driven through MCP. The populated-buffer
multi-turn proof therefore stays in the Rust `continuity_simulated_lifecycle` (AC-11), and these MCP tests
assert the protocol-observable contract: additive/absent-when-empty, no leak, graceful no-panic degrade.
(A review over a cycle with no observation rows returns MCP error -32010, so each test seeds minimal
observation rows directly — the harness convention used by the existing lifecycle cycle-review tests.)

| File | Test | Asserts | AC/Risk |
|------|------|---------|---------|
| `suites/test_tools.py` | `test_cycle_review_transcript_candidates_absent_when_empty` | no live buffer → `transcript_candidates` ABSENT (not null/empty) in response and in report JSON | AC-04 |
| `suites/test_tools.py` | `test_cycle_review_response_additive_only` | pre-existing `curation_health` block intact; candidates section purely additive (absent here) | AC-04 |
| `suites/test_lifecycle.py` | `test_cycle_review_rereview_no_persisted_candidates` | first review + restart re-review (memoization hit, #3800) carries no stale candidates; persisted `cycle_review_index.summary_json` candidate-free | AC-06, R-04 |
| `suites/test_security.py` | `test_cycle_review_corrupt_buffer_no_panic` | review over buffer-degrade path returns normal MCP response, candidates absent, server stays responsive (no handler panic) | AC-V-FUZZ, R-10 |
| `suites/test_security.py` | `test_cycle_review_no_candidate_content_in_query_surface` | no candidate/transcript content surfaced by search/status or persisted record | AC-06, R-04 |

No harness infrastructure was changed; no integration test was deleted or commented out.

## Test Results

### Unit Tests
- unimatrix-observe lib: **Total 514 · Passed 514 · Failed 0**
- unimatrix-server lib: **Total 3747 · Passed 3747 · Failed 0 · Ignored 1**
- Combined: **4261 passed, 0 failed**

### Integration Tests (infra-001, compiled `unimatrix` binary over MCP JSON-RPC)
- Smoke gate (`-m smoke`): **Total 23 · Passed 23 · Failed 0** — MANDATORY gate GREEN
- Suites run (protocol, tools, lifecycle, security, edge_cases): **Total 320 · Passed 308 · xfailed 10 · xpassed 2 · Failed 0** (46m05s)
- 5 new crt-052 tests: **5 passed** (included in the 308)

#### xfail / xpass accounting
All 10 xfailed and 2 xpassed are PRE-EXISTING markers unrelated to crt-052, each carrying its own GH
Issue (GH#576, #111, #406, #405, #305, #575, and others in the lifecycle suite). The 2 xpassed are
non-strict pre-existing markers that incidentally passed (a signal to revisit those markers in their
owning issues — out of crt-052 scope). No crt-052 test is marked xfail. No new xfail markers were added.

## Failure Triage (USAGE-PROTOCOL)

No integration test failed. One unit-test concern was triaged as pre-existing:

| Item | Triage | Action |
|------|--------|--------|
| `http::token::tests::test_concurrent_creation_no_corruption`, `http::listener::tests::test_semaphore_recovery_*`, `uds::listener::tests::stamp_read::*` | **Pre-existing flaky** — pass in isolation (1 + 21 passed cleanly); concurrency/timing races that surface only under full-parallel contention (exacerbated by the OOM-prone full-parallel build). Not crt-052 files, not a regression. | **Filed GH#705.** These are Rust unit tests (not infra-001), so no `xfail` marker applies — tracked via the issue only, per the spawn-prompt instruction. |

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 **[gate]** | PASS | `test_seam_no_parse_under_lock`, `test_concurrent_deltas_during_seam_consistent` |
| AC-02 | PASS | observe `select` tests — block-type drop, whole-block, dedup, per-session + per-cycle caps, deterministic truncation, ordering/hints (in 514 observe lib) |
| AC-03 | PASS | `test_independent_corpus_recall_ge_090` (≥0.90), `test_selected_volume_le_10pct` (≤10%), `test_corpus_provenance_header_present` |
| AC-04 | PASS | MCP `test_cycle_review_transcript_candidates_absent_when_empty`, `test_cycle_review_response_additive_only`; Rust `test_zero_attributed_sessions_section_absent`, `test_cycle_review_render_baseline_byte_identical` (golden-diff) |
| AC-05 **[gate]** | PASS | `test_exhaustiveness_fifth_return_fails`, `test_distill_strictly_before_purge_at_each_return` |
| AC-06 **[gate]** | PASS | `test_candidates_structurally_absent_from_memoized_report`, metadata-only Debug cluster, `test_audit_detail_content_free`, MCP `test_cycle_review_rereview_no_persisted_candidates` + `test_cycle_review_no_candidate_content_in_query_surface` |
| AC-07 | PASS | distill_handler fallback trigger tests (empty/elided-above-threshold/holes); topic_source reorder-not-filter |
| AC-08 | PASS | loss-section population (`test_merge_cycle_drops_into_existing_loss_row`, `SessionLossInfo.dropped_candidates`); elided/hole metadata tests |
| AC-09 | PASS | two-pipe boundary preserved — `insert_observations_batch` unchanged; batch filter untouched (gate-3b verified, in server lib suite) |
| AC-10 | PASS | `test_retention_match_no_wildcard`, `test_validate_rejects_retaindays_enterprise_only`, `test_retention_merge_revalidated_rejects_retaindays` |
| AC-11 **[gate]** | PASS | `continuity_simulated_lifecycle` — the only pre-merge primary-path proof; all five sub-assertions (a)-(e) |
| AC-12 | PASS | `test_select_4mib_under_50ms` (off-lock 4 MiB rule pass < 50 ms) |
| AC-13 | PASS (1 WARN) | regex-class-only added dep (`regex = "1"`); `cargo audit` carries 1 PRE-EXISTING CVE (RUSTSEC-2023-0071, rsa via sqlx-mysql→sqlx→unimatrix-store) with no upstream fix — NOT introduced by crt-052, NOT in any crt-052-touched chain. Consumer-guidance doc updates verified in gate-3b. |
| AC-V-SEAM **[gate]** | PASS (1 WARN) | `test_700_reuse_parses_snapshot_bytes_without_contiguous_tail`; all four metadata fields exposed; single-reader by construction |
| AC-V-FUZZ **[gate]** | PASS | observe `corpus_tests`, `test_handler_fully_corrupt_snapshot_normal_response`, MCP `test_cycle_review_corrupt_buffer_no_panic` |

## Prerequisite Delivery Gate

**ADR-009 no-consumer audit survey — RECORDED CLEAN.** Artifact:
`product/features/crt-052/reports/adr-009-no-consumer-survey.md` (Verdict: CLEAN — no downstream
consumer keys on per-close `transcript_session_purged` cadence; Wave B audit-shape move is safe). This
confirms the R-03 exactly-once tests are valid gate evidence (the cadence change is meaningful only with
no consumer dependent on the old per-close cadence).

## Gaps

No risk from RISK-TEST-STRATEGY.md lacks test coverage. The following are non-blocking awareness items
carried from Gate 3b, not coverage gaps:

1. **AC-V-SEAM "only two readers" source assertion is structural, not an explicit grep test** (WARN,
   minor). The single-reader invariant holds by construction (`data` and `snapshot_block` are private;
   exactly two public byte-returning methods exist). The #700-reuse test proves the contract positively.
   Optional defense-in-depth: add a grep-style source-assertion test. Does not affect coverage of R-06.
2. **AC-13 `cargo audit` literal "passes" carries 1 pre-existing CVE** (WARN). RUSTSEC-2023-0071
   (rsa 0.9.10 via sqlx-mysql) is a workspace-wide transitive dependency present on `main`, unchanged by
   crt-052, no upstream fix available, and absent from every crt-052-touched chain. The AC-13 gate
   concern — crt-052 introducing a vulnerable dependency — is satisfied (only `regex` was added). Also
   present: RUSTSEC-2025-0141 (bincode unmaintained, via hnsw_rs) — a non-vulnerability advisory, also
   pre-existing. Neither warrants a crt-052 action.

## GH Issues Filed

- **GH#705** — Pre-existing flaky unit tests under full-parallel workspace runs (http::token concurrent
  creation, http::listener semaphore recovery, uds stamp_read). Pre-existing, pass in isolation, not a
  crt-052 regression. Tracking only (Rust unit tests — no infra-001 xfail marker applies).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #3806/#4202/#3386/#4515 (Gate-3b/3c missing-named-test failure modes), #2758 (grep every non-negotiable test name before accepting PASS), #840 (USAGE-PROTOCOL quick ref). Applied #2758 by verifying every merge-gate test by name in isolation.
- Stored: nothing novel to store — the recurring testing patterns (cumulative test infra #238, grep-named-tests #2758/#3253, missing-named-test gate failures) are already captured; crt-052-specific results live in this report. The one cross-feature observation worth watching (a stdio MCP harness cannot populate a UDS-hook-fed in-memory buffer, so internal-only lifecycle proofs must stay in Rust and MCP tests assert only the protocol-observable contract) is a single occurrence here; not yet a 2+-feature lesson per stewardship rules.
</content>
</invoke>
