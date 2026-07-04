# FINDINGS-Q3: Scoped/searchable retrieval feasibility — THE crt-057 blocker

**Spike**: ass-091 (GH #898) · **Date**: 2026-07-04 · **Approach**: read-only, file:line-grounded, reasoned over Q1's bounds · **Confidence**: validated (mechanism); directional (live hit-rate — measurement blocked, see Unanswered)

> Grounded in FINDINGS-Q1.md's data-plane map. Q1 owns the bounds (4 MiB tail-elision, 1 MiB per-frame clip, holes, `elided_bytes`, Primary vs Reconstructed, extract-all-then-purge one-shot). Q3 answers the four retrieval questions and produces the concrete purge-reconciliation answer the crt-057 redesign encodes.

## TL;DR
- **Match survives truncation, but `no-match` is only trustworthy over a clean session.** The existing per-session `SessionLossInfo` (`elided_bytes` / `has_holes` / `Reconstructed` / `dropped_candidates`) is exactly the discriminator: no-match over a session with a loss row = INDETERMINATE (could be past the boundary); no-match over a clean Primary session = "didn't happen within retention." The scoped-match contract must propagate loss, never collapse to a bare boolean.
- **Anchor timestamp-join lands the what/when skeleton reliably** — on two caveats: `TranscriptCandidate.ts` is `Option<String>` (candidates with `ts:None` escape the join) and the anchor clock (`EvidenceRecord.ts`, Plane A epoch millis) differs from the candidate clock (Plane B JSONL string). Use a window, never exact match; fall back to `byte_offset` proximity.
- **★ Purge reconciliation: scoped retrieval purges NOTHING.** Split crt-057's fused opt-in into its two responsibilities — "return transcript" (kept by retrieval, now read-only) and "trigger destructive purge" (handed entirely to the UNCHANGED 24h-TTL + 64-cap + session-close backstops). The buffer's own `snapshot()` is already `&self`. NG-1 untouched; the non-destructive guarantee holds by construction.
- **API: `transcript` omitted = summary only; `match:".*"` + cap = the degenerate "full dump."** Scoped filters layer on top of the EXISTING candidate pipeline (no new buffer reader); phase/anchor/match AND-compose, window modifies anchor/match.

---

## Findings

### Q: Regex/`match` over truncated text — does it survive per-event truncation, or does clipping gut it? When does `no-match` mean "didn't happen" vs. "past the truncation boundary," and must high `elided_bytes` be flagged to the caller?

**Answer**: Match survives, because `match` does NOT run over raw ring bytes — it runs over `TranscriptCandidate.text`, "the whole matched user/assistant block, unwindowed" (`crates/unimatrix-observe/src/types.rs:622-623`). Selection is a three-stage funnel, and truncation bites at stage 1, not at the regex:

1. **Buffer bounds already dropped bytes** before any candidate exists: 4 MiB ring-tail elision (advances `base_offset`, bumps `elided_bytes` — `session_transcript.rs:194-211`), 1 MiB per-frame clip, and holes. The content available to match is only the *contiguous readable run* — `snapshot()` copies from the post-hole floor (`contiguous_run_start_rel`, `session_transcript.rs:322-329`) to span end (`session_transcript.rs:296-316`). Anything below `base_offset` (elided) or before the last hole is **not in the snapshot** and therefore invisible to any regex.
2. **Marker selection** picks whole blocks from what survives.
3. **A `match` regex** filters those candidate blocks — over full block text, so within a retained block clipping does not "gut" the regex; a block is either present whole or absent.

The consequence for `no-match` semantics: **a no-match is only meaningful relative to what was retained.** The discriminator already exists per session and must not be discarded — `SessionLossInfo` (`types.rs:633-646`): a session appears in `loss` whenever ANY of `elided_bytes > 0`, `has_holes`, `provenance == Reconstructed`, or `dropped_candidates > 0` (a clean Primary session with no loss is OMITTED — silence = nothing to report, `types.rs:626-631`). Therefore the rule the caller must apply:

- **no-match over a session with NO loss row (clean Primary)** ⇒ "didn't happen" within the retention envelope. Trustworthy negative.
- **no-match over a session WITH a loss row** ⇒ INDETERMINATE. The string could be past the 4 MiB tail (`elided_bytes > 0`), inside a hole, or absent from the 0.81-fidelity `Reconstructed` skeleton (`reconstruct.rs` floor, Q1 B.2). Not a trustworthy negative.

**Reconstructed provenance is a second, distinct no-match trap.** A `Reconstructed` session's text is a lossy rebuild from observations, not the real stream — a match can miss because the reconstruction dropped the phrasing, not because the agent never said it. Provenance must ride with match results, same as `elided_bytes`.

**Evidence**: `types.rs:605-646` (candidate text is whole-block; SessionLossInfo fields + omission rule); `session_transcript.rs:296-329` (snapshot = contiguous run only, elided/hole bytes excluded); `session_transcript.rs:59,379-380` (`elided_bytes` surfaced); Q1 B.2/B.5.

**Recommendation**: The scoped-`match` contract MUST return, per session, `matched: bool` **plus** a `search_complete: bool` derived from the loss row (`search_complete = false` iff `elided_bytes>0 || has_holes || provenance==Reconstructed`). Surface `elided_bytes` and `provenance` alongside. Do **not** let `match` degrade to a bare boolean — a bare no-match over a lossy session is a silent false negative and is precisely the failure the redesign must prevent. High `elided_bytes` is not "advisory" — it is the flag that converts a negative into an indeterminate.

---

### Q: Finding-anchor timestamp-join — hotspots carry timestamped event clusters (F-03 `Timeline: +90m(1) +95m(2▲)`). Does referencing a finding + window reliably land the right events?

**Answer**: Directionally yes for the what/when skeleton (Appendix A already establishes transcription recovers what/when at high fidelity), via this path, but on two structural caveats that force window-based (never exact) joins.

The join path:
- A finding is `HotspotFinding` with `evidence: Vec<EvidenceRecord>` (`types.rs:48-63`). Each `EvidenceRecord` carries `ts: u64` — "Timestamp of the evidence event (epoch millis)" (`types.rs:35-44`). The `Timeline: +90m/+95m` annotations are these evidence timestamps mapped to phase windows (`types.rs:238` — finding evidence timestamps mapped to phase windows for hotspot annotations).
- Candidates carry `ts: Option<String>` — "Block timestamp from the JSONL record; the primary ordering key" (`types.rs:618-619`) — and the section is ordered by `(ts, session_id, byte_offset)` (`types.rs:655-656`).
- **Anchor(F, window)** = collect the finding's evidence timestamps `{ts_i}`, then select candidates whose `ts` falls in `[min(ts_i) − window, max(ts_i) + window]`.

**Caveat 1 — type/clock mismatch.** `EvidenceRecord.ts` is `u64` epoch millis (Plane A, hook-event clock). `TranscriptCandidate.ts` is `Option<String>` from the JSONL block (Plane B clock). The join must parse the candidate string to epoch and must not assume the two clocks are byte-identical. For **Reconstructed** sessions the candidate `ts` is *derived from the same observations* that produced the evidence, so the clocks coincide (good); for **Primary** sessions they are independent sources and can skew. This is why "finding + window" works but "finding + exact timestamp" does not — the window absorbs the skew.

**Caveat 2 — `ts:None` candidates escape the join.** A candidate whose JSONL record lacked a timestamp has `ts:None` and cannot be placed on the timeline at all; it can only be ordered by `byte_offset`. If a nontrivial fraction of candidates lack `ts`, anchor/phase scoping silently under-selects. The join must fall back to `byte_offset` proximity within the same session for `ts:None` candidates rather than dropping them.

**Evidence**: `types.rs:35-44` (EvidenceRecord.ts u64 epoch millis), `types.rs:48-63` (HotspotFinding.evidence), `types.rs:238` (evidence-ts → phase-window mapping is the Timeline source), `types.rs:615-619,655-656` (candidate byte_offset logical-offset + Option ts + ordering key).

**Recommendation**: Implement anchor scoping as a **windowed** join — `±N events` or `±T minutes` around the finding's evidence-timestamp span, not an exact-timestamp lookup — and select by BOTH the ts-window and a `byte_offset`-proximity fallback for `ts:None` candidates. Return the finding's evidence-ts span in the response so the caller can see the window that was applied. Reliability is high for the what/when skeleton; it does NOT and must not assert causal attribution (that is the Q2 gap the leader hand-composes — the tool returns the slice, not the causation).

---

### Q: ★ Purge reconciliation (LOAD-BEARING). Scoped retrieval breaks the extract-all-then-purge one-shot model. If the caller pulls 6 events, what happens to the buffer — purge the slice, purge all, purge nothing?

**Answer**: **Purge NOTHING. Scoped retrieval is read-only.** This is the concrete answer the redesign encodes.

**Why the other two options are wrong:**

- **Purge the slice (drop only the 6 retrieved events)** — impossible cleanly and unsafe. The buffer is a byte ring, not an event store; `clear()` is all-or-nothing (`session_transcript.rs:349-360` — clears the whole `data` vec, resets `base_offset = high_water`). There is no partial-event deletion primitive, and building one would fragment the ring and violate the offset/hole invariants I1–I5 (`session_transcript.rs:37-46`). Worse, it leaves the *un-retrieved* raw content resident in memory indefinitely, weakening NG-1's guarantee that "in-memory + purge IS the secrets guarantee" (`session_transcript.rs:8`).
- **Purge all (retrieve 6, drop everything)** — preserves the one-shot destructive model but defeats scoped retrieval's entire purpose. A retro that pulls a phase slice, then realizes it needs an adjacent finding's events, cannot come back — the buffer is gone. It collapses every retrieval back into all-or-nothing, which is exactly what the redesign exists to escape.

**The reconciliation — split crt-057's fused opt-in into its two responsibilities.** Today the `include_transcript_candidates` boolean fuses two jobs: (a) "return the transcript" and (b) "trigger the sole one-shot destructive purge." The four cycle-review success returns each run `distill_before_purge` (extract-ALL) then `purge_cycle_transcripts` (drop-ALL), and the merge gate asserts exactly four purge sites, each preceded by distill+attach (`distill_handler.rs:665-687`). The redesign keeps (a) and removes (b) from retrieval:

1. **Retrieval (scoped or full) = read-only.** It calls `snapshot()`, which is already `&self` and non-mutating (`session_transcript.rs:296`), narrows the candidate set by the scoped filters, and returns it. It purges nothing. This is the property that makes crt-057's non-destructive guarantee hold *by construction* — retrieval literally cannot destroy.
2. **Reclamation = the existing, UNCHANGED backstops** the SCOPE forbids touching (line 152): the 24h-TTL stale-sweep (`transcript_hold.rs:18-19`, review-independent), the 64-session hold-cap eviction (`transcript_hold.rs:113`, oldest-first), and per-turn session-close purge. These already exist precisely to bound memory *when purge does not fire* (e.g., a session that never gets a cycle review) — so leaning retrieval on them adds no new memory risk; it uses backstops that are already load-bearing.
3. **The read-before-purge fold stays put.** The content-opaque integer fold (crt-054/055, #5030 — Q1 A.5) is read at the cycle-review seam via `activity_snapshots_for_feature` (`session.rs:566-603`), strictly before any buffer is dropped. It is NOT `force`-reproducible after purge (Q1 A.5). Because retrieval no longer purges, the buffer now *survives* past the review for the TTL window — which strictly *helps* the fold (nothing is lost sooner) and lets a subsequent scoped retrieval still hit the same buffer. The fold read is on the cycle-review path regardless of the retrieval opt-in, so decoupling does not remove it.

**What changes vs. today**: cycle-close no longer eagerly reclaims; buffers live up to 24h / until cap eviction. This is *inside* the envelope — the 24h TTL and 64-cap are exactly the backstops the SCOPE says must not change, and NG-1 (never persist to disk) is untouched: everything stays in the in-memory transient envelope throughout.

**Optional explicit purge.** Preserve the old extract-all-then-purge as a *deliberate* caller act — an explicit `purge: true` (default `false`) for "done with this feature, reclaim now." The default stays non-destructive; the destructive path becomes an explicit choice, never a side effect of asking for transcript.

**Evidence**: `session_transcript.rs:296` (snapshot is `&self`), `:349-360` (clear is all-or-nothing), `:37-46` (I1–I5 ring invariants), `:8` (in-memory+purge = secrets guarantee); `distill_handler.rs:665-687` (four fused distill→purge sites, merge gate); `transcript_hold.rs:18-19,113` (TTL sweep + cap eviction, review-independent); `session.rs:566-603` (fold read-before-purge); Q1 A.5, B.3, B.4; SCOPE lines 101, 145-152.

**Recommendation**: Encode: *"Scoped retrieval purges nothing — it is a read-only `snapshot()` of the scoped slice. Automatic buffer reclamation is delegated entirely to the unchanged 24h-TTL sweep + 64-session cap eviction + per-turn session-close purge. The crt-057 opt-in's two fused responsibilities are split: retrieval keeps 'return transcript'; the backstops own 'reclaim.' An optional explicit `purge: true` (default false) preserves the old extract-all-then-purge as a deliberate act. The content-opaque fold read (crt-054/#5030) stays on the cycle-review path, strictly before any backstop reclamation, so its non-force-reproducible integers are never lost."* This is the single answer that satisfies all three constraints simultaneously: crt-057's non-destructive guarantee, the do-not-change TTL/cap backstops, and NG-1.

---

### Q: API shape — reconcile the proposed block `transcript: { phase?, anchor?, match?, window? }` against Q1/Q2: default (summary only), the opt-in's scoping, and how "full dump" degrades to `match: ".*"` with a cap.

**Answer**: The block works as proposed; make it a set of AND-composed, all-optional filters that narrow the EXISTING candidate pipeline before attach — no new buffer reader is introduced.

| Field | Meaning | Resolution path |
|---|---|---|
| *(`transcript` omitted)* | **Default: summary only.** Plane A only, buffer untouched, non-destructive. Inherits crt-057's non-destructive default. The common case. | `build_report` has no transcript input (Q1 A.3-A.4) |
| `phase?` | Candidates within a phase window | Phase bounds from `cycle_events` (`CycleEventRecord.timestamp`, `event_type == "cycle_phase_end"`, `types.rs:344-350`); join `candidate.ts ∈ [phase_start, phase_end]`. Same ts-parse / `ts:None` caveat as the anchor answer. |
| `anchor?` | A finding id (e.g. F-03) ± window | `HotspotFinding.evidence[].ts` → select candidates in `[min−window, max+window]` (see anchor answer) |
| `match?` | Regex over `TranscriptCandidate.text` | Filter candidate blocks; **MUST return per-session `search_complete` from `SessionLossInfo`** so a no-match is disambiguated (see match answer). Bounded by the per-cycle cap. |
| `window?` | ±N events (or ±T) around anchor/match hits | Modifies `anchor` and `match`; ignored by `phase` (self-bounding) unless used as explicit padding |

**Composition**: `phase` / `anchor` / `match` AND-compose (each narrows the candidate set); `window` modifies `anchor`/`match`. `transcript: {}` present but all-None = the full candidate set under the cap = equivalent to `match: ".*"`.

**Full dump degrades to `match: ".*"` + cap.** `.*` matches every candidate block; the cap bounds the response — reuse the existing per-cycle chronological keep-earliest cap (`distill_handler.rs:222`, Q1 B.4). So "give me everything" is not a separate mode; it is the maximal point of the same scoped contract — still non-destructive, still bounded by the cap that already exists. This folds crt-057's old whole-stream behavior into the scoped contract's degenerate case, which is the clean unification the redesign wants.

**Reconciliation with Q1**: The filters run over the SAME `TranscriptCandidatesSection` that `distill_before_purge` already produces (`types.rs:657-659`), narrowed before `attach_to_response_assembly` (`distill_handler.rs:269`). `snapshot()` already exists (`session_transcript.rs:296`) — no third buffer reader is added (respecting the single-reader invariant, ADR-002). The scoped block is a filter layer, not a new content path.

**Reconciliation with Q2**: The three-source retro (Plane A summary + Plane B transcript + GH `## Knowledge Stewardship` comments) means the `transcript` block owns only Plane B. It must return **candidates + loss**, and must NOT attempt causal attribution — that is the leader-hand-composed gap (Appendix A). Design honesty: the scoped transcript gives the retro the right slice to corroborate against; it never asserts "entry X steered decision Y."

**Evidence**: `types.rs:344-350` (cycle_events phase bounds), `:611-659` (candidate + section types), `:633-646` (SessionLossInfo); `distill_handler.rs:222,269` (cycle cap, attach seam); `session_transcript.rs:296` (existing snapshot reader); Q1 A.3-A.4, B.4; SCOPE lines 102-104, Appendix A.

**Recommendation**: Adopt `transcript: { phase?, anchor?, match?, window? }` as AND-composed optional filters over the existing candidate section. Default (omit) = summary only. `match:".*"` + per-cycle cap = full dump. Every returned session carries its `SessionLossInfo` (`elided_bytes`, `provenance`, `search_complete`, `dropped_candidates`). The block returns candidates and loss only — never a causal claim.

---

## Unanswered Questions

- **Live regex hit-rate over real cycles (SCOPE line 93: "Measure hit-rates over the retained head across real cycles").** Cannot be measured in this read-only spike. Plane B is never persisted (NG-1, #4721/#4850) — no transcript corpus exists to replay, and a running server's buffers are memory-only and purged. A real hit-rate number would require live instrumentation across real cycles (a delivery-time experiment), not a read-only trace. I give the mechanism-level answer definitively (what a no-match means, when it is trustworthy, what must be flagged) — that is what the redesign needs to encode. The empirical rate is directional only. This is the one confidence-boundary in Q3: **validated on mechanism, directional on live rate.**
- **Fraction of candidates with `ts:None`.** The reliability of anchor/phase joins depends on how often JSONL blocks lack a timestamp (those candidates escape the timestamp join). Not measurable read-only for the same reason. Flag for the delivery-time experiment above.

---

## Out-of-Scope Discoveries

- **Cross-plane timestamp-clock skew is a latent correctness risk beyond Q3.** `EvidenceRecord.ts` (u64 epoch millis, Plane A) and `TranscriptCandidate.ts` (Option<String> JSONL, Plane B) are independent clocks for Primary sessions. ANY timestamp-join (phase, anchor, and ass-090's distill-into-summary work) inherits this. Worth a normalization pass — parse candidate `ts` to a canonical epoch at attach time, and record which clock each side used. One-line rationale: prevents silent window-miss across the whole transcript-retrieval cluster, not just Q3. Carry to crt-057 delivery / ass-090.
- **`ts:None` candidates are timeline-invisible.** A candidate without a JSONL timestamp can only be `byte_offset`-ordered, so it silently drops out of any timestamp-scoped selection. If common, it undermines both `phase` and `anchor`. Warrants the measurement flagged above; may warrant deriving a fallback `ts` from the observation stream at reconstruct time. Not a new spike on its own — folds into the crt-057 delivery experiment.

---

## Recommendations Summary

- **Match over truncated text**: match runs over whole candidate blocks and survives clipping; return per-session `search_complete` derived from `SessionLossInfo` so a `no-match` over a lossy/Reconstructed session is INDETERMINATE, not "didn't happen." High `elided_bytes` MUST be flagged — it is the flag that converts a negative into an indeterminate. Never collapse `match` to a bare boolean.
- **Anchor timestamp-join**: implement as a windowed join (±N events / ±T minutes) over the finding's `EvidenceRecord.ts` span, with a `byte_offset`-proximity fallback for `ts:None` candidates; reliable for the what/when skeleton, never for causation, never exact-timestamp.
- **★ Purge reconciliation**: scoped retrieval purges NOTHING — it is a read-only `snapshot()`. Split crt-057's fused opt-in: retrieval keeps "return transcript"; the unchanged 24h-TTL + 64-cap + session-close backstops own "reclaim." Optional explicit `purge:true` (default false) preserves the old extract-all-then-purge. The content-opaque fold read stays on the cycle-review path, strictly before backstop reclamation. Satisfies the non-destructive guarantee, the do-not-change backstops, and NG-1 simultaneously.
- **API shape**: adopt `transcript: { phase?, anchor?, match?, window? }` as AND-composed optional filters over the existing candidate section; omit = summary only; `match:".*"` + per-cycle cap = full dump; every session carries its loss row; returns candidates + loss only, never causation. No new buffer reader — reuses `snapshot()` and the existing distill/attach pipeline.

**Citations**: ~30 file:line across `session_transcript.rs`, `session.rs`, `distill_handler.rs`, `transcript_hold.rs`, `observe/src/types.rs`; grounded on FINDINGS-Q1 bounds (B.2/B.3/B.4/B.5, A.3-A.5) and SCOPE Q3 + guardrails.
