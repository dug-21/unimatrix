# crt-052 Architecture: Transcript-Fed Cycle Review Distillation

GH Issue: #689. Design restart on the re-verified SCOPE (six OQs resolved; three positions pinned).
Predecessors shipped: vnc-025 (#670, buffer+purge+named seam), ass-070 (#683, GO),
vnc-027 (#680), vnc-030 (#699/PR #702). Downstream consumer: #700 (review-time MARKER recovery).

This document is the "what" and the integration surface. The "why" lives in the ADRs
(`ADR-001`…`ADR-009`), each a single decision, cross-referenced here.

---

## 1. System Overview

`context_cycle_review` today analyzes only structured `ObservationRecord`s (quantitative, mute on
causation). vnc-025 built a per-session in-memory transcript byte buffer and a purge lifecycle that
destroys those bytes at review. crt-052 inserts a **distill-before-purge** pass: at every cycle-review
success, it snapshots the raw transcript bytes of every session attributed to the reviewed
`feature_cycle`, selects whole marker-matched user/assistant blocks server-side (rules select, the
agent extracts), and returns them as an additive `transcript_candidates` response section. The agent
performs all semantics and stores results via `context_store`. Raw transcript and candidates are never
persisted (the in-memory + purge posture is the secrets guarantee, #4721).

Two structural problems shape the design:

1. **The buffer is starved at review time** (#4799). Claude Code fires `Stop` per assistant turn; the
   hook maps `Stop`→`SessionClose`; every close runs `drain_and_signal_session`, freeing the buffer.
   For any multi-turn session the buffer is empty at review. The remedy is **Option B**, a server-only
   bounded transcript hold (Goal 8, OQ-1 resolved) — buffers survive drain, keep merging, re-adopt on
   re-register, and purge only at cycle review / stale sweep.
2. **#700 (MARKER recovery) must reuse the same snapshot seam** without opening a third buffer content
   reader (Constraint 4 / vnc-030 ADR-007 §4). The seam's return type is co-designed for both
   consumers now (`ADR-002`).

### Delivery staging (informs component boundaries)

The system is cut so delivery can ship two waves in one PR behind a rollback boundary:

- **Wave A — the provable pipeline**: snapshot seam + selection module + four-call-site wiring +
  response field + reconstruction fallback. Fully testable on committed fixtures with **no** Option B.
- **Wave B — the continuity remedy**: the Option B held-buffer state machine layered on top.

Wave A is correct and shippable even if every buffer is empty at call time (it degrades to the
reconstruction fallback). Wave B is what makes the primary path non-empty in real multi-turn sessions.
Components are structured so Wave A has zero compile/test dependency on Wave B (`ADR-009`).

---

## 2. Component Breakdown

| # | Component | Crate / location (new unless noted) | Responsibility | Wave |
|---|-----------|-------------------------------------|----------------|------|
| C1 | **Snapshot seam** `take_transcripts_for_feature` | `unimatrix-server/src/infra/session.rs` (sibling to existing `clear_transcripts_for_feature`) | Under registry lock: Arc-clone attributed buffers. After release: per-buffer byte copy + elision/hole metadata read. Returns OWNED RAW snapshots. THE single (second-and-last) content reader. | A |
| C2 | **Snapshot types** `TranscriptSnapshot`, `HoleInfo` | `unimatrix-server/src/infra/session_transcript.rs` (new method `snapshot()` on `TranscriptBuffer`) | Owned `(bytes, elided_bytes, hole_info)` per session — the seam's return contract, shaped for both crt-052 selection AND #700 marker parsing. | A |
| C2b | `snapshot_block()` on `TranscriptBuffer` | `session_transcript.rs` | The content-extraction primitive both consumers call. Mirrors `contiguous_tail` semantics (never crosses a hole, never returns zero-fill) but returns the full snapshotted span with metadata. | A |
| C3 | **Candidate selection module** | new `unimatrix-observe/src/distill/` (e.g. `mod.rs`, `jsonl.rs`, `markers.rs`, `select.rs`) | Pure, no-I/O, no-lock: bytes → Claude Code JSONL parse → keep user/assistant text blocks → four marker families → dedup → per-session cap → per-cycle aggregate cap → ordered `Vec<TranscriptCandidate>`. Untrusted-input-hardened (skip-with-count). | A |
| C4 | **Candidate / response types** | `unimatrix-observe/src/types.rs` (additive) | `TranscriptCandidate`, `TranscriptCandidatesSection`, `SessionLossInfo`, `CandidateProvenance` enum. Additive-optional field on the cycle-review response, `skip_serializing_if`. | A |
| C5 | **Reconstruction fallback** | new fn in `unimatrix-observe/src/distill/reconstruct.rs` | When a session's snapshot is empty/hole-ridden past threshold: build distillation input from already-loaded `ObservationRecord`s; label degraded provenance. Never writes the buffer, never produces observation rows. `topic_source` is a soft ordering preference only. | A |
| C6 | **Distill helper (handler glue)** | new `unimatrix-server/src/mcp/distill_handler.rs` (thin), called from `mcp/tools.rs` | One helper invoked at all four `result.is_ok()` success returns, BEFORE `purge_cycle_transcripts`. Orchestrates C1→C3/C5, attaches C4 at response-assembly level (outside the memoized struct). | A |
| C7 | **TranscriptRetention gate** | extends existing match in `purge_cycle_transcripts` (`server.rs:543`) | Exhaustive `TranscriptRetention` match gates both distill+purge; `RetainDays(_)` arm unreachable in OSS (rejected at `validate()`). | A |
| C8 | **Held-buffer store (Option B)** | new `unimatrix-server/src/infra/transcript_hold.rs` | Bounded server-only structure: buffers survive `drain_and_signal_session`, keep merging deltas, re-adopt on re-register, evict on cap/TTL, purge at review/sweep. Held-count cap + independent stale-sweep TTL (SR-01). | B |
| C9 | **Config knobs** | extends `RetentionConfig` (`infra/config.rs`) | `transcript_candidate_session_cap_bytes` (default 24 KB), `transcript_candidate_cycle_cap_bytes`, `transcript_hold_max_sessions`, `transcript_hold_ttl_secs`. Same `serde(default)`/`validate()`/merge pattern as `transcript_buffer_max_bytes`. | A/B |
| C10 | **Consumer guidance** | `.claude/skills/uni-retro` + cycle-review protocol step | Four-family extraction instructions, Q8 folds, call-time-vs-cached note. | A |

Files at/over the 500-line limit (`tools.rs`, `session.rs`, `listener.rs`) get **thin** wiring only;
all new logic lands in the new focused modules above (Constraint 10, #693).

---

## 3. Component Interactions / Data Flow

```
context_cycle_review handler (tools.rs, 4 success returns)
   │  result.is_ok()  (each of the 4 sites — pattern #4750)
   ▼
[C6 distill_handler::distill_before_purge(registry, feature_cycle, &observations, cfg)]
   │
   ├─(1)─► [C7] match cfg.retention.transcript_retention
   │            PurgeOnCycleClose => proceed ; RetainDays(_) => unreachable (validate-rejected)
   │
   ├─(2)─► [C1] registry.take_transcripts_for_feature(feature_cycle)
   │            Phase 1 (registry lock): Arc-clone attributed buffers      ── microseconds
   │            Phase 2 (per-buffer lock): buf.snapshot() → TranscriptSnapshot{bytes,elided,holes}
   │            returns Vec<(session_id, TranscriptSnapshot)>   ── ALL PARSING AFTER THIS POINT
   │
   ├─(3)─► for each snapshot:
   │          empty/hole-ridden past threshold (ADR-006)?
   │            no  → [C3] select_candidates(&bytes) → primary candidates
   │            yes → [C5] reconstruct_from_observations(session, &observations) → degraded candidates
   │
   ├─(4)─► aggregate, enforce per-cycle cap, attach SessionLossInfo (elided/holes/provenance)
   │            → TranscriptCandidatesSection
   │
   ├─(5)─► attach section to response AT ASSEMBLY LEVEL (NOT into the memoized RetrospectiveReport) ─ AC-06
   │
   └─(6)─► purge_cycle_transcripts(...) fires AFTER distill (existing behavior) ─ AC-05
```

Option B (Wave B) changes only **where the bytes live** at step (2): with the hold active,
`take_transcripts_for_feature` sees held buffers that survived per-turn drains, so the snapshot is
non-empty for multi-turn sessions. The seam contract is unchanged.

### Two-pipe boundary (AC-09, unchanged)

Transcript deltas flow `apply_transcript_delta` (`listener.rs:953` single, `:1206` batch tee); the
batch filter `event_type != TRANSCRIPT_DELTA_EVENT` (`listener.rs:1238`) keeps delta bytes out of
`insert_observations_batch` (`:1248`). crt-052 adds **no** observation field sourced from buffer
content; the 23 detection rules' inputs stay bit-identical. Distillation output reaches the knowledge
base only via the agent's `context_store`.

---

## 4. Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| Existing seam (counts-only today) | `clear_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptPurgeRecord>` | `infra/session.rs:299` (vnc-025 ADR-004 #4742) |
| **New seam (C1)** | `take_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<(String, TranscriptSnapshot)>` | NEW `infra/session.rs` |
| **New snapshot type (C2)** | `pub struct TranscriptSnapshot { pub bytes: Vec<u8>, pub elided_bytes: u64, pub holes: Vec<HoleInfo>, pub high_water: u64, pub base_offset: u64 }` — manual metadata-only `Debug` (no content, SR-02 / ADR-002) | NEW `infra/session_transcript.rs` |
| **Hole metadata (C2)** | `pub struct HoleInfo { pub start: u64, pub end: u64 }` | NEW `infra/session_transcript.rs` |
| **Snapshot primitive (C2b)** | `fn snapshot(&self) -> TranscriptSnapshot` on `TranscriptBuffer` — copies contiguous span + metadata under the buffer lock; poison-recovers per ADR-008 (#4764) | NEW method, `infra/session_transcript.rs` |
| Existing buffer fields consumed | `base_offset: u64`, `high_water: u64`, `elided_bytes: u64`, `holes: Vec<(u64,u64)>`, `data: Vec<u8>` | `infra/session_transcript.rs` (vnc-025 ADR-002 #4740) |
| Existing single content reader (NOT extended) | `contiguous_tail(window) -> Option<Vec<u8>>` → `extract_transcript_block_from_bytes` | `listener.rs:1834-1838` (PreCompact path) |
| **Candidate (C4)** | `pub struct TranscriptCandidate { pub session_id: String, pub byte_offset: u64, pub ts: Option<String>, pub family_hints: Vec<FamilyHint>, pub text: String }` | NEW `unimatrix-observe/src/types.rs` |
| **Family hint (C4)** | `pub enum FamilyHint { Decision, Rework, Lesson, PhaseGate }` (advisory only) | NEW `types.rs` |
| **Provenance (C4)** | `pub enum CandidateProvenance { Primary, Reconstructed }` | NEW `types.rs` |
| **Per-session loss (C4)** | `pub struct SessionLossInfo { pub session_id: String, pub elided_bytes: u64, pub has_holes: bool, pub provenance: CandidateProvenance }` | NEW `types.rs` |
| **Response section (C4)** | `pub struct TranscriptCandidatesSection { pub candidates: Vec<TranscriptCandidate>, pub loss: Vec<SessionLossInfo> }` — additive field `#[serde(skip_serializing_if = "Option::is_none")] transcript_candidates: Option<TranscriptCandidatesSection>` on the response, attached at assembly level | NEW field on cycle-review response struct |
| **Selection entry (C3)** | `fn select_candidates(bytes: &[u8], session_id: &str, base_offset: u64, session_cap: usize) -> Vec<TranscriptCandidate>` — pure, no I/O, no lock | NEW `unimatrix-observe/src/distill/select.rs` |
| **Reconstruction (C5)** | `fn reconstruct_from_observations(session_id: &str, obs: &[ObservationRecord], session_cap: usize) -> Vec<TranscriptCandidate>` | NEW `unimatrix-observe/src/distill/reconstruct.rs` |
| Observations input | `ObservationRecord { ts, event_type, source_domain, session_id, tool, input, response_size, response_snippet }` | `unimatrix-core/src/observation.rs:21`; loaded by `load_cycle_observations` (`services/observation.rs:308`) |
| `topic_source` (soft preference) | `observations.topic_source ∈ {declared, extracted, registry-fill, vote, NULL}` — ORDERING preference for reconstruction selection, NEVER a filter (SR-06) | vnc-030 ADR-004 #4816 |
| Purge gate | `purge_cycle_transcripts(...)`; exhaustive `match cfg.retention.transcript_retention { PurgeOnCycleClose @ server.rs:543 => …, RetainDays(_) @ :551 => … }` | `server.rs:541` |
| Memoization persist (the trap) | `store_cycle_review()` synchronous write to `cycle_review_index` (crt-033 ADR-001 #3793) — candidates MUST NOT ride this | `crt-033` |
| Close/sweep precedence (cite, don't rework) | `process_session_close` (`listener.rs:2069`, drain @ `:2133`), `sweep_stale_sessions` (`session.rs:687`) — declared-beats-vote, minimal-diff | vnc-030 ADR-007 §2 #4819 |
| Drain (Option B wraps, minimal diff) | `drain_and_signal_session(...) -> Option<(SignalOutput, Option<TranscriptPurgeRecord>)>` | `session.rs:651` |
| **Held store (C8)** | `transcript_hold.rs`: `hold_on_drain(session_id, Arc<Mutex<TranscriptBuffer>>)`, `readopt(session_id) -> Option<Arc<…>>`, `sweep_expired(ttl) -> Vec<TranscriptPurgeRecord>`, `purge_held_for_feature(feature_cycle) -> Vec<TranscriptPurgeRecord>` | NEW `infra/transcript_hold.rs` |

This table is binding for downstream agents: names and types here are the contract — do not invent
alternatives.

---

## 5. Technology Decisions (ADR index)

| ADR | Title | Resolves / Addresses |
|-----|-------|----------------------|
| ADR-001 | Snapshot-and-release seam: `take_transcripts_for_feature` returns owned raw snapshots, sibling to `clear` | AC-01, Constraint 1/5, SR-04 (lock discipline half) |
| ADR-002 | `TranscriptSnapshot` shape co-designed for crt-052 selection AND #700 marker parsing — the single content reader | OQ-2, SR-04, Constraint 4 |
| ADR-003 | Candidate selection is a pure module in `unimatrix-observe/src/distill/`; untrusted-input-hardened | Goal 2, OQ-5, SR-09, Constraint 6/7 |
| ADR-004 | `transcript_candidates` attached at response-assembly level, outside the memoized struct | AC-06, SR-07 |
| ADR-005 | One distill helper at all four success returns, gated on exhaustive `TranscriptRetention` match | AC-05, AC-10, SR-05, Constraint 3 |
| ADR-006 | Reconstruction-fallback trigger keyed to hole/elision state (tail-window-equivalence), not losslessness; `topic_source` soft preference | AC-07, OQ-1(topic_source), SR-06, SR-08, Constraint 9 |
| ADR-007 | Loss visibility + degraded provenance are mandatory in the section | AC-08, Constraint 8 |
| ADR-008 | Option B held-buffer store: held-count cap + independent stale-sweep TTL, loud re-adoption | Goal 8, AC-11, SR-01, SR-02 |
| ADR-009 | Audit-shape move to review/sweep with named no-consumer verification; Wave A/B rollback boundary | OQ-3, SR-03, Constraint 13, delivery staging |

---

## 6. How the top risks are addressed

- **SR-01 (held-buffer memory bound)** — `ADR-008`: the hold has an explicit `transcript_hold_max_sessions`
  cap AND an independent `transcript_hold_ttl_secs` stale-sweep TTL. Memory is bounded by
  `buffer_cap × max_sessions` regardless of whether any cycle review ever fires. Cap-hit eviction is
  oldest-last-activity-first, and evicted buffers emit the purge audit (so eviction is never silent).
  Reclamation does NOT depend on cycle-review.
- **SR-04 (#700 seam coupling)** — `ADR-002`: the seam returns owned raw bytes + `(elided_bytes, holes,
  high_water, base_offset)` as `TranscriptSnapshot`, NOT pre-filtered candidates. crt-052's selection
  and #700's marker parsing are two separate consumers of the same `snapshot()` primitive; no third
  `contiguous_tail`-style reader is opened.
- **SR-07 (memoization secrets breach)** — `ADR-004`: candidates are attached to the response at
  assembly level, strictly outside the `RetrospectiveReport` that `store_cycle_review()` persists. A
  forced re-review of the stored record returns no candidates. A content-leak grep/log gate (extending
  vnc-025 AC-12) covers the new code paths. The memoized struct never holds candidate/buffer content.

---

## 7. Open Questions (for spec / risk phases)

1. **Held-buffer cap default value (SR-01)** — `ADR-008` fixes the mechanism (cap + TTL) but the default
   `transcript_hold_max_sessions` and `transcript_hold_ttl_secs` values are a spec/tuning call. Suggest
   defaults sized to dogfooding (e.g. 64 sessions, 24 h TTL) and made config-overridable; spec should
   pin the numbers and the cap-hit eviction test.
2. **Per-cycle aggregate cap default (OQ-3)** — session cap default is 24 KB (pinned). The per-cycle
   aggregate cap default is unset; spec should pick it from ass-070's ~58 KB/6-session envelope
   (suggest ~256 KB) and define truncation order when hit (chronological keep-earliest vs family-priority).
3. **`base_offset` as `byte_offset` provenance** — candidate `byte_offset` is reported relative to the
   buffer's logical `base_offset`. Under ring-tail overflow `base_offset > 0`; spec should confirm the
   consumer treats `byte_offset` as logical (offset-into-session-stream) not array-relative, so
   provenance stays meaningful across elision.
4. **Audit consumer survey (SR-03 / OQ-3)** — `ADR-009` names the verification (no downstream consumer
   keys on per-close `transcript_session_purged` cadence). The spec must record the survey result as a
   gate condition before the audit points move.
5. **Fixture independence enforcement (AC-03 / OQ-6)** — architecturally the selection module is pure
   and fixture-testable; the independence guard (anchors authored before the regex port, or different
   author) is a spec/test-authoring obligation the spec must make an explicit gate, not a nicety.
