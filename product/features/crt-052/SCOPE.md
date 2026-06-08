# crt-052: Transcript-Fed Cycle Review Distillation — Decisions, Rework Narrative, Phase Intent

GH Issue: #689. Consumes the vnc-025 (#670, shipped) transcript buffer; extraction architecture
decided by ass-070 (#683, FINDINGS complete, **GO**). Predecessors in the pinned OSS-cloud
finalization sequence have shipped: vnc-027 (F4a, #680, MERGED) brought the TS client + delta
streaming to UDS; vnc-030 (F4b, #699, MERGED via PR #702) shipped contractual attribution and the
declared-beats-vote precedence fixes. **crt-052 is now next-up** and builds on those delivered
interfaces.

This is a design restart: the prior SCOPE.md was written before the F4 features shipped and before
the six Open Questions were decided. All six are now resolved by binding human decisions (#689,
2026-06-08); the delivery-ordering analysis is historical; cross-references that pointed at the
pre-split vnc-027 are re-homed; and two new first-class constraints (the #700 single-reader snapshot
seam, and the vnc-025 tail-window-equivalence buffer semantics) are elevated.

## Problem Statement

`context_cycle_review` analyzes structured `ObservationRecord`s only (timestamp, tool, input,
500-char response snippet — `unimatrix-core/src/observation.rs:21`). It is quantitative and mute on
causation: the report says `compile_cycles: 29 — elevated` but never *why*; it timestamps a 62-minute
gap but not the disk-exhaustion cascade behind it. The conversational narrative — decisions, rework
reasoning, phase intent, human interventions — is never captured. ass-070 measured the loss:
**65% of hand-labeled session value (28/43 items) is currently uncurated** and destroyed at purge.
vnc-025 built the per-session in-memory transcript buffer and the cycle-review purge; crt-052 inserts
the distillation step ahead of that purge so the narrative is harvested before the bytes die.

Who is affected: every feature retrospective (uni-retro, cycle review consumers) and the knowledge
base itself — decisions and rework reasoning that should become ADRs/lessons/patterns are lost unless
a human happens to curate them manually.

Why now: vnc-025 shipped the buffer, purge lifecycle, and the named crt-052 seam
(`clear_transcripts_for_feature`, ADR-004 #4742); ass-070 returned GO with an empirically validated
extraction architecture (0.95 recall / ~0.96 precision at ~16 K input tokens per cycle); the two F4
predecessors have merged, so the primary streaming path is exercisable in dogfooding and the
attribution that scopes "which transcripts belong to the reviewed cycle" is now contractual; and a
deferred follow-up (#700, review-time MARKER recovery) now formally depends on crt-052's snapshot seam.

## Goals

1. **Snapshot-and-release distillation pass** at `context_cycle_review`: for every registry session
   attributed to the reviewed `feature_cycle`, snapshot buffer bytes out under the established
   two-phase lock discipline (Arc clone under registry lock; byte copy under the per-buffer lock;
   **all parsing strictly after every lock is released**). Extends the named seam:
   `clear_transcripts_for_feature` (`session.rs:299`) becomes take-shaped per vnc-025 ADR-004's plan.
   The snapshot seam (`take_transcripts_for_feature`) is also the **sole** content-reading path #700
   (MARKER recovery) will consume — it must be designed as the one buffer content reader (Constraint 4).
2. **Server-side candidate selection (rules select, agent extracts — ass-070 Q4)**: parse snapshot as
   Claude Code JSONL; keep `user`/`assistant` text blocks only (drop
   tool_use/tool_result/thinking/command-noise); match against the four marker families (decision
   phrases, rework signals, lesson markers, phase/gate markers — ~50 patterns ported from ass-070's
   extractor); dedup; keep matched blocks **whole** (no windowing); enforce a per-session volume cap
   (config knob, default 24 KB) **and** a per-cycle aggregate cap (config knob); order chronologically
   with session-id + byte-offset + timestamp provenance and advisory family hints. No server-side
   semantic classification beyond hints.
3. **`transcript_candidates` response section**: additive optional field on the cycle-review response
   (absent when empty, following the `RetrospectiveReport` optional-field pattern,
   `unimatrix-observe/src/types.rs:381`). The calling agent performs all semantic extraction and stores
   results via `context_store` — attribution-preserving, agent-curated, zero added round-trips.
4. **Distill-before-purge at all four success returns**: candidates are extracted before
   `purge_cycle_transcripts` fires, on every success path — purged-signals, cached-MetricVector,
   memoization-hit, and full-pipeline (pattern #4750: there are FOUR success returns, not one — call
   sites `tools.rs:2110, 2236, 2925, 3027`). Error paths keep transcripts, unchanged.
5. **Reconstruction fallback (degraded, labeled)**: when an attributed session's buffer is empty or
   hole-ridden, assemble distillation input from that session's stored `ObservationRecord`s (tool,
   input, response_snippet). Distillation-input only — never back-fills the byte buffer, never produces
   observation rows. Output explicitly labeled degraded provenance (ass-070 Q6: reconstruction ceiling
   is 0.81 vs 0.98; decision capture is the weak family — 5 of 8 lost). Trigger is whole-session
   either/or per session for v1 (OQ-2 resolution), keyed to hole/elision state per vnc-025 ADR-002
   semantics (Constraint 13), not to an assumed lossless buffer.
6. **Loss visibility**: per-session elided-byte counts (`TranscriptBuffer::elided_bytes`), hole
   presence, and fallback usage surfaced in the candidates section — transcript loss is never silent
   (ass-070 Q5: anchors are uniformly distributed; elision loses early decisions proportionally).
7. **Consumer instructions**: update the cycle-review consumer guidance (uni-retro skill / protocol
   step) to instruct extraction of the four target families from candidates into `context_store`,
   including the ass-070 Q8 folds: join warning-level hotspot findings to timestamp-adjacent candidates
   (rework-why), gate-failure narratives as units, human-intervention ledger (user-block content), and
   phase-transition narration. State explicitly that candidates reflect call-time buffers, not the
   cached/memoized report (cache-hit semantics, OQ-4).
8. **Transcript continuity across the per-turn drain — Option B, server-only hold** (OQ-1 resolved,
   binding): the per-turn Stop→SessionClose drain leaves buffers empty at review time for any
   multi-turn session, starving the primary path. The remedy is a server-only bounded holding
   structure: buffers survive `drain_and_signal_session`, keep merging deltas for held sessions, are
   re-adopted on re-registration, and are purged at cycle review (post-distill) and stale sweep. No
   wire change. This is the most state-machine-heavy component; design treats it as its own area with
   explicit memory-bound and audit-shape decisions.

## Non-Goals

- **Server-side LLM extraction** — the server has no generation capability at all (ass-070 Q2:
  `unimatrix-embed` is ONNX-only — embeddings + cross-encoder scoring; GGUF exists only in TODO
  comments). Rules select; they never extract.
- **Server-side semantic family classification** — hints are advisory; the agent re-classifies.
- **Changes to the 23 detection rules** — they stay on `ObservationRecord`; their inputs must remain
  bit-identical with distillation active (the two-pipe boundary, ass-070 Q7).
- **Multi-provider transcript parsing** — Claude Code JSONL format only (ass-069 Open Thread 1).
- **Sidechain (sub-agent) transcripts** — vnc-025 streams main-thread only; SM narration holds 42/43
  of labeled value (ass-070 out-of-scope discovery). Reopens only if implementer-level lessons are
  ever wanted.
- **Excerpt windowing, spawn-prompt channel, tool/bash channels** — measured harmful or useless
  (ass-070 Q4 ablations: windowing loses multi-paragraph context; spawn channel is 3.6× volume for
  zero recall gain).
- **Decision→outcome linking, #556 declared-but-never-closed grounding, cross-session cycle
  stitching** — one follow-on feature after crt-052 ships real extractions (ass-070 Q8 items 5–7).
- **Wall-clock anomaly explanation and auto-drafted lesson/ADR text** — dropped (Q8 items 8–9).
- **Buffer cap changes / periodic distill-and-truncate** — 4 MiB stands (largest-ever session
  2.12 MiB, 0/43 items elided, ~2.8× headroom — ass-070 Q5); deferred until real sessions approach
  ~3 MiB.
- **Persistence of candidates or raw transcript in any form** — candidates are response-transient; the
  in-memory + purge posture (#4721) is the secrets guarantee.
- **Client/wire changes** — crt-052 is server + protocol-docs only. OQ-1 resolved to Option B
  (server-only); the Option A close-reason wire field is **off the table** — nothing routes to any F4
  feature.
- **Re-implementing the declared-beats-vote precedence** — vnc-030 ADR-007 §2 shipped the close/sweep
  precedence fix as a citable, minimal-diff interface (`process_session_close`, `sweep_stale_sessions`).
  crt-052 cites it and keeps diffs minimal against it; it does not rework precedence.
- **Review-time MARKER recovery (#700)** — out of scope here; #700 is a downstream consumer of
  crt-052's snapshot seam. crt-052 ships the seam such that #700 can consume it without a second buffer
  reader; it does not implement marker recovery.
- **Full registry lifecycle redesign** — per-turn drain semantics, register-overwrite amnesia
  (ass-072 discoveries 2/3) remain named-not-fixed except for the narrow transcript-continuity remedy
  in Goal 8.

## Background Research

All code claims re-verified in this workspace 2026-06-08 against the current tree (branch
`feature/vnc-030`, **post-vnc-030 merge** — file/line numbers in the prior SCOPE were stale and are
corrected here).

### ass-070 FINDINGS (#683) — the deciding evidence (unchanged, re-verified holds)

- **Extraction architecture is decided**: server-side rules are a good selector and a poor extractor
  (0.93 recall, 0.11–0.55 per-family precision — shipping rule output directly would pollute the
  knowledge base). Agent-over-candidates reaches 0.95 recall / ~0.96 precision. The hybrid cut — server
  selects whole marker-matched user+assistant blocks, agent does all semantics — shrinks input 95%
  (58 KB vs 1,156 KB across 6 sessions) at a ≤2-item recall cost.
- **Cost envelope**: server rule pass < 10 ms (Rust, estimated from 0.24 s Python over 4.7 MiB);
  +10–25 KB response payload; +2.6–6.6 K agent input tokens per cycle review; zero added round-trips.
- **Two-layer ground truth for ACs**: layer-a (15 items matching existing human curation) is the parity
  bar; layer-b (28 uncurated items) is the uplift — 27/28 recovered by the hybrid.
- **Reconstruction fallback ceiling**: 35/43 (0.81); DEC loses 5 of 8 — decisions live in prose that
  observations never carry. Fidelity floor, not parity.
- **Two-pipe boundary verified (re-located post-merge)**: transcript deltas are applied via
  `apply_transcript_delta` (single-event arm `listener.rs:953`, batch tee `listener.rs:1206`) and the
  batch filter `event_type != TRANSCRIPT_DELTA_EVENT` (`listener.rs:1238`) keeps delta bytes out of
  `insert_observations_batch` (`listener.rs:1248`). No observation field sources buffer content;
  detection-rule inputs are bit-identical with the buffer active. (Prior SCOPE cited `listener.rs:999-1025`/`:1009` — stale.)

### vnc-025 surfaces this feature consumes (re-verified in current code)

- `TranscriptBuffer` (`infra/session_transcript.rs`): `apply_delta` idempotent merge, `contiguous_tail`,
  `clear() -> u64`, `len`, `high_water`, `elided_bytes` (lifetime counter), hole tracking. **Buffer
  semantics pin (vnc-025 ADR-002 #4740, ADR-008 #4764 — active, supersedes #4746; #689 comment 2026-06-06):** the buffer
  guarantees **tail-window equivalence, not full-content convergence** — full-content equality holds
  only below the 4 MiB cap; once ring-tail advances `base_offset`, late head deltas are clipped and
  converge on the tail window, not full content. `clear()` preserves `high_water` and `elided_bytes`
  (verified `session_transcript.rs:199-208`); deltas below the elided floor are defined no-ops.
  crt-052's distillation window and the reconstruction-fallback trigger (hole/elision-keyed) are
  designed against this, **not** an assumed lossless buffer.
- `SessionState.transcript: Arc<Mutex<TranscriptBuffer>>` (ADR-001 #4739) — `get_state()` clones stay
  O(small); snapshot = Arc clone under registry lock, byte copy under buffer lock.
- **The named seam**: `clear_transcripts_for_feature` (`session.rs:299` — moved from :262 pre-merge) —
  single registry lock, linear scan on `state.feature == Some(feature_cycle)`, Arc clones collected,
  lock released, per-buffer `clear()` after. ADR-004 (#4742): "crt-052 modifies one method body and its
  caller"; becomes take-shaped here.
- **Single content reader (load-bearing for #700, Constraint 4):** the buffer's **only** production
  content-reading path today is the PreCompact block builder —
  `contiguous_tail(MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER)` → `extract_transcript_block_from_bytes`
  (`listener.rs:1834-1838`, moved from ~:1646 in vnc-030's note). All other `contiguous_tail` callers
  are tests. crt-052's `take_transcripts_for_feature` becomes the second (and last) content reader; the
  invariant is that #700 consumes it rather than opening a third.
- **Purge call topology**: `purge_cycle_transcripts` (`server.rs:541`) — exhaustive
  `TranscriptRetention` match (`PurgeOnCycleClose` arm at `:543`, `RetainDays(_)` at `:551`), content-free
  audit, fire-and-forget. Called from **four** success returns in the cycle-review handler
  (`tools.rs:2110, 2236, 2925, 3027` — unchanged), each gated `result.is_ok()` (pattern #4750 —
  inserting distillation at only the tail return silently skips cache-hit and degraded paths).
- Config knob precedent: `transcript_buffer_max_bytes` lives beside `transcript_retention`
  (`infra/config.rs:1561-1576`); the per-session cap and the new per-cycle aggregate cap follow the same
  pattern.

### vnc-030 / F4b interfaces this feature builds on (shipped, #699 / PR #702)

- **Citable close/sweep precedence interface (ADR-007 §2, #4819):** the two declared-vs-vote inversion
  fixes ship as a minimal-diff, documented interface — declared feature now beats majority vote at
  `process_session_close` (`listener.rs:2069`; drain invoked at `:2133`) and `sweep_stale_sessions`
  (`session.rs:687` — moved from :628 in vnc-030's note). vnc-030 **explicitly left**
  `drain_and_signal_session` (`session.rs:651`), `clear_transcripts_for_feature` (`session.rs:299`), and
  the transcript buffer untouched. crt-052 edits adjacent functions in these same files (its Goal 8
  continuity remedy and take-shaped seam) — it keeps diffs minimal against this interface and **cites**
  it rather than reworking precedence. Consequence for crt-052: session selection
  (`state.feature == feature_cycle` + attributed observations) now sees declared protocol sessions
  attributed by contract — a declared session's `feature_cycle` can no longer be vote-flipped at
  close/sweep.
- **`observations.topic_source` column (additive, vnc-030 ADR-004/005):** records per-row attribution
  source (`declared` / `extracted` / `registry-fill` / `vote` / NULL; enum origins
  `session.rs:112-124`). **Assessment for crt-052:** the fallback path (Goal 5) already selects
  attributed observations by feature; `topic_source` could *sharpen* that selection by preferring
  `declared`/`registry-fill` rows over `vote`/`extracted` rows when scoping which sessions/observations
  belong to the reviewed cycle, reducing mis-scoped reconstruction input. This is an **optional
  input**, not a dependency — the candidates are response-transient and crt-052 does not persist or
  re-derive `topic_source`. Whether to filter on it is an architecture/design call (see Open Questions).

### Cycle review / report surfaces (re-verified)

- `RetrospectiveReport` (`unimatrix-observe/src/types.rs:381`) is a long additive-optional-field struct
  (fields guarded by `skip_serializing_if`) — `transcript_candidates` follows the pattern, with one
  critical exception: **crt-033 memoizes the report synchronously to `cycle_review_index` (#3793)**.
  Candidates must never ride that persist (raw excerpts in SQL would violate the never-persist posture)
  — either attached at response-assembly level outside the memoized struct, or stripped on the persist
  path. Design decides the mechanism; the invariant is AC-06.
- `synthesize_narratives` (`synthesis.rs:15`) / `build_phase_narrative` (`phase_narrative.rs:21`) are
  pure, no-I/O functions; candidate selection is a third pure function beside them (ass-070 Q6:
  additive, no change to either).
- `ObservationRecord` (`unimatrix-core/src/observation.rs:21`): ts, event_type, source_domain,
  session_id, tool, input, response_size, response_snippet (≤500 chars) — the reconstruction-fallback
  input shape. The handler already loads attributed observations
  (`load_cycle_observations`, `services/observation.rs:308`); the fallback reuses them.

### Per-turn drain starves the primary path — why Goal 8 / Option B is in scope (re-verified)

Verified end-to-end in current code (pattern #4799):

- The hook client maps **`Stop` AND `TaskCompleted` → identical `SessionClose` frames**
  (`build-request.js:59-62`; the server cannot distinguish a turn boundary from a session end).
- Claude Code fires `Stop` **per assistant turn**.
- Every `SessionClose` runs `drain_and_signal_session` (defined `session.rs:651`, invoked from
  `process_session_close`, `listener.rs:2133`) — registry key removed, **transcript buffer freed**.
- After drain, deltas are silent no-ops (no auto-registration). Re-registration happens only at
  `SessionRegister` (SessionStart: startup/resume/clear/compact) and the col-022 cycle_start
  pre-register. **Nothing re-registers at a normal turn boundary.**

Consequence: for any multi-turn session, the buffer is drained at the end of every turn and subsequent
deltas drop until a rare re-register. At cycle-review time the reviewing session's buffer is empty —
the primary path yields nothing and the feature degrades to the 0.81-ceiling fallback by default, in
every realistic session. (vnc-030 left the drain untouched, as scoped; vnc-027's stamp fixes
*attribution* across the drain but not the *bytes* — the stamp surviving turn boundaries does not
restore the buffer.) **Option B (server-only transcript hold) is the binding remedy (OQ-1 resolved).**

### Prior decisions consulted (Unimatrix)

- **#4742** (vnc-025 ADR-004): purge-point shapes; `clear_transcripts_for_feature` is the single
  crt-052 insertion point; "becomes take-shaped later, parsing never under a lock".
- **#4740** (vnc-025 ADR-002): tail-window-equivalence buffer semantics — full-content convergence only
  below the cap; the buffer-semantics pin.
- **#4764** (vnc-025 ADR-008, active — supersedes #4746): checked offset arithmetic + poison-recovery; "crt-052 reconstructs" is
  the named recovery for a poisoned/cleared buffer — reinforces the fidelity-floor framing.
- **#4750**: four success-return points — success-only side effects must gate every one.
- **#4739** (vnc-025 ADR-001): transcript rides SessionState as `Arc<Mutex<TranscriptBuffer>>`.
- **#4819** (vnc-030 ADR-007): cross-feature seam contracts — §2 the citable close/sweep interface,
  §4 the marker-recovery follow-up's binding dependency on crt-052's snapshot seam + the single-reader
  pin.
- **#4816** (vnc-030 ADR-004): `FeatureSource::{Declared, Inferred}` + `topic_source` semantics.
- **#3793** (crt-033 ADR-001): cycle review memoization is a synchronous write — the persist path
  candidates must avoid.
- **#4721** (vnc-024 ADR-005): in-memory + purge IS the secrets guarantee; retention governs raw
  transcript only, never distilled knowledge.
- **#4799** (crt-052 pattern): per-turn drain empties accumulated SessionState — review-time consumers
  see at most one turn of data.

## Proposed Approach

1. **Candidate-selection module** (new pure module, leaning `unimatrix-observe` beside `synthesis.rs` /
   `phase_narrative.rs` per OQ-5; architecture finalizes): input bytes → parse Claude Code JSONL →
   user/assistant text blocks → four marker families (regex set ported from ass-070's `extractor.py`) →
   dedup → per-session cap → per-cycle aggregate cap → ordered
   `Vec<TranscriptCandidate { session_id, byte_offset, ts, family_hints, text }>`. No I/O, no locks,
   unit-testable against committed fixtures.
2. **Take-shaped seam (the single content reader for #700):** extend `clear_transcripts_for_feature`
   into / add a sibling `take_transcripts_for_feature` so the cycle-review path snapshots
   `(bytes, elided_bytes, hole_info)` per session before clearing — Arc clones under one registry lock,
   byte copies under per-buffer locks, parsing strictly after release. Design the return type and call
   contract so #700 (MARKER recovery) consumes this seam directly — no second `contiguous_tail`-style
   reader is ever opened (Constraint 4, #700 dependency).
3. **Distill helper in the handler**: one helper, called at all four success returns ahead of
   `purge_cycle_transcripts` (same `result.is_ok()` gating). For attributed sessions with
   empty/hole-ridden buffers, build reconstruction input from the already-loaded observations
   (optionally `topic_source`-filtered, OQ open). Attach `transcript_candidates` (+ per-session
   loss/provenance metadata) to the response at assembly level, excluded from the memoized record.
4. **Continuity remedy (Option B, binding):** transcript buffers survive `drain_and_signal_session` in
   a bounded holding structure keyed by session, continue accepting deltas for held sessions, are
   re-adopted on re-registration, and are purged at cycle review (post-distill) and stale sweep. This
   *is* `TranscriptRetention::PurgeOnCycleClose` semantics — the current purge-at-every-turn-close
   behavior is more aggressive than the policy name and starves the policy's purpose. The audit shape
   (`transcript_session_purged`) moves with the purge points that remain. Diffs to
   `drain_and_signal_session` / `clear_transcripts_for_feature` stay minimal against vnc-030 ADR-007 §2
   (which left these functions untouched).
5. **Protocol/skill update**: cycle-review consumer guidance instructs the four-family extraction into
   `context_store`, with the Q8 prompting folds and the call-time-vs-cached candidates note (OQ-4).

## Acceptance Criteria

- AC-01: At `context_cycle_review` success, buffer bytes of every registry session attributed to the
  reviewed `feature_cycle` are snapshotted before purge: Arc clones collected under a single registry
  lock, bytes copied under per-buffer locks, and all JSONL parsing/marker matching executes after every
  lock is released (verified structurally + a concurrency test streaming deltas during review).
- AC-02: Candidate selection retains only user/assistant text blocks from Claude Code JSONL
  (tool_use/tool_result/thinking/command-noise dropped), matches the four marker families, keeps
  matched blocks whole, dedups, enforces the per-session volume cap (config knob, default 24 KB) **and**
  the per-cycle aggregate cap (config knob), and emits candidates ordered with session-id, byte-offset,
  timestamp provenance, and advisory family hints.
- AC-03: On a labeled fixture corpus (committed, synthesized following the ass-070 ground-truth method
  and authored **independently of the ported regex set** — anchors written before porting, or a
  different author, to avoid self-fulfilment), candidate selection achieves ≥ 0.90 block-level recall of
  labeled items whose content appears in user/assistant blocks; selected volume ≤ 10% of raw fixture
  bytes.
- AC-04: The success response carries `transcript_candidates` as an additive section, absent (not
  null/empty) when no session yields candidates; all pre-existing cycle-review response fields and the
  col-024/crt-033 path behaviors are byte-unchanged when no transcripts exist.
- AC-05: Distill-before-purge ordering holds at ALL FOUR success returns (purged-signals,
  cached-MetricVector, memoization-hit, full-pipeline — pattern #4750, `tools.rs:2110/2236/2925/3027`):
  candidates reflect buffer content present at call time, and the purge still fires after. Error paths
  keep transcripts and produce no candidates (existing behavior preserved). On a memoization hit,
  candidates are distilled from whatever buffer content is present at call time (OQ-4) — they may differ
  from the cached report, which is acceptable and documented in the consumer guidance.
- AC-06: Candidates are response-transient: the memoized `cycle_review_index` record contains no
  candidate or transcript content (a forced re-review of the stored record returns it without stale
  candidates); no SQL write, file write, or log line carries candidate or buffer content (extends the
  vnc-025 AC-12 grep/test gate to the new code paths).
- AC-07: Reconstruction fallback (whole-session either/or per session — OQ-2): an attributed session
  with an empty or hole-ridden buffer (hole/elision threshold defined at design, against vnc-025 ADR-002
  semantics) contributes distillation input assembled from its stored `ObservationRecord`s (tool, input,
  response_snippet), labeled with degraded provenance distinguishable by the consumer; the fallback
  never writes to the byte buffer and never produces observation rows.
- AC-08: Loss is never silent: per-session elided-byte counts, hole indication,
  primary-vs-reconstructed provenance, and cap-forced candidate truncation (per-session volume cap and
  per-cycle aggregate cap — AC-02) appear in the candidates section whenever non-zero/active. A
  cap-forced drop of late candidates is reported, never silent — same principle as elision/hole reporting.
- AC-09: Two-pipe boundary preserved: with distillation active, the 23 detection rules' inputs are
  bit-identical to pre-crt-052 (extends `detection_isolation` tests; the batch filter `listener.rs:1238`
  is unchanged); distillation output reaches the knowledge base only via the calling agent's
  `context_store` writes.
- AC-10: The distill+purge behavior is gated on an exhaustive `TranscriptRetention` match (enterprise
  seam — never an assumed variant; `PurgeOnCycleClose` arm `server.rs:543`, `RetainDays(_)` arm `:551`);
  the `RetainDays` arm neither distills nor purges in OSS (unreachable, rejected at `validate()`).
- AC-11: Transcript continuity (Option B, concrete): after the server-only hold lands, a multi-turn
  session's buffer content streamed across ≥3 simulated turn boundaries (Stop→SessionClose drains) is
  re-adopted on re-registration and available to the cycle-review snapshot; held buffers keep merging
  deltas; memory remains bounded (cap × held sessions); the stale sweep still reclaims held buffers; and
  the moved purge/audit points fire exactly once per held session at review or sweep.
- AC-12: Server-side rule pass over a 4 MiB buffer completes in < 50 ms off-lock (ass-070 estimate:
  single-digit ms); cycle-review latency class unchanged.
- AC-13: Consumer guidance updated (uni-retro skill / protocol cycle-review step): four-family
  extraction instructions with the Q8 folds and the call-time-vs-cached note, storing via
  `context_store` with feature attribution; `cargo audit` passes; no new heavyweight runtime dependency
  (regex-class only).

## Constraints

1. **Registry lock discipline** (vnc-025 constraint 3, ADR-004): microsecond lock holds, no I/O/parse
   under any lock. Snapshot-and-release is non-negotiable.
2. **Secrets posture is architectural** (#4721): candidates are transient response content; any path
   persisting or logging them is rejected at review. The crt-033 memoization persist path is the
   specific trap (AC-06).
3. **Four success returns** (#4750): the handler has four `result.is_ok()` purge sites
   (`tools.rs:2110, 2236, 2925, 3027`); distillation inherits the same shape via one helper.
4. **One buffer content reader — the #700 single-reader invariant (load-bearing, vnc-030 ADR-007 §4 /
   #700):** the buffer's only production content reader today is the PreCompact path
   (`contiguous_tail` → `extract_transcript_block_from_bytes`, `listener.rs:1834-1838`). crt-052's
   snapshot seam (`take_transcripts_for_feature`) becomes the second and **last** content reader.
   Review-time MARKER recovery (#700) is specified to consume this seam and **MUST NOT** add a parallel
   `contiguous_tail`-style reader. This shapes the seam's return type and call contract: it must expose
   the snapshot bytes (and elision/hole metadata) in a form #700 can reuse for marker parsing without
   re-reading the buffer. This is a Constraint, not a mere interaction warning.
5. **The named seam is the only insertion point** (#4742): modify `clear_transcripts_for_feature` (one
   method body) and its caller; no parallel purge/snapshot machinery.
6. **No generation capability server-side**: ONNX embeddings + cross-encoder scoring only; the server
   selects, never extracts (ass-070 Q2 GGUF check).
7. **Claude Code JSONL only**: the parser handles one format; unknown/corrupt lines degrade to
   skip-with-count, never to error (buffer content is untrusted input from the client's disk).
8. **Reconstruction is a fidelity floor, not parity**: 0.81 ceiling, DEC-weakest — provenance labeling
   is mandatory so consumers and future quality measurement can discriminate.
9. **Tail-window-equivalence buffer semantics (vnc-025 ADR-002 #4740 / ADR-008 #4764):** the buffer is
   NOT lossless. Full-content equality holds only below the 4 MiB cap; under ring-tail overflow it
   converges on the tail window, not full content. `clear()` preserves `high_water` and `elided_bytes`;
   deltas below the elided floor are defined no-ops. The distillation window and the
   reconstruction-fallback trigger (hole/elision-keyed) are designed against these semantics, never an
   assumed lossless buffer.
10. **500-line file rule**: `tools.rs`, `session.rs`, `listener.rs` are well over; all new logic lands
    in new focused modules with thin call-site wiring.
11. **Wire contract untouched**: no client changes; `transcript_delta`/`SessionClose` frames are
    consumed as-is. OQ-1 is Option B (server-only); no wire field is added by this feature.
12. **4 MiB buffer cap stands** (ass-070 Q5): no escalation; elision visibility is the guard.
13. **Cite, don't rework, vnc-030's precedence interface (ADR-007 §2 #4819):** crt-052 edits adjacent
    functions (`drain_and_signal_session`, `clear_transcripts_for_feature`) in `session.rs`/`listener.rs`
    that vnc-030 deliberately left untouched; keep diffs minimal against the merged close/sweep
    precedence interface and cite it rather than re-deriving declared-beats-vote logic.
14. **Per-turn drain reality** (Background): acceptance tests must simulate the real lifecycle
    (register → deltas → drain → deltas → re-register → review), not just the happy single-turn path;
    Option B's hold is what makes the primary path non-empty.

## Open Questions

The six prior Open Questions are RESOLVED (binding human decisions, #689, 2026-06-08): OQ-1 = Option B
(server-only hold; Option A off the table); OQ-2 = whole-session either/or per session; OQ-3 = keep
24 KB/session default AND add a per-cycle aggregate cap (config knob); OQ-4 = distill whatever is
present at call time; OQ-5 = lean `unimatrix-observe` (architecture finalizes); OQ-6 = small synthetic
corpus (2–3 sessions, ~20 labeled items) authored independently of the ported regex set. These are
folded into Goals/ACs/Constraints above.

Genuinely still open (design-phase resolution, not blocking scope):

1. **`topic_source` use in session/observation selection** — vnc-030 shipped
   `observations.topic_source` (`declared`/`extracted`/`registry-fill`/`vote`/NULL). Should the
   reconstruction-fallback selection (Goal 5) prefer/filter on `declared`/`registry-fill` rows to avoid
   reconstructing from vote/extracted-misattributed observations, or stay feature-match-only? Optional
   input, no dependency; design decides. (Registry session selection for the primary path is already
   contract-attributed by vnc-030 ADR-007 §2.)
2. **Snapshot seam return shape for #700 reuse** — confirm at architecture that the
   `take_transcripts_for_feature` return type (bytes + elision/hole metadata) is expressed so #700 can
   consume it for marker parsing without re-reading the buffer (Constraint 4). Genuinely a design
   decision; the constraint is binding, the shape is not yet pinned.
3. **Audit-shape change from Option B** — moving the purge/`transcript_session_purged` audit off the
   per-turn close path and onto cycle-review/stale-sweep changes vnc-025's shipped audit timing.
   Confirm the new audit shape is acceptable and that no downstream consumer keys on per-close audits
   (design-phase verification; surfaced for the architect, not blocking scope).

## Tracking

GH Issue: #689. Predecessors SHIPPED: vnc-025 (#670 — buffer + purge + named seam), ass-070 (#683,
FINDINGS GO), vnc-027 (F4a, #680 — TS client + delta streaming to UDS, MERGED), vnc-030 (F4b, #699 —
contractual attribution + close/sweep precedence interface, MERGED via PR #702). Pinned delivery order
was vnc-027 → vnc-030 → **crt-052 (now next)**. Downstream dependent: #700 (review-time MARKER
recovery) consumes crt-052's snapshot seam. Will be updated with session links after Session 1.
