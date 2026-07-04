# FINDINGS: Q4 — Local Inference Over the Transcript (Horizon Track)

**Spike**: ass-091 · **Date**: 2026-07-04 · **Approach**: framing + feasibility (forward-looking; MUST NOT gate crt-057) · **Confidence**: directional

Scope of this deliverable: **Q4 only** (SCOPE L108-124). Feasibility read, not a design.

---

## Findings

### Q: Could Unimatrix run local inference over the transcript to *generate* the feedback/summary automatically, offloading it from agent context and processing?

**Answer (headline):** Feasible as a *delta* on machinery that already exists, but the delta is **larger than the SCOPE framing implies** and must be built as an **opt-in enrichment seam with a hard fallback to today's behavior** — never a dependency the review/retro path relies on.

**Grounding correction (changes the size of the delta).** SCOPE says Unimatrix "already runs local ML (NLI, GNN, GGUF)." Principle 5 (`product/PRODUCT-VISION.md:62`) does *name* all three, but the shipping reality differs:

- **NLI + sentence-embedding — SHIPPING**, via ONNX Runtime (`ort` crate), **not** GGUF. NLI cross-encoder `crates/unimatrix-embed/src/cross_encoder.rs:71` (`NliProvider`, `Mutex<Session>`); embedding `crates/unimatrix-embed/src/onnx.rs`. Both lazy-loaded, rayon-dispatched, degrade to cosine-only when absent.
- **GGUF (generative LLM) — NOT in the shipping crates.** Exists only as a validated research harness: `product/research/ass-035/harness/src/gguf.rs` (llama-cpp-2 v0.1.141, Phi-3-mini-4k q4_k_m). Workspace has anticipatory TODOs only (`crates/unimatrix-server/src/main.rs:871`, `services/mod.rs:264` — `TODO(W2-4): add gguf_rayon_pool`).
- **GNN — UNIMPLEMENTED.** Reserved fields + "W3-1" comments only (`services/search.rs:56,92`; `phase_freq_table.rs:218`).

So the honest delta for transcript summarization is: **promote the ass-035/036 GGUF harness into a wired provider** (net-new — the generative path is not in production), reusing the ONNX/NLI lifecycle, rayon pool, and graceful-degradation patterns that already ship. This is why Q4 is a "delta," not "greenfield" — but the generative model class itself is genuinely new to production.

---

**Deliverable 1 — the delta, precisely (model class / where it runs / budget / mandatory fallback).**

- **Model class.** A small quantized instruction/summarization LLM in GGUF via llama-cpp-2 — the exact class already validated in ass-036 (Phi-3-mini-4k q4_k_m). This is a *generative* model, distinct from the shipping *discriminative* ONNX models (NLI = 3-logit classifier, embedding = vector encoder). Summarizing a transcript is a generation task, so the NLI/embedding ONNX path cannot be reused for the model itself — only its lifecycle/dispatch scaffolding transfers.

- **Where it runs — MUST be review-time (or a post-review background job), NEVER the query hot path.** Principle 7 (`product/PRODUCT-VISION.md:66`): "All analytics-derived search data cached in `Arc<RwLock<_>>`, rebuilt by tick. Never read from the database at query time." Transcript summarization inference is orders of magnitude heavier than a DB read, so it is categorically barred from `context_search`/`context_get`. Two admissible homes, both already have seams:
  - **Review-time tool call** — invoked inside `context_cycle_review` handler, alongside the existing `distill_before_purge` seam (`crates/unimatrix-server/src/mcp/distill_handler.rs:48`). Dispatched through the existing `RayonPool::spawn_with_timeout` (`crates/unimatrix-server/src/infra/rayon_pool.rs:146`) under the `MCP_HANDLER_TIMEOUT` = 30s ceiling (`crates/unimatrix-server/src/infra/timeout.rs:16`). Cleanest fit: the transcript is already in hand at that seam, and inference cost is paid by the caller who asked for the review.
  - **Background job (post-review, out-of-band)** — register as a `BackgroundJob` in the crt-056 work-unit registry (`crates/unimatrix-server/src/background/jobs.rs`, ADR-004 #5167). This is the *purpose-built* extensibility seam: "future background math becomes register-a-job, not re-architect-the-loop." A `TranscriptSummaryJob` would sit beside `GraphInferenceJob` (`jobs.rs:327`), inherit the 120s tick ceiling (`background.rs:418`, `TICK_TIMEOUT`), and never block a request. Cost: the transcript must survive in Plane B until the job runs (see Deliverable 3).
  - **Recommendation:** *review-time tool call*, opt-in per the Q3 `transcript` block, because (a) the transcript is guaranteed present at that seam and (b) it avoids extending Plane B residency. Leave the BackgroundJob registry as the documented seam for a later "async pre-compute" optimization.

- **Compute/latency budget.** Bounded by the same guards the ONNX path uses: single shared rayon pool sized `(num_cpus/2).clamp(4,8)` (`rayon_pool.rs:211`), per-model `Mutex<Session>` serialization, and an explicit wall-clock timeout (30s review-time / 120s tick). A generative pass over a multi-KB transcript is far heavier than an NLI batch, so it needs its *own* pool lane or a dedicated single-thread budget — which is exactly why the anticipatory `TODO(W2-4): gguf_rayon_pool` exists (`main.rs:871`). Do not run generation on the shared ML pool that serves request-path NLI/embedding: a multi-second generation would starve `spawn_with_timeout` inference (the pool floor of 4-8 exists precisely to prevent this).

- **Mandatory graceful-degradation fallback (Principle 5 — non-negotiable).** "Absent or failed model = previous behavior, not broken behavior." The shipping pattern to copy exactly:
  - **State-machine handle** with `Loading / Ready / Failed / Retrying` and a single `get_provider()` gate returning `Err` when not-ready (`crates/unimatrix-server/src/infra/nli_handle.rs:66,160`). Missing model file → `Failed` + warn, server continues (`nli_handle.rs:280`); retries exhausted → permanently disabled (`nli_handle.rs:366`).
  - **Structural fallback at the call site** (the crt-028 contract, ADR-003 #3335): the enrichment is an `Option<Summary>`; on any `Err`/absence the code path emits **today's observation-derived review verbatim**. crt-057's core guarantee already holds this — the review summary is 100% observation-derived and buffer-independent (`build_report`, `report.rs`), so an absent summarizer changes *nothing* about the default response. The generated summary is a strictly additive, skip-if-none section — mirror the ADR-004 response-assembly attach pattern (`transcript_candidates` as `Option<...>` with `skip_serializing_if`, #4850), so the leak-proof/degradation-proof shape is inherited by construction.

---

**Deliverable 2 — rough cost/latency envelope (from the existing GGUF path only).**

Falls only partly out of the existing path. What the harness gives for free (`product/research/ass-035/harness/src/gguf.rs`, pattern #3983):
- **Model:** Phi-3-mini-4k q4_k_m (~2.3 GB q4 weights on disk; SHA-pinned per #3983).
- **Resident memory:** ~768 MB KV cache at ctx=2048 *plus* quantized weights — call it **~3 GB resident** while loaded. Context is created once and KV-cache-cleared between inferences (#3983 pattern 1); a fresh context per inference re-pays the 768 MB and risks OOM.
- **Latency:** the harness *measures* wall-clock (`start.elapsed().as_millis()`) but enforces no timeout — the only bounds are `MAX_NEW_TOKENS` and the `N_CTX` token budget. Directionally: CPU-only small-quantized-LLM generation runs at ~single-digit-to-low-tens tokens/sec on commodity CPU, so a few-hundred-token summary is **~single-digit to low-tens of seconds** — comfortably inside a 120s background ceiling, marginal against a 30s review-time ceiling for large transcripts. **A purpose-built measurement was explicitly declined by the human**, so treat this as an order-of-magnitude envelope, not a spec. The ~3 GB resident footprint is the load-bearing cost number, and it is the strongest single argument for the *background-job* placement over always-loaded.

---

**Deliverable 3 — the coupling to Q1 / Plane B ("reason to grab more").**

Local inference *is* a plausible reason to hold more of Plane B in memory, and the link is concrete: a **background** summarizer (Deliverable 1, option 2) runs *after* the review returns, so the transcript must still exist when it runs. Today Plane B is purged at the review seam (`purge_cycle_transcripts`, `crates/unimatrix-server/src/server.rs:661`; ordering-gated so every purge is preceded by `distill_before_purge`, `distill_handler.rs:669`). A post-review job would need residency extended past that purge point.

**This stays inside the NG-1 in-memory transient envelope — no invariant challenge required — IF built as follows:**
- Plane B is already a *never-persisted, in-memory* ring buffer (`crates/unimatrix-server/src/infra/session_transcript.rs:1`, module contract; wire ingress `crates/unimatrix-engine/src/wire.rs:312` "accepted-and-dropped, never persisted, principle 8"). Holding it *longer in memory* is not persistence.
- The **held store already exists for exactly this** — `TranscriptHold` (`crates/unimatrix-server/src/infra/transcript_hold.rs:155`) holds buffers past session close, bounded by two backstops SCOPE says not to change: 64-session cap (`config.rs:1858`) and 24h TTL (`config.rs:1871`). A review-time or within-TTL background summarizer reads from this store **without any new persistence and without touching the caps.**
- **Memory arithmetic for "grab more":** per-session ceiling is 4 MiB (`session_transcript.rs:26`); held-store worst case is stated in-code as `transcript_buffer_max_bytes × transcript_hold_max_sessions` = 4 MiB × 64 = **~256 MiB** (`config.rs:1854`, `transcript_hold.rs:20`). Raising *fidelity* for a better summary means raising the 4 MiB per-session cap and/or the 64-session cap; resident memory scales as the **product** of the two. Adding the ~3 GB model footprint on top makes memory, not latency, the real budget constraint.

**The one thing that WOULD challenge NG-1 (flagged explicitly, never smuggled):** any scheme that persists the transcript to disk to survive a *restart* before a deferred/batched inference runs — e.g. "queue transcripts for overnight summarization." That crosses `#4721`/`#4850`/Principle 8 and the ADR-004 structural leak-impossibility guarantee (persisted `RetrospectiveReport` has no slot for transcript content, #4850). **Do not do this without a conscious human decision.** The safe design keeps inference inside the live in-memory window (review-time, or a same-process background job within the 24h TTL) and accepts that a transcript lost to restart/TTL simply degrades to today's observation-derived review (Principle 5 again).

---

## Unanswered Questions

- **Actual measured latency/tokens-per-sec on target hardware** — not answered by design: the human declined a purpose-built measurement, and the ass-035 harness numbers are not reproduced here (would require running the harness). Envelope in Deliverable 2 is order-of-magnitude only. A follow-on spike that runs the harness against a real transcript corpus would close this.
- **Summary quality / fidelity floor** — whether a small quantized model produces a review-grade summary from `Reconstructed`-provenance (0.81 fidelity, #4858) transcript input is unmeasured. This is the actual make-or-break question for Q4 and is a measurement spike, not a feasibility read.

---

## Out-of-Scope Discoveries

- **crt-056 `BackgroundJob` registry (#5167) is the ready-made seam for Q4.** "Register a job, don't re-architect the loop." A future `TranscriptSummaryJob`/`GraphEnrichmentJob`-style unit drops in beside `GraphInferenceJob` (`background/jobs.rs:327`). Worth noting in the headline `context_cycle_review` design as the seam to *leave open* (SCOPE L135) without building.
- **The `TODO(W2-4): gguf_rayon_pool` placeholders** (`main.rs:871,1593`; `services/mod.rs:264`) are a pre-existing anti-stub-rule tension (CLAUDE.md rule 2 forbids lingering TODOs). They encode a real intent (separate pool lane for generative inference) but have sat unimplemented; flag for cleanup or an issue, independent of Q4.
- **GNN is named in Principle 5 but unimplemented** (reserved W3-1 fields only). The vision text lists a capability that does not exist; may warrant a vision-doc reconciliation so future spikes don't over-assume shipping ML breadth (same trap this Q4 correction caught).

---

## Recommendations Summary

- **Q4 model class / placement:** promote the ass-035/036 GGUF harness (Phi-3-mini-4k q4_k_m via llama-cpp-2) into a wired provider on its **own** rayon lane; run it **review-time (opt-in, Q3 `transcript` block)**, never the query hot path (Principle 7); leave the crt-056 BackgroundJob registry as the documented seam for a later async variant.
- **Mandatory fallback:** copy the NLI `nli_handle.rs` state-machine + crt-028 structural `Option`/skip-if-none attach; absent/failed model = today's 100%-observation-derived review, unchanged (Principle 5).
- **Plane B coupling:** read from the existing in-memory `TranscriptHold` (never new persistence); "grab more fidelity" = raise the 4 MiB / 64-session caps, cost scales as their product + ~3 GB model resident; **any disk-persist-to-survive-restart scheme is an explicit NG-1/Principle-8 breach requiring a conscious human call, not a smuggled one.**
- **Sizing verdict:** **Q4 warrants its own follow-on spike, and it does NOT gate crt-057.** Feasibility is positive and the fallback is clean, but the two make-or-break questions — measured latency/footprint on target hardware and summary quality from truncated/`Reconstructed` transcript input — are *measurement* work (`empirical` confidence) outside this feasibility read. Recommend a dedicated measurement spike that runs the harness against a real transcript corpus before any delivery commitment. The crt-057 redesign should only *leave a seam* (the opt-in `transcript` contract + BackgroundJob registry), building nothing.
