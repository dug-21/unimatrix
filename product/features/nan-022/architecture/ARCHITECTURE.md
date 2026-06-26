# nan-022 — Cross-Transport Parity Suite: Architecture

C0's proof artifact (#837). TEST-ONLY. Generalizes the nan-021 single-`MetricVector`
parity gate (`product/test/infra-001/`) into a **dimension-keyed parity matrix** that
drives ONE canonical workload over BOTH transports (HTTPS bridge / stdio-UDS) in ONE
pytest invocation and asserts measured parity across all six C0 dimensions.

This document is the high-level design and integration surface. Each significant decision
is captured in a separate `ADR-NNN-*.md` (also stored in Unimatrix). It builds directly on
the nan-021 ADR set (#5286/5294/5293/5291/5289/5290) and consumes that substrate verbatim.

---

## 1. System Overview

### Where this fits

`personal-cloud` (#5304, goal C0) promises: "for a remote slug, retrieval AND behavioral
signals AND analytics/learning function at parity with a local-UDS deployment — measured,
not asserted." nan-021 (#836) built the HTTPS-bridge fixture and **measured exactly one**
of the six dimensions (analytics/learning, via a single live-vs-live `MetricVector`
comparison). nan-022 is the parity *suite* on top of that fixture: it proves the other five
and re-consumes the one, producing the behavioral evidence an authorized session uses to
flip C0 → `proven`. It does not itself flip C0 (AC-12).

### The generalization in one sentence

nan-021 captured **one output** (`MetricVector`) from each leg and ran **one comparator**
(`compare_metric_vectors`) modulo **one closed exclusion set** (`EXCLUDED`). nan-022 keeps
the exact same one-workload / one-identity / one-token / pytest-as-orchestrator / closed-
justified-exclusion-set machinery and turns the **one** into a **dimension-keyed N**: each
leg drives the workload once and emits a *bundle* of per-dimension captures; the
orchestrator runs one comparator per dimension and reports a per-dimension verdict table.

```
nan-021:   workload --[leg]--> MetricVector        --[comparator]--> PASS/FAIL
nan-022:   workload --[leg]--> {dim: capture, ...}  --[comparator[dim]]--> {dim: VERDICT, ...}
```

### What is preserved verbatim (consumed, not re-authored — AC-11/SR-04)

- `ParityWorkload` manifest, `ToolCall`, `default_workload()`, `write_manifest()`,
  `from_json()` — the single declarative workload + ONE stable session identity (#832).
- `durability_barrier()` / `observe_count()` — the symmetric observe-durability barrier
  (ADR-006 nan-021 / #5291).
- `compare_metric_vectors()` / `EXCLUDED` / `field_by_field_record()` — the analytics
  dimension's comparator and exclusion set, consumed unchanged (AC-04 re-prove forbidden).
- `drive_uds_leg()`, `assert_derived_attribution()`, `run_https_leg()`, `PARITY_PHASE`.
- `load_https_vector()` token-guard, `assert_no_seed_reachable()` static guard.
- The HTTPS leg: `bridge-cycle-driver.js`, `bridge-witness.js`, `cloud-cycle-https-leg.sh`,
  `cloud_cycle_gates` (in `cloud-cycle-lib.sh`), `docker-http-posture-smoke.sh`, and the
  shipped `mcp-bridge.js`/`cert-pin.js`/`credstore.js`/`bundle.js`/`init.js` (bridge-in-path,
  D-2). NO net-new transport/cert/spawn code — that is a fork smell to FLAG.
- The release-gate lane contract: `run_smoke_gate` verify-by-name + exit-code truth table
  (ADR-005 nan-021 / #5290), `workflow_dispatch`/tag lane, skip-when-Docker-absent HARD-fail.

### What is net-new (the only substantial additions)

1. A **comparator-framework module** that single-sources the per-dimension comparator
   contract (one template, one forbidden-seed set, one cross-dimension drift guard) — closes
   SR-05/#5302 structurally instead of by convention.
2. **Five new per-dimension comparators** built on that framework (analytics consumes the
   nan-021 comparator).
3. A **determinism/tolerance policy module** single-sourced across the two embedding-ranked
   dimensions (retrieval + briefing) — closes SR-03.
4. An **outcome-class model** (`PARITY-PASS` / `PARITY-FAIL` / `INFRA-ERROR` /
   `INTRA-TRANSPORT-NONDETERMINISM`) with double-capture-and-diff intra-transport detection
   — closes SR-01/SR-02/SR-04.
5. **Dimension-bundle capture** on both legs (extend `drive_uds_leg` to a bundle; extend the
   HTTPS driver/smoke to write a dimension-keyed bundle instead of a single vector).
6. A **transport-health preflight + bounded deadlines** so a #839 half-open hang surfaces as
   `INFRA-ERROR`, never RED parity.
7. The **parity-matrix orchestrator** test that runs every comparator and emits the table.

---

## 2. Component Breakdown

All components live under `product/test/infra-001/` (cumulative extension, no fork). New
modules are additive; existing modules are extended in place only where the bundle requires.

| # | Component | Location | Responsibility | New / Extended |
|---|-----------|----------|----------------|----------------|
| K1 | **Dimension registry** | `harness/parity_dimensions.py` | The single authoritative enumeration of the six dimensions: id, capture key, wire surface (MCP-bridge vs hook-/observe), comparator class, outcome-class policy. Source of truth all consumers derive from (SR-05). | New |
| K2 | **Comparator framework** | `harness/parity_comparator.py` | Abstract `DimensionComparator` base: closed `EXCLUDED` set + `EXCLUSION_JUSTIFICATIONS`, `compare()` → diff list, `ParityMismatch`, evidence-record emitter. The nan-021 `metric_comparator` becomes one concrete subclass shape. Hosts the cross-dimension drift guard. | New |
| K3 | **Determinism/tolerance policy** | `harness/ranking_tolerance.py` | ONE embedding/ranking tolerance policy (stable-prefix + tie-class) consumed by BOTH the retrieval and briefing comparators (SR-03). One place defines "what counts as a ranking match." | New |
| K4 | **Outcome-class model** | `harness/parity_outcome.py` | The four-valued per-dimension verdict enum + `DimensionResult`; double-capture-and-diff intra-transport stability classifier; the matrix-level roll-up rules (which classes redden the gate). | New |
| K5 | **Transport-health preflight** | `harness/transport_health.py` | Per-leg connect/idle bounded-deadline guards (`InfraError`); a half-open-socket hang (the #839 class — #839 itself now closed) or any transport unavailability raises `InfraError`, never a dimension verdict. Defense-in-depth (SR-02). | New |
| C4′ | **Workload manifest** | `harness/parity_workload.py` | Consumed verbatim, *augmented* with a deterministic seed-corpus + query phase so retrieval/briefing rankings are non-degenerate (SR-06). Still ONE manifest, ONE identity, ONE token. | Extended |
| C3′ | **Leg drivers** | `harness/parity_legs.py` | `drive_uds_leg` extended to return a **dimension bundle** (retrieval results, attribution, MetricVector, briefing, restored-context, isolation probe) instead of one MetricVector. New helpers route each dimension to its correct wire surface. | Extended |
| MC | **MetricVector comparator** | `harness/metric_comparator.py` | Consumed verbatim as the analytics comparator (re-skinned as a K2 subclass; logic unchanged — AC-04 forbids re-author). | Consumed |
| C2′ | **HTTPS bridge driver** | `scripts/bridge-cycle-driver.js` | Extended to also issue retrieval (`context_search`/`lookup`/`get`) + `context_briefing` `tools/call`s through the bridge and emit them in the bundle. PreCompact/observe stay on the `/observe` route. | Extended |
| C5′ | **HTTPS smoke gate** | `scripts/cloud-cycle-lib.sh` (`cloud_cycle_gates`) | Writes a dimension-keyed bundle `{run_token, dimension_bundle:{...}}` to `$HTTPS_VECTOR_OUT` instead of `{run_token, metric_vector}`; rides the existing `run_smoke_gate` discriminator. | Extended |
| ORCH | **Parity-matrix orchestrator** | `suites/test_https_uds_parity.py` (+ a sibling matrix test) | Drives both legs once, ingests both bundles (token-guarded, never-empty), runs every dimension comparator, classifies outcomes, emits the per-dimension evidence table keyed by the run token. | Extended |

**Boundaries:** K1–K5 are pure-Python, stdlib-only, **off-Docker unit-testable** (the
nan-021 #5258 seam discipline — comparator/policy TEETH proven before any tag round). The
only Docker-bound components are C2′/C5′/ORCH's live HTTPS leg.

---

## 3. The Dimension-Keyed Parity Matrix (core design)

### Dimension registry (K1) — the single source of truth

Every dimension is one row in `DIMENSIONS`, an ordered tuple of `Dimension` records.
Nothing else hand-lists the six; all consumers (leg drivers, orchestrator, CI table,
forbidden-seed audit) iterate this one enumeration (SR-05 / #5302 — single-source the full
contract, not just shared data).

```python
@dataclass(frozen=True)
class Dimension:
    id: str                  # "retrieval" | "behavioral" | "analytics" | "proactive"
                             #   | "precompact" | "isolation"
    capture_key: str         # key under dimension_bundle both legs emit
    wire_surface: str        # "mcp_bridge" | "hook_observe"  (ADR: two-surface routing)
    comparator: type         # a K2 DimensionComparator subclass
    intra_transport_check: bool  # run double-capture-and-diff stability classifier?
    blocks_c0_proof: bool    # in the six required for the C0 flip? (all six: True — C0 #5304
                             #   done_when "parity is the bar… total"; confirmed 2026-06-25)
```

| id | capture_key | wire_surface | comparator | intra-check |
|----|-------------|--------------|------------|-------------|
| retrieval | `retrieval` | mcp_bridge | `RetrievalComparator` (uses K3) | yes |
| behavioral | `behavioral` | hook_observe | `AttributionComparator` | no (string-exact, proven) |
| analytics | `analytics` | mcp_bridge + hook_observe | `MetricVectorComparator` (nan-021) | no |
| proactive | `proactive` | mcp_bridge | `BriefingComparator` (uses K3) | yes |
| precompact | `precompact` | hook_observe | `PreCompactComparator` | no (see ADR/OQ-3) |
| isolation | `isolation` | mcp_bridge + hook_observe | `IsolationComparator` | no |

### Comparator framework (K2) — closed-set discipline as a base class, not a convention

`DimensionComparator` is the nan-021 `metric_comparator` shape lifted to a base class so the
five new dimensions cannot drift from the discipline (SR-05). Each subclass MUST declare:

- `EXCLUDED: frozenset[str]` — closed, enumerated nondeterminism exclusion set.
- `EXCLUSION_JUSTIFICATIONS: dict[str,str]` — one inline justification per excluded member
  (the nan-021 `EXCLUSION_JUSTIFICATIONS` pattern; AC-09).
- `compare(self, https, uds) -> list[diff]` — field-for-field equality modulo `EXCLUDED`,
  raising `ParityMismatch` (loud, with field + both values + leg) on any non-excluded diff.
- `evidence_record(self, https, uds, *, run_token) -> dict` — the first-live-run field-by-
  field record (ADR-003 nan-021 discipline, generalized per dimension).

**Cross-dimension drift guard (the structural SR-05/#5302 fix):** a single off-Docker test
asserts (a) every `Dimension.comparator` is a `DimensionComparator` subclass declaring a
non-empty justified `EXCLUDED` whose keys ALL appear in `EXCLUSION_JUSTIFICATIONS`; (b) the
forbidden-seed set is defined ONCE (`parity_comparator.FORBIDDEN_SEED_SITES`) and every
module on the path is audited against that one set (no per-file copy). Convention is replaced
by a guard.

### Determinism/tolerance policy (K3) — one entropy policy, two consumers (SR-03)

Retrieval and briefing are the SAME nondeterminism class (embedding/cluster ranking + HNSW
top-k membership flip, SR-01/#4990/GH#746). `ranking_tolerance.py` defines ONE policy
function consumed by both `RetrievalComparator` and `BriefingComparator`:

```python
def ranking_parity(https_ids: list, uds_ids: list, *, scores=None) -> RankingVerdict
```

- The parity signal is the **stable ranked prefix**: the longest leading run of result ids
  that is order-identical across legs. Membership/order churn BELOW the stable prefix (the
  HNSW-approximate tail) is tolerated per the closed policy — not a parity defect.
- Ties (equal score, `sort_unstable`/#2610 ordering) compare as an unordered tie-class, not
  positionally. The tie-class boundaries are derived from the scores the server returns.
- This policy is **single-sourced**: there is no second tie policy. A change to it changes
  both consumers atomically (the #5302 lesson, applied at the architecture level).

---

## 4. Outcome-Class Model (SR-01 / SR-02 / SR-04)

Per dimension, the orchestrator produces exactly ONE of four classes. They are STRUCTURALLY
distinct — an infra hang or an intra-transport flake can NEVER read as a cross-transport
parity verdict.

| Class | Meaning | Gate effect | Disposition |
|-------|---------|-------------|-------------|
| **PARITY-PASS** | Both legs captured; cross-leg comparator clean modulo the closed set. | green for this dimension | none |
| **PARITY-FAIL** | Both legs captured cleanly (each intra-transport stable); the **cross-leg** comparator found a non-excluded diff. A real C0 parity defect. | **RED** | file a NEW GH bug (AC-10); gate stays RED; fix NOT absorbed. |
| **INFRA-ERROR** | A transport-health preflight tripped, a bounded connect/idle deadline expired (a half-open-socket hang — the #839 class, #839 itself now closed), or a capture was missing/empty/un-ingestable. NOT a parity statement. | gate ERRORS (distinct exit code), never green, never counted as RED parity | re-run / diagnose transport; never a dimension verdict. |
| **INTRA-TRANSPORT-NONDETERMINISM** | A leg's own double-capture diffed (the capture is not self-stable, e.g. HNSW top-k flip #4990 within one transport). This is a pre-existing in-transport bug, NOT cloud divergence. | **excluded from the red parity gate**; surfaced separately | file/annotate a SEPARATE GH bug (GH#746 for retrieval ranking); does NOT redden the C0 parity gate (OQ-2 fixed disposition). |

### Detection mechanics

- **INFRA-ERROR** is decided by K5 (transport-health preflight + bounded deadlines) and the
  never-empty ingestion guards BEFORE any comparator runs. The HTTPS leg's `run_smoke_gate`
  exit codes already discriminate Docker-absent(3)/unacquirable(4)/broke(1); K5 adds the
  per-leg connect/idle deadline so a *hung* (not failed) socket is an INFRA-ERROR, not a
  timeout-as-RED. A bounded outer timeout already exists (`HTTPS_SMOKE_TIMEOUT_S`); K5 adds an
  explicit pre-drive reachability probe + idle-deadline classification so the hang is named.
- **INTRA-TRANSPORT-NONDETERMINISM** is decided by **double-capture-and-diff**: for any
  dimension with `intra_transport_check=True` (retrieval, proactive), each leg captures its
  dimension output **twice** in the same drive; if a leg's two captures differ (modulo the K3
  tolerance), that leg is intra-unstable → the dimension is classed
  INTRA-TRANSPORT-NONDETERMINISM and routed out of the red gate. Only when BOTH legs are
  intra-stable does the cross-leg comparator run to decide PARITY-PASS vs PARITY-FAIL. This is
  the structural separation OQ-2 mandates: intra-transport ranking nondeterminism (GH#746)
  can never masquerade as cross-transport divergence.

### Matrix roll-up

The gate is GREEN iff every dimension is PARITY-PASS. Any PARITY-FAIL → RED. Any INFRA-ERROR
→ ERROR exit (distinct code; not green, not a parity RED). INTRA-TRANSPORT-NONDETERMINISM is
recorded in the evidence table and routed to a filed bug but does NOT redden the gate.
(Per dimension, `blocks_c0_proof` defaults True for all six — C0 #5304's `done_when` settles
this: "parity is the bar; it is simple and total… the dimension list… does not narrow the bar"
(confirmed 2026-06-25). An unreachable dimension is a human-signed DOCUMENTED EXCEPTION, never a
silent exclusion; the flag is the escape valve for that signed exception, not a coin-flip.)

---

## 5. Two-HTTPS-Surface Routing (SR-08)

The HTTPS leg has TWO distinct wire surfaces; routing a dimension to the wrong one silently
records NOTHING (the #5298 legacy/rework-frame gotcha → vacuous pass). The registry's
`wire_surface` field makes routing explicit and the never-empty guard makes a misroute ERROR.

| Surface | Transport (HTTPS leg) | Transport (UDS leg) | Dimensions |
|---------|----------------------|--------------------|------------|
| **MCP bridge** | `mcp-bridge.js` JSON-RPC `tools/call` over pinned HTTPS (D-2 bridge-in-path) | `UnimatrixUdsClient` MCP UDS socket | retrieval, proactive, analytics(review read), isolation(read) |
| **hook `/observe`** | pinned HTTPS POST `/observe`, per-slug funnel | `UnimatrixHookClient` hook IPC socket | behavioral, analytics(cycle_events writes), precompact, isolation(write) |

Every observe-driven dimension MUST emit the byte-identical #5298 11-frame `RecordEvent`
sequence on both legs (never the rework/legacy variants — they record nothing). A missing
capture for any dimension ERRORS (never an empty pass) — the nan-021 R-03 discipline carried
to all six (SR-08).

---

## 6. Data Flow

```
                       ORCH (suites/test_https_uds_parity.py — pytest, ONE invocation)
                         │
   workload = default_workload()  (C4′: augmented seed-corpus + query phase, ONE identity/token)
   manifest = write_manifest(...)
                         │
        ┌────────────────┴───────────────────────────────────────────┐
        │ UDS leg (in-process)                                        │ HTTPS leg (shell-out)
        │ K5 preflight (UDS sockets reachable?) → InfraError on hang  │ K5 preflight + run_https_leg →
        │   (half-open-hang defense-in-depth; #839 closed)            │
        │ drive_uds_leg(...) →                                        │   cloud-cycle-https-leg.sh
        │   per dimension, route to wire surface, double-capture      │   → run_smoke_gate (exit-code disc)
        │   where intra_transport_check                               │   → docker-http-posture-smoke.sh
        │   durability_barrier (symmetric, shared helper)             │   → cloud_cycle_gates:
        │   → bundle_uds = {dim: capture, ...}                        │     bridge-cycle-driver.js (C2′):
        └────────────────┬───────────────────────────────────────────┘     context_cycle + retrieval +
                         │                                                   briefing via tools/call;
                         │                                              /observe route: behavioral,
                         │                                              precompact, cycle frames;
                         │                                              writes {run_token,
                         │                                              dimension_bundle:{...}} → $HTTPS_VECTOR_OUT
                         │
   bundle_https = load_https_bundle(out, run_token)  (token-guard; missing/stale/empty → INFRA-ERROR)
                         │
   for dim in DIMENSIONS:
       result[dim] = classify(dim, bundle_uds[dim], bundle_https[dim])
           # K5/ingest → INFRA-ERROR? | double-capture-diff → INTRA-NONDET?
           # | dim.comparator.compare() → PARITY-PASS | PARITY-FAIL (+ evidence record)
                         │
   emit per-dimension evidence table keyed by run_token; roll-up verdict (§4)
```

The cross-process seam is unchanged in shape: the HTTPS leg writes a JSON file; pytest
ingests it token-guarded in the SAME invocation (live-vs-live, D-6). Only the payload widens
from `metric_vector` to `dimension_bundle`.

---

## 7. Integration Surface

Exact names/types downstream agents must use (not invent). Existing surfaces are CONSUMED;
new surfaces follow the nan-021 conventions.

### 7.1 Existing surfaces consumed verbatim

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `ParityWorkload` | `@dataclass(frozen=True)` (`session_id`, `feature_cycle`, `tool_calls`, `expected_observe_count`, `bash_call`, `validate()`, `to_json/from_json`, `write_manifest/read_manifest`) | `harness/parity_workload.py` |
| `ToolCall` | `@dataclass(frozen=True)` (`name`, `args`, `observe`, `response_size`, `response_snippet`) | `harness/parity_workload.py` |
| `default_workload(*, session_id, feature_cycle) -> ParityWorkload` | factory; the canonical workload | `harness/parity_workload.py` |
| `durability_barrier(leg, expected, store_dir, *, deadline_s, poll_s, count_fn, stderr) -> int` | symmetric barrier; raises `DurabilityTimeout` | `harness/parity_workload.py` |
| `observe_count(store_dir) -> int` | single durability predicate (dir byte-size incl `-wal`) | `harness/parity_workload.py` |
| `compare_metric_vectors(mv_https, mv_uds) -> list[diff]` | analytics comparator (consumed by `MetricVectorComparator`) | `harness/metric_comparator.py` |
| `EXCLUDED`, `EXCLUSION_JUSTIFICATIONS`, `UNIVERSAL_FIELDS`, `AT_RISK_FIELDS` | the analytics closed set + classification | `harness/metric_comparator.py` |
| `ParityMismatch(diffs)` | `AssertionError` subclass (field + both values + leg) | `harness/metric_comparator.py` |
| `field_by_field_record(mv_https, mv_uds, *, run_token) -> dict`; `write_field_record(record, path) -> Path` | first-live-run evidence | `harness/metric_comparator.py` |
| `load_https_vector(out_path, expected_run_token) -> dict` | token-guarded ingest (generalize to `load_https_bundle`) | `harness/parity_workload.py` |
| `assert_no_seed_reachable(*source_paths)`; `FORBIDDEN_SEED_SITES` | no-seed static guard | `harness/parity_workload.py` |
| `drive_uds_leg(uds, hook_socket_path, workload, store_dir, *, agent_id, hook_timeout) -> dict` | UDS leg driver (extend to bundle) | `harness/parity_legs.py` |
| `assert_derived_attribution(feature, store_dir)` | behavioral attribution assertion (string-exact) | `harness/parity_legs.py` |
| `run_https_leg(*, manifest_path, run_token, https_out, sandbox)` | HTTPS shell-out seam | `harness/parity_legs.py` |
| `PARITY_PHASE = "delivery"` | the single shared phase both legs declare | `harness/parity_legs.py` |
| `UnimatrixUdsClient` | `.connect/.disconnect/.context_cycle/.context_cycle_review` + `context_search/lookup/get/briefing` MCP methods | `harness/uds_client.py` |
| `UnimatrixHookClient` | `.session_register/.session_close/.record_event/.record_cycle_start/.record_cycle_stop/.record_pre_tool_use/.record_post_tool_use` | `harness/hook_client.py` |
| `cloud_cycle_gates` (bash fn) | reads `MANIFEST_PATH`/`RUN_TOKEN`/`HTTPS_VECTOR_OUT`/`SANDBOX`; writes the out-file | `scripts/cloud-cycle-lib.sh` |
| `run_smoke_gate <image> bash <smoke>` | verify-by-name + exit-code truth table (0 pass / 3 skip→HARD-FAIL / 4 unacq / 1 broke) | `scripts/release-gate-lib.sh` |
| `bridge-cycle-driver.js <projectHash> <manifestPath> --bridge <p> --witness <p>` | drives the cycle THROUGH `mcp-bridge.js`; stdout one JSON line `{ok, metric_vector, ...}` | `scripts/bridge-cycle-driver.js` |
| RecordEvent 11-frame sequence | `SessionRegister`→`cycle_start`→`PreToolUse(TaskCreate phase-set)`→per-observe `Pre`+`Post`→`cycle_stop`→`SessionClose` | knowledge #5298 |

### 7.2 New surfaces introduced (downstream agents implement these)

| Integration Point | Type / Signature | Module |
|-------------------|------------------|--------|
| `Dimension` | `@dataclass(frozen=True)` (`id, capture_key, wire_surface, comparator, intra_transport_check, blocks_c0_proof`) | `harness/parity_dimensions.py` |
| `DIMENSIONS` | `tuple[Dimension, ...]` — the SIX, the single authoritative enumeration | `harness/parity_dimensions.py` |
| `WIRE_MCP_BRIDGE`, `WIRE_HOOK_OBSERVE` | `str` constants for `wire_surface` | `harness/parity_dimensions.py` |
| `DimensionComparator` | ABC: `EXCLUDED: frozenset[str]`, `EXCLUSION_JUSTIFICATIONS: dict[str,str]`, `compare(self, https, uds) -> list[tuple[str,Any,Any]]`, `evidence_record(self, https, uds, *, run_token) -> dict` | `harness/parity_comparator.py` |
| `MetricVectorComparator` | `DimensionComparator` wrapping `compare_metric_vectors` (analytics; logic unchanged) | `harness/parity_comparator.py` |
| `RetrievalComparator`, `BriefingComparator`, `AttributionComparator`, `PreCompactComparator`, `IsolationComparator` | `DimensionComparator` subclasses | `harness/parity_comparator.py` |
| `FORBIDDEN_SEED_SITES` | `tuple[str,...]` — single definition; all modules audited against it | `harness/parity_comparator.py` |
| `assert_comparator_contract(DIMENSIONS)` | cross-dimension drift guard (off-Docker) | `harness/parity_comparator.py` |
| `ranking_parity(https_ids, uds_ids, *, scores=None) -> RankingVerdict` | the ONE ranking tolerance policy (retrieval + briefing) | `harness/ranking_tolerance.py` |
| `RankingVerdict` | `@dataclass` (`matched: bool`, `stable_prefix_len: int`, `tail_churn: list`, `tie_classes: list`) | `harness/ranking_tolerance.py` |
| `Outcome` | `enum`: `PARITY_PASS`, `PARITY_FAIL`, `INFRA_ERROR`, `INTRA_TRANSPORT_NONDETERMINISM` | `harness/parity_outcome.py` |
| `DimensionResult` | `@dataclass` (`dimension: str`, `outcome: Outcome`, `diffs: list`, `detail: str`) | `harness/parity_outcome.py` |
| `classify_dimension(dim, cap_uds, cap_https) -> DimensionResult` | the per-dimension classifier (INFRA → INTRA → compare) | `harness/parity_outcome.py` |
| `intra_transport_stable(cap_a, cap_b, *, tolerance) -> bool` | double-capture-and-diff stability check | `harness/parity_outcome.py` |
| `rollup(results: list[DimensionResult]) -> (verdict, exit_code)` | matrix roll-up (§4 rules) | `harness/parity_outcome.py` |
| `InfraError(Exception)` | distinct exit-class exception (half-open hang / unreachable / empty capture) | `harness/transport_health.py` |
| `preflight_leg(leg, *, connect_deadline_s, idle_deadline_s) -> None` | per-leg reachability/idle bounded-deadline probe; raises `InfraError` | `harness/transport_health.py` |
| `load_https_bundle(out_path, expected_run_token) -> dict[str, Any]` | generalized token-guarded ingest; out-file is `{run_token, dimension_bundle:{...}}`; missing/stale/empty → `InfraError` | `harness/parity_workload.py` (or K5) |
| Dimension bundle (on-disk) | `{"run_token": str, "dimension_bundle": {"retrieval": {...}, "behavioral": {...}, "analytics": {...}, "proactive": {...}, "precompact": {...}|null, "isolation": {...}}}` | written by C5′ `cloud_cycle_gates`; returned by C3′ `drive_uds_leg` |
| `bridge-cycle-driver.js` retrieval/briefing additions | additional `tools/call` envelopes for `context_search`/`context_lookup`/`context_get`/`context_briefing`; emit results into `dimension_bundle` | `scripts/bridge-cycle-driver.js` |

### 7.3 Capture shapes (the per-dimension bundle entries — both legs emit identically)

| Dimension | capture shape (dict) |
|-----------|----------------------|
| retrieval | `{ "queries": [{"tool","args","result_ids","scores"}...], "capture_2": [...] (intra) }` |
| behavioral | `{ "topic_signals": ["nan-022", ...] }` (string-exact; derived, never seeded) |
| analytics | `{ "metric_vector": {...}, "informs_edges": [...], "phase_signal": {...} }` |
| proactive | `{ "briefing_ids": [...], "briefing_scores": [...], "injection_set": [...], "capture_2": {...} (intra) }` |
| precompact | `{ "restored_payload": {...} | null, "measurable": bool, "host_side_gap": str|null }` (ADR/OQ-3) |
| isolation | `{ "slug_a_writes_visible_to_b": bool, "landed_only_in_a": bool }` |

---

## 8. Dependencies

- **#836 / nan-021** — consumed verbatim (`infra-001` substrate). Hard dependency.
- **#830 self-heal** — intentional coupling carried from nan-021 ADR-002: the fixture relies
  on the shipped single-flight `keep_alive` re-init; a flake SIGNALS a #830 regression; do
  not re-implement reconnection.
- **#839 / #5303** — CLOSED (landed via commit 5b6badad / PR #842, 2026-06-25); the C0
  precondition is met and delivery is UNBLOCKED. #839 is NOT a gating dependency. The K5
  INFRA-ERROR class + bounded connect/idle deadlines REMAIN as DEFENSE-IN-DEPTH: any future
  half-open-socket hang (not just #839) must surface as INFRA-ERROR, never a parity result
  (SR-02). The fixture stays a standing guard, but #839 no longer blocks.
- **GH#746 / #4990 (SR-01)** — HNSW approximate top-k membership flip; the deferred
  intra-transport retrieval-ranking nondeterminism. The K4 INTRA-TRANSPORT-NONDETERMINISM
  class routes it OUT of the red gate; file/annotate against GH#746.
- **#2610** — HashMap iteration-order trap in ranking; absorbed by the K3 tie-class policy.
- **Docker** (engine 29.5.2, Compose v2.40.3, linux/arm64) for the live HTTPS/TLS leg;
  off-Docker seam keeps K1–K5 comparator/policy TEETH unit-tested pre-tag (nan-019 #5258).
- **Release-gate lane** — `workflow_dispatch`/tag, skip-when-Docker-absent HARD-fails; NOT
  the JS-only `ci.yml` `pull_request` matrix (nan-021 D-3 carries forward).

---

## 9. Design Decisions (ADR index)

| ADR | Title | Closes |
|-----|-------|--------|
| ADR-001 | Dimension-keyed parity matrix generalizing the nan-021 single-output gate via a single dimension registry | AC-01..AC-08, SR-04 |
| ADR-002 | Four-valued outcome-class model; INFRA-ERROR via transport-health preflight + bounded deadlines; double-capture-and-diff for intra-transport nondeterminism | SR-02, SR-04, OQ-2 |
| ADR-003 | Comparator framework as a base class + single forbidden-seed set + cross-dimension drift guard (structural SR-05/#5302 fix) | SR-05, AC-09 |
| ADR-004 | One embedding/ranking tolerance policy single-sourced across retrieval + briefing (stable-prefix + tie-class) | SR-01, SR-03 |
| ADR-005 | Two-HTTPS-surface routing keyed by the registry; never-empty / #5298-conformant capture | SR-08 |
| ADR-006 | PreCompact restoration parity over the hook /observe route; measurability determination + clean delivery-time host-side call-out | SR-07, OQ-3 |
| ADR-007 | Augmented single workload: deterministic seed-corpus + query phase under ONE identity/token (non-degenerate ranking) | SR-06, OQ-1 |

---

## 10. Open Questions (for spec / human)

- **OQ-A — RESOLVED (confirmed 2026-06-25, not open):** ALL SIX dimensions block the C0 flip
  (`blocks_c0_proof=True` for all six is correct and aligned). C0 #5304's corrected `done_when`
  settles it: "parity is the bar; it is simple and total… the dimension list is the present
  expression of the parity bar and grows with the pipeline; it does not narrow the bar," with
  the disposition that any unreachable dimension is a HUMAN-SIGNED DOCUMENTED EXCEPTION, never
  silently excluded. The `blocks_c0_proof` flag is the escape valve for that signed exception,
  not a pending coin-flip. No human question remains here.
- **OQ-B (delivery-time, ADR-006):** Whether `CompactContext` restoration is fully
  test-only-drivable from BOTH legs or has a host-side (CC) component the harness cannot
  drive. Design keeps PreCompact IN scope and makes any host-side gap a DOCUMENTED
  measurability call-out (`precompact.host_side_gap` in the capture + an evidence-table note),
  never a silent drop (OQ-3 fixed disposition). Resolve at first live drive.
- **OQ-C (spec):** The exact seed-corpus size + query set (ADR-007) that makes retrieval and
  briefing rankings non-degenerate (SR-06) while keeping the workload one-identity/one-token.
  Architecture fixes the shape; spec fixes the numbers.
- **OQ-D (delivery-time, ADR-004):** Whether the `Informs`-edge + phase-signal sub-surface of
  analytics is deterministic for an identical workload or needs a barrier/tolerance (the
  `MetricVector` slice is proven; edges/phase are net-new). First live run decides; default is
  exact with a justified exclusion only on product sign-off (nan-021 ADR-003 disposition).
