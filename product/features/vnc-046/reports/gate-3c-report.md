# Gate 3c Report: vnc-046

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-07-08
> Result: PASS
> Validator agent: vnc-046-gate-3c
> Validated FROM ARTIFACTS (RISK-COVERAGE-REPORT, feature diff vs main, gate-3a/3b reports,
> GH #934 posted Stage 3c results). Per spawn instruction, `cargo test --workspace` and the
> full pytest suites were NOT re-run (green on committed HEAD; re-running risks a sandbox
> SIGTERM-reap that would hang the gate with no verdict).

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS (1 WARN) | R-01…R-16 mapped to passing tests. R-01/R-02/R-03/R-14 fully proven. AC-07 behavioral-parity leg + OQ-2 live `signal_class_counts>0` deferred to infra-003 Docker (structurally wired + boot-asserted in-process) — WARN, not blocker |
| 2. Test coverage completeness | PASS (1 WARN) | Every Phase-2 risk exercised; R-16 shares the AC-07 wire deferral |
| 3. Specification compliance | PASS (1 WARN) | FR-1…13 delivered; AC-01/02/03/04/05/06/08/09/10 verified; AC-07 diff-review leg PASS, behavioral-parity leg deferred to wire |
| 4. Architecture compliance | PASS | ADR-001…005 honored (funnel completeness, construction parity, real boot assertion, no side-map, #925 NOT subsumed) — confirmed Gate 3b |
| 5. Knowledge stewardship | PASS | Tester report carries `## Knowledge Stewardship` with Queried + Stored (#5641) |
| INT: smoke (`-m smoke`) | PASS | 35 passed / 0 failed (committed-HEAD foreground re-run) |
| INT: #800 multi-slug HTTPS isolation | PASS | Suite BUILT (extended infra-001, not forked) + 4 passed / 0 failed over real HTTPS |
| INT: coverage-report integration counts + AC-06 table | PASS | RISK-COVERAGE-REPORT carries integration counts + the AC-06 coverage-enumeration table |
| INT: xfail hygiene | PASS | Zero xfail markers added; none needed (no pre-existing integration failure encountered) |
| INT: no integration tests deleted/commented | PASS | Diff shows only AC-09 vestigial-param removals in existing listener test macros — no test deleted or disabled |

Checks: 10 passed / 10 (3 WARN residuals, all the same deferred wire leg)
GH #934 Stage 3c comment posted; matches RISK-COVERAGE-REPORT numbers.

## Detailed Findings

### Check 1 — Risk mitigation proof
**Status**: PASS (1 WARN)
**Evidence**: RISK-COVERAGE-REPORT maps every R-01…R-16 to a named test with a result. The four
"especially" risks are directly verified in the diff:

- **R-01 (one-directional false-GREEN)** — FULL. Behavioral suite is bidirectional AND carries a
  live negative control. Verified in `tests/project_routing_integration.rs`:
  `test_observe_transcript_fidelity_a`/`_b`, `test_observe_isolation_identical_cycle_a_driver`/
  `_b_driver`, and `test_observe_negative_control_predicate_is_sensitive` (present + passing). The
  negative control is non-vacuous (detects where the write landed + an injected foreign marker); the
  real reverse (B→A) direction is additionally exercised over the wire by #800
  `test_observe_transcript_isolation_a/b_driver`. A true resolver-level reverse mis-wire is not
  constructible from the external crate (`StoreResolver` names the `pub(crate)` `McpAdapter`), so
  the sensitivity check is the reachable equivalent — acceptable.
- **R-02 (assembled-wiring-only, no hand-passed registry)** — FULL. Confirmed by direct grep of the
  behavioral crate diff: every `Arc::ptr_eq` occurrence is confined to the clearly-separated
  `mod vnc046_white_box_wiring_pins` (lines 453–493); the behavioral `test_observe_*`/
  `test_knowledge_*`/`test_config_*` fns contain no `ptr_eq`, no `dispatch_request(registry=…)`
  hand-pass, no field-overwrite. AC-06 clean.
- **R-14 (500-not-404)** — FULL. `test_post_store_star_for_err_maps_to_500_not_404` (binary crate,
  Stage 3b verified).
- **R-03 (real boot assertion, not debug_assert)** — FULL. `main_boot_assertion_tests.rs` (275 lines
  in diff); `assert_per_slug_isolation` returns `Result<(),ServerError>` and is `?`-propagated
  (Gate 3b §3).
- **OQ-2 (non-zero `signal_class_counts`)** — WARN. The hollow-counts #930 symptom is prevented
  structurally: the per-slug signature scanner is wired non-empty (compiled from
  `r.transcript_signals`, Stage 3b + pattern #5638), the construction-parity pins assert the field
  is non-default (`signal_class_names==["alpha_signal"]`, `max_content_bytes==12345`), and the boot
  assertion aborts a slug that declares signals but leaves `transcript_signal_class_names` empty
  (`test_assert_per_slug_isolation_unset_config_sentinels_return_err`). The LIVE behavioral proof
  (count>0 through `cycle_review`) is deferred to the infra-003 Docker gate because `cycle_review` is
  `pub(crate)` + embedding/serving-bound and did not persist reliably under local cold-warmup. The
  team deliberately did NOT ship a flaky local check (anti-fake-green #4452). Deferral is declared
  with a named vehicle — WARN, not a gap.

**WARN**: AC-07 behavioral-parity + OQ-2 live count>0 rest on the infra-003 Docker gate. Recommend
running it in the pre-merge CI lane for the live proof. The isolation invariants themselves — the
feature's core deliverable — are fully proven in-process and over real HTTPS.

### Check 2 — Test coverage completeness
**Status**: PASS (1 WARN)
**Evidence**: All 16 Phase-2 risks carry a mapping and result in the coverage report. Cross-component
integration risks are exercised by the 9 behavioral tests (assembled `PathRouter → route_observe →
resolver.*_for → dispatch_request` edge, N=2, bidirectional) plus the #800 real-HTTPS suite. Edge
cases from the risk analysis (identical `{phase}-{NNN}` collision, unknown-slug 404, zero-delta
own-fold, config declared-vs-not) are covered by INV-T2 tests + #800 unknown-slug-404. R-16
(UDS==HTTPS parity flake) shares the AC-07 wire deferral (WARN).

### Check 3 — Specification compliance
**Status**: PASS (1 WARN)
**Evidence**: FR-1…13 delivered (Gate 3b §1–3). Acceptance criteria: AC-01, AC-02, AC-03, AC-04,
AC-05, AC-06, AC-08, AC-09, AC-10 verified (RISK-COVERAGE-REPORT "Acceptance Criteria Verification"
table + diff). **AC-07 is the single partially-verified criterion**: its diff-review leg PASSES
(UDS/stdio construction unchanged — `listener.rs` net −178 lines, no change to local paths, Gate 3b
§2), while its behavioral HTTPS==UDS fold-parity leg is deferred to the infra-003 Docker gate (same
`cycle_review` reachability boundary as OQ-2). Consistent with the Gate 3a adjudication (WARN-2) and
Gate 3b carried-item (b), both of which accepted this split. `store_config`/`inference_config` are
the declared AC-06 white-box exceptions (wired + boot-asserted + census-forced; behavioral proof
legitimately absent for lack of a public surface) — named, never silently omitted.

### Check 4 — Architecture compliance
**Status**: PASS
**Evidence**: Gate 3b confirmed ADR-001 (no parallel side-map; three no-default trait methods),
ADR-002 (full construction parity incl. the ratified per-slug scanner triple), ADR-003 (real
Result-returning boot assertion + exhaustive no-`..` census, compiled into release), ADR-004
(bidirectional N≥2 behavioral seam), ADR-005 (#925 NOT subsumed — reconfirmed in RISK-COVERAGE-REPORT
"#925 Reconciliation" and the #934 comment; PR must state the distinction). No architectural drift.

### Check 5 — Knowledge stewardship
**Status**: PASS
**Evidence**: `agents/vnc-046-agent-4-tester-report.md` carries a `## Knowledge Stewardship` block —
Queried (`context_briefing` vnc-046 Stage 3c + `context_get(5637)`, governing isolation patterns
#5348/#5347/#5172/#5427/#5285 applied) and Stored (entry #5641 "Prove per-slug HTTP isolation via the
pub PathRouter edge + durable store read" via `/uni-store-pattern`, a genuinely novel test-infra
technique). The Gate 3a bookkeeping note (tester's separate report file) is now satisfied.

### Integration test validation (mandatory)
**Status**: PASS
- **Smoke (`-m smoke`)**: 35 passed / 0 failed on committed HEAD (Delivery-Leader foreground re-run;
  #934 comment + RISK-COVERAGE-REPORT line 71). (The tester report's stale "32 / PENDING" is an
  earlier run superseded by the committed-HEAD re-run — reconciled, not a discrepancy.)
- **#800 multi-slug HTTPS isolation**: `harness/multi_slug_client.py` (NEW, 240 lines) +
  `suites/test_project_isolation.py` (NEW, 93 lines) BUILT by extending infra-001 (SR-08 — not
  forked); 4 passed / 0 failed over real HTTPS (INV-T2 a/b driver + 2×2 matrix + unknown-slug-404,
  bidirectional).
- **Coverage report contents**: RISK-COVERAGE-REPORT carries the two-vehicle integration table
  (lines 6–9), integration counts (lines 70–74), and the AC-06 coverage-enumeration table
  (lines 86–98) mirroring `test_vnc046_coverage_enumeration`.
- **xfail hygiene**: zero `@pytest.mark.xfail` added; none needed. Confirmed in report + #934.
- **No integration tests deleted/commented**: diff audited — the only edits to existing test files
  (`uds/listener/tests/stamp_read.rs`, `transcript.rs`, −2 lines each) are AC-09 vestigial-param
  removals (`&vs`, `&adapt` dropped from a `dispatch!` macro), not test deletion or disabling.
- **SIGTERM-reaped non-smoke regression — assessment**: NOT a genuine feature gap. The feature's own
  integration surface (per-slug HTTPS isolation) is exercised end-to-end by the #800 suite (4/0, real
  HTTPS, bidirectional) + the 30 behavioral tests + the full-workspace `cargo test` (6982/0). The
  non-smoke suites are broad pre-existing regression, not feature-specific; their non-completion is a
  documented sandbox env limit (rc=143), with a named post-PR CI/infra-003 recommendation. Adequate
  integration coverage for THIS feature.
- **Two pre-existing `verbosity.rs` clippy warnings**: filed as #935; untouched by vnc-046 — not a
  feature defect.

## Rework Required
None. The three WARN residuals (AC-07 behavioral-parity leg, OQ-2 live `signal_class_counts>0`,
R-16 UDS==HTTPS parity) are the single `cycle_review`-reachability deferral to the infra-003 Docker
gate — structurally wired + boot-asserted in-process, deliberately not faked locally, and already
accepted by Gates 3a and 3b. They are not agent-fixable rework; they are a pre-merge CI action.

## Recommendations (non-blocking, for the human / PR)
1. Run the infra-003 Docker multi-tenant-isolation gate in the pre-merge CI lane for the live
   AC-07 / OQ-2 proof.
2. Decide on the two pre-existing `verbosity.rs` `-D warnings` blockers (#935) — they block
   `-D warnings` on `main` too; not this feature's defect.
3. PR must state the ADR-005 #925 plane distinction so no reviewer closes #925 as "subsumed"; leave
   the #930 close decision to the human.
