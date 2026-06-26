# Test Plan: ORCH — `suites/test_https_uds_parity.py` (+ sibling matrix test, extended)

The live parity-matrix orchestrator: drives BOTH legs ONCE under one identity/token, ingests
both bundles token-guarded, runs every dimension comparator, classifies (INFRA→INTRA→PARITY),
and emits the per-dimension evidence table keyed by run token. This is where the LIVE proving
load lands: **R-03** (#5298 byte-identity), **R-04** (WAL-flush barrier ordering), R-09 (live
JS-emit bundle), R-08 (PreCompact measurability determination), R-10/R-12 (skip HARD-fail +
run-marker), and the full roll-up. EXTENDS the existing nan-021 `test_https_uds_parity.py`
(reusing `daemon_server`, `@pytest.mark.integration`/`@pytest.mark.parity`, the shell-out seam).

Tier: **B (live UDS, `@pytest.mark.integration`)** for the in-process bundle drive + barrier;
**C (live cross-leg, `@pytest.mark.parity`)** for the matrix orchestrator that shells out to the
Docker smoke (Stage 3c / release-gate lane). SKIPs absent the Docker smoke env — the off-Docker
seam proof (below) covers the wiring when SKIPped (the nan-021 #5258 precedent).

## Test Expectations

### Off-Docker seam proof (R-10 scenario 1 — wiring without Docker)
- `test_matrix_orchestrator_seam_with_fixture_bundle` (A): drive the orchestrator's ingest +
  classify + roll-up over a FIXTURE HTTPS bundle (the golden schema fixture from
  `parity_bundle_contract.md`) — proves the orchestrator wiring (ingest → classify → table →
  roll-up) without Docker, BEFORE any tag round (#5258). Covers the path when the live test SKIPs.

### One execution, one identity/token (AC-01, R-13)
- `test_matrix_drives_both_legs_one_execution` (C): both legs are driven in ONE pytest invocation
  under `run_token == workload.session_id`; assert no stale `$HTTPS_VECTOR_OUT` at start (R-12).
- `test_matrix_both_legs_replay_same_manifest` (C): both legs replay the SAME augmented manifest
  (R-13 live half).

### #5298 11-frame byte-identity on BOTH legs (R-03 scenario 2 — the live false-GREEN guard)
- `test_matrix_5298_frames_byte_identical_both_legs` (C): for EACH observe-driven dimension
  (behavioral, precompact, analytics-cycle, isolation-write) the byte-identical #5298 11-frame
  `RecordEvent` sequence is emitted on BOTH legs (via the wire-witness); assert NO rework/legacy
  frame variant appears on either leg. **The live half of the R-03 wrong-surface guard.**
- `test_matrix_wrong_surface_fault_injection_is_infra` (C, negative): force a dimension's capture
  to the wrong wire surface (fault injection) → the capture is empty → the never-empty guard
  raises INFRA-ERROR, NEVER PARITY-PASS (R-03 scenario 3). **Load-bearing vacuous-pass guard.**

### WAL-flush barrier ordering (R-04 scenarios 1–4 — the live false-RED/GREEN guard)
- `test_matrix_db_reads_barrier_gated_both_legs` (C): every DB-reading capture (D2 observations,
  D6 isolation landing, D3 analytics cycle-events + Informs edges) is taken AFTER the symmetric
  `durability_barrier` is satisfied on that leg (expected observe count reached, dir byte-size
  incl `-wal` settled). R-04 scenario 1.
- `test_matrix_pre_barrier_read_is_infra` (C/A, negative): a DB read forced BEFORE the barrier →
  empty/partial capture → MUST classify INFRA-ERROR (barrier-not-satisfied), NEVER PARITY-FAIL and
  NEVER an empty-equals-empty pass (R-04 scenario 2). The most dangerous WAL false-pass.
- `test_matrix_barrier_same_helper_both_legs` (C): the barrier is the SAME helper on both legs —
  one leg cannot be checkpoint-gated while the other is not (R-04 scenario 3).

### Per-dimension live parity (AC-02..AC-07)
- `test_matrix_retrieval_stable_prefix_parity` (C, AC-02): stable-prefix ordered-set compare
  cross-leg; result set ≥ N on both legs before the comparator runs, else INFRA (degenerate guard,
  R-06 scenario 2); double-capture classifies an HNSW/tie flip as INTRA-NONDET (not RED).
- `test_matrix_behavioral_topic_signal_string_exact` (C, AC-03): symmetric `topic_signal` read
  (barrier-gated) string-exact cross-leg; `unattributed`/NULL HARD-fails.
- `test_matrix_analytics_metric_vector_plus_edges_phase` (C, AC-04): MetricVector via the consumed
  comparator + Informs-edge SET (IDs exact, wall-clock fields excluded) + exact phase, all
  barrier-gated; pre-barrier edge/phase compare → INFRA (R-11 scenario 1).
- `test_matrix_proactive_briefing_stable_prefix_plus_injection` (C, AC-05): stable-prefix briefing
  compare + injection-set equality; double-capture intra-stability.
- `test_matrix_precompact_measurability_determination` (C/manual, AC-06, R-08): determine at first
  live drive whether `CompactContext` `/observe` frames are symmetrically capturable from both
  legs. If only partially: `measurable=False` + a NAMED `host_side_gap` is recorded as a DOCUMENTED
  measurability call-out in the evidence table — the dimension does NOT pass on the un-driven
  portion (R-08 scenarios 1,3). Resolve OQ-B here and state it plainly for the flip session.
- `test_matrix_isolation_boolean_exact_both_transports` (C, AC-07, security): per-slug isolation
  probe per transport (write slug A `/observe`, read-back slug B MCP, on-disk landing in
  `/data/.unimatrix/<slug>/` not the hash dir, DB read barrier-gated); cross-transport equality of
  the isolation boolean compared EXACTLY (no tolerance, NFR-6). A missing probe → INFRA, never an
  assumed-isolated pass (Security Risks — a false-GREEN here masks a cross-tenant leak).

### Intra-transport classification live (R-07)
- `test_matrix_intra_nondet_routed_out_of_red_gate` (C): an intra-unstable leg (HNSW flip in the
  double-capture) is classed INTRA-NONDET, recorded + annotated against GH#746, and does NOT
  redden the gate; cross-leg compare runs ONLY when both legs are intra-stable.

### Evidence table + roll-up + skip HARD-fail (AC-08, AC-10, R-10, R-12)
- `test_matrix_emits_per_dimension_evidence_table_keyed_by_run_token` (C): the orchestrator emits
  a per-dimension PASS/FAIL table keyed by the run token (the C0 proof artifact, AC-12) and the
  anchored run-marker tied to the run token is present this run (R-12).
- `test_matrix_skip_when_docker_absent_hard_fails` (C/shell, AC-08): the skip-when-Docker-absent
  path HARD-fails by the DISTINCT exit code (false-green-proof), not a silent green.
- `test_matrix_parity_fail_emits_evidence_record_and_stays_red` (C, AC-10): a real two-intra-stable
  cross-leg divergence → PARITY-FAIL → gate RED + first-live-run field-by-field evidence record;
  disposition is "file a NEW GH bug, do NOT fix here" (verified by review of the failure-handling
  path + NO fix code in the diff). The roll-up never absorbs the fix.

### Pre-tag local Docker exercise (R-10 scenario 2 — process, not a single test)
- The FULL matrix is driven against the LOCAL Docker HTTPS fixture (not the release tag) BEFORE
  any release tag, so the live layers (#5298 byte-identity, cross-language bundle emit, bridge
  carriage, barrier ordering) surface BEFORE the release round. Budget multiple tag rounds; treat
  sequentially-revealed live failures as new layers, not regressions (#5267).

## Coverage Requirement
The matrix drives both legs in one execution under one identity/token; #5298 11-frame byte-
identity is proven on both legs with no rework/legacy variant and a wrong-surface fault injection
is INFRA (R-03); every DB read is barrier-gated symmetrically and a pre-barrier read is INFRA
(R-04); each dimension's live parity is asserted per its comparator (D1/D4 stable-prefix, D2/D6/
phase exact); INTRA-NONDET is routed out of the red gate; the evidence table is emitted keyed by
run token with the anchored run-marker (R-12); skip-when-Docker-absent HARD-fails (AC-08); a real
PARITY-FAIL stays RED + emits the evidence record + is filed as a GH bug, fix NOT absorbed (AC-10).
The full live matrix is exercised on the local Docker fixture before any release tag (R-10).
