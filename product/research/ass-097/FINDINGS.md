# FINDINGS: Retrieval-measurement trust on the current connected corpus

**Spike**: ass-097
**Date**: 2026-07-10
**Approach**: measurement (G1–G3), measurement + proof-of-concept (G4), investigation (G5–G6)
**Confidence**: empirical (G1–G4), directional (G5–G6)
**Tracking**: GH #950

Framing kept sharp throughout: **Comparator** = dev-time A/B on a frozen shape (snapshot P@5/MRR/CC@k/ICD). **Floor-proof / odometer** = an absolute verdict a behavioral invariant holds (floor) and whether quality rises over a deployment's life (longitudinal). SL-METRIC needs the latter; MRR structurally supplies only a (here, near-degenerate) instance of the former.

**Artifacts** (scratch, uncommitted — snapshots are sensitive): snapshot `snap.db` hash `c6c92df278e3`, 3118 scenarios, reports `report_base.md` / `report_g3.md` / `report_pen.md` in `/var/tmp/claude-1000/.../scratchpad/eval/`. The one permitted repo write — the G2 re-baseline line — was appended to `product/test/eval-baselines/log.jsonl`. No Unimatrix writes, no PR.

---

## Findings

### Q: G1 — Corpus characterization (empirical). What does the corpus look like NOW, and is the graph dense enough for graph-expansion signal?

**Answer.** The corpus is materially more connected than at the original PPR eval, but by the ass-037-derived retest criteria it is **still below the density bar where graph-expansion signal is expected to surface**.

Current state (`context_status` + snapshot SQL on `snap.db`):

| Dimension | Now (2026-07-10) |
|---|---|
| Entries (total) | 5,683 |
| Active / Deprecated / Quarantined / Proposed | 2,286 / 893 / 2,504 / 0 |
| Typed graph edges | **1,619** |
| — by type | CoAccess 504, Informs 475, Supports 447, Prerequisite 84, Advances 83, About 22, Contradicts 2, Supersedes 2 |
| Co-access pairs | **39,134** active, across 2,914 distinct entries |
| — pair-weight spread | count=1: 30,737 · 2–3: 6,493 · 4–10: 1,730 · 10+: 174 |
| Correction chains | 753 superseded entries, 523 active terminal heads; depth 1:543, 2:118, 3:32, … **max depth 13** |
| Connectivity (active) | 48.3% (1,104/2,286); 1,182 isolated; mean degree 1.42; cross-category edges 728; inferred edges 1,127 |
| Informs density | **0.21 Informs edges / active entry** |
| All typed density | 0.71 typed edges / active entry |
| Contradictions detected | 1 live pair (#5202/#5208, nan-019) |

**Then vs now.** ass-037 (Unimatrix #3988) tested PPR-via-Informs at **160 Informs edges / 1,134 entries (0.14/entry) → zero delta**, and set a retest threshold of **>=5,000 active entries OR Informs density >=1.0 edges/entry**. ass-038 (#3989) then showed density 1,086->6,738 edges (6.2x) still produced **zero** delta and diagnosed the bottleneck as *architecture* (re-ranker within k=20), not density — fixed later by the PPR expander (crt-030/crt-042, live in ~48% of queries per ass-074 #4917).

**Evidence -> interpretation.** Today's 1,619 typed + 39,134 co-access is a real step up from the 160-edge synthetic era, and the correction-graph now has genuine depth (chains to depth 13, 523 terminal heads) for the redirect/penalty machinery to act on. But Informs density (0.21/entry) is still **below** the 1.0 retest bar, and active count (2,286) is **below** 5,000. So on ass-037's own criteria the graph is denser but **not yet past the point where graph-expansion is expected to move ranking**.

**Recommendation.** Treat the corpus as "connected enough to exercise the correction/penalty machinery, NOT yet connected enough to expect a graph-expansion ranking lift." Do not read a flat G2 as "the graph does nothing" — read it as "density is still under the ass-037 retest threshold." Re-run the density retest only when Informs density crosses ~1.0/active or active entries cross ~5,000.

---

### Q: G2 — Re-baseline (empirical). Did the richer graph move retrieval, and in which metric?

**Answer.** On the current snapshot the baseline (compiled defaults, k=5, 3,118 scenarios) measures:

| Metric | 2026-03-26 baseline | **Now (ass-097)** | Delta |
|---|---|---|---|
| Scenarios | 3,307 | 3,118 | — |
| P@5 | 0.3058 | **0.5461** | +0.2403 |
| MRR | 0.4181 | **0.7187** | +0.3006 |
| CC@5 | 0.2636 | **0.2524** | -0.0112 |
| ICD | 0.5244 | **0.4428** | -0.0816 |
| Avg latency | 8.7 ms | 20.1 ms | +11.4 ms |

**Evidence.** Baseline-only run on `snap.db` (`report_base.md`); re-baseline line appended to `log.jsonl`.

**The movement is NOT a quality gain — it is self-consistency, and it is not longitudinally comparable.** P@5/MRR here are computed against **soft ground truth = the results the current pipeline itself logged** for each query. A different scenario set + a self-referential ground truth means the 0.42->0.72 MRR jump largely measures **how faithfully the replayed pipeline reproduces its own recent query_log**, not whether retrieval got better in absolute terms. The tell: the two **absolute, non-self-referential** diversity metrics — CC@5 and ICD, precisely the axis the graph-expansion stack was built to lift — **did not rise; they slipped** (-0.011, -0.082). If the denser graph were producing a real retrieval improvement, CC@k/ICD is where it would show, and it is flat-to-down.

**Recommendation.** Record the re-baseline (done) but annotate it as a **self-consistency snapshot, not a longitudinal quality reading**. Do not report "MRR improved 72%." The honest one-liner for #950: *retrieval is more self-consistent with its own logs; the diversity axis the graph would move is flat-to-down; no evidence the denser graph lifted ranking.*

---

### Q: G3 — Metric trust / consistency (the crux). Is snapshot-MRR consistent and interpretable on our corpus? Where is it trustworthy vs categorically wrong-shaped?

**Answer.** Snapshot-MRR on the current corpus is a **near-degenerate comparator**: it is architecturally **blind** to the levers we most want to A/B, and it barely moves for the ones it can see. It is trustworthy only as a coarse regression tripwire for replay-time top-of-list changes on a fixed scenario+snapshot pair. It is categorically wrong-shaped for absolute/longitudinal quality and for confidence/freshness/penalty tuning.

**Evidence — two known-signal controls, both run on the paired snapshot:**

1. **Confidence-weight controls (inert lever).** A deliberately-degraded profile (fresh 0.52 / usage 0.30, starving base/trust/corr) and a plausible-better profile (corr+trust boosted) both returned **byte-identical top-5 to baseline** — 0 ranking changes, dP@5 = dMRR = dCC@k = dICD = 0.0000 (`report_g3.md`; per-scenario top-5 verified identical across all three profiles). **Mechanism, confirmed in source:** `eval/profile/layer.rs:382` constructs the eval search stack with `ConfidenceParams::default()` unconditionally, and `[confidence.weights]` is validated for the 0.92-sum invariant then **dropped** (comment: "GH #311: default params for eval profiles"). Confidence is also a *stored* per-entry scalar in the snapshot; offline replay never recomputes it. So any change mediated by confidence/freshness/usage is **structurally invisible** offline — a zero delta is a false negative, not evidence of no effect.

2. **Graph-penalty control (live lever).** `[graph_penalty]` *is* threaded live (`layer.rs:391`). A steep profile (all severities -> 0.10) produced **some** per-scenario reordering (e.g. qlog-1591 flagged in section 2) but again **zero aggregate delta** (P@5 0.5461, MRR 0.7187, unchanged; `report_pen.md`). The penalty reorders deprecated entries that are not the soft-GT target, so the aggregate never moves.

3. **Historical positive (NLI).** The only change that ever moved the aggregate was NLI re-ranking (P@5 0.30->0.12), a replay-time top-list re-rank — and that "movement" was itself flagged a soft-GT artifact (NLI surfaced *different* entries), with the real decision driven by 150x latency + gross reshuffle for no demonstrable gain. That is the harness's genuine positive-control precedent, and it discriminated on **magnitude of top-list disruption + cost**, not on a trustworthy quality delta.

**Why a positive control is structurally impossible here.** Baseline == the soft-ground-truth ceiling by construction; any candidate can only move *away* from GT. The snapshot harness is therefore a **one-sided regression detector** for gross replay-time top-list changes — blind to (a) stored-state levers (confidence/freshness) and (b) fine reorderings that don't touch the GT entry's rank.

**Recommendation — the metric-trust statement:**
- **Trustworthy:** as a coarse **regression tripwire** for replay-time, top-of-list changes (embedding-model swap, NLI/GGUF re-rank) on a **fixed** scenario+snapshot pair, read as "did the top of the list get grossly reshuffled, and at what latency."
- **Categorically wrong-shaped:** (i) absolute or longitudinal quality (odometer) — self-referential GT; (ii) confidence/freshness/usage A/B — inert offline; (iii) penalty-steepness A/B on the snapshot — live but aggregate-flat; (iv) any "the graph made retrieval smarter" claim — CC@k/ICD, the axis it would show on, did not move. The human's stated doubt is **confirmed**: do not stake "proven" on snapshot-MRR for this corpus.

---

### Q: G4 — Fixture property-assertion class as floor-proof (empirical/directional). Does it give a stable absolute pass/fail that survives corpus mutation? Which floor caps convert, which need new shapes, which are out of reach?

**Answer.** The property-assertion class is a **deterministic, absolute, alias-durable** pass/fail and is the correct floor-proof instrument for *status/currency* invariants — the exact two-sided discrimination the snapshot cannot give. It is green on HEAD but under-proven on real semantics, and it covers only part of the capability map.

**Evidence.**
- **Machinery green:** `cargo test -p unimatrix-server --lib eval` -> **282 passed, 0 failed**. This includes the `trust.rs` truth-table (the load-bearing `rank_below` asymmetry: A-absent=>PASS, B-absent-while-A-present=>FAIL; redirect head-absent=>FAIL), the corpus loader fail-loud validation (literal-id / null-expected / missing-alias rejected), and `test_ac14_correlated_sweep_non_vacuous` (trust + P@5/MRR + cost in one run, >=1 rank_below evaluated with both anchors present).
- **Two-sided discrimination proven:** AC-14 cond.3 shows the `[graph_penalty]` lever produces an **observable non-zero final_score delta** on the deprecated-heavy fixture — good-vs-bad separation on a durable corpus, alias-based so it survives re-snapshot (crt-013 #703). This is what the snapshot fails to do (G3).
- **CAVEAT — under-proven on semantics.** The shipped AC-14 sweep uses a **deterministic hash embedding provider** (non-semantic). It proves the machinery evaluates non-vacuously and the lever moves scores; it does **not** prove the assertions *PASS under real semantic retrieval*. A real-model `run_fixture_sweep` is needed to certify "good config passes / bad config fails" on semantics, and `run_fixture_sweep` is **test-only (no CLI)** — out of reach this spike without adding throwaway product code. SL-METRIC's own `proven_by` (#5572) also flags this test as **flaky (#833/#790)**.

**Floor-cap -> assertion-type map:**

| Capability | Assertion | Verdict |
|---|---|---|
| **SL2** — misleading recedes (#5556) | `rank_below[deprecated, active]` | **PARTIAL.** Converts the *static status-penalty* sub-claim (deprecated ranks below active) to proven-on-evidence. Does **NOT** cover SL2's actual `done_when` — the *dynamic* helpful/unhelpful-vote -> confidence-shift -> rank-change link. That is a live-path behavioral test (confidence is inert offline, per G3), not a fixture assertion. |
| **Search read-currency** (sibling of **SL7**) | `redirect_to_head` | **CONVERTS** — a genuine floor-proof that the terminal-active head surfaces at/above stale members in **search**. But note SL7 (#5532) is defined as `context_get`-**by-id** scoped and is **already proven** by vnc-042 + #885 behavioral tests — a different surface. Name them distinctly: `redirect_to_head` proves the *search* invariant; SL7-the-get-capability is green elsewhere. |
| **SL4** — co-access co-surface (#5560) | (needs `co_surface`) | **OUT OF REACH via existing shapes.** Requires a **new** assertion type asserting two frequently-co-accessed entries co-appear at retrieval. The fixture authors superseded_by chains + status, not co_access edges (co-access is the snapshot realism layer). New shape required. |
| **KI-CONTRADICT** — no conflicting pair served (#5548) | (needs contradiction shape + `forbidden_absent`) | **OUT OF REACH now.** `forbidden_absent` could express "never serve both ends of a Contradicts edge," but needs a **new contradiction fixture shape** (a Contradicts pair) that does not exist, AND detection was **removed/regressed** (ass-092 #899); live corpus has only 2 Contradicts edges / 1 detected pair. Needs new shape + detection restore. |
| **SL1** — attribution (#5552) | — | **OUT OF HARNESS REACH by construction.** A session/provenance property, not a rank property. Name it; do not force it into the rank harness. |

**Bracketing note.** `deprecated_connected.toml` authors the required spread (5 deprecated x 3-active band, ADR-004 section 5) — adequate structure for a steepness sweep, but the sweep has not been run with real semantics on current state; nan-018 explicitly anticipated a revision pass, still owed.

**Recommendation.** Adopt the property-assertion class as the floor-proof instrument for the **status/currency** invariants (SL2-static, search redirect_to_head) — it is deterministic, absolute, and durable. Before staking "proven," (a) run a **real-model fixture sweep** to certify the assertions PASS on semantics (not just non-vacuously) and (b) **de-flake #833/#790**. Author the two missing shapes (`co_surface` for SL4, contradiction for KI-CONTRADICT) as follow-ups. Keep SL1 out.

---

### Q: G5 — SL-METRIC redefinition + the gate boundary (directional). What should `done_when` be, and should eval cross the nan-018 "not a standing gate" line?

**Answer — `done_when` redefinition.** SL-METRIC #5572 already records `done_when(1)` DONE (nan-018 trust class discriminates good/bad on fixtures) and `done_when(2)` OPEN (a signal "calibrated/validated as TRUSTED for LIVE-corpus interpretation and adopted as the standing accepted quality verdict"). This spike's evidence says: **do not fill `done_when(2)` with snapshot-MRR.** Redefine it as a **two-instrument** clause:
- **Floor (absolute):** the property-assertion class PASSES on the durable fixture corpus under **real-semantic** retrieval for the enumerated invariants (status-penalty `rank_below`, search `redirect_to_head`) — the trustworthy absolute verdict.
- **Odometer (longitudinal):** **reuse-rate** (SL-REUSE #5577), explicitly **not MRR**, reported as a trend.

**Reuse-rate as odometer — viable, with a stated attribution caveat.** Live signal exists: effectiveness analysis shows 60.4% "effective" (1,380/2,286 active), injection_log actual-success 1.00 at 0.4–0.6 confidence, 39,134 co-access pairs. But "effective" = co-occurrence of injection + session pass, **not proven causal**, and SL-REUSE's `done_when` is only "emits a non-zero measure" (proven on synthetic rows, real-data gap). So reuse-rate is an acceptable **direction indicator** if reported as trend-with-attribution-caveat — never a causal quality claim. This is the odometer MRR structurally cannot be.

**Answer — gate boundary.** **Do NOT cross the nan-018 line for the snapshot path; the line was right.** G3 shows snapshot-eval is a near-degenerate, one-sided comparator on this corpus — unfit as an authoritative quality gate; keeping it a dev aid is correct. **But the fixture property-assertion path is a different object:** deterministic, absolute, alias-durable, already drift-guarded, and **already lives in the normal test suite** (the 282 eval lib tests). The needle-threading recommendation: **capability floor-proofs = targeted fixture/behavioral tests in the standard `cargo test` suite** (the existing pattern, and how SL7 was proven), **not** the snapshot `eval` CLI wired into CI-on-every-PR. This honors nan-018 (snapshot eval stays a dev aid) while giving the capability map real, standing floor-proofs.

**Recommendation.** Rewrite SL-METRIC `done_when(2)` = "property-assertion floor-proof passes on the durable corpus under real semantics (floor) AND reuse-rate is emitted as a longitudinal trend (odometer) — MRR retired to regression tripwire." Keep the snapshot eval a dev aid; make floor-proofs targeted tests. (uni-zero applies the capability-status write after reviewing this — not this spike.)

---

### Q: G6 — Go/no-go. Is there a trustworthy measurement path to floor-proof self-learning + integrity retrieval invariants on the corpus we have now?

**Answer: GO — with a re-scoped instrument.** There is a trustworthy path, but it is **not** snapshot-MRR. It is the **fixture property-assertion class** (for status/currency floor invariants) + **reuse-rate** (for the longitudinal odometer). Snapshot-MRR is demoted to a coarse regression tripwire.

**Evidence:** G3 (snapshot-MRR near-degenerate, inert to confidence, flat to penalty, one-sided by construction) + G4 (property class green, deterministic, two-sided on penalty, alias-durable).

**Blocking gaps to close before "proven" is defensible** (each a scoped follow-up, none blocking the GO decision):
1. Run a **real-model `run_fixture_sweep`** to certify assertions PASS on semantics (not just non-vacuously) + **de-flake #833/#790** — the current proof uses a non-semantic hash provider and is flaky.
2. New **`co_surface`** assertion shape for SL4.
3. New **contradiction fixture shape + detection restore** (ass-092 #899) for KI-CONTRADICT.
4. SL2's dynamic vote->rank claim is a **live-path** behavioral test (confidence inert offline) — separate from the fixture floor.

**Recommendation.** Proceed. Adopt the fixture-class-as-floor + reuse-rate-as-odometer redefinition of SL-METRIC; retire snapshot-MRR to a tripwire; schedule the four gaps as follow-up work.

---

## Unanswered Questions

- **Do the property assertions PASS under real semantic retrieval on the current corpus?** Blocked: `run_fixture_sweep` is test-only (no CLI) and the shipped proof uses a non-semantic hash provider; certifying semantic pass needs a throwaway real-model harness (out of this spike's read-only/no-product-code scope). Needs a small follow-up spike or a delivery task.
- **Is reuse-rate causally attributable to knowledge quality (vs coincidence)?** Out of scope here; SL-REUSE real-data attribution is an accepted col-020b gap. Needs its own measurement design.
- **Does the graph-expansion stack lift ranking once density crosses the ass-037 threshold?** Cannot answer at current density (0.21 Informs/active < 1.0). Blocked on corpus growth; retest when the threshold is crossed.

## Out-of-Scope Discoveries

- **`[confidence.weights]` profile knob is misleading.** It validates a 0.92-sum invariant then is silently dropped in offline eval (`layer.rs:382` uses `ConfidenceParams::default()`). The knob looks tunable but does nothing offline — a documentation/UX trap. Worth a doc note or a load-time warning. (Not pursued.)
- **`eval report` WARN on `profile-meta.json`.** Report parsing emits `WARN: skipping .../profile-meta.json (parse error: missing field scenario_id)` — it tries to parse the sidecar as a scenario result and WARN-skips it. Harmless to output, but noise; candidate GH issue. (Not pursued.)
- **2,504 Quarantined entries (44% of the corpus).** Large quarantine population relative to 2,286 active. Not characterized here; may warrant a hygiene look. (Not pursued.)
- **One live contradiction still served-eligible** (#5202/#5208, nan-019 IMAGE= smoke pair). A concrete KI-CONTRADICT instance in the live corpus. (Not pursued — flagged for ass-092.)

---

## Recommendations Summary

- **G1 (Corpus):** Denser than the original PPR eval (1,619 typed + 39,134 co-access, chains to depth 13) but Informs density 0.21/active is still **below** the ass-037 retest bar (1.0/active or 5,000 active) — connected enough to exercise the correction/penalty machinery, not yet to expect a graph-expansion ranking lift. Retest density only when the threshold is crossed.
- **G2 (Re-baseline):** P@5 0.5461 / MRR 0.7187 / CC@5 0.2524 / ICD 0.4428 on 3,118 scenarios (logged). The MRR jump vs 2026-03-26 is **self-consistency, not a quality gain, and not longitudinally comparable**; CC@k/ICD (the graph's axis) is flat-to-down. Do not report it as improvement.
- **G3 (Metric trust):** Snapshot-MRR is a **near-degenerate, one-sided comparator** on this corpus — inert to confidence (validated-then-dropped), aggregate-flat to live penalty, and blind to everything but gross replay-time top-list reshuffles. Trust it only as a coarse regression tripwire on a fixed scenario+snapshot pair; it is categorically wrong-shaped for absolute/longitudinal quality and for confidence/freshness/penalty A/B. Human's doubt confirmed.
- **G4 (Floor-proof):** The property-assertion class is deterministic, absolute, alias-durable, two-sided on penalty (282 tests green) — the right instrument for status/currency floors. Converts SL2-static + search `redirect_to_head`; SL7-get is already proven elsewhere; SL4 and KI-CONTRADICT need new shapes; SL1 is out of reach by construction. Caveat: shipped proof is non-semantic + flaky (#833/#790) — certify on real semantics before staking "proven."
- **G5 (SL-METRIC + gate):** Redefine `done_when(2)` = property-assertion floor-proof under real semantics (floor) + reuse-rate trend (odometer), MRR retired to tripwire. **Do not** cross the nan-018 line for the snapshot path; author capability floor-proofs as targeted fixture/behavioral tests in the normal suite (the existing pattern), not the eval CLI in CI.
- **G6 (Go/no-go):** **GO** with the re-scoped instrument (fixture class + reuse-rate, not MRR). Close four scoped gaps before "proven": real-model fixture sweep + de-flake #833/#790; `co_surface` shape (SL4); contradiction shape + detection restore (KI-CONTRADICT, ass-092 #899); live-path SL2 vote->rank test.
