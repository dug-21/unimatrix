# ORCH — Parity-matrix orchestrator (`suites/test_https_uds_parity.py` + sibling matrix test)

**Extended in place**, cumulative. ADR-001. Drives both legs once, ingests both bundles
token-guarded, runs every comparator via the K4 classifier, rolls up, emits the per-dimension
evidence table keyed by the run token. The existing `test_https_uds_parity` MetricVector test +
its contract tests stay GREEN (cumulative — do not break the proven path).

## Purpose

The pytest-as-orchestrator entrypoint for the dimension-keyed matrix: ONE invocation, ONE
workload, BOTH transports, six per-dimension verdicts. It owns sequencing (preflight ->
both legs -> token-guarded ingest -> classify -> roll up -> emit table), never a per-leg
assertion (parity is MEASURED cross-leg, not asserted per-leg).

## Consumed verbatim

`default_workload`, `durability_barrier`, `drive_uds_leg` (now bundle-returning), `run_https_leg`,
`assert_derived_attribution`, `PARITY_PHASE`, `UnimatrixUdsClient`, `UnimatrixHookClient`,
`daemon_server` fixture, `write_field_record`, the existing `load_https_vector`-based
`test_https_uds_parity` MetricVector test (UNCHANGED).

## Imports (new)

```
from harness.parity_dimensions import DIMENSIONS
from harness.parity_comparator import assert_comparator_contract
from harness.parity_outcome import classify_dimension, rollup, Outcome
from harness.transport_health import preflight_leg, load_https_bundle, InfraError
```

## The matrix orchestrator (sibling test)

```
@pytest.mark.integration
@pytest.mark.parity
def test_https_uds_parity_matrix(daemon_server, tmp_path):
    """Drive BOTH legs in ONE execution and assert the six-dimension parity matrix (AC-01..08).
    UDS leg in-process -> dimension bundle; HTTPS leg via the smoke shell-out -> dimension bundle;
    classify each dimension (INFRA->INTRA->PARITY); roll up; emit the evidence table. A missing
    leg/capture ERRORS — never a vacuous pass."""

    # ---- 0. drift guard BEFORE any drive (off-Docker discipline, fails fast) ----
    assert_comparator_contract(DIMENSIONS)

    # ---- 1. ONE workload / ONE identity / ONE token ----
    workload = default_workload(); workload.validate()
    run_token = workload.session_id
    store_dir = daemon_server["store_dir"]
    sandbox = tmp_path / "sandbox"; sandbox.mkdir(parents=True, exist_ok=True)
    https_out = sandbox / "https_dimension_bundle.json"
    assert not https_out.exists(), "stale HTTPS out-file present at start (R-12 guard)"
    manifest_path = workload.write_manifest(sandbox / "parity_workload.json")

    # ---- 2. UDS leg (in-process), preflight first (defense-in-depth) ----
    try:
        preflight_leg("uds", connect_deadline_s=..., idle_deadline_s=...)   # K5; InfraError -> INFRA roll-up
    except InfraError as e:
        _emit_infra_and_exit(e, run_token, sandbox)   # distinct ERROR exit, never parity RED

    uds = UnimatrixUdsClient(daemon_server["mcp_socket_path"], timeout=30.0); uds.connect()
    try:
        bundle_uds = drive_uds_leg(uds, daemon_server["socket_path"], workload, store_dir)
    finally:
        uds.disconnect()
    assert_derived_attribution(workload.feature_cycle, store_dir)   # AC-03 (UDS leg, verbatim)

    # ---- 3. HTTPS leg (shell-out), preflight first ----
    try:
        preflight_leg("https", connect_deadline_s=..., idle_deadline_s=...)
        run_https_leg(manifest_path=manifest_path, run_token=run_token, https_out=https_out, sandbox=sandbox)
        bundle_https = load_https_bundle(https_out, run_token)   # token-guarded; missing/stale/null -> InfraError
    except InfraError as e:
        _emit_infra_and_exit(e, run_token, sandbox)

    # ---- 4. classify every dimension (INFRA -> INTRA -> PARITY) ----
    results = []
    for dim in DIMENSIONS:                                   # the SINGLE enumeration (no hand-list)
        r = classify_dimension(dim, bundle_uds[dim.capture_key], bundle_https[dim.capture_key])
        results.append(r)

    # ---- 5. evidence table keyed by run_token + first-live-run records ----
    table = _evidence_table(results, run_token)
    write_field_record(table, sandbox / f"parity_matrix_{run_token}.json")
    for dim, r in zip(DIMENSIONS, results):                  # per-dimension evidence record
        rec = dim.comparator().evidence_record(bundle_https[dim.capture_key], bundle_uds[dim.capture_key], run_token=run_token)
        write_field_record(rec, sandbox / f"evidence_{dim.id}_{run_token}.json")

    # ---- 6. roll up + assert (§4) ----
    verdict, exit_code = rollup(results)
    # GREEN iff every dimension PARITY_PASS. PARITY_FAIL -> RED (file GH bug). INFRA_ERROR ->
    # distinct ERROR. INTRA recorded, does not redden. D5 documented-exception surfaced honestly.
    _assert_rollup(verdict, exit_code, results)              # fails the test loud on RED/ERROR with the table
```

### Disposition on the roll-up (C-4 / C-8 / AC-10)

- `PARITY_FAIL` -> the test FAILS RED; the detail/diffs + the first-live-run evidence record name
  the divergent field; disposition is "file a NEW GH bug, fix NOT absorbed". The implementer/
  tester never widens an exclusion set.
- `INFRA_ERROR` -> the test surfaces a DISTINCT error (not a parity RED); the transport-health /
  ingest detail is shown; re-run/diagnose transport. A D5 documented-exception is reported
  HONESTLY (measured-where-drivable + named gap), never rounded up to fully-measured.
- `INTRA_TRANSPORT_NONDETERMINISM` -> recorded in the table + flagged for a SEPARATE GH bug
  (GH#746); does NOT redden the gate.

## Evidence table

```
_evidence_table(results, run_token):
    return {
      "run_token": run_token,
      "dimensions": [ {dimension, outcome:value, blocks_c0_proof, detail, diffs} per result ],
      "verdict": <GREEN|RED|ERROR>,
      "intra_nondeterminism": [dims classed INTRA],   # routed to GH#746
      "documented_exceptions": [D5 host_side_gap call-outs],   # honest, never vacuous
    }
```

The table IS the C0 proof artifact (AC-12). This feature does NOT flip C0 — an authorized
session reads the table and performs the flip.

## Off-Docker wiring proof (extends the nan-021 #5258 seam test)

Mirror the existing `test_c3_orchestrator_seam_with_fixture_https_vector`: a contract-shaped
FIXTURE `dimension_bundle` round-trips through `load_https_bundle` + every comparator + the
classifier + the roll-up WITHOUT Docker (R-09 sc.3, R-10 sc.1). Proves the seam Stage 3c plugs
the live HTTPS leg into.

## Structural / contract tests (extend the existing nan-021 set)

- One `ParityWorkload` object; `run_token == workload.session_id`; one barrier (R-13 — extend
  `test_c3_same_session_identity_as_https`).
- The orchestrator iterates `DIMENSIONS` (no hand-list of six) — assert by source inspection.
- `assert_comparator_contract(DIMENSIONS)` is called BEFORE any drive (assert by source order).
- A missing HTTPS bundle / stale token -> `InfraError` (extend `test_c3_missing_https_leg_
  errors_never_empty` / `test_c3_seam_rejects_stale_token` to the bundle).
- No seed site reachable from this module (extend `test_c3_no_seed_site_reachable` to include all
  net-new modules + the seed loader).
- The existing MetricVector `test_https_uds_parity` + its contract tests still pass (cumulative).

## Data flow

- INPUT: `default_workload()`, the live `daemon_server`, the HTTPS smoke shell-out env.
- OUTPUT: per-dimension `DimensionResult`s, the evidence table keyed by run_token, the roll-up
  verdict + distinct exit code.

## Error handling

- `InfraError` (preflight or ingest) -> distinct ERROR exit via `_emit_infra_and_exit`, never a
  parity RED, never a hang.
- `ParityMismatch` is caught inside `classify_dimension` -> `PARITY_FAIL` -> the test fails RED
  with the evidence.
- A missing capture_key in either bundle is rejected at ingest (`load_https_bundle`) / by the
  classifier (`bundle_uds[...]` KeyError surfaced as INFRA, never an empty-pass).

## Key test scenarios (hints)

- Full matrix: all six PARITY_PASS -> GREEN (the happy-path C0 proof; live Docker).
- One dimension PARITY_FAIL -> RED, evidence record names the field, exit code = parity-fail
  (R-01/R-07 sc.3 — a real cross-leg divergence on two intra-stable legs).
- One dimension INFRA_ERROR -> distinct ERROR exit, NOT counted as parity RED (R-02 sc.3).
- One dimension INTRA -> recorded, does not redden (R-07 sc.2).
- D5 measurable=False -> documented-exception in the table, not green/pass (R-08 sc.1/4).
- Off-Docker fixture-bundle seam round-trips end-to-end (R-09 sc.3, R-10 sc.1).
- Stale `$HTTPS_VECTOR_OUT` from a prior round -> InfraError on ingest (R-12 sc.1).
- The matrix runs in ONE pytest invocation owning BOTH legs (R-03/D-6 — extend
  `test_c3_runs_in_same_pytest_invocation`).
