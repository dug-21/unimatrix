## ADR-003 nan-022: Comparator Framework as a Base Class + One Forbidden-Seed Set + Cross-Dimension Drift Guard (Structural SR-05/#5302 Fix)

### Context
SR-05 is the dominant maintenance hazard: six per-dimension exclusion sets, hand-authored on the
nan-021 `metric_comparator` template, drift silently — the EXACT #5302 lesson (single-source the
full CONTRACT, not just the shared DATA; nan-021 hit this family twice: cross-leg frame drift and
forbidden-seed-list drift, one caught mid-delivery, one at a gate). Six near-duplicate comparators
multiply the surface: each independently re-declares its `EXCLUDED` set, its justifications, its
diff-walking, and (worst) the forbidden-seed audit list. The #5302 takeaway is explicit:
convention ("conform to the nan-021 template") is NOT a guard — either single-source the whole
contract or add a cross-site equivalence test that fails loud on drift. nan-021's own Gate 3b
flagged the residual cross-language duplication as an unguarded parity-drift risk.

### Decision
Make the closed-set discipline a BASE CLASS + a SINGLE forbidden-seed set + a CROSS-DIMENSION
DRIFT GUARD — so the six comparators cannot drift from the discipline or from each other by
construction, not by convention.

(1) **`harness/parity_comparator.py` defines `DimensionComparator` (ABC).** The nan-021
`metric_comparator` shape is lifted to a base class. Every concrete comparator
(`MetricVectorComparator` wrapping the consumed nan-021 logic, plus `RetrievalComparator`,
`BriefingComparator`, `AttributionComparator`, `PreCompactComparator`, `IsolationComparator`) MUST
declare: `EXCLUDED: frozenset[str]` (closed, enumerated), `EXCLUSION_JUSTIFICATIONS: dict[str,str]`
(one inline justification per excluded member — the nan-021 pattern, AC-09), `compare(self, https,
uds) -> list[diff]` (field-for-field equality modulo `EXCLUDED`, raising `ParityMismatch` loud with
field + both values + leg on any non-excluded diff), and `evidence_record(self, https, uds, *,
run_token) -> dict` (the first-live-run field-by-field record, ADR-003 nan-021 generalized).

(2) **ONE forbidden-seed set.** `FORBIDDEN_SEED_SITES` is defined ONCE in
`parity_comparator.py` (generalizing `parity_workload.FORBIDDEN_SEED_SITES`); EVERY module on the
parity path is audited against that ONE tuple via `assert_no_seed_reachable`. No per-file copy of
the seed list exists (the #5302 forbidden-seed-drift instance, fixed structurally).

(3) **CROSS-DIMENSION DRIFT GUARD.** `assert_comparator_contract(DIMENSIONS)` — an OFF-DOCKER
test — asserts: every `Dimension.comparator` is a `DimensionComparator` subclass; each declares a
NON-EMPTY `EXCLUDED` whose every key appears in `EXCLUSION_JUSTIFICATIONS` (no unjustified
exclusion, AC-09); the forbidden-seed set is referenced from the single definition only; and the
registry's `capture_key`s are unique and match the bundle schema. This is the structural drift
detector #5302 demands — the gate/live-run is no longer the last-resort drift catcher.

(4) **Disposition authority carried verbatim** (nan-021 ADR-003 #5293 / NFR-8): any field OUTSIDE
a closed `EXCLUDED` that differs is a REAL failure surfaced LOUD; the implementer/tester NEVER
silently widens a set — disposition is a PRODUCT/HUMAN call (GH bug OR product-signed amendment
recorded via `context_correct`). The base class enforces this by making `ParityMismatch` the only
exit from a non-excluded diff.

### Consequences
Easier: the six comparators share ONE enforced shape; an unjustified or drifted exclusion fails an
off-Docker test BEFORE any tag round (#5258 seam discipline); the forbidden-seed list has one home,
so the #5302 "middle site missing from all copies" failure cannot recur; adding a dimension means
subclassing + one registry row, both checked by the guard. Harder: the ABC adds an abstraction layer
over the nan-021 procedural comparator (the `MetricVectorComparator` adapter must wrap the consumed
`compare_metric_vectors` without altering its logic — AC-04); the drift guard itself is load-bearing
and must be kept in the off-Docker lane; per-dimension `EXCLUDED` contents remain a product-disposed
artifact (the framework guards the SHAPE, not the membership — membership is still first-live-run +
human sign-off).

Related: SR-05, AC-09, NFR-8 (carried). Implements the #5302 lesson structurally. Consumes nan-021
ADR-003 (#5293) comparator logic. Depends on nan-022 ADR-001 (registry). Pairs with ADR-004 (the
ranking comparators' shared tolerance) and ADR-002 (the outcome classifier that calls `compare`).
