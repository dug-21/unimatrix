# crt-052 — Transcript-Fed Cycle Review Distillation: Specification

GH Issue: #689. Source: `product/features/crt-052/SCOPE.md` (design restart, approved/re-verified
2026-06-08). Scope risks: `product/features/crt-052/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-09).

This specification refines the SCOPE's AC-01..AC-13 into testable functional/non-functional
requirements with explicit verification methods, and defines the domain models, ubiquitous language,
and user workflows. Downstream consumers: architect, pseudocode, tester, risk strategist.

---

## Objective

Insert a snapshot-and-release distillation pass into `context_cycle_review`, ahead of the existing
transcript purge, so the conversational narrative (decisions, rework reasoning, phase intent, human
interventions) of a reviewed feature cycle is harvested before the per-session transcript bytes are
purged. The server *selects* whole marker-matched user/assistant blocks from the per-session
transcript buffers (rules select), attaches them as a response-transient `transcript_candidates`
section, and the calling agent performs all semantic extraction into `context_store` (agent
extracts). A server-only bounded held-buffer structure (Option B) keeps multi-turn buffers alive
across the per-turn drain so the primary path is non-empty; a labeled reconstruction fallback covers
empty/hole-ridden buffers at a documented fidelity floor.

---

## Ubiquitous Language

| Term | Definition |
|------|------------|
| **Reviewed cycle** | The `feature_cycle` argument of a `context_cycle_review` call. |
| **Attributed session** | A registry session whose `state.feature == Some(feature_cycle)` (primary-path selection) or whose stored observations are attributed to the reviewed cycle (fallback selection). |
| **Snapshot seam** | `take_transcripts_for_feature` — the take-shaped successor to `clear_transcripts_for_feature` (`session.rs:299`). Copies buffer bytes + loss metadata out per session under lock discipline, then (per the purge policy) clears. The **sole** new buffer content-reading path; #700 consumes it (Constraint 4). |
| **Candidate** | One whole user/assistant text block matched by ≥1 marker family, carried in the response. `TranscriptCandidate`. |
| **Marker family** | One of four pattern groups (decision phrases, rework signals, lesson markers, phase/gate markers — ~50 regex patterns ported from ass-070's `extractor.py`). |
| **Family hint** | Advisory, server-emitted tag on a candidate naming which families matched. Never authoritative — the agent re-classifies. |
| **Snapshot** | An owned copy of a session's buffer bytes plus its loss metadata, taken under lock and parsed strictly after every lock is released. |
| **Held buffer** | A transcript buffer surviving `drain_and_signal_session` in the server-only bounded holding structure (Option B); continues merging deltas; re-adopted on re-registration. |
| **Reconstruction fallback** | Degraded distillation input assembled from a session's stored `ObservationRecord`s when its buffer is empty/hole-ridden. Distillation-input only — never writes the byte buffer, never produces observation rows. |
| **Loss / elision** | `TranscriptBuffer::elided_bytes` (lifetime counter) + hole presence. Loss is surfaced, never silent. |
| **Provenance** | Per-candidate / per-session origin label: `primary` (buffer-sourced) vs `reconstructed` (observation-sourced, degraded). |
| **Tail-window equivalence** | vnc-025 ADR-002/ADR-008 buffer guarantee: full-content equality holds only below the 4 MiB cap; under ring-tail overflow the buffer converges on its tail window, not full content. The buffer is **not** lossless. |
| **Two-pipe boundary** | The invariant that transcript-delta bytes never reach `insert_observations_batch`; detection-rule inputs are bit-identical with distillation active. |
| **topic_source** | `observations.topic_source` column (`declared`/`extracted`/`registry-fill`/`vote`/NULL). A **soft** ordering preference for fallback selection — never a hard filter (SR-06). |

---

## Domain Models

### TranscriptCandidate (response-transient)

One selected user/assistant block. Carried in `transcript_candidates`; **never persisted** (AC-06).

| Field | Type | Notes |
|-------|------|-------|
| `session_id` | String | Source registry session. |
| `byte_offset` | u64 | Offset of the block within the snapshot bytes (provenance + ordering key). |
| `ts` | timestamp | Block timestamp from the JSONL record (ordering key). |
| `family_hints` | Vec<FamilyHint> | Advisory; which marker families matched. Non-empty. |
| `provenance` | Provenance enum | `Primary` (buffer) or `Reconstructed` (observation fallback). |
| `text` | String | The whole matched block, unwindowed. |

Ordering: chronological by `(ts, session_id, byte_offset)`. Selection is a **separate consumer** of
the snapshot seam, not baked into the seam (decided).

### Snapshot seam return type (reusable by #700 — Constraint 4 / SR-04)

`take_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptSnapshot>`.

`TranscriptSnapshot`:

| Field | Type | Notes |
|-------|------|-------|
| `session_id` | String | |
| `bytes` | owned raw bytes (e.g. `Vec<u8>` / `Bytes`) | The copied buffer content — the contiguous readable window. Owned so all parsing happens after lock release. |
| `elided_bytes` | u64 | Lifetime elision counter snapshot. |
| `hole_info` | hole-presence + (optional) ranges | Whether the snapshot has holes / is below the elided floor. |
| `high_water` | u64 | For loss/threshold calibration against ADR-002 semantics. |

Hard requirement: the return shape MUST expose owned raw bytes + per-session `elided_bytes`/`hole_info`
in a form #700 (MARKER recovery) can parse for markers **without re-reading the buffer** and **without
adding a second `contiguous_tail`-style reader** (Constraint 4). Candidate selection and #700 marker
recovery are two independent consumers of one snapshot.

### Held-buffer holding structure (Option B — SR-01/SR-02)

Server-only, no wire change. Keyed by session id. Holds `Arc<Mutex<TranscriptBuffer>>` for sessions
that have been drained but not yet purged.

Required design properties (bound by ACs / SRs, mechanism finalized by architect):
- **Bounded**: explicit held-session **count cap** (config knob) + independent **stale-sweep TTL**;
  memory bound = per-session byte cap × held-count, and held-count has a ceiling (SR-01). Eviction
  policy when the cap is hit is surfaced (not silent).
- **Re-adoption**: a held buffer rebinds to the **same** `feature_cycle` on re-registration; key
  derivation is first-class and **fails loud** (not silent) on mismatch (SR-02, cite #981).
- **Merge-while-held**: held buffers keep applying deltas (idempotent merge) for held sessions.
- **Purge points**: held buffers are purged post-distill at cycle review and at stale sweep; the
  moved `transcript_session_purged` audit fires **exactly once per held session** at review or sweep
  (SR-03 / AC-11).

### Provenance / loss-visibility metadata (candidates section)

Per-session metadata block accompanying the candidates, present whenever non-zero/active:

| Field | Type | Notes |
|-------|------|-------|
| `session_id` | String | |
| `elided_bytes` | u64 | From the snapshot; surfaced when > 0. |
| `has_holes` | bool | Hole presence from `hole_info`. |
| `provenance` | enum | `Primary` vs `Reconstructed`. |
| `fallback_used` | bool | True when reconstruction fallback supplied this session's input. |

### Marker-family set (ported, frozen for v1)

Four families, ~50 regex patterns ported from ass-070's `extractor.py`: **decision phrases**, **rework
signals**, **lesson markers**, **phase/gate markers**. Regex-class dependency only (AC-13). No
server-side semantic classification beyond hints (Non-Goal).

---

## Functional Requirements

Each FR is testable; verification appears in Acceptance Criteria. FRs trace to SCOPE Goals (G#),
Constraints (C#), and risks (SR#).

- **FR-1 (Snapshot under lock discipline; G1, C1).** At `context_cycle_review` success, for every
  registry session attributed to the reviewed `feature_cycle`, the server snapshots buffer bytes:
  Arc clones collected under a single registry lock; bytes copied under per-buffer locks; **all** JSONL
  parsing and marker matching execute strictly after every lock is released. (→ AC-01)

- **FR-2 (Take-shaped seam, single content reader; G1, C4, C5, SR-04).** `clear_transcripts_for_feature`
  becomes / gains sibling `take_transcripts_for_feature` returning `Vec<TranscriptSnapshot>`
  (owned bytes + `elided_bytes`/`hole_info`/`high_water` per session). It is the second and **last**
  buffer content reader; the return shape is reusable by #700 marker recovery without a second
  `contiguous_tail`-style reader. (→ AC-01, AC-11; verification AC-V-SEAM)

- **FR-3 (Candidate selection — rules select; G2).** A new pure, no-I/O, no-lock module parses snapshot
  bytes as Claude Code JSONL; keeps only `user`/`assistant` text blocks (drops
  tool_use/tool_result/thinking/command-noise); matches the four marker families; keeps matched blocks
  **whole** (no windowing); dedups; emits ordered candidates with `session_id`, `byte_offset`, `ts`,
  and advisory `family_hints`. (→ AC-02, AC-03)

- **FR-4 (Dual volume cap; G2, OQ-3 resolved).** Selection enforces BOTH a **per-session** volume cap
  (config knob, default 24 KB) AND a **per-cycle aggregate** cap (config knob). Both caps are
  independently configurable; exceeding either deterministically truncates further candidate inclusion
  (truncation rule defined by architecture; must be deterministic and testable). (→ AC-02)

- **FR-5 (Additive response section; G3).** The success response carries `transcript_candidates` as an
  additive optional field following the `RetrospectiveReport` optional-field pattern
  (`#[serde(skip_serializing_if)]`): **absent** (not null, not empty) from JSON when no session yields
  candidates. Pre-existing cycle-review fields and col-024/crt-033 behaviors are byte-unchanged when no
  transcripts exist. (→ AC-04)

- **FR-6 (Distill-before-purge at all four success returns; G4, C3, SR-05).** Distillation is invoked
  via one shared helper, gated on `result.is_ok()`, at ALL FOUR success returns
  (`tools.rs:2110/2236/2925/3027` — purged-signals, cached-MetricVector, memoization-hit,
  full-pipeline), ahead of `purge_cycle_transcripts`. The purge still fires after. Error paths keep
  transcripts and produce no candidates. (→ AC-05)

- **FR-7 (Response-transient — never persisted; G3, C2, SR-07).** Candidates and per-session metadata
  are attached at response-assembly level **outside** the memoized `cycle_review_index` record (crt-033
  synchronous persist, #3793), or stripped on the persist path. No candidate or buffer content reaches
  any SQL write, file write, or log line. A forced re-review of the stored record returns it without
  stale candidates. (→ AC-06)

- **FR-8 (Reconstruction fallback — degraded, labeled; G5, C8, SR-08).** When an attributed session's
  buffer is empty or hole-ridden (hole/elision threshold defined against vnc-025 ADR-002 #4740 /
  ADR-008 semantics — cite #4764 active, not #4746), distillation input for that session is assembled
  from its already-loaded stored `ObservationRecord`s (tool, input, response_snippet). The fallback
  trigger is **whole-session either/or per session** (OQ-2). The fallback NEVER writes the byte buffer
  and NEVER produces observation rows. Output is labeled `Reconstructed` provenance, distinguishable by
  the consumer. (→ AC-07, AC-08)

- **FR-9 (topic_source as soft preference only; SR-06, OQ-1-design).** If used, `topic_source` is an
  **ordering/recall preference** for fallback selection (prefer `declared`/`registry-fill` over
  `vote`/`extracted`), NEVER a hard filter. Candidates remain feature-match-scoped; no session is
  dropped because of `topic_source`. crt-052 does not persist or re-derive `topic_source`. (→ AC-07)

- **FR-10 (Loss visibility; G6).** Per-session `elided_bytes` (when > 0), hole indication, and
  `Primary`-vs-`Reconstructed` provenance appear in the candidates section whenever non-zero/active.
  Transcript loss is never silent. (→ AC-08)

- **FR-11 (Two-pipe boundary preserved; G2 Non-Goal, C6).** With distillation active, the 23 detection
  rules' inputs are **bit-identical** to pre-crt-052; the batch filter (`listener.rs:1238`) is
  unchanged; distillation output reaches the knowledge base only via the agent's `context_store`
  writes. (→ AC-09)

- **FR-12 (Enterprise retention seam; C2, AC-10).** Distill+purge is gated on an **exhaustive**
  `TranscriptRetention` match (`PurgeOnCycleClose` arm `server.rs:543`, `RetainDays(_)` arm `:551`),
  never an assumed variant. The `RetainDays` arm neither distills nor purges in OSS (unreachable,
  rejected at `validate()`). (→ AC-10)

- **FR-13 (Transcript continuity — Option B held buffer; G8, C14, SR-01/SR-02/SR-03).** Buffers survive
  `drain_and_signal_session` in the bounded holding structure, keep merging deltas while held, are
  re-adopted on re-registration under the same `feature_cycle`, and are purged post-distill at cycle
  review and at stale sweep. Memory stays bounded (per-session cap × held-count, with an explicit
  held-count cap). No wire change. Diffs to `drain_and_signal_session` / `clear_transcripts_for_feature`
  stay minimal against vnc-030 ADR-007 §2 (Constraint 13). (→ AC-11)

- **FR-14 (Untrusted-input parser hardening; C7, SR-09).** The JSONL parser treats buffer content as
  untrusted client-disk input: unknown/corrupt lines degrade to **skip-with-count**, never to error or
  panic. The cycle-review handler never panics on malformed/adversarial transcript bytes. (→ AC-02,
  AC-V-FUZZ)

- **FR-15 (Consumer guidance; G7, AC-13).** Cycle-review consumer guidance (uni-retro skill / protocol
  cycle-review step) instructs four-family extraction into `context_store` with feature attribution,
  including the ass-070 Q8 folds (warning-level hotspot ↔ timestamp-adjacent candidates / rework-why;
  gate-failure narratives as units; human-intervention ledger from user-block content; phase-transition
  narration) and the explicit **call-time-vs-cached** note (candidates reflect call-time buffers, not
  the memoized report — OQ-4). (→ AC-13)

---

## Non-Functional Requirements

- **NFR-1 (Lock-hold latency; C1).** No I/O or parsing under any registry or buffer lock; lock holds
  remain microsecond-class (Arc clone + byte copy only). Verified structurally + concurrency test.

- **NFR-2 (Rule-pass throughput; AC-12).** The server-side rule pass over a 4 MiB buffer completes in
  **< 50 ms** off-lock (ass-070 estimate: single-digit ms). Cycle-review latency class unchanged.

- **NFR-3 (Selection recall/volume quality; AC-03).** ≥ **0.90** block-level recall of labeled items
  whose content appears in user/assistant blocks, on the independent labeled fixture corpus; selected
  volume ≤ **10%** of raw fixture bytes.

- **NFR-4 (Memory bound; SR-01).** Held-buffer memory is bounded by per-session byte cap × held-session
  count cap; held-count has an explicit ceiling and eviction policy. No unbounded growth from
  never-reviewed/never-swept sessions.

- **NFR-5 (Payload budget; ass-070).** Added response payload +10–25 KB per cycle review; agent input
  +2.6–6.6 K tokens; **zero** added round-trips.

- **NFR-6 (Dependency posture; AC-13).** No new heavyweight runtime dependency (regex-class only);
  `cargo audit` passes.

- **NFR-7 (No wire change; C11).** Server + protocol-docs only. `transcript_delta` / `SessionClose`
  frames consumed as-is; no client change; no new wire field (OQ-1 = Option B server-only).

- **NFR-8 (Secrets posture; C2, #4721).** In-memory + purge is the secrets guarantee. Distilled
  candidates are transient response content only; raw transcript and candidates are never persisted in
  any form.

- **NFR-9 (File-size discipline; C10).** New logic lands in new focused modules with thin call-site
  wiring; `tools.rs` / `session.rs` / `listener.rs` (already > 500 lines) gain only minimal wiring.

---

## Acceptance Criteria (with verification methods)

Each retains its SCOPE AC-ID. "Verification" states how a tester proves it.

- **AC-01 — Snapshot-and-release.** Buffer bytes of every session attributed to the reviewed
  `feature_cycle` are snapshotted before purge; Arc clones under a single registry lock, byte copies
  under per-buffer locks, all parsing/matching after every lock release.
  *Verification:* (a) structural/source assertion that no parse/match call occurs inside a lock guard
  scope; (b) **concurrency test** streaming transcript deltas concurrently during a cycle review,
  asserting no deadlock, no torn read, and a consistent snapshot.

- **AC-02 — Selection contract + dual cap.** Only user/assistant text blocks retained
  (tool_use/tool_result/thinking/command-noise dropped); four families matched; matched blocks whole;
  deduped; per-session cap (default 24 KB) AND per-cycle aggregate cap enforced; candidates ordered with
  `session_id` + `byte_offset` + `ts` + advisory `family_hints`.
  *Verification:* unit tests on the pure selection module over fixtures: assert dropped block types
  absent; assert whole-block preservation; assert dedup; assert per-session and per-cycle truncation at
  configured limits (two independent knobs); assert ordering and provenance fields populated.

- **AC-03 — Independent fixture recall/volume.** On a committed labeled corpus synthesized per the
  ass-070 ground-truth method **and authored independently of the ported regex set**, selection
  achieves ≥ 0.90 block-level recall of labeled in-block items; selected volume ≤ 10% of raw bytes.
  *Independence mechanism (verifiable requirement):* corpus anchors are authored **before** the regex
  set is ported into this feature, **or** by a different author than the porter; the corpus file
  carries a committed provenance header asserting the independence mode used and the authoring order/
  author, and the regex set is not consulted while labeling. *Verification:* a committed fixture with
  the provenance header; a test computing block-level recall against the labels and asserting ≥ 0.90 and
  ≤ 10% volume; a review check that the independence header is present and asserts anchors-before-port
  or different-author.

- **AC-04 — Additive, absent-when-empty.** Response carries `transcript_candidates` as an additive
  section, **absent (not null/empty)** when no session yields candidates; all pre-existing fields and
  col-024/crt-033 behaviors byte-unchanged when no transcripts exist.
  *Verification:* serde round-trip test asserting the field is omitted from JSON when None; golden-output
  diff of a no-transcript cycle review against pre-crt-052 output (byte-identical for existing fields).

- **AC-05 — Four-return ordering.** Distill-before-purge holds at all four success returns
  (`tools.rs:2110/2236/2925/3027`); candidates reflect call-time buffer content; purge fires after.
  Error paths keep transcripts, produce no candidates. On a memoization hit, candidates are distilled
  from call-time buffer content and may differ from the cached report (acceptable, documented).
  *Verification:* per-path tests exercising each of the four returns asserting (distill → purge) order;
  an **exhaustiveness test** that fails if a fifth success return is added without wiring the helper
  (SR-05); an error-path test asserting transcripts retained and no candidates.

- **AC-06 — Response-transient, no leak.** The memoized `cycle_review_index` record contains no
  candidate or transcript content (a forced re-review of the stored record returns it without stale
  candidates); no SQL write, file write, or log line carries candidate or buffer content.
  *Verification:* (a) re-review-of-stored-record test asserting candidates absent from the cached path;
  (b) a **content-leak gate** extending the vnc-025 AC-12 grep/test gate to the new code paths —
  asserting no candidate/buffer bytes appear in persisted `cycle_review_index` rows, audit events
  (content-free), or logs (cite SR-07).

- **AC-07 — Labeled reconstruction fallback.** An attributed session with an empty/hole-ridden buffer
  (threshold per ADR-002 semantics) contributes input assembled from its stored `ObservationRecord`s
  (tool, input, response_snippet), labeled `Reconstructed` and consumer-distinguishable; the fallback
  never writes the byte buffer and never produces observation rows; trigger is whole-session either/or.
  *Verification:* tests with (i) empty buffer + observations present → reconstructed candidates labeled
  degraded; (ii) hole-ridden buffer at/above the defined threshold → fallback fires whole-session;
  (iii) assertion that no buffer write and no observation-row insert occurs on the fallback path;
  (iv) if `topic_source` ordering is used, assert it only reorders and never drops a feature-matched
  session (FR-9 / SR-06).

- **AC-08 — Loss never silent.** Per-session `elided_bytes`, hole indication,
  primary-vs-reconstructed provenance, **and cap-forced candidate truncation (per-session volume cap
  AND per-cycle aggregate cap — AC-02)** appear in the candidates section whenever non-zero/active. A
  cap-forced drop of late candidates is reported, never silent — the same principle the elision/hole
  reporting already enforces.
  *Verification:* tests asserting metadata block populated for elided/holed/reconstructed sessions and
  omitted when zero/inactive; assert provenance label matches the source path; **assert that when
  either the per-session or the per-cycle aggregate cap truncates candidates, the dropped count (and
  affected session/cycle) surfaces in the loss-visibility section** — no silent aggregate-cap drop.

- **AC-09 — Two-pipe boundary preserved.** With distillation active, the 23 detection rules' inputs are
  bit-identical to pre-crt-052; batch filter (`listener.rs:1238`) unchanged; distillation output reaches
  the KB only via agent `context_store`.
  *Verification:* extend the existing `detection_isolation` tests to run with distillation active and
  assert rule inputs byte-identical; assert no new path feeds buffer bytes into `insert_observations_batch`.

- **AC-10 — Exhaustive retention match.** Distill+purge gated on an exhaustive `TranscriptRetention`
  match (`PurgeOnCycleClose` `server.rs:543`, `RetainDays(_)` `:551`); `RetainDays` neither distills nor
  purges in OSS (unreachable, rejected at `validate()`).
  *Verification:* a match-exhaustiveness compile guarantee (no wildcard arm); a test asserting the
  `RetainDays` configuration is rejected at `validate()` (OSS unreachable).

- **AC-11 — Transcript continuity (Option B), simulated real lifecycle [PRE-MERGE PRIMARY-PATH PROOF].**
  This is the **only** pre-merge evidence the primary (non-fallback) path works before the dogfooding
  switchover, and is a **hard, named verification**. A multi-turn session's buffer content streamed
  across **≥ 3 simulated turn boundaries** is re-adopted and available to the cycle-review snapshot;
  held buffers keep merging deltas; memory stays bounded (cap × held sessions); the stale sweep still
  reclaims held buffers; the moved purge/audit points fire **exactly once per held session** at review
  or sweep.
  *Verification (named test `continuity_simulated_lifecycle`):* a **faithful per-turn-drain simulation**,
  NOT a single-turn happy path — it executes the real sequence
  **register → deltas → drain (Stop→SessionClose) → deltas → drain → deltas → drain → re-register →
  cycle review** (≥ 3 drain cycles, deltas applied between drains to prove merge-while-held), asserting:
  (a) the snapshot at review contains content streamed across all turns, not just the last;
  (b) re-adoption rebinds to the same `feature_cycle` and fails loud on key mismatch (SR-02);
  (c) held-buffer count stays within the configured cap and eviction is observable when exceeded;
  (d) stale sweep reclaims a held buffer that is never re-registered/reviewed;
  (e) `transcript_session_purged` fires exactly once per held session at review/sweep (SR-03).

- **AC-12 — Throughput.** Server rule pass over a 4 MiB buffer < 50 ms off-lock; cycle-review latency
  class unchanged.
  *Verification:* a benchmark/timed test over a 4 MiB fixture asserting < 50 ms for the off-lock rule
  pass.

- **AC-13 — Consumer guidance + dependency posture.** uni-retro skill / protocol cycle-review step
  updated with four-family extraction instructions, the Q8 folds, and the call-time-vs-cached note,
  storing via `context_store` with feature attribution; `cargo audit` passes; no new heavyweight runtime
  dependency (regex-class only).
  *Verification:* documentation review checklist (four families, Q8 folds, call-time-vs-cached note, and
  feature-attributed `context_store` present); CI/protocol `cargo audit` pass; dependency-diff review
  confirming regex-class only.

### Supplementary verification (referenced by FRs)

- **AC-V-SEAM (Constraint 4 / SR-04).** A test/structural check asserting `take_transcripts_for_feature`
  returns owned bytes + per-session `elided_bytes`/`hole_info` and that a #700-style marker-recovery
  caller can parse the snapshot without invoking `contiguous_tail` or any second buffer reader.
- **AC-V-FUZZ (Constraint 7 / SR-09).** Malformed/adversarial-line fixtures (truncated JSON, non-UTF-8,
  oversized line, unknown record type) assert skip-with-count and no panic in the parser or handler.

---

## User Workflows

### W1 — Cycle-review consumer (uni-retro extraction flow), primary path

1. A retrospective agent (uni-retro skill / protocol step) calls `context_cycle_review` for a reviewed
   `feature_cycle`.
2. Server selects attributed sessions, snapshots their (held + live) buffers via the seam, runs the
   pure selection module off-lock, attaches `transcript_candidates` + per-session loss/provenance
   metadata, then purges.
3. The agent receives candidates (whole user/assistant blocks, advisory family hints, provenance) and
   performs **all** semantic extraction, classifying each into the four target families and storing
   ADRs/lessons/patterns via `context_store` with feature attribution.
4. The agent applies the Q8 folds (join warning-level hotspots to timestamp-adjacent candidates for
   rework-why; treat gate-failure narratives as units; build a human-intervention ledger from user-block
   content; narrate phase transitions).
5. The agent treats candidates as **call-time** content (not the memoized report) per the
   call-time-vs-cached note.

### W2 — Reconstruction fallback (degraded)

When an attributed session's buffer is empty/hole-ridden, the server supplies that session's
distillation input from stored observations, labeled `Reconstructed`. The agent sees the degraded
provenance and weights decision-family extraction accordingly (decision is the weakest family, 0.81
ceiling).

### W3 — Held-buffer lifecycle (server-internal, no agent interaction)

Register → deltas accumulate → Stop fires per turn → `drain_and_signal_session` moves the buffer into
the bounded held structure instead of freeing it → deltas continue merging into the held buffer →
re-registration re-adopts the buffer under the same `feature_cycle` → at cycle review the snapshot seam
reads the held + live content → post-distill purge (or stale-sweep reclaim) emits the single audit.

### W4 — #700 marker-recovery (downstream, future)

#700 consumes the same `TranscriptSnapshot` return for review-time MARKER parsing, reusing owned
bytes + elision/hole metadata — **no second buffer reader** opened. crt-052 ships the seam so this is
possible; it does not implement marker recovery.

---

## Constraints (from SCOPE; binding on architecture)

C1 Registry lock discipline — microsecond holds, no I/O/parse under any lock (FR-1, NFR-1).
C2 Secrets posture architectural — candidates transient; crt-033 memoization persist is the trap (FR-7).
C3 Four success returns — one helper at all four `result.is_ok()` sites (FR-6).
C4 One buffer content reader — seam is second and last; #700 reuses it (FR-2, AC-V-SEAM).
C5 Named seam is the only insertion point — modify `clear_transcripts_for_feature` + caller (FR-2).
C6 No generation server-side — rules select, never extract (FR-3, FR-11).
C7 Claude Code JSONL only; unknown/corrupt → skip-with-count, never error (FR-14).
C8 Reconstruction is a fidelity floor, not parity — provenance labeling mandatory (FR-8, FR-10).
C9 Tail-window-equivalence buffer semantics — not lossless; design fallback trigger against ADR-002/008.
C10 500-line file rule — new focused modules, thin wiring (NFR-9).
C11 Wire contract untouched — no client change, no new wire field (NFR-7).
C12 4 MiB buffer cap stands — elision visibility is the guard (FR-10).
C13 Cite, don't rework vnc-030 ADR-007 §2 precedence — minimal diffs to adjacent functions (FR-13).
C14 Per-turn drain reality — acceptance tests simulate the real lifecycle, not happy single-turn (AC-11).

---

## Dependencies

- **Crates / components:** `unimatrix-observe` (selection module leans here beside `synthesis.rs` /
  `phase_narrative.rs`, OQ-5; pure no-I/O functions); `unimatrix-server` (handler wiring in
  `mcp/tools.rs`, registry in `session.rs`, drain/close in `listener.rs`, purge in `server.rs`,
  config in `infra/config.rs`); `unimatrix-core` (`ObservationRecord`); regex-class crate only (AC-13).
- **Shipped predecessor interfaces:** vnc-025 `TranscriptBuffer` / `clear_transcripts_for_feature` /
  `purge_cycle_transcripts` (#670); vnc-030 contractual attribution + close/sweep precedence interface
  (ADR-007 §2, #4819, #699/PR #702); crt-033 cycle-review memoization (#3793).
- **Decisions consulted:** #4742 (named seam / take-shaped), #4750 (four success returns), #4740/#4764
  (tail-window-equivalence — cite #4764 active per SR-08, not #4746), #4721 (secrets posture), #4819
  (cross-feature seam contracts, §4 #700 single-reader pin), #4799 (per-turn drain starvation).
- **Downstream dependent:** #700 review-time MARKER recovery (consumes the snapshot seam; Constraint 4).
- **Lifecycle reference:** #981 (NULL/mis-set `feature_cycle` silently breaks retrospective — cite for
  re-adoption fail-loud, SR-02); #3359 (threshold/window mismatch over-fires — SR-08).

---

## NOT in Scope (explicit exclusions)

- Server-side LLM/semantic extraction or family classification — rules select, hints advisory only.
- Changes to the 23 detection rules; their inputs must stay bit-identical (two-pipe boundary).
- Multi-provider transcript parsing — Claude Code JSONL only.
- Sidechain / sub-agent transcripts — main-thread only.
- Excerpt windowing, spawn-prompt channel, tool/bash channels.
- Decision→outcome linking, #556 declared-but-never-closed grounding, cross-session cycle stitching.
- Wall-clock anomaly explanation, auto-drafted lesson/ADR text.
- Buffer cap changes / periodic distill-and-truncate — 4 MiB stands.
- Persistence of candidates or raw transcript in any form.
- Client / wire changes; Option A close-reason wire field is off the table.
- Re-implementing vnc-030 declared-beats-vote precedence — cite, don't rework.
- Review-time MARKER recovery (#700) — crt-052 ships the consumable seam only.
- Full registry lifecycle redesign — per-turn drain / register-overwrite amnesia remain named-not-fixed
  except the narrow Option B continuity remedy.
- `topic_source` as a hard filter — soft ordering preference only (SR-06).

---

## Requirement Count

- Functional requirements: **15** (FR-1..FR-15)
- Non-functional requirements: **9** (NFR-1..NFR-9)
- Acceptance criteria: **13** SCOPE ACs (AC-01..AC-13, all present) + **2** supplementary verification
  criteria (AC-V-SEAM, AC-V-FUZZ)
- Constraints: **14** (C1..C14)
- Domain models: **6** (TranscriptCandidate, TranscriptSnapshot seam return, held-buffer holding
  structure, provenance/loss-visibility metadata, marker-family set, plus the response section shape)

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #4742 (named seam / take-shaped, content-free
  audit), #4750 (four success returns, one gated helper), #3793/#3795/#3794 (crt-033 memoization is a
  synchronous persist — the AC-06 trap), and the `RetrospectiveReport` optional-field/`skip_serializing_if`
  precedent (`unimatrix-observe/src/types.rs:381`). All folded into FR-2/FR-5/FR-6/FR-7 and the domain
  models. Read-only tier — no storage (spec decisions are feature-specific).
