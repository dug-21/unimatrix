# FINDINGS: Client-Streamed Session Transcript — Fidelity, Attribution, and UDS-Path Simplification

**Spike**: ass-069
**Date**: 2026-06-04
**Approach**: Investigation + evaluation + targeted PoC (Q1 empirical)
**Confidence**: Actionable

---

## Executive Summary

**The gate passes.** Per-session attribution survives concurrency because attribution in this codebase is **already keyed entirely on the `session_id` string carried in the wire payload** — not on process lineage. Process lineage (`SO_PEERCRED`, `/proc/{pid}/cmdline`) is a **UID authentication** check (`auth.rs`), not a session-routing mechanism. The HTTP `/observe` path already mints a transport-scoped identity (`prefix_session_id` → `http-{session_id}`, `observe.rs`). Streamed transcript deltas ride the exact same keying. The PoC drove up to 128 concurrent mixed-transport sessions with interleaved, reordered, dropped, and duplicated deltas and produced **zero mis-attribution and zero cross-contamination**.

A second, load-bearing clarification reframes the whole spike: the "~90% heuristic" the SCOPE asks us to retire (`enrich_topic_signal`, `check_eager_attribution`) is **feature/phase attribution**, a *different problem* from session attribution. Session attribution is and remains exact. Streamed transcript is what finally lets us retire most of the *feature*-attribution heuristics, because feature/phase become readable from the authoritative conversation instead of voted from tool-call topic signals.

---

## Findings

### Q1: Attribution under concurrency — the gate (PoC required)

**Answer**: **GO.** Per-session attribution is correct under concurrency, by construction, for both transports, and is unaffected by streamed deltas, out-of-order arrival, drops, or duplicate delivery.

**Evidence (code)**:

1. **Session routing is a string-keyed HashMap, not process lineage.** `SessionRegistry` is `Mutex<HashMap<String, SessionState>>` keyed on `session_id` (`session.rs:159-161`). Every record path (`record_injection`, `record_topic_signal`, `record_rework_event`, …) looks up by `session_id` and is a **silent no-op for an unregistered key** (`session.rs:206-220` and throughout). There is no path by which a write keyed on session A can mutate session B.

2. **Process lineage is auth, not attribution.** `authenticate_connection` (`auth.rs:97-122`) uses `SO_PEERCRED` to verify the peer's **UID matches the server UID** (Layer 2) and reads `/proc/{pid}/cmdline` only as an **advisory** Layer 3 check that *logs but never rejects and never routes* (`auth.rs:112-119, 130-144`). The pid is never used to pick a session slot. The SCOPE's premise that "HTTP has no process lineage" is therefore not an attribution gap — UDS does not use lineage for attribution either.

3. **HTTP already mints a transport-scoped session identity.** `prefix_session_id` (`observe.rs:69-93`) rewrites every inbound `session_id` to `http-{session_id}` across `SessionRegister`, `SessionClose`, `RecordEvent(s)`, `CompactPayload`, and `ContextSearch`. This is exactly the SCOPE's candidate mechanism ("authoritative `session_id` minted at register and echoed on every subsequent frame") — it ships today. It also guarantees a UDS session and an HTTP session that happen to share a raw client id land in **distinct** registry slots (no namespace collision). The comment at `observe.rs:65-67` already names the enterprise evolution: `http-{subject_hash}-` per-user under OAuth.

4. **One-connection-per-session is NOT assumed.** The registry keys on the payload `session_id`, not on the socket/connection. UDS spawns a fresh per-connection task per *request* (`listener.rs:335`, `into_std`/`from_std` per call); HTTP is stateless POST. A single remote client multiplexing many sessions over one HTTP keep-alive connection is already handled — each request carries its own `session_id`. There is no connection→session binding to break.

**Evidence (PoC)** — throwaway, `/tmp/ass069-poc`, models the real keying (`Mutex<HashMap<String,_>>`, prefix-on-HTTP, silent-no-op-on-unregistered, per-session byte-offset merge). Each session tags every transcript byte with its own id; the verifier asserts every surviving byte in each buffer equals that session's tag.

| sessions | deltas/sess | drop% | dropped | mis-attr | contaminated bytes | total bytes |
|---|---|---|---|---|---|---|
| 2 | 50 | 0 | 0 | **0** | **0** | 5,328 |
| 8 | 200 | 0 | 0 | **0** | **0** | 96,984 |
| 8 | 200 | 20 | 308 | **0** | **0** | 77,778 |
| 32 | 500 | 30 | 4,807 | **0** | **0** | 719,319 |
| 64 | 1000 | 40 | 25,822 | **0** | **0** | 2,513,322 |
| 128 | 500 | 50 | 32,016 | **0** | **0** | 2,092,686 |

Plus three adversarial unit tests (all pass): (a) UDS `sess-X` and HTTP `http-sess-X` interleaved → distinct buffers, no bleed; (b) a dropped delta leaves a content hole but **never** back-fills with foreign bytes and never mis-attributes; (c) a delta for an unregistered session is a silent no-op that creates no slot and touches no other session.

**Reliability interaction confirmed**: attribution correctness does **not** depend on delivery. A dropped delta loses content (a gap bounded by byte offset) but cannot mis-attribute surviving content, because routing happens by key before any byte is written. This matches the existing fire-and-forget contract.

**Recommendation**: **GO.** Mint the authoritative `session_id` at `SessionRegister` (already done), echo it on every delta frame (the wire already requires `session_id` on `RecordEvent`/`CompactPayload`), and key the per-session transcript buffer on that id in `SessionState`. Add a per-session `transcript_high_water: u64` and apply deltas with an offset-bounded merge (PoC `apply_delta`) so out-of-order/duplicate deltas are idempotent. Do **not** introduce any connection→session binding. No new identity mechanism is required; the existing keying is the guarantee.

---

### Q2: Delta-streaming mechanism and bounding

**Answer**: Ship transcript deltas as a new fire-and-forget `RecordEvent` event-type (`event_type: "transcript_delta"`) carrying `{offset, bytes}`, with the client tracking a per-session byte offset against `transcript_path`. Cap individual deltas; truncate-with-marker at the cap. No 12KB assumption leaks.

**Evidence & design**:

- **Client offset tracking**: the transcript is append-only JSONL and `transcript_path` is on every hook event (`HookInput.transcript_path`, `wire.rs:59-60`). The client keeps `last_offset` per `session_id` (the new TS client is otherwise stateless, ass-068 Q2 — this is the one piece of per-session client state, derivable by `stat`-ing file size at each event and shipping `[last_offset, file_len)`; persist `last_offset` in the same `~/.unimatrix/{hash}/` dir the event queue uses). Ship-since-last-offset → server appends to the per-session buffer keyed on `session_id` (Q1).

- **Hot-path safety**: deltas ride **fire-and-forget** events only — never the sync trio (UserPromptSubmit, PreCompact, SubagentStart). The natural carrier is the already-fire-and-forget `PostToolUse`/`Stop`/`RecordEvent` path. ass-068 Q1 measured fire-and-forget as dominant (16-20 of ~24 spawns) and non-blocking against 2-30s model latency. A transcript delta is one extra fire-and-forget frame on events that already fire; it does **not** touch the ass-068 sync budget. **No regression to the latency budget.**

- **Volume bounding**: the wire already enforces `MAX_PAYLOAD_SIZE = 1 MiB` (`wire.rs:16`) and `write_frame`/`read_frame` reject anything larger. Recommend a **soft per-delta cap of 64 KiB** with **head+tail truncation** (keep first 48 KiB + last 12 KiB, insert a `…[N bytes elided]…` marker), well under the 1 MiB hard frame limit. What is lost at the cap: the middle of a single oversized tool result (e.g., a multi-MB file read). This is acceptable — the head carries the call/intent, the tail carries the outcome/error, and the distillation pipeline (Q5) cares about decisions and rework, not verbatim large blobs. Make the cap a config knob (ties to Q7 retention/policy seam).

- **Framing parity**: UDS uses 4-byte BE length-prefix (`wire.rs:300-355`); HTTP uses the JSON body. Both already carry arbitrary-size payloads up to 1 MiB. The **12KB tail is a self-imposed PreCompact injection budget** (`TAIL_WINDOW_BYTES = MAX_PRECOMPACT_BYTES × TAIL_MULTIPLIER = 3000 × 4`, `hook.rs:39-50`), not a transport limit. It lives only in `extract_transcript_block` (`hook.rs:1383-1392`) and does not constrain a delta-streaming path. **No 12KB assumption leaks.**

**Recommendation**: Add `event_type: "transcript_delta"` to the fire-and-forget `RecordEvent` family, payload `{ "offset": u64, "bytes": "<text>" }`. Server: new `SessionState.transcript` buffer + `transcript_high_water`, offset-bounded merge (Q1 PoC). Client: per-session `last_offset` persisted next to the event queue; ship `[last_offset, file_len)` on each fire-and-forget event, soft-cap 64 KiB with head+tail truncation. Reuse the existing 1 MiB frame guard as the hard ceiling.

---

### Q3: UDS-path simplification given the authoritative log *(Doug Q1)*

**Answer**: The authoritative transcript lets us retire the **feature/phase heuristic stack** (not the session-attribution machinery, which is already exact) and **shrink — not delete — the hook event set**. Most of the 13 events stay because they carry signals the transcript cannot give cheaply (discrete timing, failure flags, lifecycle boundaries). The net is *modest deletion plus quality uplift*, not a wholesale teardown.

**What becomes redundant (feature/phase attribution)**:

- `enrich_topic_signal()` (`listener.rs:143-174`) — the registry-fallback that fills `topic_signal` from `state.feature` when the hook didn't extract one. With the transcript in hand, feature/phase/topic are **read from the conversation** (the agent's prompts, the SM protocol headers, `cycle_start` markers in text) at cycle-review time. The per-event enrichment becomes unnecessary for *durable* attribution.
- `check_eager_attribution()` (`session.rs:397-426`) — the 3+/>60% threshold vote, and `majority_vote_internal` (`session.rs:565-598`) at sweep — exist to *guess* the feature mid-session from accumulated topic tallies. The transcript makes the feature **observable**, so the guess can be replaced by a read.
- The `topic_signals: HashMap<String, TopicTally>` accumulation (`session.rs:127, 432-445`) and the `#198` eager/payload feature-set plumbing (`listener.rs:793-847`) shrink to a thin fallback.

**Recovered accuracy**: today ~90% via process-lineage-adjacent + majority vote. Reading the explicit `cycle_start`/phase markers and SM protocol text directly from the transcript moves feature/phase attribution toward **near-100%** for protocol-driven sessions (the markers are literally written into the conversation). Estimate: **~90% → ~98-99%** for sessions that run the swarm protocols; the residual is ad-hoc sessions with no explicit markers, where a fallback heuristic still earns its keep. **Keep the heuristics as a degraded fallback; demote them from primary to secondary.**

**Hook-event redundancy analysis (the 13-event surface, ass-064 catalog)**:

| Event | Recoverable from transcript? | Verdict |
|---|---|---|
| SessionStart / Stop / TaskCompleted | No — lifecycle boundary + outcome | **Mandatory** (registers/closes the session, triggers purge) |
| UserPromptSubmit | Yes (prompt is in transcript) **but** it is a **sync injection** event | **Mandatory** — needed for proactive injection, not for observation |
| PreCompact | Partially (transcript_excerpt now server-side) | **Mandatory** — sync compaction-defense injection; this spike makes it *better*, not removable |
| SubagentStart | Yes (subagent spawn visible) **but** sync injection | **Mandatory** — needed for subagent context injection |
| PreToolUse | Yes — tool call appears in transcript | **Candidate for reduction** — keep only where it intercepts `cycle_start`/`cycle_stop` |
| PostToolUse | **Partially** — tool *result* is in transcript, but `had_failure` flag + precise timing are discrete signals the detection rules consume | **Keep** — feeds ReworkEventsRule, ToolFailureRule, co-access; cheaper as a flag than re-parsing transcript |
| PostToolUseFailure | Discrete failure signal | **Keep** — `ToolFailureRule` (col-027) wants the per-tool count |
| SubagentStop | Marginal | **Reducible / nice-to-have** (ass-064 already tiers it nice-to-have) |
| Ping | Health | **Keep** (orthogonal) |
| cycle_start / cycle_stop | Yes — but they *force* feature/phase state synchronously | **Keep** — authoritative lifecycle markers; cheaper and more reliable than transcript scraping |

**Minimal-necessary hook set** (what *must* stay regardless): SessionStart, Stop/TaskCompleted, UserPromptSubmit, PreCompact, SubagentStart, PostToolUse (+PostToolUseFailure), cycle_start/cycle_stop, Ping. **Plausibly retirable/merged**: standalone PreToolUse observation (keep only the cycle-event interception), SubagentStop. The transcript's real dividend is not *fewer events* — it is **richer feature/phase attribution from the same events** plus the authoritative conversation for distillation.

**Before/after of the observation surface**:
- *Before*: 13 events; feature/phase reconstructed by majority-vote heuristics (~90%); PreCompact reads a 12KB local tail; remote skips transcript entirely.
- *After*: ~11 events (PreToolUse demoted to cycle-interception-only, SubagentStop optional); feature/phase **read** from the authoritative transcript (~98-99% on protocol sessions, heuristic kept as fallback); full conversation available server-side on both transports; PreCompact restoration works remotely.

**Recommendation**: Demote `enrich_topic_signal` / `check_eager_attribution` / topic-tally voting from **primary** feature attribution to **fallback** (used only when the transcript yields no explicit marker). Keep the full sync-injection event set and the discrete-signal events (PostToolUse, PostToolUseFailure, cycle_*). Retire standalone PreToolUse *observation* (keep its cycle_start/cycle_stop interception) and make SubagentStop optional. Do not delete the heuristics — degrade them.

---

### Q4: Ephemerality and secrets handling

**Answer**: Buffer the raw transcript **in-memory** in `SessionState` (`Arc`-shared `SessionRegistry`, principle 7), never spill to disk. Purge on session close / cycle close. The audit log records *that* a streamed session existed and was purged — never its content.

**Evidence & design**:

- **Placement**: `SessionRegistry` is already an in-memory `Mutex<HashMap>` shared via `Arc` (`session.rs:159`, `listener.rs:259`). The transcript buffer is a new `SessionState.transcript: Vec<u8>` (or `String`) field, same lifecycle as `injection_history`/`topic_signals` — all of which are documented "in-memory only; never persisted; reset on register_session" (`session.rs:131-151`). **Do not disk-spill.** Raw transcript may contain secrets/keys (principle 8: no secrets in any database); the cleanest secrets posture is to never write it to durable storage at all. Crash recovery is not worth the exposure: a crash mid-session simply loses that session's not-yet-distilled transcript, which degrades to today's behavior (server reconstructs from observations). The PoC confirms drops/losses never corrupt attribution.

- **Purge triggers**: the registry already removes session state on close — `drain_and_signal_session` (`session.rs:475-490`) `remove`s the key, and `sweep_stale_sessions` (`session.rs:501-534`) evicts stale sessions after 4h. The transcript buffer rides this exact lifecycle: removing the `SessionState` drops the `Vec<u8>` and frees the heap allocation. **Cycle close** (`context_cycle_review`, drains on call — `server.rs:67`) is the point where the meaningful bits have been distilled (Q5) and the raw buffer should be explicitly cleared even if the session stays open. "Genuinely scrubs" = `Vec::clear()` + the buffer goes out of scope; since it never touched disk, there is no file to shred.

- **What survives**: only distilled knowledge — the detection-rule findings and any lessons/patterns extracted at cycle review, which already flow through `context_store`/the existing pipeline. The raw transcript is working state, not a durable artifact (matches the SCOPE's privacy-by-construction goal).

- **Audit log**: append-only (principle 2). It should record a `transcript_session_purged` event with `session_id`, byte count, and timestamp — metadata only, **never the content**. This mirrors the existing `uds_auth_failure` audit pattern (`listener.rs:402-414`) which logs the event without sensitive payload. A `RetentionConfig` and `gc_audit_log(retention_days)` already exist (`config.rs:1501`, `retention.rs:271`) — the audit entry slots into that existing GC policy.

**Recommendation**: Add `transcript: Vec<u8>` (or bounded ring) to `SessionState`, in-memory only, never persisted. Purge on `drain_and_signal_session` / sweep (automatic via key removal) and explicitly `clear()` at `context_cycle_review`. No disk spill — accept that a crash loses the in-flight raw transcript (degrades to reconstruction, not data corruption). Emit a content-free `transcript_session_purged` audit event under the existing append-only audit log + `RetentionConfig` GC.

---

### Q5: Cycle-review distillation quality

**Answer**: The existing **23 detection rules consume structured `ObservationRecord`s, not raw text** — so they need **no rework** and gain nothing directly from the transcript. The uplift comes from a **new, additive distillation pass** that reads the authoritative transcript for decisions/rework-narrative/phase-narrative that the metric rules cannot express. Run it server-side at `context_cycle_review`.

**Evidence**:

- The detection pipeline (`detection/mod.rs:48-82`, `default_rules`) is 23 `Box<dyn DetectionRule>` over `&[ObservationRecord]` (`detect_hotspots`, `mod.rs:27-36`). Rules are structured-metric: `OrphanedCallsRule`, `ToolFailureRule`, `ReworkEventsRule`, `FileBreadthRule`, `RereadRateRule`, `EditBloatRule`, `SourceFileCountRule`, `PhaseDurationOutlierRule`, etc. They read counts, breadths, timing, and tool names from observations (`source.rs` loads `ObservationRecord` by feature/cycle). **None parse conversation prose.** Feeding raw transcript *into these rules* is neither needed nor helpful — they would not know what to do with it.

- Therefore the SCOPE's framing ("real transcript vs reconstructed observations" for the rules) is a slight mismatch: the rules already run on real observations (actual tool calls), and those are *not* reconstructed today — they are recorded directly. What today's path *reconstructs* poorly is the **conversational narrative** (the *why* behind decisions, rework reasoning, phase intent) — and **that** is what the transcript supplies.

- "Save the meaningful bits" concretely extracts: (1) **decisions** (architecture/design choices stated in the conversation — already a `context_store` category), (2) **rework narrative** (the agent's stated reason for re-editing, complementing the structural `ReworkEventsRule`), (3) **patterns/lessons** (gotchas surfaced in dialogue), (4) **phase narrative** (`phase_narrative.rs` already exists in unimatrix-observe — it currently works from observations; the transcript makes it far richer). Extraction runs **server-side at `context_cycle_review`** (`server.rs:67`, drains on call), then the raw buffer is purged (Q4).

**Recommendation**: Do **not** rework the 23 detection rules — they stay on `ObservationRecord`. Add an **additive transcript-distillation step** at `context_cycle_review` that extracts decisions, rework narrative, patterns, and phase narrative from the in-memory transcript buffer and feeds them to the existing `context_store`/synthesis path (`synthesis.rs`, `phase_narrative.rs`). The uplift is qualitative (the *why*), not a change to the quantitative rules. This is where #670's buffer/distill machinery is reused (Q6).

---

### Q6: Reconciliation with #670

**Answer**: Client-streamed transcript **absorbs** #670. Keep #670's buffer/distill/purge *destination and machinery*; replace its data source (observation-reconstruction) with the real streamed transcript. Reconstruction survives as the **degraded fallback** when streaming is unavailable or capped.

**Evidence**: #670 ("server-side session transcript buffer — iterative content accumulation from observation events") and this spike target the **same buffer destination** (the per-session server-side transcript) with **different data sources**: #670 reconstructs from observation events; this spike streams the real JSONL. ass-068 Q5 already named #670 as the strategic convergence point for transcript handling. The buffer field (Q4), the purge lifecycle (Q4), and the distill step (Q5) are exactly what #670 would have built — so this spike does not duplicate #670, it **supplies #670's buffer with better data**.

- **Fallback role**: reconstruction is valuable precisely where streaming degrades — (a) a delta is dropped (content hole), (b) a delta is truncated at the 64 KiB cap (Q2), (c) a pre-streaming client. In those cases the server still has the observation events and can reconstruct the missing span. So reconstruction is not dead; it is the **floor** beneath streaming. This is graceful degradation, consistent with the exit-0/fire-and-forget posture (ass-064).

**Recommendation for #670 disposition**: **Re-scope #670, do not close it.** Reframe its description from "iterative content accumulation from observation events" to "server-side session transcript buffer (primary source: client-streamed deltas; fallback: observation reconstruction); distill at cycle review; purge on close." It becomes the buffer/distill/purge chunk that this spike's streaming feeds. Reconstruction moves from #670's *primary* mechanism to its *fallback* mechanism.

---

### Q7: Enterprise extension seams

**Answer**: All four seams either exist or are one-field additions. Carry `(tenant, project)` keying from day one; "delete on cycle close" is the default of the **already-existing** `RetentionConfig`; the `/observe` endpoint already passes the capability check behind the bearer seam; the audit gap for fire-and-forget is real and named below.

**Evidence & checklist**:

- **Tenant dimension**: the HTTP session id is already transport-namespaced (`http-{session_id}`, `observe.rs:69`) with the explicit documented evolution to `http-{subject_hash}-` per-user/tenant under OAuth (`observe.rs:65-67`). Recommendation: make the prefix a structured `(tenant, project, session)` key from day one, OSS populating `tenant = "default"`, so multi-tenant is a *populated dimension*, not a re-key. The registry HashMap key becomes the composite string; no structural change.

- **Retention as policy**: `RetentionConfig` (`config.rs:1501`) with per-data-type `*_retention_days` fields and a background GC (`background.rs`, `retention.rs:gc_audit_log`) **already exist** (crt-036). "Delete raw transcript on cycle close" is the OSS **default value** of a new `transcript_retention` knob in this struct; enterprise sets retain-N-days / encrypt-at-rest / data-residency by changing config. The seam is not new architecture — it is one config field on an existing policy object.

- **Capability gating**: `/observe` already routes through the service-layer capability check — `dispatch_request` takes `capabilities: &[Capability]` and every write arm checks `Capability::SessionWrite` (`listener.rs:527, 541-546, 626-631, 663-668, 737-742`). The bearer seam (`BearerValidator`) sits in front of the HTTP listener (`http/listener.rs`). Transcript deltas ride `RecordEvent`, so they inherit `SessionWrite` gating and the bearer check for free. **No new auth surface.**

- **Known gap — auditability (named, not solved)**: transcript deltas are **fire-and-forget** (RecordEvent family) — low delivery guarantee by design (ass-064; principle: exit-0 degradation). For OSS this is fine: a dropped delta loses content, never mis-attributes (Q1). **For enterprise audit confidence this is insufficient** — a compliance audit demanding a complete, acknowledged-and-delivered record of every conversation byte cannot rely on a write path that may silently drop. The seam is insufficient at exactly the **delivery-guarantee layer**: enterprise would need a separate acknowledged/replayed write path (the event-queue replay machinery, ass-068 Q5, is the natural starting point) with at-least-once semantics and gap detection via the byte-offset high-water (Q1/Q2). **This spike names the gap and stops** — designing the enterprise acknowledged-delivery path is out of scope (SCOPE constraint).

**Recommendation**: Use a composite `(tenant, project, session)` registry key (OSS tenant = default). Add a `transcript_retention` field to the existing `RetentionConfig` with default = purge-on-cycle-close. Let deltas inherit the existing `SessionWrite` capability + bearer gating. Document the fire-and-forget audit gap and point enterprise at the event-queue replay path as the future at-least-once seam — do not build it here.

---

## Roadmap Fit (feeds ass-068's chunked migration)

This spike slots cleanly into the ass-068 five-chunk migration without reordering it:

- **ass-068 Chunk 1** (wire codegen + content negotiation): add the `transcript_delta` event-type to the wire contract here, so ts-rs codegen carries it from day one. Add `transcript_retention` to `RetentionConfig`.
- **ass-068 Chunk 2** (TS HTTP client): the TS client gains per-session `last_offset` tracking + ship-since-last-offset on fire-and-forget events. This is ass-068's Q5 transcript-reader question, answered at higher ambition: **stream deltas continuously** instead of reading a 12KB tail reactively at PreCompact. Remote fidelity reaches local parity (closes the #4676 PreCompact-restoration gap).
- **ass-068 Chunk 3** (TS UDS client): same delta path over UDS framing.
- **New server chunk (was #670)**: `SessionState.transcript` buffer + offset-merge + purge lifecycle + cycle-review distillation. Independently shippable; can land before or alongside Chunk 2.
- **ass-068 Chunk 5** (Rust hook retirement) is unaffected.

**ass-066 impact (stated, not designed here)**: client-streamed transcript gives the server the **full authoritative conversation over either transport without hosting the session**. This removes session hosting's "strictly superior fidelity for observation" argument (ass-066 Q2). ass-066's remaining unique value collapses to **proactive / inter-turn injection (control)** — the six injection-capabilities, not the observation ones. That is a smaller, far more deferrable prize, and it preserves the single-edge-language decision (no Python — ass-066's Python recommendation stays rejected, ass-068 Q2). **Recommendation: shrink ass-066 to injection-only and defer it; do not build `unimatrix run` for observation.**

---

## Unanswered Questions

1. **Transcript JSONL schema stability across host CLIs** — the delta path ships raw transcript bytes; Claude Code's JSONL format is known (`build_exchange_pairs`, `hook.rs:1205`), but Codex/Gemini transcript formats differ. The distillation pass (Q5) must parse per-provider. Not blocking for Claude Code; needs a per-provider parser when those clients stream. (Carries ass-066 Q1 / ass-064 RQ-4 multi-client context.)

2. **Exact distillation extraction quality** — Q5 asserts the transcript yields decisions/rework-narrative/phase-narrative; the *quality* of an automated extractor (LLM-based vs rule-based) was not measured here (no extractor exists yet). Requires a separate measurement spike once the buffer ships and real transcripts accumulate.

3. **Cumulative delta volume over a long session** — the in-memory buffer (Q4) grows with the full session transcript. For a multi-hour delivery session this could be tens of MB in RAM per session. The 64 KiB per-delta cap bounds *deltas*, not the *accumulated buffer*. A bounded-ring or periodic-distill-and-truncate policy may be needed; not quantified here. Flag for the #670 server chunk.

---

## Out-of-Scope Discoveries

1. **The "~90% attribution" framing conflates two problems.** Session attribution is exact (string-keyed); only *feature/phase* attribution is heuristic at ~90%. Future scoping should keep these separate — they have different mechanisms, different failure modes, and different fixes. (Likely a Unimatrix lesson once validated by the simplification delivery.)

2. **`prefix_session_id` is a ready-made multi-tenant seam.** `observe.rs:65-93` already documents the OAuth `http-{subject_hash}-` evolution. Worth promoting to a first-class composite-key type rather than string concatenation before enterprise lands — reduces re-key risk. (Carry-forward to enterprise design.)

3. **`RetentionConfig` + background GC (crt-036) is a general policy seam already in place** for observations, audit log, and sessions. Any future "ephemeral working state vs durable knowledge" feature should extend it rather than hardcode lifetimes. (Pattern candidate.)

4. **Process-lineage Layer 3 is advisory and currently a near-no-op** (`auth.rs:130-144` accepts any non-empty cmdline). It is neither attribution nor real authorization today. If hardening is ever wanted, that is the place — but it is unrelated to this spike and should not be touched as part of it.

5. **`phase_narrative.rs` already exists** in unimatrix-observe and is the natural home for transcript-fed phase distillation — no new module needed for that part of Q5.

---

## Recommendations Summary

- **Q1 (Attribution gate)**: **GO.** Attribution is string-keyed on `session_id` (not process lineage); HTTP already mints `http-{session_id}`. PoC: 0 mis-attribution / 0 contamination across <=128 concurrent mixed-transport sessions with reorder, <=50% drop, and duplicates. Key the transcript buffer on the same id; add offset-bounded merge; no connection->session binding.
- **Q2 (Delta mechanism)**: Add fire-and-forget `transcript_delta` events `{offset, bytes}`; client tracks per-session `last_offset` vs `transcript_path`; soft-cap 64 KiB head+tail truncation under the existing 1 MiB frame guard; rides existing fire-and-forget path -> no sync-budget regression. 12KB tail is PreCompact-only, does not leak.
- **Q3 (Simplification)**: Demote `enrich_topic_signal`/`check_eager_attribution`/topic-vote from primary feature attribution to **fallback** (transcript reads feature/phase -> ~90%->~98-99% on protocol sessions). Retire standalone PreToolUse *observation* (keep cycle-event interception), make SubagentStop optional; keep all sync-injection + discrete-signal + lifecycle events. Net: modest deletion + attribution-quality uplift.
- **Q4 (Ephemerality/secrets)**: In-memory `SessionState.transcript` only — never disk-spill (principle 8). Purge via existing key-removal on close/sweep + explicit `clear()` at cycle review. Content-free `transcript_session_purged` audit event under existing append-only audit + RetentionConfig GC.
- **Q5 (Distillation)**: Do **not** rework the 23 detection rules (they consume structured `ObservationRecord`, not text). Add an additive server-side transcript-distillation pass at `context_cycle_review` for decisions/rework-narrative/patterns/phase-narrative; reuse `synthesis.rs`/`phase_narrative.rs`.
- **Q6 (#670)**: Client-streamed transcript **absorbs** #670 — reuse its buffer/distill/purge machinery, swap its data source from reconstruction to streaming; reconstruction becomes the degraded fallback. **Re-scope #670, do not close.**
- **Q7 (Enterprise seams)**: Composite `(tenant, project, session)` key (OSS tenant=default); `transcript_retention` field on existing `RetentionConfig` (default = purge-on-close); deltas inherit `SessionWrite` capability + bearer gating. **Named gap**: fire-and-forget delivery is insufficient for enterprise audit confidence — the seam is the delivery-guarantee layer; point enterprise at event-queue replay for at-least-once; do not build it here.
- **Cross-spike**: Shrink **ass-066 to injection-only** and defer (streaming removes its observation-fidelity argument; Python stays rejected). This spike answers **ass-068 Q5** at higher ambition and feeds its Chunk 1/2 + the re-scoped #670 server chunk; closes the **#4676** remote PreCompact-fidelity gap.
