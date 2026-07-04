# ass-091 (SPIKE): Cycle-Review Data Planes, Retro Consumer Demand, and a Recommended `context_cycle_review` Design

> **uni-zero-authored scope.** Framed interactively with the human (uni-zero session, 2026-07-04).
> A research session begins only when the human confirms this SCOPE is complete (breadth / time-box
> in "Open Questions" still to nail). Read-only in Unimatrix — produces FINDINGS.md, stores nothing.

**Tracking:** GH #898
**Capability:** SL6 (#5225) — *the system learns from observed agent activity* (transcript is the substrate)
**Goal:** self-learning (#5219)
**Phase:** Assimilate (ass) — research spike
**Blocks:** **crt-057 (#894)** — its `include_transcript_candidates` boolean is now provisional
pending this spike's recommended design (human chose "redesign-before-delivery", 2026-07-04).
**Relationship to ass-090 (#896):** see "Relationship to other transcript work" — ass-091 owns the
authoritative data-plane map + review design; ass-090 (distill-signal-into-summary) should be
re-sequenced to consume this spike's output.
**Mode:** read-only investigation. Do NOT store anything in Unimatrix.

---

## Why this spike exists (the decision that spawned it)

crt-057 reached design-complete and human-locked: default review = non-destructive observation
summary; raw transcript gated behind a **boolean** `include_transcript_candidates` that is *also*
the sole one-shot destructive purge trigger. A subsequent uni-zero conversation surfaced that the
boolean is the wrong granularity — a retro almost never wants the whole ~50-event reconstructed
stream; it wants a *scoped slice* (by phase, by finding-anchor, or by regex match ± N events). The
human chose to **redesign crt-057's opt-in axis before delivery** rather than ship the boolean and
unwind a tested contract later. This spike produces the research + recommended design that redesign
consumes. crt-057's non-destructive-default core (D-1/D-2, the SL6 harvest fix) is unaffected;
only the opt-in retrieval axis is in scope here.

---

## Q1 (FOUNDATION) — The two data planes, articulated precisely

There are two distinct substrates behind the cycle review. Pin both down, file:line-grounded, and
draw the sharp line between them. (Part 1 of ass-090 begins this map; ass-091 owns the definitive
version — do not duplicate-and-diverge, produce the one authoritative map.)

**Plane A — durable observations (hook tool-events).**
- Hook-delivered, tool-based events, persisted to the observation table.
- The source of record. crt-057's enabling fact: the review summary is **100% observation-derived**
  and buffer-independent — byte-identical whether or not the transcript buffer is present
  (`build_report`, `report.rs`). Reproducible via `force`.
- Deliverable: what an observation event captures, its schema, what survives the cycle, and exactly
  which summary fields derive from it (counts, hotspots, phase timeline, knowledge-reuse).

**Plane B — in-memory transcription (`transcript_candidates`).**
- The richer per-session stream: streamed into an in-memory ring buffer, **never persisted to disk**
  (#4721/#4850), held past session close (crt-052 Wave B).
- Bounds that define its fidelity ceiling: 4 MiB ring-tail elision, 64-session cap, 24h TTL,
  per-event truncation (~few hundred bytes), `Primary` vs `Reconstructed` provenance (ADR-007,
  ~0.81 fidelity when reconstructed from observations).
- Consumed at exactly one content seam (`take_transcripts_for_feature` → `distill_before_purge`),
  attached out-of-band to the response, distilled OUTSIDE the server by the retro architect.
- Deliverable: buffer lifecycle, every reclamation path with its trigger, and the loss/provenance
  accounting a consumer can see.

**The sharp line (make crystal clear):** the cycle review consumes *nothing* from Plane B for its
prose; the downstream retro flow is the *only* consumer of Plane B content. A third de-facto source
exists and must be named — **persisted GH-issue comments** (agent `## Knowledge Stewardship` blocks):
full-fidelity, durable, but neither plane. The retro's real provenance runs across all three.

---

## Q2 — Consumer demand: how a retro agent actually wants to use these

Not hypothetical. **Grounded in a real retro that just ran** (bugfix-891). That trace is the primary
evidence for this spike and is captured in Appendix A so it survives. What it showed:

- "57 entries served" → came from the **summary** (Plane A, measured).
- The two standout IDs (#5417, #3827) → came from **agent GH comments** (the third source), *not*
  from the summary's top-entries table (which ranked a different five) and *not* primarily from the
  transcript (Plane B corroborated only, at `Reconstructed` fidelity).
- The causal "steered the decision" narrative → the leader **hand-composed** it; no plane asserted it.

Research questions:
1. For each thing a retro needs — counts, **causal attribution** (entry X → decision Y), the
   **rework-why** join, the **human-intervention ledger** — which plane serves it, and where does
   each fall short?
2. What did the leader have to hand-compose because no plane delivered it? That gap is the design
   target.
3. Pressure-test the standing hypothesis: *GH-comment discipline (durable prose), not the transcript,
   is what makes retros robust; the transcript is the fallback when durable artifacts are thin.* Is
   that true across cycle types (clean bugfix vs. messy multi-agent rework), or does it break down?

---

## Q3 — Scoped/searchable retrieval feasibility (THE crt-057 BLOCKER)

The redesign needs these answered concretely, grounded in Q1's numbers:
- **Regex/`match` over truncated text** — does it survive per-event truncation, or does clipping gut
  it? Measure hit-rates over the retained head across real cycles; report when a `no-match` means
  "didn't happen" vs. "past the truncation boundary" (and whether high `elided_bytes` must be flagged
  to the caller).
- **Finding-anchor timestamp-join** — hotspots already carry timestamped event clusters (e.g. F-03
  `Timeline: +90m(1) +95m(2▲)`). Does referencing a finding + window reliably land the right events?
- **★ Purge reconciliation (load-bearing).** Today the opt-in is the *sole one-shot destructive*
  trigger (extract-all-then-purge). Scoped retrieval breaks that model: if the caller pulls 6 events,
  what happens to the buffer — purge the slice, purge all, purge nothing? crt-057's entire
  non-destructive guarantee hangs here. Produce the answer the redesign will encode.
- **API shape** (design output, not open research): reconcile the conversation's proposed block
  `transcript: { phase?, anchor?, match?, window? }` against Q1/Q2 — default (summary only), the
  opt-in's scoping, and how "full dump" degrades to `match: ".*"` with a cap.

---

## Q4 — Local inference in Unimatrix (FORWARD-LOOKING — must NOT block crt-057)

A horizon dimension the human wants on record: could Unimatrix run **local inference over the
transcript** to *generate* the feedback/summary automatically, offloading it from agent context and
processing?

- **Delta, not greenfield.** Unimatrix already runs local ML (NLI, GNN, GGUF — Principle 5). Frame
  this as the *delta* to add a summarization/feedback inference: model class, where it runs (review-
  time tool call vs. tick vs. hot path — Principle 7 forbids DB reads on the query hot path), compute
  /latency budget, and the **mandatory graceful-degradation fallback** (Principle 5: absent/failed
  model = previous behavior, never broken behavior).
- **The coupling to Q1.** Local inference may be the *reason* to hold more of Plane B in memory (the
  human's "reason to grab more"). Explore that link — but any "hold/grab more" recommendation stays
  inside the in-memory transient envelope (NG-1), or explicitly flags itself as challenging that
  invariant so it is a conscious human call, never smuggled.
- **Sizing guard:** frame feasibility + cost + fallback only. Do NOT design it. If Q4 turns out large
  it splits into its own follow-on spike; it does not gate crt-057 or the harvest fix.

---

## ★ Headline deliverable — a recommended `context_cycle_review` design

Synthesized from Q1–Q3, **validated against the real bugfix-891 retro (Appendix A)**. It must answer:
- What is the default response (non-destructive summary — inherited from crt-057).
- The opt-in retrieval shape: the scoped `transcript` contract and its purge semantics (from Q3).
- How the design serves the retro's *actual* three-source usage (Plane A + Plane B + GH comments)
  without the leader having to hand-compose provenance the tool should have surfaced.
- What, if anything, from Q4 the design should leave a seam for (without building it).

This artifact is what feeds the crt-057 redesign. Bar: it must be traceable to what the cycle leader
*actually did*, not to a hypothetical retro — the same "prove against a real artifact" discipline we
hold capabilities to.

---

## Non-Goals / Guardrails

- **NG-1 (absolute):** no raw transcript persisted to disk, in any form (#4721/#4850). Any "grab more"
  or "hold more for inference" lives in the in-memory transient envelope or flags itself as
  challenging the invariant.
- **Q4 must not gate crt-057.** Q1+Q2+Q3 unblock the redesign; Q4 is a horizon read feeding a future
  goal/capability decision.
- Not building anything — this maps, researches, and recommends a design. Delivery is a later crt-057
  redesign + delivery session.
- Not changing the 64-cap / 24h TTL backstops or the human merge gate.

---

## Relationship to other transcript work (avoid fragmentation)

There is a transcript-research cluster; delineate cleanly:
- **crt-057 (#894)** — the feature this spike feeds. Provides the non-destructive default + the single
  consumption seam. Its opt-in axis is what ass-091 redesigns.
- **ass-090 (#896)** — *distill transcript signal INTO the summary* (server-side enrichment). Distinct
  thrust, but its Part 1 as-is map is the SAME map as ass-091 Q1. **Recommendation to human:**
  re-sequence ass-090 to depend on ass-091 and consume this spike's authoritative data-plane map
  rather than re-deriving it. ass-091 = *what's the retrieval/design*; ass-090 = *what more to distill
  at the seam*.
- **ass-078 (#751)** fold-at-ingest aggregation, **ass-077 (#741)** transcript value within the
  never-persist constraint — read as prior art; do not re-run.

---

## Approach & Breadth _(confirm time-box with human)_

- **Read-only.** Trace code paths with file:line evidence. Read crt-052/054/055, vnc-024/025, and the
  crt-057 ADRs for the retention/secrets envelope (do NOT store).
- **Breadth:** thorough on Q1 (the authoritative map) and Q3 (the crt-057 blocker); grounded-empirical
  on Q2 (measure against real cycles, lead with the bugfix-891 trace); framing + feasibility on Q4.
- **Grounding entries** (context_get, read-only): #4721, #4850, #4742, #4857, #4750, #5030 (content-
  opaque activity-fold, the model for "derived signal must be content-opaque to persist"), #5417/#3827
  (the bugfix-891 standout entries, as Q2 evidence).

---

## Open Questions to Resolve With the Human Before Running

- Time-box / breadth confirmation.
- Should the recommended design commit to a single opt-in contract, or present 2 ranked options for a
  design-session call?
- Confirm ass-090 re-sequencing (depend-on-ass-091) so the as-is map isn't authored twice.
- Q4 depth: feasibility read only, or also a rough cost/latency envelope from the existing GGUF path?

---

## Appendix A — bugfix-891 retro provenance trace (primary Q2 evidence, captured so it survives)

Real retro-leader behavior, verbatim intent:
- **"57 entries served"** — from `context_cycle_review` Knowledge Reuse section (Plane A, measured).
- **Standout IDs #5417, #3827** — from the investigator's & architect's `## Knowledge Stewardship`
  GH-comment blocks on #891 (the third source). The review's own top-entries table ranked a
  *different* five (#92/#93/#648/#684/#922). Transcript (Plane B) corroborated the two `context_get`
  fetches but at `Reconstructed` fidelity.
- **"shaped/steered the decision"** — leader's own synthesis; no plane asserted the causal link.

Three-tier durability the trace exposed:
1. Live agent return payloads — richest, full-fidelity, **ephemeral** (coordinator-only, gone after session).
2. GH-issue comments (stewardship blocks) — persisted, full-fidelity, readable by the fresh-context retro subagent.
3. `transcript_candidates` — reconstructed, truncated, memory-only.
The retro subagent never had tier 1; it worked off tier 2 + tier 3. "Transcription-only" recovers
~the *what/when* skeleton at high fidelity but degrades on *causal attribution* and *depth* — the
gap the recommended design must close.

## Knowledge Stewardship

- **Queried (carried in):** GH #894/#896/#891; the bugfix-891 retro trace (Appendix A); grounding
  entries listed above.
- **Declined:** Storing anything — spike is read-only in Unimatrix; findings live in FINDINGS.md.
