# Test Plan — Primary fixture corpus assets + AC-14 proof-by-use (`eval/corpus/fixtures/`)

**Component**: in-repo TOML/JSON fixture entry-graphs (the five status shapes) + manifest stamp under `crates/unimatrix-server/src/eval/corpus/fixtures/`.
**Wave**: 1. **Primary risks**: R-09 (corpus audit, High), R-15 (AC-14 trivial-pass, High — **the Wave-1 exit gate**).

This component owns **two of the three non-negotiable Wave-1 backstop tests** (R-09 audit, R-15 non-vacuous AC-14).

## R-09 — static corpus audit — AC-05 — **Wave-1 backstop test #1 (may NOT be deferred)**
- `test_primary_corpus_audit_zero_literal_id_zero_null`: scan **every** shipped primary-corpus scenario; assert **zero literal-ID `expected`** and **zero null `expected`** — every scenario uses **only** `redirect-to-head` / `absence` / `rank-below` property assertions. This is a static audit over the shipped assets (distinct from the loader-rejection tests in `corpus-loader.md`, which test the *gate*; this tests the *shipped corpus passes the gate*).

## AC-06 — corpus presence + loads-and-searches — AC-06
- `test_corpus_contains_required_four_shapes`: assert presence of multi-correction chain, dangling chain, superseded-Active, deprecated-connected (optional 5th dead-end chain).
- `test_each_shape_loads_and_searches`: each shape loads via the corpus loader and runs through the search replay path without error.

## AC-14 — non-vacuous proof-by-use (R-15) — **Wave-1 EXIT GATE / backstop test #3 (may NOT be deferred)**

`test_ac14_correlated_sweep_non_vacuous` — one `eval run` of a steepness sweep across ≥2 profiles on the fixture corpus. **All five conditions asserted; failing any one fails AC-14:**

1. **Correlated four-family report**: assert the report section contains, for the **same scenarios**, trust outcomes (AC-02/03) AND P@5/MRR (AC-04) AND token-weighted cost (AC-09) — all four families present together.
2. **Non-vacuous trust** (the load-bearing assertion): assert **≥1 trust assertion is evaluated against a non-empty result set** — the anchor entries are present so the assertion is *meaningfully* checked, not vacuously satisfied. Inspect the evaluated-assertion count / `TrustOutcome.violations`, **not** merely `pass == true`. (A `rank_below(A,B)` where both A and B are in top-k is the canonical non-vacuous case; an empty-result absence-pass does NOT count.)
3. **Each-shape-exercised**: assert each of the 4 required shapes loads and yields **≥1 evaluated assertion**.
4. **Observable lever delta**: the two profiles differ in one penalty lever; assert a **non-zero** penalty/ranking delta between them in the report (lever proven live, not inert).
5. **Bit-for-bit baseline on a guarded corpus**: assert the swept **baseline** (default penalties) reproduces current behavior **bit-for-bit** (R-01 default-equivalence green — cross-ref `engine-penalty.md`/`search-threading.md`) AND the corpus is guarded by a **deterministic, actually-firing** drift guard (R-03 + R-06 green — cross-ref `shape-hash.md`).

> Proof-by-use = proof the instrument **MEASURES a moving trust signal**, not that the harness runs. Condition 2 is where the R-15 trivial-pass trap is closed. A sweep that executes end-to-end but fails any of 1–5 does NOT satisfy the exit.

## Corpus authoring depth (ADR-004 §5 — beyond the AC-14 floor) — non-test obligation
- `test_deprecated_connected_crossover_is_bracketed` (if mechanizable): assert the deprecated-but-connected shape has enough variation that the steepness crossover (where connected-deprecated crosses the weakest-active threshold) sits inside a **bracketed range** of points, not a single exemplar — so ass-073's sweep rests on real evidence. If not cleanly mechanizable, this is a **design-review obligation** flagged to the leader (AC-14 proves the corpus *measures*, not that it is a good-enough yardstick).
- **Named revision loop**: "ass-073 finds Wave-1 corpus insufficient → revise + re-stamp" is an anticipated, valid loop — budget one revision pass. Not a test; noted for the leader.

## Edge cases
- k larger than corpus size: every entry returned ⇒ absence assertions become strict (cross-ref `trust-metric.md`).
- Dead-end chain (optional 5th shape): redirect-to-head has no valid head ⇒ defined failure.
