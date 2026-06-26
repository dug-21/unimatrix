# FINDINGS: Empirically identify the best parity-retrieval corpus — wide-margin design vs HNSW top-k boundary flips (D1/D4)

**Spike**: ass-085 | **Date**: 2026-06-26 | **Approach**: measurement (empirical) + throwaway PoC corpora | **Confidence**: empirical (≥150-iteration bar; 300–500 iters per candidate)

## Executive answer (read first)

The human's premise — *"wide inter-entry similarity separation reliably eliminates the boundary flip"* — **does not hold as stated.** Margin is **not** the dominant lever and is **non-monotonic**; wider margins do not reduce flips and are geometrically capped. The dominant lever is **entry count**.

A well-designed corpus (N≈25, entries well above k, a wide boundary moat) reduces the flip **~30×** (~14% → ~0.2–0.6%) and **eliminates the catastrophic short-return mode that is the literal #844 signature** — a real, worth-adopting improvement. **But no lever available to this spike — entry count, inter-rank margin, boundary moat, or requested retrieval breadth (effective ef) — drives the cross-process top-k flip to a hard zero at the sampling bar.** The residual floor is **~0.2–1.2% (≈1–6 per 500)**, **intrinsic to `hnsw_rs` per-process build randomness (no seed API, #746/#4990)**. It appears even inside the **top-3** prefix, so narrowing asserted depth does not escape it.

**Consequence for #844: the wide-margin corpus does NOT remove the need for the D1/D4 C0 (#5304) documented exception.** It complements — does not replace — the existing ADR-004 stable-prefix/tie-class policy.

## Method (cheapest faithful layer)

Off-Docker seam: a tight rebuild-HNSW-and-query loop using workspace-locked `hnsw_rs` 0.3.3 / `anndists` 0.1.4 with exact production params (`DistDot`, dim=384, M=16, ef_construction=200, max_layer=16, ef_search=32 per `services/search.rs::EF_SEARCH`, top_k=5 per harness `k=SEED_CORPUS_SIZE`). Compiled inside `unimatrix-vector` so HNSW behavior is byte-identical to production, then deleted.

- **Faithful jitter source:** `hnsw_rs` draws layer-assignment randomness from `thread_rng`, which advances between successive `Hnsw::new` builds in one process — reproducing the per-process build variance that appears as cross-process (HTTPS-vs-UDS) variance in production (#4990). Largely transport-independent, so the seam is the correct sampling layer.
- **Synthetic margin control:** query=e₀; entry *i* has explicit cosine-to-query `sᵢ` plus a fixed pseudo-random orthogonal component (mutually near-orthogonal = "distinct subjects, shared topic"). Geometry fixed across iterations; only HNSW's internal RNG varies. Exact reference computed analytically. Flip = returned top-k (id **and** order) ≠ exact.
- Sampling: 300 iters sweeps, 500 iters confirmation (> the ≥150 bar). Lockfile resolves `hnsw_rs` 0.3.3 (scope says 0.3.4); same 0.3.x family, no seed API either.

## Findings

### Q1: What corpus design eliminates the flip — empirically? (entry count, margin, query construction)

**Answer:** No design eliminates it to hard zero at the bar. Best achievable (reaches the irreducible floor, removes catastrophic regime):
- **Entry count N ≈ 25** on the shared topic. The current `SEED_CORPUS_SIZE=5` is the primary defect. N≥8 eliminates short-returns; N≈25 reaches the floor; N=40 adds nothing.
- **Boundary "moat" ≥ 0.20 cosine** between intended head and tail, head internally separated ~0.02–0.03. Secondary — only matters at N≈12; at N≈25 moat barely moves the floor.
- **Query construction:** single on-topic query; requested breadth does not help (Q2).

Full sweep (`s0=0.80`, uniform margin δ, 300 iters; flip% = top-5 id+order ≠ exact):

| N | δ=0.002 | δ=0.005 | δ=0.01 | δ=0.02 | δ=0.05 | δ=0.10 |
|---|---|---|---|---|---|---|
| 5 | 11.7%(all short) | 15.3% | 12.0% | 11.0% | 19.3% | 24.7% |
| 6 | 14.7% | 11.3% | 11.3% | 10.3% | 18.7% | 18.0% |
| 8 | 6.0% | 9.7% | 9.3% | 10.7% | 19.0% | 10.0% |
| 12 | 0.33% | 1.33% | 13.3% | 16.7% | 21.7% | 1.0% |
| 25 | 0.33% | 0.67% | 0.67% | 0.67% | 1.33% | 100%* |

\* N=25/δ=0.10=100% is a corpus-construction artifact (sᵢ crosses below −1 → `√(1−s²)` NaN → poisoned vectors); it is *evidence for the geometric cap* (Q2), excluded from interpretation. Later probes clamp `|s|≤0.95`. Two facts dominate: **N is the lever** (11–25% band collapses only at N≈25); **margin is non-monotonic and not protective**.

Boundary-moat design (top-5 tight at 0.75/int-0.02, then moat, then tail; 300 iters):

| N | moat=0.02 | 0.05 | 0.10 | 0.20 | 0.30 |
|---|---|---|---|---|---|
| 12 | 13.3% | 12.7% | 13.0% | 2.7% | 1.3% |
| 25 | 0.67% | 0.33% | 0.33% | **0.00%** | 0.33% |
| 40 | 0.33% | 0.00% | 1.0% | 0.33% | 0.67% |

The lone 0.00%/300 cell **did not hold** at 500 iters (3/500) — exactly why the ≥150 bar exists. High-iteration confirm (500 iters):

| Candidate | top-5 flip% | short% |
|---|---|---|
| **N=5 uniform (current-corpus analog)** | **13.8%** | **13.8% (all short)** |
| N=8, moat 0.10 | 14.4% | 0.40% |
| N=12, moat 0.10 | 12.6% | 0.20% |
| N=12, moat 0.20 | 3.8% | 0.20% |
| N=25, moat 0.10 | 0.20% | 0.00% |
| N=25, moat 0.20 | 0.60% | 0.00% |
| N=25, int 0.03, moat 0.20 | 0.40% | 0.00% |
| N=40, moat 0.20 | 0.40% | 0.00% |

**Recommendation:** Raise `SEED_CORPUS_SIZE` 5 → ~25 distinct-subject entries with a ≥0.20-cosine boundary moat and ~0.02–0.03 internal head separation. Adopt it (~30× improvement, kills short-returns). Do **not** expect exact-parity non-flakiness — floor ~0.2–0.6%, not zero (Q2). Pair with ADR-004 stable-prefix or a documented exception.

### Q2: Minimum inter-rank margin that dominates HNSW jitter — reproducible threshold

**Answer: there is no such margin.** Three measured reasons:

1. **Non-monotonic (margin isn't the lever):** increasing δ 0.002→0.10 at fixed N does not reduce and often increases the flip (N=8: 6.0%→19.0%; N=12: 1.3%@0.005 vs 21.7%@0.05). Margin helps only as a moat and only at N≈12; at N≈25 moat barely moves the ~0.4% floor.
2. **Geometric ceiling:** cosine ∈ [−1,1]; on-topic MiniLM short texts cluster ~0.3–0.7. 5 ranks + a 0.20 moat already span ~0.3; wider drives vectors past `|s|=1` (the NaN artifact). "Arbitrarily wide separation" is **not physically realizable**.
3. **Residual is not a boundary effect:** raising effective ef above N (near-exhaustive) does not clear it (N=25, moat 0.20, 500 iters):

| requested k | effective ef | top-5 flip% |
|---|---|---|
| 5 | 32 | 0.60% |
| 16 | 32 | 0.40% |
| 25 | 32 | 0.40% |
| 32 | 32 | 0.20% |
| 40 | 40 | 0.60% |
| 50 | 50 | 0.80% |
| 64 | 64 | 0.40% |

With ef=64 over 25 points the search visits essentially the whole graph, yet ~0.4% still mis-rank top-5; it also surfaces in the **top-3** prefix (moat 0.10→0.4–0.6%, 0.20→0.2–0.4%, 0.30→up to 1.2%). The residual is a **graph-construction** property — a node stranded by per-process layer-assignment RNG — i.e. the `hnsw_rs` no-seed-API root cause (#746/#4990), which corpus design cannot touch.

**Reproducible floor-reaching recipe (since a zero-flip margin doesn't exist):** N≥25 (above all escape the N≤8 short-return regime); moat ≥0.20; internal head separation ~0.02–0.03; verify at the seam with a rebuild-and-query loop ≥150 iters (this spike's probe is the template), expecting a ~0.2–0.6% residual. Higher rate ⇒ mis-designed corpus; "zero on 40 runs" ⇒ under-sampling.

### Q3: Is the recommended corpus a non-degenerate ranking?

**Answer: yes — non-degeneracy is strengthened, not traded away.** Stability comes from *adding* entries (5→25), making the ranking deeper. The only near-zero-flip config is top-1 (single hit) — the forbidden vacuous case (#5177). A genuine top-3/top-5 over 25 entries cannot be made exactly stable (Q2), which confirms it is genuinely ranked. The current N=k=5 is borderline-degenerate the wrong way: it returns <5 results 11–25% of the time (under-ranked failure). The recommendation keeps ranking depth ≥ STABLE_PREFIX_FLOOR=3 with head-room. Non-degeneracy and stability both want **more entries**.

### Q4: Impact on existing test strategy

**(a) D2/D3/D6 shared corpus — low risk, additive.** Corpus is Phase-1 `context_store` calls with `observe=False` in the single `ParityWorkload`. D2/D3 ride the Phase-2 observe cycle + `MetricVector` (unchanged); `expected_observe_count` (barrier predicate) counts only `observe=True` calls, so seeds don't move the barrier. D6 just sees a larger per-slug store. Only real interaction: more writes lengthen Phase-1 / grow the store dir → may need a longer durability-barrier deadline (a timing tunable, not correctness). **Hypothesis-2 verdict:** one shared corpus *can* serve D2/D3/D6 while enlarged for D1/D4 — retrieval needs no separate sub-corpus for their sake (it just doesn't gain exact stability from it).

**(b) Eval-sweep (#746).** Same `hnsw_rs` build-RNG mechanism. The recorded candidate fix *"widen eval retrieval breadth, k stays 5"* is **empirically insufficient** — probe-4 raised effective ef to 64 over 25 entries and the floor stayed ~0.4%. Widening breadth cuts short-returns, not the membership flip. #746 should treat "widen breadth" as mitigation, not fix. Corpora otherwise independent (no shared module) — a transferable lesson, not coupling.

**(c) Compensating ranking-precision coverage.** With parity demoted to "detect egregious divergence," fine boundary-precision sensitivity must move to a **single-transport golden ranking-precision test** (eval-sweep layer or vector-crate seam) asserting exact top-k order over a known-ordered corpus. Because even single-transport exact top-k flickers ~0.4% (Q2), that golden test must itself assert a stable shallow prefix or run under the same intrinsic-floor disposition — not a naive exact-order assertion at the ≥150 bar.

**Enumerated regression/coverage gaps:**
1. **Lost boundary-precision sensitivity in parity** — a wide-margin corpus no longer detects a small boundary-only ranking regression; that moves to (c). If (c) is not added, sensitivity is silently lost.
2. **Residual-floor flake remains** — corpus design leaves ~0.4%; an exact D1/D4 assertion at the bar still eventually reds. This is the gap forcing ADR-004 or the exception.
3. **Tension with already-stored ADR-004** (#5315/#5308 stable-prefix + tie-class; #5316 prefix/tail = STABLE_PREFIX_FLOOR). The 2026-06-26 "wide-margin, no tolerance band, exact assertion" direction is partially incompatible. Empirical resolution: wide-margin corpus is a valuable *complement* (stronger prefix signal, kills short-returns) but cannot *replace* tolerance/exception since exact-zero is unreachable. A stable-*prefix* assertion is not the forbidden score-epsilon band — but probe-3 shows even a shallow exact prefix flickers, so it must pair with tie-class handling or an exception. This is a product/human disposition (ADR-004(5)), surfaced with data.

**Does the wide-margin corpus remove the D1/D4 C0 (#5304) documented exception? No.** It removes the catastrophic short-return failure and cuts the flip ~30×, but the ~0.2–0.6% intrinsic `hnsw_rs` build-RNG floor survives every corpus/breadth/prefix lever. Per ADR-004(5)'s own rule, the disposition stands: keep ADR-004's stable-prefix/tie-class policy (already built) and/or file the human-signed C0 exception; adopt the wide-margin corpus as the complement that makes both the prefix signal and any exception narrower and better-justified.

## Unanswered Questions

- **Real-embedding (MiniLM) confirmation** — measurements used synthetic geometry-controlled vectors (faithful to the build-RNG jitter and to "distinct subjects, shared topic"). Whether a real 25-subject MiniLM corpus can realize a ≥0.20 moat, and whether its real floor matches ~0.4%, was not measured (ONNX model not exercised). Deferred on cost; mechanism proved geometry-robust, so the floor is expected to persist. Cheap to check at fixture-authoring time: embed the 25 subjects, print the pairwise cosine matrix.
- **Full Docker HTTPS+UDS confirmation of the residual** — deliberately not run as a "confirmation" (a reasoned decision, not a capability gap; Docker is available). The residual is a ~1-in-250 event; a feasible handful of rounds returns all-green by under-sampling — the exact bugfix-742 false-confidence trap. The seam at ≥150–500 iters is authoritative; a small Docker run can confirm plumbing (seeds, both legs rank, comparator fires) but **not** zero, and must not be cited as doing so. Recommend a one-time small Docker smoke for plumbing only, labeled non-statistical.

## Out-of-Scope Discoveries

- **Current parity corpus (N=k=5) sits in the worst regime** — returns <5 results in 11–25% of builds; short-returns are the literal #844 signature (`[3,1,5,4]` = 4 returned). Reframes #844's primary cause as **under-population**, not boundary-margin subtlety. Worth fixing regardless of the margin debate (N≥8 eliminates it).
- **bugfix-742's #746 candidate fix ("widen breadth, k stays 5") is empirically insufficient** — effective ef=64 ≫ N still floors at ~0.4%. Flag for #746 planning.
- **`hnsw_rs` 0.3.x residual is build-time, not search-time** — flip survives near-exhaustive ef, so a true determinism fix must address graph *construction* (seed API / deterministic layer assignment), not search breadth. Sharpens the #746/#4990 direction. Recorded here only — provisional, not stored to Unimatrix.

## Recommendations Summary

- **Q1 (corpus):** Raise `SEED_CORPUS_SIZE` 5 → ~25 distinct-subject entries, ≥0.20-cosine boundary moat, ~0.02–0.03 internal head separation. ~30× improvement, removes short-returns — adopt it — but it does not reach zero.
- **Q2 (margin):** No finite inter-rank margin zeroes the flip; margin is non-monotonic, geometrically capped, and the residual is build-RNG, not margin. Floor-reaching recipe: N≥25, moat≥0.20, verify ≥150 iters expecting ~0.2–0.6% residual.
- **Q3 (non-degeneracy):** Recommended corpus is a genuine, deeper ranking; stability comes from added entries, never from collapsing the ranking. Top-1 is the only near-stable depth and is the forbidden vacuous case.
- **Q4 (test strategy):** Enlarging the shared corpus is additive/safe for D2/D3/D6 (only a barrier-deadline tunable); transfers to #746 as a mechanism lesson and **contradicts** the "widen breadth" candidate fix; requires a single-transport golden ranking-precision test to retain boundary sensitivity. **The wide-margin corpus does NOT remove the D1/D4 C0 (#5304) exception** — keep ADR-004's stable-prefix/tie-class policy and/or file the human-signed documented exception; the corpus is the complement, not the replacement.
