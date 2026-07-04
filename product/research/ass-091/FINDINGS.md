# FINDINGS: Cycle-Review Data Planes, Retro Consumer Demand, and a Recommended `context_cycle_review` Design

**Spike**: ass-091 (GH #898)
**Date**: 2026-07-04
**Approach**: read-only investigation (synthesis of four tracks: Q1 data-plane map, Q2 consumer demand, Q3 scoped-retrieval feasibility, Q4 local-inference feasibility)
**Confidence**: validated on Q1 / Q2 / Q3-mechanism and the headline design; directional on Q3 live-hit-rate and Q4
**Mode**: read-only in Unimatrix — stores nothing. Track files (FINDINGS-Q1..Q4.md) remain the audit trail; this file distills, it does not replace them.

> This is the headline deliverable. It answers all four SCOPE Goal questions and produces a single recommended `context_cycle_review` design, validated against what the bugfix-891 retro leader actually did (SCOPE Appendix A). Per the human's confirmed scope decisions: it commits to a SINGLE opt-in contract (alternatives noted inline only at genuine forks); it confirms the ass-090 re-sequence; Q4 stays feasibility-only and does NOT gate crt-057.

---

## Findings

### Q: Q1 (FOUNDATION) — The two data planes, articulated precisely

**Answer**: Two distinct substrates sit behind the cycle review, with a sharp line between them, plus a third de-facto source that is neither plane.

- **Plane A — durable observations.** Hook tool-events persisted to SQL are the source of record. The cycle-review summary (`RetrospectiveReport`) is **100% Plane-A-derived and buffer-independent**: `build_report()` takes no transcript argument (`report.rs:15-22`), so the summary is a pure function of durable SQL and is `force`-reproducible byte-for-byte. Plane A is a *union* of durable substrates — observation rows, `cycle_events` (phase timeline), `SessionRecord`s (rework ratio), `query_log ∪ injection_log` (served-knowledge count), historical `MetricVector`s (baseline), and the cached `CycleReviewRecord`. Every summary field maps to one of these (Q1 A.3).
- **Plane B — in-memory `transcript_candidates`.** A per-session byte ring buffer, **never persisted to disk** (NG-1; ADR-005 vnc-024/#4721, crt-054/#5030), held past session close. Bounded: 4 MiB per-session ring-tail elision, 1 MiB per-frame clip, 64-session hold cap, 24h TTL, plus `Primary` vs `Reconstructed` (~0.81 fidelity floor) provenance. Consumed at **exactly one seam** — `take_transcripts_for_feature` → `distill_before_purge` — where the server does *mechanical* select/reconstruct/attach and hands candidates + loss accounting out-of-band; semantic distillation happens OUTSIDE the server, in the retro agent.
- **The sharp line.** Review prose reads *nothing* from Plane B; the downstream retro flow is Plane B's only consumer. The one transcript-derived durable survivor is a **content-opaque integer fold** (`transcript_bytes_total`, `*_delta_count`, `*_error_count`, `*_refusal_count`, `signal_class_counts_json`) landed read-before-purge onto the *separate* `CycleReviewRecord` — integers only, never prose, and **not** `force`-reproducible after purge (ADR-005 crt-054/#5030). Do not conflate this fold (durable integers) with Plane B raw content (memory-only). This is the divergence trap ass-090 must respect.
- **Third de-facto source.** Persisted GH-issue `## Knowledge Stewardship` comment blocks — durable, full-fidelity prose, readable by a fresh-context retro subagent, but neither plane. The retro's real provenance runs across all three.

**Evidence**: FINDINGS-Q1.md — ~70 file:line citations across `report.rs`, `review_aggregates.rs`, `session_transcript.rs`, `session.rs`, `distill_handler.rs`, `transcript_hold.rs`, `config.rs`; grounding entries #4721, #4850, #4742, #4857, #4750, #5030.

**Recommendation**: Treat FINDINGS-Q1.md as the ONE authoritative data-plane map for the whole transcript-research cluster (crt-057 delivery, ass-090). Do not re-derive it. Encode the sharp line — summary prose ⟂ Plane B content — as a design invariant: the scoped `transcript` block owns Plane B only and never touches summary derivation.

**Confidence**: validated (file:line-grounded throughout).

---

### Q: Q2 — Consumer demand: how a retro agent actually wants to use these planes

**Answer**: Four retro needs map to three sources with one clean split, proven against the real bugfix-891 retro (a *messy 8-phase multi-agent rework* cycle — the hard case, not a clean one-shot):

| Retro need | Served by | Where it falls short |
|---|---|---|
| **Counts** (entries served, sessions, rework count) | **Plane A** (measured, `force`-reproducible) | **Salience-blind.** "57 entries served" is a true aggregate, but the summary's top-entries table ranked #92/#93/#648/#684/#922 by frequency/recency — **none** of the entries that actually drove the fix. |
| **Causal attribution** (entry X → decision Y) | **GH stewardship comments** (primary); Plane A partial; Plane B corroboration only | **No plane holds the causal edge.** The (#5417 → "capture must stay unbounded"), (#3827 → "intra-block placement is legal") edges exist *only* because the investigator and design-reviewer hand-wrote them into stewardship prose. Plane A proves served + happened, not *caused*; Plane B shows a `context_get` fetch at Reconstructed ~0.81 fidelity, asserting no "because". |
| **Rework-why** | **Split: Plane A (count) + GH (why); nothing joins them** | `rework_session_count` gives "1 rework iteration" and *when* (cycle_events); the *why* ("Rust tests updated, Python integration layer missed") lives in a *different* GH comment. The join is manual. |
| **Human-intervention ledger** | **Nothing durable — ephemeral tier-1 only, GH second-hand** | The human's ≥2 load-bearing calls (direction sign-off; retire-vs-build → deferred to #895) emit no tool-event, so Plane A misses them; Plane B might hold in-session text but truncated/Reconstructed/purged; GH records the *outcome* second-hand, never the human's reasoning. A genuine hole all durable sources share. |

**What the leader hand-composed (the design target).** Decomposed from the real trace into two distinct artifacts:
- **(a) Salience re-ranking** — elevating #5417/#3827 (read off stewardship prose "directly shaped the proposed fix") over the summary's mechanical top-5. The tool ranks by served-*frequency*; the retro needs ranking by *applied-causality*.
- **(b) The cross-source causal arc** — joining per-agent local attributions (GH) + rework count (Plane A) + rework-why (a different GH comment) + the human decision (tier-1/second-hand GH) into one cycle-level "why it went this way" narrative, across sources that share no key.

**Hypothesis verdict.** *"GH-comment discipline, not the transcript, makes retros robust; transcript is the fallback."* — **substantially TRUE for the causal/knowledge-reuse axis, but conditional and better restated as complementary orthogonal axes.** It held on the hard messy-rework case, but: (1) only because the **stewardship gate enforces** block presence — remove enforcement and the transcript becomes the sole witness for any thinned phase; (2) it **flips on the human-intervention axis** — neither prose nor transcript is authoritative there; (3) "fallback" mis-frames the transcript, which is *primary* for the **what/when temporal skeleton** (a different axis), not a lower-fidelity prose substitute. Restated: durable prose = primary for *why/salience* (robust iff gate-enforced); transcript = primary for the *what/when skeleton* (complementary); both blind to the human ledger.

**Evidence**: FINDINGS-Q2.md — every claim traced to the GH #891 comment record (8 phase stewardship blocks + gate PASS-check) or the Q1 file:line map; #5417/#3827 roles quoted verbatim from stewardship prose and confirmed via read-only `context_get`.

**Recommendation**: The design's target is automating (a) and (b) — surface applied-entry attribution keyed to phase and to the served count, and expose the rework iteration pre-joined to its cause-comment, so salience-by-causality and the cross-source arc become a *query, not a compose*. The tool cannot manufacture the human-ledger content (genuinely absent) but MUST make the absence explicit rather than let the leader silently paper over it.

**Confidence**: validated (grounded-empirical against a real retro).

---

### Q: Q3 — Scoped/searchable retrieval feasibility (THE crt-057 blocker)

**Answer**: Scoped retrieval is feasible over the existing candidate pipeline with no new buffer reader, on four concrete findings:

1. **Match survives truncation; `no-match` is only trustworthy over a clean session.** `match` runs over `TranscriptCandidate.text` (whole matched block, unwindowed), not raw ring bytes — truncation bites at selection (stage 1), not the regex, so a block is present whole or absent. Therefore a no-match over a session with **no** `SessionLossInfo` row (clean `Primary`) = "didn't happen within retention" (trustworthy negative); a no-match over a session **with** a loss row (`elided_bytes>0` / `has_holes` / `Reconstructed` / `dropped_candidates>0`) = **INDETERMINATE**. High `elided_bytes` is not advisory — it is the flag that converts a negative into an indeterminate.
2. **Anchor timestamp-join lands the what/when skeleton reliably, via a WINDOW never an exact match.** A finding's `HotspotFinding.evidence[].ts` (u64 epoch millis, Plane A clock) selects candidates whose `ts` (Option<String>, Plane B clock) falls in `[min−window, max+window]`. Two caveats force windowing: cross-plane clock skew for `Primary` sessions, and `ts:None` candidates that escape the timestamp join (fall back to `byte_offset` proximity). Reliable for what/when; never for causation.
3. **★ Purge reconciliation: scoped retrieval purges NOTHING.** (See the headline design below for the full rationale.) Split crt-057's fused opt-in into its two responsibilities — "return transcript" (kept by retrieval, now read-only `snapshot()`, already `&self`) and "trigger destructive purge" (handed entirely to the UNCHANGED 24h-TTL + 64-cap + session-close backstops). Purge-the-slice is impossible cleanly (byte ring, `clear()` is all-or-nothing, leaves un-retrieved content resident — weakens NG-1); purge-all defeats the entire purpose. Optional explicit `purge:true` (default `false`) preserves the old extract-all-then-purge as a deliberate caller act.
4. **API `transcript: { phase?, anchor?, match?, window? }`** works as proposed: AND-composed all-optional filters narrowing the existing candidate section before attach. Omit = summary only (default, non-destructive). `match:".*"` + the existing per-cycle cap = the degenerate "full dump" — folding crt-057's old whole-stream behavior into the scoped contract's maximal case. Every returned session carries its `SessionLossInfo`; the block returns candidates + loss only, never a causal claim.

**Evidence**: FINDINGS-Q3.md — ~30 file:line across `session_transcript.rs` (snapshot `&self` :296; clear all-or-nothing :349-360; I1–I5 invariants :37-46), `distill_handler.rs` (four fused distill→purge sites + merge gate :665-687; cycle cap :222; attach :269), `transcript_hold.rs` (TTL sweep :18-19, cap eviction :113), `observe/src/types.rs` (candidate + SessionLossInfo :605-659); grounded on Q1 bounds.

**Recommendation**: Adopt all four findings verbatim into the crt-057 redesign. The load-bearing one — scoped retrieval purges nothing, reclamation delegated to the unchanged backstops — is the single answer that satisfies crt-057's non-destructive guarantee, the do-not-change TTL/cap backstops, and NG-1 simultaneously.

**Confidence**: validated on mechanism; directional on the live regex hit-rate and `ts:None` fraction (unmeasurable in a read-only spike — Plane B is never persisted; see Unanswered).

---

### Q: Q4 — Local inference over the transcript (FORWARD-LOOKING; must NOT gate crt-057)

**Answer**: Feasible as a *delta* on existing machinery, but the delta is larger than the SCOPE framing implies and MUST be an opt-in enrichment seam with a hard fallback to today's behavior — never a dependency the review path relies on.

- **Grounding correction (resizes the delta).** SCOPE says Unimatrix "already runs local ML (NLI, GNN, GGUF)." Shipping reality: **NLI + sentence-embedding ship via ONNX Runtime (not GGUF)**; **GGUF generative LLM is NOT in the shipping crates** — it exists only as the validated ass-035/036 research harness (Phi-3-mini-4k q4_k_m via llama-cpp-2); **GNN is unimplemented** (reserved fields only). So the honest delta is: promote the GGUF harness into a wired provider (net-new generative path), reusing the ONNX/NLI lifecycle, rayon pool, and graceful-degradation patterns that already ship.
- **Model class / placement.** A small quantized summarization LLM (GGUF). Runs **review-time (opt-in, per the Q3 `transcript` block), NEVER the query hot path** (Principle 7 bars DB-heavy work on `context_search`/`context_get`). Must run on its **own** rayon lane (the anticipatory `TODO(W2-4): gguf_rayon_pool` exists precisely for this) — never the shared ML pool serving request-path NLI. The crt-056 `BackgroundJob` registry (#5167) is the alternative async home, left as a documented seam.
- **Mandatory fallback (Principle 5, non-negotiable).** Copy the NLI `nli_handle.rs` state-machine (`Loading/Ready/Failed/Retrying`, `get_provider()` gate) + the crt-028 structural `Option`/skip-if-none attach. Absent/failed model = today's 100%-observation-derived review, unchanged — crt-057's core already guarantees this (`build_report` is buffer-independent).
- **Cost envelope (order-of-magnitude only; human declined a purpose-built measurement).** ~3 GB resident while loaded (Phi-3-mini q4 weights + ~768 MB KV cache); latency ~single-digit-to-low-tens of seconds for a few-hundred-token summary — comfortable inside a 120s background ceiling, marginal against a 30s review-time ceiling for large transcripts. **Memory (the ~3 GB footprint), not latency, is the binding constraint.**
- **Plane B coupling ("reason to grab more").** A background summarizer runs after the review returns, so the transcript must survive past the purge point. This stays INSIDE the NG-1 in-memory transient envelope IF it reads from the existing `TranscriptHold` (never new persistence; worst case 4 MiB × 64 = ~256 MiB) — holding longer in memory is not persistence. "Grab more fidelity" = raise the 4 MiB / 64-session caps; cost scales as their product plus the ~3 GB model. **The one thing that WOULD breach NG-1/Principle 8** — persisting the transcript to disk to survive a restart before deferred inference — is flagged explicitly and requires a conscious human decision, never smuggled.

**Evidence**: FINDINGS-Q4.md — `cross_encoder.rs:71`, `onnx.rs`, `nli_handle.rs:66,160,280,366`, `rayon_pool.rs:146,211`, `timeout.rs:16`, `distill_handler.rs:48,669`, `background/jobs.rs:327`, `background.rs:418`, `server.rs:661`, `transcript_hold.rs:155`, `config.rs:1854,1858,1871`, `session_transcript.rs:1,26`; ass-035 harness `gguf.rs`; Principles 5/7/8 (`PRODUCT-VISION.md:62,66`); grounding #5167, #3983, #3335, #4850.

**Recommendation**: Q4 warrants its own follow-on **measurement** spike (empirical confidence) and does NOT gate crt-057. The two make-or-break questions — measured latency/footprint on target hardware, and summary quality from truncated/`Reconstructed` input — are measurement work outside this feasibility read. The crt-057 redesign should only *leave a seam* (the opt-in `transcript` contract + the crt-056 BackgroundJob registry), building nothing.

**Confidence**: directional (feasibility read; measurement explicitly deferred).

---

## ★ HEADLINE DELIVERABLE — Recommended `context_cycle_review` design

Synthesized from Q1–Q3, validated against the real bugfix-891 retro (SCOPE Appendix A). This is the artifact that feeds the crt-057 redesign. It commits to a **single** contract; the one genuine fork (explicit-purge flag) is noted inline.

### 1. The default response — non-destructive observation summary (inherited from crt-057, Q1)

`context_cycle_review` with **no `transcript` field** returns the observation-derived `RetrospectiveReport` and **nothing from Plane B**. This is 100% Plane-A-derived, buffer-independent, and `force`-reproducible (`build_report` has no transcript input — Q1 A.3-A.4). The buffer is untouched; the response is byte-identical whether or not a transcript buffer exists. This is the common case and crt-057's non-destructive-default core, unchanged.

### 2. The opt-in retrieval shape — scoped `transcript` block + its purge semantics (Q3)

```
transcript: {
  phase?:  <phase id>            // candidates within a phase window (cycle_events bounds)
  anchor?: <finding id> ± window // finding evidence-ts span ± window
  match?:  <regex>               // over whole TranscriptCandidate.text blocks
  window?: ±N events (or ±T)     // modifies anchor/match; ignored by self-bounding phase
}
```

- **All optional, AND-composed.** `phase`/`anchor`/`match` each narrow the candidate set; `window` modifies `anchor`/`match`. `transcript: {}` (present, all-None) = the full candidate set under the existing per-cycle cap ≡ `match: ".*"` — the degenerate "full dump," still non-destructive and still bounded by the cap that already exists. There is no separate whole-stream mode.
- **Runs over the EXISTING candidate pipeline** — the same `TranscriptCandidatesSection` `distill_before_purge` already produces, narrowed before `attach_to_response_assembly`. Reuses `snapshot()` (already `&self`); **no new buffer reader** (respects the single-reader invariant, ADR-002).
- **★ Purge semantics — scoped retrieval purges NOTHING.** Split crt-057's fused `include_transcript_candidates` boolean into its two responsibilities:
  - **Retrieval keeps "return transcript"** — a read-only `snapshot()` of the scoped slice. It literally cannot destroy; the non-destructive guarantee holds *by construction*.
  - **Reclamation is delegated ENTIRELY to the UNCHANGED backstops** the SCOPE forbids touching — the 24h-TTL stale-sweep (review-independent), the 64-session hold-cap eviction (oldest-first), and per-turn session-close purge. These already bound memory when purge does not fire, so leaning retrieval on them adds no new memory risk. What changes vs. today: cycle-close no longer eagerly reclaims; buffers live up to 24h / until cap eviction — strictly *inside* the envelope, NG-1 untouched.
  - **No purge verb at all (human decision, 2026-07-04).** The spike originally floated an optional explicit `purge: true` escape hatch. That is **dropped**: a destructive purge on a review has no natural caller — a retro agent never wants to destroy the buffer, so the flag would be dead surface. `context_cycle_review` therefore carries **zero** destructive capability. If operator-triggered immediate reclamation is ever needed (e.g. a secrets scare), that is a *separate admin/ops verb*, never a parameter on the review tool.
  - **The content-opaque fold read (crt-054/#5030) stays on the cycle-review path**, strictly before any backstop reclamation. Because retrieval no longer purges, the buffer now survives past the review, which strictly *helps* the fold (nothing lost sooner) and lets a subsequent scoped retrieval hit the same buffer.

### 3. Serving the retro's ACTUAL three-source usage (Q2), against the bugfix-891 trace concretely

The design must serve Plane A summary + Plane B scoped transcript + GH stewardship comments *without* the leader hand-composing provenance the tool should surface. Against the real trace:

- **The #5417/#3827 salience re-rank.** The summary's top-entries table ranked #92/#93/#648/#684/#922 (frequency/recency); the causally load-bearing entries were #5417/#3827, knowable only from stewardship prose. **The tool cannot invent the causal edge** (no plane holds it — Q2), but it CAN stop making the leader fight the summary: surface applied-entry attribution **keyed to phase and to the served-entry count**, sourced from the GH `## Knowledge Stewardship` blocks, so "which served entry an agent said it applied" is a field, not a manual read. Where the tool cannot assert causality, it returns the scoped Plane B slice (`anchor` on the capture-design finding) so the retro can corroborate the `context_get(#5417)` fetch at its true `Reconstructed` fidelity — never dressing corroboration up as causation.
- **The rework-count ↔ cause join.** Plane A gives `rework_session_count = 1` and the REWORK→PASS timeline; the *why* ("Rust tests updated, Python integration layer missed") is a different GH comment. The design exposes the rework iteration **pre-joined to its cause-comment**, so the leader stops hand-stitching count (A) to cause (GH).
- **The human-ledger absence made explicit.** The retire-vs-build and direction calls survive only second-hand (deferred to #895). The tool MUST NOT fabricate this; it MUST **surface the absence explicitly** — mark the human-intervention ledger as "no durable source" — so a no-match there reads as a known hole, not a silent omission the leader papers over.

### 4. Per-session loss propagation — no-match is never a silent false negative (Q3)

Every returned session carries its `SessionLossInfo`, and the `match` contract returns per session:
- `matched: bool`
- `search_complete: bool` — derived as `false` iff `elided_bytes > 0 || has_holes || provenance == Reconstructed`. A no-match with `search_complete == false` is **INDETERMINATE**, not "didn't happen."
- `elided_bytes` and `provenance` surfaced alongside — so high `elided_bytes` (past the 4 MiB tail) and `Reconstructed` (lossy rebuild) each independently flag a negative as untrustworthy.
- For `anchor`/`phase`: return the evidence-ts span / phase bounds that defined the window, and fall back to `byte_offset` proximity for `ts:None` candidates so they never silently drop out.

`match` MUST NOT collapse to a bare boolean — a bare no-match over a lossy/Reconstructed session is exactly the silent false negative the redesign exists to prevent.

### 5. The Q4 seam to LEAVE (without building it)

Leave — do not build — a single opt-in inference seam:
- The **review-time opt-in** already carried by the `transcript` block is the natural invocation point (transcript guaranteed present, cost paid by the caller who asked).
- The **crt-056 `BackgroundJob` registry** (#5167) is the documented seam for a later async/pre-compute variant ("register a job, don't re-architect the loop") — a future `TranscriptSummaryJob` beside `GraphInferenceJob`.
- Any generated summary is a strictly-additive `Option`/skip-if-none section with a hard fallback to today's observation-derived review (Principle 5). Building it is a separate measurement spike; it must NOT gate crt-057.

### Design one-liner

Default = observation summary (Plane A, non-destructive, force-reproducible). Opt-in `transcript: { phase?, anchor?, match?, window? }` = read-only scoped `snapshot()` of Plane B, returns candidates + `SessionLossInfo` (never causation), purges nothing — reclamation stays with the unchanged 24h-TTL/64-cap/session-close backstops; **no purge verb on the tool at all**. Applied-entry attribution + rework-cause join sourced from GH stewardship blocks; human ledger surfaced as explicit absence. Q4 inference is a left-open seam, built by no one here.

---

### ★ Design note — the review is non-destructive; reclamation belongs to the backstops (human decision, 2026-07-04)

The load-bearing simplification the crt-057 redesign should inherit directly:

**`context_cycle_review` is a fully non-destructive, read-only operation. It has no purge capability — not a default, not an opt-in flag.** The eager, review-triggered purge (`purge_cycle_transcripts` at the four success returns, #4750) is **removed**. It existed only because crt-057's boolean fused two unrelated jobs — "return the transcript" and "reclaim the buffer." Once retrieval is a read-only `snapshot()`, the reclaim job has no reason to ride on the review.

**Reclamation belongs entirely to the three existing backstops**, which fire independently of review cadence and already bound memory when no review runs at all:
- 24h TTL stale-sweep (`transcript_hold.rs:18-19`)
- 64-session hold-cap eviction (`transcript_hold.rs:113`)
- per-turn session-close purge

These are the backstops the SCOPE forbids changing — so this design *uses* them rather than adding anything. NG-1's "in-memory + purge IS the secrets guarantee" stays intact: the *system* still purges, just on TTL/cap/session-close, not on review.

**What stays on the review seam:** exactly one success-side-effect — the content-opaque fold read (crt-054/#5030), which is read-before-purge and *not* force-reproducible once the buffer is gone. It must still be gated at all four success returns per the #4750 pattern. Only the *purge* moves off the review; the fold read does not.

**The one conscious tradeoff (name it, don't smuggle it):** dropping eager purge lengthens raw-transcript residency in memory from *gone-at-review* to *≤24h* (TTL) / until cap eviction. Still memory-only, still bounded, still NG-1-compliant (never touches disk) — but it is a deliberate lengthening of the raw-content window, and it is a human risk-posture call, not a free consequence. It aligns with Q4's "hold more in memory for inference" — the same residency extension pays for both, so a later local-inference decision inherits it at no additional cost.

**Net for crt-057:** the redesign is *simpler* than the original boolean. There is no destructive trigger to grant a granularity to, so the entire "what granularity should the purge be?" question that spawned this spike dissolves. The tool exposes one axis — an optional read-only scoped `transcript` retrieval — and nothing destructive at all.

---

## Unanswered Questions

Merged across all four tracks (deduplicated):

- **Live regex hit-rate over real cycles** (Q3; SCOPE line 93). Unmeasurable in a read-only spike — Plane B is never persisted (NG-1, #4721/#4850), so no corpus exists to replay and a running server's buffers are memory-only/purged. The mechanism-level answer (what a no-match means, when trustworthy, what must be flagged) is given definitively; the empirical rate is directional. A real number requires live delivery-time instrumentation. **This is Q3's one confidence boundary: validated on mechanism, directional on live rate.**
- **Fraction of candidates with `ts:None`** (Q3). Anchor/phase join reliability depends on how often JSONL blocks lack a timestamp; those candidates escape the timestamp join. Not measurable read-only — folds into the same delivery-time experiment.
- **Actual measured latency / tokens-per-sec / footprint on target hardware** (Q4). The human declined a purpose-built measurement; the ass-035 harness numbers are an order-of-magnitude envelope only. Closing this needs a follow-on spike that runs the harness.
- **Summary quality / fidelity floor from `Reconstructed` (0.81) transcript input** (Q4). Whether a small quantized model produces a review-grade summary from truncated/Reconstructed input is unmeasured — the actual make-or-break question for Q4, and a measurement spike, not a feasibility read.
- *(Q2: none — all three sub-questions answered against the real bugfix-891 trace. The mechanism for surfacing applied-entry attribution keyed to phase/count was handed to Q3/headline and is resolved in the design above.)*

---

## Out-of-Scope Discoveries

Merged and deduplicated; carry-forwards preserved.

- **Human-intervention-ledger durability gap (possible new spike).** All three durable sources miss the human's own decision rationale; it lives only in ephemeral tier-1 conversation (bugfix-891's retire-vs-build call is the concrete instance). Neither this spike nor crt-057 owns closing it. Rationale: self-learning (#5219) that cannot see *why the human overrode* is blind to the highest-signal decisions in a cycle. Any solution that tries to persist raw conversation collides with NG-1 — so this needs its own scoped spike, not an ad-hoc fix. The headline design surfaces the absence explicitly as an interim measure.
- **ass-090 re-sequence (confirmed recommendation).** ass-090 (distill-signal-into-summary) should be re-sequenced to **depend on ass-091** and consume this spike's authoritative Q1 data-plane map rather than re-deriving it. ass-091 = *what's the retrieval/design*; ass-090 = *what more to distill at the seam*. Concretely, ass-090 should extend the content-opaque fold (#5030) at the existing seam and must NOT touch Plane B raw content. A candidate ass-090 signal: an *applied-attribution* salience signal sourced from stewardship prose (not served-frequency) — but per #5030 it must land as a content-opaque derived signal, never raw text.
- **Q4 measurement spike (follow-on).** Feasibility is positive and the fallback is clean, but measured latency/footprint on target hardware and summary quality from Reconstructed input are empirical-confidence measurement work. Recommend a dedicated spike running the ass-035 harness against a real transcript corpus before any delivery commitment. Does NOT gate crt-057.
- **Cross-plane timestamp-clock skew (latent correctness risk).** `EvidenceRecord.ts` (u64 epoch millis, Plane A) and `TranscriptCandidate.ts` (Option<String> JSONL, Plane B) are independent clocks for `Primary` sessions. ANY timestamp-join (phase, anchor, and ass-090's distill-into-summary work) inherits this. Worth a normalization pass — parse candidate `ts` to a canonical epoch at attach time and record which clock each side used. Carry to crt-057 delivery / ass-090. (Related: `ts:None` candidates are timeline-invisible — folds into the crt-057 delivery experiment, not a new spike.)
- **`TODO(W2-4): gguf_rayon_pool` placeholders** (`main.rs:871,1593`, `services/mod.rs:264`) are a pre-existing anti-stub tension (CLAUDE.md rule 2). They encode real intent (a separate generative pool lane) but have sat unimplemented — flag for cleanup or an issue, independent of Q4.
- **GNN named in Principle 5 but unimplemented** (reserved W3-1 fields only). The vision text lists a capability that does not exist; may warrant a vision-doc reconciliation so future spikes don't over-assume shipping ML breadth (the same trap the Q4 grounding correction caught).

---

## Recommendations Summary

*(Self-contained — this section is posted to GH #898.)*

- **Q1 — data planes:** Adopt FINDINGS-Q1.md as the ONE authoritative data-plane map for the transcript cluster; do not re-derive. Plane A (durable observations) is the source of record and the summary is 100% Plane-A-derived / buffer-independent / force-reproducible. Plane B (`transcript_candidates`) is in-memory-only, never persisted, consumed at exactly one seam. The only transcript-derived durable survivor is a content-opaque **integer fold** on a separate record — never conflate it with Plane B raw content. Third de-facto source = persisted GH `## Knowledge Stewardship` comments.
- **Q2 — consumer demand:** Counts → Plane A (but its top-entries table ranks by frequency, NOT salience — do not use it as the salience ranking). Causal attribution → GH stewardship comments (the only source that holds the edge); make the stewardship gate a hard precondition of retro robustness. Rework-why → expose Plane A count pre-joined to its GH cause-comment. Human-intervention ledger → served by nothing durable; surface the absence explicitly. The two hand-composed artifacts (salience re-rank by applied-causality; cross-source causal arc) are the headline design target. Adopt the restated hypothesis: durable prose = primary for why/salience (robust iff gate-enforced); transcript = primary for the what/when skeleton (complementary, not fallback); both blind to the human ledger.
- **Q3 — scoped retrieval:** `match` survives truncation (runs over whole candidate blocks); return per-session `search_complete` from `SessionLossInfo` so a no-match over a lossy/Reconstructed session is INDETERMINATE, never a silent false negative. Anchor join = windowed (±N/±T) over the finding's evidence-ts span with `byte_offset` fallback for `ts:None`; reliable for what/when, never for causation. **★ Scoped retrieval purges NOTHING** — read-only `snapshot()`; split crt-057's fused opt-in so reclamation is delegated entirely to the unchanged 24h-TTL + 64-cap + session-close backstops. **`context_cycle_review` carries no purge verb at all** (human decision 2026-07-04 — a destructive purge on a review has no natural caller); the eager review-triggered purge is removed and the review becomes fully non-destructive. API: `transcript: { phase?, anchor?, match?, window? }`, AND-composed optional filters over the existing candidate pipeline (no new reader); omit = summary only; `match:".*"` + cap = full dump; returns candidates + loss only.
- **Q4 — local inference (feasibility only; does NOT gate crt-057):** Feasible as a delta — promote the ass-035/036 GGUF harness (Phi-3-mini-4k q4_k_m) into a wired provider on its OWN rayon lane, run review-time (opt-in via the Q3 block), never the query hot path. Mandatory Principle-5 fallback: absent/failed model = today's observation-derived review, unchanged. ~3 GB resident is the binding cost; memory not latency is the constraint. Read from the existing in-memory `TranscriptHold` (never new persistence); any disk-persist-to-survive-restart scheme is an explicit NG-1/Principle-8 breach requiring a conscious human call. Warrants its own measurement spike.
- **★ Headline design:** Default = non-destructive observation summary (Plane A). Opt-in `transcript: { phase?, anchor?, match?, window? }` = read-only scoped `snapshot()` of Plane B returning candidates + `SessionLossInfo` (never causation), purging nothing; reclamation stays with the unchanged backstops; the review is fully non-destructive with no purge verb at all. Serves the three-source retro by surfacing applied-entry attribution (keyed to phase + served count) and the rework-count↔cause join from GH stewardship blocks, and by making the human-ledger absence explicit — validated against the bugfix-891 #5417/#3827 re-rank and the Rust/Python rework-cause join the leader stitched by hand. Loss propagation (`search_complete`/`elided_bytes`/`provenance`) guarantees no silent false negatives. Leaves the Q4 inference seam (review-time opt-in + crt-056 BackgroundJob registry) open, built by no one here. This is what the crt-057 redesign consumes.
- **Scope decisions confirmed:** single `context_cycle_review` contract recommended, **fully non-destructive with no purge verb** (human decision 2026-07-04 — the earlier optional-`purge:true` fork is dropped as dead surface; reclamation belongs entirely to the unchanged TTL/cap/session-close backstops — see the ★ Design note in the headline deliverable); **ass-090 re-sequenced to depend on ass-091**; **Q4 stays feasibility-only and MUST NOT gate crt-057** (its non-destructive-default core is unaffected — only the opt-in axis is redesigned).
