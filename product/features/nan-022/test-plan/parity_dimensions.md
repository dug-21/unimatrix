# Test Plan: K1 — `harness/parity_dimensions.py`

Covers the registry half of **R-03 (Critical)** and the single-source half of R-05. The single
authoritative enumeration `DIMENSIONS`; every consumer iterates THIS tuple — nothing else
hand-lists the six (SR-05/#5302). Wrong wire-surface routing is the single most dangerous
false-GREEN path (records nothing → vacuous pass); the registry-vs-driver routing consistency
check is the off-Docker guard for it.

Surface under test:
- `Dimension(id, capture_key, wire_surface, comparator, intra_transport_check, blocks_c0_proof)`
  — frozen dataclass
- `DIMENSIONS: tuple[Dimension, ...]` — the SIX
- `WIRE_MCP_BRIDGE`, `WIRE_HOOK_OBSERVE` — `str` constants

Tier: **A (off-Docker unit)** — pure structural assertions over the registry. File:
`suites/test_parity_dimensions.py`.

## Unit Test Expectations

### Enumeration completeness + identity (SR-05 single-source)
- `test_dimensions_enumerates_exactly_six`: `DIMENSIONS` has exactly six rows with ids
  `{retrieval, behavioral, analytics, proactive, precompact, isolation}` — no more, no fewer
  (the C0 #5304 dimension list; broadening is out of scope per Non-Goals).
- `test_dimension_is_frozen_dataclass`: `Dimension` is frozen (immutable) — a consumer cannot
  mutate a registry row at runtime.
- `test_capture_keys_unique`: every `capture_key` is unique (no orphan / collision) — couples to
  the drift guard's capture_key↔schema check in `parity_comparator.md`.

### Wire-surface routing (R-03 scenario 1 — registry side)
- `test_wire_surface_is_one_of_two_constants`: every `Dimension.wire_surface` is exactly
  `WIRE_MCP_BRIDGE` or `WIRE_HOOK_OBSERVE` (no third/typo'd surface).
- `test_wire_surface_assignments_match_architecture`: parametrized — assert each dimension's
  `wire_surface` matches the architecture's routing table (retrieval→mcp_bridge,
  behavioral→hook_observe, analytics→both, proactive→mcp_bridge, precompact→hook_observe,
  isolation→both). A misassignment here would route a capture to the wrong surface (R-03).
- `test_dual_surface_dimensions_declared`: analytics and isolation are flagged as touching BOTH
  surfaces (the fan-out the leg driver must perform — couples to `parity_legs.md` R-03 integration risk).

### Outcome-policy flags
- `test_intra_transport_check_only_retrieval_and_proactive`: `intra_transport_check=True` for
  exactly `retrieval` and `proactive` (the embedding-ranked dims), `False` for the other four.
- `test_blocks_c0_proof_all_six_true`: `blocks_c0_proof is True` for all six (the resolved C0
  #5304 disposition — parity is the total bar; the flag is the data-only escape valve for a
  human-signed documented exception, asserted as the default).

### Comparator binding (couples to drift guard)
- `test_each_dimension_comparator_is_dimension_comparator_subclass`: every `Dimension.comparator`
  is a `DimensionComparator` subclass (a lightweight structural check; the full justified-EXCLUDED
  guard lives in `assert_comparator_contract`, `parity_comparator.md`).

## Registry-vs-driver routing consistency (R-03 scenario 1 — the off-Docker false-GREEN guard)
- `test_registry_routing_matches_driver_behavior`: assert every `Dimension.wire_surface` matches
  the surface the leg driver (`parity_legs.drive_uds_leg`) actually captures for that dimension —
  a registry-vs-driver consistency assertion. A drift between the registry's declared surface and
  the driver's actual capture is the #5298 wrong-surface trap; this off-Docker test catches it
  before the live #5298 byte-identity check (the live half is in `test_https_uds_parity.md`).

## Coverage Requirement
The registry is the single source of truth: exactly six dimensions, unique capture_keys, every
wire_surface one of two constants matching both the architecture routing table and the leg
driver's actual capture, intra-check flags correct, all six `blocks_c0_proof=True`. The
registry-vs-driver routing consistency guard runs off-Docker and is the first line against the
R-03 wrong-surface vacuous-pass trap.
