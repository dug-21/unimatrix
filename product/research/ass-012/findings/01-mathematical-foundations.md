# Finding 01: Mathematical Foundations for Irrational De-Alignment

**Date:** 2026-02-27
**Status:** Validated — the mathematical claim is well-grounded

---

## Weyl's Equidistribution Theorem (1909)

The formal foundation for π breaking periodicity is **Weyl's Equidistribution Theorem**:

> If α is irrational, then the sequence {nα mod 1} for n = 1, 2, 3, ... is **equidistributed** on [0, 1].

This means multiples of any irrational number, taken modulo 1, never cluster — they fill the unit interval uniformly. Crucially, if α is **rational**, the sequence is periodic and clustering occurs.

**Implication for quantization:** When scaling thresholds by π, the fractional parts of π-scaled values are guaranteed to never repeat or align with a binary grid. No finite binary representation can capture π, so every scaled value introduces a unique offset relative to the nearest binary grid point. The rounding errors produced are equidistributed rather than periodic.

**Source:** [Weyl's Equidistribution Theorem](https://en.wikipedia.org/wiki/Equidistribution_theorem), also [IAS Resonance article](https://www.ias.ac.in/article/fulltext/reso/008/05/0030-0037)

---

## Irrational Rotation ↔ Aperiodicity Equivalence

A stronger result: there is a formal equivalence between the set of irrational numbers and the set of all aperiodic function trajectories (in discrete time, finite precision). Conversely, rational numbers correspond to periodic functions.

This means using π (or any irrational) as a scaling constant is **provably** the correct tool for preventing periodicity in discrete systems. The choice of π specifically (vs φ, √2, e) is aesthetic — all transcendental irrationals provide the same equidistribution guarantee.

**Source:** [Aperiodic Irrationals](https://blbadger.github.io/aperiodic-irrationals.html)

---

## Low-Discrepancy Sequences: The Applied Version

The practical application of this principle is well-established in quasi-Monte Carlo methods:

- **Halton sequences** use prime bases to generate uniform partitions
- **Weyl sequences** use irrational increments (α, 2α, 3α...) to fill space without clustering
- The **golden ratio φ** is provably the optimal irrational for 1D low-discrepancy sequences because it is the "most irrational" number (hardest to approximate by rationals)

π is not the most irrational number (φ is), but π is transcendental, which provides a stronger guarantee: it is not a root of any polynomial with rational coefficients, making it impossible for polynomial rounding patterns to align with it.

**Source:** [Unreasonable Effectiveness of Quasirandom Sequences](https://extremelearning.com.au/unreasonable-effectiveness-of-quasirandom-sequences/), [Low-discrepancy sequences (Wikipedia)](https://en.wikipedia.org/wiki/Low-discrepancy_sequence)

---

## Why π Specifically (vs φ or e)

| Irrational | Transcendental? | Binary-incommensurate? | Practical advantage |
|-----------|----------------|----------------------|-------------------|
| √2 | No (algebraic) | Yes | Satisfies x²=2; polynomial rounding could theoretically align |
| φ (golden ratio) | No (algebraic) | Yes | "Most irrational" for 1D sequences; root of x²-x-1=0 |
| e | Yes | Yes | Natural exponential base; appears in decay functions |
| **π** | **Yes** | **Yes** | **No polynomial relationship; deeply embedded in circular/wave math** |

All provide equidistribution. π's advantage is **conceptual**: in systems dealing with wave-like phenomena (oscillation, resonance), π already describes the underlying periodicity being disrupted. Using π to break π-periodic patterns has a structural elegance.

---

## Finite Precision Caveat

In practice, computers truncate π to finite precision (f32: ~7 digits, f64: ~15 digits). A truncated π is **rational**. However:

- At f64 precision, the period of π-derived rounding patterns would be astronomically long (~10^15 steps)
- For practical systems, this period far exceeds operational lifetime
- The de-alignment benefit holds for all practical purposes even with truncated representations

---

## Verdict

**The mathematical claim is sound.** Weyl's theorem provides a rigorous foundation. The use of irrational constants to prevent periodic alignment in discrete systems is a well-established technique in quasi-random sampling, anti-aliasing, and phyllotaxis. Applying it to quantization calibration is a novel but mathematically valid extension.
