# Agent Report — nan-022-agent-3-parity-legs (C3' Leg drivers, bundle)

## Scope
Extend C3' `harness/parity_legs.py` (in place, cumulative) — dimension-keyed bundle
driver, per-dimension wire-surface routing, double-capture for intra dims, barrier-gated
DB reads, PreCompact measurability honesty. TEST-ONLY, off-Docker Tier-A.

## Files created / modified
- `product/test/infra-001/harness/parity_legs.py` (modified) — added `drive_uds_bundle`
  (composes the committed `drive_uds_leg`), PHASE-0 corpus seed over MCP, PHASE-3
  per-dimension routing delegation, `DRIVER_WIRE_SURFACES` re-export. `drive_uds_leg`
  kept byte-compatible for the committed source-inspection guards.
- `product/test/infra-001/harness/parity_legs_capture.py` (new) — per-dimension capture
  helpers + `capture_dimension` routing + `DRIVER_WIRE_SURFACES` (≤500-line split).
- `product/test/infra-001/suites/test_parity_legs.py` (new) — 21 off-Docker unit tests.

## Design decisions
- **Sibling `drive_uds_bundle`, `drive_uds_leg` kept verbatim.** The brief headline says
  "extend drive_uds_leg to return the bundle"; the spawn prompt's harder constraints
  ("do NOT break the committed orchestrator", "do NOT modify other components' files",
  "ORCH Wave E adds the sibling matrix path") forced the pseudocode's FIRST-recommended
  option: add `drive_uds_bundle` that COMPOSES `drive_uds_leg` (one cycle, one barrier,
  the returned MetricVector IS the analytics capture). This keeps every committed
  `test_https_uds_parity` source-inspection + MetricVector assertion green WITHOUT
  editing that ORCH-owned file. Wave E wires `drive_uds_bundle` into the matrix path.
- **Capture-routing split** into `parity_legs_capture.py` to honor the ≤500-line rule
  (driver 427 lines, capture 496 lines).
- **PreCompact: measurable=False + named host_side_gap** (ADR-006 / OQ-2). The restored
  CompactContext has a CC host-side component the harness can't drive test-only; the
  /observe frame is still emitted (the gap is named from a real drive), restored_payload
  is the legal null-with-measurable=False. K4 surfaces it as a DOCUMENTED EXCEPTION,
  never a vacuous pass. First-live-drive may flip this to measurable=True.

## Tests
- New `test_parity_legs.py`: **21 passed**.
- Daemon-free K-suite regression (test_parity_dimensions, test_ranking_tolerance,
  test_transport_health, test_parity_comparator, test_parity_outcome, test_parity_workload,
  test_parity_legs, test_https_uds_parity, `-m "not integration and not parity"`):
  **194 passed, 3 deselected** (the 3 deselected need a live daemon — Stage 3c).
- nan-021 backward-compat: the 9 non-integration `test_https_uds_parity` guards stay
  GREEN; `drive_uds_leg` still returns the MetricVector dict and its committed
  inspection tokens are intact (asserted by `test_drive_uds_leg_source_unbroken_*`).
- Live UDS drive + the bundle-contract live half are Tier B/C (Stage 3c) — not run
  off-Docker by design.

## Issues / adjacent breakage flagged
- **None blocking.** No other component's files modified (git status: 1 modified, 2 new,
  all under `product/test/infra-001/`).
- **For Wave E (ORCH):** the matrix orchestrator must call `drive_uds_bundle` (not
  `drive_uds_leg`) and ingest the HTTPS side via `load_https_bundle`. The analytics
  capture's `metric_vector` is the SAME object `drive_uds_leg` returns.
- **For Wave D (C5'/C2' HTTPS leg):** the on-disk capture shapes this driver emits are
  the contract the JS/shell emit must match byte-for-byte (parity_bundle_contract.md).
  Retrieval/briefing result parsing is tolerant of `{"entries":[...]}`/`{"results":[...]}`
  /bare-list shapes with `id` + `score`/`similarity`; the HTTPS emit should populate those.
- **First-live-drive (OQ-2/OQ-4):** retrieval/briefing result-id + score extraction and
  the Informs-edge read shape are best-effort against the server's JSON; confirm the
  actual `context_search`/`context_briefing`/`context_cycle_review` JSON field names at
  first live UDS drive (Stage 3c) — a shape mismatch surfaces as INFRA (empty capture),
  never a vacuous pass, so it fails loud rather than silently.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` / `context_search` — UNAVAILABLE this
  session (ToolSearch returned no matches for the deferred Unimatrix MCP tools; the
  server tools could not be loaded). Proceeded without (non-blocking per protocol); the
  binding ADRs (ADR-001/005/006) and the bundle contract were fully captured from the
  read feature files (architecture/ADR-005, pseudocode/parity_legs.md,
  test-plan/parity_bundle_contract.md), which was sufficient.
- Stored: nothing — Unimatrix store tools were not loadable this session. One pattern
  worth recording when the server is available: "When a committed driver has
  source-inspection guards in another component's test file, do NOT refactor its body to
  extract shared helpers — compose a sibling that CALLS it instead; extracting breaks the
  `inspect.getsource` token asserts without touching behavior." Flagging for the Delivery
  Leader to store via `/uni-store-pattern` (topic: infra-001 / nan-022) if desired.
