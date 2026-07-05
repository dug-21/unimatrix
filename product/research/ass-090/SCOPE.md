# ass-090 (SPIKE): In-Loop Distillation of Transcript Signal into the Cycle-Review Summary

> **DRAFT — NOT READY TO RUN.** This scope is a starting point to be completed
> **interactively with the human** (uni-zero session) before any research session is dispatched.
> Sections marked _(to complete with human)_ need the human's framing/priorities first.
> Per protocol, Phase 1 scope completion is interactive; a research session begins only when
> this SCOPE.md is complete.

**Tracking:** GH #896
**Capability:** SL6 (#5225)
**Phase:** Assimilate (ass) — research spike
**Depends on:** crt-057 (#894) — see "Relationship to crt-057" below. crt-057 must define the
buffer's single consumption point before this spike explores what more to distill there.
**Mode:** read-only investigation. Do NOT store anything in Unimatrix. Produces FINDINGS.md.

---

## Human's Explicit Priority for This Spike

**Document HOW transcript signal is used EXACTLY TODAY.** Lead the spike with a precise,
file:line-grounded map of the current flow before exploring anything forward-looking. The human
wants the "as-is" pinned down first: what actually reads the transcript buffer, what it produces,
who consumes it, and — critically — the sharp line between what the cycle review itself consumes
(nothing transcript-derived) and what the downstream retro flow consumes (the raw candidates).

---

## Problem Statement (draft)

The retrospective harvest turns a completed cycle's raw session transcripts into curated
knowledge. Today the transcript buffer feeds exactly ONE product: the crt-052
`transcript_candidates` payload — raw verbatim excerpts handed to a downstream retro architect for
distillation OUTSIDE the server. The cycle-review summary itself derives **nothing** from
transcript content (verified in crt-057 §B; posted to #894). This spike asks whether there is
value in distilling transcript signal **in-loop** — at the moment the buffer is consumed — to
enrich the review or the harvest, and if so, what is feasible without reintroducing the ~62 KB
bloat, blocking a hot path, or crossing the raw-never-persists retention line.

This is exploratory. The spike does not commit to building anything; it maps the current flow,
frames the forward questions, and reports feasibility with recommendations.

---

## Part 1 (PRIORITY): Precise map of the current transcript-signal flow

Produce a file:line-grounded end-to-end map. Anchor points already located during crt-057
research (verify and extend):

```
raw transcript bytes (client transcript_delta)
  │  streamed into per-session in-memory ring buffer (never persisted to disk — #4721)
  ▼
TranscriptBuffer  (crates/unimatrix-server/src/infra/session_transcript.rs)
  │  ring-tail bound (4 MiB default); held past session close in transcript_hold (crt-052 Wave B)
  ▼
take_transcripts_for_feature  (crates/unimatrix-server/src/infra/session.rs:502-541)
  │  registered ∪ held buffers for the cycle; snapshot (Arc-clone + byte copy), NOT a drain
  ▼
distill_before_purge  (crates/unimatrix-server/src/mcp/distill_handler.rs:48-140)
  │  per session: select_candidates (Primary) OR reconstruct_from_observations (Reconstructed
  │  fallback, distill_handler.rs:80) via fallback_triggered predicate (:150-176)
  │  → TranscriptCandidatesSection { candidates, loss }
  ▼
attach_to_response_assembly  (distill_handler.rs:281-298)
  │  appends "\ntranscript_candidates: {json}" as an out-of-band Content item on the
  │  CallToolResult — NEVER onto the memoized RetrospectiveReport (crt-052 ADR-004, secrets)
  ▼
context_cycle_review MCP response  (four success returns, tools.rs — pattern #4750)
  ▼
uni-retro SKILL / retro architect  (.claude/skills/uni-retro/SKILL.md — refs at lines 43, 47,
  │  102, 215, 299; also uni-agent-routing.md:156-159)
  │  consumes transcript_candidates, distills them into knowledge entries
  ▼
stored knowledge  (context_store — patterns / procedures / lessons / ADRs)
```

**Deliverables for Part 1 (must be file:line-grounded):**
1. **Buffer lifecycle**: how bytes enter the buffer, the ring-tail bound, the held-buffer
   lifecycle (crt-052 Wave B), and every reclamation path (cycle-review purge, 64-cap eviction,
   24h TTL sweep, session-close/stale sweep, readopt-mismatch drop) with triggers.
2. **The single content consumer**: confirm `take_transcripts_for_feature` (`session.rs:502`) has
   exactly one non-test caller (`distill_before_purge`) that reads buffer CONTENT, and document
   what it produces (`TranscriptCandidatesSection`: Primary vs Reconstructed provenance, loss
   accounting, per-session and per-cycle caps).
3. **The content-opaque metrics reader**: document `activity_snapshots_for_feature`
   (`session.rs:566`) / `land_activity_fold` (`activity_fold_handler.rs`) reading
   `ActivitySnapshot` (`transcript_activity.rs:106`) — counters only (`bytes_total`,
   `delta_count`, `class_counts`), NO content, persisted durably to `cycle_review_index`. This is
   the ONLY transcript-derived signal that touches the summary today, and it is opaque.
4. **The sharp line (make crystal clear):**
   - **The cycle review / RetrospectiveReport consumes NOTHING transcript-derived** for its prose
     (`build_report`, `report.rs:15-53`, takes only observation records / metrics / hotspots /
     baseline / entries_analysis). The report is byte-identical whether or not the buffer is
     present.
   - **The downstream retro flow** (uni-retro skill → architect) is the ONLY consumer of raw
     transcript content, via `transcript_candidates`, and distillation happens OUTSIDE the server.
5. **The retention line at each hop**: mark where raw-never-persists applies (buffer → candidates
   are memory-only / response-transient, #4721 / #4850) vs where distilled output MAY persist
   (the retro architect's stored knowledge entries — sanitized, non-verbatim).

---

## Part 2 (forward exploration — frame, do not pre-decide) _(refine with human)_

Once the as-is map is pinned, frame (not answer) the exploration questions:

- **EQ-1 — What is distillable in-loop?** Beyond raw excerpts handed off for external
  distillation, is there transcript-derived signal worth computing server-side at consumption time
  (e.g. friction markers, decision moments, error/refusal clustering already partly captured by
  the crt-054/055 activity fold)? What would enrich the summary or the harvest?
- **EQ-2 — Feasibility without 62 KB bloat / hot-path block.** Any in-loop distillation must not
  reintroduce the #871 bloat into a human-facing summary, and must not block a hot path.
  `distill_before_purge` already runs at review time (an explicit tool call, not a background
  tick) — is that the right seam, and what is the compute/latency budget? Where does the 62 KB go
  if it is distilled rather than dumped?
- **EQ-3 — Retention line: raw-never-persists vs distilled-may-persist (#4721).** Distilled,
  sanitized signal MAY persist (like existing observations/knowledge); raw transcript MAY NOT
  (absolute, #4721 / #4850 / #4742). Where exactly does any proposed in-loop distillation sit, and
  how is the boundary enforced structurally (as crt-052 ADR-004 did for candidates)? No content
  redactor exists — any persisted output must be content-opaque or provably sanitized.
- **EQ-4 — Relationship to crt-057's contract.** crt-057 makes the expanded review the single,
  one-shot consumption point for the buffer. Any in-loop distillation ass-090 proposes attaches at
  THAT point (or the default review's content-opaque fold). Explore how in-loop distillation
  composes with: the one-shot purge, the non-destructive default, the detail-level gate (OQ-1),
  and the completeness caveat (crt-057 OQ-2 — no durable roster of expected sessions).
- **EQ-5 — Completeness / fidelity.** What fidelity is achievable given buffer bounds (4 MiB
  ring-tail elision, holes, Primary-vs-Reconstructed fallback)? crt-057 §C established completeness
  is only partially detectable (live-session signal + per-session loss; no expected-session
  roster). What does that mean for the trustworthiness of in-loop distilled signal?

---

## Non-Goals (draft)

- Not building the in-loop distillation — this is a spike; it maps and recommends.
- Not changing crt-057's contract (default non-destructive / expanded one-shot / purge trigger) —
  ass-090 builds ON it.
- Not persisting raw transcript to disk in any form (#4721 absolute) — out of bounds for any
  proposal.
- Not changing the 64-cap / 24h TTL backstops or the disk-retention posture.

---

## Approach & Breadth (draft) _(confirm with human)_

- **Read-only.** Trace the code paths above with file:line evidence; read the crt-052 / crt-054 /
  crt-055 / vnc-024 / vnc-025 ADRs for the retention and secrets envelope (do NOT store).
- **Breadth:** medium-to-thorough on Part 1 (the as-is map is the human's priority and must be
  exhaustive and exact); framing-only on Part 2 (enumerate options and feasibility, recommend, do
  not decide).
- **Grounding entries** (context_get, read-only): #4721, #4850, #4742, #4857, #4750, and the
  crt-054 content-opaque activity-fold ADR (#5030) — the model for "derived signal must be
  content-opaque to persist."

---

## Dependencies

- **crt-057 (#894)** — must land (or at least fix the consumption contract) first: it defines the
  single, well-defined buffer consumption point (expanded one-shot review) that ass-090's forward
  exploration attaches to. Ordering dividend: crt-057 = *when/by whom the buffer is consumed*;
  ass-090 = *what more to distill at that moment*.
- Retention/secrets envelope: #4721, #4850, #4742, #4857 (absolute constraints on any proposal).
- Capability SL6 (#5225).

---

## Open Questions to Resolve With the Human Before Running _(to complete with human)_

- Confirm the spike's output boundary: pure as-is map + forward framing, or also a recommended
  direction / go-no-go on in-loop distillation?
- Confirm breadth and time-box.
- Confirm whether ass-090 should also survey what the retro architect currently does with the
  candidates externally (to identify what could move in-loop) or treat that as out of scope.
- Any specific transcript-derived signals the human already has in mind for EQ-1?

---

## Knowledge Stewardship

- **Queried (crt-057 research, carried in):** GH #894 / #871; `context_get` #4721, #4850, #4742,
  #4857, #4750; code paths listed in Part 1.
- **Declined:** Storing anything — spike is read-only in Unimatrix; findings will live in
  FINDINGS.md when the research session runs.
