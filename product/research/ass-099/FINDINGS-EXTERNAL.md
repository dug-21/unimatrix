# FINDINGS (EXTERNAL TRACK): MetaHarness capability summary — ass-099 / G1

**Spike**: ass-099 (GH #952) · External/literature track (owns G1 only)
**Date**: 2026-07-11 · **Approach**: literature (primary source fetched live)
**Confidence**: validated for the paper's description (arxiv `2603.28052` retrieved live from three independent surfaces — PDF, abstract page, HTML full-text — plus a third-party analysis). Directional for ecosystem situating.

## TOP-LINE FINDING — read before using anything below

**The paper is real, retrievable, and correctly identified — but it is NOT the thing the SCOPE framing expects.**

`2603.28052` resolves to **"Meta-Harness: End-to-End Optimization of Model Harnesses"** — Yoonho Lee, Roshen Nair, Qizheng Zhang, Kangwook Lee, Omar Khattab, Chelsea Finn; submitted 2026-03-30 (v1); Stanford IRIS Lab; code at `github.com/stanford-iris-lab/meta-harness`.

The paper is about **automatically optimizing the "harness"** — defined by the authors as *"the code that determines what information to store, retrieve, and present to the model … a stateful program that wraps a language model and determines what context the model sees at each step."* It is a machine-learning / context-engineering optimization paper in the prompt/program-optimizer lineage (DSPy/GEPA/TextGrad/OPRO — Omar Khattab is a DSPy author). It treats **harness code as a search space** and evolves it with an agentic coder.

**It is NOT a multi-agent *delivery* orchestration framework.** The SCOPE and the human autonomy framing anticipate MetaHarness as a source of "controls that de-risk higher autonomy" — checkpoints, verification gates, rollback, bounded authority, evidence/traceability, escalation-to-human triggers for autonomous multi-feature delivery. The actual paper offers **none** of these as first-class concepts. "Harness" here = *LLM context-construction code*, not *a delivery/orchestration process*. There is a genuine terminology collision between the paper's "harness" and Unimatrix's "protocol/agent orchestration harness."

This is the load-bearing thing for synthesis: **do not build G4–G6 on an assumption that MetaHarness supplies autonomy-de-risking delivery controls. It does not.** What it *does* supply is a narrower, transferable idea (below).

## Findings — Q (G1)

**What it is.** An **outer-loop optimizer that searches over harness code for LLM applications**. Given a task and base model, it repeatedly proposes, evaluates, and refines *the code that wraps the model* (retrieval, memory/state, prompt assembly), keeping a Pareto frontier of high scorers.

**Problem it solves.** LLM-system performance depends not only on weights but on the harness; harnesses are hand-built and brittle. Existing optimizers (OPRO, TextGrad, GEPA) "compress feedback too aggressively" — distilling each trial to a scalar/short summary, hitting an "abstraction ceiling" that hides subtle multi-step bugs. Thesis: **give an agentic proposer rich, uncompressed access to prior experience and let it engineer the harness directly.**

**Architecture / control model (Algorithm 1, outer loop):**
- Maintain a **filesystem `D`** storing every prior candidate's *source code, scores, and full execution traces*.
- Each iteration: an **agentic proposer** inspects prior artifacts via the filesystem, generates *k* new harness variants, evaluated on a **search set**; results written back to `D`. Return the Pareto frontier.
- **Proposer = Claude Code (Opus-4.6)** — a real coding agent using terminal tools (`grep`, `cat`) to navigate history and edit code, not a fixed-prompt token model. ~82 files read/iteration; ~10M tokens of diagnostic context available per evaluation, vs 0.002–0.026 MTok/iter for compressed-feedback baselines.
- **Selection:** task reward on the search set; competing objectives (accuracy vs context-token cost) resolved by **Pareto dominance**. No fixed parent-selection rule. Short runs: ~20–40 iterations, ~40–110 harnesses total.
- **The only in-loop "gate"** is an **interface/type validation** (line 11) that a candidate conforms to the expected interface before evaluation — a correctness check, not a safety/human-review/authority control.

**Claimed benefits + evidence (three domains):**
1. **Online text classification** (LawBench/Symptom2Disease/USPTO-50k; GPT-OSS-120B): **48.6% vs 40.9%** ACE baseline (**+7.7 pts**), **~4× fewer context tokens** (11.4K vs 50.8K). Matches OpenEvolve/TTT-Discover in ~0.1× evaluations. OOD on 9 unseen datasets: 73.1% vs 70.2%.
2. **Retrieval-augmented math** (200 IMO-level problems; corpus ≥500K solved problems): a *single* discovered harness gives **+4.7 pts** avg over no-retrieval across **five held-out models** (GPT-5.4-nano/mini, Gemini-3.1-Flash-Lite, Gemini-3-Flash, GPT-OSS-20B) — cross-model transfer of a discovered artifact.
3. **Agentic coding (TerminalBench-2, 89 tasks):** Opus-4.6 harness **76.4%** vs Terminus-KIRA 74.7% (**+1.7**); Haiku-4.5 harness **37.6%** vs Goose 35.5% (**+2.1**).

Load-bearing claim: the novelty is the **evidence-richness of the feedback channel** (full uncompressed traces via filesystem), not the outer-loop evolutionary search (conventional).

**Stated limitations (faithfully — the paper is thin here; no dedicated Limitations section, caveats scattered in Sec. 5 Discussion):**
- Results rest on **one particularly strong proposer (Claude Code)**; variation across other/weaker proposers is future work.
- The TerminalBench-2 harness is **specialized to that benchmark** — acknowledged as effectively test-set-directed; they run regex audits for task-string leakage; benchmark itself is noted as contested.
- **No failure-mode analysis / negative results.**
- High search compute (millions of diagnostic tokens/evaluation) — implicit, not framed as a limitation.
- **Future work:** co-evolve harness *and* model weights; broaden proposer-agent study.

**Autonomy / safety posture — directly relevant to the spike, and mostly a negative result:**
- The search loop is **fully autonomous**: the proposer makes every edit/inspect decision with **no human-in-the-loop review, no approval gate, no rollback, no bounded-authority model, no escalation trigger.** Only automatic guardrails: the interface/type validation, and (coding domain) a **15-second bootstrap timeout that "fails silently."**
- **No security content**: no prompt-injection analysis, no agent isolation/sandbox, no secret handling. Unrestricted filesystem access to candidate history is treated as *the feature*, not a risk.
- The one discipline with genuine transfer value: **strict search-set / held-out-test-set separation** (proposer never sees test results during search) + **Pareto multi-objective selection** — an *evaluation-integrity* control, not an *autonomy* control.

**Ecosystem situating (novel vs table-stakes).** "Harness engineering" is a fast-emerging mid-2026 subfield (several adjacent arxiv ids 2605–2606 within ~90 days). Direct lineage/baselines (table-stakes it builds on): DSPy, GEPA, TextGrad, OPRO, OpenEvolve, and the ACE context-management system it beats. The outer-loop-evolutionary shape is **conventional**; what is **novel** is (a) making the search space *harness code* specifically and (b) the *uncompressed full-trace filesystem feedback channel* to an agentic coder. Adjacent 2026 papers (titles only — NOT read, do not attribute contents): "Harness Engineering as Categorical Architecture" (2605.12239); "HarnessX: A Composable, Adaptive, and Evolvable Agent Harness Foundry" (2606.14249); "Code as Agent Harness: Toward Executable, Verifiable, and Stateful Agent Systems" (2605.18747); "Meta-Engineering Harnesses … Contract-Driven Adversarial Verification Architecture" (2605.25665); "Self-Programmed Execution for Language-Model Agents" (2605.06898); community list `ai-boost/awesome-harness-engineering`. **These, not Meta-Harness, are where verification/contracts/statefulness/orchestration concepts live.**

## Unanswered Questions
- **Do MetaHarness's controls de-risk capability-level autonomous *delivery*?** Cannot be answered from this paper — it addresses none of that surface. Honest answer to the spike's load-bearing sub-question: **MetaHarness is the wrong source for autonomy-de-risking delivery controls.** The adjacent verifiable/contract-driven-agent papers are the right place, and were out of this track's scope to read.
- Robustness of the gains across proposer agents and beyond the tested benchmarks is unestablished (authors say so).

## Out-of-Scope Discoveries (noted, not pursued)
- **Framing mismatch in the spike premise (highest-priority flag).** #952/SCOPE frame MetaHarness as an orchestration/autonomy-control concept to benchmark our workflow against; the paper does not support that framing. Synthesis/human should explicitly reconcile this before G5.4 (autonomy migration) leans on MetaHarness, or the recommendations will cite a control model that does not exist in the source.
- **The real "autonomy controls" literature is adjacent, not this paper.** "Contract-Driven Adversarial Verification" (2605.25665), "Code as Agent Harness: Executable/Verifiable/Stateful" (2605.18747), "HarnessX" (2606.14249) plausibly carry the verification-gate / bounded-authority / statefulness concepts the spike actually wants. **Warrants a small follow-on literature spike** if the autonomy-migration question stays live.
- **The transferable on-target idea:** the "abstraction ceiling of compressed feedback" — an autonomous improvement loop does better with rich, uncompressed, navigable evidence of prior attempts than with scalar/summary feedback. A legitimate lens for the internal track's cycle-review/lessons loop, independent of delivery autonomy.

## Recommendations Summary
- **G1**: `2603.28052` = "Meta-Harness: End-to-End Optimization of Model Harnesses" (Lee, Nair, Zhang, Lee, Khattab, Finn; Stanford IRIS; 2026-03-30). An **automated harness/context-engineering optimizer** — agentic outer loop (Claude Code proposer) evolving LLM-harness *code* via uncompressed full-trace filesystem feedback + Pareto selection; +7.7pts/4× fewer tokens (classification), +4.7pts cross-model (IMO-math retrieval), +1.7–2.1pts (TerminalBench-2).
- **G1 (critical for synthesis)**: Contains **no** delivery-orchestration, checkpoint, rollback, bounded-authority, escalation, prompt-injection, or agent-isolation machinery. **Not** a source of autonomy-de-risking *delivery* controls; synthesis must not derive such recommendations "from MetaHarness."
- **G1 (transferable insight)**: One carry-over idea — **evidence richness**: compressed scalar/summary feedback has an "abstraction ceiling"; autonomous improvement benefits from rich, navigable, uncompressed evidence of prior attempts. Plus one integrity discipline: strict search/held-out-test separation + Pareto multi-objective.
- **G1 (ecosystem)**: Outer-loop-search shape is table-stakes (DSPy/GEPA/OpenEvolve/ACE lineage); novelty = harness-code-as-search-space + full-trace feedback channel. The autonomy/verification concepts the spike wants live in **adjacent** 2026 harness papers — flagged as a follow-on read.
- **Faithfulness note**: All numbers are single-lab, self-reported, thin on stated limits, and partly test-set-directed (coding). Do not over-endorse.

---

*Persisted by the research leader from the external researcher's returned findings (agent was blocked from writing the file directly). No Unimatrix access was made; the codebase was not read by the external track.*
