"""K1 — Dimension registry (nan-022 / #837 / ADR-001 / SR-05 / #5302).

The SINGLE authoritative enumeration of the six C0 (#5304) parity dimensions:
retrieval, behavioral, analytics, proactive, precompact, isolation. Nothing else
hand-lists the six — every consumer (leg drivers C3', orchestrator ORCH, the K2
drift guard, the CI evidence table, the forbidden-seed audit) iterates this ONE
`DIMENSIONS` tuple. The registry is data-only so a future re-disposition of
`blocks_c0_proof` is a DATA change, not a code change (ADR-001).

Pure-Python, stdlib-only, OFF-Docker unit-testable (the #5258 seam). ZERO new
runtime deps. TEST-ONLY; no production-code diff (NFR-1/NFR-2/AC-11).

--------------------------------------------------------------------------------
COMPARATOR BINDING — late-binding registration hook (OVERVIEW Wave A → Wave B)
--------------------------------------------------------------------------------
`Dimension.comparator` ultimately holds a K2 `DimensionComparator` subclass (the
CLASS, not an instance). But K2 (`parity_comparator`) is authored in Wave B and its
`assert_comparator_contract` imports `DIMENSIONS` from THIS module — a direct
`from harness.parity_comparator import RetrievalComparator` here would be a Wave-A
circular import.

Resolution (prescribed by OVERVIEW: "Author K1 constants/dataclass + WIRE_* first;
wire the comparator references after K2"): the registry carries each comparator's
class NAME (a string — data the brief's Data Structures table already fixes) until
K2 exists. K2, once imported, calls `bind_comparators({name: class, ...})` ONCE to
resolve every name to its class in place. This makes K1 fully importable in Wave A
with NO stub comparator and NO circular import; K2 completes the binding in Wave B.

`Dimension.comparator` therefore reads as `str` (the K2 class name) before binding
and as a `type` (the K2 class) after `bind_comparators` has run. The K2 drift guard
(`assert_comparator_contract`) is what asserts every bound value is a
`DimensionComparator` subclass — that structural check is K2's, not K1's (this
module never imports K2).
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from typing import Union

# =============================================================================
# 1. Wire-surface constants (ADR-005 / SR-08 — the TWO HTTPS/UDS wire surfaces)
# =============================================================================
# A dimension routed to the WRONG surface records NOTHING → the never-empty guard
# makes that an INFRA-ERROR (R-03), never a vacuous empty-equals-empty pass. The
# drift guard + the registry tests assert every `wire_surface` is exactly one of
# these two literals (no third / typo'd surface).

WIRE_MCP_BRIDGE: str = "mcp_bridge"  # context_* tools/call over bridge / MCP UDS
WIRE_HOOK_OBSERVE: str = "hook_observe"  # pinned /observe POST / hook IPC

WIRE_SURFACES: frozenset[str] = frozenset({WIRE_MCP_BRIDGE, WIRE_HOOK_OBSERVE})

# Dual-surface dimensions touch BOTH wire surfaces. The registry records the PRIMARY
# `wire_surface` for routing/labelling; the leg drivers (C3'/C5') fan out the
# secondary surface explicitly (a third constant is NOT introduced — the secondary
# capture is handled in the driver). This frozenset is the single source the leg
# drivers and the registry-vs-driver routing consistency test consult.
DUAL_SURFACE_DIMENSIONS: frozenset[str] = frozenset({"analytics", "isolation"})


# =============================================================================
# 2. The Dimension record (frozen — the single source cannot be mutated at runtime)
# =============================================================================
@dataclass(frozen=True)
class Dimension:
    """One row of the C0 parity matrix. Frozen so no consumer can mutate the single
    authoritative enumeration at runtime (SR-05).

    `comparator` is `str` (the K2 class name) until K2 calls `bind_comparators`,
    then a `type` (the K2 `DimensionComparator` subclass). See the module docstring.
    """

    id: str  # "retrieval"|"behavioral"|"analytics"|"proactive"|"precompact"|"isolation"
    capture_key: str  # key under dimension_bundle both legs emit
    wire_surface: str  # WIRE_MCP_BRIDGE | WIRE_HOOK_OBSERVE (the PRIMARY surface)
    comparator: Union[str, type]  # K2 class name (str) → bound to the K2 class (type)
    intra_transport_check: bool  # run double-capture-and-diff stability classifier?
    blocks_c0_proof: bool  # in the six required for the C0 (#5304) flip?


# =============================================================================
# 3. DIMENSIONS — the SIX, the single authoritative enumeration (ADR-001 table)
# =============================================================================
# Flags taken EXACTLY from the brief's Data Structures table:
#   intra_transport_check=True ONLY for retrieval + proactive (embedding-ranked dims).
#   blocks_c0_proof=True for ALL SIX — CONFIRMED correct (human, 2026-06-25): the
#   corrected C0 (#5304) done_when makes parity the TOTAL bar; the dimension list
#   grows with the pipeline and never narrows the bar. Any unreachable dimension is a
#   human-signed DOCUMENTED EXCEPTION (the flag is the data-only escape valve), never
#   a silent exclusion. `comparator` holds the K2 class NAME until K2 binds it.
DIMENSIONS: tuple[Dimension, ...] = (
    Dimension(
        id="retrieval",
        capture_key="retrieval",
        wire_surface=WIRE_MCP_BRIDGE,
        comparator="RetrievalComparator",
        intra_transport_check=True,
        blocks_c0_proof=True,
    ),
    Dimension(
        id="behavioral",
        capture_key="behavioral",
        wire_surface=WIRE_HOOK_OBSERVE,
        comparator="AttributionComparator",
        intra_transport_check=False,
        blocks_c0_proof=True,
    ),
    Dimension(
        id="analytics",
        capture_key="analytics",
        wire_surface=WIRE_MCP_BRIDGE,
        comparator="MetricVectorComparator",
        intra_transport_check=False,
        blocks_c0_proof=True,
    ),
    Dimension(
        id="proactive",
        capture_key="proactive",
        wire_surface=WIRE_MCP_BRIDGE,
        comparator="BriefingComparator",
        intra_transport_check=True,
        blocks_c0_proof=True,
    ),
    Dimension(
        id="precompact",
        capture_key="precompact",
        wire_surface=WIRE_HOOK_OBSERVE,
        comparator="PreCompactComparator",
        intra_transport_check=False,
        blocks_c0_proof=True,
    ),
    Dimension(
        id="isolation",
        capture_key="isolation",
        wire_surface=WIRE_MCP_BRIDGE,
        comparator="IsolationComparator",
        intra_transport_check=False,
        blocks_c0_proof=True,
    ),
)


# =============================================================================
# 4. Late-binding registration hook — K2 (Wave B) completes the comparator binding
# =============================================================================
def bind_comparators(classes: dict[str, type]) -> None:
    """Resolve each `Dimension.comparator` class NAME to its K2 class IN PLACE.

    Called ONCE by K2 (`parity_comparator`) after its `DimensionComparator`
    subclasses are defined: `bind_comparators({"RetrievalComparator": ..., ...})`.
    This is the prescribed late-binding mechanism (OVERVIEW Wave A→B) that keeps K1
    importable in Wave A with no stub and no circular import.

    Idempotent: re-binding an already-bound name to the SAME class is a no-op; the
    `classes` mapping MUST cover every comparator name currently in `DIMENSIONS` and
    introduce no extra names (so the binding cannot silently drift from the registry).

    Raises KeyError if a registry comparator name is absent from `classes`, or if
    `classes` carries a name not in the registry (the single-source guard).
    """
    global DIMENSIONS

    registry_names = {
        d.comparator for d in DIMENSIONS if isinstance(d.comparator, str)
    }
    # Already-bound names (type, not str) are accepted only if re-offered identically.
    bound_names = {d.comparator.__name__ for d in DIMENSIONS if not isinstance(d.comparator, str)}
    all_names = registry_names | bound_names

    missing = all_names - set(classes)
    if missing:
        raise KeyError(
            f"bind_comparators: registry comparator name(s) absent from binding map: "
            f"{sorted(missing)}"
        )
    extra = set(classes) - all_names
    if extra:
        raise KeyError(
            f"bind_comparators: binding map carries name(s) not in DIMENSIONS "
            f"(single-source drift): {sorted(extra)}"
        )

    DIMENSIONS = tuple(
        replace(d, comparator=classes[d.comparator])
        if isinstance(d.comparator, str)
        else d
        for d in DIMENSIONS
    )


# =============================================================================
# 5. Helpers — the ONLY accessors over DIMENSIONS (no second source)
# =============================================================================
def dimension_by_id(dim_id: str) -> Dimension:
    """Linear scan over `DIMENSIONS`; raise KeyError if absent (no second source)."""
    for dim in DIMENSIONS:
        if dim.id == dim_id:
            return dim
    raise KeyError(f"no dimension with id {dim_id!r} in DIMENSIONS")


def capture_keys() -> tuple[str, ...]:
    """Every `capture_key`, in registry order. Used by `load_https_bundle`'s
    required-key check (C4'/K5) and the orchestrator's bundle ingest (ORCH)."""
    return tuple(d.capture_key for d in DIMENSIONS)
