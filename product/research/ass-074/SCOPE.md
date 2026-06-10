# ASS-074: PPR Re-Evaluation + Retrieval Tuning Re-Validation at Current Corpus Scale

**Date**: 2026-06-10
**Spike type**: Measurement — eval harness (nan-018), profile sweeps on the realism snapshot
**Status**: SCOPE
**Working number**: ass-074 (provisional)
**Origin**: uni-zero PPR decision-history trace (2026-06-10), prompted by ass-073's finding that PPR is inert in the fixture corpus.

## Problem Statement

PPR (Personalized PageRank) exists to widen search beyond close semantic matches — to surface entries that are *related but not similar* to the query. The decision history shows it is currently in a contradictory, likely-dead state, and nobody has made a clean decision about it:

- It walks five positive edge types (Supports, CoAccess, Prerequisite, Informs, RelatedTo — `graph_ppr.rs`; RelatedTo added vnc-015 ADR-006 #4429; Advances/Motivates deliberately excluded).
- **ASS-037 (#3984) measured PPR at zero P@5/MRR delta** — yet it was retained at `ppr_blend_weight=0.15`.
- **ASS-037 Q3b (#3988) recommended `ppr_blend_weight=0.0`, `ppr_max_expand=0`** (zero value at 1,134 entries / 0.14 Informs-density) — **never applied; no decision overrode it.**
- **ASS-038 (#3989)** found the root cause is architecture: PPR is a *re-ranker within k=20*, it cannot reach entries outside the HNSW candidate set. Density isn't the bottleneck.
- **crt-042** built the fix — an **expander** (BFS from HNSW seeds to widen the pool before ranking) — but it ships **off** (`ppr_expander_enabled=false`, verified `config.rs:1102`).
- **crt-032 (#3785)** zeroed `w_coac`, making PPR the **sole carrier of the co-access signal** — so zeroing PPR also drops co-access.

Two facts make this newly testable: the prod corpus is now ~6,700 vectors (past Q3b's 5,000-entry retest threshold), and nan-018 shipped a harness that sweeps these knobs. The question: **does PPR earn its keep (expander on, at current scale) — or get zeroed out?**

## Why this matters to crt-053 (the load-bearing coupling)

crt-053's injection-path leak fixes (ES-4/5/6 — a stale entry re-admitted via PPR/graph_expand injection at `penalty=1.0`) **only matter if PPR is actually injecting.** ass-073 already confirmed the **HNSW-path eviction** is crt-053's load-bearing fix; the injection-path leak was untestable (fixtures have no positive edges). So ass-074's PPR-activity finding directly shapes crt-053's scope:

- **If PPR is dead** (no edges / expander off / zero delta) → the injection leak is **dormant**; crt-053 narrows to eviction + redirect on the HNSW path and **defers ES-4/5/6** until PPR is turned on. Smaller crt-053.
- **If PPR is live** (or about to be activated) → ES-4/5/6 is a real correctness gap and stays in crt-053 scope; crt-053 is the **guardrail that must land before PPR expansion is enabled.**

So a first-class output of this spike is an explicit **scope recommendation to crt-053** (#717). crt-053 stays HOLD until this returns.

## Phase 0 — Edge Inventory (GO/NO-GO GATE)

**Measure the positive-edge density in the current prod snapshot before running anything else.** Per-entry counts of RelatedTo / Supports / Informs / CoAccess / Prerequisite (the five PPR types), plus the totals and the Q3b density metric (Informs/entry).

**Gate:** if positive-edge density is near-zero / below the Q3b retest threshold (≈1.0/entry), **STOP — do not run the sweep.** The finding is then: *PPR is starved because the corpus doesn't create positive edges — a process problem, not an algorithm problem. The injection-path leak (crt-053 ES-4/5/6) is dormant. crt-053 should defer it.* A sweep over a graph with no edges measures nothing.

If density is sufficient, proceed to Phases 1–3.

## Phase 1 — PPR re-evaluation (only if Phase 0 passes)

Profiles on the realism snapshot: **PPR-off** / **re-ranker (blend 0.15)** / **expander-on (`ppr_expander_enabled=true`)**. Measure P@5 / MRR / CC@k / ICD / cost. The expander-on profile is the one that matters — the re-ranker has already been proven zero-delta twice (#3984, #3988); the open question is whether the crt-042 expander adds value at current scale + density.

## Phase 2 — Co-access coupling

Because crt-032 made PPR the sole co-access carrier, a "zero PPR" decision silently drops co-access. Isolate whether co-access still contributes at current scale (e.g., a profile separating the co-access signal) so the keep-or-zero decision is made eyes-open, not blind to a side effect.

## Phase 3 — Formula sanity (bounded)

ASS-037's optimal (`w_sim=0.50` / `w_conf=0.35`, all else 0) was fit at 1,134 entries; the corpus is ~6x larger. A **light** check that the formula still holds at current scale (re-run the ASS-037 conf-boost profiles). **Escalate to a full re-ablation only if drift appears** — do not commit to one up front.

## Outputs

A FINDINGS.md with:
1. **The PPR verdict:** keep (expander on, if it earns its keep) vs zero out (Q3b's 0.0, accepting co-access loss) vs "edges not built — process fix first."
2. **The crt-053 scope recommendation:** is ES-4/5/6 (injection-path leak) live or dormant — keep in scope or defer.
3. The edge inventory (the durable answer to "are we creating the edges PPR needs").
4. The formula-still-holds (or drifted) verdict at current scale.

## Non-Goals / Out of Scope
- Building anything — measurement only. Config changes (turning PPR on/off, the expander) are *decisions this informs*, made elsewhere (an ASS-037-class real-distribution decision per ADR-006 #4894).
- crt-053's status-trust work — separate thread; this only *informs its scope*.
- A full formula re-ablation — only triggered if Phase 3 shows drift.
- The fixture corpus — PPR can't be tested there (no positive edges); this runs on the realism snapshot.

## Dependencies
- **nan-018 (#716, MERGED)** — the eval harness + the config-exposed PPR/penalty knobs.
- **Realism snapshot** — fresh prod snapshot (shares infra with ass-073's Track B; re-snapshot for current state).
- **Decision history** (background): crt-030 (PPR intro, ADR-007 #3737), crt-032 (#3785 co-access→PPR), crt-037 (Informs), vnc-015 ADR-006 (#4429 RelatedTo), ASS-037 (#3984), ASS-037 Q3b (#3988), ASS-038 (#3989), crt-042 (expander, #4050/#4051/#4052), vnc-018 lesson (#4495 — Advances/Motivates kept out).

## Open Questions (human, before spike start)
- OQ-1: snapshot — fresh current-state (lean, for the 6.7k-scale measurement).
- OQ-2: does the expander need a minimum edge density to be worth testing — i.e., is the Phase-0 gate threshold the Q3b ≥1.0/entry, or a lower "any signal" bar? (Architect/spike judgment; the gate exists either way.)

## Tracking
GH Issue: see below (`goal:self-learning`, `research`). Separate thread from crt-053 (#717) but **informs/limits its scope** (the ES-4/5/6 live-vs-dormant question). Anchored on the PPR decision-history trace; precedent ASS-037/ASS-038.
