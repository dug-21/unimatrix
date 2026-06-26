# C3' — Leg drivers, bundle (`harness/parity_legs.py`)

**Extended in place**, cumulative. ADR-001/005. Consumes the nan-021 leg drivers verbatim;
extends `drive_uds_leg` to return the dimension bundle and adds per-dimension wire-surface
routing + double-capture for intra-check dimensions. No net-new transport/spawn/framing path.

## Purpose

Drive the augmented workload over the UDS transport once and emit the full dimension-keyed
bundle (not a single MetricVector). Route each dimension to its correct wire surface
(`UnimatrixUdsClient` MCP vs `UnimatrixHookClient` hook IPC); fan out the dual-surface
dimensions (analytics, isolation); double-capture the intra-check dimensions (retrieval,
proactive). The barrier-gating discipline is applied identically per the SHARED helper (R-04).

## Consumed verbatim

`PARITY_PHASE`, `assert_derived_attribution`, `run_https_leg`, `_assert_hook_ok`,
`_extract_metric_vector`, the existing hook 11-frame sequence in `drive_uds_leg`,
`durability_barrier`, `UnimatrixUdsClient`, `UnimatrixHookClient`.

## Change 1 — `drive_uds_leg` returns the dimension bundle

Keep the existing nan-021 signature and the existing MetricVector capture; WIDEN the return from
`dict` (MetricVector) to the bundle. To avoid breaking the existing `test_https_uds_parity`
MetricVector test, recommendation: ADD a sibling `drive_uds_bundle(...)` that composes
`drive_uds_leg` (which still returns the MetricVector for the analytics dimension) plus the new
per-dimension captures, OR extend `drive_uds_leg` to return the bundle and update the existing
test to read `bundle["analytics"]["metric_vector"]`. The brief says "extend `drive_uds_leg` to
return the dimension bundle"; follow that, and update the existing MetricVector assertions to
index the bundle (cumulative, one driver).

```
def drive_uds_leg(uds, hook_socket_path, workload, store_dir, *,
                  agent_id="nan-022-uds-leg", hook_timeout=30.0) -> dict:
    """Drive the augmented manifest over local hook IPC + MCP UDS and return a dimension bundle:
       { "retrieval": {...}, "behavioral": {...}, "analytics": {...},
         "proactive": {...}, "precompact": {...}, "isolation": {...} }
    keyed by Dimension.capture_key (iterate DIMENSIONS — no hand-list). ALL captures under the
    ONE stable session id (#832). The barrier gates every DB-reading capture (R-04)."""
    sid = workload.session_id

    # ---- PHASE 0: seed the corpus over MCP (context_store) BEFORE the observe cycle ----
    #   Replay workload.seed_calls via uds.context_store(...) so the store is identically seeded
    #   on both legs (CONTENT only — never a compared output; R-15).

    # ---- PHASE 1: the existing observe cycle (11-frame #5298 sequence, verbatim) ----
    #   SessionRegister -> cycle_start(phase=PARITY_PHASE) -> TaskCreate phase-set ->
    #   per observe Pre+Post -> cycle_stop -> SessionClose.  (analytics WRITES + behavioral)

    # ---- PHASE 2: SYMMETRIC durability barrier (shared helper, leg="UDS") ----
    #   gate BEFORE any DB-reading capture (behavioral topic_signals, isolation landing,
    #   analytics cycle/edges) — R-04. Same helper/predicate/deadline as the HTTPS leg.

    # ---- PHASE 3: per-dimension capture, routed by wire_surface ----
    bundle = {}
    for dim in DIMENSIONS:
        bundle[dim.capture_key] = _capture_dimension(dim, uds, hook_socket_path, workload, store_dir, sid)
    return bundle
```

## Change 2 — per-dimension capture + wire-surface routing

```
def _capture_dimension(dim, uds, hook_socket_path, workload, store_dir, sid) -> dict:
    # Route by dim.wire_surface; fan out dual-surface dims explicitly.
    if dim.id == "retrieval":
        cap  = _capture_retrieval(uds, workload)          # MCP bridge surface (UDS: uds.context_search/lookup/get)
        cap2 = _capture_retrieval(uds, workload)          # SECOND capture (intra double-capture)
        return {"queries": cap, "capture_2": cap2}
    if dim.id == "behavioral":
        return {"topic_signals": _read_topic_signals(store_dir)}   # hook_observe surface; DB read AFTER barrier
    if dim.id == "analytics":
        # DUAL surface: cycle_events written via hook (PHASE 1); review read via MCP.
        mv = _extract_metric_vector(uds.context_cycle_review(workload.feature_cycle, agent_id=..., format="json"), "UDS")
        return {"metric_vector": mv,
                "informs_edges": _read_informs_edges(uds, workload),   # MCP read, barrier-gated
                "phase_signal":  _read_phase_signal(mv)}
    if dim.id == "proactive":
        cap  = _capture_briefing(uds, workload)           # MCP bridge surface
        cap2 = _capture_briefing(uds, workload)           # SECOND capture (intra)
        return {"briefing_ids": cap["ids"], "briefing_scores": cap["scores"],
                "injection_set": cap["injection_set"], "capture_2": cap2}
    if dim.id == "precompact":
        return _capture_precompact(hook_socket_path, workload, sid)    # hook_observe surface; see ADR-006
    if dim.id == "isolation":
        # DUAL surface: write to slug A via hook/MCP, read from slug B; check on-disk landing.
        return _capture_isolation(uds, hook_socket_path, store_dir, sid)
    raise InfraError("uds", f"unrouted dimension {dim.id}")            # never silently skip (R-03)
```

### Capture helpers

```
_capture_retrieval(uds, workload):
    # Issue workload.query_calls (context_search/lookup/get) over MCP UDS; return a list of
    # {"tool","args","result_ids","scores"} — result_ids in RANKED order, scores aligned.
    # A result set shorter than STABLE_PREFIX_FLOOR is a degenerate-corpus condition: return it
    # as-is; K4 _capture_is_empty flags it INFRA-ERROR (R-06), never a vacuous pass here.

_capture_briefing(uds, workload):
    # Issue workload.briefing_calls (context_briefing) over MCP UDS; return
    # {"ids":[...ranked...], "scores":[...], "injection_set":[...]}.

_read_topic_signals(store_dir):
    # Read DISTINCT topic_signal from the per-slug observations table (the assert_derived_
    # attribution read, returned as a set/list) — DERIVED, never seeded. MUST run AFTER the
    # barrier (R-04). Used for cross-leg compare (extends the UDS-only nan-021 assertion to a
    # capture both legs emit).

_read_informs_edges(uds, workload):  # MCP read of the Informs edge-ID set; barrier-gated (R-11)
_read_phase_signal(mv):              # extract the phase signal from the MetricVector/review
_capture_precompact(...):            # see ADR-006 measurability section below
_capture_isolation(...):             # write slug A / read slug B + on-disk landing; booleans
```

### PreCompact capture (ADR-006 / OQ-B, measurability-aware)

```
_capture_precompact(hook_socket_path, workload, sid):
    # Drive the PreCompact /observe frame and capture the SERVER-restored CompactContext payload
    # over the hook IPC (UDS) / /observe (HTTPS) — NOT the MCP bridge.
    # Determine measurability AT FIRST DRIVE (OQ-B): if the restored payload is symmetrically
    # capturable test-only, set measurable=True, restored_payload={...}, host_side_gap=null.
    # If a host-side (CC) component cannot be driven test-only, set measurable=False,
    # restored_payload=null, host_side_gap="<named un-driven portion>".
    # NEVER silently drop, NEVER vacuous pass — the gap is NAMED and surfaced (R-08).
    return {"restored_payload": <{...}|null>, "measurable": <bool>, "host_side_gap": <str|null>}
```

## Routing discipline (R-03 / SR-08)

- Every capture is routed by `dim.wire_surface`; a dual-surface dimension fans out BOTH surfaces
  explicitly. A dimension that records nothing (wrong surface) returns an empty capture which K4
  classifies INFRA-ERROR — NEVER an empty-pass.
- Observe-driven captures (behavioral, precompact, analytics-cycle, isolation-write) ride the
  byte-identical #5298 11-frame RecordEvent sequence (PHASE 1), never the rework/legacy variants.
- The UDS leg is the SOURCE OF TRUTH for the frame shapes; the HTTPS leg (C5') conforms.

## Barrier discipline (R-04)

- The SAME shared `durability_barrier(leg="UDS", ...)` runs AFTER cycle_stop and BEFORE any
  DB-reading capture (behavioral, isolation landing, analytics cycle/edges). A pre-barrier DB
  read is an INFRA-ERROR (barrier-not-satisfied), never PARITY-FAIL or empty-equals-empty.
- The barrier helper/predicate/deadline is IDENTICAL on both legs (one leg cannot be
  checkpoint-gated while the other is not — R-04 sc.3).

## Data flow

- INPUT: the augmented `ParityWorkload`, the UDS/hook clients, the per-slug store dir.
- OUTPUT: the dimension bundle dict keyed by `capture_key`, consumed by the orchestrator and
  classified per dimension by K4.

## Error handling

- An unrouted dimension -> `InfraError` (never silently skipped — R-03).
- `DurabilityTimeout` from the barrier -> HARD failure (existing nan-021 behavior).
- A hook Error frame -> `_assert_hook_ok` fails loud (verbatim).
- A missing capture surfaces as an empty/null entry -> K4 INFRA-ERROR (never empty-pass).

## Key test scenarios (hints)

- `drive_uds_leg` returns a bundle with EVERY `Dimension.capture_key` present (iterates
  DIMENSIONS, no hand-list).
- Every `dim.wire_surface` matches the capture the driver actually performs (registry-vs-driver
  consistency, R-03 sc.1 — off-Docker structural).
- Intra-check dimensions (retrieval, proactive) emit BOTH `capture` and `capture_2`; non-intra
  dimensions do not (off-Docker structural).
- Dual-surface dimensions (analytics, isolation) fan out BOTH surfaces (no half-capture, R-03).
- The barrier runs AFTER cycle_stop and BEFORE every DB-reading capture (R-04 sc.1 live;
  ordering asserted off-Docker by source inspection as in the existing nan-021 test).
- A forced wrong-surface capture -> empty -> INFRA-ERROR via the never-empty guard, NEVER
  PARITY-PASS (R-03 sc.3 fault injection).
- PreCompact capture carries `measurable`/`host_side_gap`; a `measurable=False` is a named gap,
  never a silent pass (R-08 sc.3).
- The existing `assert_derived_attribution` still holds on the UDS leg (AC-03 cumulative).
- No seed site reachable from this module (extends the nan-021 `test_c3_no_seed_site_reachable`).
