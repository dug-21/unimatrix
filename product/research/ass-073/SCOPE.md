# ASS-073: Evaluate crt-053's Retrieval Questions on the nan-018 Eval Harness

**Date**: 2026-06-10 (finalized — nan-018 shipped, PR #719 merged)
**Spike type**: Measurement — run the upgraded eval harness; answer crt-053's open questions (Q5/Q8) with data
**Status**: READY — nan-018 (#716, merged) delivered the instrument; this scope is now pinned to the delivered surface
**Feeds**: crt-053 (#717, HOLD) — findings set Q5/Q8 and inform AC-05/AC-11/AC-12/AC-13
**Informed by**: crt-053 SCOPE.md (locked Q1/Q6; open Q5/Q8), nan-018 ADR-002/003 (#4895/#4896), #4886 (premise-drift)

## Position in the Chain

```
nan-018 (#716, MERGED)  →  ass-073 (THIS)  →  crt-053 (#717, HOLD)
the instrument shipped      measure for         consume results, shape
                            crt-053's Qs         search behavior
```

Read-only on retrieval behavior: this spike **runs** the harness and **reads** results; it writes no search/briefing code. crt-053 stays HOLD until this returns numbers.

## Delivered nan-018 surface this spike uses (pinned, verified in merged code)

- **Sweep entry point**: `run_fixture_sweep(corpus_dir, target_db, profiles, k, out, provider, project_dir) -> SweepOutcome` (`eval/runner/sweep.rs:115`). **Profile ordering: first = BASELINE (default penalties); rest = swept candidates.** It guards corpus shape (HARD on mismatch) before any replay, embeds-at-load, threads the lever live into each `EvalServiceLayer::from_profile`, and replays with the alias map so trust evaluates non-vacuously. (Or the `eval run` CLI wrapping it.)
- **Sweepable knobs** (`GraphPenaltyConfig`, `infra/config.rs`): `orphan`, `clean_replacement`, `hop_decay`, `partial_supersession`, `dead_end`, `fallback`, `max_traversal_depth`, + `multiplier` overlay. **Clamp coupling (delivered + documented):** `clean_replacement` is *also the hop-decay clamp ceiling* — a `clean_replacement` sweep is a **compound knob** (base penalty + clamp ceiling move together); read its effect as compound, not isolated. `hop_decay`/`max_traversal_depth` are shape, never multiplier-scaled.
- **Trust constraint**: `evaluate_trust(...) -> TrustOutcome { absence_pass, rank_pass, violations }` (`eval/runner/trust.rs:104`). `absence_pass` = all `forbidden_absent`; `rank_pass` = all `rank_below` + `redirect_to_head`. The trust property holds iff `absence_pass && rank_pass`. (Note the asymmetric `rank_below` B-absent⇒FAIL semantics.)
- **Cost**: `cost_tokens = Σ token_proxy(result)` (`eval/runner/cost.rs`), token-weighted (ADR-003), surfaced per profile.
- **Primary fixture corpus** (`eval/corpus/fixtures/`): five shapes shipped — `multi_correction_chain`, `dangling_deprecated`, `superseded_active`, `deprecated_connected`, `dead_end_chain` + `manifest.toml` (shape stamp).
- **Docs to read first** (`docs/testing/`): `eval-config-knobs.md` (knob semantics, multiplier precedence, clamp coupling), `eval-fixture-authoring.md`, `eval-two-corpus-model.md`, `eval-corpus-migration.md`.

## Critical measurement caveat (read before running)

**nan-018 shipped the instrument only — it did NOT fix crt-053's injection leaks (ES-4/5/6) or change ranking behavior** (nan-018 Non-Goal 3). So this spike sweeps the fixture corpus through the **current, pre-crt-053 search pipeline, which still leaks.** Consequence:

- A leaked stale entry (PPR/graph_expand injection at `penalty=1.0`) **bypasses the penalty**, so **no steepness touches it.** If the `deprecated_connected` / chain fixtures route their stale entry through the leak path, the trust assertions will **fail at every steepness** — which is itself a finding: it empirically confirms crt-053's thesis that the **leak fix is the load-bearing change, not steepness.**
- This spike therefore measures the steepness→trust relationship **on the penalized path** (entries that arrive via HNSW + penalty), and **diagnoses** whether each fixture's stale entry is steepness-addressable (penalized path) or leak-addressable (injection path). The clean post-fix steepness is finalized by **crt-053 on its own leak-fixed pipeline**; ass-073 supplies the candidate + the diagnosis + the relevance/cost baseline.

## Anti-overfitting guard (binding — see crt-053 C-13)

The fixture corpus is a small synthetic **proxy**. This spike finds the steepness that **satisfies the structural trust property AND does not regress realism-snapshot relevance** — NOT the steepness that maximizes a fixture metric.
- **Fixture corpus → property verification + candidate discovery.** Report the *minimum* steepness that holds the guarantee. Never the deployment authority.
- **Realism snapshot → anti-overfitting cross-check.** Re-check any candidate against P@5/MRR on real traffic; a fixture-good/snapshot-regressing value is a fail.
- **Deployment authority → ASS-037-class real-distribution evidence** (#3984; ADR-006 #4894), never this fixture sweep.

Report steepness as a **guarantee threshold** (property holds at-or-above it) with the realism-snapshot relevance cost at that threshold — not a single "optimal" number.

## Research Questions

- **RQ-1 — Q8 steepness characterization.** Via `run_fixture_sweep`, sweep the penalty levers (notably `clean_replacement` [compound knob] and the `multiplier`) across profiles on the fixture corpus. For each shape, report at what penalty magnitude `absence_pass && rank_pass` holds for the `deprecated_connected` (connectivity-refund) case, **and** whether that case is steepness-addressable or leak-dominated (the caveat above). Output: the candidate **guarantee threshold** + trade curve + the leak-vs-steepness diagnosis.
- **RQ-2 — Q5 relevance/cost baseline + bound.** On the realism snapshot (fresh): measure current P@5/MRR + `cost_tokens` baseline; recommend the acceptable regression tolerance for crt-053 AC-11 (what makes "fewer, more trustworthy" pass and "silently tanked recall" fail).
- **RQ-3 — #406 graph-snapshot diagnostic.** Confirm whether the rebuilt eval `TypedGraphState` carries the full multi-hop Supersedes chain at search time (suspected #406 root cause = graph-snapshot construction). Diagnostic, not a metric.
- **RQ-4 — Trust-assertion validation in both modes.** Confirm the trust metric expresses crt-053's properties end-to-end in Flexible (search) AND Strict (briefing) modes on the fixture corpus.
- **RQ-5 — Corpus sufficiency (the no-feasibility-probe gap).** nan-018 authored the corpus without a prior feasibility probe (a consequence of the chain simplification). Assess whether the `deprecated_connected` shape carries enough sim/conf variation to make the steepness crossover *findable*. If not, report a **corpus-revision request back to nan-018** — a valid, expected outcome; the Wave-1 corpus is not assumed final.

## Output
A FINDINGS.md feeding crt-053: the Q8 candidate **guarantee threshold** (+ realism relevance cost + trade curve + leak-vs-steepness diagnosis), the Q5 baseline + regression bound, the #406 root-cause confirmation, the trust-assertion validation (both modes), and any corpus-sufficiency revision request. Unblocks crt-053 from HOLD.

## Out of Scope
- Fixing crt-053 behavior (leaks, redirect, #406, #585) — that is crt-053.
- Extending the harness — nan-018 is merged; a gap is *flagged back*, not fixed here.
- Re-tuning fusion weights / NLI / PPR algorithm (ASS-037 authority).
- Answering crt-053's briefing-policy divergences (Q-B1/B2/B3) — informed, not set, by these measurements.

## Open Questions (human, before/at spike start)
- OQ-1: snapshot for the realism baseline — fresh (current state, for the Q5 bound) is the lean; #4886/#500 drift caution noted.
- OQ-2: steepness-sweep granularity — coarse-to-locate the crossover, fine around it.

## Tracking
GH Issue: see below (`goal:self-learning`, `research`). Chain: nan-018 (#716, MERGED) → **ass-073** → crt-053 (#717, HOLD). Drift discipline #4886; precedent ASS-037 (#3984).
