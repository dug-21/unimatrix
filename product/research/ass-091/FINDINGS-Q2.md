# FINDINGS-Q2: Consumer demand — how a retro agent actually wants to use the planes (ass-091, CONSUMER-DEMAND TRACK)

**Spike**: ass-091 (GH #898) · **Date**: 2026-07-04 · **Approach**: grounded-empirical (leads with the real bugfix-891 retro) · **Confidence**: validated (every claim traced to the GH #891 comment record or the Q1 file:line map)

> Consumes FINDINGS-Q1.md (the Plane A / Plane B / third-source map) — does not re-derive it. Answers SCOPE Q2 only. Primary evidence = the real retro on **bugfix-891** (SCOPE Appendix A), cross-checked against the **actual** `## Knowledge Stewardship` blocks now on GH #891.

## TL;DR
- **Four retro needs, three sources, one clean split.** *Counts* → Plane A (measured, salience-blind). *Causal attribution (entry X → decision Y)* → the **GH-comment third source** (agents hand-assert it in stewardship blocks); Plane A knows served-and-happened but not the causal edge; Plane B corroborates the *fetch* at Reconstructed fidelity only. *Rework-why* → a **split join**: rework *count* is Plane A (`rework_session_count`), rework *why* is GH-comment prose; no plane joins them. *Human-intervention ledger* → **served by nothing durable** — lives in ephemeral tier-1 conversation, referenced only second-hand in GH prose.
- **What the leader hand-composed (the design target):** (a) **salience re-ranking** — elevating #5417/#3827 over the summary's mechanical top-5 (#92/#93/#648/#684/#922); and (b) the **cross-source causal join** stitching per-agent local attributions + rework count + rework-why + the human's call into one cycle-level "why it went this way" arc.
- **Hypothesis verdict:** TRUE across cycle types *for the causal/knowledge-reuse axis, and only because stewardship discipline is gate-enforced* — it held even on this messy 8-phase multi-agent rework cycle. It **flips on the human-intervention axis** (neither prose nor transcript is authoritative) and is better restated as **complementary orthogonal axes** (durable prose = *why/salience*; transcript = *what/when skeleton*) than as primary/fallback on one axis.

---

## The bugfix-891 trace, as it actually ran (the anchor)

bugfix-891 retired a tick-starved detection rule. The GH #891 record is **8 phase comments**: investigator → design-reviewer → uni-zero product review → fix → tester **REWORK** → rework → tester **PASS** → gate PASS. It is a *messy multi-agent rework cycle*, not a clean one-shot — which makes it the stronger pressure-test subject.

Every phase posted a `## Knowledge Stewardship` block (Queried / Stored / Declined). The gate comment explicitly checks and PASSes on their presence ("all three phase comments carry a block"). That enforcement is load-bearing for the hypothesis (Q2.3).

The two standout entries and their **causal roles are asserted verbatim in the stewardship prose**, not inferred by the retro:
- Investigator: *"surfaced #3827 (crt-034 tick-ordering ADR: no new steps between compaction and rebuild — **constrains where the capture phase may live**), #5417 (bugfix-879 lesson: recovery/capture pass must match the destructive pass's bound — **applied to the proposed capture phase**). All directly shaped the proposed fix."*
- Design-reviewer: *"#5417 (**applied to confirm the capture must stay unbounded**), #3827 (**applied to confirm intra-block placement is legal**)."*

So the (entry → decision) edges the retro reported were **hand-written by the working agents into durable GH prose**. Plane A never held them; Plane B held only a Reconstructed shadow of the `context_get` fetch. (Grounding: #5417 is the bugfix-879 recovery-set-coverage lesson; #3827 is the crt-034 SR-06 tick-ordering ADR — confirmed via `context_get`, read-only.)

---

## Q2.1 — Each retro need → which plane serves it, and where it falls short

Legend: **A** = Plane A durable observations · **B** = Plane B in-memory transcript · **GH** = GH-comment third source (`## Knowledge Stewardship` blocks) · **T1** = ephemeral tier-1 live agent/human return payloads (gone after session).

| Retro need | Served by | bugfix-891 evidence | Where it falls short |
|---|---|---|---|
| **Counts** (entries served, sessions, rework count) | **A** (measured, force-reproducible) | "57 entries served" from the review's Knowledge Reuse section = `feature_knowledge_reuse`, `query_log ∪ injection_log` (Q1 A.3; `review_aggregates.rs:88-102`). `session_count`/`total_records` from observations (`report.rs:23-32`). | **Salience-blind.** The count is a true aggregate but names *which* entries only via the summary's top-entries table, ranked by a mechanical heuristic (frequency/recency). On bugfix-891 that table ranked **#92/#93/#648/#684/#922** — none of which were the entries that actually drove the fix. The count is correct; the ranking is wrong for retro purposes. |
| **Causal attribution** (entry X → decision Y) | **GH** (primary) · A (partial) · B (corroboration only) | The (#5417 → "capture must stay unbounded"), (#3827 → "intra-block placement is legal") edges are asserted in the investigator's and design-reviewer's stewardship blocks. | **No plane holds the causal edge.** Plane A can prove #5417 was *served* (injection_log) and that a capture-design *decision happened* (cycle_events/observations) — but not that one *caused* the other. Plane B at best shows a `context_get(#5417)` fetch occurred, at **Reconstructed ~0.81 fidelity** (Q1 B.2), and asserts no "because". The causal claim exists **only** because the agent hand-wrote it into GH prose. Absent the stewardship discipline, this need is unserved. |
| **Rework-why join** (why did the cycle need a second iteration?) | **Split: A (count) + GH (why); nothing joins them** | Rework *count*: `rework_session_count` from `SessionRecord` outcome ratio (Q1 A.3; `review_aggregates.rs:81-97`) + the phase timeline from `cycle_events` shows the REWORK→PASS loop. Rework *why*: the tester's REWORK comment states it — "*the fix updated the Rust tests… but missed the Python integration layer*" (4 obsolete integration tests). | **The join is manual.** Plane A gives "there was 1 rework iteration" and *when*; it cannot say *why*. The why is GH-comment prose in a **different** comment (tester verdict + rework summary) than the count's source. No plane produces "rework #1 was caused by the Rust/Python test-layer gap." The retro leader stitches count (A) to cause (GH) by hand. |
| **Human-intervention ledger** (where/why the human overrode) | **Nothing durable — T1 only, GH second-hand** | The human made ≥2 load-bearing calls: (1) direction sign-off (target- vs source-Deprecated, F2), (2) **retire vs build** — "*Implemented the human-finalized retirement scope (not the capture-table build — deferred to #895)*". uni-zero even flagged OQ5 for the human. | **Weakest-served need.** The human's decision emits **no** tool-event unless it triggers a `context_` call, so Plane A misses it. Plane B *might* hold the human's in-session message text — but memory-only, truncated, Reconstructed, and **purged at cycle close** (Q1 B.3). GH prose records the *outcome* ("human-finalized retirement scope") **second-hand**, never the human's own reasoning. The actual deliberation lived in ephemeral tier-1 conversation the retro subagent never had (Appendix A tier-1). This is a genuine hole all three durable sources share. |

**Cross-cutting shortfall.** Three of the four needs (causal attribution, rework-why, human-ledger) are only partly or not-at-all served by the two *planes*; the load falls on the **GH-comment third source** and, for the human ledger, on **ephemeral T1 that no source captures**. The planes serve *counts and the what/when skeleton* well; they serve *why and who-decided* poorly.

---

## Q2.2 — What the leader had to hand-compose (the design target)

Appendix A: *"'shaped/steered the decision' — leader's own synthesis; no plane asserted the causal link."* The real trace lets me decompose that hand-composition into **two distinct artifacts**, because the per-agent stewardship blocks *do* assert local causal links — so the leader's work is not "invent attribution from nothing," it is:

**(a) Salience re-ranking.** The summary's top-entries table is ordered mechanically and ranked #92/#93/#648/#684/#922. The leader instead surfaced **#5417/#3827** as the standouts — reading their load-bearing role off the stewardship prose ("*directly shaped the proposed fix*", "*applied to confirm the capture must stay unbounded*"). The re-rank signal is **"which served entry an agent explicitly says it applied,"** which lives in GH prose, not in the count. The tool ranks by served-frequency; the retro needs ranking by *applied-causality*.

**(b) The cross-source causal join.** Each stewardship block is a **local, per-agent, per-phase** attribution ("in my phase, X shaped sub-decision Z"). No block — and no plane — asserts the **cycle-level arc**: *"prior lessons #5417/#5420 were correctly recalled and shaped a sound capture-before-destroy design; the human then overrode scope to retire-not-build (deferred to #895); the one rework iteration was caused not by bad knowledge reuse but by the Rust-fix agent's briefing not spanning the parallel Python integration layer."* Producing that arc requires joining: per-agent local attributions (GH) + rework count (A) + rework-why (GH, a different comment) + the human decision (T1/second-hand GH). The leader performs that join by hand across sources that share no key.

**Design target.** (b) is the headline deliverable's target. The tool should let the retro *assemble* the cycle-level causal arc without hand-stitching — i.e., surface the per-agent applied-entry attributions **already keyed to phase and to the served-entry count**, and expose the rework iteration **joined to its cause-comment**, so salience-by-causality (a) and the cross-source arc (b) are a query, not a compose. It cannot *manufacture* the human-ledger content (that data is genuinely absent), but it can make the absence explicit rather than silently missing.

---

## Q2.3 — Pressure-testing the hypothesis

**Hypothesis:** *GH-comment discipline (durable prose), not the transcript, is what makes retros robust; the transcript is the fallback when durable artifacts are thin.*

**Verdict: substantially TRUE for the causal/knowledge-reuse axis, but conditional and incomplete. It flips under three named conditions, and its primary/fallback framing should be restated as complementary orthogonal axes.**

**Where it holds (strongly, and on the hard case).** bugfix-891 is the *messy multi-agent rework* end of the spectrum, yet the retro's real substance — the standout entries, their causal roles, the rework cause — all came from durable GH prose. The transcript (Plane B) was **corroboration only, at Reconstructed fidelity** (Appendix A; Q1 B.2). So the hypothesis survives its hardest cycle type. A clean single-agent bugfix satisfies it trivially: one stewardship block carries everything, transcript near-irrelevant.

**Condition 1 — it holds only because discipline is *enforced*.** Robustness is not intrinsic to prose; it is a function of the block actually being written. On bugfix-891 all 8 phases posted a block **and the gate PASS-checked their presence**. Remove that enforcement and any phase that skips or thins its block collapses that phase's durable record to nothing — at which point the transcript becomes the *only* witness of what that agent queried/applied, and the hypothesis inverts for that phase. **The hypothesis is downstream of the stewardship gate, not of prose per se.**

**Condition 2 — it flips on the human-intervention axis regardless of discipline.** Stewardship blocks are agent-authored and scoped to agent knowledge-ops (Queried/Stored/Declined). They structurally **do not** capture the human's decisions or reasoning. bugfix-891's retire-vs-build and direction calls survive only as second-hand agent references. For this axis, durable prose is *incomplete* and the transcript (if it captured the in-session human exchange) would be *more* authoritative — except it is memory-only, truncated, Reconstructed, and purged. So on the human axis **neither** source is authoritative: the hypothesis's two-source world is missing a third actor.

**Condition 3 — "fallback" mis-frames the transcript's real role.** The transcript is not a lower-fidelity substitute for prose along one quality axis; it covers a **different axis**. Durable prose is strong on *why/causal/salience* and weak on *sequence/timing*. The transcript (even Reconstructed) is strong on the ***what/when skeleton*** — the ordered sequence of tool calls — and weak on *why/depth* (Appendix A: "recovers the what/when skeleton at high fidelity but degrades on causal attribution and depth"). When the retro question is temporal ("in what order were the lessons consulted vs. the decision made?"), the transcript is **primary**, not a fallback. They are complementary, orthogonal sources.

**Restated hypothesis (the version the design should encode):**
> Durable GH-comment discipline is the primary source of a retro's *causal and salience* substance, and it is robust across cycle types **so long as the stewardship gate enforces it**. The transcript is not a general fallback but the primary source for the *what/when temporal skeleton*, complementary to prose. Both share a blind spot on the *human-intervention ledger*, which is neither durably captured today — a gap the design must surface explicitly rather than let the leader silently paper over.

---

## Unanswered Questions
None from SCOPE Q2 — all three sub-questions answered against the real bugfix-891 trace. One dependency handed forward: the *mechanism* by which the design surfaces applied-entry attribution keyed to phase/count (the Q2.2(b) join) is a Q3/headline concern, not a Q2 finding.

## Out-of-Scope Discoveries
- **The human-intervention ledger is an unowned durability gap (carry-forward, possible new spike).** All three durable sources miss the human's own decision rationale; it lives only in ephemeral tier-1 conversation. bugfix-891's retire-vs-build call is the concrete instance. Neither this spike nor crt-057 owns closing it. One-line rationale: self-learning (#5219) that cannot see *why the human overrode* is blind to the highest-signal decisions in a cycle. Flag for a future spike; do **not** solve here (and any solution collides with NG-1 if it tries to persist raw conversation).
- **Salience-by-frequency vs salience-by-applied-causality (feeds ass-090).** The summary's top-entries ranking (#92/#93/#648/#684/#922) diverged completely from the causally load-bearing entries (#5417/#3827). ass-090 (distill-signal-into-summary) should consider an *applied-attribution* signal sourced from stewardship prose, not just served-frequency — but that content is agent prose, so per #5030 it must land as a content-opaque derived signal, never raw text.

## Recommendations Summary
- **Counts:** keep on Plane A (measured, force-reproducible); do **not** treat the summary's top-entries table as the salience ranking — it ranks by frequency, not by causal load.
- **Causal attribution:** design must consume the GH-comment stewardship blocks as the authoritative (entry → decision) source; Plane A/B cannot assert the edge. Make the stewardship gate a hard precondition of retro robustness, not a nicety.
- **Rework-why:** the design should expose the rework *count* (Plane A) pre-joined to its *cause-comment* (GH) so the leader stops hand-stitching them.
- **Human-intervention ledger:** the design must **surface the absence explicitly** (no durable source holds it) rather than let the leader silently hand-compose it; closing the gap is a separate carry-forward spike, constrained by NG-1.
- **Hand-composed gap = headline target:** automate (a) salience re-ranking by applied-causality and (b) the cross-source causal join; these are the two artifacts the bugfix-891 leader built by hand because no source delivered them.
- **Hypothesis:** adopt the restated form — durable prose = primary for *why/salience* (robust iff gate-enforced); transcript = primary for the *what/when skeleton* (complementary, not fallback); both blind to the human ledger.
