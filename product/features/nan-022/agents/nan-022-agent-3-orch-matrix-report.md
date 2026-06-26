# nan-022 Agent 3 (ORCH — parity-matrix orchestrator) — Report

## Files created / modified
- MODIFIED `product/test/infra-001/suites/test_https_uds_parity.py` — added the
  `test_https_uds_parity_matrix` live sibling (drive both legs once under one token →
  token-guarded ingest → classify per dimension → evidence table → roll-up assert),
  plus sequencing helpers `_classify_matrix` / `_emit_evidence_records` /
  `_preflight_uds` / `_preflight_https`. nan-021 `test_https_uds_parity` MetricVector
  path UNCHANGED. (476 lines)
- CREATED `product/test/infra-001/harness/parity_matrix_support.py` — evidence-table
  emit (AC-08), roll-up assertion (GREEN/RED/ERROR disposition), distinct INFRA exit,
  and the contract-shaped off-Docker fixture dimension bundle. (247 lines)
- CREATED `product/test/infra-001/suites/test_https_uds_parity_matrix.py` — daemon-free
  ORCH unit tests: fixture-bundle seam round-trip, per-dimension classification wiring,
  evidence-table-keyed-by-token, missing/empty-capture→INFRA, stale-token guard,
  roll-up verdict mapping, source-inspection structural tests, no-seed audit,
  nan-021 cumulative guard. (431 lines)

All files ≤500 lines.

## Tests (off-Docker)
- New matrix + orchestrator daemon-free: **33 passed, 4 deselected** (the live
  integration/parity tests).
- Full parity/transport K-suite regression (dimensions, comparator, outcome, legs,
  workload, ranking_tolerance, transport_health, both parity files):
  **218 passed, 4 deselected — 0 regression.**
- Whole infra-001 suite collects cleanly (630 tests, no import errors from the
  import-surface changes).
- Live `test_https_uds_parity_matrix` confirmed collected under `pytest -m parity` in
  `suites/test_https_uds_parity.py` → the existing release.yml gate (line 612,
  `pytest suites/test_https_uds_parity.py -v -m parity`) picks it up automatically.
  **No new release.yml job added** (C-7 honoured).

## Markers / lane
- Live matrix test marked `@pytest.mark.integration` + `@pytest.mark.parity` → run in
  the release-gate lane, deselected off-Docker. Off-Docker unit tests carry NO such
  marker → run in the daemon-free K-suite.

## Analytics single-source reconciliation (Wave D seam) — NO contract fix needed
Verified end-to-end. The C2' bridge driver (`scripts/bridge-cycle-driver.js`, lines
360–367) OWNS `informs_edges` / `phase_signal` and emits them into the fragment from
the review read. The C5' shell assembler (`scripts/cloud-bundle-lib.sh`
`emit_dimension_bundle`, line 263) reads them from the driver fragment with
`drv.informs_edges || []` / `drv.phase_signal || {}` — a BACKFILL for a driver-emitted
null, NOT a shadow that overrides a real value. So analytics is single-sourced (driver
owns; shell backfills empties only). The orchestrator reads analytics from
`bundle["analytics"]` consistently on BOTH legs (`MetricVectorComparator` reads
`https["informs_edges"]` / `https["phase_signal"]`), and `_capture_dimension`
(`parity_legs_capture.py`) builds the UDS analytics capture from the review/MetricVector
identically. **No double-source. No shell-default shadow of a driver value.** Flagged
here for the seam owner, not silently fixed.

## Issues / adjacent breakage
- None. The committed Wave A–D dependencies (`drive_uds_bundle`, `load_https_bundle`,
  `classify_dimension`, `rollup`, `assert_comparator_contract`, the K1 registry) wired
  together cleanly; the matrix path calls `drive_uds_bundle` (NOT `drive_uds_leg`) per
  the C3' implementer handoff.
- `_preflight_https` is a documented no-op: the orchestrator does not own the
  container's TLS endpoint (the smoke does), so the binding HTTPS health gate stays
  `run_smoke_gate` inside `run_https_leg` (C-2 — no net-new transport/cert path added).
  The UDS preflight (`uds_socket_leg`) is live defense-in-depth. Flagged for Stage 3c.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (decision, topic nan-022) — surfaced
  ADR-001 #5305, ADR-002 #5313, ADR-003 #5307, ADR-007 #5311; (pattern) surfaced #5316
  (ranking floor). Applied the ADR-002 roll-up precedence + ADR-006 D5 disposition.
- Stored: entry #5322 "Parity-matrix roll-up precedence (ERROR>RED) traps the
  always-INFRA D5 fixture in orchestrator tests" via `/uni-store-pattern`
  (edge: Supports → #5313 ADR-002). A real off-Docker-test trap discovered while
  wiring the roll-up: the honest measurable=False D5 fixture is INFRA by design, and
  ERROR > RED, so a naive "assert RED" test mis-fires unless D5 is made measurable.
