"""Off-Docker unit tests for C3' — `harness/parity_legs.py` (+ `parity_legs_capture.py`).

Covers the DRIVER half of the nan-022 parity matrix (#837): the dimension-bundle
assembly, per-dimension WIRE-SURFACE routing (registry-vs-driver consistency, R-03),
double-capture for the intra-check dimensions (R-07), the dual-surface fan-out
(analytics, isolation), the barrier-gating discipline for DB reads (R-04), the
wrong-surface → missing-capture → INFRA classification (C-9 / R-03), and the
PreCompact measurability-honesty (ADR-006 / R-08). Plus the nan-021 backward-compat
guards (the committed `drive_uds_leg` MetricVector path is untouched).

Tier A (off-Docker, daemon-free): the bundle driver is exercised with FAKE UDS/hook
clients (no live `daemon_server`). The live UDS drive is Tier B (Stage 3c). These tests
DO NOT mark `@pytest.mark.integration` — they run in the daemon-free K-suite.
"""

from __future__ import annotations

import inspect
import json

import pytest

from harness import parity_legs, parity_legs_capture as cap
from harness.parity_dimensions import (
    DIMENSIONS,
    WIRE_HOOK_OBSERVE,
    WIRE_MCP_BRIDGE,
    dimension_by_id,
)
from harness.parity_legs import (
    DRIVER_WIRE_SURFACES,
    assert_derived_attribution,
    drive_uds_bundle,
    drive_uds_leg,
)
from harness.parity_outcome import Outcome, classify_dimension
from harness.parity_workload import default_workload
from harness.transport_health import InfraError


# ===========================================================================
# Fakes — daemon-free stand-ins recording the surface each capture went over.
# ===========================================================================


class _FakeHookResponse:
    def __init__(self, raw=None):
        self.raw = raw or {"type": "Ok"}


class FakeHookClient:
    """Records every hook frame (the WIRE_HOOK_OBSERVE surface). One instance is
    shared across the per-connection `with` blocks via a parent registry so the test
    can inspect the full frame sequence after the drive."""

    def __init__(self, registry, *, error_on=None):
        self._registry = registry
        self._error_on = error_on or set()

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def _record(self, label, payload):
        self._registry.append((label, payload))
        if label in self._error_on:
            return _FakeHookResponse({"type": "Error", "code": 1, "message": "boom"})
        return _FakeHookResponse()

    def session_register(self, sid, *, agent_role=None, feature=None, **k):
        return self._record("SessionRegister", {"sid": sid, "feature": feature})

    def record_cycle_start(self, sid, fc, *, phase=None, **k):
        return self._record("cycle_start", {"sid": sid, "phase": phase})

    def record_cycle_stop(self, sid, fc, **k):
        return self._record("cycle_stop", {"sid": sid})

    def session_close(self, sid, **k):
        return self._record("SessionClose", {"sid": sid})

    def record_pre_tool_use(self, sid, tool, *, tool_input=None, **k):
        return self._record("PreToolUse", {"tool": tool})

    def record_post_tool_use(self, sid, tool, **k):
        return self._record("PostToolUse", {"tool": tool})

    def record_event(self, sid, event_type, payload, **k):
        return self._record(f"RecordEvent:{event_type}", payload)


class FakeUdsClient:
    """Records every MCP call (the WIRE_MCP_BRIDGE surface) and returns canned JSON
    results so the bundle assembles with non-degenerate captures."""

    def __init__(self):
        self.calls: list[str] = []
        # A deep-enough corpus so retrieval/briefing clear STABLE_PREFIX_FLOOR (3).
        self._entries = [
            {"id": i, "score": 1.0 - i * 0.01} for i in range(1, 6)
        ]

    def _text_result(self, doc: dict) -> dict:
        return {"content": [{"type": "text", "text": json.dumps(doc)}], "isError": False}

    def context_store(self, content, topic, category, **k):
        self.calls.append("context_store")
        return self._text_result({"id": 99})

    def context_search(self, query, **k):
        self.calls.append("context_search")
        # The isolation slug-B probe searches an isolation marker — return EMPTY so the
        # cross-slug read shows no leak (the healthy isolation case).
        if "isolation-marker" in (query or ""):
            return self._text_result({"entries": []})
        return self._text_result({"entries": self._entries})

    def context_lookup(self, **k):
        self.calls.append("context_lookup")
        return self._text_result({"entries": self._entries})

    def context_get(self, entry_id, **k):
        self.calls.append("context_get")
        return self._text_result({"entries": [self._entries[0]]})

    def context_briefing(self, role, task, **k):
        self.calls.append("context_briefing")
        return self._text_result(
            {"entries": self._entries, "injection_set": [1, 2, 3]}
        )

    def context_cycle_review(self, feature_cycle, **k):
        self.calls.append("context_cycle_review")
        return self._text_result(
            {
                "metrics": {
                    "universal": {"total_tool_calls": 3},
                    "phases": {"delivery": {"tool_call_count": 3}},
                    "domain_metrics": {},
                },
                "informs_edges": [{"id": 10}, {"id": 11}],
            }
        )


# ===========================================================================
# Test fixtures: a fake-driven bundle (monkeypatched clients + DB reads).
# ===========================================================================


@pytest.fixture
def fake_bundle(monkeypatch, tmp_path):
    """Drive `drive_uds_bundle` with fake clients off-Docker and return
    ``(bundle, uds, hook_frames)``. Patches the hook client constructor, the barrier
    (no real store dir), and the DB-reading captures (behavioral/isolation)."""
    workload = default_workload()
    hook_frames: list = []

    def _fake_hook_ctor(path, timeout=30.0):
        return FakeHookClient(hook_frames)

    monkeypatch.setattr(parity_legs, "UnimatrixHookClient", _fake_hook_ctor)
    monkeypatch.setattr(cap, "UnimatrixHookClient", _fake_hook_ctor)
    # Barrier is a no-op off-Docker (no real WAL); the ordering is asserted by source.
    monkeypatch.setattr(parity_legs, "durability_barrier", lambda **k: 1)
    # DB-reading captures: behavioral topic_signals + isolation landing read the
    # per-slug sqlite db, absent off-Docker. Patch them to deterministic values so the
    # bundle assembles (the live DB read is Tier B / Stage 3c).
    monkeypatch.setattr(
        cap, "read_topic_signals", lambda store_dir: [workload.feature_cycle]
    )
    monkeypatch.setattr(cap, "_marker_present_in_db", lambda store_dir, fc: True)

    uds = FakeUdsClient()
    bundle = drive_uds_bundle(uds, tmp_path / "hook.sock", workload, tmp_path / "store")
    return bundle, uds, hook_frames


# ===========================================================================
# Bundle shape (R-09 driver side, AC-01)
# ===========================================================================


def test_drive_uds_bundle_returns_full_dimension_bundle(fake_bundle):
    """The UDS bundle driver returns a bundle with EVERY registry capture_key, each
    matching the documented capture shape (iterates DIMENSIONS — no hand-list)."""
    bundle, _, _ = fake_bundle
    assert set(bundle) == {d.capture_key for d in DIMENSIONS}
    # Documented sub-keys per dimension (couples to parity_bundle_contract.md).
    assert {"queries", "capture_2"} <= set(bundle["retrieval"])
    assert "topic_signals" in bundle["behavioral"]
    assert {"metric_vector", "informs_edges", "phase_signal"} <= set(bundle["analytics"])
    assert {"briefing_ids", "briefing_scores", "injection_set", "capture_2"} <= set(
        bundle["proactive"]
    )
    assert {"restored_payload", "measurable", "host_side_gap"} <= set(
        bundle["precompact"]
    )
    assert {"slug_a_writes_visible_to_b", "landed_only_in_a"} <= set(bundle["isolation"])


def test_drive_uds_bundle_keys_iterate_registry_not_handlist(fake_bundle):
    """The bundle keys EXACTLY equal the registry capture_keys (no orphan, no missing
    dimension) — the driver iterates DIMENSIONS, never a hand-list."""
    bundle, _, _ = fake_bundle
    from harness.parity_dimensions import capture_keys

    assert sorted(bundle) == sorted(capture_keys())


# ===========================================================================
# Intra-check dimensions carry capture_2 (R-07 double-capture source)
# ===========================================================================


def test_intra_dims_carry_capture_2(fake_bundle):
    """retrieval + proactive (intra_transport_check=True) carry the second capture;
    non-intra dimensions do not."""
    bundle, _, _ = fake_bundle
    assert bundle["retrieval"]["capture_2"] is not None
    assert bundle["proactive"]["capture_2"] is not None
    # Non-intra dims have no capture_2 field.
    for non_intra in ("behavioral", "analytics", "precompact", "isolation"):
        assert "capture_2" not in bundle[non_intra]


def test_intra_dims_double_capture_invoked(fake_bundle):
    """retrieval is double-captured: the retrieval query set is issued TWICE over MCP
    (two captures from the same drive — the per-leg intra source K4 diffs)."""
    _, uds, _ = fake_bundle
    workload = default_workload()
    n_retrieval = len(workload.retrieval_calls)
    # context_search/lookup/get fired once per retrieval call PER capture (x2), plus
    # the isolation slug-B probe (one context_search). Assert at least 2x the set.
    retrieval_tool_calls = sum(
        uds.calls.count(t)
        for t in ("context_search", "context_lookup", "context_get")
    )
    assert retrieval_tool_calls >= 2 * n_retrieval


def test_registry_marks_exactly_retrieval_and_proactive_intra():
    """Only retrieval + proactive are intra-check (the embedding-ranked dims) — the
    flag the double-capture keys off (single-sourced in the registry)."""
    intra = {d.id for d in DIMENSIONS if d.intra_transport_check}
    assert intra == {"retrieval", "proactive"}


# ===========================================================================
# Wire-surface routing (R-03 scenario 1 — driver side)
# ===========================================================================


def test_driver_wire_surfaces_match_registry_primary_surface():
    """Each dimension's PRIMARY wire surface in DRIVER_WIRE_SURFACES contains the
    registry `wire_surface` — the registry-vs-driver consistency check (R-03 sc.1).
    Single-sourced: DRIVER_WIRE_SURFACES keys == registry ids, no hand-list."""
    assert set(DRIVER_WIRE_SURFACES) == {d.id for d in DIMENSIONS}
    for dim in DIMENSIONS:
        surfaces = DRIVER_WIRE_SURFACES[dim.id]
        assert dim.wire_surface in surfaces, (
            f"{dim.id}: registry wire_surface {dim.wire_surface!r} not among the "
            f"driver's capture surfaces {sorted(surfaces)}"
        )


def test_dual_surface_dimensions_touch_both_surfaces():
    """analytics + isolation fan out BOTH wire surfaces (a missed fan-out captures
    HALF a dimension — the two-surface fan-out integration risk)."""
    both = {WIRE_MCP_BRIDGE, WIRE_HOOK_OBSERVE}
    assert DRIVER_WIRE_SURFACES["analytics"] == both
    assert DRIVER_WIRE_SURFACES["isolation"] == both
    # Single-surface dims touch exactly one.
    for single in ("retrieval", "behavioral", "proactive", "precompact"):
        assert len(DRIVER_WIRE_SURFACES[single]) == 1


def test_dual_surface_fanout_complete(fake_bundle):
    """analytics fans out: cycle frames on the hook surface (PHASE 1) + review read +
    informs edges over MCP. isolation fans out: slug-A on-disk landing + slug-B MCP
    probe. Both perform BOTH halves (no half-capture, R-03)."""
    bundle, uds, frames = fake_bundle
    # analytics MCP half: review read happened over MCP; cycle frames over hook.
    assert "context_cycle_review" in uds.calls
    assert any(lbl == "cycle_start" for lbl, _ in frames)
    assert bundle["analytics"]["informs_edges"] == [10, 11]
    # isolation: slug-B probe over MCP + slug-A landing (booleans both present).
    assert bundle["isolation"]["slug_a_writes_visible_to_b"] is False
    assert bundle["isolation"]["landed_only_in_a"] is True


def test_wrong_surface_capture_empty_classifies_infra():
    """A dimension routed to the WRONG surface records nothing → empty capture → K4
    INFRA-ERROR, NEVER an empty-pass / PARITY-PASS (C-9 / R-03 fault injection).

    Simulate the misroute by feeding K4 an EMPTY retrieval capture (what a wrong-
    surface capture would produce) on both legs and asserting INFRA, not PASS."""
    dim = dimension_by_id("retrieval")
    empty = {"queries": [], "capture_2": []}
    result = classify_dimension(dim, empty, empty)
    assert result.outcome == Outcome.INFRA_ERROR
    assert result.outcome != Outcome.PARITY_PASS


def test_unrouted_dimension_raises_infra(monkeypatch):
    """An unrouted dimension id → InfraError (never silently skipped — R-03)."""
    from dataclasses import replace

    bogus = replace(dimension_by_id("retrieval"), id="bogus", capture_key="bogus")
    with pytest.raises(InfraError):
        cap.capture_dimension(
            bogus,
            FakeUdsClient(),
            "/tmp/x.sock",
            default_workload(),
            "/tmp/store",
            "sid",
            metric_vector={},
            agent_id="a",
            hook_timeout=1.0,
        )


# ===========================================================================
# #5298 11-frame sequence on the UDS hook client (R-03 scenario 2 — UDS half)
# ===========================================================================


def test_drive_emits_11_frame_sequence_no_legacy_variant(fake_bundle):
    """The bundle drive emits the #5298 frame sequence (SessionRegister → cycle_start
    → PreToolUse(TaskCreate) → per-observe Pre+Post → cycle_stop → SessionClose) and
    NEVER a rework/legacy variant (`SessionStart`/`PostToolUse` as a stale `type`)."""
    _, _, frames = fake_bundle
    labels = [lbl for lbl, _ in frames]
    # The CYCLE prologue, in order (PHASE 1). PHASE-3 capture frames (e.g. the
    # PreCompact /observe frame) follow the cycle and are NOT part of the 11-frame
    # sequence — so assert the cycle frames in order, not that SessionClose is last.
    assert labels[0] == "SessionRegister"
    assert labels[1] == "cycle_start"
    assert labels[2] == "PreToolUse"  # the TaskCreate phase-set
    # cycle_stop precedes SessionClose, and SessionClose closes the cycle (before any
    # PHASE-3 capture frame).
    assert "cycle_stop" in labels and "SessionClose" in labels
    assert labels.index("cycle_stop") < labels.index("SessionClose")
    # The phase-set PreToolUse + at least the cycle close happen before PHASE 3.
    assert labels.index("PreToolUse") < labels.index("cycle_stop")
    # No legacy/rework wire variant anywhere.
    assert "SessionStart" not in labels
    assert not any(lbl.startswith("RecordEvent:post_tool_use_rework") for lbl in labels)


def test_hook_error_frame_asserts_not_silent(monkeypatch, tmp_path):
    """An Error frame from the hook daemon fires the `_assert_hook_ok` guard (loud
    failure) — NOT a silent skip (R-03 scenario 4)."""
    frames: list = []

    def _err_hook_ctor(path, timeout=30.0):
        return FakeHookClient(frames, error_on={"cycle_start"})

    monkeypatch.setattr(parity_legs, "UnimatrixHookClient", _err_hook_ctor)
    with pytest.raises(pytest.fail.Exception):  # the _assert_hook_ok loud failure
        drive_uds_leg(
            FakeUdsClient(), tmp_path / "h.sock", default_workload(), tmp_path / "s"
        )


# ===========================================================================
# PreCompact measurability honesty (ADR-006 / R-08)
# ===========================================================================


def test_precompact_carries_measurable_and_host_side_gap(fake_bundle):
    """PreCompact capture carries `measurable`/`host_side_gap`; a `measurable=False`
    names the gap (never a silent pass), and a null restored_payload is legal ONLY
    with measurable=False (R-08 / the bundle contract)."""
    bundle, _, _ = fake_bundle
    pc = bundle["precompact"]
    assert pc["measurable"] is False
    assert pc["restored_payload"] is None  # legal ONLY with measurable=False
    assert isinstance(pc["host_side_gap"], str) and pc["host_side_gap"]


def test_precompact_measurable_false_classifies_documented_exception(fake_bundle):
    """K4 routes a measurable=False precompact to a DOCUMENTED-EXCEPTION (INFRA-ERROR
    call-out), never a vacuous PASS — the honest host-side gap surfaces."""
    bundle, _, _ = fake_bundle
    dim = dimension_by_id("precompact")
    result = classify_dimension(dim, bundle["precompact"], bundle["precompact"])
    assert result.outcome == Outcome.INFRA_ERROR
    assert "MEASURABILITY" in result.detail.upper() or "gap" in result.detail.lower()


# ===========================================================================
# Barrier discipline (R-04) — DB reads gated behind the ONE shared barrier
# ===========================================================================


def test_barrier_runs_after_cycle_stop_before_db_reads():
    """The bundle driver composes `drive_uds_leg`, whose barrier runs AFTER cycle_stop
    and BEFORE the review read; the bundle's PHASE-3 DB-reading captures run AFTER the
    composed `drive_uds_leg` (so after the barrier). Asserted by source ordering."""
    src = inspect.getsource(drive_uds_bundle)
    seed_at = src.index("_seed_corpus_uds(")
    drive_at = src.index("drive_uds_leg(")
    capture_at = src.index("_capture_dimension(")
    assert seed_at < drive_at < capture_at, (
        "bundle order must be: seed → drive_uds_leg (cycle+barrier) → per-dim capture "
        "(every DB read post-barrier — R-04)"
    )
    # drive_uds_leg itself gates the barrier AFTER cycle_stop and BEFORE the review.
    leg_src = inspect.getsource(drive_uds_leg)
    assert leg_src.index("record_cycle_stop(") < leg_src.index("durability_barrier(")
    assert leg_src.index("durability_barrier(") < leg_src.index("context_cycle_review(")


def test_uses_the_one_shared_barrier_helper():
    """The leg uses the ONE shared `durability_barrier` helper (R-04 symmetry — the
    same helper both legs use), parameterized `leg="UDS"`."""
    import harness.parity_workload as pw

    leg_src = inspect.getsource(drive_uds_leg)
    assert "durability_barrier(" in leg_src
    assert 'leg="UDS"' in leg_src
    # It is the SAME callable C4' exposes (no duplicate).
    assert parity_legs.durability_barrier is pw.durability_barrier


def test_db_reads_are_post_barrier_helpers():
    """The DB-reading captures (behavioral topic_signals, isolation on-disk landing)
    live in the capture module and are invoked ONLY from PHASE 3 (after the composed
    `drive_uds_leg` barrier) — never inside the cycle/barrier phase."""
    bundle_src = inspect.getsource(drive_uds_bundle)
    # The DB-reading captures are routed via _capture_dimension (PHASE 3), not before.
    assert "read_topic_signals" not in bundle_src  # only reached via routing
    cap_src = inspect.getsource(cap.capture_dimension)
    assert "read_topic_signals" in cap_src
    assert "capture_isolation" in cap_src


# ===========================================================================
# Backward compat (nan-021) — the committed MetricVector path is untouched
# ===========================================================================


def test_drive_uds_leg_still_returns_metric_vector():
    """nan-021 backward compat: `drive_uds_leg` STILL returns the MetricVector dict
    (the committed `test_https_uds_parity` path), not the bundle (AC-11)."""

    class _MVOnlyUds(FakeUdsClient):
        pass

    # Patch the hook + barrier so the committed driver runs daemon-free.
    import harness.parity_workload as pw

    orig_barrier = parity_legs.durability_barrier
    parity_legs.durability_barrier = lambda **k: 1
    orig_hook = parity_legs.UnimatrixHookClient
    parity_legs.UnimatrixHookClient = lambda path, timeout=30.0: FakeHookClient([])
    try:
        mv = drive_uds_leg(_MVOnlyUds(), "/tmp/h.sock", default_workload(), "/tmp/s")
    finally:
        parity_legs.durability_barrier = orig_barrier
        parity_legs.UnimatrixHookClient = orig_hook
    # MetricVector shape, NOT a dimension bundle.
    assert "universal" in mv
    assert "retrieval" not in mv and "isolation" not in mv


def test_drive_uds_leg_source_unbroken_for_committed_inspection():
    """The committed source-inspection guards in `test_https_uds_parity` inspect
    `drive_uds_leg` for these tokens — keep them present (AC-11, do not refactor the
    committed driver out from under them)."""
    src = inspect.getsource(drive_uds_leg)
    for token in (
        "workload.tool_calls",
        "sid = workload.session_id",
        "PARITY_PHASE",
        "record_cycle_stop(",
        "durability_barrier(",
        'leg="UDS"',
        "context_cycle_review(",
    ):
        assert token in src, f"committed inspection token absent from drive_uds_leg: {token!r}"


def test_assert_derived_attribution_consumed_verbatim():
    """`assert_derived_attribution` is consumed verbatim (string-exact == feature,
    `unattributed`/NULL HARD-fails) — its signature is unchanged from nan-021."""
    sig = inspect.signature(assert_derived_attribution)
    assert list(sig.parameters) == ["feature", "store_dir"]


# ===========================================================================
# No-seed static guard (AC-03) — extends the nan-021 audit to the new modules
# ===========================================================================


def test_no_seed_site_reachable_from_leg_modules():
    """NO forbidden seed site is reachable from the leg driver OR the capture module
    (extends the nan-021 `test_c3_no_seed_site_reachable` to the nan-022 split)."""
    import harness.parity_workload as pw

    pw.assert_no_seed_reachable(parity_legs.__file__, cap.__file__)
