"""K2 — Comparator framework + cross-dimension drift guard (nan-022 / #837).

ADR-003 (#5307) / SR-05 / #5302. Pure-Python, stdlib-only, OFF-Docker
unit-testable (the #5258 seam). ZERO new runtime deps. TEST-ONLY; no
production-code diff (NFR-1/NFR-2/AC-11).

Lifts the nan-021 `metric_comparator` shape into ONE base class so the five new
C0 dimensions cannot drift from the closed-exclusion-set discipline by
construction, not by convention (the structural #5302 fix). Single-sources:

  * ONE `DimensionComparator` ABC + six concrete comparators.
  * ONE `ranking_parity` tolerance (imported from K3) shared by retrieval +
    briefing — there is NO second tie policy (SR-03 / NFR-4 / C-5).
  * ONE `FORBIDDEN_SEED_SITES` tuple — re-exported from C4' `parity_workload`
    (which has owned it since nan-021). K2 defines NO private copy; the drift
    guard asserts object identity with the C4' tuple (SR-05 / #5302).
  * ONE `assert_comparator_contract` off-Docker drift guard.

DISPOSITION AUTHORITY (carried verbatim, nan-021 ADR-003 #5293 / C-4 / NFR-8):
any field OUTSIDE a closed `EXCLUDED` that differs is a REAL failure surfaced
LOUD via `ParityMismatch`; the implementer/tester NEVER silently widens a set —
disposition is a PRODUCT/HUMAN call (GH bug OR product-signed `context_correct`
amendment). The base class enforces this by making `ParityMismatch` the only
exit from a non-excluded diff.

--------------------------------------------------------------------------------
REGISTRY BINDING (Wave A → Wave B late-binding hook, K1-prescribed)
--------------------------------------------------------------------------------
K1's `Dimension.comparator` holds the comparator CLASS NAME (str) until bound.
At module import this module calls `parity_dimensions.bind_comparators({...})`
ONCE with all six {name: class} pairs so the registry resolves names → classes.
Without this the orchestrator / drift-guard subclass checks fail loudly (intended).
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any

from harness import parity_dimensions
from harness import parity_workload
from harness.metric_comparator import (
    EXCLUDED as MV_EXCLUDED,
)
from harness.metric_comparator import (
    EXCLUSION_JUSTIFICATIONS as MV_JUSTIFICATIONS,
)
from harness.metric_comparator import (
    ParityMismatch,
    compare_metric_vectors,
    field_by_field_record,
)
from harness.parity_dimensions import (
    WIRE_HOOK_OBSERVE,
    WIRE_MCP_BRIDGE,
)
from harness.ranking_tolerance import ranking_parity

# Re-export the ONE forbidden-seed tuple that lives in C4' parity_workload so
# there is exactly ONE such tuple object in the codebase. K2 does NOT define a
# private copy — the drift guard asserts object identity (SR-05 / #5302).
FORBIDDEN_SEED_SITES: tuple[str, ...] = parity_workload.FORBIDDEN_SEED_SITES

# Re-export so callers have ONE import surface for the loud non-excluded-diff
# signal (ParityMismatch is owned by metric_comparator; re-exported, not redefined).
__all__ = [
    "DimensionComparator",
    "MetricVectorComparator",
    "RetrievalComparator",
    "BriefingComparator",
    "AttributionComparator",
    "PreCompactComparator",
    "IsolationComparator",
    "ParityMismatch",
    "FORBIDDEN_SEED_SITES",
    "assert_comparator_contract",
]


# =============================================================================
# 1. Base class — the ONE enforced comparator shape (ADR-003)
# =============================================================================
class DimensionComparator(ABC):
    """Base class for every per-dimension parity comparator.

    Every concrete subclass MUST declare:
      EXCLUDED: frozenset[str]                 closed, enumerated exclusions.
      EXCLUSION_JUSTIFICATIONS: dict[str, str] one inline justification per key.

    and implement `compare`. `compare` performs field-for-field equality MODULO
    `EXCLUDED` and raises `ParityMismatch` (loud, field + both values + leg) on
    any non-excluded diff — the ONLY exit from a non-excluded divergence (C-4).

    An EMPTY `EXCLUDED` is permitted ONLY where the dimension is provably
    exclusion-free (transport-invariant booleans/strings — attribution, isolation);
    the drift guard encodes that policy. For an empty set, `EXCLUSION_JUSTIFICATIONS`
    is also empty and the key-coverage invariant holds vacuously.
    """

    EXCLUDED: frozenset[str] = frozenset()
    EXCLUSION_JUSTIFICATIONS: dict[str, str] = {}

    @abstractmethod
    def compare(self, https: Any, uds: Any) -> list[tuple[str, Any, Any]]:
        """Field-for-field equality MODULO `EXCLUDED`. Returns the (empty) diff
        list on parity; raises `ParityMismatch` on any non-excluded diff."""
        raise NotImplementedError

    def evidence_record(self, https: Any, uds: Any, *, run_token: str) -> dict:
        """First-live-run field-by-field record (ADR-003 discipline, generalized).

        Default impl: a non-raising SHADOW compare so the raw captures + the
        observed diffs are emitted for product/human disposition on a PARITY-FAIL.
        Keyed by the run-correlation token so a stale prior-tag artifact cannot
        masquerade. `MetricVectorComparator` overrides to delegate to the consumed
        `field_by_field_record` verbatim."""
        try:
            diffs = self.compare(https, uds)
        except ParityMismatch as exc:
            diffs = list(exc.diffs)
        return {
            "run_token": run_token,
            "dimension": self.__class__.__name__,
            "excluded_set": sorted(self.EXCLUDED),
            "exclusion_justifications": dict(self.EXCLUSION_JUSTIFICATIONS),
            "raw_https": https,
            "raw_uds": uds,
            "diffs": [list(d) for d in diffs],
        }


# =============================================================================
# 2. Analytics — MetricVectorComparator (wraps nan-021 verbatim, AC-04 / R-14)
# =============================================================================
def _compare_informs_edges(
    https_edges: list, uds_edges: list
) -> list[tuple[str, Any, Any]]:
    """Compare the Informs edge-ID SET as UNORDERED, IDs EXACT (NFR-6 / R-11).

    Any wall-clock/ordering edge field is a justified `EXCLUDED` entry handled by
    the caller; the edge-ID set itself is NEVER excluded. Captures are settled
    behind the durability barrier upstream (R-04/R-11) — a missing capture is
    INFRA-ERROR in K4, never compared here.
    """
    h_set, u_set = set(https_edges), set(uds_edges)
    if h_set != u_set:
        return [("informs_edges", sorted(h_set), sorted(u_set))]
    return []


def _compare_phase_signal(
    https_signal: Any, uds_signal: Any
) -> list[tuple[str, Any, Any]]:
    """Compare the phase signal EXACTLY (NFR-6 — no tolerance)."""
    if https_signal != uds_signal:
        return [("phase_signal", https_signal, uds_signal)]
    return []


class MetricVectorComparator(DimensionComparator):
    """Analytics (D3) — a thin ADAPTER over the consumed nan-021 logic (AC-04).

    Delegates the MetricVector slice to `compare_metric_vectors` UNCHANGED; the
    net-new analytics surfaces (Informs edges + phase signal) are compared
    alongside. The MetricVector logic is NOT re-authored here.
    """

    EXCLUDED = MV_EXCLUDED  # the nan-021 closed 3-field set, unchanged
    EXCLUSION_JUSTIFICATIONS = MV_JUSTIFICATIONS

    def compare(self, https: Any, uds: Any) -> list[tuple[str, Any, Any]]:
        # analytics capture: {"metric_vector":{...}, "informs_edges":[...], "phase_signal":{...}}
        # compare_metric_vectors raises ParityMismatch internally on a MV diff;
        # capture the diffs to combine with the net-new analytics surfaces so the
        # full divergence is reported in ONE ParityMismatch.
        try:
            diffs = compare_metric_vectors(https["metric_vector"], uds["metric_vector"])
        except ParityMismatch as exc:
            diffs = list(exc.diffs)
        diffs += _compare_informs_edges(https["informs_edges"], uds["informs_edges"])
        diffs += _compare_phase_signal(https["phase_signal"], uds["phase_signal"])
        if diffs:
            raise ParityMismatch(diffs)
        return diffs

    def evidence_record(self, https: Any, uds: Any, *, run_token: str) -> dict:
        # Delegate the MetricVector slice to the consumed record verbatim, then
        # attach the informs/phase rows. Do NOT re-author the MetricVector record.
        rec = field_by_field_record(
            https["metric_vector"], uds["metric_vector"], run_token=run_token
        )
        rec["informs_edges_https"] = https["informs_edges"]
        rec["informs_edges_uds"] = uds["informs_edges"]
        rec["phase_signal_https"] = https["phase_signal"]
        rec["phase_signal_uds"] = uds["phase_signal"]
        return rec


# =============================================================================
# 3. Retrieval (D1) + Briefing (D4) — both call the SAME K3 ranking_parity
# =============================================================================
class RetrievalComparator(DimensionComparator):
    """Retrieval (D1) — ranked result-id parity via the ONE K3 `ranking_parity`."""

    EXCLUDED = frozenset(
        {"tail_churn", "score_jitter_beyond_prefix", "tie_order_within_class"}
    )
    EXCLUSION_JUSTIFICATIONS = {
        "tail_churn": (
            "HNSW approximate top-k membership flip below the stable prefix "
            "(#4990/GH#746) — intra-transport, not cross-transport divergence"
        ),
        "score_jitter_beyond_prefix": (
            "per-process embedding score jitter beyond the stable ranked prefix"
        ),
        "tie_order_within_class": (
            "equal-score tie ordering (#2610 HashMap / sort_unstable) — compared "
            "as an unordered tie-class, not positionally"
        ),
    }

    def compare(self, https: Any, uds: Any) -> list[tuple[str, Any, Any]]:
        # capture: {"queries":[{"tool","args","result_ids","scores"}...], "capture_2":[...]}
        diffs: list[tuple[str, Any, Any]] = []
        for i, (q_https, q_uds) in enumerate(zip(https["queries"], uds["queries"])):
            verdict = ranking_parity(
                q_https["result_ids"],
                q_uds["result_ids"],
                scores=(q_https.get("scores"), q_uds.get("scores")),
            )
            if not verdict.matched:
                diffs.append(
                    (
                        f"query[{i}].stable_prefix",
                        q_https["result_ids"],
                        q_uds["result_ids"],
                    )
                )
        if diffs:
            raise ParityMismatch(diffs)
        return diffs


class BriefingComparator(DimensionComparator):
    """Proactive briefing (D4) — the SAME K3 `ranking_parity` policy (single-sourced)."""

    EXCLUDED = frozenset(
        {"injection_history_timestamp", "tail_churn", "tie_order_within_class"}
    )
    EXCLUSION_JUSTIFICATIONS = {
        "injection_history_timestamp": (
            "wall-clock session-state injection-history stamp"
        ),
        "tail_churn": (
            "ranked-prefix tail churn (shared D1/D4 entropy class — #4990/GH#746)"
        ),
        "tie_order_within_class": (
            "equal-score tie ordering compared as an unordered tie-class"
        ),
    }

    def compare(self, https: Any, uds: Any) -> list[tuple[str, Any, Any]]:
        # capture: {"briefing_ids":[...], "briefing_scores":[...], "injection_set":[...], "capture_2":{...}}
        diffs: list[tuple[str, Any, Any]] = []
        verdict = ranking_parity(
            https["briefing_ids"],
            uds["briefing_ids"],
            scores=(https.get("briefing_scores"), uds.get("briefing_scores")),
        )
        if not verdict.matched:
            diffs.append(
                ("briefing.stable_prefix", https["briefing_ids"], uds["briefing_ids"])
            )
        # injection set: unordered SET equality, IDs exact (NFR-6) — NOT ranked.
        if set(https["injection_set"]) != set(uds["injection_set"]):
            diffs.append(
                (
                    "injection_set",
                    sorted(https["injection_set"]),
                    sorted(uds["injection_set"]),
                )
            )
        if diffs:
            raise ParityMismatch(diffs)
        return diffs


# =============================================================================
# 4. Attribution (D2) — string-exact, EXCLUDED empty
# =============================================================================
class AttributionComparator(DimensionComparator):
    """Behavioral attribution (D2) — topic-signal set, string-exact (NFR-6)."""

    EXCLUDED = frozenset()  # transport-invariant; no wall-clock field
    EXCLUSION_JUSTIFICATIONS = {}

    def compare(self, https: Any, uds: Any) -> list[tuple[str, Any, Any]]:
        # capture: {"topic_signals":[...]}
        s_https, s_uds = set(https["topic_signals"]), set(uds["topic_signals"])
        diffs: list[tuple[str, Any, Any]] = []
        # an "unattributed" signal on either leg is a HARD fail (derivation broke).
        if "unattributed" in s_https or "unattributed" in s_uds:
            diffs.append(
                ("topic_signals.unattributed", sorted(s_https), sorted(s_uds))
            )
        if s_https != s_uds:
            diffs.append(("topic_signals", sorted(s_https), sorted(s_uds)))
        if diffs:
            raise ParityMismatch(diffs)
        return diffs


# =============================================================================
# 5. PreCompact (D5) — measurability-aware (ADR-006)
# =============================================================================
def _compare_restored_payload(
    https_payload: Any, uds_payload: Any, *, excluded: frozenset[str]
) -> list[tuple[str, Any, Any]]:
    """Compare the SERVER-restored payload byte-equal MODULO `excluded`.

    Equality is over the server-restored content (the set of restored entry ids +
    their restored content/order fields), NOT host-side presentation. A top-level
    excluded key (e.g. `restoration_timestamp`) and any `envelope.*` field are
    dropped before comparison; everything else compares exactly.
    """
    # Static-prefix exclusion: a literal key, or any prefix ending in ".*".
    prefixes = tuple(e[:-1] for e in excluded if e.endswith(".*"))  # e.g. "envelope."
    literals = frozenset(e for e in excluded if not e.endswith(".*"))

    def _excluded(key: str) -> bool:
        return key in literals or any(key.startswith(p) for p in prefixes)

    def _filtered(payload: dict) -> dict:
        return {k: v for k, v in payload.items() if not _excluded(k)}

    h = _filtered(https_payload)
    u = _filtered(uds_payload)
    diffs: list[tuple[str, Any, Any]] = []
    if set(h) != set(u):
        diffs.append(("restored_payload.keys", sorted(h), sorted(u)))
    for k in sorted(set(h) & set(u)):
        if h[k] != u[k]:
            diffs.append((f"restored_payload.{k}", h[k], u[k]))
    return diffs


class PreCompactComparator(DimensionComparator):
    """PreCompact restoration (D5) — content-equal modulo wall-clock/envelope (ADR-006).

    MEASURABILITY is handled in K4 BEFORE `compare` runs:
      * measurable=False on either leg -> K4 records a DOCUMENTED-EXCEPTION; compare
        NOT called (never a vacuous pass, never rounded up to "fully measured").
      * measurable=True on both         -> `compare` runs over the restored payload.
    This method assumes both legs are measurable with non-null payloads.
    """

    EXCLUDED = frozenset({"restoration_timestamp", "envelope.*"})
    EXCLUSION_JUSTIFICATIONS = {
        "restoration_timestamp": "wall-clock stamp of the restore",
        "envelope.*": (
            "non-content transport envelope fields (not the restored content)"
        ),
    }

    def compare(self, https: Any, uds: Any) -> list[tuple[str, Any, Any]]:
        # capture: {"restored_payload":{...}|null, "measurable":bool, "host_side_gap":str|null}
        diffs = _compare_restored_payload(
            https["restored_payload"],
            uds["restored_payload"],
            excluded=self.EXCLUDED,
        )
        if diffs:
            raise ParityMismatch(diffs)
        return diffs


# =============================================================================
# 6. Isolation (D6) — boolean-exact, EXCLUDED empty (security-load-bearing, NFR-6)
# =============================================================================
class IsolationComparator(DimensionComparator):
    """Per-slug isolation (D6) — isolation booleans compared EXACTLY (no tolerance).

    A tolerance here would mask a cross-tenant leak; per NFR-6 isolation probes are
    compared exactly. Additionally the isolation property must HOLD on each leg:
    `slug_a_writes_visible_to_b` must be False and `landed_only_in_a` must be True.
    """

    EXCLUDED = frozenset()  # boolean isolation property; no wall-clock field
    EXCLUSION_JUSTIFICATIONS = {}

    def compare(self, https: Any, uds: Any) -> list[tuple[str, Any, Any]]:
        # capture: {"slug_a_writes_visible_to_b":bool, "landed_only_in_a":bool}
        diffs: list[tuple[str, Any, Any]] = []
        # cross-leg parity: the isolation booleans must agree EXACTLY.
        for f in ("slug_a_writes_visible_to_b", "landed_only_in_a"):
            if https[f] != uds[f]:
                diffs.append((f, https[f], uds[f]))
        # security property: a VIOLATION on either leg is a parity-relevant divergence.
        for leg_label, cap in (("HTTPS", https), ("UDS", uds)):
            if cap["slug_a_writes_visible_to_b"]:
                diffs.append(
                    (f"{leg_label}.slug_a_writes_visible_to_b", True, False)
                )
            if not cap["landed_only_in_a"]:
                diffs.append((f"{leg_label}.landed_only_in_a", False, True))
        if diffs:
            raise ParityMismatch(diffs)
        return diffs


# =============================================================================
# 7. Cross-dimension drift guard (the structural SR-05/#5302 fix)
# =============================================================================
# Dimensions whose spec declares a NON-EMPTY justified exclusion set. The empty-set
# dimensions (behavioral/isolation) are transport-invariant booleans/strings.
_MUST_EXCLUDE = ("retrieval", "analytics", "proactive", "precompact")


def assert_comparator_contract(dimensions: tuple) -> None:
    """Off-Docker drift guard — raises AssertionError on ANY drift (#5302).

    The orchestrator runs this BEFORE any leg drives; the off-Docker suite runs it
    standalone. Asserts, over the bound `DIMENSIONS`:
      (a) every Dimension.comparator is a DimensionComparator SUBCLASS;
      (b) every EXCLUDED key appears in EXCLUSION_JUSTIFICATIONS (no unjustified
          exclusion — AC-09; vacuously true when both empty);
      (c) the should-exclude dimensions carry a NON-EMPTY justified EXCLUDED;
      (d) capture_keys are UNIQUE and (e) match the on-disk bundle schema keys;
      (f) wire_surface is one of the two constants;
      (g) FORBIDDEN_SEED_SITES IS the single C4' tuple object (no private copy).
    """
    seen_keys: set[str] = set()
    schema_keys = set(parity_dimensions.capture_keys())
    for dim in dimensions:
        comp = dim.comparator
        # (a) comparator bound to a DimensionComparator subclass.
        assert isinstance(comp, type) and issubclass(comp, DimensionComparator), (
            f"{dim.id}: comparator {comp!r} is not a DimensionComparator subclass "
            f"(bind_comparators not run, or a non-subclass bound)"
        )
        # (b) every EXCLUDED key has a justification.
        for k in comp.EXCLUDED:
            assert k in comp.EXCLUSION_JUSTIFICATIONS, (
                f"{comp.__name__}: unjustified exclusion {k!r} (AC-09)"
            )
        # (c) should-exclude dimensions carry a non-empty justified set.
        if dim.id in _MUST_EXCLUDE:
            assert comp.EXCLUDED, (
                f"{comp.__name__}: expected a non-empty justified EXCLUDED set"
            )
        # (d) capture_key uniqueness.
        assert dim.capture_key not in seen_keys, (
            f"duplicate capture_key {dim.capture_key!r}"
        )
        seen_keys.add(dim.capture_key)
        # (e) capture_key matches the on-disk bundle schema (no orphan key).
        assert dim.capture_key in schema_keys, (
            f"capture_key {dim.capture_key!r} has no matching bundle schema key"
        )
        # (f) wire_surface is one of the two constants.
        assert dim.wire_surface in (WIRE_MCP_BRIDGE, WIRE_HOOK_OBSERVE), (
            f"{dim.id}: unknown wire_surface {dim.wire_surface!r}"
        )
    # (g) ONE forbidden-seed set: this module's tuple IS the C4' tuple object.
    assert FORBIDDEN_SEED_SITES is parity_workload.FORBIDDEN_SEED_SITES, (
        "forbidden-seed set duplicated — must be the single C4' definition "
        "(SR-05/#5302)"
    )


# =============================================================================
# 8. Late-binding: resolve K1 registry comparator NAMES -> these classes (ONCE)
# =============================================================================
# K1's Dimension.comparator holds the class NAME (str) until bound. Call the
# K1-published hook ONCE at import so the registry resolves names -> classes and
# the drift guard / orchestrator subclass checks pass. Idempotent in K1.
parity_dimensions.bind_comparators(
    {
        "RetrievalComparator": RetrievalComparator,
        "AttributionComparator": AttributionComparator,
        "MetricVectorComparator": MetricVectorComparator,
        "BriefingComparator": BriefingComparator,
        "PreCompactComparator": PreCompactComparator,
        "IsolationComparator": IsolationComparator,
    }
)
