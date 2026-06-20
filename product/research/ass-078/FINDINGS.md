# FINDINGS: Fold-at-ingest streaming aggregation of the session transcript — content-opaque activity signal, durably cycle-tied

**Spike**: ass-078
**Date**: 2026-06-14
**Approach**: investigation + design-space evaluation + design input (read-code dominant; light external touch on token-estimation norms)
**Confidence**: directional — recommended design with ranked options; no PoC
**Tracking**: GH #751

---

## Orientation — what the code actually permits (load-bearing for every RQ)

The fold-at-ingest hypothesis lives or dies on five facts established by reading the seams:

1. **The fold hook already exists and is content-opaque by contract.** `TranscriptBuffer::apply_delta(offset, bytes)` (`session_transcript.rs:150`) is the single merge point. It already takes `&[u8]` and is forbidden by ADR-002 (#4740) from emitting any `Display`/`Debug`/`Result` carrying bytes. A fold is *another* O(1) read of `bytes` at exactly this site that emits **integers**, then the bytes are merged or dropped as today. **No new content-bearing surface is created** — the bytes are already in hand here for the memcpy.

2. **The accumulator's parking spot already exists with the right lifecycle.** `SessionState` (`session.rs:146`) owns `transcript: Arc<Mutex<TranscriptBuffer>>`. The per-turn drain (#4799, `drain_and_signal_session:834`) hands that Arc to `TranscriptHold` keyed by `session_id` **with its `feature_cycle`** (`session.rs:857-861`), where it survives drains, keeps merging deltas (`session.rs:388-395` routes held deltas), and purges only at review/sweep/cap-evict. A scalar accumulator co-located with the buffer rides this exact lifecycle for free — it is held, summed, and purged on the same path crt-052 Wave B already built.

3. **The durable landing already takes derived integer columns.** crt-047 added **seven integer columns** (`corrections_total`, `corrections_agent`, …, `orphan_deprecations`) to `CycleReviewRecord` (`cycle_review_index.rs:57-90`), written through the single `store_cycle_review()` INSERT/UPDATE (`cycle_review_index.rs:195-294`), governed by `SUMMARY_SCHEMA_VERSION` (currently **3**, `cycle_review_index.rs:35`). This is the precise precedent for fold-at-ingest's output: more integer columns, same single writer, bump the version. Pattern #4178 governs it; pattern #4750's four success returns govern *when* it fires.

4. **The leak gate is structural and must stay structural.** `RetrospectiveReport` has no candidate/content field; `test_candidates_structurally_absent_from_memoized_report` (`distill_handler.rs:775`) asserts the persisted JSON cannot contain `transcript_candidates`. Integer columns on `CycleReviewRecord` do not touch this gate at all — they are siblings of the curation-health integers, which already coexist with it.

5. **Config externalization has a working precedent shape.** `RetentionConfig` (`config.rs:1746`) uses `#[serde(default)]` per field + a `validate()` that *rejects* out-of-policy values (`RetainDays` rejected as enterprise-only, `config.rs:1729-1737`). The category allowlist (#4591) is the runtime-extensible-but-capped, validated-at-load precedent. A regex catalog config lands the same way: defaulted, bounded, validated.

**Net**: the mechanism is not speculative plumbing. Every piece it needs already exists for an adjacent purpose. The honest question is not *can* we fold-at-ingest, but *which signals are worth the per-delta cost and bounded state*, and *whether the cycle-attribution chain can carry the integers to the right row without lying.* The verdicts below are accordingly conservative.

---

## Findings

### RQ-1 — Line-by-line (per-delta, O(1)) reductions — the ranked per-delta catalog

The hot path is `apply_delta`, invoked once per transcript delta (≤1 MiB frame ceiling). Budget rule: each per-delta signal must be **O(bytes_in_delta) with a tiny constant**, allocation-free, branch-light, and emit only an integer increment. The output contract is identical to ADR-002's: a number, never retained text.

| Rank | Signal | Per-delta compute | State | Decision value | Verdict |
|---|---|---|---|---|---|
| 1 | **Total bytes / delta count** | `+= bytes.len()`; `+= 1` | 2× u64 | High — the *only* source of total agent conversation throughput; PostToolUse carries tool I/O, not turn volume | **KEEP** — free, the anchor signal |
| 2 | **Token estimate (bytes/N heuristic)** | one integer divide on the byte counter at *read* time, not per delta | 0 extra (derived from #1) | High — relative efficiency signal per cycle | **KEEP** — derived from #1; near-zero |
| 3 | **Tool-error / retry hit count** | one pass of a small compiled regex set (Aho-Corasick or `regex::RegexSet`) over `bytes` | 1× u32 per class | Medium-high — "this cycle thrashed" is the clearest process-leak signal | **KEEP (capped catalog)** — earns its cost only if the set stays tiny (RQ-4) |
| 4 | **Refusal / safety-stop hit count** | same RegexSet pass | 1× u32 | Medium — rare but high-information when present | **KEEP (same pass)** — marginal cost ~0 once #3's pass runs |
| 5 | **Re-read / re-ground signature count** | same RegexSet pass | 1× u32 | Medium — feeds the durable-reload signal (RQ-2) | **KEEP (same pass)** — shares the pass; see RQ-2 caveat on boundary state |
| 6 | Newline / turn-delimiter count (cheap turn proxy) | byte scan for `\n` or JSONL record boundary | 1× u32 | Low-medium — a turn proxy, but deltas don't align to turns | **DEFER** — deltas are offset-chunks, not turns; the count is noisy and a true turn metric needs windowed state (RQ-2 #3). Not worth a per-delta counter that mismeasures. |
| 7 | Per-delta entropy / compressibility estimate | byte histogram or sample | histogram | Low — speculative "is this boilerplate vs novel" signal | **DEFER** — no demonstrated decision-value; real per-delta cost; classic scope-creep. |
| 8 | Language/code-fence detection | regex pass | counters | Low — domain-flavored, drifts toward SDLC assumptions | **DEFER** — RQ-4 hazard incarnate; not domain-agnostic. |

**Critical efficiency note**: signals 3–5 share **one** `RegexSet`/Aho-Corasick pass over `bytes`. The cost is *one* multi-pattern scan per delta, not one per pattern — this is the whole reason a capped catalog is affordable. Adding the Nth pattern to the set is near-free; the cost is the *byte scan*, paid once. This is why RQ-4's cap is about *catalog size as a domain-assumption / maintenance hazard*, not primarily a compute hazard.

**Boundary on a held delta path**: a delta routed to a *held* buffer (drained session, `session.rs:388-395`) must fold into the **held** accumulator, not a registry one — the accumulator must travel with the Arc into `TranscriptHold` (see RQ-5). If it doesn't, held-session bytes silently don't count: the exact "believable zero" trap ass-077 named. Fold logic therefore lives where the buffer lock is taken, on both the registered and held route.

**Recommendation (RQ-1)**: Ship signals **1, 2, and a single shared-pass RegexSet covering 3–5** in v1. Defer 6–8 — none clears the decision-value bar for its per-delta cost or its domain-neutrality. Output is always an integer increment; no delta text is retained past the existing merge.

---

### RQ-2 — Multi-line / windowed aggregation — only where it earns its state

Default posture (per SCOPE): scalar counters in v1; windowed aggregates only with a justified state budget. Evaluated each candidate against decision-value-per-byte-of-state.

| Candidate | Required cross-delta state | Decision value | Verdict |
|---|---|---|---|
| **Durable `context_reload`** (compaction-boundary count + subsequent re-read detection) | 1× u32 reload counter + 1 bool/timestamp "saw a compaction marker" latch | **High** — ass-077 ranked faithful reload as the transcript's *unique* contribution but had to demote it to ephemeral; fold-at-ingest persists it as a pure counter. Cross-session by construction (counter rides the hold). | **KEEP** — minimal state (a counter + a latch), highest unique value. This is the signal that most justifies the whole mechanism. |
| **Turn-size distribution** (min/max/mean/p95 of turn byte-length) | running count + sum + max + a small reservoir or t-digest for p95 | Medium — diagnostic ("turns ballooning") but rarely changes the next run; ass-077 ranked tool-mix-style diagnostics as enrichment, not KPI | **DEFER to mean only** — keep `turn_count` + reuse byte total to derive mean turn size (zero extra state). Reject p95/reservoir: t-digest state on the hot path is not justified by the decision-value. Mean-turn-size is a free byproduct of #1. |
| **Thrash / loop detection** (rolling hash window over recent deltas to catch repeated content) | a ring of N rolling hashes + a small seen-set | Medium — "the agent is looping" is actionable, but a content-rolling-hash window is the **most state-heavy and most content-adjacent** candidate (it must remember shapes of recent content) | **DEFER** — fails the cost test twice: bounded-window *content-shape* state on the hot path, and it edges toward retaining a content fingerprint (a hash of conversation bytes), which invites the "is a hash content?" argument the spike is trying to avoid. The retry-count (RQ-1 #3) already proxies thrash cheaply and content-oblivously. Revisit only if retry-count proves insufficient *with measured evidence.* |

**The reload latch — the one piece of genuine windowed state worth it.** Re-read detection needs to know "a compaction boundary was crossed, and *after* it the agent re-read prior context." That is a two-state latch per session (`saw_compaction: bool` + `reload_after_compaction: u32`), incremented when a re-ground signature (RQ-1 #5) fires while the latch is set. Total state: ~5 bytes per session. It is durable across drains because it rides the held accumulator. This converts ass-077's demoted-to-ephemeral reload metric into a persisted integer — **the strongest single argument for fold-at-ingest existing at all.**

**Recommendation (RQ-2)**: KEEP exactly one windowed signal — the durable reload counter + compaction latch (~5 bytes/session). KEEP mean-turn-size as a zero-extra-state byproduct. DEFER turn-size percentiles (reservoir state unjustified) and thrash/rolling-hash detection (state-heavy *and* content-fingerprint-adjacent; retry-count already proxies it). This is a deliberately small windowed surface.

---

### RQ-3 — Token-estimate fidelity — method + honest framing

**Method recommendation: bytes/N heuristic, computed at read time from the byte counter. NOT an embedded tokenizer.**

- **No Claude tokenizer exists server-side**, and the true token count is **model-dependent** (Opus/Sonnet/Haiku and future models tokenize differently). Anthropic's exact tokenizer is not distributed; the `count_tokens` API is a network call (out of scope — adds a billing-grade dependency and external latency to the hot path). Any server-side number is therefore an *estimate by definition.*
- An **embedded generic BPE** (e.g. a `tiktoken`/`cl100k`-style vocab) would add a multi-MB vocab dependency, real per-delta CPU (BPE merge passes), and *still* be wrong for Claude — it would trade a transparent ±N% heuristic for an opaque, differently-wrong, heavier one. **Net negative** for a relative signal.
- **bytes/N** (N≈3.5–4.0 for English-dominant text; the catalog config should make N tunable, not hardcoded) is O(1), allocation-free, derived from the byte counter already kept (RQ-1 #1). Directional accuracy for *English prose* is typically within ~10–20% of true tokens; it degrades for code, non-Latin scripts, and heavy whitespace/JSON, where the bytes-per-token ratio shifts.

**Honest fidelity framing (mandatory):** the persisted number is a **relative efficiency signal, not a billing-grade count.** It answers *"this cycle ran ~30% heavier than the median cycle of this feature-type"* — a within-corpus, same-estimator comparison where the estimator's bias **cancels** because every cycle uses the same N. It does **not** answer *"this cycle cost $X"* or *"this cycle used exactly T tokens."* Surfacing it as a dollar or exact-token figure would be dishonest and drifts straight into FinOps (RQ-8).

- **Good enough**: relative cross-cycle / cross-feature-type efficiency comparison, drift detection ("this feature-type is trending heavier"), outlier flagging. All same-estimator comparisons.
- **Where exactness would be needed → out of scope**: billing, quota enforcement, cost attribution to a customer. These require the `count_tokens` API per model and a billing pipeline — explicitly excluded by RQ-8.

**Recommendation (RQ-3)**: bytes/N heuristic, N config-tunable (default ~3.8), computed at read time from the byte counter. Persist it labeled as an **estimate**; never surface it as an exact token count or a cost. Reject embedded BPE (heavier, opaque, still model-wrong). If exactness is ever required, that is a separate billing feature, not this one.

---

### RQ-4 — Catalog governance / domain-agnostic model

The regex catalog (RQ-1 #3–5) is the one open-ended, domain-assumption-bearing surface. Without governance it grows unbounded and hardcodes SDLC-flavored patterns ("test failed", "compile error") that don't generalize to a non-coding collective.

**Design (following dsn-001 config externalization + #4591 capped-allowlist precedent):**

1. **Config-externalized, not hardcoded.** The catalog is a `[transcript_signals]` config section (sibling to `[retention]`, `config.rs:1746`), each entry `{ class_name, pattern, enabled }`. Default set ships small and **domain-neutral** (generic signatures: retry/error/refusal at the *behavioral* level, not language-specific). Deployments override per their domain — the same externalization dsn-001 established and #4591 proved runtime-extensible.
2. **Hard cap on catalog size** (`validate()`-enforced, e.g. ≤16 classes). This is the #4591 pattern: runtime-extensible HashSet, but bounded. The cap protects the *maintenance/domain-assumption* surface more than compute (RQ-1: all patterns share one RegexSet pass, so the marginal compute is near-zero — the real cost of an unbounded catalog is a sprawl of domain assumptions nobody audits).
3. **Compiled once at load, validated at load.** Patterns compile into one `RegexSet` at config load; an invalid regex fails `validate()` loudly (same posture as `RetainDays` rejection, `config.rs:1729`). No per-delta compilation; no runtime catalog mutation on the hot path.
4. **Numbers are class-counts only.** The persisted output is `count[class_name]` — integers keyed by a class label, never the matched text. A new class adds a column-or-keyed-integer, never a content field.
5. **Domain-neutral default posture.** The shipped default catalog must avoid SDLC-specific literals. Prefer behavioral signatures that generalize (the *shape* of a retry/error/refusal) over tool-specific strings. Anything language- or tool-specific is a deployment override, not a default — this keeps the collective domain-agnostic by construction.

**Recommendation (RQ-4)**: Externalize the catalog as a defaulted, `validate()`-bounded (≤~16 classes) config section per dsn-001 / #4591; compile once at load; ship a small domain-neutral default; persist class-counts as keyed integers only. The cap's primary job is bounding *domain assumptions*, secondarily hot-path discipline.

---

### RQ-5 — Accumulator location + ingest seam

**Decision: the accumulator is a small scalar struct co-located with `TranscriptBuffer`, folded at `apply_delta`, and it rides the existing `Arc<Mutex<…>>` through drain → hold → review exactly as the buffer does.**

Evaluated three candidate seams:

| Seam | Assessment |
|---|---|
| **`drain_and_signal_session` (`session.rs:834`)** | Wrong granularity for folding — fires once per turn at *close*, after deltas already streamed. Right place to **hand the accumulator to the hold** (it already hands the buffer Arc), wrong place to *compute* per-delta. |
| **Delta-merge path (`apply_transcript_delta`, `session.rs:369-402` → `session_transcript.rs:150`)** | **Correct fold site.** The bytes are already in hand under the buffer lock for the memcpy; folding is a second O(1) read of the same slice. Both the registered route (`session.rs:400-401`) and the held route (`session.rs:388-395`) pass through here. |
| **Held-buffer bridge (`transcript_hold.rs`)** | Wrong place to *compute* (it holds, it doesn't merge logic) but the **correct place for the accumulator to live between drain and review**, alongside the held `Arc<Mutex<TranscriptBuffer>>`. |

**Component shape**: fold the counters **into `TranscriptBuffer` itself** as additional private scalar fields. Co-locating *inside* `TranscriptBuffer` is preferred: it then rides every existing path (drain hand-off, held-delta route, snapshot, purge) with **zero new lifecycle wiring** and inherits ADR-002's content-opacity Debug discipline. The fields are pure `u64`/`u32` counters — `derive`-safe metadata, like `elided_bytes`/`high_water` already are.

- **Ownership**: same as the buffer — `SessionState.transcript` Arc, handed to `TranscriptHold` on drain.
- **Lifetime**: created at session register; accumulates across deltas; survives the per-turn drain inside the hold (#4799/Wave B); read at review via a new metadata-only getter; zeroed/dropped at purge (cycle review / sweep / cap-evict) — the same terminal points the buffer purges at.
- **Hot-path latency budget**: one extra slice scan per delta (RegexSet) + a few integer adds, all under the *already-held* buffer lock. No new lock, no new allocation, no I/O. Strictly additive to the existing memcpy cost; the RegexSet pass dominates and is linear in delta size.
- **Interaction with `TranscriptBuffer`/`TranscriptHold` without holding content**: the accumulator reads `bytes` at fold time and keeps **only integers**. It never stores a slice, never returns bytes. At review, a new `snapshot()`-sibling — `activity_snapshot()` returning a small `Copy` struct of counters — feeds the persist path. **No content-bearing read is added to persistence** (see RQ-7).

**Crucial correctness point (the held route)**: because the accumulator lives *inside* `TranscriptBuffer`, the held-delta route (`session.rs:388-395`, which fetches the held Arc and calls `apply_delta`) folds into the **same** accumulator automatically. There is no separate "registry accumulator vs held accumulator" to keep in sync — the single Arc carries both buffer and counters. This is the decisive reason to co-locate inside the buffer rather than beside it in `SessionState`.

**Recommendation (RQ-5)**: Fold at `apply_delta`; co-locate the scalar accumulator **inside `TranscriptBuffer`** (additional `u64`/`u32` fields, metadata-only Debug); it rides the buffer Arc through the existing drain→hold→review lifecycle. New read surface is one metadata-only `activity_snapshot()` returning a `Copy` counter struct. No new lock, no new lifecycle, no content stored.

---

### RQ-6 — Durability + cycle binding (the hard part)

**The attribution chain is already solved by Wave B — fold-at-ingest inherits it rather than re-solving it. This is the key finding.**

The hold keys held buffers on `session_id` **but stamps each with its `feature_cycle`** at drain (`transcript_hold.rs:106-116`, set from `state.feature` at `session.rs:857-861`). It only holds buffers that *carry* an attributed cycle (empty cycle → not held, drain frees it, `transcript_hold.rs:229-233`). Re-adoption **fails loud on cycle mismatch** (`readopt_inner`, `transcript_hold.rs:259-302`, cite #981) — a buffer is never re-bound under the wrong cycle. So by the time review fires, every held accumulator is already bound to a *declared* cycle.

**Late-bind vs eager-bind decision → LATE-BIND, at review, via the hold's existing `feature_cycle` filter.**

- The accumulator keys on `session_id` at ingest (it has nothing else then — the #4828 UDS/HTTP split means even the session_id namespace varies by transport). It does **not** try to resolve a cycle at ingest.
- At review, `purge_held_for_feature(feature_cycle)` / `held_arcs_for_feature(feature_cycle)` (`transcript_hold.rs:331,412`) already select exactly the held sessions bound to the reviewed cycle. The accumulator read piggybacks on **the same filter** — sum the per-session counters of every session the hold reports for that `feature_cycle`.
- **Why not eager-bind** at cycle-declare time: eager-bind would require mutating the accumulator's key when `set_feature_force` fires, re-keying mid-stream, and handling the topic_source/#4828 cases where the cycle resolves late or the session-id namespace differs by transport. The hold *already* did this work correctly (stamp-at-drain + fail-loud re-adopt). Re-implementing it in the accumulator would duplicate the #981-hardened logic and risk diverging from it. **Late-bind reuses the settled, fail-loud machinery.**

**Multi-session accumulation**: at review, `take_transcripts_for_feature` already collects *every* session (registered ∪ held, dedup by Arc identity, `session.rs:474-493`) attributed to the cycle. The accumulator sum is the **same collection** with counters read instead of snapshots: `cycle_total = Σ session_accumulator` over that set. Because each session's accumulator rode its own buffer Arc, the per-session integers are independent and simply add. Multi-session is correct **by construction**, same as the snapshot union.

**The unmatched-session / fail-loud-drop case (ass-077 RQ-5)**: a held session whose cycle never matches the reviewed cycle (mismatch on re-adopt, or never reviewed) is **dropped with its terminal purge audit** (`TRIGGER_READOPT_MISMATCH` / `TRIGGER_STALE_SWEEP` / `TRIGGER_CAP_EVICT`, `transcript_hold.rs:57-60`). Its accumulator dies with the buffer — its bytes/tokens are **not** attributed to any cycle. This is the correct fail-loud behavior: **never attribute counters to a cycle the session didn't declare.** The drop is audited (counts-only, content-free), so the loss is visible, not silent — satisfying ass-077's "fail-loud drop, never fake-zero" requirement. A cycle row therefore reflects only sessions that genuinely declared it; the `raw_signals_available` flag already on `CycleReviewRecord` signals when held state was present vs purged.

**Where the per-session accumulator is parked between drain and review without holding content**: inside the held `TranscriptBuffer` in `TranscriptHold` (RQ-5). It survives session close exactly as the buffer does (the whole point of Wave B), and holds **only integers** — content was dropped at fold time. The hold's bounded-memory guarantees (`max_sessions` cap + stale-sweep TTL, `transcript_hold.rs:14-21`) bound the accumulator state too, since it lives inside the same struct.

**The `cycle_review_index` write + `SUMMARY_SCHEMA_VERSION` path** (pattern #4178 + #4750):

1. Add the activity integer columns to `CycleReviewRecord` (`cycle_review_index.rs:57`) — e.g. `transcript_bytes_total`, `transcript_token_estimate`, `reload_count`, plus the keyed class-counts (RQ-4) as a small integer map or fixed columns. **Exactly the crt-047 shape** (seven curation integers added the same way).
2. Extend the single `store_cycle_review()` INSERT/UPDATE (`cycle_review_index.rs:233-294`) with the new bind params. **No second write site** (#4178).
3. **Bump `SUMMARY_SCHEMA_VERSION` 3 → 4** (`cycle_review_index.rs:35`) so stale memoized rows without the new fields are recomputed, and update the pinned-version test (`cycle_review_index.rs:541-548`). Migration adds the columns with `DEFAULT 0` (crt-047 migration is the template).
4. The compute that fills these fields is a new side-effect in `context_cycle_review`, gated at **all four success returns** (#4750) — factor it into one helper called on `result.is_ok()` at each of: purged-signals path, cached-MetricVector path, memoization-hit, full-pipeline dispatch.

**Ordering hazard to flag for the design session**: the accumulator read and the buffer purge both fire at cycle review. The counter sum **must be read before** `purge_held_for_feature`/`clear_transcripts_for_feature` zero the buffers (RQ-5: purge drops the accumulator with the buffer). This is a sequencing constraint in `purge_cycle_transcripts`, not a new mechanism, but it is the one place a careless ordering yields zeros.

**Recommendation (RQ-6)**: **Late-bind** at review via the hold's existing `feature_cycle` filter — do not re-implement cycle resolution in the accumulator; reuse Wave B's stamp-at-drain + fail-loud re-adopt (#981). Sum per-session counters over the same registered∪held collection `take_transcripts_for_feature` uses. Unmatched sessions drop with their audited terminal purge — counters never mis-attributed, loss never silent. Persist via the crt-047-shaped column addition: extend `CycleReviewRecord`, single `store_cycle_review()` write, bump `SUMMARY_SCHEMA_VERSION` 3→4, fire at all four #4750 returns, **read counters before purge**.

---

### RQ-7 — Never-persist envelope proof: fold-at-ingest is categorically NOT ass-077's R-A

This is the load-bearing distinction. The proof is structural, in five claims, each backed by a code fact.

**The two mechanisms, precisely:**
- **ass-077's rejected R-A** = *read the held buffer at persist time and parse its content to derive a number.* The content-bearing read sits **on the persist path**: `take_transcripts_for_feature` → `snapshot()` returns `TranscriptSnapshot { bytes }` (`session_transcript.rs:93-96`) → parse those bytes → derive a number → persist it. R-A's objection (ass-077 RQ-7) is that this *adds a content-bearing read to the storage path*, eroding the structural guarantee that storage never touches conversation bytes.
- **Fold-at-ingest** = *parse content at the streaming boundary, drop the bytes, persist only integers.* The content-bearing read sits on the **ingest path** (`apply_delta`), which **already reads the bytes** for the merge. The persist path reads **only the integer counters** (`activity_snapshot()` returns a `Copy` struct of `u64`/`u32` — no `Vec<u8>`, no `bytes` field).

**The five-claim proof:**

1. **Content is folded at the streaming boundary and dropped.** The fold happens in `apply_delta` (`session_transcript.rs:150`) where `bytes: &[u8]` is already borrowed for the memcpy. After the fold + merge, the delta slice is gone — it was never owned by the accumulator. *Code basis: the accumulator stores `u64`/`u32`, never `&[u8]` or `Vec<u8>`.*

2. **Only integers persist.** The persist path is `activity_snapshot()` → integer fields on `CycleReviewRecord` → `store_cycle_review()`. There is **no `Vec<u8>` anywhere on this path.** The new columns are siblings of crt-047's seven integer columns, which already persist next to the leak gate without breaching it. *Code basis: `CycleReviewRecord` (`cycle_review_index.rs:57-90`) holds only scalars + the existing `summary_json` string, which fold-at-ingest does not extend with content.*

3. **No content-bearing read on the persist path.** This is the exact line R-A crosses and fold-at-ingest does not. The review-time read is `activity_snapshot()`, which returns **counters, not bytes**. `take_transcripts_for_feature`/`snapshot()` (the byte-bearing reader) is **not invoked by the persist path for these integers** — it stays on the response-only distillation path it already serves (ADR-002). *Code basis: the byte-bearing `TranscriptSnapshot.bytes` is consumed only by `distill_handler` → response (`distill_handler.rs:295`, attached to response, never to the stored row); the integer path is a separate, bytes-free read.*

4. **The structural leak gate is preserved.** `RetrospectiveReport` gains **no content field**; `test_candidates_structurally_absent_from_memoized_report` (`distill_handler.rs:775`) still holds — integer columns on `CycleReviewRecord` are not candidate/content fields and don't appear in `summary_json`. The gate that makes content-in-the-report **compile-impossible** is untouched. *Code basis: the new fields are typed `u64`/`u32`/`i64`, structurally incapable of carrying a transcript byte; the gate test asserts on `transcript_candidates`, which is unaffected.*

5. **The persisted columns are categorically the same as a network byte-counter (ADR-002 #4740, AC-06).** A `bytes_total` counter folded from a stream is indistinguishable, in content-opacity terms, from counting TCP bytes on a socket: both observe a byte stream and emit a count, retaining nothing. ADR-002's content-opacity is defined as *"no `Display`/`Debug`/`Result` carrying bytes; in-memory + purge IS the secrets guarantee."* A counter satisfies this by construction — it cannot carry a byte. The class-count signals (RQ-1 #3–5) are the same: a regex *match count* is a number, not the matched text; equivalent to a packet-classifier emitting "N retransmits" without storing packets.

**Where fold-at-ingest could still drift toward R-A — and the guardrail.** The one way to accidentally become R-A is to compute a signal that *needs the assembled buffer* (e.g., "deduplicate repeated content across the whole session") rather than a per-delta fold — that would force a content read at review. The guardrail: **every persisted signal must be expressible as a running fold over deltas (a counter), never as a query over the assembled buffer.** RQ-2's rejection of thrash/rolling-hash detection enforces this — it was the one candidate that leaned toward remembering content shape. Keeping the persisted set to pure counters keeps fold-at-ingest provably distinct from R-A.

**Recommendation (RQ-7)**: The line holds. Fold-at-ingest reads content only at the *already-content-bearing* ingest boundary, drops the bytes, and persists only integers via a bytes-free review read — categorically unlike R-A, which adds a content read to the persist path. Preserve the structural leak gate (no content field on `RetrospectiveReport`/`CycleReviewRecord`). **Enforce the guardrail: every persisted signal must be a running fold (counter), never a query over the assembled buffer** — this is the bright line that keeps the mechanism distinct from R-A.

---

### RQ-8 — Vision-lane + scope boundary

**This is a self-learning *process signal*, full stop. It surfaces knowledge; it does not govern execution.**

In-lane (what this feature does): persist content-opaque activity counters per cycle, and surface them as **knowledge** — e.g. *"this cycle ran N tokens (estimated), ~X% above the median for this feature-type"* — so agents and humans can see where the process runs heavy and improve it. This is the same self-learning bar as ass-077: trustworthy *information about the process.* The token estimate is explicitly a **relative** signal (RQ-3), surfaced as a comparison-to-baseline, never as a cost.

**Out of lane — explicitly excluded so the feature cannot drift into FinOps** (the vision's "Unimatrix is not an orchestration engine… does not manage workflows" line):

1. **No budget enforcement.** The system must never *block, throttle, or refuse* a cycle/session/agent for exceeding a token or byte threshold. Enforcement = orchestration; this feature only *observes and reports*. There is no "budget" config knob, no rejection path keyed on the counters.
2. **No cost dashboards as a product.** Surfacing the relative signal as knowledge (a number in the retro, a baseline comparison) is in-lane. Building a cost-monitoring dashboard, a spend view, or a billing-grade reporting surface as a deliverable is **out** — that is a FinOps product, not a self-learning signal. (A human reading the retro number is fine; a "cloud cost console" is not.)
3. **No scheduling-by-cost.** The counters must never feed an orchestrator that *decides what runs next* based on cost/volume (cheapest-first, defer-expensive, route-by-budget). Unimatrix does not schedule work; wiring cost into a scheduler would cross the orchestration-engine line directly.
4. **No exact-cost / billing attribution.** Per RQ-3, the number is an estimate, not a billing figure. Attributing dollar cost to a customer/cycle requires the `count_tokens` API + a billing pipeline — a separate product, out of scope, and the reason the estimate is framed as relative-only.

**The bright line**: the counters **inform** (surface as knowledge for humans/agents to act on); they never **control** (gate, schedule, bill, or enforce). The moment a counter is read by a path that *decides or blocks execution*, the feature has left the self-learning lane and entered orchestration/FinOps. The design session and any downstream review should treat "does this counter ever gate or schedule anything?" as the disqualifying test.

**Recommendation (RQ-8)**: Keep it a read-only, surface-as-knowledge process signal. Exclude budget enforcement, cost dashboards-as-product, scheduling-by-cost, and billing-grade cost attribution. The disqualifying test for any future consumer: *does this counter control execution?* If yes, it is out of lane.

---

## The challengeable hypothesis — is fold-at-ingest the right mechanism at all?

The SCOPE invites an honest verdict. **Verdict: YES for a deliberately small set (RQ-1 #1–5, RQ-2 reload latch); the mechanism is justified — but barely, and only because the plumbing already exists.**

- **For**: it dissolves ass-077's RQ-5 buffer-availability gap (counters survive multi-turn/multi-session by construction); it captures throughput signal no durable stream carries (PostToolUse is tool I/O, not turn volume); it rescues the demoted-but-valued reload metric into a durable integer; and the entire lifecycle (fold site, accumulator parking, cycle-attribution, durable landing) **already exists for the buffer** — the marginal cost is a slice scan + integer adds + a column addition.
- **Against / honest deferrals**: most *candidate* signals don't earn their cost (RQ-1 #6–8, RQ-2 percentiles + thrash all deferred). The token number is only ever relative (RQ-3). If the only signal that survived scrutiny were "total bytes," the per-delta machinery would be over-engineered relative to just counting deltas at drain. The mechanism earns its keep specifically because of (a) the **reload latch** (genuinely needs the streaming boundary and is high-value) and (b) the **shared-pass class-counts** (cheap once, decision-relevant). Strip those two and the case weakens to "nice-to-have byte counter."

So: **build it, but small.** The recommended v1 is ~4 counters + 1 latch + a capped class-count map. That is the set where decision-value clears per-delta-cost-plus-state. Everything else is deferred with cause.

---

## Unanswered Questions

- **Is crt-052 Wave B actually wired in the running deployment?** (Carried from ass-077.) The entire RQ-5/RQ-6 design assumes the held store is constructed with the hold handle (`main.rs:700,714,1236,1250` construct it; whether the deployed config path does is a wiring check). Without Wave B, accumulators drain per-turn exactly as buffers do and multi-turn collapses — the fold-at-ingest durability claim depends on Wave B being live. **Config/wiring verification, not a research question.**
- **Default value of N for the bytes/N token heuristic across this collective's actual transcript mix.** ~3.8 is a reasonable English-prose default, but the real ratio depends on how code-heavy / JSON-heavy this deployment's transcripts run. **Needs a measurement spike** (envelope-safe: instrument the byte counter + an offline `count_tokens` sample on a real cycle to calibrate N, then drop the content). Until then the estimate is directional-only, which RQ-3 already frames honestly.
- **Default domain-neutral catalog contents.** RQ-4 establishes the *governance shape*, but the specific shipped default class set (which behavioral signatures generalize across domains) needs a small design pass with the human — it is a product/domain judgment, not derivable from code. Out of scope here; flag for the design session.

## Out-of-Scope Discoveries

- **The "counter must be read before purge" sequencing is a reusable hazard class.** Any future review-time aggregate that reads held-buffer-resident state shares the trap: the purge at cycle review zeroes the source. Worth a convention note ("read all hold-resident aggregates before `purge_cycle_transcripts`") — recurs for any aggregate parked in the hold. Carry-forward, not pursued.
- **The accumulator-inside-buffer pattern generalizes beyond transcripts.** Co-locating O(1) fold state *inside* the content-opaque buffer (so it rides the buffer's Arc through every lifecycle path automatically, inheriting the content-opacity Debug discipline) is a clean pattern for "durable metadata derived from ephemeral content." Could apply to any future ephemeral-stream-with-durable-summary need. Carry-forward.
- **The "is a hash content?" question is worth a pinned decision someday.** RQ-2 deferred thrash-detection partly to avoid arguing whether a rolling hash of conversation bytes is "content" under ADR-002. A future ADR could settle the content-opacity status of fixed-width hashes/fingerprints of content — it recurs whenever someone wants a similarity/dedup signal. Not pursued; flag if thrash-detection is ever revisited.

## Recommendations Summary

- **RQ-1**: Ship per-delta signals 1–5 (bytes/delta-count, token estimate, and a single shared-pass RegexSet for error/refusal/re-read counts); defer 6–8 (turn proxy, entropy, language detection) — none clears decision-value vs per-delta cost or domain-neutrality. Output always an integer; fold both registered and held routes.
- **RQ-2**: KEEP one windowed signal — the durable reload counter + compaction latch (~5 bytes/session, the strongest single justification for the mechanism) — plus mean-turn-size as a zero-state byproduct. DEFER turn-size percentiles (reservoir state) and thrash/rolling-hash (state-heavy *and* content-fingerprint-adjacent; retry-count proxies it).
- **RQ-3**: bytes/N heuristic (N config-tunable, ~3.8), computed at read time from the byte counter; reject embedded BPE (heavier, opaque, still Claude-wrong). Frame as a **relative** efficiency signal, never a billing-grade count; exactness (billing) is out of scope.
- **RQ-4**: Externalize the regex catalog as a defaulted, `validate()`-bounded (≤~16 classes) config section (dsn-001 / #4591 precedent); compile once at load; ship a small domain-neutral default; persist class-counts as keyed integers. The cap primarily bounds *domain assumptions*.
- **RQ-5**: Fold at `apply_delta`; co-locate scalar counters **inside `TranscriptBuffer`** so they ride the buffer Arc through the existing drain→hold→review lifecycle with zero new wiring; expose one metadata-only `activity_snapshot()` returning a `Copy` counter struct. No new lock, no content stored.
- **RQ-6**: **Late-bind** at review via the hold's existing `feature_cycle` filter — reuse Wave B's stamp-at-drain + fail-loud re-adopt (#981), do not re-resolve cycles in the accumulator. Sum per-session counters over the same registered∪held set; unmatched sessions drop with their audited terminal purge (never mis-attributed, never silent). Persist via crt-047-shaped column addition: extend `CycleReviewRecord`, single `store_cycle_review()` write, **bump `SUMMARY_SCHEMA_VERSION` 3→4**, fire at all four #4750 returns, **read counters before purge**.
- **RQ-7**: Fold-at-ingest is categorically distinct from R-A — content read only at the already-content-bearing ingest boundary, bytes dropped, only integers persist via a bytes-free review read; no content-bearing read on the persist path; structural leak gate preserved; columns equivalent to a network byte-counter (ADR-002 AC-06). Guardrail: every persisted signal must be a running fold (counter), never a query over the assembled buffer.
- **RQ-8**: Self-learning process signal only — surface as knowledge ("N tokens, X% above median for this feature-type"). Exclude budget enforcement, cost dashboards-as-product, scheduling-by-cost, billing-grade attribution. Disqualifying test: *does this counter control execution?*
- **Hypothesis verdict**: Build it, but **small** — ~4 counters + 1 reload latch + a capped class-count map. Justified chiefly by the reload latch (high-value, genuinely needs the streaming boundary) and the cheap shared-pass class-counts; most other candidate signals deferred with cause.

**Proposed follow-up feature issues**: (1) Fold-at-ingest accumulator inside `TranscriptBuffer` (signals 1–5 + reload latch) + `activity_snapshot()`; (2) `CycleReviewRecord` activity-column addition + `SUMMARY_SCHEMA_VERSION` 3→4 + read-before-purge sequencing, fired at all four #4750 returns; (3) `[transcript_signals]` config catalog (defaulted, `validate()`-capped, domain-neutral default set); (4) token-heuristic N calibration measurement spike (envelope-safe, offline `count_tokens` sample).
