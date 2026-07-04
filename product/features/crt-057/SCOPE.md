# crt-057: Non-Destructive `context_cycle_review` with Scoped, Honest Transcript Retrieval

**Tracking:** GH #894 (promoted from bug #871, now closed)
**Phase:** Cortical (crt) — Learning & drift
**Session type:** design (no IMPLEMENTATION-BRIEF.md exists)
**Contract status:** REOPENED and REWRITTEN by human (2026-07-04) following research spike **ass-091 (#898)**, which returned a materially simpler design. The previously-locked 3-axis boolean contract is **superseded** — see the Changelog note at the end and the API Contract below.
**Design source:** `product/research/ass-091/FINDINGS.md` (headline deliverable + the ★ design note). Q1 data-plane map and Q3 scoped-retrieval mechanism are the load-bearing inputs.

---

## Problem Statement

`context_cycle_review` today does two ungated things on the common path, and its previously-designed
fix (a fused `include_transcript_candidates` boolean that both emits candidates and purges the buffer)
carried avoidable coupling:

1. **Bloat / lost lean default (the #871 symptom).** The `markdown` default was born in vnc-011
   (#196) as the lean, human-digestible response with acceptance criterion **AC#10: ≥80% token
   reduction vs JSON**. crt-052 (#706) bolted an ungated `transcript_candidates` append onto all four
   success returns, pushing the default back to ~75 KB (~88% of the response was raw candidate bytes).

2. **A destructive-by-default review.** The first successful review of a cycle purges the in-memory
   transcript buffers (`purge_cycle_transcripts` at the four success returns, #4750), keyed only on
   `result.is_ok()`. A routine `markdown`-first review distilled the candidates, discarded them, and
   **purged the source permanently** — a later review could only produce degraded `Reconstructed`
   candidates or none.

3. **The fused boolean coupled two unrelated jobs.** The old fix named a single
   `include_transcript_candidates` flag that both *returned* the transcript AND *triggered the purge*.
   ass-091 (Q3, ★ design note) showed these are separable, and that separating them removes the entire
   "what granularity should the purge have?" question that spawned the research: once retrieval is a
   read-only `snapshot()`, the destructive job has no reason to ride on the review at all.

**Who is affected:** the retrospective-harvest pipeline (`/uni-retro` → `context_cycle_review` →
transcript retrieval → agent-curated knowledge), which feeds self-learning (#5219); and any human who
requests a summary and instead receives a bloated dump of raw (possibly secret-bearing) bytes.

**Why now:** ass-091 (#898) resolved the crt-057 blocker (Q3 scoped-retrieval feasibility) and
returned a simpler, honest design. The human reopened the locked contract to adopt it.

---

## The Two Data Planes (foundation — from ass-091 Q1)

The design rests on a sharp line the spike established as the authoritative data-plane map. Do not
re-derive it.

- **Plane A — durable observations.** SQL-persisted hook tool-events are the source of record. The
  review summary (`RetrospectiveReport`) is **100% Plane-A-derived and buffer-independent**:
  `build_report()` takes no transcript argument, so the summary is a pure function of durable SQL and
  is `force`-reproducible byte-for-byte. This is A-1 (re-confirmed by ass-091): the report is buffer-
  independent.
- **Plane B — in-memory `transcript_candidates`.** A per-session byte ring buffer, **never persisted
  to disk** (NG-1; ADR-005 vnc-024/#4721, crt-054/#5030). Bounded: 4 MiB per-session ring-tail
  elision, 1 MiB per-frame clip, 64-session hold cap, 24h TTL, `Primary` vs `Reconstructed` (~0.81
  fidelity floor) provenance. Consumed at exactly one seam.
- **The one durable transcript-derived survivor** is a **content-opaque integer fold**
  (`transcript_bytes_total`, `*_delta_count`, `*_error_count`, `*_refusal_count`,
  `signal_class_counts_json`) on the separate `CycleReviewRecord` (crt-054/#5030) — integers only,
  never prose, not `force`-reproducible once the buffer is gone. Never conflate this fold with Plane B
  raw content.

crt-057 provides exactly two things: **(1) the non-destructive Plane-A observation summary** and
**(2) honest scoped retrieval of Plane B (candidates + loss)**. Nothing that interprets, joins, or
attributes across planes — see the Ownership Boundary below.

---

## Ownership Boundary — Retro Synthesis Is Agent-Owned, Not Unimatrix's (human clarification, 2026-07-04)

The `transcript` retrieval is a **targeted retrieval tool the retro AGENT optionally uses** to capture
*what transpired* during execution. It is complementary to both the Plane-A observation summary AND the
agent's own `## Knowledge Stewardship` GH blocks.

**Unimatrix does NOT, and crt-057 does not build:**
- Synthesizing or joining GH `## Knowledge Stewardship` comment blocks.
- Manufacturing applied-entry attribution (which served entry an agent "applied").
- The rework-count ↔ cause join.
- Surfacing a human-intervention ledger.

All of the "three-source serving" / attribution / human-ledger richness proposed in the ass-091
headline design (§3 of the headline deliverable) is **OUT OF SCOPE**. It is agent-managed retro,
outside Unimatrix and outside this feature. The retro agent owns the synthesis; Unimatrix serves honest
planes. crt-057 exposes exactly one retrieval axis and nothing that interprets or attributes.

---

## Goals

1. **Default review is non-destructive and summarized.** With no `transcript` field, the response is
   the observation-derived report only — nothing from Plane B, buffer untouched, no purge. This is the
   common case (D-1, unchanged intent).
2. **`context_cycle_review` is FULLY non-destructive — no purge verb at all.** Not a flag, not a
   default. The eager review-triggered purge (`purge_cycle_transcripts` at the four success returns,
   #4750) is **REMOVED**. Reclamation is delegated ENTIRELY to the unchanged backstops.
3. **Transcript retrieval becomes a SCOPED, read-only retrieval**, not a boolean:
   `transcript: { phase?, anchor?, match?, window? }` — all-optional, AND-composed filters that narrow
   the EXISTING candidate pipeline via a read-only `snapshot()` (no new buffer reader). Omit
   `transcript` = summary only; `transcript: {}` (present, all-None) = full candidate set under the
   existing per-cycle cap; `match:".*"` ≡ the same degenerate full dump.
4. **Loss propagation — no-match is never a silent false negative.** Every returned session carries its
   `SessionLossInfo`; a `match` no-match over a lossy/`Reconstructed` session is **INDETERMINATE**, not
   "didn't happen."
5. **Clock-skew normalization is a first-class interface requirement.** The agent queries in the terms
   IT knows (finding/anchor id, phase id, regex, a window in events or time); Unimatrix converts and
   normalizes INTERNALLY to the stored Plane-B unit. The agent must NEVER be required to know Plane B's
   storage clock.
6. **Restore vnc-011 AC#10** (≥80% token reduction vs full JSON) for the default response.
7. **`force` and `format` are orthogonal to transcript retrieval.** `force:true` recomputes the report
   from durable observations (never retrieves candidates, and — now trivially — never purges); `format`
   is render-only.
8. **Reconcile every downstream consumer** (D-4) so the candidate-bearing call uses the new
   `transcript{}` block instead of the old boolean, and no doc implies the old behavior. Restructure the
   close-of-cycle lifecycle in both protocols (D-5) so retro runs post-close and can retrieve
   non-destructively.

---

## API Contract (three axes; the third is a scoped retrieval, NOT a destructive one)

`context_cycle_review` exposes three independent parameters. **There is no destructive axis.**

| Axis | Parameter | Semantics |
|------|-----------|-----------|
| **1. Render** | `format: "markdown" \| "json"` (default `markdown`) | **RENDER ONLY.** Chooses serialization of the report. Content is identical either way (the report is buffer-independent). Never affects retrieval, never purges. |
| **2. Recompute** | `force: bool` (default `false`) | Durable recompute of the report from the observation table (bypass memoization). Never retrieves candidates, never purges. |
| **3. Scoped retrieval** | `transcript: { phase?, anchor?, match?, window? }` (optional; omit = none) | **READ-ONLY SCOPED RETRIEVAL** over the existing candidate pipeline via `snapshot()`. Returns candidates + per-session `SessionLossInfo`. **Purges NOTHING.** |

### The `transcript` block

```
transcript: {
  phase?:  <phase id>              // candidates within a phase window (cycle_events bounds)
  anchor?: <finding/anchor id>     // finding evidence-ts span, ± window
  match?:  <regex>                 // over whole TranscriptCandidate.text blocks
  window?: ±N events (or ±T time)  // modifies anchor/match; ignored by self-bounding phase
}
```

- **All optional, AND-composed.** `phase`/`anchor`/`match` each narrow the candidate set; `window`
  modifies `anchor`/`match`.
- **Omit `transcript`** = observation summary only (non-destructive default, unchanged behavior).
- **`transcript: {}`** (present, all-None) = the full candidate set under the existing per-cycle cap ≡
  `match:".*"` — the degenerate full dump, still non-destructive, still bounded by the cap that already
  exists. There is no separate whole-stream mode.
- **Runs over the EXISTING candidate pipeline** — the same `TranscriptCandidatesSection` the seam
  already produces, narrowed before attach. Reuses `snapshot()` (already `&self`); **no new buffer
  reader** (respects the single-reader invariant, ADR-002 #4848).
- **The block owns Plane B only** and never touches summary derivation (the sharp line, Q1).

### Loss propagation (per returned session)

- `matched: bool`
- `search_complete: bool` — `false` iff `elided_bytes > 0 || has_holes || provenance == Reconstructed`.
  A no-match with `search_complete == false` is **INDETERMINATE**, not "didn't happen."
- `elided_bytes` and `provenance` surfaced alongside — high `elided_bytes` (past the 4 MiB tail) and
  `Reconstructed` each independently flag a negative as untrustworthy.
- For `anchor`/`phase`: return the evidence-ts span / phase bounds that defined the window, and fall
  back to `byte_offset` proximity for `ts:None` candidates so they never silently drop out.
- `match` MUST NOT collapse to a bare boolean — a bare no-match over a lossy/`Reconstructed` session is
  exactly the silent false negative this redesign exists to prevent.

### Clock normalization (interface + correctness requirement)

The agent expresses its query in its own units — a finding/anchor id, a phase id, a regex, a window in
events or time. Unimatrix converts and normalizes INTERNALLY to the Plane-B storage unit:

- Plane A `EvidenceRecord.ts` is u64 epoch-millis; Plane B `TranscriptCandidate.ts` is
  `Option<String>` JSONL — independent clocks for `Primary` sessions. Unimatrix parses candidate `ts`
  to a canonical epoch and joins over a WINDOW (never an exact match), handling the epoch-millis ↔ JSONL
  skew server-side.
- `ts:None` candidates fall back to `byte_offset` proximity so they never escape the join silently.
- **The agent must never be required to know Plane B's storage clock.** This is a first-class interface
  requirement (previously only a delivery carry-forward — now promoted, per human 2026-07-04).

---

## Non-Goals

- **NG-1: No persisting raw transcript content to disk in any form.** Absolute
  (#4721 / #4850 / #4742). This work touches only in-memory purge *timing*; no disk/SQL/file/log
  persistence of buffer bytes.
- **NG-2: No change to the 64-session cap or 24h TTL sweep or per-turn session-close purge.** These are
  the backstops reclamation is delegated to; they stay exactly as-is.
- **NG-3: No change to distilled-knowledge / observation / audit retention.**
- **NG-4: No new content secret-scanner / redactor.** Accept-and-drop + in-memory + system-purge IS the
  secrets guarantee.
- **NG-5: No cross-plane synthesis, attribution, or human-ledger surfacing (agent-owned retro).** Per
  the Ownership Boundary above, Unimatrix does not join GH stewardship blocks, does not manufacture
  applied-entry attribution, does not do the rework-count ↔ cause join, and does not surface a
  human-intervention ledger. crt-057 provides the non-destructive summary and honest scoped Plane-B
  retrieval; nothing that interprets or attributes.
- **NG-6: No purge verb on `context_cycle_review`.** No `purge:true` flag, no destructive default. If
  operator-triggered immediate reclamation is ever needed (e.g., a secrets scare), that is a separate
  admin/ops verb, never a parameter on the review tool.
- **NG-7: Distilling transcript signal INTO the review summary is OUT of scope** — deferred to spike
  **ass-090 (#896)**, which is re-sequenced to depend on ass-091 (consuming its Q1 data-plane map, not
  re-deriving it). ass-090 must extend the content-opaque fold (#5030) at the existing seam and must NOT
  touch Plane B raw content.
- **NG-8: No local inference over the transcript in crt-057.** Q4 (local GGUF summarization) is
  feasibility-only and MUST NOT gate crt-057. Leave a seam (the review-time opt-in + the crt-056
  `BackgroundJob` registry #5167); build nothing.

---

## Disk vs Memory Retention Note (read before reviewing)

This feature is frequently misread as a persistence change. It is not.

| Axis | Governed by | This feature's effect |
|------|-------------|----------------------|
| **Raw transcript to DISK** (SQL / file / log) | #4721, #4850, #4742 (absolute, structural) | **UNTOUCHED.** Buffers stay memory-only; retrieval is a read-only `snapshot()`; audits stay content-free. |
| **How long raw transcript lives in MEMORY** | #4857 (an *envelope*, not a minimum: bounded by 64-cap and 24h TTL) | **Longer, always.** Review no longer purges at all, so raw bytes reside in memory until a backstop reclaims them. Bounded by the UNCHANGED cap + TTL + session-close. See the Accepted Residency Trade-off below. |

---

## Accepted Residency Trade-off (settled / human-ratified, 2026-07-04)

Name it; do not smuggle it. Removing the eager review-purge lengthens raw-transcript residency in
memory from **gone-at-review** to **≤24h (TTL) / until 64-cap eviction / until session-close**.

- Still **memory-only**. Still **bounded**. **NG-1 intact** (never touches disk).
- It is a deliberate, human-ratified lengthening of the raw-content window — a risk-posture call, not a
  free consequence.
- It aligns with the Q4 "hold more in memory for later inference" direction: the same residency
  extension pays for both, so a later local-inference decision inherits it at no additional cost.

The system still purges — just on TTL / cap / session-close, not on review. NG-1's "in-memory +
system-purge IS the secrets guarantee" stays intact.

---

## Proposed Approach

1. **Remove the eager review-purge.** Delete the `purge_cycle_transcripts` call at all four success
   returns (#4750). `context_cycle_review` gains zero destructive capability.
2. **Add the scoped `transcript{}` retrieval** over the existing candidate pipeline: AND-composed
   optional `phase`/`anchor`/`match`/`window` filters narrowing the candidate section before attach,
   via read-only `snapshot()`. No new buffer reader.
3. **Attach candidates only when `transcript` is present.** Omit = no attach (summary only), restoring
   the lean default and AC#10.
4. **Emit per-session `SessionLossInfo`** on every returned session, with `matched` /
   `search_complete` / `elided_bytes` / `provenance`, so a no-match over a lossy/`Reconstructed` session
   reads as INDETERMINATE.
5. **Normalize clocks server-side.** Parse candidate `ts` to a canonical epoch, join over a window,
   fall back to `byte_offset` for `ts:None`. The agent never sees Plane B's clock.
6. **Keep the content-opaque fold read (crt-054/#5030) on the review seam.** It is now the ONLY
   remaining success side-effect, gated at all four success returns per the #4750 pattern. Because the
   buffer now survives the review, the fold strictly benefits (nothing lost sooner) and a subsequent
   scoped retrieval can hit the same buffer. Only the *purge* leaves the seam; the fold read does not.
7. **Keep `force` render/recompute orthogonal.** `force` recomputes from durable data; `format` renders
   only. Neither reads nor clears the buffer.
8. **Record an amending ADR** stating the purge removal and the residency posture plainly.
9. **Reconcile consumers (D-4) and restructure the retro lifecycle (D-5).**

---

## In-Scope Deliverables

- **D-1 (server): scoped `transcript{}` retrieval + non-destructive default.** Thread the optional
  `transcript` block from the handler through the existing candidate pipeline; narrow via
  `phase`/`anchor`/`match`/`window` over `snapshot()`; attach only when the block is present. Keep
  `format` render-only and resolve the dead `"summary"` alias (drop or fold to markdown — architect
  detail). Preserve the #4750 four-site lockstep and the `distill_handler.rs` source-assertion tests.

- **D-2 (server): remove the eager review-purge; `context_cycle_review` is fully non-destructive.**
  Delete `purge_cycle_transcripts` from all four success returns. No purge verb anywhere on the tool.
  Reclamation is delegated to the unchanged 24h TTL sweep, 64-session hold-cap eviction, and per-turn
  session-close purge. The content-opaque fold read stays, gated at the four returns, as the only
  remaining success side-effect.

- **D-3 (server): loss propagation.** Every returned session carries `SessionLossInfo`; `match` returns
  per session `matched`, `search_complete` (false iff `elided_bytes>0 || has_holes ||
  provenance==Reconstructed`), plus `elided_bytes` and `provenance`. `anchor`/`phase` return the
  window/bounds that defined them and fall back to `byte_offset` proximity for `ts:None` candidates.

- **D-4 (server): clock normalization.** Convert agent-supplied query units (finding/anchor id, phase
  id, regex, event/time window) to the Plane-B storage unit internally; parse candidate `ts` to a
  canonical epoch; join over a window; `byte_offset` fallback for `ts:None`. The agent never supplies or
  sees Plane B's clock.

- **D-5 (ADR): amending ADR** for the purge removal and residency posture, amending #4742 and #4857.
  States: (a) the eager review-triggered purge is removed and `context_cycle_review` is fully
  non-destructive with no purge verb; (b) the residency change — raw transcript now resides in memory
  until a backstop reclaims it (TTL / cap / session-close), bounded by the UNCHANGED backstops;
  (c) disk posture unchanged (NG-1); (d) `force`/`format` orthogonal to retrieval; (e) the content-
  opaque fold read remains the sole review-seam side-effect. Human-ratified residency change.

- **D-6 (consumer reconciliation — D-4 consumer set, same feature):** the atomic unit is **server
  (D-1..D-4) + `.claude/skills/uni-retro/SKILL.md` + the `context_cycle_review` tool description + BOTH
  protocol files (`uni-delivery-protocol.md`, `uni-bugfix-protocol.md`)**. `uni-agent-routing.md` is NOT
  an active consumer and is excluded.
  - `uni-retro/SKILL.md` — the candidate-bearing call now uses the `transcript{}` block (not the old
    boolean); the retro agent owns the synthesis (Ownership Boundary). The protocol simply invokes
    `/uni-retro` post-close.
  - The `context_cycle_review` tool description — document the three axes: `format` render-only; `force`
    durable recompute (never retrieves, never purges); `transcript{}` read-only scoped retrieval
    returning candidates + `SessionLossInfo`, purging nothing. State plainly that the tool has no purge
    verb.

- **D-7 (protocol retro-lifecycle restructure — BOTH protocols, D-5 amendment):** applies to
  `uni-delivery-protocol.md` AND `uni-bugfix-protocol.md`. A distinct pr-review / bug-review phase stays
  OPEN through the human merge decision; the cycle is closed only AFTER merge
  (`context_cycle` phase-end then stop); `/uni-retro` runs post-merge, after cycle-close. Strict
  ordering: **merge → close cycle → retro**. The human merge gate is unchanged.
  - **This gets SIMPLER under the redesign:** because both review and close are now fully non-
    destructive, close → retro trivially preserves candidates. The retro can retrieve non-destructively
    as often as it wants (no one-shot to sequence around). The prior contract's "accepted trade-off"
    about buffers aging out before a late retro extraction is softened — the only residual exposure is
    the same TTL/cap aging every buffer already has, unrelated to review ordering.

- **D-8 (tests):** matrix must cover —
  - **default** (no `transcript`, markdown): NO candidates block, **no purge** (buffer intact).
  - **json render**: identical content to markdown, NO candidates block, **no purge**.
  - **force:true (no `transcript`)**: report recomputed from durable observations AND reproducible, NO
    candidates block, **no purge** — orthogonality.
  - **`transcript:{}`** (full dump): full candidate set under the per-cycle cap, **no purge**; second
    identical call returns the same candidates (buffer survived) until a backstop reclaims.
  - **`transcript:{ match:"<regex>" }`**: narrowed candidate set; a no-match over a session with
    `search_complete==false` reports INDETERMINATE (not a bare false).
  - **`transcript:{ anchor:<id>, window:±N }`** and **`transcript:{ phase:<id> }`**: window/bounds
    returned; `ts:None` candidates included via `byte_offset` fallback.
  - **clock normalization**: an agent-clock query resolves against skewed Plane-B `ts` correctly.
  - **no purge verb**: no parameter or path on `context_cycle_review` purges the buffer; reclamation
    only via TTL/cap/session-close.
  - **AC#10**: ≥80% token reduction of the default response vs full JSON — measured assertion.
  - **memo-hit path** honors the `transcript` block identically to the full-pipeline path (all four
    sites).

---

## Acceptance Criteria

- **AC-01:** The default response (no `transcript` field, `markdown`) contains NO `transcript_candidates`
  block.
- **AC-02:** With `transcript` present, the response contains a candidate section scoped by the supplied
  `phase`/`anchor`/`match`/`window` filters (absent — not null — when the scope yields nothing).
- **AC-03 (non-destructive review):** `context_cycle_review` NEVER purges the transcript buffer on any
  path or parameter combination. There is no purge verb. A second, identical review returns the same
  candidates (buffer survived) until a backstop reclaims. The eager review-triggered purge
  (`purge_cycle_transcripts`) is removed from all four success returns.
- **AC-04 (fold read preserved):** The content-opaque fold read (crt-054/#5030) still runs, gated at all
  four success returns per #4750, as the sole remaining review-seam side-effect. No candidate/buffer
  content reaches any SQL/file/log write.
- **AC-05 (`transcript:{}` full dump):** `transcript:{}` (present, all-None) returns the full candidate
  set under the existing per-cycle cap, equivalent to `match:".*"`, non-destructively.
- **AC-06 (indeterminate no-match):** For `match`, each returned session reports `matched`,
  `search_complete` (false iff `elided_bytes>0 || has_holes || provenance==Reconstructed`),
  `elided_bytes`, and `provenance`. A no-match over a session with `search_complete==false` is reported
  as INDETERMINATE, never a bare false negative.
- **AC-07 (anchor/phase windowing + `ts:None`):** `anchor`/`phase` return the evidence-ts span / phase
  bounds that defined the window and include `ts:None` candidates via `byte_offset` proximity fallback,
  so no candidate silently drops out of a windowed query.
- **AC-08 (clock normalization):** An agent expressing its query in its own units (finding/anchor id,
  phase id, regex, event/time window) resolves correctly against skewed Plane-B `ts` without the agent
  supplying or knowing Plane B's storage clock; candidate `ts` is normalized to a canonical epoch
  server-side and `ts:None` uses `byte_offset` fallback.
- **AC-09 (force orthogonality):** `force:true` is always accepted, performs a report-only recompute
  from durable observations, and NEVER retrieves candidates and NEVER purges. The report is reproducible
  regardless of buffer state.
- **AC-10 (restored vnc-011 AC#10):** The default response achieves ≥80% token reduction versus the full
  JSON candidate-bearing response for a typical review — asserted by a measured test.
- **AC-11 (format render-only):** `format:"json"` renders identical report content to `markdown` — no
  candidates, no purge; the two differ only in serialization.
- **AC-12 (four-site lockstep):** The `transcript` gate and the fold-read gate apply identically at all
  four success returns; `distill_handler.rs` source-assertion tests pass (or are updated with recorded
  rationale). No per-site forking.
- **AC-13 (backstops unchanged):** The 64-cap, 24h TTL sweep, and per-turn session-close purge are
  unchanged and are the sole reclamation path. No new cycle-close purge trigger is added.
- **AC-14 (secrets posture):** Candidates stay response-transient, outside the memoized report; the
  persisted `RetrospectiveReport` gains no candidate slot; the scoped retrieval creates no new
  persistence path.
- **AC-15 (ADR):** An amending ADR is stored (amends #4742, #4857) recording the purge removal, the
  fully-non-destructive review, the residency posture change, and the disk-posture-unchanged statement.
- **AC-16 (consumer reconciliation):** `uni-retro/SKILL.md` and the `context_cycle_review` tool
  description use the `transcript{}` block (not the old boolean) and imply no purge-on-review or
  any-review-carries-candidates behavior. `uni-agent-routing.md` is excluded.
- **AC-17 (both protocols, lifecycle restructure):** BOTH `uni-delivery-protocol.md` AND
  `uni-bugfix-protocol.md` implement cycle-close-after-merge followed by `/uni-retro`, ordering
  **merge → close cycle → retro**; the retro retrieves non-destructively (no one-shot sequencing). The
  human merge gate is unchanged.

---

## Constraints

- **C-1: Four-site lockstep (#4750).** The `transcript` gate and the surviving fold read apply
  identically at all four success returns via the shared helper; keep the `distill_handler.rs`
  source-assertion tests passing (or update deliberately with rationale). Purge is removed from all
  four in lockstep.
- **C-2: Single buffer reader (ADR-002 #4848).** Scoped retrieval reuses the existing `snapshot()`
  (`&self`); no new buffer reader.
- **C-3: Secrets posture (#4850 / #4721 / #4750) invariant.** Candidates stay response-transient,
  outside the memoized report; no buffer content reaches SQL/file/log. The scoped-retrieval path creates
  no new persistence.
- **C-4: Backstops untouched (#4857).** 64-cap, 24h TTL, and session-close are the sole reclamation and
  are unchanged. They now carry the full reclamation load previously shared with the eager review-purge.
- **C-5: Exhaustive `TranscriptRetention` match.** Where the retention enum is still matched (the fold /
  any residual), keep it exhaustive; `RetainDays` stays a no-op; no `_` arm.
- **C-6: Consumer + server change ship together as one atomic unit** — server (D-1..D-4) +
  `uni-retro/SKILL.md` + tool description + BOTH protocol files. Shipping the server change without D-6/
  D-7 leaves the retro calling the old boolean and starves the harvest (#5219).
- **C-7: 500-line file limit / fmt / clippy.** Extend existing `distill_handler.rs` test fixtures; no
  isolated scaffolding.
- **C-8: Rebase awareness.** Prior work touched `distill_handler.rs`; confirm no live conflict before
  delivery.

---

## Dependencies

- **ass-091 (#898)** — the design source. Q1 data-plane map (authoritative; do not re-derive), Q2
  consumer demand, Q3 scoped-retrieval mechanism (the crt-057 blocker, resolved), and the ★ headline
  design + ★ non-destructive design note are consumed directly by this scope.
- **crt-052** (#706) — transcript_hold / candidates pipeline. Amends its purge clause (ADR-008 #4857).
- **crt-054/#5030** — content-opaque integer fold at the review seam (the sole surviving side-effect;
  must remain correct now that the review never purges).
- **vnc-024 / vnc-025** — retention enum + purge-point audit. Amends vnc-025 ADR-004 (#4742).
- **vnc-011** (#196) — origin of `format`/markdown and AC#10.
- **ass-090 (#896)** — downstream spike, re-sequenced to DEPEND on ass-091; consumes the Q1 map and
  extends the fold (#5030). Does not touch Plane B raw content.
- **crt-056 `BackgroundJob` registry (#5167)** — the documented seam for a future async
  transcript-summary job (Q4). Left open; built by no one here.
- Stored ADRs to amend: **#4742**, **#4857** (via D-5). Anchors (unchanged): **#4721**, **#4850**.
  Pattern: **#4750**.
- **Superseded:** the prior crt-057 ADRs authored for the boolean contract (#5429 ADR-001, #5422 ADR-002,
  #5423 ADR-003, #5424 ADR-004) will be reworked/deprecated during the design phase to match this scope.

---

## Open Questions

- **OQ-1 (live regex hit-rate / `ts:None` fraction — deferred, not blocking).** ass-091 established the
  *mechanism* definitively (what a no-match means, when trustworthy, what must be flagged) but the
  empirical live regex hit-rate and the fraction of `ts:None` candidates are unmeasurable read-only
  (Plane B is never persisted). These fold into a delivery-time instrumentation experiment; they do not
  block design. The correctness contract (loss propagation, indeterminate no-match, `byte_offset`
  fallback) holds regardless of the rate.
- **OQ-2 (window default sizing).** The default `window` magnitude for `anchor`/`match` (±N events or ±T
  time) needs an architect/spec choice, informed by the cross-plane skew envelope. Not a blocker — a
  conservative default plus caller override suffices.
- **OQ-3 (memo-hit interaction).** Confirm the memo-hit success return honors the `transcript` block
  identically to the full-pipeline path across all four sites. Likely mechanical; covered in D-8.

---

## Out of Scope / Carry-Forward (noted, not scoped into crt-057)

- **Q4 local inference (leave a seam only).** The review-time opt-in already carried by the `transcript`
  block + the crt-056 `BackgroundJob` registry (#5167) are the documented seam for a future
  `TranscriptSummaryJob`. Build nothing. Any generated summary would be a strictly-additive
  `Option`/skip-if-none section with a hard fallback to today's observation-derived review. A separate
  Q4 measurement spike (latency/footprint on target hardware; summary quality from `Reconstructed`
  input) is warranted and does NOT gate crt-057.
- **ass-090 re-sequenced to depend on ass-091** (#896) — consumes the Q1 map, extends the fold, must not
  touch Plane B raw content.
- **Human-intervention-ledger durability gap** — all durable sources miss the human's own decision
  rationale; a possible separate spike (collides with NG-1 if it tries to persist raw conversation).
  crt-057 does not own closing it (Ownership Boundary / NG-5).
- **`TODO(W2-4): gguf_rayon_pool` anti-stub** (`main.rs:871,1593`, `services/mod.rs:264`) — pre-existing
  anti-stub tension (CLAUDE.md rule 2); flag for cleanup or an issue, independent of crt-057.
- **GNN named in Principle 5 but unimplemented** (reserved fields only) — vision-doc reconciliation flag
  so future spikes don't over-assume shipping ML breadth. Independent of crt-057.

---

## Knowledge Stewardship

- **Queried:**
  - `mcp__unimatrix__context_briefing` (crt-057) — surfaced #4750 (four-site side-effect pattern), #4850
    (candidates outside the memoized struct), #5031 (crt-054 survival-to-review obligation), #5429/#5422/
    #5423/#5424 (the prior boolean-contract crt-057 ADRs — to be reworked), #4848 (single content
    reader), #4799 (per-session registry read-at-review caveat).
  - Read: `product/research/ass-091/FINDINGS.md` (headline deliverable + ★ non-destructive design note +
    Q1/Q3), the prior boolean-contract SCOPE.md (superseded).
- **Stored:** nothing — read-only in Unimatrix per this task. No generalizable pattern emerged beyond
  those the spike already grounds; the design source is ass-091 FINDINGS + this SCOPE.
