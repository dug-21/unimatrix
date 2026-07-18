# SCOPE — ass-099: Evaluate the MetaHarness concept against Unimatrix's protocol/agent/skill workflow — where does it beat us, and what should we adopt?

**Status: APPROVED (Phase 1 complete, 2026-07-11). Directional this phase; a next phase deepens.**

Origin: GH issue **#952** ("Evaluate the MetaHarness concept for how our protocol workflow's could be
improved"). Deep research of the paper at `https://arxiv.org/pdf/2603.28052`, evaluated against our
shipped orchestration system (`.claude/protocols/uni/`, `.claude/agents/uni/`, `.claude/skills/`) and
grounded in *real* recurring-hotspot evidence from our own cycle reviews and lessons-learned — not a
vibes comparison.

This is a **meta-process** spike: the object under study is our own delivery machinery, not the
product. It produces no code and no Unimatrix writes — it produces FINDINGS.md and, if warranted,
design input for a follow-on design spike.

## Framing

Three distinct jobs the spike must keep separate:

- **Describe** MetaHarness faithfully from the paper (external/literature) — capabilities, claims,
  evidence, and its stated boundaries. Reproduce, don't editorialize.
- **Diagnose** our own recurring workflow pain empirically (internal) — mine cycle-review hotspots +
  stored lessons + the auto-memory feedback corpus for *consistent* patterns, not one-offs.
- **Prescribe** — where the MetaHarness concept (or the ecosystem around it) addresses a diagnosed
  pain we actually have, recommend an adoption shape. A capability we lack is only worth naming if it
  maps to a real, evidenced hotspot; novelty for its own sake is out of scope.

The load-bearing discipline: **recommendations must trace to evidenced pain.** Every "we should adopt
X" ties back to (a) a MetaHarness capability and (b) a recurring hotspot/lesson in our own history.
A recommendation with only (a) is a gap note, not a recommendation.

**The autonomy question, precisely stated (human framing, 2026-07-11).** The interest is not looser
in-run gates. It is whether our methodology can **move human review up one level**: define a
goal/**capability** priority → deep-plan the capability → **autonomously deliver a *series* of
features (each still by our existing design/delivery/bugfix protocols)** to fulfill that capability →
human reviews at the **capability boundary** instead of per-feature. This maps onto our existing
capability-map model (`uni-capability` skill; goals→capabilities→behavioral evidence). The load-bearing
sub-question: **do MetaHarness's controls make this capability-level autonomous orchestration *less
risky*** — enough to be worth prototyping? This is under investigation only; no decision to implement
has been made, and the spike must not assume one.

## Goal (answerable questions)

Dual-track (see Breadth): external track answers G1; internal track answers G2–G3; synthesis answers
G4–G6.

- **G1 — MetaHarness capability summary (external/literature).** What is MetaHarness? Summarize its
  capabilities, the problem it solves, its architecture/control model, its claimed benefits and
  supporting evidence, and its *stated limitations*. Situate it against the adjacent ecosystem
  (agent-orchestration / harness / eval frameworks) so we know what is novel vs table-stakes. Deliver
  a faithful capability map, not an endorsement.
- **G2 — Our workflow capability inventory (internal).** What does our current system actually do —
  the design/delivery/bugfix/research protocols, the specialist agent roster, the skills, the gate
  model, the human-in-the-loop seams, and the Unimatrix knowledge loop? Produce a capability map of
  our orchestration comparable in shape to G1's, so G4 can diff them.
- **G3 — Recurring-hotspot & workflow-challenge diagnosis (internal, directional this phase).**
  Sampling `context_cycle_review` hotspots from recent feature cycles, stored **lessons-learned**, and
  the session auto-memory feedback corpus (`~/.claude/.../memory/`), identify the *consistent* workflow
  challenges — recurring gate rejections, swarm hazards, context/handoff failures, rework loops. This
  phase: a directional read with representative evidence citations (cycle ids, lesson ids, memory
  files) and a recurrence *impression* — not an exhaustive recurrence-counted sweep (that is the next
  phase). This is the ground truth the recommendations must earn against; flag where a deeper empirical
  pass would change the picture.
- **G4 — Comparison & opportunity map (synthesis).** Diff G1 vs G2. Where does MetaHarness do
  something we do not, or do better? Where are we already ahead? Cross the diff with G3: which
  MetaHarness capabilities land on a *real* diagnosed hotspot (adopt candidates) vs address pain we
  don't have (skip). Explicitly name what NOT to adopt and why.
- **G5 — Recommendations across the five issue axes (synthesis, directional).** For each axis the
  issue names, give evidence-traced recommendations:
  1. **Benefits we lack** — capabilities MetaHarness/ecosystem has that map to a G3 hotspot.
  2. **Controls & testability** — how to make the workflow more inspectable/verifiable (gates,
     evidence, behavioral assertions).
  3. **Security** — workflow/agent-orchestration security (prompt-injection surface, swarm shared-
     worktree/git hazards, agent isolation, secret handling) — *not* product security.
  4. **Autonomy migration** — evaluate the **capability-level autonomous orchestration** shift
     (Framing): human review moves up from per-feature to the capability boundary; a series of
     protocol-driven features is delivered autonomously to fulfill a planned capability. Which
     MetaHarness controls (if any) de-risk this — and what is the smallest safe step toward it? Keep
     the human-held boundary explicit: **not fully autonomous**, and this is investigation, not a
     commitment to build.
  5. **Key perceived gaps** — the highest-leverage improvement gaps overall.
- **G6 — Prioritized recommendation shape + go/no-go on a follow-on (synthesis, directional).** Rank
  the recommendations by leverage vs cost/risk. State which (if any) warrant a follow-on **design**
  spike vs a lightweight protocol edit vs "no action". This spike recommends; it does not change any
  protocol/agent/skill file.

## Breadth

`industry` + `code` (Case 3 dual-track).

- **External track (`uni-external-researcher`):** the MetaHarness paper (G1) + the surrounding
  agent-orchestration / harness / eval-framework ecosystem for situating novelty. No Unimatrix access.
- **Internal track (`uni-spike-researcher`):** our protocols/agents/skills inventory (G2) + the
  empirical hotspot/lesson/memory diagnosis (G3). Read-only in Unimatrix (`context_cycle_review`,
  `context_search`, `context_get`).
- **Synthesis (`uni-spike-researcher`):** G4–G6 from both track files.

## Approach

- `literature` for G1 (read + faithfully summarize the paper and situate it).
- `investigation` for G2 (capability inventory), G3 (directional hotspot read this phase), and G4–G6
  (comparison, recommendations).

## Confidence required

- `directional` for all goals this phase — faithful description, evidence-cited but not
  recurrence-counted diagnosis, and defensible recommendations; no PoC, no validated build. A next
  phase deepens G3 to `empirical` (recurrence counts) and the recommendations to a concrete backlog.

## Target outputs

- **G1** — MetaHarness capability map (capabilities, control model, claimed benefits + evidence,
  stated limits, ecosystem situating).
- **G2** — our-workflow capability map (protocols, agents, skills, gates, human seams, knowledge loop).
- **G3** — recurring-hotspot/challenge list with representative evidence citations (cycle ids / lesson
  ids / memory files) and a directional recurrence read; note where a deeper counted pass would matter.
- **G4** — comparison + opportunity map (their-edge / our-edge / adopt-candidates crossed with G3 /
  explicit do-not-adopt).
- **G5** — evidence-traced recommendations across the five axes (benefits, controls/testability,
  security, autonomy-migration, key gaps).
- **G6** — prioritized recommendation shape + go/no-go on a follow-on design spike vs protocol edit vs
  no-action.

## Constraints

**Hard (fixed):**
- **Research only — no committed code, no PR, no protocol/agent/skill edits.** This spike recommends;
  changes (if any) happen in a downstream design/delivery session after human approval.
- **No Unimatrix writes** (research is provisional; per protocol Phase 4). Read-only tools only.
- **Recommendations must trace to evidenced pain** (Framing) — a capability gap with no G3 hotspot
  behind it is reported as a gap, not recommended as an adoption.
- **Faithful description of MetaHarness** — reproduce the paper's claims and *its own stated limits*;
  do not oversell. If the paper is inaccessible/misidentified at execution, say so plainly rather than
  hallucinating its contents. (The arxiv id `2603.28052` post-dates training — the external researcher
  fetches it live.)
- **Auto-memory / lessons are sensitive process history** — cite by id/filename; do not paste
  wholesale sensitive content into a committed file beyond what the finding needs.

**Hypothesis (challengeable positions to TEST — not givens):**
- **Capability-level autonomous orchestration is worth investigating — bounded, "not totally
  autonomous."** The human-held position: human review may move up to the *capability boundary*
  (plan a capability, then autonomously deliver its series of protocol-driven features), but it does
  not go fully hands-off — the human still reviews at that raised level, and this is investigation, not
  a commitment. The researcher treats this as a stated position: assess feasibility and whether
  MetaHarness controls de-risk it; surface tension but do not erase the boundary or assume a build.
- **MetaHarness addresses pain we actually have.** It may solve problems we don't have; G3 is the test.
- **Our current gate/swarm model is a strength worth preserving**, not just overhead to automate away
  — recommendations should weigh what we'd lose.

## Dependencies

None hard. Relevant prior context (not blocking): the delivery/design/bugfix/research protocols
themselves; the swarm-hazard and process lessons already captured in session auto-memory and Unimatrix
lessons (the G3 corpus).

If **go**, ass-099 unblocks: a follow-on **design** spike for any adopted workflow change (issue #952
stays open until the human closes it).

## Prior art

- `.claude/protocols/uni/` — design, delivery, bugfix, research protocols + agent-routing (the G2
  subject).
- `.claude/agents/uni/` — the specialist roster (the G2 subject).
- `.claude/skills/` — the skill set incl. store-lesson/retro/capability (the G2 subject).
- `context_cycle_review` (per-cycle hotspots + evidence), stored **lessons-learned**, and the session
  auto-memory corpus at `~/.claude/projects/-workspaces-unimatrix/memory/` — the G3 evidence base.
- The MetaHarness paper `https://arxiv.org/pdf/2603.28052` — the G1 primary source (fetched live).

## Tracking

GH Issue: **#952** (open; closed by human on findings acceptance).

---

## OPEN — RESOLVED (human, 2026-07-11)

1. **Autonomy framing.** RESOLVED — the axis is **capability-level autonomous orchestration** (review
   moves up to the capability boundary; a series of protocol-driven features delivered autonomously to
   fulfill a planned capability), and the sub-question is whether MetaHarness controls de-risk it. Not
   fully autonomous; investigation only, no commitment. Folded into Framing + G5.4 + Hypothesis.
2. **G3 depth.** RESOLVED — **directional this phase** (representative citations + recurrence
   impression), not an exhaustive counted sweep. Next phase deepens to empirical.
3. **Deliverable actionability.** RESOLVED — **directional recommendations + areas for improvement**,
   no concrete edit backlog and no file edits this phase.

None blocking. Phase 1 complete; proceeding to Phase 2 (dual-track execution).
