# Gate 3a Report: nan-022

> Gate: 3a (Component Design Review)
> Date: 2026-06-26
> Result: PASS
> Validator: nan-022-gate-3a

Cross-Transport Parity Suite — C0 proof artifact (#837). TEST-ONLY, cumulative on
`product/test/infra-001/`. Validated the pseudocode (OVERVIEW + 11 components) and test-plan
(OVERVIEW + 11 components + `parity_bundle_contract.md`) against ARCHITECTURE.md,
SPECIFICATION.md, RISK-TEST-STRATEGY.md, and IMPLEMENTATION-BRIEF.md.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | All 11 components realize the K1–K5 + C3′/C4′/C2′/C5′/MC/ORCH model; four-valued outcome, drift guard, single ranking policy, two-surface routing all structural, not by convention. |
| 2. Specification coverage | PASS | Per-dimension capture/compare, four outcome classes, closed/justified exclusion-set discipline, #5298 11-frame contract, no-seed discipline, D5 measurability call-out. No scope additions. |
| 3. Risk coverage (test plans) | PASS | All four Critical risks (R-01/R-02/R-03/R-04) carry the load-bearing *rejecting* negative + boundary tests at the correct tier; R-07 cross-divergent-never-INTRA negative present. |
| 4. Interface consistency | PASS | Signatures match brief Function Signatures + OVERVIEW shared-types; cross-language bundle contract identical across C2′ emit / C5′ assemble / `load_https_bundle` ingest / `classify_dimension`. |
| 5. Knowledge stewardship | PASS | Design-phase agent reports (architect, spec) and both OVERVIEW deliverables carry valid `## Knowledge Stewardship` blocks with Queried + Stored/declined entries. |
| Cumulative / no-fork | PASS | Diff confined to `infra-001`; MC consumed verbatim (object identity, golden-identical adapter test); no net-new transport/cert/spawn code (R-16 clean); ORCH preserves the nan-021 MetricVector test. |

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**:
- K1 `parity_dimensions` is the single authoritative enumeration; every consumer (legs,
  orchestrator, drift guard, no-seed audit) iterates `DIMENSIONS`. `Dimension` frozen dataclass
  fields match ARCHITECTURE §7.2 / brief Data Structures exactly (`id, capture_key, wire_surface,
  comparator, intra_transport_check, blocks_c0_proof`); all six rows match the §3 registry table
  with `blocks_c0_proof=True` for all six.
- K2 `parity_comparator` realizes the `DimensionComparator` ABC + per-subclass `EXCLUDED` /
  `EXCLUSION_JUSTIFICATIONS`, the single `FORBIDDEN_SEED_SITES`, and `assert_comparator_contract`
  as a structural drift guard (ADR-003 / SR-05 / #5302 — convention replaced by a guard).
- K3 `ranking_tolerance` is ONE `ranking_parity` / `RankingVerdict` (stable-prefix + unordered
  tie-class) consumed by BOTH `RetrievalComparator` and `BriefingComparator` (ADR-004 / NFR-4) —
  no second tolerance.
- K4 `parity_outcome` realizes the four-valued `Outcome`, the FIXED classifier order
  INFRA→INTRA→PARITY, `intra_transport_stable` double-capture, and the §4 roll-up truth table.
- K5 `transport_health` realizes `InfraError` + bounded `preflight_leg(connect/idle deadline)`
  as defense-in-depth (ADR-002 / SR-02).
- C4′/C3′/C2′/C5′/ORCH realize the extensions (augmented workload, bundle leg drivers,
  bridge-in-path retrieval/briefing, dimension-keyed out-file, matrix orchestrator).

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: FR-1..FR-19 each map to pseudocode/test-plan: one workload/one identity/one token
(FR-1/FR-2, R-13), #5298 11-frame on both legs (FR-3, C-3), no-seed-of-outputs (FR-4, R-15),
dimension-keyed bundle (FR-5), per-dimension wire routing (FR-6, R-03), bridge-in-path (FR-7,
R-16), comparator-on-template + closed justified exclusions (FR-8/AC-09), evidence table
(FR-9), four outcome classes (FR-10), double-capture-and-diff (FR-11/R-07), missing/stale →
INFRA (FR-12/R-09/R-12), transport-health preflight (FR-13/R-02), off-Docker teeth (FR-19/R-10).
No unrequested feature; the net-new constructs are scope-mandated SR mitigations (matches the
clean ALIGNMENT-REPORT).

### Check 3 — Risk coverage (the four Critical risks)
**Status**: PASS
**Evidence** — each Critical risk's load-bearing negative/boundary test is present:
- **R-01 (ranking nondeterminism / loose tolerance)** — `ranking_tolerance` test plan has the
  full matrix: deep-prefix-match (tolerate tail churn), the **in-prefix-divergence NEGATIVE**
  (`matched=False` — tolerance cannot swallow a real cross-leg ranking divergence),
  tie-class-permute, **tie-class-member-loss NEGATIVE**, scores-absent membership fallback, and
  the prefix-floor boundary (N vs N-1).
- **R-02 (#839 hang → INFRA-ERROR)** — `transport_health` test plan has unreachable→InfraError,
  half-open-simulation→InfraError, and the **slow-but-healthy boundary that must PASS, not
  INFRA**; `parity_outcome` roll-up proves INFRA never converts to a parity RED. Defense-in-depth
  retained even though #839 is closed.
- **R-03 (wrong-surface → records-nothing → vacuous pass)** — registry-vs-driver routing
  consistency (off-Docker), live #5298 11-frame byte-identity on both legs with NO rework/legacy
  frame, and the **wrong-surface fault-injection NEGATIVE → INFRA-ERROR, never an empty
  PARITY-PASS**.
- **R-04 (WAL-flush capture timing)** — every DB-reading capture (D2/D3/D6) is barrier-gated
  BEFORE the read; the **pre-barrier read NEGATIVE → INFRA-ERROR (never PARITY-FAIL, never
  empty-equals-empty pass)**; barrier symmetry asserted as the SAME `durability_barrier` helper
  on both legs.
- Supporting **R-07** — the insidious "BOTH legs intra-stable but cross-leg divergent → MUST be
  PARITY-FAIL, never reclassified INTRA-NONDET" negative is present, with an explicit
  classifier-order (INFRA→INTRA→PARITY) proof and a single shared K3 tolerance for intra + cross.
High/Medium risks (R-05/06/08/09/10/11/12/13/14/15/16) each map to scenarios in the OVERVIEW
risk→test table and the component plans.

### Check 4 — Interface consistency
**Status**: PASS
**Evidence**: New signatures (`ranking_parity`, `classify_dimension`, `intra_transport_stable`,
`rollup`, `preflight_leg`, `load_https_bundle`, `DimensionComparator`, `assert_comparator_contract`)
match the brief's Function Signatures and the OVERVIEW shared-types table. The cross-language
dimension bundle (`{run_token, dimension_bundle:{...}}`) is identical across the C2′ emit, C5′
assembly, `load_https_bundle` ingest, and `classify_dimension` consumption; retrieval/proactive
carry `capture_2`; only `precompact.restored_payload` may be `null` and only with
`measurable=False`; any other missing/null/empty capture → INFRA-ERROR. `parity_bundle_contract.md`
asserts the schema round-trip on both the Python (off-Docker) and live (JS emit) sides (R-09).

### Check 5 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: The two active-storage design-phase agent reports carry valid blocks:
`nan-022-agent-1-architect-report.md` has `Queried:` (context_briefing/get/lookup) and `Stored:`
(ADRs #5305–#5311, with a `Supports` edge to #5302); `nan-022-agent-2-spec-report.md` has
`Queried:` with a reasoned read-only "no storage" disposition. Both pseudocode and test-plan
OVERVIEW deliverables also carry `## Knowledge Stewardship` blocks with Queried + reasoned
"nothing novel to store" entries (one-feature-deep, below the 2+-feature pattern bar). The 11
per-component pseudocode and 12 per-component test-plan files do not individually carry blocks —
this is expected; stewardship is assessed at the steward agent's report/deliverable level, and
those are present and reasoned. No missing block at the agent-report tier.

### Stage-3a open questions — assessment (all delivery-time, none design-blocking)
- **STABLE_PREFIX_FLOOR N** — architecture fixes the shape (N>1, non-degenerate, stable-prefix
  is the parity signal); the concrete N + corpus size are a Stage-3a/delivery test-design tuning
  call (ADR-007 / OQ-C / OQ-3). The drift/degeneracy guard (result-set < N → INFRA) is specified,
  so a thin-corpus vacuous pass is structurally blocked regardless of the final N. Not blocking.
- **D5 PreCompact host-side measurability** — resolved by design as a first-live-drive
  determination with a documented `measurable`/`host_side_gap` call-out, never a silent drop or
  vacuous pass (ADR-006 / OQ-B). Not blocking.
- **Informs-edge / phase timing** — barrier-gated, exact post-barrier, justified exclusion only on
  product sign-off; first-live-run decides (ADR-004 / OQ-D / R-11). Not blocking.
- **`load_https_bundle` ownership (K5 vs C4′)** — RESOLVED in the pseudocode: logic in K5 (depends
  on `InfraError`, no circular import), re-exported from C4′. Consistent across files. Not open.
- **K5 deadline constants + EXIT_INFRA code collision** — delivery-time tuning; see advisories.

## Advisories for Stage 3b (NON-BLOCKING — not rework)

These are tuning calls the implementing agents must settle during delivery; none is a design gap:

| # | Advisory | Owner |
|---|----------|-------|
| A1 | Pin the concrete `EXIT_INFRA` exit-code constant and assert it ∉ {0 pass, 1 broke, 3 skip, 4 unacquirable} reserved by `run_smoke_gate` (5 is free). Add the assertion to the off-Docker rollup truth-table test. | K4 / ORCH dev |
| A2 | Choose the `drive_uds_leg` widening shape (extend-in-place vs sibling bundle entry) keeping the existing nan-021 MetricVector `test_https_uds_parity` test green; confirm cumulative, no parallel scaffold. | C3′ / ORCH dev |
| A3 | Pin the single owner (C2′ vs C5′) that emits `informs_edges` + `phase_signal` into the analytics capture so neither double-writes nor drops it. | C2′ / C5′ devs |
| A4 | D5 `measurable=False` is folded into `INFRA_ERROR` with a documented-exception detail string rather than a fifth class — verify the flip session reads the detail; keep it honest (never rounded to "fully measured"). | K4 / ORCH dev |
| A5 | Per the InferenceConfig lesson (crt-022): the dev implementing `ranking_parity` / `preflight_leg` / `load_https_bundle` / `assert_comparator_contract` must open the component **test plan** (not just pseudocode) and ship the listed boundary/negative tests in the SAME pass — the prefix-floor boundary, slow-but-healthy boundary, and pre-barrier negative are mandatory same-pass tests. | all K-dev |

## Rework Required
None.

## Scope Concerns
None. Diff is confined to `product/test/infra-001/`; zero production-code change; cumulative on
nan-021 with MC consumed verbatim; no fork, no parallel scaffold, no net-new transport/cert/spawn
code. The design is a trustworthy, false-green-proof C0 proof artifact as specified.
