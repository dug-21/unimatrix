# ass-085 — Empirically identify the best parity-retrieval corpus: a wide-margin design that eliminates HNSW top-k boundary flips (D1/D4), and its impact on the existing test strategy

## Problem Statement

The nan-022 cross-transport parity matrix classifies **retrieval (D1)** and **proactive/
briefing (D4)** as intermittent `PARITY_FAIL` (#844). Root cause is **not** a transport bug:
it is an HNSW approximate top-k **membership flip** at the similarity boundary — `hnsw_rs`
0.3.4 has no public seeding API, so the index is rebuilt from OS entropy per process and an
entry whose similarity sits at the *k*/*k+1* boundary lands in/out of the top-k run-to-run
(#746 / #4990). #844's failing run: HTTPS `[3,1,5,4]` vs UDS `[3,1,2,5,4]` — entry 2 on the
cutoff. D4 ranks over the same corpus and tracks D1.

The current fixture corpus packs entries close enough in similarity that the boundary is
"shallow," so the technology-inherent jitter manifests as a parity failure. The proposed
direction (human, 2026-06-26): **design the corpus with wide, deliberate similarity
separation between ranked entries** so the margin dominates HNSW's approximate error — top-k
membership becomes stable across transports, and a divergence then signals an *egregious /
broken* discrepancy rather than ANN boundary noise. This trades fine ranking-precision
sensitivity (explicitly accepted) for a stable, meaningful parity assertion — and is cleaner
than a tolerance band, which risks *swallowing* real regressions (the #844 warning).

This is an **empirical** claim ("wide margins → stable top-k"), and prior work has been burned
trusting small samples (bugfix-742: a 40-run green streak under-samples a ~2% flake). It must
be **measured**, not assumed, before #844's fix is committed. This spike produces the
recommended corpus and the test-strategy impact assessment that #844 depends on.

## Goal

1. **What corpus design eliminates the flip — empirically?** Identify the recommended
   parity-retrieval corpus: entry count, the inter-rank similarity-margin separation, and the
   query construction such that the cross-transport top-k **membership and order are identical
   run-to-run**, measured at the sampling bar below. Output a concrete, minimal recommended
   corpus (not just a principle).
2. **What is the minimum inter-rank similarity margin** that dominates HNSW (`hnsw_rs` 0.3.4,
   no seed API) approximate jitter on this corpus so boundary flips do not occur? Express it as
   a measurable threshold a fixture author can reproduce, with the data behind it.
3. **Is the recommended corpus a non-degenerate ranking?** Confirm it preserves a real ranked
   retrieval (multiple ranked hits, enough entries to be a genuine ranking — nan-022 Open Q1),
   not a trivial single-hit fixture that passes vacuously. The stability must come from margin
   design, not from collapsing the test.
4. **Impact on the existing test strategy.** Assess how adopting a wide-margin parity corpus
   interacts with: (a) the nan-022 parity matrix dimensions that **share** the workload/corpus
   (D2 behavioral, D3 analytics, D6 isolation) — does changing the corpus weaken or break any
   of them?; (b) the separate **eval-sweep** harness (#746 cond.1, its own ~25-entry corpus) —
   does this recommendation transfer, conflict, or stay independent?; (c) the division of
   labor where **parity detects egregious divergence** and **ranking-precision-at-boundary is
   tested elsewhere** — name the compensating coverage (e.g. a single-transport golden
   ranking-precision test) the change requires so precision sensitivity is not silently lost.
   Enumerate any regression or coverage gap the change introduces.

## Breadth

`code-only`. The corpus, harness, and HNSW integration are internal
(`harness/parity_workload.py`, the retrieval capture, `scripts/bridge-cycle-driver.js`, the
`hnsw_rs` usage in the vector crate); the relevant history/lessons are internal Unimatrix
knowledge (bugfix-742, #746/#4990). No ecosystem or literature input.

## Approach

`measurement` (empirical) + a `proof-of-concept` corpus build. Build candidate corpora that
vary entry count and inter-rank margin; measure the top-k flip rate for each; identify the
minimal design that reaches **zero flips at the sampling bar**, then assess test-strategy
impact.

**Measure at the cheapest faithful layer.** The flip is a property of the per-process HNSW
index rebuild + top-k query and is largely transport-independent; do the high-iteration
measurement at the off-Docker unit/seam layer (the nan-019/021 precedent) — a tight
rebuild-index-and-query loop — where ≥150 iterations is fast, then **confirm** the recommended
corpus with a feasible number of full cross-transport (Docker HTTPS + UDS) runs. Do NOT attempt
150 full Docker rounds; that is neither necessary nor lane-safe.

## Confidence required

`empirical`. The recommendation must be backed by measured flip-rate data per candidate
corpus, at the sampling bar. A "this margin works" claim without the iteration data behind it
does not satisfy this spike.

## Target outputs

FINDINGS.md containing:
- **The recommended corpus** (Goal 1/2/3): concrete entry set / margin threshold / query, with
  the measured flip-rate table per candidate and the iteration counts.
- **The minimum-margin threshold** with its supporting data and how a fixture author reproduces
  it.
- **A non-degeneracy confirmation** (it is a real ranking).
- **A test-strategy impact assessment** (Goal 4): shared-corpus dimension effects, eval-sweep
  relationship, and the compensating ranking-precision coverage the change requires — with any
  regression/coverage gap called out.
- A clear input for the **#844 reframe**: whether the wide-margin corpus removes the need for a
  D1/D4 C0 (#5304) documented exception (the likely strong outcome) or not.

## Constraints

**Hard** (changing these means changing shipped code or abandoning the test's integrity):
- **Test-only.** No production change. This spike does **not** fix HNSW determinism
  (#746/#4990 owns that); it designs the fixture to be *insensitive* to the jitter.
- **No tolerance band.** The parity assertion must stay **exact top-k membership + order**; the
  corpus must achieve stability so a band is unnecessary. A tolerance/tie-swallowing
  "solution" is explicitly out of bounds — it risks masking real regressions (the #844 R-01
  warning). The lever is corpus design, not loosened assertions.
- **Sampling bar (verified, bugfix-742).** Stability claims require **≥150 iterations under
  contention** at the measurement layer; a 40-run green streak under-samples a ~2% flake
  (expected <1 failure in 40). Never declare stable on a small sample.
- **Extend infra-001 cumulatively** — no fork, no parallel scaffold. PoC corpora are throwaway
  and not committed as fixture changes.
- **Read-only in Unimatrix.**

**Hypothesis** (positions to test, not assume):
- **"Wide inter-entry similarity separation reliably eliminates the boundary flip."** The
  human's premise — validate it empirically; it may require an impractically large margin or
  entry count, or may not fully reach zero. Report honestly if the margin needed is impractical.
- **"A single shared corpus can serve all parity dimensions AND be wide-margin-stable for
  retrieval."** Retrieval may need its own sub-corpus or augmented entries; test whether one
  corpus can satisfy both without weakening the other dimensions (Goal 4a).

## Dependencies

- **Input / prior art:** #844 (the D1/D4 RED + evidence); nan-022 SCOPE
  (`product/features/nan-022/SCOPE.md`, Open Q1 workload shape, Open Q2 retrieval determinism
  tolerance-vs-exact — this spike informs Q2); the infra-001 harness
  (`harness/parity_workload.py` corpus + the retrieval capture, `scripts/bridge-cycle-driver.js`).
- **Lessons / history:** bugfix-742 (cond.1 = HNSW approximate top-k membership flip; the
  search.rs sum fix + single-materialization fixed the float-drift assertion but NOT cond.1;
  cond.1 deferred to #746 with the candidate fix "widen eval retrieval breadth, k stays 5"; the
  ≥150x sampling bar). #746 / #4990 (HNSW approximate top-k, no public seed API — the root
  cause this spike designs around).
- **Unblocks:** the **#844 reframe** (wide-margin corpus as the test-only D1/D4 fix) and the
  D1/D4 path to a clean parity PASS without a C0 exception.

## Prior art

- **#844** — failing run `[3,1,5,4]` (HTTPS) vs `[3,1,2,5,4]` (UDS); both legs intra-stable per
  run, flip across runs; D4 tracks D1; root cause #746/#4990.
- **bugfix-742 lesson** — cond.1 HNSW flip on a ~25-entry eval corpus, top-5 approximate, index
  rebuilt from OS entropy per invocation; a final-sort id tie-break was prototyped and dropped
  (no effect); the candidate fix is to widen retrieval breadth while keeping metric k=5; the
  ≥150x-under-contention sampling bar.
- **nan-022 SCOPE Open Q1/Q2** — workload shape vs retrieval/briefing (needs a non-degenerate
  ranking); retrieval determinism tolerance-vs-exact (the human's call this spike informs).
- **#746 / #4990** — HNSW approximate top-k membership flip; `hnsw_rs` 0.3.4 has no seeding API.

## Tracking

GH research issue **#852** (ass-085). Reframed **#844** depends on this spike's recommended
corpus + test-strategy impact assessment. Single spike → execute via `uni-spike-researcher`
once scope is confirmed complete.
