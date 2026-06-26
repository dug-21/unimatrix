# Test Strategy + Integration Plan: nan-022

Cross-Transport Parity Suite — C0's proof artifact (#837). TEST-ONLY; extends the nan-021
`product/test/infra-001/` fixture into a six-dimension × two-transport parity matrix. This
test plan's one job is to make the suite a **trustworthy** C0 proof artifact — it must not
**false-RED** (red/error on non-parity-defects, blocking the flip on noise) and must not
**false-GREEN** (pass without measuring cross-transport parity — a vacuous pass that flips
C0 → `proven` on no evidence). Every test below is scored against one of those two cardinal
failures and traces to a risk in `RISK-TEST-STRATEGY.md`.

## 1. Test Strategy

Three test tiers, matching the architecture's seam discipline (§2 "Boundaries": K1–K5 are
pure-Python, stdlib-only, OFF-Docker unit-testable; only C2′/C5′/ORCH's live HTTPS leg is
Docker-bound). The bulk of proving load is OFF-Docker, per #5258 (seam teeth before any tag)
and #5267 (off-Docker arithmetic is not live topology — budget tag rounds).

| Tier | Marker / lane | What it proves | Where |
|------|---------------|----------------|-------|
| **A. Off-Docker unit (teeth)** | none (pure pytest, no fixture) | Comparator teeth, classifier order INFRA→INTRA→PARITY, ranking tolerance policy, drift guard, rollup exit-code truth table, bundle contract round-trip, no-seed audit, single-source invariants | `suites/test_parity_outcome.py`, `test_ranking_tolerance.py`, `test_parity_comparator.py`, `test_parity_dimensions.py`, `test_transport_health.py`, `test_parity_bundle_contract.py`; extends `test_parity_workload.py` |
| **B. Live UDS leg** | `@pytest.mark.integration` | In-process UDS bundle drive: per-dimension capture, double-capture for intra-check dims, barrier-gated DB reads, #5298 frame emission on UDS hook client | `suites/test_https_uds_parity.py` (sibling matrix test) |
| **C. Live cross-leg matrix** | `@pytest.mark.parity` | #5298 11-frame byte-identity on BOTH legs, WAL-flush barrier ordering, bridge-in-path carriage, cross-language bundle emit (JS/shell→Python ingest), full roll-up | `suites/test_https_uds_parity.py` matrix orchestrator (shells to Docker smoke; Stage 3c / release-gate lane) |

**Determinism floor (NFR-6):** counts, edge-ID sets, attribution strings, isolation booleans,
phase signal are compared EXACTLY (no float tolerance). ONLY retrieval (D1) + briefing (D4)
admit the bounded `ranking_parity` tie-tolerance. Every test asserting equality on a
non-D1/D4 dimension uses exact compare.

**Test conventions** (consume existing infra-001 patterns, never fork):
- Off-Docker tests follow the `test_parity_workload.py` precedent: pure-Python over synthetic
  dicts, NO Docker, NO daemon, NO fixtures. Naming `test_{module}_{behavior}_{expected}`.
- Live tests reuse the `daemon_server` fixture + `@pytest.mark.integration`; the matrix
  orchestrator additionally carries `@pytest.mark.parity` and SKIPs absent the Docker smoke env
  (the nan-021 seam-proof precedent — off-Docker seam test covers the wiring when SKIPped).
- Disposition authority (NFR-8, C-4): the tester NEVER silently widens an exclusion set. A
  non-excluded cross-leg diff is a PARITY-FAIL → file a NEW GH bug (AC-10), gate stays RED, fix
  NOT absorbed. An unachievable exact-order is a FILED BUG + documented C0 exception, never a
  quiet tolerance widening.

## 2. Risk → Test Mapping

| Risk | Priority | Cardinal failure | Primary tests | Component test plan |
|------|----------|------------------|---------------|---------------------|
| R-01 ranking nondeterminism / loose tolerance | **Critical** | both | `ranking_tolerance.md` (deep-prefix, in-prefix-divergence NEG, tie-permute, tie-member-loss NEG, scores-absent, prefix-floor boundary) | ranking_tolerance |
| R-02 half-open hang read as verdict | **High** (#839 CLOSED; defense-in-depth) | false-RED | `transport_health.md` (unreachable, half-open NEG, slow-but-healthy boundary), `parity_outcome.md` (INFRA never → RED) | transport_health, parity_outcome |
| R-03 wrong-surface → records nothing → vacuous | **Critical** | false-GREEN | `parity_dimensions.md` (registry-vs-driver routing), live #5298 11-frame byte-identity + NO rework/legacy frame, fault-inject wrong-surface → INFRA NEG | parity_dimensions, parity_legs, bridge-cycle-driver, test_https_uds_parity |
| R-04 WAL-flush capture timing | **Critical** | both | live barrier-gated DB-read; pre-barrier read → INFRA NEG; barrier symmetry both legs | parity_legs, parity_workload, test_https_uds_parity |
| R-05 exclusion-set / capture-shape drift | High | false-GREEN | `parity_comparator.md` `assert_comparator_contract` (subclass, justified EXCLUDED, one seed set, capture_key↔schema), unjustified-entry NEG | parity_comparator |
| R-06 thin corpus → degenerate ranking | High | false-GREEN | corpus depth ≥ N>1 off-Docker; live result-set < N → INFRA; seed via `context_store` only | parity_workload, ranking_tolerance |
| R-07 intra double-capture mis-tuned | High | both | classifier order INFRA→INTRA→PARITY; two intra-stable + cross-divergent → PARITY-FAIL (NEVER INTRA); one K3 tolerance | parity_outcome |
| R-08 PreCompact host-side gap as pass | High | false-GREEN | `measurable=False`+`host_side_gap` → documented call-out NOT pass; `measurable=True` field compare | parity_comparator, test_https_uds_parity |
| R-09 cross-language bundle mismatch | High | false-GREEN | `load_https_bundle` missing/null/empty key → INFRA; schema round-trip both sides | parity_workload, cloud-cycle-lib, test_https_uds_parity |
| R-10 release-only matrix never-green | High | n/a (process) | off-Docker teeth pre-tag; pre-tag local Docker exercise; skip-when-absent HARD-fail | OVERVIEW §4, test_https_uds_parity |
| R-11 Informs edges / phase timing | Medium | both | barrier-gated edge/phase compare; edge SET exact, phase exact; pre-barrier → INFRA | metric_comparator, parity_legs, test_https_uds_parity |
| R-12 stale-token bundle | Medium | both | `load_https_bundle` run_token mismatch → INFRA; live run-marker present | parity_workload, test_https_uds_parity |
| R-13 manifest augmentation breaks ONE-identity | Medium | n/a (drift) | single `ParityWorkload`, `run_token==session_id`, one barrier, manifest round-trip | parity_workload |
| R-14 ABC adapter alters MetricVector logic | Low | drift | adapter golden diff == consumed `compare_metric_vectors` | metric_comparator |
| R-15 forbidden-seed audit misses module | Medium | false-GREEN | `assert_no_seed_reachable` over ALL net-new modules + seed loader; seed writes content only | parity_workload, parity_comparator |
| R-16 fork smell (net-new transport/cert) | Low | scope | `git diff` confined to infra-001; bridge-in-path reuse | bridge-cycle-driver, cloud-cycle-lib (review-flag) |

### AC → Test Mapping (verification owners)

| AC | Primary tests | Tier |
|----|---------------|------|
| AC-01 one-workload/one-token/no-seed | `test_parity_workload` single-object/token/round-trip + `assert_no_seed_reachable` all modules; live same-manifest replay | A + C |
| AC-02 retrieval parity | `ranking_parity` off-Docker matrix; live stable-prefix + double-capture INTRA classify | A + B/C |
| AC-03 behavioral parity | live string-exact `topic_signal` barrier-gated; `AttributionComparator` empty EXCLUDED off-Docker | A + C |
| AC-04 analytics parity | `MetricVectorComparator` golden-identical off-Docker; live Informs-set + phase barrier-gated | A + C |
| AC-05 proactive parity | `BriefingComparator` imports SAME `ranking_parity`; live stable-prefix + injection-set | A + C |
| AC-06 PreCompact parity | `measurable` call-out off-Docker; live `/observe` capture + measurability determination | A + C/manual |
| AC-07 isolation parity | isolation boolean EXACT off-Docker; live per-slug probe barrier-gated | A + C |
| AC-08 CI matrix / exit-code | `rollup` truth table off-Docker; live evidence table + skip HARD-fail + run-marker | A + C |
| AC-09 closed justified exclusions | `assert_comparator_contract` + unjustified NEG off-Docker | A |
| AC-10 real defect → GH bug, RED | PARITY-FAIL path + evidence record; manual review of failure-handling + no-fix diff | A + manual |
| AC-11 zero production-code change | `git diff` confined to infra-001 (shell) | shell |
| AC-12 matrix is C0 proof, not flip | manual + no-C0-flip-in-diff; `blocks_c0_proof` registry flag | manual |

## 3. Integration Harness Plan

This feature IS an extension of the infra-001 integration harness; "integration tests" here
means the parity-matrix suite itself, not the legacy 9-suite behavioral catalog. The legacy
catalog is NOT touched (C-1) and is not in this feature's test surface.

### Smoke gate (MANDATORY minimum, Stage 3c)
`cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` MUST pass —
proves the consumed substrate (server binary, MCP path) is healthy before the parity matrix
runs. The parity matrix is NOT in the smoke set (it is `@pytest.mark.parity`/`integration`).

### Suites that apply (suite-selection table)
The matrix touches server tool logic, store/retrieval, confidence-adjacent ranking, schema/
storage (per-slug isolation), and security (isolation). Per the selection table, the relevant
legacy suites as a substrate-health baseline are `tools`, `protocol`, `lifecycle`,
`edge_cases`. These are run as a **regression baseline only** — the parity matrix does not
modify them and any failure there is triaged as pre-existing (file GH Issue + xfail, never
fixed in this PR).

### New tests this feature adds (Stage 3c implements)
All new tests live under `product/test/infra-001/suites/` (cumulative, no fork). New off-Docker
unit modules + extensions to the existing parity orchestrator:

| New / extended test file | Tier | Covers |
|--------------------------|------|--------|
| `suites/test_parity_dimensions.py` (new) | A | K1 registry: routing constants, capture_key uniqueness, all-six enumeration, blocks_c0_proof defaults |
| `suites/test_parity_comparator.py` (new) | A | K2 drift guard `assert_comparator_contract`, per-comparator EXCLUDED/justifications, one FORBIDDEN_SEED_SITES, PreCompact measurable call-out, isolation/attribution exact |
| `suites/test_ranking_tolerance.py` (new) | A | K3 `ranking_parity` full matrix incl. negative tests + prefix-floor boundary |
| `suites/test_parity_outcome.py` (new) | A | K4 classifier order, `intra_transport_stable`, `rollup` exit-code truth table, INFRA-never-RED, cross-divergent-never-INTRA |
| `suites/test_transport_health.py` (new) | A | K5 `preflight_leg` unreachable/half-open/slow-healthy-boundary, `load_https_bundle` |
| `suites/test_parity_bundle_contract.py` (new) | A | Cross-language bundle schema round-trip; missing/null/stale-token → INFRA |
| `suites/test_parity_workload.py` (extend) | A | Augmented manifest single-identity/token/barrier; seed-content-only; no-seed audit over net-new modules |
| `suites/test_https_uds_parity.py` (extend + sibling matrix test) | B + C | Live bundle drive both legs, #5298 byte-identity, barrier-gated DB reads, bridge carriage, full roll-up, evidence table |

### When NOT to add an integration test
- Behavior provable off-Docker with synthetic dicts stays a Tier-A unit test (most teeth).
- No new legacy-suite (tools/lifecycle/...) tests — this feature changes no production behavior.
- No new harness infrastructure beyond the K1–K5 modules + parity-orchestrator extension; any
  net-new transport/cert/spawn code is a FORK SMELL to FLAG (R-16), not to add.

### New pytest marker
Reuse `@pytest.mark.parity` for the matrix orchestrator and `@pytest.mark.integration` for the
live UDS leg. Tier-A unit tests carry NO marker (collected by default, run pre-tag with no
Docker). No new marker is required; if the matrix orchestrator is split from the nan-021
single-vector test, both share the `parity` marker.

## 4. Cross-Component Test Dependencies & Execution Order

1. **Tier A first, pre-tag** (#5258/#5267): all off-Docker teeth + drift guard + rollup truth
   table + bundle contract + no-seed audit. These gate any tag round. A red here is a real
   implementation bug, fix before proceeding.
2. **Tier B local** (`@pytest.mark.integration`, local `daemon_server`): UDS bundle drive +
   barrier-gated reads. No Docker.
3. **Tier C pre-tag local Docker exercise** (R-10/#5267): drive the FULL matrix against the
   local Docker HTTPS fixture BEFORE any release tag, so #5298 byte-identity, the cross-language
   bundle emit, and bridge-in-path carriage surface before the release round. Budget multiple
   tag rounds; treat sequentially-revealed live failures as new layers, not regressions.
4. **Release-gate lane**: `workflow_dispatch`/tag, skip-when-Docker-absent HARD-fails by the
   distinct exit code (AC-08); NOT the JS-only `ci.yml` pull_request matrix (C-7).

**Dependency chain:** K1 registry feeds every consumer → K2 comparator + K3 tolerance feed
classifier → K4 classifier + K5 preflight feed the orchestrator → C3′/C2′/C5′ legs feed the
bundle → ORCH ingests. The off-Docker drift guard (`assert_comparator_contract`) is the
single structural check that the registry, comparators, seed set, and bundle schema have not
drifted — it must pass before any live tier.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search` (decision/nan-022 + parity
  false-green patterns) — surfaced nan-022 ADRs #5305/#5307/#5311/#5313, #5302 (single-source
  the full contract), #5177 (multi-wave under-tests earlier parity ACs → vacuous), #5258
  (off-Docker seam teeth pre-tag), #5267 (release-gate never-green-on-tag, budget N rounds),
  #5285 (derive topic_signal, never seed).
- Stored: nothing novel at plan-design time — the matrix shape is captured in nan-022 ADRs and
  is one-feature-deep (below the 2+-feature pattern-stewardship bar). A storable cross-feature
  test pattern (off-Docker outcome-class teeth + classifier-order proof + cross-language bundle
  round-trip) becomes worth storing if a Stage-3c discovery yields a reusable fixture/helper;
  flagged for the execution phase.
