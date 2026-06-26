# MC — MetricVector comparator (`harness/metric_comparator.py`)

**CONSUMED VERBATIM — DO NOT RE-AUTHOR (AC-04 / R-14).** No pseudocode body; this file documents
the consumed surface and the ONLY permitted interaction (a thin K2 wrapper).

## Purpose

The proven nan-021 analytics-dimension comparator. nan-022 wraps it UNCHANGED as the analytics
`DimensionComparator` subclass (`MetricVectorComparator` in K2). Its logic, exclusion set, and
evidence record are re-used exactly; re-proving/re-authoring is forbidden.

## Consumed surface (exact, from the existing module)

```
EXCLUDED: frozenset[str]                      # the closed 3 wall-clock fields (computed_at,
                                              #   universal.total_duration_secs, phases.*.duration_secs)
EXCLUSION_JUSTIFICATIONS: dict[str, str]      # one inline justification per excluded field
UNIVERSAL_FIELDS: tuple[str, ...]             # the 21 classified UniversalMetrics fields
AT_RISK_FIELDS: tuple[str, ...]               # session-lifecycle prime-suspect fields
class ParityMismatch(AssertionError)          # field + both values + leg; re-exported by K2
def assert_non_empty(mv, label) -> None       # structural non-empty AFTER the barrier
def compare_metric_vectors(mv_https, mv_uds) -> list[tuple[str,Any,Any]]   # raises ParityMismatch
def field_by_field_record(mv_https, mv_uds, *, run_token) -> dict
def write_field_record(record, path) -> Path
```

## The ONLY permitted interaction

K2 `MetricVectorComparator` (see parity_comparator.md) wraps these:
- `MetricVectorComparator.EXCLUDED = EXCLUDED` (the same object).
- `MetricVectorComparator.compare` delegates the MetricVector slice to `compare_metric_vectors`
  VERBATIM (then adds the NET-NEW Informs-edge + phase comparison — those are nan-022 surfaces,
  not changes to this module).
- `MetricVectorComparator.evidence_record` delegates the MetricVector slice to
  `field_by_field_record` VERBATIM (then attaches informs/phase rows).

No edit to this module. The adapter must be behavior-IDENTICAL to the consumed comparator on a
golden nan-021 MetricVector pair.

## Error handling

- Unchanged: `compare_metric_vectors` raises `ParityMismatch` on any non-excluded diff;
  `assert_non_empty` asserts structural non-emptiness after the barrier.

## Key test scenarios (hints)

- The K2 adapter produces the IDENTICAL diff list as `compare_metric_vectors` on a golden pair
  (R-14 — adapter behavior-identical; this is the only test that touches MC and it asserts
  NON-modification).
- No nan-022 change to this file's source (cumulative/no-fork; `git diff` shows it untouched
  except possibly an unchanged import surface).
