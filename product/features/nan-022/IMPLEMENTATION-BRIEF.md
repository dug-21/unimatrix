# nan-022 — Cross-Transport Parity Suite (C0 Proof Artifact): Implementation Brief

GH Issue #837. TEST-ONLY. Generalizes the nan-021 single-`MetricVector` parity gate
(`product/test/infra-001/`) into a **dimension-keyed parity matrix** that drives ONE
canonical workload over BOTH transports (HTTPS bridge / stdio-UDS) in ONE pytest
invocation and asserts measured parity across all six C0 (#5304) dimensions.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-022/SCOPE.md |
| Scope Risk Assessment | product/features/nan-022/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/nan-022/specification/SPECIFICATION.md |
| Architecture | product/features/nan-022/architecture/ARCHITECTURE.md |
| ADR-001 Dimension-keyed parity matrix | product/features/nan-022/architecture/ADR-001-dimension-keyed-parity-matrix.md |
| ADR-002 Outcome-class model | product/features/nan-022/architecture/ADR-002-outcome-class-model.md |
| ADR-003 Comparator framework + drift guard | product/features/nan-022/architecture/ADR-003-comparator-framework-drift-guard.md |
| ADR-004 Ranking tolerance policy | product/features/nan-022/architecture/ADR-004-ranking-tolerance-policy.md |
| ADR-005 Two-HTTPS-surface routing | product/features/nan-022/architecture/ADR-005-two-https-surface-routing.md |
| ADR-006 PreCompact restoration parity | product/features/nan-022/architecture/ADR-006-precompact-restoration-parity.md |
| ADR-007 Augmented workload seed corpus | product/features/nan-022/architecture/ADR-007-augmented-workload-seed-corpus.md |
| Risk-Test Strategy | product/features/nan-022/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-022/ALIGNMENT-REPORT.md |

## Component Map

All components live under `product/test/infra-001/` — cumulative extension, no fork, no
parallel scaffold (AC-11/NFR-1/NFR-2). New modules (K1–K5) are pure-Python, stdlib-only,
off-Docker unit-testable. Pseudocode and test-plan files are produced in Session 2 Stage 3a;
paths below are the expected components from the architecture.

| Component | Module / Location | New / Ext | Pseudocode | Test Plan |
|-----------|-------------------|-----------|-----------|-----------|
| K1 Dimension registry | `harness/parity_dimensions.py` | New | pseudocode/parity_dimensions.md | test-plan/parity_dimensions.md |
| K2 Comparator framework | `harness/parity_comparator.py` | New | pseudocode/parity_comparator.md | test-plan/parity_comparator.md |
| K3 Ranking tolerance policy | `harness/ranking_tolerance.py` | New | pseudocode/ranking_tolerance.md | test-plan/ranking_tolerance.md |
| K4 Outcome-class model | `harness/parity_outcome.py` | New | pseudocode/parity_outcome.md | test-plan/parity_outcome.md |
| K5 Transport-health preflight | `harness/transport_health.py` | New | pseudocode/transport_health.md | test-plan/transport_health.md |
| C4′ Workload manifest (augmented) | `harness/parity_workload.py` | Ext | pseudocode/parity_workload.md | test-plan/parity_workload.md |
| C3′ Leg drivers (bundle) | `harness/parity_legs.py` | Ext | pseudocode/parity_legs.md | test-plan/parity_legs.md |
| MC MetricVector comparator | `harness/metric_comparator.py` | Consumed | pseudocode/metric_comparator.md | test-plan/metric_comparator.md |
| C2′ HTTPS bridge driver | `scripts/bridge-cycle-driver.js` | Ext | pseudocode/bridge-cycle-driver.md | test-plan/bridge-cycle-driver.md |
| C5′ HTTPS smoke gate | `scripts/cloud-cycle-lib.sh` (`cloud_cycle_gates`) | Ext | pseudocode/cloud-cycle-lib.md | test-plan/cloud-cycle-lib.md |
| ORCH Parity-matrix orchestrator | `suites/test_https_uds_parity.py` (+ sibling matrix test) | Ext | pseudocode/test_https_uds_parity.md | test-plan/test_https_uds_parity.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Generalize nan-021's single-output HTTPS-vs-UDS parity gate into a dimension-keyed parity
matrix that drives ONE canonical workload over BOTH transports in ONE pytest invocation and
asserts *measured* parity across all six C0 dimensions (retrieval, behavioral signals,
analytics/learning, proactive delivery, PreCompact restoration, per-slug isolation).
Passing the full matrix is the behavioral evidence an authorized session uses to flip C0
(#5304) → `proven`; this feature does NOT itself flip C0, fix any defect it surfaces, or
change any production code.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| One → N generalization shape | One authoritative dimension registry (`DIMENSIONS`); leg drivers return a dimension-keyed bundle; orchestrator runs one comparator per dimension; analytics is CONSUMED, not re-proven | SR-04, AC-01..AC-08 | architecture/ADR-001-dimension-keyed-parity-matrix.md |
| Failure-class separation | Four-valued outcome model (PARITY-PASS / PARITY-FAIL / INFRA-ERROR / INTRA-TRANSPORT-NONDETERMINISM); ordered classifier INFRA→INTRA→PARITY; double-capture-and-diff for intra detection; bounded preflight deadlines for half-open hangs (the #839 class, now CLOSED — defense-in-depth) | SR-02, SR-04, OQ-2 | architecture/ADR-002-outcome-class-model.md |
| Comparator drift prevention | `DimensionComparator` ABC + one `FORBIDDEN_SEED_SITES` + off-Docker `assert_comparator_contract` drift guard (structural #5302 fix, not convention) | SR-05, AC-09 | architecture/ADR-003-comparator-framework-drift-guard.md |
| Ranking nondeterminism | ONE `ranking_parity` policy single-sourced across retrieval + briefing; stable-prefix signal + unordered tie-class; HNSW tail churn (#4990/GH#746) tolerated, not a defect. Tolerance MUST be scrutinized at first live run so it cannot swallow a real cross-transport divergence; an unachievable exact order (no HNSW seed API) is a FILED BUG + documented C0 exception, never a quiet widening (product/human-signed only). | SR-01, SR-03, AC-02, AC-05 | architecture/ADR-004-ranking-tolerance-policy.md |
| Two HTTPS wire surfaces | `Dimension.wire_surface` (`mcp_bridge`/`hook_observe`) drives explicit routing; #5298 11-frame conformance on both legs; missing capture → INFRA-ERROR (never empty-pass) | SR-08, AC-02, AC-03 | architecture/ADR-005-two-https-surface-routing.md |
| PreCompact measurability | Stays IN scope on `/observe`; capture carries `measurable`/`host_side_gap`. D5 may legitimately be "measured-where-drivable + documented host-side gap" rather than a full symmetric measurement (the harness cannot drive a live CC host) — a human-signed documented-exception that MUST be stated plainly for the flip session, never a silent drop, vacuous pass, or rounded up to "fully measured" | SR-07, OQ-3, AC-06 | architecture/ADR-006-precompact-restoration-parity.md |
| Non-degenerate ranking | Augment the single workload (OQ-1 option a) with a deterministic seed-corpus + query phase under ONE identity/token; seed CONTENT only, never the `topic_signal` output | SR-06, OQ-1, AC-02/05/07 | architecture/ADR-007-augmented-workload-seed-corpus.md |

## Files to Create / Modify

All under `product/test/infra-001/`.

**Create (K1–K5, pure-Python, off-Docker unit-testable):**
- `harness/parity_dimensions.py` — `Dimension`, `DIMENSIONS`, `WIRE_MCP_BRIDGE`, `WIRE_HOOK_OBSERVE`.
- `harness/parity_comparator.py` — `DimensionComparator` ABC + 6 concrete comparators, `FORBIDDEN_SEED_SITES`, `assert_comparator_contract`, `ParityMismatch` (or re-export).
- `harness/ranking_tolerance.py` — `ranking_parity`, `RankingVerdict`.
- `harness/parity_outcome.py` — `Outcome` enum, `DimensionResult`, `classify_dimension`, `intra_transport_stable`, `rollup`.
- `harness/transport_health.py` — `InfraError`, `preflight_leg` (and possibly `load_https_bundle`).

**Modify (extend in place, cumulative):**
- `harness/parity_workload.py` — augment `default_workload()` with seed-corpus + query phase (ONE manifest/identity/token preserved); generalize `load_https_vector` → `load_https_bundle`; extend `assert_no_seed_reachable` coverage.
- `harness/parity_legs.py` — extend `drive_uds_leg` to return the dimension bundle; per-dimension wire-surface routing; double-capture for intra-check dimensions.
- `scripts/bridge-cycle-driver.js` — add retrieval (`context_search`/`lookup`/`get`) + `context_briefing` `tools/call` envelopes through the existing bridge; emit into `dimension_bundle`. No net-new transport/cert/spawn code.
- `scripts/cloud-cycle-lib.sh` (`cloud_cycle_gates`) — write `{run_token, dimension_bundle:{...}}` to `$HTTPS_VECTOR_OUT` instead of `{run_token, metric_vector}`.
- `suites/test_https_uds_parity.py` (+ a sibling matrix test) — drive both legs once, ingest both bundles token-guarded, run every comparator, classify, emit the per-dimension evidence table keyed by run token.

**Consumed verbatim (do NOT re-author — AC-04):**
- `harness/metric_comparator.py` — `compare_metric_vectors`, `EXCLUDED`, `EXCLUSION_JUSTIFICATIONS`, `field_by_field_record`; wrapped unchanged by `MetricVectorComparator`.

## Data Structures

The six dimensions (single authoritative enumeration in `DIMENSIONS`):

| id | capture_key | wire_surface | comparator | intra-check | blocks_c0_proof |
|----|-------------|--------------|------------|-------------|-----------------|
| retrieval | `retrieval` | mcp_bridge | `RetrievalComparator` (uses K3) | yes | True |
| behavioral | `behavioral` | hook_observe | `AttributionComparator` | no | True |
| analytics | `analytics` | mcp_bridge + hook_observe | `MetricVectorComparator` (nan-021) | no | True |
| proactive | `proactive` | mcp_bridge | `BriefingComparator` (uses K3) | yes | True |
| precompact | `precompact` | hook_observe | `PreCompactComparator` | no | True |
| isolation | `isolation` | mcp_bridge + hook_observe | `IsolationComparator` | no | True |

`blocks_c0_proof` defaults `True` for all six — CONFIRMED correct and aligned (human, 2026-06-25):
the corrected C0 (#5304) `done_when` makes parity the total bar and the dimension list grows with
the pipeline without narrowing the bar; any unreachable dimension is a human-signed documented
exception. The data-only flag keeps a future re-disposition a data change, not a code change.

**Dimension bundle (on-disk cross-language contract, both legs emit identically):**
```
{"run_token": str,
 "dimension_bundle": {
   "retrieval":  {"queries":[{"tool","args","result_ids","scores"}...], "capture_2":[...]},
   "behavioral": {"topic_signals":[...]},
   "analytics":  {"metric_vector":{...}, "informs_edges":[...], "phase_signal":{...}},
   "proactive":  {"briefing_ids":[...], "briefing_scores":[...], "injection_set":[...], "capture_2":{...}},
   "precompact": {"restored_payload":{...}|null, "measurable":bool, "host_side_gap":str|null},
   "isolation":  {"slug_a_writes_visible_to_b":bool, "landed_only_in_a":bool}}}
```
Only `precompact.restored_payload` may be `null` (and only with `measurable=False`); any
other null/missing capture → INFRA-ERROR.

**Key types:**
- `Outcome` enum: `PARITY_PASS`, `PARITY_FAIL`, `INFRA_ERROR`, `INTRA_TRANSPORT_NONDETERMINISM`.
- `DimensionResult(dimension: str, outcome: Outcome, diffs: list, detail: str)`.
- `RankingVerdict(matched: bool, stable_prefix_len: int, tail_churn: list, tie_classes: list)`.
- `Dimension(id, capture_key, wire_surface, comparator, intra_transport_check, blocks_c0_proof)` — frozen dataclass.

## Function Signatures

New surfaces downstream agents implement (do not invent):

```python
# parity_comparator.py
class DimensionComparator(ABC):
    EXCLUDED: frozenset[str]
    EXCLUSION_JUSTIFICATIONS: dict[str, str]
    def compare(self, https, uds) -> list[tuple[str, Any, Any]]   # raises ParityMismatch
    def evidence_record(self, https, uds, *, run_token) -> dict
FORBIDDEN_SEED_SITES: tuple[str, ...]
def assert_comparator_contract(DIMENSIONS) -> None                # off-Docker drift guard

# ranking_tolerance.py
def ranking_parity(https_ids: list, uds_ids: list, *, scores=None) -> RankingVerdict

# parity_outcome.py
def classify_dimension(dim, cap_uds, cap_https) -> DimensionResult     # INFRA → INTRA → compare
def intra_transport_stable(cap_a, cap_b, *, tolerance) -> bool
def rollup(results: list[DimensionResult]) -> tuple[verdict, exit_code]

# transport_health.py
class InfraError(Exception): ...
def preflight_leg(leg, *, connect_deadline_s, idle_deadline_s) -> None  # raises InfraError
def load_https_bundle(out_path, expected_run_token) -> dict[str, Any]   # missing/stale/empty → InfraError
```

Consumed-verbatim surfaces (full inventory in ARCHITECTURE §7.1): `ParityWorkload`,
`ToolCall`, `default_workload`, `durability_barrier`, `observe_count`,
`compare_metric_vectors`, `EXCLUDED`/`EXCLUSION_JUSTIFICATIONS`, `ParityMismatch`,
`field_by_field_record`, `assert_no_seed_reachable`, `drive_uds_leg`,
`assert_derived_attribution`, `run_https_leg`, `PARITY_PHASE`, `UnimatrixUdsClient`,
`UnimatrixHookClient`, `cloud_cycle_gates`, `run_smoke_gate`, `bridge-cycle-driver.js`,
RecordEvent 11-frame sequence (#5298).

## Constraints

- **C-1 No production code.** Test-only, cumulative on `infra-001`; no fork, no parallel scaffold. Any `crates/**`, shipped-`lib/`, or production-script change is an automatic SCOPE-FAIL. (NFR-1/NFR-2, AC-11)
- **C-2 Bridge-in-path (D-2).** Drive `context_*` THROUGH the shipped `mcp-bridge.js` over pinned HTTPS; never POST `mcp_url` directly. Observe/PreCompact ride the pinned `/observe` route. Reuse `mcp-bridge.js`/`cert-pin.js`/`credstore.js`/`bundle.js`/`init.js` as-is; net-new transport/cert/spawn code is a fork smell to FLAG.
- **C-3 #5298 RecordEvent contract.** Every observe-driven dimension emits the byte-identical 11-frame hook sequence on both legs; `context_cycle_review`'s primary path reads hook-written rows, not MCP `context_cycle`; never the rework/legacy frame variants (they record nothing — the SR-08 vacuous-pass trap).
- **C-4 Closed-exclusion-set discipline + disposition authority.** Per dimension, exclusions are closed/justified; any non-excluded divergence is a PRODUCT/HUMAN call (GH bug OR product-signed amendment via `context_correct`), never a silent widen by implementer/tester. (NFR-8, AC-09)
- **C-5 Single-source the full CONTRACT.** ONE manifest, ONE identity, ONE token, ONE barrier helper, ONE comparator template, ONE forbidden-seed set, ONE ranking tolerance. (#5302/SR-05, NFR-3/NFR-4)
- **C-6 #830 coupling.** Rely on the shipped single-flight `keep_alive` self-heal; do NOT re-implement reconnection. It covers only SIGNALLED (404) eviction; the silent half-open-socket hang (#839 / #5303) it did not cover is now CLOSED (commit 5b6badad / PR #842, 2026-06-25), so the C0 precondition is met and delivery is UNBLOCKED. As defense-in-depth (not a gating dependency), any half-open hang must still surface as INFRA-ERROR via the bounded preflight/deadline, never a parity result.
- **C-7 CI lane.** Release-gate Docker lane via `workflow_dispatch`/tag, false-green-proof, skip-when-Docker-absent HARD-fails. NOT the JS-only `ci.yml` `pull_request` matrix.
- **C-8 Outcome-class separation.** Cross-transport divergence (PARITY-FAIL, RED) and intra-transport nondeterminism (separate filed bug, NOT this gate) MUST be structurally separated. (FR-10/FR-11)
- **C-9 Missing-capture errors.** A dimension routed to the wrong wire surface records nothing; that MUST ERROR (INFRA-ERROR), never empty-pass. (FR-12)
- **NFR-6 Determinism floor.** Counts, edge sets, attribution strings, isolation probes compared EXACTLY (no float tolerance); only retrieval + briefing admit the bounded tie-tolerance.

## Dependencies

- **#836 / nan-021** — consumed verbatim (`infra-001` substrate). HARD dependency.
- **#830 self-heal** — intentional coupling carried from nan-021 ADR-002; do not re-implement reconnection.
- **#839 / #5303** — CLOSED (commit 5b6badad / PR #842, 2026-06-25): the silent half-open-socket eviction it tracked is fixed; the C0 precondition is met and delivery is UNBLOCKED. No longer a gating dependency. The suite still classifies any half-open hang as INFRA-ERROR as defense-in-depth, never read as a verdict.
- **GH#746 / #4990** — HNSW approximate top-k flip from per-process OS entropy (`hnsw_rs` 0.3.4, no seed API), deferred; shapes the D1/D4 ranked-prefix tolerance and the INTRA-TRANSPORT-NONDETERMINISM class.
- **#2610** — HashMap iteration-order trap in ranking; absorbed by the K3 tie-class policy.
- **#5298** — canonical RecordEvent 11-frame contract every observe-driven dimension conforms to.
- **Environment:** Docker (engine 29.5.2, Compose v2.40.3, linux/arm64) for the live HTTPS/TLS leg; off-Docker seam keeps K1–K5 teeth unit-tested pre-tag. Python 3.11 + pytest; stdlib-only harness (zero new runtime deps).

## NOT in Scope

- No production-code changes. Any defect found is filed as a GH bug, not fixed (AC-10).
- Not a fix-it feature for any parity, determinism (SR-01/#4990/GH#746), or transport bug surfaced.
- Not a re-prove of nan-021's analytics slice — its comparator/workload/barrier consumed verbatim.
- Not a soak / load / performance test — parity of observable outcomes, not throughput.
- Not a Claude-Code-driven integration — workload driven by the harness/bridge, not a live CC host (relevant to the D5 host-side call-out).
- Does not broaden C0's surface beyond the six named dimensions, nor invent new server behavior.
- Does not itself amend any exclusion set silently — amendments are product-signed.
- Does not itself flip C0 (#5304) → `proven` — it produces the proof artifact; an authorized session performs the flip (AC-12).
- Not wired into the JS-only `ci.yml` `pull_request` matrix — release-gate lane only.

## Alignment Status

Vision guardian (ALIGNMENT-REPORT.md): **no VARIANCE/FAIL.** Vision Alignment, Milestone Fit,
Scope Gaps, Scope Additions, Architecture Consistency, and Risk Completeness all PASS. The
net-new constructs (outcome-class model, drift guard, transport-health preflight) are
scope-mandated SR mitigations, not surface expansion.

**C0 flip bar (six vs three) — RESOLVED, not a blocker (human-confirmed 2026-06-25).** The
formerly-WARN item is settled: the corrected C0 (#5304) `done_when` makes parity the total bar
— "the dimension list is the present expression of the parity bar and grows with the pipeline;
it does not narrow the bar," with any unreachable dimension a human-signed documented exception,
never silently excluded. So all SIX dimensions block the flip, and the design default
`blocks_c0_proof=True` for all six is CORRECT and ALIGNED — not a pending coin-flip. Encoded as
a **data-only registry flag** (`Dimension.blocks_c0_proof`) so a future re-disposition stays a
data change, not code. The documented-exception escape valve covers a legitimately unreachable
dimension (e.g. the D5 PreCompact host-side gap). This feature does NOT itself flip C0 (AC-12).

## Open Questions

- **OQ-1 — RESOLVED (C0 flip bar, OQ-6 / AC-12; human-confirmed 2026-06-25).** All SIX
  dimensions block the C0 flip; the corrected C0 (#5304) `done_when` makes parity the total bar
  (the dimension list grows with the pipeline and never narrows the bar), with any unreachable
  dimension a human-signed documented exception. Design default `blocks_c0_proof=True` for all
  six is correct — not a pending question. Re-disposition (should it ever be needed) is a
  data-only registry change. The flip itself remains out of scope for this feature.
- **OQ-2 (delivery-time, ADR-006 / OQ-3).** Is `CompactContext` restoration fully
  test-only-drivable from BOTH legs, or is there a host-side (CC) component the harness cannot
  drive? Resolve at first live drive. D5 may legitimately be "measured-where-drivable +
  documented host-side gap" rather than a full symmetric measurement — a human-signed
  documented-exception that MUST be stated plainly for the flip session, never rounded up to
  "fully measured" and never a silent drop.
- **OQ-3 (spec/delivery, ADR-007 / OQ-C).** Exact seed-corpus size + query set that makes
  retrieval and briefing rankings non-degenerate (stable-prefix floor N > 1) while preserving
  one identity/one token. Architecture fixes the shape; the concrete numbers are a Stage-3a
  test-design call.
- **OQ-4 (delivery-time, ADR-004 / OQ-D).** Are `Informs` edges + phase signal deterministic
  for the identical workload, or do they need a barrier/tolerance for tick/background timing?
  First live run decides; default is exact post-barrier with a justified exclusion only on
  product sign-off.
