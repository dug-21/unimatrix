# K1 — Dimension registry (`harness/parity_dimensions.py`)

**New**, pure-Python, stdlib-only, off-Docker unit-testable.

## Purpose

The SINGLE authoritative enumeration of the six C0 parity dimensions (ADR-001, SR-05/#5302).
Nothing else hand-lists the six; every consumer (leg drivers, orchestrator, drift guard, CI
table, forbidden-seed audit) iterates this ONE tuple. The registry is data-only so a future
re-disposition of `blocks_c0_proof` is a data change, not a code change.

## Imports

- From K2 `parity_comparator`: `MetricVectorComparator`, `RetrievalComparator`,
  `BriefingComparator`, `AttributionComparator`, `PreCompactComparator`, `IsolationComparator`
  (the `comparator` field binds to these classes — see dependency note in OVERVIEW: K2 names
  must exist before `DIMENSIONS` is frozen).

## Constants

```
WIRE_MCP_BRIDGE  = "mcp_bridge"      # context_* tools/call over bridge / MCP UDS
WIRE_HOOK_OBSERVE = "hook_observe"   # pinned /observe POST / hook IPC
```

## Type

```
@dataclass(frozen=True)
class Dimension:
    id: str                      # "retrieval"|"behavioral"|"analytics"|"proactive"|"precompact"|"isolation"
    capture_key: str             # key under dimension_bundle both legs emit
    wire_surface: str            # WIRE_MCP_BRIDGE | WIRE_HOOK_OBSERVE  (primary surface)
    comparator: type             # a K2 DimensionComparator subclass (the CLASS, not an instance)
    intra_transport_check: bool  # run double-capture-and-diff stability classifier?
    blocks_c0_proof: bool        # in the six required for the C0 flip?
```

Note on dual-surface dimensions: `analytics` and `isolation` touch BOTH surfaces. The registry
records the PRIMARY `wire_surface` for routing/labelling; the leg drivers (C3'/C5') fan out the
secondary surface explicitly (documented there). `wire_surface` is one of the two constants
(asserted by the drift guard); a dual-surface dimension's secondary capture is handled in the
driver, not by a third constant.

## `DIMENSIONS`

```
DIMENSIONS: tuple[Dimension, ...] = (
  Dimension("retrieval",  "retrieval",  WIRE_MCP_BRIDGE,   RetrievalComparator,   intra_transport_check=True,  blocks_c0_proof=True),
  Dimension("behavioral", "behavioral", WIRE_HOOK_OBSERVE, AttributionComparator, intra_transport_check=False, blocks_c0_proof=True),
  Dimension("analytics",  "analytics",  WIRE_MCP_BRIDGE,   MetricVectorComparator,intra_transport_check=False, blocks_c0_proof=True),
  Dimension("proactive",  "proactive",  WIRE_MCP_BRIDGE,   BriefingComparator,    intra_transport_check=True,  blocks_c0_proof=True),
  Dimension("precompact", "precompact", WIRE_HOOK_OBSERVE, PreCompactComparator,  intra_transport_check=False, blocks_c0_proof=True),
  Dimension("isolation",  "isolation",  WIRE_MCP_BRIDGE,   IsolationComparator,   intra_transport_check=False, blocks_c0_proof=True),
)
```

`blocks_c0_proof=True` for all six is CONFIRMED correct (human, 2026-06-25): the corrected C0
(#5304) `done_when` makes parity the total bar. Any unreachable dimension is a human-signed
documented exception (the flag is the escape valve), never a silent exclusion.

## Helpers

```
def dimension_by_id(dim_id: str) -> Dimension:
    # linear scan over DIMENSIONS; raise KeyError if absent (no second source).

def capture_keys() -> tuple[str, ...]:
    # tuple(d.capture_key for d in DIMENSIONS) — used by load_https_bundle's
    # required-key check and by the orchestrator's bundle ingest.
```

## Data flow

- INPUT: none (static data module).
- OUTPUT: `DIMENSIONS` consumed by C3' (routing + capture loop), ORCH (classify loop), K2
  (`assert_comparator_contract`), C4'/K5 (`load_https_bundle` required-key set).

## Error handling

- No runtime errors. The module is data; correctness is asserted structurally by the K2 drift
  guard (`assert_comparator_contract`) and the off-Docker registry tests below.

## Key test scenarios (hints; full plan in test-plan/)

- Exactly six dimensions; ids are unique and exactly the six named.
- `capture_key`s are unique and match the OVERVIEW on-disk bundle schema keys (no orphan key).
- Every `wire_surface` is one of `{WIRE_MCP_BRIDGE, WIRE_HOOK_OBSERVE}` (R-03 registry side).
- Every `comparator` is a `DimensionComparator` subclass (overlaps K2 drift guard).
- `intra_transport_check=True` exactly for `retrieval` and `proactive`.
- `blocks_c0_proof=True` for all six.
- `Dimension` is a frozen dataclass (immutability; no accidental mutation of the single source).
