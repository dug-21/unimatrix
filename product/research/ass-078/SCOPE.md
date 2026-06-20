# ASS-078: Fold-at-ingest streaming aggregation of the session transcript — content-opaque activity signal, durably cycle-tied

**Date**: 2026-06-14
**Spike type**: Investigation + design-space evaluation (read-code dominant; light external touch on token-estimation norms)
**Status**: SCOPE (confirmed — research in progress)
**Working number**: ass-078
**Tracking**: GH Issue #751
**Feeds**: a design session for an ingest-time transcript-aggregation component + its durable per-cycle store
**Sibling**: ass-077 (#741) — complementary, not overlapping (see "Relationship to ass-077")

## Origin

ass-077 (#741) evaluated the transcript as a substrate **read at review time** and correctly concluded: don't lean on it — re-ground the existing per-session metrics on durable PostToolUse, keep the transcript as ephemeral response-time enrichment. That conclusion stands for *those* metrics.

But ass-077 assumed only two mechanisms exist for using the transcript: (a) content → response, never persisted, or (b) at persist time, read the held buffer and parse content to derive a number (flagged R-A, "erodes the structural leak gate," not recommended). It never evaluated a **third mechanism**:

> **Fold-at-ingest.** Reduce each transcript delta into running, content-free numeric accumulators *as it streams in* — at the per-turn drain / delta-merge seam — then drop the bytes. Persist only the accumulated integers at review. Conversation content never lands in a persisted structure; no content field is added to the stored row; the number is computed at the *streaming* boundary, not the *storage* boundary.

This mechanism matters for three reasons:

1. **It dissolves ass-077's hardest problem.** ass-077's RQ-5 agony — the buffer doesn't survive to review without Wave B, multi-turn collapses to the 0.81 reconstruction floor, 24 KiB/256 KiB distillation caps are the real ceiling — exists *only because that approach needs the buffer present at review*. Streaming accumulators need nothing at review; they survive multi-turn and multi-session **by construction** because counters keep incrementing across every drain. The per-turn drain (#4799) that ass-077 treats as the adversary is the ideal fold-and-discard hook.
2. **It captures signal no durable stream carries.** PostToolUse rows hold `tool`+`input`+`response_size`+a ≤500-byte snippet — tool I/O, not total agent throughput. The transcript is the *only* source of conversation volume. Fold-at-ingest yields **tokens/bytes per cycle** (a self-learning efficiency signal) as a pure counter, retaining nothing.
3. **It rescues the metric ass-077 most wanted but demoted.** ass-077 ranks faithful `context_reload` (transcript + PreCompact tail) as the transcript's *unique* contribution, then relegates it to ephemeral response-only because it can't persist it safely. Fold compaction-boundary + re-read detection at ingest → reload becomes a **durable persisted number, content never retained.**

**Framing for self-learning.** The bar is the same as ass-077: produce trustworthy *information about the process* that agents/humans use to improve it. Token/activity volume is a process-efficiency signal ("this cycle type runs heavy; the process leaks here"). This spike decides what is cheaply and safely extractable in-flight, and how it is made durable and bound to the right cycle.

## Goal — questions to answer

### Dimension A — What to capture (the aggregation catalog)

- **RQ-1 — Line-by-line (per-delta, O(1)) reductions.** What is computable cheaply and content-opaque on each delta as it arrives: total bytes, token estimate, per-delta **regex-class hit counts** (e.g. tool-error / retry / refusal / re-read signatures). Define the candidate catalog, the per-delta compute budget (this is the hot ingest path), and the rule that output is always a number, never retained text.
- **RQ-2 — Multi-line / windowed aggregation (only where it earns its state).** Which signals genuinely need cross-delta or bounded-window state, and are worth that state: durable `context_reload` (compaction boundary + subsequent re-read detection), turn-size distribution (streaming moments), thrash/loop detection (rolling hash window). For each: the bounded-state cost, and an honest verdict on whether it *makes sense* vs. defer. Default posture: scalar counters in v1, windowed aggregates only with a justified state budget.
- **RQ-3 — Token-estimate fidelity.** Method (bytes/N heuristic vs. an embedded generic tokenizer), achievable accuracy, and model-dependence (no Claude tokenizer server-side; varies by model). Establish the honest framing: a **relative efficiency signal, not a billing-grade count.** Where is the estimate good enough, where would exactness be needed (and therefore out of scope)?
- **RQ-4 — Catalog governance / domain-agnostic.** The regex catalog is open-ended and each pattern is a per-delta cost and a domain assumption. How is it kept **small, config-externalized, and capped** (dsn-001 config externalization precedent) so it doesn't hardcode SDLC-flavored patterns or grow unbounded on the hot path?

### Dimension B — Where and how to make the aggregate durable and cycle-tied (the user's explicit second ask)

- **RQ-5 — Accumulator location + ingest seam.** Where does the stateful accumulator live and update — the `drain_and_signal_session` drain (`session.rs:834`), the delta-merge path (`session.rs:388-395`), or the held-buffer bridge (`transcript_hold.rs`)? Component shape, ownership, lifetime, and hot-path latency budget. How does it interact with the `TranscriptBuffer` / `TranscriptHold` lifecycle without holding content.
- **RQ-6 — Durability + cycle binding (the hard part).** How does an aggregate become durable and tied to the **correct `feature_cycle`**? The landing store is settled — `cycle_review_index`, single write via `store_cycle_review()` (`tools.rs:2864`), pattern #4178, bump `SUMMARY_SCHEMA_VERSION` (currently 3). The hard question is **attribution timing**: at ingest the accumulator has a `session_id`, but the cycle binding resolves later (cycle declaration, `topic_source`, and the #4828 UDS/HTTP session-split blindness). So:
  - Does the accumulator key on `session_id` at ingest and **late-bind** to `feature_cycle` at review (session→cycle join), or eager-bind at cycle-declare time?
  - How does it accumulate correctly across a **multi-session** cycle (sum per-session accumulators into the per-cycle row)?
  - What happens to counters for a session that never declares a matching cycle (the fail-loud drop case from ass-077 RQ-5)?
  - Where is the per-session accumulator parked between drain and review so it survives session close without holding content?
- **RQ-7 — Never-persist envelope proof.** Demonstrate the line holds and that this is **not** ass-077's rejected R-A: content folded at ingest and dropped; only integers persist; **no content-bearing read on the persist path**; the structural leak gate (no content field on `RetrospectiveReport` / `cycle_review_index`) preserved. Confirm the persisted columns are categorically the same as a network byte-counter (ADR-002 #4740, AC-06).
- **RQ-8 — Vision-lane + scope boundary.** Confirm this stays a **self-learning process signal** (surface "N tokens, X% above median for this feature-type" as knowledge) and explicitly excludes orchestration-adjacent surfaces (budget enforcement, cost dashboards as a product, scheduling-by-cost) — the vision's "not an orchestration engine" line. Name what is out of scope so the feature can't drift into FinOps.

## Relationship to ass-077

| | ass-077 (#741) | ass-078 (this) |
|---|---|---|
| Seam | review time (pull) | ingest time (fold-and-discard) |
| Mechanism | change a filter literal — re-ground existing metrics on PostToolUse | new stateful accumulator component |
| Signal | recover legacy per-session metrics (Calls/Tools/reload) | net-new volume/cost/activity + **durable** reload |
| Buffer-at-review dependency | yes (its RQ-5 gap) | **none** (counters accumulate) |

ass-077's #750 re-grounding and fail-loud stopgap ship independently. ass-078 **reuses** ass-077's settled landing (`cycle_review_index`, single write, `SUMMARY_SCHEMA_VERSION` bump) but feeds it from a new ingest accumulator rather than a review-time read.

## Breadth

**`code-only`** (primary) — the drain / delta-merge / hold seam (`session_transcript.rs`, `session.rs`, `transcript_hold.rs`), `cycle_review_index` + `store_cycle_review()` + `SUMMARY_SCHEMA_VERSION`, the cycle-attribution chain (`feature_cycle` binding, `topic_source`, #4828 session-split), config externalization (dsn-001) for the catalog. Light **`code+ecosystem`** touch only on token-estimation norms (heuristic vs. embedded BPE accuracy).

## Approach

**Investigation** (the ingest seams, the accumulator lifecycle, the attribution-timing chain) + **evaluation** (rank capturable signals by decision-value-per-state-cost; weigh late-bind vs eager-bind cycle attribution) + **design input**. Honest verdicts on which multi-line aggregates earn their bounded state and which to defer.

## Confidence required

**Directional** — a recommended design with ranked options and an explicit flag wherever a signal pushes against the never-persist envelope or the vision lane. No PoC required; FINDINGS.md is input to a design session.

## Target outputs

FINDINGS.md delivering:
- A **ranked capture catalog** — per-delta (RQ-1) and bounded-window (RQ-2) signals, each with compute/state cost and a keep/defer verdict; token-estimate method + fidelity framing (RQ-3); catalog governance model (RQ-4).
- A **durability + cycle-binding design** (RQ-5, RQ-6): accumulator location, the late-bind-vs-eager-bind attribution decision, multi-session accumulation, the unmatched-session case, and the `cycle_review_index` write/schema-version path.
- An **envelope proof** (RQ-7) distinguishing fold-at-ingest from ass-077's R-A, and a **vision-lane boundary** statement (RQ-8).
- May spawn follow-up feature issues.

## Prior art / constraints

- **ass-077 / #741 FINDINGS** — the durable-stream substrate analysis, the RQ-5 availability gap this mechanism dissolves, the R-A flag this mechanism is distinct from, and the structural leak gate to preserve.
- **ADR-002 #4740 / AC-06** — content opacity, never-persist, response-only transcript path.
- **Pattern #4178** — derived review-time aggregates belong on `cycle_review_index`; single write via `store_cycle_review()`; bump `SUMMARY_SCHEMA_VERSION`.
- **Pattern #4750** — `context_cycle_review` has four success-return points; any success-gated side effect fires at all four.
- **#4799** per-turn drain (the fold seam); **#4828** UDS/HTTP session-split blindness (the attribution hazard); **dsn-001** config externalization (the catalog governance precedent).
- **Vision** — "Unimatrix is not an orchestration engine… does not manage workflows" (the RQ-8 boundary).

## Depends on

vnc-025 (#670), vnc-026 (#679), vnc-027 (#680) — shipped. Sibling to ass-077 (#741). Relates to #750, #707 (`topic_source`), #4828.
