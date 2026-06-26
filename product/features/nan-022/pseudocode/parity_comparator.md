# K2 — Comparator framework + drift guard (`harness/parity_comparator.py`)

**New**, pure-Python, stdlib-only, off-Docker unit-testable. ADR-003 (#5307).

## Purpose

Lift the nan-021 `metric_comparator` shape into a base class so the five new dimensions cannot
drift from the closed-exclusion-set discipline (structural SR-05/#5302 fix, not convention).
Hosts the ONE `FORBIDDEN_SEED_SITES` set and the cross-dimension `assert_comparator_contract`
drift guard.

## Imports (consumed verbatim — AC-04)

```
from harness.metric_comparator import (
    compare_metric_vectors, EXCLUDED as MV_EXCLUDED,
    EXCLUSION_JUSTIFICATIONS as MV_JUSTIFICATIONS, ParityMismatch,
    field_by_field_record,
)
from harness.ranking_tolerance import ranking_parity, RankingVerdict   # K3
from harness.parity_dimensions import DIMENSIONS                       # for the drift guard only
```

`ParityMismatch` is re-exported from this module so callers have one import surface.

## Single forbidden-seed set (the #5302 single-source fix)

```
# Re-export the ONE definition that lives in C4' parity_workload.FORBIDDEN_SEED_SITES so there
# is exactly one tuple in the codebase. K2 does NOT define a private copy.
from harness.parity_workload import FORBIDDEN_SEED_SITES   # noqa: F401  (single source)
```

Rationale: nan-021 already defines `FORBIDDEN_SEED_SITES` + `assert_no_seed_reachable` in
`parity_workload.py`. To avoid a second copy (the SR-05 trap), K2 re-exports that tuple. The
drift guard asserts no module on the path carries a private duplicate.

## Base class

```
class DimensionComparator(ABC):
    EXCLUDED: frozenset[str]                 # closed, enumerated nondeterminism exclusions
    EXCLUSION_JUSTIFICATIONS: dict[str, str] # one inline justification per excluded member

    @abstractmethod
    def compare(self, https, uds) -> list[tuple[str, Any, Any]]:
        # field-for-field equality MODULO EXCLUDED; returns the (empty) diff list on parity;
        # raises ParityMismatch (field + both values + leg) on any non-excluded diff.
        ...

    def evidence_record(self, https, uds, *, run_token) -> dict:
        # default first-live-run field-by-field record (ADR-003 discipline, generalized).
        # Default impl: {run_token, dimension(=self.__class__.__name__), excluded_set,
        #                raw_https, raw_uds, diffs(non-raising shadow compare)}.
        # MetricVectorComparator overrides to delegate to field_by_field_record (verbatim).
```

Convention: every concrete subclass MUST declare a NON-EMPTY... — except where the dimension is
provably exclusion-free (behavioral, isolation), where `EXCLUDED` is empty. The drift guard
encodes this: an empty `EXCLUDED` is permitted ONLY for dimensions whose spec declares the set
empty (attribution/isolation are transport-invariant booleans/strings — no wall-clock field).
For those, `EXCLUSION_JUSTIFICATIONS` is also empty and the guard checks the
key-coverage invariant trivially (every key in EXCLUDED appears in JUSTIFICATIONS — vacuously
true when both empty). The guard's NON-EMPTY assertion applies to comparators that DO exclude
(retrieval, analytics, proactive, precompact).

## Concrete comparators

### `MetricVectorComparator` (analytics — wraps nan-021 verbatim, R-14)

```
class MetricVectorComparator(DimensionComparator):
    EXCLUDED = MV_EXCLUDED                      # the nan-021 closed 3-field set, unchanged
    EXCLUSION_JUSTIFICATIONS = MV_JUSTIFICATIONS

    def compare(self, https, uds):
        # analytics capture shape: {"metric_vector":{...}, "informs_edges":[...], "phase_signal":{...}}
        diffs = compare_metric_vectors(https["metric_vector"], uds["metric_vector"])  # verbatim; raises ParityMismatch internally
        # NET-NEW analytics surfaces (AC-04): Informs edges + phase signal.
        diffs += _compare_informs_edges(https["informs_edges"], uds["informs_edges"])
        diffs += _compare_phase_signal(https["phase_signal"], uds["phase_signal"])
        if diffs:
            raise ParityMismatch(diffs)
        return diffs

    def evidence_record(self, https, uds, *, run_token):
        # delegate the MetricVector slice to the consumed field_by_field_record verbatim,
        # then attach the informs/phase rows. Do NOT re-author the MetricVector record.
        rec = field_by_field_record(https["metric_vector"], uds["metric_vector"], run_token=run_token)
        rec["informs_edges_https"] = https["informs_edges"]; rec["informs_edges_uds"] = uds["informs_edges"]
        rec["phase_signal_https"] = https["phase_signal"]; rec["phase_signal_uds"] = uds["phase_signal"]
        return rec
```

`_compare_informs_edges`: compare the edge-ID set as an UNORDERED SET, IDs EXACT (NFR-6). Any
wall-clock/ordering edge field (e.g. creation timestamp) is a justified `EXCLUDED` entry; the
edge-ID set itself is NEVER excluded (R-11). `_compare_phase_signal`: compare EXACTLY.
IMPORTANT: edges/phase are only compared AFTER the durability barrier guarantees they landed —
the leg drivers gate the CAPTURE behind the barrier (R-04/R-11); the comparator itself assumes a
settled capture. If a capture is `None`/absent (barrier not satisfied), that is INFRA-ERROR
upstream in K4 `classify_dimension`, never compared here.

### `RetrievalComparator` (D1 — uses K3)

```
class RetrievalComparator(DimensionComparator):
    EXCLUDED = frozenset({"tail_churn", "score_jitter_beyond_prefix", "tie_order_within_class"})
    EXCLUSION_JUSTIFICATIONS = {
        "tail_churn": "HNSW approximate top-k membership flip below the stable prefix (#4990/GH#746) — intra-transport, not cross-transport divergence",
        "score_jitter_beyond_prefix": "per-process embedding score jitter beyond the stable ranked prefix",
        "tie_order_within_class": "equal-score tie ordering (#2610 HashMap / sort_unstable) — compared as an unordered tie-class, not positionally",
    }

    def compare(self, https, uds):
        # capture shape: {"queries":[{"tool","args","result_ids","scores"}...], "capture_2":[...]}
        diffs = []
        # one query set; compare per query by index (queries are ordered + identical by manifest).
        for i, (q_https, q_uds) in enumerate(zip(https["queries"], uds["queries"])):
            verdict = ranking_parity(q_https["result_ids"], q_uds["result_ids"],
                                     scores=(q_https.get("scores"), q_uds.get("scores")))
            if not verdict.matched:
                diffs.append((f"query[{i}].stable_prefix", q_https["result_ids"], q_uds["result_ids"]))
        if diffs:
            raise ParityMismatch(diffs)
        return diffs
```

NOTE the disposition (load-bearing, R-01): the tie-class tolerance MUST be scrutinized at first
live run so it cannot swallow a real cross-transport ranking divergence. An unachievable exact
order (no HNSW seed API) is a FILED BUG + documented C0 exception, never a quiet widening. The
implementer/tester does NOT widen `EXCLUDED`; that is a product/human `context_correct`.

### `BriefingComparator` (D4 — uses the SAME K3 policy)

```
class BriefingComparator(DimensionComparator):
    EXCLUDED = frozenset({"injection_history_timestamp", "tail_churn", "tie_order_within_class"})
    EXCLUSION_JUSTIFICATIONS = {
        "injection_history_timestamp": "wall-clock session-state injection-history stamp",
        "tail_churn": "ranked-prefix tail churn (shared D1/D4 entropy class — #4990/GH#746)",
        "tie_order_within_class": "equal-score tie ordering compared as an unordered tie-class",
    }

    def compare(self, https, uds):
        # capture shape: {"briefing_ids":[...], "briefing_scores":[...], "injection_set":[...], "capture_2":{...}}
        diffs = []
        verdict = ranking_parity(https["briefing_ids"], uds["briefing_ids"],
                                 scores=(https.get("briefing_scores"), uds.get("briefing_scores")))
        if not verdict.matched:
            diffs.append(("briefing.stable_prefix", https["briefing_ids"], uds["briefing_ids"]))
        # injection set: unordered SET equality, IDs exact (NFR-6) — NOT ranked.
        if set(https["injection_set"]) != set(uds["injection_set"]):
            diffs.append(("injection_set", sorted(https["injection_set"]), sorted(uds["injection_set"])))
        if diffs:
            raise ParityMismatch(diffs)
        return diffs
```

Single-sourcing (NFR-4/SR-03): both `RetrievalComparator` and `BriefingComparator` call the SAME
`ranking_parity`; there is NO second tie policy.

### `AttributionComparator` (D2 — string-exact, EXCLUDED empty)

```
class AttributionComparator(DimensionComparator):
    EXCLUDED = frozenset()                  # attribution is transport-invariant; no wall-clock field
    EXCLUSION_JUSTIFICATIONS = {}

    def compare(self, https, uds):
        # capture shape: {"topic_signals":[...]}
        s_https, s_uds = set(https["topic_signals"]), set(uds["topic_signals"])
        diffs = []
        if "unattributed" in s_https or "unattributed" in s_uds:
            diffs.append(("topic_signals.unattributed", sorted(s_https), sorted(s_uds)))  # HARD fail
        if s_https != s_uds:
            diffs.append(("topic_signals", sorted(s_https), sorted(s_uds)))
        if diffs:
            raise ParityMismatch(diffs)
        return diffs
```

### `PreCompactComparator` (D5 — measurability-aware, ADR-006)

```
class PreCompactComparator(DimensionComparator):
    EXCLUDED = frozenset({"restoration_timestamp", "envelope.*"})
    EXCLUSION_JUSTIFICATIONS = {
        "restoration_timestamp": "wall-clock stamp of the restore",
        "envelope.*": "non-content transport envelope fields (not the restored content)",
    }

    def compare(self, https, uds):
        # capture shape: {"restored_payload":{...}|null, "measurable":bool, "host_side_gap":str|null}
        # MEASURABILITY is handled in K4 BEFORE compare runs:
        #   - measurable=False on either leg -> K4 records a DOCUMENTED-EXCEPTION, compare NOT called.
        #   - measurable=True on both -> compare the restored payload here.
        # This method assumes both legs are measurable with non-null payloads.
        diffs = _compare_restored_payload(https["restored_payload"], uds["restored_payload"],
                                          excluded=self.EXCLUDED)
        if diffs:
            raise ParityMismatch(diffs)
        return diffs
```

`_compare_restored_payload`: compare the set of restored entry ids + their restored
content/order fields, byte-equal modulo `EXCLUDED`. Equality is over the SERVER-restored
content, not host-side presentation. The measurability branch is K4's responsibility (see
parity_outcome.md); the comparator never sees `measurable=False`.

### `IsolationComparator` (D6 — boolean-exact, EXCLUDED empty, security-load-bearing)

```
class IsolationComparator(DimensionComparator):
    EXCLUDED = frozenset()                  # boolean isolation property; no wall-clock field
    EXCLUSION_JUSTIFICATIONS = {}

    def compare(self, https, uds):
        # capture shape: {"slug_a_writes_visible_to_b":bool, "landed_only_in_a":bool}
        diffs = []
        for f in ("slug_a_writes_visible_to_b", "landed_only_in_a"):
            if https[f] != uds[f]:                 # EXACT boolean compare (NFR-6, no tolerance)
                diffs.append((f, https[f], uds[f]))
        # additionally: the isolation property must HOLD on each leg (security) —
        #   slug_a_writes_visible_to_b must be False and landed_only_in_a must be True.
        # A property VIOLATION on either leg is a parity-relevant divergence surfaced here.
        if diffs:
            raise ParityMismatch(diffs)
        return diffs
```

## Drift guard (the structural SR-05/#5302 fix)

```
def assert_comparator_contract(DIMENSIONS) -> None:
    # off-Docker; raises AssertionError on any drift. The orchestrator runs this BEFORE
    # any leg drives (and the off-Docker test suite runs it standalone).
    seen_keys = set()
    for dim in DIMENSIONS:
        # (a) comparator is a DimensionComparator subclass
        assert issubclass(dim.comparator, DimensionComparator)
        c = dim.comparator
        # (b) every EXCLUDED key has a justification (vacuously true if both empty)
        for k in c.EXCLUDED:
            assert k in c.EXCLUSION_JUSTIFICATIONS, f"{c.__name__}: unjustified exclusion {k!r} (AC-09)"
        # (c) comparators that exclude must justify NON-emptily (retrieval/analytics/proactive/precompact)
        if dim.id in ("retrieval", "analytics", "proactive", "precompact"):
            assert c.EXCLUDED, f"{c.__name__}: expected a non-empty justified EXCLUDED set"
        # (d) capture_key uniqueness
        assert dim.capture_key not in seen_keys, f"duplicate capture_key {dim.capture_key!r}"
        seen_keys.add(dim.capture_key)
        # (e) wire_surface is one of the two constants
        assert dim.wire_surface in (WIRE_MCP_BRIDGE, WIRE_HOOK_OBSERVE)
    # (f) ONE forbidden-seed set: this module's FORBIDDEN_SEED_SITES IS the C4' tuple object.
    assert FORBIDDEN_SEED_SITES is parity_workload.FORBIDDEN_SEED_SITES, \
        "forbidden-seed set duplicated — must be the single C4' definition (SR-05/#5302)"
```

## Error handling

- `compare()` raises `ParityMismatch` (loud, field + both values) on any non-excluded diff —
  the K4 classifier catches it to produce `PARITY_FAIL`.
- `assert_comparator_contract` raises `AssertionError` off-Docker, failing the build before any
  tag round (#5258).
- A capture with a missing required sub-key reaching `compare` is a programming error; K4 guards
  against missing/null captures BEFORE calling `compare`, so a `KeyError` here would indicate a
  contract bug surfaced loud (acceptable — never a silent pass).

## Key test scenarios (hints)

- `assert_comparator_contract(DIMENSIONS)` passes for the real registry; fails when fed a
  comparator with an unjustified exclusion, an empty EXCLUDED on a should-exclude dimension, a
  duplicated capture_key, or a private forbidden-seed copy (R-05 scenarios).
- `MetricVectorComparator` produces the IDENTICAL diff list as `compare_metric_vectors` on a
  golden nan-021 MetricVector pair (R-14 — adapter behavior-identical).
- `RetrievalComparator`/`BriefingComparator` both call `ranking_parity` (single-sourced — assert
  by import/inspection that no second tolerance exists, R-07 scenario 4).
- `AttributionComparator`: differing topic_signal sets -> ParityMismatch; `unattributed` present
  -> HARD ParityMismatch.
- `IsolationComparator`: differing isolation booleans -> ParityMismatch (security, no tolerance).
- `PreCompactComparator`: two restored payloads differing in a non-excluded content field ->
  ParityMismatch; differing only in `restoration_timestamp` -> clean.
- `_compare_informs_edges`: edge-ID set compared unordered, IDs exact; a missing edge ID -> diff;
  a differing wall-clock edge field -> excluded (justified).
