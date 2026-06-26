"""K1 unit tests — the dimension registry (nan-022 / #837).

Tier A (off-Docker unit): pure structural assertions over `harness.parity_dimensions`.
Maps 1:1 to test-plan/parity_dimensions.md. Covers the registry half of R-03 (Critical)
and the single-source half of R-05 (SR-05/#5302): `DIMENSIONS` is the ONE enumeration;
every consumer iterates THIS tuple. Wrong wire-surface routing is the single most
dangerous false-GREEN path (records nothing → vacuous pass); the registry-vs-driver
routing consistency check is the off-Docker guard for it.

NO Docker, NO daemon. K2 (`parity_comparator`) is Wave B and not imported here — the
comparator binding is exercised through the prescribed late-binding hook with local
stand-in classes, never a module-level stub.
"""

from __future__ import annotations

import dataclasses

import pytest

from harness import parity_dimensions as pd
from harness.parity_dimensions import (
    DIMENSIONS,
    WIRE_HOOK_OBSERVE,
    WIRE_MCP_BRIDGE,
    Dimension,
    bind_comparators,
    capture_keys,
    dimension_by_id,
)

# The six dimension ids — the C0 (#5304) dimension list; broadening is out of scope.
EXPECTED_IDS = {
    "retrieval",
    "behavioral",
    "analytics",
    "proactive",
    "precompact",
    "isolation",
}

# Architecture routing table (ARCHITECTURE §5 / brief Data Structures): the PRIMARY
# wire_surface each dimension is registered under. analytics + isolation also touch
# the secondary surface (the leg-driver fan-out), declared via DUAL_SURFACE_DIMENSIONS.
EXPECTED_PRIMARY_SURFACE = {
    "retrieval": WIRE_MCP_BRIDGE,
    "behavioral": WIRE_HOOK_OBSERVE,
    "analytics": WIRE_MCP_BRIDGE,
    "proactive": WIRE_MCP_BRIDGE,
    "precompact": WIRE_HOOK_OBSERVE,
    "isolation": WIRE_MCP_BRIDGE,
}

# The brief's Data Structures table comparator names (pre-binding, the K2 class names).
EXPECTED_COMPARATOR_NAME = {
    "retrieval": "RetrievalComparator",
    "behavioral": "AttributionComparator",
    "analytics": "MetricVectorComparator",
    "proactive": "BriefingComparator",
    "precompact": "PreCompactComparator",
    "isolation": "IsolationComparator",
}


# ===========================================================================
# Enumeration completeness + identity (SR-05 single-source)
# ===========================================================================
def test_dimensions_enumerates_exactly_six():
    assert len(DIMENSIONS) == 6
    assert {d.id for d in DIMENSIONS} == EXPECTED_IDS


def test_dimension_is_frozen_dataclass():
    assert dataclasses.is_dataclass(Dimension)
    params = getattr(Dimension, "__dataclass_params__")
    assert params.frozen is True
    # A consumer cannot mutate a registry row at runtime.
    with pytest.raises(dataclasses.FrozenInstanceError):
        DIMENSIONS[0].id = "mutated"  # type: ignore[misc]


def test_capture_keys_unique():
    keys = capture_keys()
    assert len(keys) == len(set(keys))
    assert len(keys) == 6


def test_capture_keys_match_bundle_schema():
    # The on-disk dimension_bundle keys (OVERVIEW R-09 contract) are EXACTLY the
    # registry capture_keys — no orphan key on either side.
    assert set(capture_keys()) == EXPECTED_IDS


# ===========================================================================
# Wire-surface routing (R-03 scenario 1 — registry side)
# ===========================================================================
def test_wire_surface_is_one_of_two_constants():
    for d in DIMENSIONS:
        assert d.wire_surface in (WIRE_MCP_BRIDGE, WIRE_HOOK_OBSERVE), d.id


@pytest.mark.parametrize("dim_id,surface", sorted(EXPECTED_PRIMARY_SURFACE.items()))
def test_wire_surface_assignments_match_architecture(dim_id, surface):
    assert dimension_by_id(dim_id).wire_surface == surface


def test_dual_surface_dimensions_declared():
    # analytics + isolation touch BOTH surfaces — the fan-out the leg driver performs.
    assert pd.DUAL_SURFACE_DIMENSIONS == {"analytics", "isolation"}
    assert pd.DUAL_SURFACE_DIMENSIONS <= EXPECTED_IDS


# ===========================================================================
# Outcome-policy flags
# ===========================================================================
def test_intra_transport_check_only_retrieval_and_proactive():
    intra = {d.id for d in DIMENSIONS if d.intra_transport_check}
    assert intra == {"retrieval", "proactive"}


def test_blocks_c0_proof_all_six_true():
    assert all(d.blocks_c0_proof is True for d in DIMENSIONS)


# ===========================================================================
# Comparator binding (couples to K2 drift guard) — exercised via the late-binding hook
# ===========================================================================
def _comparator_name(comparator) -> str:
    """The K2 class NAME for a comparator field, whether it is still a string (Wave-A,
    pre-bind) or already bound to a real K2 class (any K2 importer in the session)."""
    return comparator if isinstance(comparator, str) else comparator.__name__


def _restring_dimensions(snapshot):
    """A copy of `snapshot` with every comparator field forced back to its string name,
    so `bind_comparators` (which only re-binds `str` entries) acts on it deterministically
    no matter what a prior K2 importer left bound (the #5317 contamination shape)."""
    return tuple(
        dataclasses.replace(d, comparator=_comparator_name(d.comparator))
        for d in snapshot
    )


def test_comparator_field_resolves_to_expected_name():
    # Each comparator field resolves to the brief's Data-Structures-table class NAME —
    # order-independent: a string in Wave A, a real K2 class once K2 has bound it. K1
    # must be importable without K2; the bound-class case is the K2-in-session path.
    for d in DIMENSIONS:
        assert _comparator_name(d.comparator) == EXPECTED_COMPARATOR_NAME[d.id], d.id


def test_each_dimension_comparator_is_dimension_comparator_subclass():
    # The structural subclass check is K2's drift guard; here we prove the late-binding
    # hook resolves names → classes so the bound registry satisfies it. Local stand-in
    # classes simulate K2's DimensionComparator subclasses (NOT a module stub).
    #
    # Order-/contamination-robust (pattern #5317): snapshot DIMENSIONS and restore it in
    # finally, and re-string the registry BEFORE the local bind. `bind_comparators` only
    # re-binds `str` entries, so without re-stringing, a real K2 import-time bind already
    # in the session would make the local bind a no-op and the issubclass(_Base) check
    # would run against the real K2 classes and fail. The snapshot restores real bindings.
    class _Base:
        pass

    classes = {
        name: type(name, (_Base,), {}) for name in EXPECTED_COMPARATOR_NAME.values()
    }
    snapshot = pd.DIMENSIONS
    try:
        pd.DIMENSIONS = _restring_dimensions(snapshot)
        bind_comparators(classes)
        for d in pd.DIMENSIONS:
            assert isinstance(d.comparator, type), d.id
            assert issubclass(d.comparator, _Base), d.id
            assert d.comparator is classes[EXPECTED_COMPARATOR_NAME[d.id]], d.id
    finally:
        pd.DIMENSIONS = snapshot


def test_bind_comparators_rejects_missing_name():
    snapshot = pd.DIMENSIONS
    try:
        pd.DIMENSIONS = _restring_dimensions(snapshot)
        classes = {
            name: type(name, (object,), {})
            for name in EXPECTED_COMPARATOR_NAME.values()
            if name != "RetrievalComparator"
        }
        with pytest.raises(KeyError):
            bind_comparators(classes)
    finally:
        pd.DIMENSIONS = snapshot


def test_bind_comparators_rejects_extra_name():
    snapshot = pd.DIMENSIONS
    try:
        pd.DIMENSIONS = _restring_dimensions(snapshot)
        classes = {
            name: type(name, (object,), {})
            for name in EXPECTED_COMPARATOR_NAME.values()
        }
        classes["GhostComparator"] = type("GhostComparator", (object,), {})
        with pytest.raises(KeyError):
            bind_comparators(classes)
    finally:
        pd.DIMENSIONS = snapshot


# ===========================================================================
# Helpers
# ===========================================================================
def test_dimension_by_id_raises_on_unknown():
    with pytest.raises(KeyError):
        dimension_by_id("does_not_exist")


# ===========================================================================
# Registry-vs-driver routing consistency (R-03 scenario 1 — off-Docker false-GREEN guard)
# ===========================================================================
def test_registry_routing_matches_driver_behavior():
    # Every Dimension.wire_surface must match the surface the leg driver actually
    # captures for that dimension. C3' (parity_legs.drive_uds_leg per-dimension wire
    # routing) is a Wave-C extension not yet present; the registry-side guard asserts
    # the registry agrees with the architecture's authoritative routing table — the
    # leg driver is built to THIS table. The live byte-identity half is in
    # test-plan/test_https_uds_parity.md (Wave E). See the wave-ordering flag.
    for d in DIMENSIONS:
        assert d.wire_surface == EXPECTED_PRIMARY_SURFACE[d.id], d.id
