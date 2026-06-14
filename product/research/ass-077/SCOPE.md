# ASS-077: Best utilization of the streamed session transcript — value within the never-persist constraint

**Date**: 2026-06-14
**Spike type**: Investigation + design-space evaluation (read-code dominant; light empirical measurement of the availability gap)
**Status**: SCOPE (draft — pending human confirmation of flagged decisions)
**Working number**: ass-077 (renumbered from ass-076 — directory collision with the #708 "edges-on-`context_get`" spike, which holds `product/research/ass-076/`)
**Tracking**: GH Issue #741
**Feeds**: a design session for transcript utilization and the `context_cycle_review` evaluation substrate (research → design → delivery)

## Origin

Two threads converge here.

**Thread 1 — value left on the table.** The TS client now streams the **full session transcript** continuously over UDS+HTTP (vnc-026 #679, vnc-027 #680). The server holds it only in a per-session, in-memory `TranscriptBuffer`, drained per turn (#4799) and purged at cycle review / session close / TTL. We pay for the streamed traffic but likely do not extract its full value, and it is unclear the buffer even survives to be available at cycle review for a multi-turn / multi-session feature.

**Thread 2 — a live regression exposed the retro's fragile substrate (#750, lesson #4998).** ADR-004 (vnc-027) deliberately narrowed PreToolUse: the TS client returns a null no-send sentinel for non-cycle PreToolUse, and the install-level matcher no longer fires the hook for ordinary tools. But `context_cycle_review`'s per-session aggregation (`compute_session_summaries` in `unimatrix-observe/session_metrics.rs`; the phase breakdown in `mcp/tools.rs`; `compute_context_reload_pct`) counts **only** PreToolUse records. Result: every TS-client cycle reports per-session **Calls 0 / Tools — / knowledge 0 / context_reload 0%** — a believable zero, not an honest "unavailable." Cycle-level findings (hotspots, knowledge reuse, phase timeline) survived because they read PostToolUse, the corpus, and cycle events.

The two candidate point-fixes for #750 — re-point aggregation onto PostToolUse (Option A), or have the client re-emit non-cycle PreToolUse (Option B, which reverses the matcher reduction) — **both re-ground the retro on a hook event class.** That is the fragile altitude: the next deliberate event-set change breaks it again. This spike asks whether the transcript — a richer, client-event-independent ground truth we already collect — should instead become the durable substrate for the retro's process-improvement signal, and what that costs inside the never-persist / content-opacity envelope (ADR-002, SR-02).

**Framing for self-learning.** The objective is **effectiveness toward the self-learning goal.** A retrospective does not itself improve a process — but it produces the *information about the process* that agents and humans need in order to improve it, which is a precondition for learning. The bar for this spike is therefore: does the proposed substrate surface the right, trustworthy process-signal at review time, durably, for multi-session cycles?

## Goal — questions to answer

### Retro evaluation substrate (primary thread — the folded question)

- **RQ-1 — What is available to `context_cycle_review`, assuming a multi-session cycle?**
  Inventory every signal the review *can* read at run time for a cycle spanning multiple sessions: the transcript content that actually reaches distillation (see RQ-5), surviving PostToolUse RecordEvents, cycle events (cycle_start / phase-end / stop), and corpus deltas (entries stored/corrected/curated during the cycle). For each, state fidelity, lifetime, and whether it survives session close and the per-turn drain.

- **RQ-2 — What is worth aggregating to help future agents/humans continually improve processes?**
  From the available signals, identify the *high-value* aggregations — the ones that tell an agent or human something actionable about *how the work went* and where the process leaks. Candidates to evaluate, not presume: phase durations and transitions, re-read / context-reload behavior around compaction boundaries, tool-mix per phase, knowledge reuse vs. fresh-store ratio, rework hotspots, attribution completeness. Rank by *decision value* (would it change how the next cycle is run?), not by ease of computation. Distinguish per-session vs. per-cycle aggregation for the multi-session case.

- **RQ-3 — How does this compare to the previous PreToolUse / `"*"` architecture?**
  For each legacy per-session metric (Calls / Tools / knowledge-per-session / `context_reload`), state precisely what it measured under the `"*"` PreToolUse hook, what is **recoverable** from the surviving streams, what is **better measured** from the transcript (e.g. `context_reload` is natively a transcript + PreCompact-tail concept, plausibly more faithful than a PreToolUse counter), and what is **genuinely lost**. Do not anchor on reproducing the old numbers — assess whether the transcript substrate yields a *better* process-signal than the legacy proxy.

- **RQ-4 — Where should the substrate live, and when is it computed?**
  Evaluate the architecture inversion: instead of the review *pulling* raw hook events, have crt-052's **distill-before-purge** seam (it has the transcript in hand) *push* a durable, content-opaque per-cycle **evaluation record** onto `cycle_review_index` (per pattern #4178 — derived aggregates belong there, single write via `store_cycle_review()`, bump `SUMMARY_SCHEMA_VERSION`), which `context_cycle_review` then consumes. Compare against the #750 stopgap (re-point onto PostToolUse). Assess robustness to future hook-set churn, and respect the four success-return points (#4750). Recommend a `#750` stopgap (lean: fail-loud / mark unavailable over fake-zero) decoupled from the durable re-grounding.

### Transcript utilization (carried from #741)

- **RQ-5 — Availability / lifetime.** Given the per-turn `Stop→drain` (#4799) and the crt-052 held-buffer bridge (`transcript_hold.rs`), what transcript content *actually* reaches a `context_cycle_review` distillation today — for a single-turn feature, a multi-turn feature, and a multi-session feature? **Quantify the gap between "streamed" and "usable at review"** (light empirical measurement on a real multi-session cycle is in scope).

- **RQ-6 — Value extraction beyond the retro.** Which other functions benefit from richer/longer transcript access — marker recovery (#700), attribution accuracy (#703), PreCompact tail fidelity, drift/learning signals — and what is the marginal value of each?

- **RQ-7 — Security envelope.** Which improvements are achievable *within* never-persist / in-memory / content-opacity (ADR-002, SR-02)? Distilling *derived aggregates* (counts, rates, durations, phase boundaries) is persistable; raw transcript content is not — establish where that line falls for each proposed aggregation. Where real value would require relaxing the constraint, isolate it and flag it explicitly for human judgment. **Do not assume the constraint moves.**

- **RQ-8 — Traffic justification.** Is the cost/value ratio of continuous delta streaming favorable? Could value-per-byte rise (or traffic fall) without losing fidelity?

## Breadth

**`code-only`** (primary) — `TranscriptBuffer` + per-turn drain path, `transcript_hold.rs` (crt-052 reconstruction), the crt-052 distillation seam, `unimatrix-observe/session_metrics.rs`, the `mcp/tools.rs` `context_cycle_review` pipeline and its four success returns, `cycle_review_index` schema + `store_cycle_review()` + `SUMMARY_SCHEMA_VERSION`, and the cycle-events store. Light **measurement** to quantify the RQ-5 availability gap on a live multi-session cycle. No external ecosystem survey required.

## Approach

**Investigation** (what each stream contains and what reaches review today) + **evaluation** (rank substrate options against process-signal value, trust, and the never-persist envelope) + **design input** for the subsequent design session. Empirically measure the streamed-vs-usable gap rather than asserting it.

## Confidence required

**Directional** — a recommended design with ranked options and an explicit security-trade-off flag where value pushes against never-persist. No working PoC required; FINDINGS.md is input to a design session.

## Target outputs

FINDINGS.md delivering:
- An **availability inventory** for `context_cycle_review` on a multi-session cycle (RQ-1, RQ-5) with the streamed-vs-usable gap quantified.
- A **ranked set of process-improvement aggregations** worth surfacing, scored by decision value (RQ-2).
- A **legacy comparison table** — per metric: recoverable / better / lost vs. the PreToolUse `"*"` architecture (RQ-3).
- A **substrate recommendation** (transcript-distilled durable eval record vs. PostToolUse re-point) with the architecture and write-point spelled out (RQ-4), plus a decoupled **#750 stopgap** recommendation.
- A transcript-utilization options list beyond the retro (RQ-6), each ranked by value-within-constraints, with any constraint-relaxing option isolated for human decision (RQ-7, RQ-8).
- May spawn follow-up feature issues.

## Prior art / constraints

- **ADR-001/002/004/005/006 (vnc-025)** — buffer shape, content opacity, parity, retention cap.
- **#750 / lesson #4998** — the PreToolUse-retirement regression; the exact aggregation read points and the vnc-034↔vnc-035 discriminator.
- **ADR-004 (vnc-027)** — deliberate PreToolUse narrowing (matcher + client sentinel); event-set parity explicitly not a goal.
- **Pattern #4178** — derived review-time aggregates belong on `cycle_review_index`, not `cycle_events`; single write via `store_cycle_review()`; bump `SUMMARY_SCHEMA_VERSION`.
- **Pattern #4750** — `context_cycle_review` has four success-return points; any success-gated side effect (e.g. crt-052 distill-before-purge) must fire at all four.
- **#4799** per-turn drain; **#4828** UDS/HTTP session-split blindness.
- **Open consumers**: #700 (MARKER tier), #703 (AC-07 attribution-accuracy proof), #707 (topic_source). crt-052 distillation + `transcript_hold` reconstruction.

## Depends on

vnc-025 (#670), vnc-026 (#679), vnc-027 (#680) — all shipped. Relates to #700, #703, #750.
