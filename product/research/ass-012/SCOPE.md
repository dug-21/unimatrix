# ASS-012: π-Based Quantization De-Resonance & Coherence Gating

**Type:** Research Spike (Mathematical Foundations & Applicability Analysis)
**Date:** 2026-02-27

**Purpose:** Investigate the mathematical foundations of using π (and irrational constants generally) as structural de-alignment tools in quantized systems, assess the claimed mechanism of quantization resonance in low-precision deterministic systems, and evaluate relevance to Unimatrix's numerical subsystems (confidence scoring, embedding similarity, search re-ranking).

---

## Core Concept Under Investigation

### The Claim

In low-precision quantized systems (3/5/7-bit), weight values, thresholds, and sparse masks all snap to binary grid points. When multiple quantized components share similar binary boundaries, rounding errors can form repeating patterns that reinforce each other — a phenomenon termed **quantization resonance**. In always-on deterministic systems, this resonance causes systematic drift over time.

### The Proposed Solution: π-Based Calibration

Using digits of π as calibration constants introduces **structural de-alignment**. Because π is:
- **Irrational** — its decimal expansion never terminates or repeats
- **Transcendental** — it is not a root of any polynomial with rational coefficients
- **Binary-incommensurate** — no finite binary fraction can represent it exactly

...scaling thresholds and sparse masks by π-derived constants prevents the system from locking into binary-aligned resonance cycles. This is not randomness — it is deterministic aperiodicity.

### Layered Precision Lanes

Complementing π-calibration is an adaptive precision system:
- Signals start in 3-bit (cheap)
- Graduate to 5-bit or 7-bit if novelty or drift increases
- Drop back to 3-bit when stable
- Compute follows information value

### λ (Lambda) Coherence Gate

A min-cut coherence signal (λ) measures structural stability across the system, gating updates. If coherence drops below threshold, updates are suppressed or precision is escalated.

### Convergence Observation

The ruQu system independently derived the same coherence gate invariant, converging on π as a mathematical primitive. Independent convergence on the same invariant suggests a fundamental rather than incidental relationship.

---

## Research Questions

1. What is the mathematical foundation for irrational constants breaking periodicity in discrete systems?
2. Is quantization resonance a documented phenomenon in ML/signal processing literature?
3. Does Unimatrix have any numerical subsystems vulnerable to analogous drift patterns?
4. Could π-based calibration or coherence gating concepts improve Unimatrix's scoring/ranking?

---

## Outcome (2026-02-27)

### Answers

1. **Weyl's Equidistribution Theorem (1909)** provides rigorous foundation. Multiples of any irrational number are provably equidistributed mod 1. π is transcendental, giving even stronger guarantees than algebraic irrationals (φ, √2). Well-established in quasi-Monte Carlo sampling, anti-aliasing, and phyllotaxis. → `findings/01-mathematical-foundations.md`

2. **Partially documented.** Weight oscillation in QAT (Nagel et al., ICML 2022), correlated quantization noise in signal processing, and error propagation in iterative quantized models are all established. The unifying "resonance" framing and irrational-constant solution are novel contributions. → `findings/02-quantization-resonance.md`

3. **Yes — two current issues, two future concerns.** Stale stored confidence (freshness component never recomputed) causes systematic over-ranking of old entries. HNSW graph accumulates stale nodes from re-embeds. At scale, embedding quantization and score clustering become relevant. → `findings/03-unimatrix-relevance.md`

4. **The λ coherence gate is the highest-value concept.** A unified coherence metric gating self-maintenance operations (confidence refresh, graph compaction, re-embed, contradiction scan) maps directly to Unimatrix's architecture. π-calibration of scoring constants is theoretically interesting but practically negligible at f32 precision. → `findings/03-unimatrix-relevance.md`, `findings/04-convergence-analysis.md`

### Key Takeaways for Unimatrix

| Concept | Relevance | When |
|---------|-----------|------|
| λ coherence gate | **High** — unifies existing maintenance signals | Now (design) |
| Stale confidence refresh | **Medium** — existing drift behavior | Now (known issue) |
| HNSW graph compaction | **Medium** — stale node accumulation | Now (known issue) |
| Embedding quantization with π-calibration | **Low** — full f32 sufficient currently | At scale (>50K entries) |
| Adaptive precision lanes | **Low** — no multi-precision path exists | Future architecture |
| π-scaled scoring weights | **Very low** — ULP-level artifacts at f32 | Theoretical only |

### Findings

- `findings/01-mathematical-foundations.md` — Weyl's theorem, equidistribution, why π works
- `findings/02-quantization-resonance.md` — Literature validation, what's novel vs prior art
- `findings/03-unimatrix-relevance.md` — Full codebase analysis, 6 areas assessed
- `findings/04-convergence-analysis.md` — Independent derivation analysis, min-cut λ, π as universal primitive
