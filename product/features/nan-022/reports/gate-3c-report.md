# Gate 3c Report: nan-022

> Gate: 3c (Final Risk-Based Validation)
> Feature: nan-022 — Cross-Transport Parity Suite (C0 #5304 proof artifact, #837)
> Branch: feature/nan-022 @ 623fd457
> Date: 2026-06-26
> Result: **PASS**

## Framing applied

The DELIVERABLE validated here is the measurement SUITE (a proof apparatus), NOT a green C0.
A correctly-functioning apparatus produces a RED live matrix when parity is not yet met, with
real defects FILED and the gate left RED. C0 is explicitly NOT flipped (AC-12). This gate
validates the suite's correctness and risk coverage, not whether C0 is green. On that basis,
the suite did its job: it surfaced and correctly classified every divergence, filed two GH
bugs (#844, #845), masked nothing, and left the gate RED (verdict=ERROR, exit 7).

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | All 16 risks mapped to tests + results in RISK-COVERAGE-REPORT.md; per-dimension live evidence table present (4 runs); R-01/R-04/R-06 live-realized and caught as designed |
| 2. Test coverage completeness | PASS | 4 Critical risks (R-01..R-04) fully covered; off-Docker teeth independently re-run green (218 + 26 + 32); integration smoke 24/24; live matrix exercised on local Docker pre-tag |
| 3. Specification compliance | PASS | Four-valued outcome model, classifier order INFRA→INTRA→PARITY, single ranking policy, two-surface routing, augmented one-workload all present and matched; ACs verified per ACCEPTANCE-MAP |
| 4. Architecture compliance | PASS | Component structure (K1–K5, C2′–C5′, ORCH) matches ARCHITECTURE §2; no architectural drift; bridge-in-path reuse intact |
| 5. Knowledge stewardship | PASS | RISK-COVERAGE-REPORT.md (tester deliverable) carries `## Knowledge Stewardship` with `Queried:` + `Stored:` entries |
| Integration: smoke gate (`-m smoke`) | PASS | 24 passed (tester-reported), mandatory gate met |
| Integration: live parity matrix (`-m parity`) | PASS (RED-by-design) | verdict=ERROR exit 7, correct given filed defects; gate NOT forced green |
| AC-11: zero production-code change | PASS | Diff confined to `product/`; the 28 impl files all under `product/test/infra-001/`; no `crates/**`/`lib/`/`src/` change |
| xfail hygiene | PASS | No xfail added to nan-022 suites; pre-existing xfails live only in unchanged files, each referencing a GH issue |
| Stage-3c substrate fixes | PASS | All 5 fixes are legitimate test-infra changes within infra-001 (AC-11 intact) |

## Detailed Findings

### Check 1 — Risk mitigation proof
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md §"Coverage Summary" maps every R-01..R-16 to a named
test and a result, all marked "Full" coverage. The four Critical risks:
- **R-01 (ranking nondeterminism)**: `test_ranking_tolerance` off-Docker (re-run green here);
  live D1/D4 flip (1/4 runs) correctly classified PARITY_FAIL on two intra-stable legs, root-
  caused as HNSW top-k entropy (#4990/GH#746), filed GH#844 + documented C0 exception. Tolerance
  was NOT widened — the INTRA-vs-PARITY disposition was honoured (the flip is cross-leg, both
  legs intra-stable, so it correctly did NOT escape into the INTRA bucket).
- **R-02 (#839/INFRA half-open)**: `test_transport_health` re-run green; INFRA class proven
  distinct; the UDS-preflight false-half-open was a Stage-3c fix (liveness now uses the shipped
  `UnimatrixUdsClient` handshake — a true half-open still trips).
- **R-03 (wrong-surface/vacuous-pass)**: `test_parity_legs` registry-vs-driver + live #5298
  11-frame byte-identity emitted on both legs; never-empty guard makes a misroute INFRA-ERROR.
- **R-04 (WAL-flush)**: a real live WAL-flush defect was found (db-only=0 rows; rows lived in
  the uncheckpointed `-wal`) and fixed by copying `-wal`/`-shm` sidecars; pre-barrier read =
  INFRA proven. This is the apparatus catching a substrate-shape bug exactly as R-04 predicted.

No Phase-2 risk lacks coverage.

### Check 2 — Test coverage completeness
**Status**: PASS
**Evidence**: Independently re-ran the off-Docker teeth and reproduced the reported counts
exactly: pytest **218 passed, 4 deselected**; node `--test` **26 pass / 0 fail**; shell logic
**21 + 11 = 32 passed**. Integration smoke reported **24 passed** (mandatory gate). The live
matrix was exercised on local Docker (engine 29.5.2) pre-tag (R-10 pre-tag real-server
exercise satisfied — live layers surfaced here, not on a release tag). The classifier order
(INFRA→INTRA→PARITY) and both-legs-stable guard are present in `parity_outcome.py` and proven
by the green teeth, satisfying R-07's "cross-leg divergence on two intra-stable legs can never
be reclassified INTRA."

### Check 3 — Specification compliance
**Status**: PASS
**Evidence**: SPECIFICATION §2/§4/§5 load-bearing constructs are all implemented and matched:
four-valued `Outcome` enum (PARITY_PASS/PARITY_FAIL/INFRA_ERROR/INTRA_TRANSPORT_NONDETERMINISM),
single `ranking_parity` policy shared D1+D4 (NFR-4), two-surface routing keyed by the registry
`wire_surface` (SR-08), augmented one-workload with one identity/token/barrier (FR-1/FR-2),
exact-compare floor for non-ranked dimensions (NFR-6). ACCEPTANCE-MAP AC-01..AC-12 verified;
AC-02/AC-05 PARTIAL (HNSW flip → GH#844, R-01 disposition, not a tolerance failure); AC-06
PASS-with-documented-gap (D5); AC-07 OPEN (D6 harness gap → GH#845); AC-10/AC-11/AC-12 PASS.

### Check 4 — Architecture compliance
**Status**: PASS
**Evidence**: All net-new modules from ARCHITECTURE §2 exist under `product/test/infra-001/`
(parity_dimensions, parity_comparator, ranking_tolerance, parity_outcome, transport_health,
plus the bundle-support and seed-corpus modules). The dimension registry is the single source
of truth; the cross-dimension drift guard (`assert_comparator_contract`) is present and green.
No architectural drift. Bridge-in-path reuse confirmed (no net-new transport/cert/spawn code —
R-16/AC-11). All extended files stay within their architected boundaries.

### Check 5 — Knowledge stewardship
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md (the tester's Stage-3c deliverable) carries a
`## Knowledge Stewardship` block (lines 185–194) with a `Queried:` entry (context_briefing +
surfaced ADRs, four-valued model applied) and a `Stored:` entry (the reusable Stage-3c
"off-Docker teeth green on synthetic dicts but first live run surfaces substrate-shape defects"
pattern, deferred to a 2nd parity-matrix reuse per the 2+-feature pattern bar). Complete with
reason. No separate tester agent file exists; the report IS the tester deliverable and carries
the block.

## Integration test validation (mandatory)

- **Smoke (`-m smoke`)**: 24 passed (tester-reported). Gate met.
- **Live parity matrix (`-m parity`)**: run on local Docker 29.5.2; verdict=ERROR (exit 7).
  This RED/ERROR is **correct by design** — D5 INFRA (documented), D6 PARITY_FAIL (harness),
  and intermittent D1/D4 PARITY_FAIL drive the gate non-green. Defects are genuinely filed
  (#844, #845 both OPEN, bug-labeled; #746 referenced and OPEN). The gate was **NOT** forced
  green.
- **xfail hygiene**: NO xfail/skip added to any nan-022 suite file (`test_https_uds_parity*`,
  `test_parity_*`, `test_ranking_tolerance`, `test_transport_health`). The pre-existing xfails
  reported by grep live only in files NOT in this diff (test_adaptation/confidence/edge_cases/
  lifecycle/tools), each referencing a GH issue. No integration test was deleted or commented
  out (no test-body/assert/marker deletions in the suites diff). No exclusion set widened; no
  feature bug masked.
- **RISK-COVERAGE-REPORT.md** includes integration test counts (off-Docker 218/26/32, smoke 24,
  live matrix verdict) AND the per-dimension live evidence table (4 runs, D1–D6 with
  dispositions).

## Per-dimension live outcome — validated against report

| Dim | Stable outcome | Validator finding |
|-----|----------------|-------------------|
| D2 behavioral | PASS | PROVEN parity (`topic_signal='nan-021'` both legs, derived not seeded) — confirmed |
| D3 analytics | PASS | PROVEN parity (MetricVector + Informs edges + phase, consumed nan-021 comparator) — confirmed |
| D1 retrieval | flaky→PARITY_FAIL | HNSW #4990/GH#746 → GH#844; tolerance NOT widened; INTRA-vs-PARITY disposition correct — confirmed |
| D4 proactive | flaky→PARITY_FAIL | tracks D1 → GH#844 — confirmed |
| D5 precompact | INFRA (documented) | host-side gap named, `measurable=false`, OQ-3 resolved, never vacuous — confirmed acceptable |
| D6 isolation | PARITY_FAIL (harness) | false-RED harness measurability gap → GH#845 — see judgment below |

## Stage-3c substrate fixes — re-reviewed against committed diff

All five confirmed as legitimate test-infra fixes within `product/test/infra-001/`, each with
inline R-XX rationale, none touching production code (AC-11 intact):
1. **Drift-guard import order** — `test_https_uds_parity.py` imports `parity_comparator` before
   `parity_dimensions` so `bind_comparators`' DIMENSIONS rebind is observed. Test-file fix.
2. **Socket liveness** — `transport_health.py` UDS preflight uses the shipped `UnimatrixUdsClient`
   handshake instead of a bare `\n` nudge; a true half-open still trips. Harness fix, C-2 honoured.
3. **WAL-copy** — `cloud-bundle-lib.sh` copies `-wal`/`-shm` sidecars through the same busybox
   `vol` sidecar Gate 7 uses, so sqlite3 sees the durable post-barrier view (R-04). Shell test-infra.
4. **Seed dedup** — `parity_seed_corpus.py` uses 8 semantically-distinct subjects so ≥3 survive
   the server's near-duplicate collapse (R-06/OQ-3). Harness fix.
5. **Briefing text-vs-JSON parser** — both `parity_legs_capture.py` (`_parse_briefing_result`)
   and `bridge-cycle-capture.js` (`parseBriefingResult`) fall back to the text table since
   `context_briefing` ignores `format=json` (R-09 cross-language contract). Test-infra, both sides.

None is a production change; AC-11 holds.

## Explicit judgment — D6 (#845) measurability gap

**Judgment: ACCEPTABLE documented gap for THIS deliverable, BUT a genuine coverage limitation
the human must weigh before flipping C0. NOT a validator gate-fail.**

The HTTPS leg uses a real two-slug container and correctly measures `slug_a_writes_visible_to_b
=false` — isolation holds. The UDS leg's `daemon_server` fixture is single-slug, so the
cross-slug probe has no slug B to be isolated from; the `feature="…slug-b"` hint does not route
to a separate store and the probe returns the slug-A marker (`=true`). This is a **measurement
artifact, not a cross-tenant leak** — verified, and the HTTPS leg independently proves isolation
holds, so there is no evidence of a real D6 divergence.

Why this is acceptable for the apparatus deliverable: the suite did exactly what a correct proof
apparatus must — it detected the false-RED, root-caused it as a harness (not product) gap, filed
GH#845, left the gate RED, and did NOT mask it or assume isolation. That is the apparatus working,
identical in character to the D5 host-side call-out.

Why the human must weigh it: D6 cross-transport PARITY is **genuinely unmeasured** until the UDS
fixture is made two-slug. The suite cannot currently measure dimension 6 symmetrically — it proves
D6 on HTTPS only. Per C0 #5304's "parity is the bar… total" disposition, D6 must be a human-signed
documented exception (AC-07 = OPEN), and C0 is therefore NOT yet flippable on D6 (which the report
states plainly). This is a scope/coverage fact for the flip session, surfaced honestly — it does
NOT make this gate fail, because the deliverable is the apparatus and the apparatus correctly
reported its own limitation rather than vacuously passing.

## Result

**PASS.** The measurement suite is correct: every risk is covered, the off-Docker teeth are
independently green at the reported counts, the live matrix's RED/ERROR verdict is correct-by-
design with both defects genuinely filed and the gate not forced green, zero production code
changed (AC-11), and all five Stage-3c fixes are legitimate test-infra changes. The D6 (#845)
and D5 measurability gaps are honestly documented call-outs that the human flip session must
weigh — they are the apparatus reporting its own limits, not gate failures.
