## ADR-001: `context_cycle_review` Is Fully Non-Destructive — Eager Review-Purge REMOVED, Reclamation Delegated Entirely to the Unchanged Backstops (amends #4742, #4857)

Feature: crt-057 · GH #894 · Amends: vnc-025 ADR-004 (#4742), crt-052 ADR-008 (#4857)
Anchors unchanged: vnc-024 ADR-005 (#4721), crt-052 ADR-004 (#4850)
Reworked 2026-07-04 after ass-091 (#898) + human re-scope: the prior "purge trigger redefined to the
`include_transcript_candidates` flag" decision is superseded — there is now **no purge verb at all**.

### Context

`purge_cycle_transcripts` (`server.rs:661`) fired on **any** successful `context_cycle_review` under
`TranscriptRetention::PurgeOnCycleClose`, keyed only on `result.is_ok()` at each of the four success
returns (#4750). A routine `markdown`-first review distilled candidates, discarded them, and
**permanently purged the only source** — buffers are memory-only, never persisted (crt-052 AC-06).

The prior crt-057 contract made extraction an opt-in boolean and fired purge iff the flag was true.
ass-091 (Q3, ★ design note) showed the boolean fused two separable jobs — *return the transcript* and
*reclaim the buffer* — and that once retrieval is a read-only `snapshot()`, the reclaim job has no
reason to ride on the review. The human adopted the simpler design: **the review carries zero
destructive capability**. This still refines the "purge at review" clause of **#4742** (purge points /
content-free audit) and **#4857** (held-buffer store; cap + TTL) — now by *removing* the review as a
purge trigger entirely, not by gating it — so both stored decisions must be amended (SR-11) or they
stay internally contradictory with shipped behavior. This is not a disk/persistence change; it changes
in-memory purge *timing* only, and the human has ratified the residency change by adopting the design.

### Decision

Remove the review purge and state the residency posture plainly. Four points (all required, SR-11):

**(a) No purge verb — the review is fully non-destructive.** Delete the `purge_cycle_transcripts`
call at all four success returns (`tools.rs:2379, 2558, 3328, 3451`). `context_cycle_review` gains
**zero** destructive capability — not a default, not a flag, not an opt-in. The default response, a
`json`-rendered response, a `force:true` recompute, and any `transcript{}` scoped retrieval **all**
leave the buffer intact. A second identical review returns the same candidates. Verified: those four
were the ONLY non-test callers of `purge_cycle_transcripts`; removing them **orphans** the function
and its helpers `clear_transcripts_for_feature` / `purge_held_for_feature`, which delivery deletes
(anti-stub / dead-code, CLAUDE.md rule 2). If operator-triggered immediate reclamation is ever needed
(e.g. a secrets scare), that is a **separate admin/ops verb** authored then, never a parameter on the
review tool (NG-6).

**(b) Memory-residency change (the plain statement).** Raw transcript now resides in memory **longer
on every path**: no review purges, so buffer bytes reside until a backstop reclaims them. Bounded by
the **UNCHANGED** backstops from #4857 — the held-count cap (`transcript_hold_max_sessions`, 64), the
independent 24h stale-sweep TTL (`transcript_hold_ttl_secs`, `sweep_expired`), and per-turn
session-close purge, all review-independent. **Worst-case resident volume = up to 64 held buffers ×
per-buffer cap bytes, for up to the 24h TTL** — behaviorally identical to "no review has run", which
#4857 already budgets and bounds. Steady-state resident volume rises versus the old
purge-at-every-review behavior; it does not become unbounded. This is a deliberate, human-ratified
lengthening of the raw-content window, and it also pays for the Q4 "hold more in memory for later
inference" direction at no additional cost.

**(c) Disk posture UNCHANGED.** #4721 / #4850 remain absolute: buffers stay memory-only; candidates
stay response-transient, attached at assembly level OUTSIDE the memoized `RetrospectiveReport` (which
gains no candidate slot); no buffer or candidate content reaches any SQL / file / log write. The
read-only scoped-retrieval path creates no new persistence. `RetainDays` stays rejected in OSS
(#4721). The **exhaustive `TranscriptRetention` match** (C-5) that lived inside
`purge_cycle_transcripts` relocates with the deletion: the surviving backstop reclaim paths must honor
retention exhaustively (`RetainDays` a no-op, no `_` arm). Backstop reclamation still emits the
content-free terminal audit (`transcript_session_purged`, `trigger=stale_sweep` / cap-eviction,
bytes-only) — the audit trail is preserved; it simply fires only at the backstop now (SR-02).

**(d) Orthogonality.** `force` and `format` are orthogonal to transcription and to each other:
`force` recomputes the report from durable observations; `format` is render-only; `transcript{}` is a
read-only Plane-B snapshot. None purges. All compose with no precedence.

### Consequences

Easier: the default review is non-destructive and lean (vnc-011 AC#10 restored); the one-shot
ordering footgun is **fully dissolved** — a review can never destroy the source, so retrieval is
repeatable and the retro may re-run in any scope; the residency envelope is explicit and
human-ratified; the retro-lifecycle reordering (ADR-005) becomes trivially safe because no reaper sits
anywhere on the review→close→retro path.

Harder: raw bytes reside in memory longer on every path (bounded by the unchanged cap+TTL); the
content-free purge audit now fires only at the backstop, so audit-trail readers must not assume "purge
audit ⇒ a review occurred"; a cycle whose retro is delayed past 24h TTL or evicted past the 64-cap
loses verbatim candidates (degraded `Reconstructed`/empty) — not a regression (backstops unchanged)
but now the sole loss mode, surfaced via loss propagation (ADR-003); the exhaustive-match obligation
(C-5) must be re-homed on the backstops as `purge_cycle_transcripts` is deleted.

Amendment mechanism: #4742 and #4857 are amended via `context_correct` (not deprecate+store), so the
still-valid clauses (audit shape; held-buffer store; cap/TTL; loud re-adoption) carry forward with the
purge-timing clause updated to "review never purges; reclamation is backstops-only", and provenance is
preserved through the correction chain. This crt-057 ADR is the authoritative amendment record.

Cross-refs: #4742, #4857 (amended); #4721, #4850 (anchors, unchanged); #4750 (four-site pattern);
ADR-002 (API surface), ADR-004 (fold-read-only seam gating), ADR-005 (retro lifecycle),
ADR-006 (scoped-retrieval mechanism).
