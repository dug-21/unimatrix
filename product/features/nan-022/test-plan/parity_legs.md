# Test Plan: C3′ — `harness/parity_legs.py` (extended)

Covers the driver half of **R-03 (Critical, wrong-surface)**, the UDS-leg half of **R-04
(Critical, WAL-flush timing)**, the double-capture fan-out for **R-07**, and the dual-surface
fan-out integration risk. `drive_uds_leg` is extended to return a dimension BUNDLE instead of
one MetricVector; new helpers route each dimension to its correct wire surface and double-
capture the intra-check dimensions. EXTENDS `harness/parity_legs.py`.

Surface under test (new/extended):
- `drive_uds_leg(uds, hook_socket_path, workload, store_dir, *, agent_id, hook_timeout) -> dict`
  — extended to return `{capture_key: capture, ...}` (the dimension bundle)
- new per-dimension wire-surface routing helpers (MCP-bridge vs hook `/observe`)
- double-capture for `intra_transport_check=True` dimensions
- consumed verbatim: `assert_derived_attribution`, `run_https_leg`, `PARITY_PHASE`

Tier: mostly **B (live UDS, `@pytest.mark.integration`)** since the leg drives a live
`daemon_server`; routing-consistency and shape assertions are **A (off-Docker)** where the
driver can be exercised with a stub client.

## Test Expectations

### Bundle shape (R-09 driver side, AC-01)
- `test_drive_uds_leg_returns_full_dimension_bundle` (B): the UDS leg returns a bundle with all
  six capture_keys, each matching the documented capture shape (couples to `parity_bundle_contract.md`).
- `test_drive_uds_leg_intra_dims_carry_capture_2` (B): retrieval + proactive captures include the
  second capture (`capture_2`) from the same drive (R-07 double-capture source).

### Wire-surface routing (R-03 scenario 1 — driver side)
- `test_drive_uds_leg_routes_each_dimension_to_registry_surface` (A/B): each dimension is captured
  via the wire surface its registry row declares — retrieval/proactive via `UnimatrixUdsClient`
  MCP methods, behavioral/precompact via `UnimatrixHookClient` `/observe`, analytics + isolation
  fan out to BOTH. Assert the driver's actual capture surface == the registry `wire_surface`
  (the registry-vs-driver consistency the off-Docker `parity_dimensions.md` test pairs with).
- `test_drive_uds_leg_dual_surface_fanout_complete` (B): analytics (cycle write on `/observe` +
  review read via MCP) and isolation (write `/observe` slug A + read MCP slug B) both perform
  BOTH halves — a missed fan-out captures HALF a dimension (Integration Risk: two-surface fan-out).

### WAL-flush barrier-gated DB reads (R-04 scenarios 1,3 — UDS leg side)
- `test_drive_uds_leg_db_reads_gated_behind_barrier` (B): every DB-reading capture — D2
  observations read, D6 isolation landing read, analytics cycle-events read — is taken ONLY AFTER
  `durability_barrier`/`observe_count` confirms the WAL has flushed (expected observe count
  reached, dir byte-size incl `-wal` settled). Assert the barrier is satisfied before each read.
- `test_drive_uds_leg_uses_shared_barrier_helper` (B/A): the leg uses the ONE shared
  `durability_barrier` helper (R-04 scenario 3 symmetry — the same helper both legs use; pairs
  with the HTTPS-leg barrier in `cloud-cycle-lib.md`).

### #5298 RecordEvent frames on the UDS hook client (R-03 scenario 2 — UDS half)
- `test_drive_uds_leg_emits_11_frame_sequence` (B): for each observe-driven dimension the UDS leg
  emits the byte-identical #5298 11-frame `RecordEvent` sequence via `UnimatrixHookClient`
  (`SessionRegister`→`cycle_start`→`PreToolUse(TaskCreate phase-set)`→per-observe `Pre`+`Post`→
  `cycle_stop`→`SessionClose`); assert NO rework/legacy frame variant
  (`post_tool_use_rework_candidate`, `{"type":"PostToolUse"}`) is emitted. (The cross-leg
  byte-identity vs the HTTPS leg is asserted in `test_https_uds_parity.md`.)
- `test_drive_uds_leg_hook_error_frame_asserts` (B): an error frame from the `/observe` route
  fires the `_assert_hook_ok`/hook-Error-frame assertion (R-03 scenario 4) — not a silent skip.

### Derived attribution consumed verbatim (R-15, AC-03)
- `test_assert_derived_attribution_string_exact` (B): `assert_derived_attribution` reads
  `topic_signal` from per-slug `observations` (barrier-gated) and asserts string-exact `== feature`;
  `unattributed`/NULL HARD-fails — consumed unchanged from nan-021, never seeded (R-15).

## Coverage Requirement
The UDS leg returns the full dimension bundle with correct per-dimension capture shapes; each
dimension is routed to the registry-declared wire surface with complete dual-surface fan-out for
analytics + isolation; every DB-reading capture is barrier-gated behind the ONE shared
`durability_barrier` (R-04); the #5298 11-frame sequence is emitted with no rework/legacy variant
(R-03); double-capture is performed for intra-check dimensions (R-07).
