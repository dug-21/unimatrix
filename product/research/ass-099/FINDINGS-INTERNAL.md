# FINDINGS (INTERNAL TRACK): ass-099 — Unimatrix workflow capability inventory + recurring-hotspot diagnosis

**Spike**: ass-099 (GH #952) · **Track**: INTERNAL (G2, G3 only) · **Date**: 2026-07-11 · **Approach**: investigation · **Confidence**: directional. Read-only Unimatrix throughout (`context_cycle_review`, `context_search`, `context_get`). No writes, no code, no ADRs.

---

## G2 — Capability map of our orchestration system (six axes, diffable against an external capability map)

Our system is a **protocol-driven, human-gated, multi-agent swarm** over a persistent knowledge substrate (Unimatrix). Not a monolithic harness — a set of session protocols executed by a **non-authoring coordinator** ("Leader" = `uni-scrum-master` role) that spawns single-purpose specialists, enforces glass-box validator gates between stages, and retains cross-session knowledge as typed graph entries.

**Axis A — Session orchestration (the "harness" layer)**
- A1. Intent→protocol routing (`uni-agent-routing.md`): intent → session type (design/delivery/bugfix/research) → protocol. Hard rule: no `IMPLEMENTATION-BRIEF.md` ⇒ force design.
- A2. Non-authoring coordinator: Leader spawns agents, manages gates, updates GH issues, runs cycle attribution; MUST NOT generate content or spawn itself.
- A3. Swarm composition: coordinator + validator at gates; max 5 workers/stage in dependency waves; parallel spawns in one message; skip-swarm for typos/one-liners/docs.
- A4. Attribution/telemetry: `context_cycle(start|phase-end|stop)` brackets every phase → the observation data `context_cycle_review` mines.

**Axis B — Specialist agent roster (19 defs, single-purpose, each with an explicit negative boundary)**
- Design: `uni-researcher` (SCOPE.md), `uni-architect` (architecture + **sole** ADR authority), `uni-specification`, `uni-risk-strategist` (scope-risk / architecture-risk modes), `uni-vision-guardian`, `uni-synthesizer` (BRIEF + ACCEPTANCE-MAP + GH issue).
- Delivery: `uni-pseudocode`, `uni-tester` (test-plan design + execution; owns infra-001 suite catalog + failure triage), `uni-rust-dev` (`crates/**`), `uni-js-dev` (`packages/**`, fail-open + size-budget + wire-parity contracts).
- Bugfix/shared: `uni-bug-investigator` (deep-reasoning, model fable), `uni-security-reviewer` (fresh-context cold-diff), `uni-docs` (conditional, blast-radius-scoped).
- Research: `uni-spike-researcher` (internal/synthesis, Unimatrix read-only), `uni-external-researcher` (external, **no** Unimatrix), `uni-research-sm` (campaign).
- Gate/product-lens: `uni-validator` (three-gate), `uni-zero-reviewer` (advisory product-lens at human gates; cannot mutate capability map).

**Axis C — Gate model (control layer)**
- Validator gates (delivery): **3a** Component Design Review (pseudocode/test-plan ⟶ architecture/spec/risk); **3b** Code Review (matches pseudocode, compiles, no stubs/`todo!()`/`.unwrap()`, no file >500 lines, security + `cargo audit`); **3c** Final Risk-Based Validation (behavioral-outcome proof from the user's real entry path per vnc-047/#944, risk coverage, mandatory integration smoke).
- Validator gate (bugfix): ONE — Bug Fix Validation — validated **from artifacts** (reads diff + tester's posted results, does not re-run cargo/clippy/pytest per lesson #5207).
- Semantics: PASS → phase-end+commit+proceed; REWORKABLE FAIL → loop to prior stage (max 2); SCOPE FAIL → stop→human. Gates read **committed HEAD**, not working tree.
- Glass-box: every gate writes `reports/gate-{id}-report.md` (or GH comment) — inspectable artifact, not a boolean.
- Advisory product gates (`scope-review`, `design-review`, `fix-approach`, `pr-review`): `uni-zero-reviewer` relays a stance verbatim; Leader never parses or gates on it.

**Axis D — Human-in-the-loop seams (explicit — the autonomy crux)**
- Design: human approves `SCOPE.md`; human reviews all design artifacts before delivery. Delivery: **★ HUMAN MERGE GATE ★** (otherwise autonomous). Bugfix: **★ HUMAN CHECKPOINT ★** after diagnosis (before any code) + **★ HUMAN MERGE GATE ★**. Research: human approves FINDINGS.md.
- **Net: human review today sits at the per-feature / per-bugfix boundary** — one approval to enter delivery, one to merge. Exactly the seam the autonomy question proposes to raise to the capability boundary.

**Axis E — Knowledge loop (persistent substrate)**
- Store: ADRs → Unimatrix (`decision`, sole store); patterns/procedures/lessons via `uni-store-*`; delivery agents query before, store after.
- Retro: strict order **merge → close → retro**; `/uni-retro` runs `context_cycle_review` then `uni-architect` (retro mode) extracts patterns/procedures/lessons/edges. Close-before-merge or retro-before-close = defect.
- Provenance: updates via `context_correct` (not deprecate+store); status flips via `context_tag` replace.

**Axis F — Capability-map model (`uni-capability`; load-bearing for the autonomy question)** — the layer **between goals (intent) and features (delivery)**.
- Unit: a *capability* must **exist AND behaviorally work** for a goal to be delivered. `category: capability`; a shared cap is ONE entry with multiple `Advances` edges.
- Fields: `kind` (functional|nfr); `name` (a user/operator OUTCOME, never an implementation); `why`; `done_when` (1–2 **behavioral, runnable** statements = proof gate + definition of done); `delivered_by`; `proven_by`.
- Edges: `Advances` (cap→goal), `Prerequisite` (cap→cap DAG), `Motivates` (research→cap, inert until graduation), `About` (nfr→functional).
- Archetypes: **threshold** (binary, `proven` terminal) vs **curve** (asymptotic, `proven` never terminal, needs a **keystone ruler** cap). Claim accounting: **Claim-floor** (named threshold caps that must be `proven` to claim the goal) vs **North-star** (curve caps, never terminal).
- **Firewall:** status→`proven` ONLY on attached behavioral, real-artifact evidence; a merged feature with no behavioral proof of its `done_when` stays `partial`. Status is a **tag** (`delivery:proven|partial|missing|asserted`). `proven_by` must name re-runnable tests exercising the **assembled production path** — not a proxy/tautology/injected-dependency (catches #917/#918/#930).
- Two sub-processes: **structural** (what caps exist) — uni-zero + research, human-in-the-loop; **status** (is it done) — per-delivery, enforced at Vision-guardian / Gate 3c.
- **Where human review sits today:** structural changes and status→`proven` are **uni-zero vision-session acts, human-in-the-loop**; `uni-zero-reviewer` only *recommends*. So a capability is **planned** in a vision session (goal→caps→`done_when`), **delivered** as a *series of independent per-feature protocol runs (each with its own human merge gate)*, and **proven** only when Gate 3c attaches behavioral evidence. **No existing mechanism plans a capability and then autonomously drives its whole feature series** — each feature re-touches the human at its own merge gate. That gap is the autonomy-migration target.

**G2 synthesis handoff:** diff this six-axis map against the external capability map. Axes **D+F together are the crux**: the higher-level unit and its behavioral proof gate already exist, but nothing autonomously sequences a capability's feature series — the exact seam a MetaHarness-style control must de-risk.

---

## G3 — Recurring workflow-challenge clusters (directional; recurrence impression per cluster)

Evidence base: `context_cycle_review` on vnc-040, vnc-034, bugfix-851 (crt-033 = **no observation data**); `context_search` category `lesson-learned` (~40 entries); the 21-file auto-memory corpus at `~/.claude/projects/-workspaces-unimatrix/memory/` (cited by filename per sensitivity constraint).

- **H1 — Knowledge-Stewardship / gate-report omission (HIGH).** Agents omit the `## Knowledge Stewardship` block → REWORKABLE FAIL with zero code change. Lessons **#5464** (corpus-labelled "recurring"; col-022/028, crt-030/033/036/039/051, nan-009/015, vnc-043), **#2657**, **#4976**, **#4532** ("most common bugfix gate failure"), **#647**, **#4155**. Memory `swarm-agents-must-emit-stewardship-report.md`.
- **H2 — Committed-state / working-tree vs gate (HIGH).** Gates read committed HEAD; agents leave prod files uncommitted or commit to wrong branch. **#2477**, **#4155**, **#2463**, **#683**, **#4094** (wrong branch → cherry-pick recovery). Memory `shared-checkout-branch-trap.md`.
- **H3 — Swarm shared-worktree / git hazard (MED–HIGH).** Parallel non-isolated subagents corrupt each other's uncommitted work; root trigger = agents running `git checkout/restore` to isolation-test a red crate. **#5101** (vnc-038), **#3925**, **#684**. Memory `swarm-shared-worktree-git-hazard.md`, `swarm-fmt-churn-revert-before-wave-commit.md` (noted 2x).
- **H4 — Behavioral-proof / test-coverage shortfall at gates (HIGH).** Tests exist but don't exercise the specified/assembled path. **#4202** (crt-048), **#2758**, **#3806** (crt-033), **#3935** (crt-036), **#3548**, **#2577**, **#4473**. Memory `tester-must-run-foreground-and-post.md`. Note: the uni-capability firewall (#917/#918/#930) was built to close exactly this — pain and mitigation both in-corpus, i.e. partially addressed but still recurring.
- **H5 — Spawn-prompt / context-handoff omission (MED–HIGH).** Leader drops a load-bearing detail (branch, scope boundary, blast radius). **#4094**, **#5099** (vnc-038, blast radius incl. test fixtures + FLAG duty), **#4796** (CI-dependent ACs), **#3819** (Task tool down → Leader executes roles → stewardship gaps). Memory `swarm-file-scope-flag-adjacent-breakage.md`, `read-protocol-dont-adlib.md`, `local-gates-linux-only-ci-is-crossplatform.md`, `unimatrix-write-requires-agent-id.md`.
- **H6 — Cold-restart / multi-session context-reload cost (MED; high per-incident).** vnc-040 F-01 (170-min gap → 15 re-reads of SPEC/ARCH/SCOPE/RISK/ADR), F-05 (2.8h session gap); vnc-034 F-01 (355 KB before first write), F-03 (96 compile cycles), F-07 (100 files re-read), F-09 (30 sleep-workarounds, 6-sigma); lesson **#324**. Contrast: bugfix-851 (single 38-min session) was clean — cost scales with session span, not the protocol.

**G3 read:** H1/H2/H4 are the best-evidenced, corpus-labelled-recurring adoption targets (any adopted MetaHarness capability must map onto one to count as a recommendation per the Framing). H3/H5 feed G5.3 (security — more autonomy = more unattended swarm turns = more H3/H5 surface) and G5.4. **H6 is the autonomy cost signal**: capability-level autonomy *increases* session span, so H6 is a risk the migration must budget for, not a pain it solves.

---

## Unanswered Questions
- **G3 is directional by design** (SCOPE OPEN-2). A counted pass would change two things: (a) re-rank H3 vs H5 (similar citation density); (b) establish whether H1/H4 recurrence is *declining* post-firewall or flat — deciding "solved-but-lagging" vs "open." Trend direction not establishable from a directional sample.
- **crt-033 has no `context_cycle_review` observation data** — its gate-3b pain survives only via lessons #3806/#3935, not telemetry. A counted pass should record which cycles lack observation data before weighting telemetry-derived hotspots.

## Out-of-Scope Discoveries
- **The uni-capability firewall is a pre-existing internal analogue of behavioral-verification controls** (`done_when`/`proven_by`/assembled-path, #917/#918/#930). Likely our *edge*, not a gap — flag so G4 does not mis-file it as an adopt-candidate.
- **"Storage doesn't fix recurrence"** (memory `read-protocol-dont-adlib.md`): H1/H5 persist *despite* being stored lessons that are served (vnc-040 served 55 lesson entries). Leverage is **structural enforcement at spawn/gate time**, not more knowledge — relevant to G5.2.
- **Corpus-hygiene guardrails** (`bugs-are-gh-issues-not-lessons.md`, `no-outcome-recording-in-unimatrix.md`): any autonomy/telemetry recommendation must not propose auto-storing bug outcomes as knowledge.

## Recommendations Summary
- **G2**: deliver the six-axis map (A orchestration · B 19-agent roster · C gates 3a/3b/3c + bugfix gate · D per-feature/merge human seams · E merge→close→retro knowledge loop · F goal→capability→behavioral-proof firewall) as the internal capability map; flag Axes D+F as the autonomy crux — the capability unit and proof gate exist, nothing autonomously sequences a capability's feature series.
- **G3**: six recurring clusters — H1 stewardship omission (HIGH), H2 committed-state vs gate (HIGH), H3 swarm worktree hazard (MED–HIGH), H4 behavioral-proof shortfall (HIGH), H5 spawn-prompt handoff omission (MED–HIGH), H6 cold-restart cost (MED). H1/H2/H4 are the strongest adoption targets; H3/H5 feed security+autonomy risk; H6 is the cost autonomy must budget. Directional only.

---

*Persisted by the research leader from the internal researcher's returned findings (agent was blocked from writing the file directly). Read-only Unimatrix; no writes, no code.*
