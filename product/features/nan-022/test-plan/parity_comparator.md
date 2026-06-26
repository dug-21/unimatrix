# Test Plan: K2 — `harness/parity_comparator.py`

Covers **R-05 (High)**, R-08 (PreCompact call-out), R-14 (adapter logic — shared with
`metric_comparator.md`), and the structural half of R-15 (one forbidden-seed set). The
`DimensionComparator` ABC + six concrete comparators + ONE `FORBIDDEN_SEED_SITES` + the
off-Docker `assert_comparator_contract` drift guard. This is the structural #5302 fix — the
drift guard is the LAST-RESORT-replacing structural detector: an unjustified or drifted
exclusion must fail an off-Docker test BEFORE any tag round (#5258), not at the gate.

Surface under test:
- `DimensionComparator(ABC)` — `EXCLUDED: frozenset[str]`, `EXCLUSION_JUSTIFICATIONS: dict[str,str]`,
  `compare(self, https, uds) -> list[tuple[str,Any,Any]]` (raises `ParityMismatch`),
  `evidence_record(self, https, uds, *, run_token) -> dict`
- `MetricVectorComparator`, `RetrievalComparator`, `BriefingComparator`, `AttributionComparator`,
  `PreCompactComparator`, `IsolationComparator`
- `FORBIDDEN_SEED_SITES: tuple[str, ...]`
- `assert_comparator_contract(DIMENSIONS) -> None`
- `ParityMismatch`

Tier: **A (off-Docker unit)** — synthetic capture dicts. File: `suites/test_parity_comparator.py`.

## Unit Test Expectations

### Cross-dimension drift guard (R-05 scenarios 1–4 — the structural SR-05 fix, AC-09)
- `test_assert_comparator_contract_passes_on_clean_registry`: the real `DIMENSIONS` passes the
  guard (every comparator is a `DimensionComparator` subclass; each `EXCLUDED` is non-empty-or-
  justified-empty; every `EXCLUDED` key has an `EXCLUSION_JUSTIFICATIONS` entry; capture_keys
  unique and match the bundle schema; seed set single-sourced).
- `test_assert_comparator_contract_fails_unjustified_exclusion`: inject a stub comparator with an
  `EXCLUDED` key absent from `EXCLUSION_JUSTIFICATIONS` → `assert_comparator_contract` FAILS loud
  off-Docker (R-05 scenario 4 — before any tag round). **Load-bearing false-GREEN guard.**
- `test_assert_comparator_contract_fails_non_subclass_comparator`: a `Dimension.comparator` that is
  not a `DimensionComparator` subclass → guard FAILS.
- `test_assert_comparator_contract_fails_capture_key_schema_mismatch`: a registry capture_key with
  no matching on-disk bundle schema key (orphan key / unhandled dimension) → guard FAILS (R-05
  scenario 3).
- `test_forbidden_seed_sites_single_definition`: assert `FORBIDDEN_SEED_SITES` is defined ONCE here
  and no per-comparator/per-file private copy exists (import-graph / module-attribute identity
  assertion; pairs with the grep-style check in `parity_workload.md`). R-05 scenario 2.

### Per-comparator EXCLUDED discipline (AC-09 + NFR-6 determinism floor)
- `test_attribution_comparator_excluded_is_empty`: `AttributionComparator.EXCLUDED` is empty — D2
  `topic_signal` is string-exact, no wall-clock field (AC-03). Exact compare, no tolerance.
- `test_isolation_comparator_excluded_is_empty_exact`: `IsolationComparator` compares the isolation
  boolean EXACTLY (NFR-6, AC-07) — empty `EXCLUDED`; a missing probe is not its concern (INFRA via
  classifier). Security-sensitive: a tolerance here would mask a cross-tenant leak.
- `test_metric_vector_comparator_excluded_matches_consumed_set`: `MetricVectorComparator.EXCLUDED`
  IS the consumed nan-021 `EXCLUDED` (not a re-declared copy) — AC-04 (see `metric_comparator.md`).
- `test_retrieval_briefing_excluded_justified`: `RetrievalComparator`/`BriefingComparator` carry the
  ranking-tolerance entry as an enumerated JUSTIFIED `EXCLUDED` member (AC-09, the R-01 tolerance is
  a justified exclusion, not an implicit loosening).

### compare() raises loud on non-excluded diff (C-4 disposition authority, NFR-8)
- `test_compare_non_excluded_diff_raises_parity_mismatch`: a non-excluded field diff → `compare`
  raises `ParityMismatch` carrying field + both values + leg (the ONLY exit from a non-excluded
  diff; the base class enforces no silent widening).
- `test_compare_excluded_field_diff_tolerated`: a diff confined to an `EXCLUDED` field → `compare`
  returns clean (modulo the closed set).

### Ranking comparators single-source K3 (R-07 scenario 4 / SR-03)
- `test_retrieval_and_briefing_import_same_ranking_parity`: assert both `RetrievalComparator` and
  `BriefingComparator` delegate to `ranking_tolerance.ranking_parity` — no second tie policy
  (paired with `ranking_tolerance.md`).

### PreCompact measurability call-out (R-08 scenarios 1–2, AC-06)
- `test_precompact_comparator_measurable_false_is_documented_callout_not_pass`: a `precompact`
  capture with `measurable=False` + non-null `host_side_gap` → the comparator/result is a
  DOCUMENTED MEASURABILITY LIMITATION (a distinct, visible outcome the roll-up records), NOT
  `PARITY_PASS` and NOT silently green. **Load-bearing false-GREEN guard on D5.**
- `test_precompact_comparator_measurable_true_field_compare`: `measurable=True` with two restored
  payloads → field-for-field compare modulo the closed wall-clock/ordering `EXCLUDED` → PASS/FAIL
  normally.
- `test_precompact_comparator_null_payload_only_with_measurable_false`: `restored_payload=null` is
  permitted ONLY with `measurable=False`; a null payload with `measurable=True` is an error/INFRA
  signal, not a pass.

### evidence_record (AC-10 first-live-run record)
- `test_evidence_record_field_by_field`: `evidence_record(https, uds, run_token=...)` returns a
  field-by-field dict keyed by run_token — the first-live-run evidence emitted on a PARITY-FAIL
  (the ADR-003 nan-021 discipline generalized per dimension).

## Coverage Requirement (from R-05)
The cross-dimension drift guard runs OFF-Docker and fails loud on an unjustified exclusion, a
duplicated forbidden-seed list, a non-subclass comparator, or a capture_key/schema mismatch —
all proven by negative tests before any tag round. Every per-dimension `EXCLUDED` is closed,
enumerated, and individually justified (AC-09); non-D1/D4 comparators carry empty/exact
exclusion sets (NFR-6); the PreCompact `measurable=False` path is a visible call-out, never a pass.
