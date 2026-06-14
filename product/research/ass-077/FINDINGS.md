# FINDINGS: Best utilization of the streamed session transcript — value within the never-persist constraint

**Spike**: ass-077
**Date**: 2026-06-14
**Approach**: investigation + design-space evaluation (read-code dominant; light empirical reasoning on the availability gap from drain/hold code)
**Confidence**: directional
**Tracking**: GH #741

---

## Orientation — what the code actually does (load-bearing for every RQ)

Three independent ground-truth streams reach `context_cycle_review`, with very different durability:

1. **SQL observation rows** (`observations` table) — `ObservationRecord { ts, event_type, source_domain, session_id, tool, input, response_size, response_snippet }`, loaded by the three-path lookup in `tools.rs:1976-2025`. **Durable**. PostToolUse rows carry `tool` + `input` identically to PreToolUse (`observation.rs:640-649`). This is the corpus the legacy per-session metrics and all hotspot detection read.
2. **The in-memory `TranscriptBuffer`** (`session_transcript.rs`) — streamed transcript, never persisted, content-opaque by construction (ADR-002 #4740). Read once at review via `take_transcripts_for_feature` (`session.rs:469-508`), distilled by `distill_before_purge` (`distill_handler.rs:48-140`), attached to the **response only** (`attach_to_response_assembly:281`), then purged. **Never touches a SQL write or `cycle_review_index`** — the persisted `RetrospectiveReport` has no candidate slot (structural leak gate test `test_candidates_structurally_absent_from_memoized_report`).
3. **`cycle_events`** (cycle_start / cycle_phase_end / cycle_stop) and **corpus deltas** (entries stored/corrected/deprecated in window) — **durable**, read at `tools.rs:2599` and `cycle_review_index.rs:313-390`.

Persisted aggregate store is `cycle_review_index` (one row per `feature_cycle`), written once via `store_cycle_review()` at `tools.rs:2864`, governed by `SUMMARY_SCHEMA_VERSION`=3 and pattern #4178.

**The #750 regression, precisely.** `compute_session_summaries` and `compute_context_reload_pct` (`session_metrics.rs:54,114,160,170,181`) filter `event_type != "PreToolUse"` and `continue`. The TS client (ADR-004 vnc-028) stopped emitting non-cycle PreToolUse. So every PreToolUse-gated counter computes over zero rows → **honest computation, dishonest output**: Calls 0 / Tools — / knowledge 0 / reload 0%. Cycle-level hotspots survive (read PostToolUse). Discriminator: vnc-034 Calls 224/45/1056 + 26% reload; vnc-035 all zero (lesson #4998).

---

## Findings

### RQ-1 — What is available to `context_cycle_review` (multi-session cycle)

Four signal classes, with fidelity / lifetime / drain-survival:

| Signal | Fidelity | Lifetime | Survives session close? | Survives per-turn drain (#4799)? |
|---|---|---|---|---|
| PostToolUse rows (`tool`,`input`,`response_snippet`≤500,`ts`) | High — exact tool+args+outcome | Durable | Yes | Yes (SQL) |
| PreToolUse rows | Was high; **now empty for TS-client cycles** | Durable when present | Yes | Yes |
| SubagentStart / cycle_* events | High (structural) | Durable | Yes | Yes |
| cycle_events (start/phase-end/stop, outcome, next_phase) | High — authoritative phase timeline | Durable | Yes | Yes |
| Corpus deltas (`feature_cycle`,`supersedes`,`status`,`updated_at`) | High — actual knowledge writes/curation | Durable | Yes | Yes |
| Transcript buffer content (raw conversation) | Highest *when present*; degrades via holes/elision; 0.81 reconstruction floor when empty | **Ephemeral** in-memory; purged at review/close/TTL/cap | **Only via crt-052 held store, on exact `feature_cycle` match** | **No, unless held** |

Decisive asymmetry: classes 1–5 are durable but **client-event-class-dependent** (the #750 fragility); class 6 is richest but ephemeral and response-only — collected, used once, discarded. Nothing distilled from the transcript is persisted anywhere today.

**Recommendation**: Durable layer (PostToolUse + cycle_events + corpus) = substrate for persisted aggregates; transcript = response-time enrichment that may *upgrade* (not ground) them. Never make a persisted retro signal depend on one hook event class again.

### RQ-5 — Availability / lifetime; streamed-vs-usable gap

Reasoned from drain/hold code (no live multi-session artifact exists in-repo; basis stated):

- **Single-turn**: buffer live+registered at review → Primary path, ~100% of streamed-and-retained bytes usable (minus ring-tail past 4 MiB + holes). Streamed≈usable.
- **Multi-turn**: each Stop calls `drain_and_signal_session` (`session.rs:834`). **Without Wave B** the registered buffer purges at every drain → empty at review → fallback (`distill_handler.rs:150`) → **Reconstructed 0.81 floor from PostToolUse**; all but the final turn lost to distillation. **With Wave B** the live Arc is handed to `TranscriptHold` (`session.rs:857-861`), deltas keep merging (`session.rs:388-395`), re-adopted on matching-cycle re-register (`transcript_hold.rs:259-302`) → **Primary preserved across turns**.
- **Multi-session**: each session's buffer held independently, snapshotted as registered∪held union (`session.rs:482-492`, dedup by Arc identity). Usable iff each session declared the matching cycle (re-adopt fails loud on mismatch/empty, #981), held store not TTL/cap-evicted (`transcript_hold_max_sessions`, stale sweep), and 4 MiB cap not exceeded.

**Quantified gap (directional, from code mechanics):**

| Scenario | Streamed | Usable (Primary) | Gap | Floor when total |
|---|---|---|---|---|
| Single-turn | N | ~N (≤4 MiB tail) | ring-tail+holes | n/a |
| Multi-turn, **no Wave B** | N over T turns | final turn tail only | **~(T−1)/T of bytes** → Reconstructed | 0.81 from PostToolUse |
| Multi-turn, **Wave B** | N over T turns | ~N (merged ≤4 MiB) | ring-tail+holes | — |
| Multi-session, Wave B, all matched | N over S sessions | ~N per session | per-session ring-tail + TTL/cap eviction | 0.81 per evicted |
| Multi-session, any mismatch/empty cycle | N | that session → 0 (fail-loud drop) | whole session | 0.81 from its PostToolUse |

**Second, harder gap on the Primary path: the distillation cap.** Full snapshot is read, but only marker-matched user/assistant blocks survive `select_candidates`, then **24 KiB/session** + **256 KiB/cycle** keep-earliest caps apply (`config.rs:1813-1825`, `select.rs:106`, `distill_handler.rs:222`). So "usable at review" ≠ "surfaced": of a 4 MiB Primary buffer, ≤24 KiB/session reaches the response. Fine for a response payload; **binding constraint** if the transcript ever became a persisted-aggregate source.

**Recommendation**: With Wave B the transcript is Primary-grade for multi-turn/multi-session; without it, multi-turn collapses to the 0.81 floor. Any transcript-leaning design MUST assume Wave B is wired and treat 24 KiB/256 KiB as the real ceiling, not the 4 MiB buffer cap.

### RQ-2 — What is worth aggregating (ranked by decision value)

| Rank | Aggregation | Decision value | Best source | Scope |
|---|---|---|---|---|
| 1 | Phase durations + transitions + rework loops | Highest — where the process leaks | cycle_events (durable) | per-cycle |
| 2 | Rework/failure session ratio | High — clearest "went badly" | SessionRecord.outcome (durable) | per-session→cycle ratio |
| 3 | Knowledge reuse vs fresh-store ratio | High — is the collective leveraged | corpus deltas + injection (durable) | per-cycle |
| 4 | Curation health (corrections, orphan deprecations vs baseline) | High — already shipped (crt-047), the model to copy | entries window (durable) | per-cycle |
| 5 | Context-reload around compaction boundaries | Medium-high — **transcript+PreCompact more faithful than PreToolUse file-overlap proxy** | transcript (native) > PostToolUse overlap | per-cycle (≥2 sessions) |
| 6 | Tool-mix per phase | Medium — diagnostic, rarely changes next run | PostToolUse (durable) | per-session→phase |
| 7 | Attribution completeness | Medium — trust gauge, low standalone action | discover_sessions_for_feature (durable) | per-cycle |
| 8 | Decision/lesson density (marker hits) | Medium — advisory, noisy; better as enrichment | transcript markers | per-cycle |

**Key judgment**: the top-4 highest-value aggregations are all from durable, non-transcript streams and mostly already computed. The transcript's unique contribution is rank 5 (faithful reload/compaction) and rank 8 (decision density) — valuable but not the load-bearing KPIs.

**Recommendation**: Persist ranks 1–5 as durable per-cycle aggregates on `cycle_review_index`. Keep 6–8 as response-time enrichment. Transcript's job = sharpen rank 5, add rank 8 — not become the substrate for 1–4.

### RQ-3 — Compare to PreToolUse `"*"` architecture (recoverable / better / lost)

Critical finding: **PostToolUse rows carry `tool` and `input` identically to PreToolUse** (`observation.rs:621-649`). Every legacy per-session metric is recoverable by changing one filter literal.

| Legacy metric | Measured | Recoverable from PostToolUse? | Better from transcript? | Genuinely lost? |
|---|---|---|---|---|
| Calls | per-session tool count | **Yes** (PostToolUse one per call) | No | No |
| Tools (read/write/exec/search/store/curate) | classify_tool over `tool` | **Yes** (`tool` present) | No | No |
| Top file zones | extract_file_path over `input` | **Yes** (`input` present) | Marginally | No |
| knowledge served/stored/curated | context_* counts | **Yes** (same filter on PostToolUse) | No | No |
| context_reload % | cross-session file overlap | **Yes** (PostToolUse overlap) | **Yes — more faithful**: reload is transcript+PreCompact-tail-native, not a file-overlap proxy | No |
| PreToolUse-vs-PostToolUse divergence (blocked call) | — | n/a | n/a | Minor; never produced anyway |

**Bottom line**: nothing of value is lost by abandoning PreToolUse. Calls/Tools/zones/knowledge all recoverable from PostToolUse; `context_reload` is measured *better* from the transcript. Don't reproduce old numbers — produce better signal.

**Recommendation**: Re-ground per-session metrics on PostToolUse (durable survivor); compute `context_reload` from transcript/PreCompact tail when available, fall back to PostToolUse overlap.

### RQ-4 — Where should the substrate live + #750 stopgap

**Substrate recommendation (folded question): PostToolUse-grounded durable aggregates on `cycle_review_index`, transcript as response-time enrichment — NOT the persisted substrate.**

Evaluated the scope's proposed inversion (crt-052 seam *pushes* a transcript-derived durable record) vs PostToolUse re-point, on process-signal value, trust, never-persist envelope:

- **Transcript cannot be the durable substrate without breaching the envelope.** Distilling derived aggregates from it is persistable in principle (RQ-7), but every high-value aggregation (ranks 1–4) is already available from durable, content-opaque streams that don't read conversation bytes. Persisting transcript-derived numbers would (a) add a content-bearing read on the persist path the structural leak gate currently forbids by construction, (b) make persisted aggregates depend on the ephemeral buffer's presence (Wave B wired, cycle matched, no TTL/cap eviction) — re-introducing the exact fragility #750 exposed with a different dependency, (c) be capped at 24 KiB/session of marker-matched text — a poor basis for stable KPIs.
- **PostToolUse is durable, event-class-robust ground truth** already feeding hotspots. Re-grounding per-session aggregations on it fixes #750 at the source, yields metrics equal-or-better than the legacy proxy (RQ-3).
- **Transcript's unique value is response-time, not persist-time.** It already flows as `transcript_candidates`; it can additionally sharpen the response-time `context_reload`. Keeps the highest-fidelity/highest-risk data on the never-persist/response-only path where ADR-002/AC-06 already guarantee opacity.

**Architecture + write-point:**
- **Compute**: in the full-pipeline block, change `compute_session_summaries`/`compute_context_reload_pct` to read **PostToolUse** (the filter literal in `session_metrics.rs`); add RQ-2 ranks 1–5 derived aggregates to the report.
- **Write-point**: the **existing single write** `store_cycle_review()` (`tools.rs:2864`), pattern #4178 — derived review-time aggregates on `cycle_review_index`, single writer, **bump `SUMMARY_SCHEMA_VERSION` 3→4**. No new write site, no cycle_events pollution.
- **Transcript role**: unchanged structurally — `distill_before_purge` keeps producing response-only candidates at all four returns (#4750); optionally compute response-time reload/compaction-fidelity from the same snapshot, attach to response, never to stored row.
- **Robustness to hook-set churn**: ground persisted aggregates on PostToolUse + cycle_events + corpus (three independent durable classes); add a regression guard test asserting the aggregation reads a non-empty event class for a representative TS-client cycle, so the next event-set change fails a test instead of silently zeroing.

Respects all four #4750 returns: the change is to *what aggregation reads* and *what columns persist*, both inside the single full-pipeline path and single `store_cycle_review` write — the four-return distill/purge lockstep is untouched.

**#750 stopgap (decoupled, lean): fail-loud / mark-unavailable, never fake-zero.**
- When PreToolUse rows are absent for a cycle that has PostToolUse/cycle activity, render per-session Calls/Tools/knowledge/reload as **"unavailable — per-session telemetry not collected for this client"** rather than `0 / — / 0 / 0%`.
- Detection: `attributed` has PostToolUse but zero PreToolUse → set an `unavailable` flag on the session-summary block; formatter prints the marker.
- Presentation-layer guard, ships in hours, no schema change, stops the "believable zero" misread. Strictly safer than Option A (re-point) or B (client re-emit) as a *first* move — it does not re-ground the retro on any single event class, it just stops lying.

**Recommendation**: Ship the fail-loud stopgap first (presentation guard). Then the durable re-grounding: PostToolUse-sourced per-session aggregates + ranks 1–5, persisted via existing `store_cycle_review` single write, `SUMMARY_SCHEMA_VERSION`→4, plus event-class regression test. Do NOT make persisted aggregates read transcript content.

### RQ-6 — Value extraction beyond the retro (ranked by value-within-constraints)

| Rank | Consumer | Marginal value | Within envelope? |
|---|---|---|---|
| 1 | #700 MARKER recovery | High — markers transcript-native; Primary/longer access raises recall; shares the exact `TranscriptSnapshot` reader | Yes (response-time; agent stores explicitly) |
| 2 | #703 attribution accuracy (AC-07) | High — content disambiguates cycle better than topic_source voting | Yes (derived attribution) |
| 3 | PreCompact tail fidelity | Medium-high — `contiguous_tail` already feeds it; Wave B spans turns | Yes (sanctioned content output) |
| 4 | crt-052 drift/learning candidates | Medium — already feed agent; marginal value is continuity (Wave B) | Yes (response-only) |
| 5 | `topic_source` ordering activation | Low-medium — improves reconstruction ordering; needs topic_source projected onto ObservationRecord (gap `reconstruct.rs:36-52`) | Yes (metadata only) |

**Recommendation**: Highest-leverage transcript investment = relying on/wiring crt-052 Wave B continuity so #700 and #703 get Primary-grade multi-turn access. Activate `topic_source` projection (small, metadata-only) to unblock reconstruction ordering. All five stay within envelope.

### RQ-7 — Security envelope: where the persistable line falls (constraint-relaxing options isolated)

Line = **derived aggregate vs raw content**, falling cleanly:
- **Persistable** (counts/rates/durations/phase boundaries, family-hint *counts*): RQ-2 ranks 1–7, marker density as a number. No conversation bytes → safe on `cycle_review_index`. crt-047 curation health is the precedent.
- **Not persistable** (raw content): `TranscriptCandidate.text`, verbatim blocks, snapshot `bytes`. Response-transient path only (ADR-002 #4740, AC-06). The structural leak gate (`RetrospectiveReport` has no candidate field) makes accidental persistence compile-impossible today — **preserve that gate.**

**Where a persistable aggregate still requires a content read**: marker-density counts require parsing the transcript. Fine *in the response path* (already happens). On the *persist* path it would add a content-bearing read to storage — allowed in principle (only a number persists) but **erodes the structural guarantee**.

**Constraint-relaxing options (flagged, NOT recommended — for human judgment):**
- **R-A**: persist transcript-derived aggregates (requires a content read on the persist path). Relaxes the structural leak-gate guarantee. **Flag for human.**
- **R-B**: enable `RetainDays` transcript retention (currently OSS-rejected as enterprise-only, `config.rs:1729-1737`). Moves raw transcript to at-rest storage; directly breaches never-persist. **Flag for human — out of scope to assume.**

**Recommendation**: Keep the line at derived-aggregate-from-durable-streams; persist nothing requiring a transcript-byte read on the storage path; preserve the structural leak gate. Treat R-A/R-B as explicit human decisions, not defaults.

### RQ-8 — Traffic justification

Directionally **yes, favorable, but value-per-byte is currently low because most streamed bytes never reach a usable distillation** (RQ-5). The delta stream is cheap and self-healing (ADR-004 vnc-026 #4759: deltas never queued, re-derived from offset; transcript file is the durable client-side source). Cost is server-side in-memory holding, hard-bounded at `transcript_buffer_max_bytes × transcript_hold_max_sessions`.

Value-per-byte rises / traffic falls without fidelity loss by: (1) **Wave B continuity** — same bytes, far higher usable fraction (multi-turn ~1/T → ~1); biggest lever, already shipped. (2) Client-side marker pre-filtering NOT advisable — moves classification to the edge (violates Constraint 6) and risks dropping context. (3) Tighten only if measured — caps already bound what is *used*; streamed volume bounded by 4 MiB ring-tail.

**Recommendation**: Don't change the streaming contract. Realize value via Wave B (raises usable fraction, zero extra traffic). Revisit traffic only on measured memory pressure.

---

## Unanswered Questions

- **Live empirical RQ-5 numbers**: no real multi-session cycle artifact (held-buffer logs / per-cycle byte tallies) exists in-repo to measure streamed-vs-usable directly. Gap reasoned from drain/hold mechanics. A design/delivery session could instrument `take_transcripts_for_feature` to emit per-session `bytes/elided/holes/provenance` (metadata-only, envelope-safe) on a real cycle to replace the directional table with measured values.
- **Is crt-052 Wave B actually wired in the running deployment?** Code supports it (`with_transcript_hold`); whether production constructs the registry with the hold handle decides whether multi-turn is Primary or 0.81-floor today. Config/wiring check, not a research question.

## Out-of-Scope Discoveries

- **"Believable zero" is a general anti-pattern, not just #750**: any metric filtering on a single `event_type` literal that silently yields 0 when that class is absent will mislead. Worth a lint/convention: aggregations over an event class must distinguish "measured zero" from "class absent." (The trap recurs every time the edge event set changes.)
- **`topic_source` shipped as a SQL column but not projected onto `ObservationRecord`** (`reconstruct.rs:36-52`), so reconstruction ordering is a no-op. Cheap metadata-only fix improving the 0.81 fallback.
- **The structural leak gate (candidates cannot reach `RetrospectiveReport`) is a strong, cheap safety property** worth preserving as a convention for any future persisted retro field — makes a whole class of content-leak bugs compile-impossible.

## Recommendations Summary

- **RQ-1**: Durable streams (PostToolUse + cycle_events + corpus) = persisted-aggregate substrate; transcript = ephemeral response-time enrichment. Never re-ground a persisted retro signal on one hook event class.
- **RQ-2**: Persist top-5 decision-value aggregates (phase durations/transitions, rework ratio, knowledge reuse, curation health, faithful reload) — all from durable streams; keep tool-mix/attribution/marker-density as enrichment.
- **RQ-3**: Nothing of value lost vs PreToolUse `"*"` — Calls/Tools/zones/knowledge recoverable from PostToolUse (carries `tool`+`input`); `context_reload` measured *better* from transcript. Produce better signal, not old numbers.
- **RQ-4 (substrate)**: Re-ground per-session aggregations on PostToolUse; persist ranks 1–5 via existing single `store_cycle_review()` write (`cycle_review_index`, #4178); bump `SUMMARY_SCHEMA_VERSION` 3→4; add event-class regression guard. Do NOT make persisted aggregates read transcript content.
- **RQ-4 (#750 stopgap, decoupled)**: Ship a presentation-layer fail-loud guard first — "unavailable" not fake-zero when PreToolUse absent but other activity exists. No schema change; safer than re-point/re-emit as first move.
- **RQ-5**: With Wave B transcript is Primary-grade for multi-turn/multi-session; without it multi-turn collapses to 0.81 floor (~(T−1)/T bytes lost). 24 KiB/256 KiB distillation caps — not 4 MiB buffer — are the real surfaced-content ceiling.
- **RQ-6**: Highest-leverage transcript investment = crt-052 Wave B continuity (lifts #700, #703 to Primary-grade); activate `topic_source` projection (metadata-only).
- **RQ-7**: Persistable line = derived aggregate vs raw content; persist nothing requiring a transcript-byte read on the storage path; preserve structural leak gate. Flag R-A (content read on persist) and R-B (`RetainDays` at-rest) as explicit human decisions.
- **RQ-8**: Keep the streaming contract; realize value via Wave B (same bytes, higher usable fraction). Revisit traffic only on measured memory pressure.

**Proposed follow-up feature issues**: (1) PostToolUse re-grounding of per-session aggregates + `SUMMARY_SCHEMA_VERSION`→4 + event-class regression guard; (2) #750 fail-loud presentation stopgap; (3) `topic_source` projection onto `ObservationRecord`; (4) optional metadata-only instrumentation of `take_transcripts_for_feature` for measured RQ-5 numbers.
