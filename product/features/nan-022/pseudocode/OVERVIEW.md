# nan-022 Pseudocode — OVERVIEW

C0 proof artifact (#837). TEST-ONLY, cumulative on `product/test/infra-001/`. Generalizes
nan-021's single-`MetricVector` HTTPS-vs-UDS gate into a dimension-keyed parity matrix:
ONE workload, BOTH transports, ONE pytest invocation, six per-dimension verdicts.

All signatures below are FIXED by ARCHITECTURE §7 / brief Function Signatures. Implementation
agents use them exactly; do not invent new public surfaces. Where a surface is consumed from
nan-021 it is named verbatim (no re-author — AC-04/AC-11).

---

## Component set (brief Component Map)

| Component | Module | New/Ext | Pseudocode file |
|-----------|--------|---------|-----------------|
| K1 Dimension registry | `harness/parity_dimensions.py` | New | parity_dimensions.md |
| K2 Comparator framework | `harness/parity_comparator.py` | New | parity_comparator.md |
| K3 Ranking tolerance | `harness/ranking_tolerance.py` | New | ranking_tolerance.md |
| K4 Outcome-class model | `harness/parity_outcome.py` | New | parity_outcome.md |
| K5 Transport-health preflight | `harness/transport_health.py` | New | transport_health.md |
| C4' Workload (augmented) | `harness/parity_workload.py` | Ext | parity_workload.md |
| C3' Leg drivers (bundle) | `harness/parity_legs.py` | Ext | parity_legs.md |
| MC MetricVector comparator | `harness/metric_comparator.py` | Consumed | metric_comparator.md |
| C2' HTTPS bridge driver | `scripts/bridge-cycle-driver.js` | Ext | bridge-cycle-driver.md |
| C5' HTTPS smoke gate | `scripts/cloud-cycle-lib.sh` | Ext | cloud-cycle-lib.md |
| ORCH Parity-matrix orchestrator | `suites/test_https_uds_parity.py` | Ext | test_https_uds_parity.md |

---

## Component interaction / data flow

```
ORCH (pytest, ONE invocation)
  workload = default_workload()           # C4': augmented seed-corpus + query phase
  workload.validate()                     # one identity / one token preserved
  run_token = workload.session_id         # the SINGLE correlation token
  manifest  = workload.write_manifest(...)
  assert_comparator_contract(DIMENSIONS)  # K2 drift guard, off-Docker, BEFORE any drive

  UDS leg (in-process):                    HTTPS leg (shell-out):
    preflight_leg("uds", ...)  [K5]          preflight_leg("https", ...)  [K5]
    bundle_uds = drive_uds_leg(...)  [C3']   run_https_leg(...)  -> cloud_cycle_gates [C5']
      per dim: route to wire_surface           bridge-cycle-driver.js [C2'] issues
      double-capture where intra_check          context_search/lookup/get/briefing tools/call;
      durability_barrier (shared)               /observe route: behavioral/precompact/cycle frames
      -> {run_token, dimension_bundle}          writes {run_token, dimension_bundle} -> $HTTPS_VECTOR_OUT
                                              bundle_https = load_https_bundle(out, run_token) [C4'/K5]

  for dim in DIMENSIONS:                                       # K1 is the only enumeration
      results[dim] = classify_dimension(dim,
                        bundle_uds[dim.capture_key],
                        bundle_https[dim.capture_key])         # K4: INFRA -> INTRA -> compare
  verdict, exit_code = rollup(results)                          # K4 §4 roll-up
  emit per-dimension evidence table keyed by run_token
```

Cross-process seam is unchanged in SHAPE from nan-021: HTTPS leg writes a JSON file; pytest
ingests it token-guarded in the SAME invocation (live-vs-live, D-6). Only the payload widens
from `metric_vector` to `dimension_bundle`.

---

## The dimension bundle — cross-language contract (R-09, load-bearing)

Both legs emit BYTE-IDENTICALLY-SHAPED JSON. Python ingests it; JS/shell emits it. A key
typo / missing key / `null` where a dict is expected MUST become INFRA-ERROR, never an
empty-equals-empty pass.

```
{ "run_token": str,
  "dimension_bundle": {
    "retrieval":  { "queries": [ {"tool", "args", "result_ids", "scores"} ... ],
                    "capture_2": [ ... ] },                 # intra: second capture
    "behavioral": { "topic_signals": [ ... ] },             # string-exact; derived
    "analytics":  { "metric_vector": {...}, "informs_edges": [ ... ], "phase_signal": {...} },
    "proactive":  { "briefing_ids": [ ... ], "briefing_scores": [ ... ],
                    "injection_set": [ ... ], "capture_2": {...} },  # intra: second capture
    "precompact": { "restored_payload": {...} | null,
                    "measurable": bool, "host_side_gap": str | null },
    "isolation":  { "slug_a_writes_visible_to_b": bool, "landed_only_in_a": bool } } }
```

Rules enforced by `load_https_bundle` (HTTPS) and by `classify_dimension` (both legs):
- Every `Dimension.capture_key` in `DIMENSIONS` MUST be present in `dimension_bundle`.
- `restored_payload` is the ONLY value that may be `null`, and ONLY when `measurable=False`.
- Any other missing/`null`/empty capture -> `InfraError` / `Outcome.INFRA_ERROR`.

The contract is asserted on BOTH sides: off-Docker the Python ingest round-trips a fixture
bundle through every comparator (R-09); live the JS/shell-emitted bundle satisfies the same
schema.

---

## Shared types (defined once, used everywhere)

| Type | Owner module | Shape |
|------|--------------|-------|
| `Dimension` | K1 parity_dimensions | `@dataclass(frozen)`: `id, capture_key, wire_surface, comparator, intra_transport_check, blocks_c0_proof` |
| `DIMENSIONS` | K1 parity_dimensions | `tuple[Dimension, ...]` — the SIX, the single authoritative enumeration |
| `WIRE_MCP_BRIDGE`, `WIRE_HOOK_OBSERVE` | K1 parity_dimensions | `str` constants for `wire_surface` |
| `DimensionComparator` | K2 parity_comparator | ABC: `EXCLUDED: frozenset[str]`, `EXCLUSION_JUSTIFICATIONS: dict[str,str]`, `compare(self, https, uds) -> list[tuple[str,Any,Any]]`, `evidence_record(self, https, uds, *, run_token) -> dict` |
| `FORBIDDEN_SEED_SITES` | K2 parity_comparator | `tuple[str, ...]` — the ONE forbidden-seed set (re-exports C4' tuple) |
| `ParityMismatch` | MC metric_comparator (re-exported by K2) | `AssertionError` subclass (field + both values + leg) |
| `RankingVerdict` | K3 ranking_tolerance | `@dataclass`: `matched: bool, stable_prefix_len: int, tail_churn: list, tie_classes: list` |
| `Outcome` | K4 parity_outcome | `Enum`: `PARITY_PASS, PARITY_FAIL, INFRA_ERROR, INTRA_TRANSPORT_NONDETERMINISM` |
| `DimensionResult` | K4 parity_outcome | `@dataclass`: `dimension: str, outcome: Outcome, diffs: list, detail: str` |
| `InfraError` | K5 transport_health | `Exception` subclass (half-open hang / unreachable / empty capture) |

Consumed verbatim (NEVER re-authored): `ParityWorkload`, `ToolCall`, `default_workload`,
`durability_barrier`, `observe_count`, `DurabilityTimeout`, `compare_metric_vectors`,
`EXCLUDED`/`EXCLUSION_JUSTIFICATIONS`/`UNIVERSAL_FIELDS`/`AT_RISK_FIELDS`,
`field_by_field_record`/`write_field_record`, `assert_no_seed_reachable`,
`UnimatrixUdsClient`, `UnimatrixHookClient`, `PARITY_PHASE`, `run_https_leg`,
`assert_derived_attribution`, `cloud_cycle_gates`, `run_smoke_gate`, `bridge-cycle-driver.js`
core spawn/RPC machinery, the #5298 11-frame RecordEvent sequence.

---

## Outcome-class model (K4 §4) — structural separation, never conflated

Per dimension exactly ONE of four classes. Classifier order is FIXED: **INFRA -> INTRA -> PARITY**.

| Class | When | Gate effect |
|-------|------|-------------|
| `INFRA_ERROR` | K5 preflight tripped, deadline expired, capture missing/empty/stale/null (non-D5) | distinct ERROR exit; never green, never parity RED |
| `INTRA_TRANSPORT_NONDETERMINISM` | a leg's two captures self-diverge within the stable prefix (intra_check dims only) | recorded + filed separately (GH#746); does NOT redden the gate |
| `PARITY_FAIL` | both legs intra-stable; cross-leg comparator finds a non-excluded diff | RED; file NEW GH bug; fix NOT absorbed |
| `PARITY_PASS` | both legs captured; cross-leg comparator clean modulo closed set | green for that dimension |

Roll-up (`rollup`): GREEN iff every dimension is `PARITY_PASS`. Any `PARITY_FAIL` -> RED.
Any `INFRA_ERROR` -> distinct ERROR exit. `INTRA_TRANSPORT_NONDETERMINISM` recorded but does
not redden. D5 `measurable=False` + named `host_side_gap` is a DOCUMENTED-EXCEPTION call-out
in the table (never a vacuous pass, never silently green).

---

## Wire-surface routing (K1 `wire_surface`, ADR-005 / SR-08)

| Surface | HTTPS leg | UDS leg | Dimensions |
|---------|-----------|---------|------------|
| `WIRE_MCP_BRIDGE` | `mcp-bridge.js` JSON-RPC `tools/call` over pinned HTTPS | `UnimatrixUdsClient` MCP socket | retrieval, proactive, analytics(review read), isolation(read) |
| `WIRE_HOOK_OBSERVE` | pinned HTTPS POST `/observe` | `UnimatrixHookClient` hook IPC | behavioral, analytics(cycle frames), precompact, isolation(write) |

`analytics` and `isolation` touch BOTH surfaces — the leg drivers fan out explicitly. A
dimension routed to the wrong surface records NOTHING; the never-empty guard makes that an
`INFRA_ERROR` (R-03). Every observe-driven dimension emits the byte-identical #5298 11-frame
RecordEvent sequence (never the rework/legacy variants).

---

## Build / dependency ordering (wave recommendation)

K1–K5 + MC are pure-Python, stdlib-only, OFF-Docker unit-testable (the #5258 seam). The teeth
must be proven off-Docker before any release tag (#5267 / R-10). Order by dependency edges:

- **Wave A (foundations, no cross-deps within wave, fully parallel):**
  - K1 `parity_dimensions.py` — but its `comparator` field references K2 classes, so K1's
    `DIMENSIONS` table is finalized once K2's class NAMES exist. Author K1 constants/`Dimension`
    dataclass + `WIRE_*` first; wire the comparator references after K2.
  - K3 `ranking_tolerance.py` — depends on nothing in this feature. Independent.
  - K5 `transport_health.py` — `InfraError` + `preflight_leg`; independent. (`load_https_bundle`
    may live in C4' or K5; see parity_workload.md — it depends on `InfraError`.)
  - MC `metric_comparator.py` — CONSUMED unchanged; no work beyond confirming the wrap surface.

- **Wave B (comparators + registry close):**
  - K2 `parity_comparator.py` — `DimensionComparator` ABC, six concrete comparators
    (`MetricVectorComparator` wraps MC; `RetrievalComparator`/`BriefingComparator` use K3),
    `FORBIDDEN_SEED_SITES`, `assert_comparator_contract`. Depends on K3 (ranking) + MC (wrap) +
    K1 `DIMENSIONS` for the drift guard.
  - K1 finalize: bind `Dimension.comparator` to the K2 classes; freeze `DIMENSIONS`.
  - K4 `parity_outcome.py` — `Outcome`, `DimensionResult`, `classify_dimension`,
    `intra_transport_stable`, `rollup`. Depends on K2 (compare/ParityMismatch), K3 (tolerance),
    K5 (InfraError class).

- **Wave C (workload + leg drivers, off-Docker seam-testable):**
  - C4' `parity_workload.py` — augment `default_workload` (seed corpus + query phase),
    generalize `load_https_vector` -> `load_https_bundle`, extend `assert_no_seed_reachable`
    coverage, keep `FORBIDDEN_SEED_SITES` the single source K2 re-exports.
  - C3' `parity_legs.py` — extend `drive_uds_leg` to return the bundle; per-dimension wire-surface
    routing; double-capture for intra-check dims. Depends on C4', K1, K5.

- **Wave D (HTTPS leg, Docker-bound):**
  - C2' `bridge-cycle-driver.js` — add retrieval/briefing `tools/call` envelopes; emit into
    `dimension_bundle`. Depends on the bundle contract (OVERVIEW) + C5' wiring.
  - C5' `cloud-cycle-lib.sh` — assemble the dimension bundle (MetricVector + bridge retrieval/
    briefing + /observe behavioral/precompact/isolation) and write `{run_token, dimension_bundle}`.

- **Wave E (orchestrator):**
  - ORCH `test_https_uds_parity.py` — drive both legs once, ingest token-guarded, classify per
    dimension, roll up, emit the evidence table. Depends on ALL of the above. Keeps the existing
    `test_https_uds_parity` (MetricVector path) green; adds the sibling matrix test.

Sequencing rationale: K1's `DIMENSIONS.comparator` references force K2 before K1-final; K4's
classifier consumes K2+K3+K5; the leg drivers and HTTPS leg consume the bundle contract; ORCH
consumes everything. Off-Docker teeth (Waves A/B/C unit tests) precede any Docker tag round.

---

## Cross-cutting invariants (do not violate in any component)

- **C-5 single-source the full CONTRACT:** ONE manifest, ONE identity, ONE token, ONE barrier
  helper (`durability_barrier`), ONE comparator template (`DimensionComparator`), ONE
  forbidden-seed set (`FORBIDDEN_SEED_SITES`), ONE ranking tolerance (`ranking_parity`). No
  second copy of any of these.
- **NFR-6 determinism floor:** counts, edge sets, attribution strings, isolation booleans
  compared EXACTLY (no float tolerance). Only retrieval + briefing admit `ranking_parity`'s
  bounded tie-tolerance.
- **C-4 disposition authority:** a non-excluded divergence is a PRODUCT/HUMAN call (GH bug OR
  product-signed `context_correct` amendment). No implementer/tester widens an exclusion set.
- **C-1/C-2 no fork:** test-only diff confined to `product/test/infra-001/`; reuse shipped
  `mcp-bridge.js`/`cert-pin.js`/`credstore.js`/`bundle.js`/`init.js`; net-new transport/cert/
  spawn code is a fork smell to FLAG.

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern + decision) — surfaced the nan-022 ADR set
  (#5305 ADR-001, #5307 ADR-003, #5313 ADR-002) and generic test-plan patterns (#2928, #3776,
  #5175); ADR detail read from the architecture/brief (sufficient, no `context_get` needed).
- Deviations from established patterns: none. The four-valued outcome model, the
  `DimensionComparator` base + single forbidden-seed drift guard, and the transport-health
  preflight are scope-mandated SR mitigations following the nan-021 `metric_comparator` /
  single-source-the-contract precedent, not surface expansion.
