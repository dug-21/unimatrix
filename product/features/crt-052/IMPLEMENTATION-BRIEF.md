# crt-052 Implementation Brief — Transcript-Fed Cycle Review Distillation

GH Issue: #689. Compiled from the approved Session-1 design artifacts (SCOPE, SPECIFICATION, ARCHITECTURE +
ADR-001..009, RISK-TEST-STRATEGY, ALIGNMENT-REPORT). This brief is the Session-2 entry point: it pins the
one-name decisions, the wave/gate plan, the resolved decisions, and the contract types/signatures delivery
agents implement. Where ARCH §4 (the binding integration-surface table) and this brief agree, ARCH §4 is the
source of truth; where naming forked, this brief pins the single name.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-052/SCOPE.md |
| Scope Risk Assessment | product/features/crt-052/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/crt-052/specification/SPECIFICATION.md |
| Architecture | product/features/crt-052/architecture/ARCHITECTURE.md |
| Risk / Test Strategy | product/features/crt-052/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-052/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/crt-052/ACCEPTANCE-MAP.md |

### ADR Index (file paths — cite these, not pattern IDs)

| ADR | File | Decides |
|-----|------|---------|
| ADR-001 | architecture/ADR-001-snapshot-and-release-seam.md | `take_transcripts_for_feature` returns owned raw snapshots, sibling to `clear` |
| ADR-002 | architecture/ADR-002-snapshot-shape-single-reader.md | `TranscriptSnapshot` shape co-designed for crt-052 selection AND #700 marker parsing; single content reader |
| ADR-003 | architecture/ADR-003-pure-selection-module.md | Selection is a pure module in `unimatrix-observe/src/distill/`; untrusted-input-hardened |
| ADR-004 | architecture/ADR-004-candidates-outside-memoized-struct.md | `transcript_candidates` attached at response-assembly level, outside the memoized struct |
| ADR-005 | architecture/ADR-005-one-helper-four-returns-exhaustive-gate.md | One distill helper at all four success returns, gated on exhaustive `TranscriptRetention` match |
| ADR-006 | architecture/ADR-006-fallback-trigger-and-topic-source.md | Reconstruction trigger keyed to hole/elision; `topic_source` soft preference only |
| ADR-007 | architecture/ADR-007-loss-visibility-provenance.md | Loss visibility + degraded provenance mandatory in the section |
| ADR-008 | architecture/ADR-008-option-b-held-buffer-store.md | Option B held-buffer store: held-count cap + independent TTL, loud re-adoption |
| ADR-009 | architecture/ADR-009-audit-shape-move-and-wave-staging.md | Audit-shape move to review/sweep with no-consumer survey; Wave A/B rollback boundary |

## Component Map

Pseudocode and test-plan files are produced in Session 2 Stage 3a. Components below are the architecture's
C1..C10 (ARCH §2); actual file paths are filled during delivery.

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1 Snapshot seam `take_transcripts_for_feature` (`infra/session.rs`) | pseudocode/snapshot-seam.md | test-plan/snapshot-seam.md |
| C2 Snapshot types `TranscriptSnapshot` / `HoleInfo` + `snapshot()` primitive (`infra/session_transcript.rs`) | pseudocode/snapshot-types.md | test-plan/snapshot-types.md |
| C3 Candidate selection module (`unimatrix-observe/src/distill/`) | pseudocode/selection-module.md | test-plan/selection-module.md |
| C4 Candidate / response types (`unimatrix-observe/src/types.rs`) | pseudocode/response-types.md | test-plan/response-types.md |
| C5 Reconstruction fallback (`distill/reconstruct.rs`) | pseudocode/reconstruct.md | test-plan/reconstruct.md |
| C6 Distill helper / handler glue (`mcp/distill_handler.rs`) | pseudocode/distill-handler.md | test-plan/distill-handler.md |
| C7 TranscriptRetention gate (`server.rs` match) | pseudocode/retention-gate.md | test-plan/retention-gate.md |
| C8 Held-buffer store Option B (`infra/transcript_hold.rs`) | pseudocode/held-buffer-store.md | test-plan/held-buffer-store.md |
| C9 Config knobs (`infra/config.rs`) | pseudocode/config-knobs.md | test-plan/config-knobs.md |
| C10 Consumer guidance (`.claude/skills/uni-retro` + protocol) | pseudocode/consumer-guidance.md | test-plan/consumer-guidance.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Insert a snapshot-and-release distillation pass into `context_cycle_review`, ahead of the existing transcript
purge, so a reviewed feature cycle's conversational narrative (decisions, rework reasoning, phase intent,
human interventions) is harvested before per-session transcript bytes are purged. The server *selects* whole
marker-matched user/assistant blocks (rules select), attaches them as a response-transient
`transcript_candidates` section, and the calling agent does all semantic extraction into `context_store`
(agent extracts). A server-only bounded held-buffer structure (Option B) keeps multi-turn buffers alive across
the per-turn drain so the primary path is non-empty; a labeled reconstruction fallback covers empty/hole-ridden
buffers at a documented 0.81 fidelity floor.

## Delivery Staging — Two Waves, One PR, One Rollback Boundary

Both waves land in a SINGLE PR. Staging exists to give a rollback boundary so a Wave B problem cannot
contaminate the proven Wave A pipeline (ADR-009, RISK R-11).

- **Wave A — the provable pipeline (rollback boundary).** Snapshot seam (`take_transcripts_for_feature`) +
  snapshot types/primitive + pure selection module + four-call-site wiring + response-assembly candidates field
  + reconstruction fallback + retention gate + config knobs. Fully testable on committed fixtures. Degrades
  cleanly to the reconstruction fallback when every buffer is empty at call time. **Contains NO Option B.**
  Wave A modules MUST have zero compile-time reference to `transcript_hold.rs` (R-11 dependency-direction
  assertion). This is the safe revert target.
- **Wave B — the continuity remedy.** The Option B held-buffer state machine (ADR-008) layered on top: buffers
  survive `drain_and_signal_session`, keep merging deltas, re-adopt on re-registration, evict on cap/TTL, purge
  at review/sweep. Wave B is what makes the primary path non-empty in real multi-turn sessions. A Wave B revert
  must leave Wave A compiling, tests passing, shipping degraded.

**ADR-009 no-consumer audit survey is a PREREQUISITE GATE before Wave B moves the audit points.** Named
delivery task: survey `gc_audit_log` (crt-036), retention/analytics readers, and per-close-emission tests to
confirm no downstream consumer keys on per-close `transcript_session_purged` cadence. The audit move must not
merge until this survey is recorded clean (RISK Coverage Gap 1, SR-03).

## Merge Gates (non-negotiable — PR does not merge until all pass)

1. **AC-11 `continuity_simulated_lifecycle`** — a faithful per-turn-drain simulation with **≥3 drain cycles**
   and **deltas applied between each drain** (register → deltas → drain → deltas → drain → deltas → drain →
   re-register → cycle review). This is the ONLY pre-merge proof of the primary path before the dogfooding
   switchover. A single-turn happy path does NOT satisfy it. Asserts: cross-turn content presence (not just
   last turn), loud re-adopt on cycle match / fail-loud on mismatch (R-01), bounded held-count + observable
   eviction (R-02), TTL reclaim independent of review (R-02), exactly-once audit per held session (R-03).
2. **Content-leak gate (AC-06)** — structural absence of candidates from the memoized `RetrospectiveReport`
   (compile-level) + extended grep/log/SQL gate over all new paths + re-review-of-stored-record test
   (candidates absent from the cached path) + content-free audit `detail` + metadata-only `Debug` on snapshot
   types (R-04, R-19).
3. **Four-return exhaustiveness (AC-05)** — distillation wired at all four `result.is_ok()` returns via one
   shared helper, plus a regression test that fails if a fifth success return is added unwired (R-07).
4. **AC-V-FUZZ no-panic (R-10)** — malformed/adversarial JSONL corpus (truncated JSON, non-UTF-8, oversized
   line, unknown record type, embedded NUL) degrades to skip-with-count; parser and handler never panic.
5. **AC-V-SEAM single-reader (R-06)** — source assertion that no third buffer content reader exists (only
   PreCompact `contiguous_tail` + the seam's `snapshot()`); #700-shaped reuse test parsing
   `TranscriptSnapshot.bytes` without invoking `contiguous_tail` (`#700` reuse).
6. **AC-01 snapshot-and-release (R-08)** — no-parse-under-lock source assertion + concurrency/stress test
   streaming deltas during a review (no deadlock, no torn read, consistent snapshot).

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Snapshot seam shape | `take_transcripts_for_feature` returns owned raw `TranscriptSnapshot`s; sibling to `clear_transcripts_for_feature`; parse strictly after all locks released | OQ-2, Constraint 1/5 | architecture/ADR-001-snapshot-and-release-seam.md |
| Single content reader / #700 reuse | Seam returns owned bytes + `(elided_bytes, holes, high_water, base_offset)`; #700 reuses the same `snapshot()` primitive — no third `contiguous_tail`-style reader | OQ-2, Constraint 4, SR-04 | architecture/ADR-002-snapshot-shape-single-reader.md |
| Selection module placement | Pure no-I/O no-lock module in `unimatrix-observe/src/distill/`; untrusted-input-hardened (skip-with-count) | OQ-5, SR-09, Constraint 6/7 | architecture/ADR-003-pure-selection-module.md |
| Candidates vs memoization | `transcript_candidates` attached at response-assembly level, OUTSIDE the memoized `RetrospectiveReport` | AC-06, SR-07 | architecture/ADR-004-candidates-outside-memoized-struct.md |
| Distill wiring + retention gate | One shared helper at all four `result.is_ok()` returns, gated on an exhaustive `TranscriptRetention` match (no wildcard) | AC-05/AC-10, SR-05, Constraint 3 | architecture/ADR-005-one-helper-four-returns-exhaustive-gate.md |
| Fallback trigger / topic_source | Trigger keyed to hole/elision against tail-window-equivalence (NOT losslessness); `topic_source` soft ordering preference only, never a filter | OQ-1(topic_source), SR-06/SR-08, Constraint 9 | architecture/ADR-006-fallback-trigger-and-topic-source.md |
| Loss visibility | Per-session `elided_bytes`/hole/provenance mandatory in the section when non-zero/active | AC-08, Constraint 8 | architecture/ADR-007-loss-visibility-provenance.md |
| Option B held buffer | Bounded server-only store: held-count cap + independent stale-sweep TTL; re-adopt on `feature_cycle` match, fail loud on mismatch | OQ-1, SR-01/SR-02 | architecture/ADR-008-option-b-held-buffer-store.md |
| Audit-shape move + waves | Audit moves to review/sweep/evict (exactly-once); no-consumer survey is a prerequisite gate; Wave A/B rollback boundary | OQ-3, SR-03, Constraint 13 | architecture/ADR-009-audit-shape-move-and-wave-staging.md |
| OQ-1 continuity | Option B (server-only hold); Option A close-reason wire field off the table | SCOPE OQ-1 (#689, 2026-06-08) | architecture/ADR-008-option-b-held-buffer-store.md |
| OQ-2 fallback trigger | Whole-session either/or per session | SCOPE OQ-2 (#689) | architecture/ADR-006-fallback-trigger-and-topic-source.md |
| OQ-3 caps | Keep 24 KB/session default AND add per-cycle aggregate cap (config knob) | SCOPE OQ-3 (#689) | architecture/ADR-005-one-helper-four-returns-exhaustive-gate.md |
| OQ-4 cache-hit semantics | Distill whatever buffer content is present at call time; may differ from cached report (documented) | SCOPE OQ-4 (#689) | architecture/ADR-005-one-helper-four-returns-exhaustive-gate.md |
| OQ-6 fixture independence | Small synthetic corpus authored independently of the ported regex set (anchors-before-port OR different author); committed provenance header is a review gate | SCOPE OQ-6 (#689) | architecture/ADR-003-pure-selection-module.md |
| `byte_offset` semantics | LOGICAL stream offset (`base_offset`-relative), NOT array-relative; meaningful across elision. (Spec listed open; ADR-002 governs — recorded CLOSED.) | ARCH OQ-3, R-12 | architecture/ADR-002-snapshot-shape-single-reader.md |

### Naming Pin (one name — do not fork)

The snapshot return type is named **`TranscriptSnapshot`** (per ARCH §4 / ADR-002). SPEC §Domain Models calls
it `SessionTranscriptSnapshot`; the field sets are compatible. **Delivery uses `TranscriptSnapshot`.** Do not
introduce `SessionTranscriptSnapshot`.

### Delivery-Time Defaults (mechanism pinned in ADRs; values are starting defaults, config-tunable)

| Knob | Starting default | Bounds risk | Source |
|------|------------------|-------------|--------|
| `transcript_hold_max_sessions` | ≈ 64 | R-02 memory bound (held-count ceiling) | ADR-008, ARCH OQ-1 |
| `transcript_hold_ttl_secs` | ≈ 24h (86400) | R-02 memory bound (independent TTL sweep) | ADR-008, ARCH OQ-1 |
| `transcript_candidate_session_cap_bytes` | 24 KB | per-session volume | OQ-3 (pinned), FR-4 |
| `transcript_candidate_cycle_cap_bytes` | ≈ 256 KB | per-cycle aggregate; from ass-070 ~58 KB/6-session envelope | ARCH OQ-2, FR-4 |

Per-cycle aggregate-cap truncation order: **deterministic, chronological keep-earliest** (unless the
delivery-stage architecture pseudocode pins otherwise). Must be repeatable and testable (R-15).

## Files to Create / Modify

New focused modules (all substantive logic — Constraint 10 / 500-line rule; #693):

- `crates/unimatrix-server/src/infra/transcript_hold.rs` — NEW. Option B held-buffer store (Wave B).
- `crates/unimatrix-server/src/mcp/distill_handler.rs` — NEW. Thin distill helper called at the four returns.
- `crates/unimatrix-observe/src/distill/mod.rs` — NEW. Distill module root.
- `crates/unimatrix-observe/src/distill/jsonl.rs` — NEW. Claude Code JSONL parse, untrusted-input-hardened.
- `crates/unimatrix-observe/src/distill/markers.rs` — NEW. Four marker families (~50 regex, ported from ass-070 `extractor.py`).
- `crates/unimatrix-observe/src/distill/select.rs` — NEW. `select_candidates` pure entry.
- `crates/unimatrix-observe/src/distill/reconstruct.rs` — NEW. `reconstruct_from_observations` fallback.

Modify (thin wiring only — these files are over 500 lines):

- `crates/unimatrix-server/src/infra/session.rs` — add `take_transcripts_for_feature`; minimal diff to `drain_and_signal_session` for Wave B hold (cite vnc-030 ADR-007 §2 #4819, do not rework precedence).
- `crates/unimatrix-server/src/infra/session_transcript.rs` — add `TranscriptSnapshot`, `HoleInfo`, `snapshot()` / `snapshot_block()` on `TranscriptBuffer`.
- `crates/unimatrix-observe/src/types.rs` — add `TranscriptCandidate`, `FamilyHint`, `CandidateProvenance`, `SessionLossInfo`, `TranscriptCandidatesSection`; additive optional response field.
- `crates/unimatrix-server/src/mcp/tools.rs` — wire the one helper at the four `result.is_ok()` returns (`tools.rs:2110/2236/2925/3027`); attach section at assembly level.
- `crates/unimatrix-server/src/server.rs` — extend exhaustive `TranscriptRetention` match in `purge_cycle_transcripts` (`:543`/`:551`).
- `crates/unimatrix-server/src/infra/config.rs` — add the four config knobs to `RetentionConfig` (`serde(default)`/`validate()`/merge pattern, beside `transcript_buffer_max_bytes` `:1561-1576`).
- `crates/unimatrix-server/src/listener.rs` — minimal Wave B diff for held-buffer delta routing / drain hook; batch filter `:1238` UNCHANGED (two-pipe boundary).
- `.claude/skills/uni-retro` + cycle-review protocol step — consumer guidance (four families, Q8 folds, call-time-vs-cached note).

## Data Structures (contract — ARCH §4 is binding)

```rust
// infra/session_transcript.rs  (Wave A)
pub struct TranscriptSnapshot {
    pub bytes: Vec<u8>,        // owned readable window; parse only after lock release
    pub elided_bytes: u64,     // lifetime elision counter snapshot
    pub holes: Vec<HoleInfo>,
    pub high_water: u64,
    pub base_offset: u64,      // byte_offset is base_offset-relative (LOGICAL stream offset)
}                              // manual metadata-only Debug — NO content (R-19)
pub struct HoleInfo { pub start: u64, pub end: u64 }

// unimatrix-observe/src/types.rs  (Wave A)
pub struct TranscriptCandidate {
    pub session_id: String,
    pub byte_offset: u64,      // LOGICAL = base_offset + in_snapshot_offset (R-12)
    pub ts: Option<String>,
    pub family_hints: Vec<FamilyHint>,   // advisory, non-empty
    pub text: String,                    // whole matched block, unwindowed
}
pub enum FamilyHint { Decision, Rework, Lesson, PhaseGate }
pub enum CandidateProvenance { Primary, Reconstructed }
pub struct SessionLossInfo {
    pub session_id: String,
    pub elided_bytes: u64,
    pub has_holes: bool,
    pub provenance: CandidateProvenance,
}
pub struct TranscriptCandidatesSection {
    pub candidates: Vec<TranscriptCandidate>,
    pub loss: Vec<SessionLossInfo>,
}
// additive on the cycle-review response, attached at assembly level (NOT on RetrospectiveReport):
//   #[serde(skip_serializing_if = "Option::is_none")] transcript_candidates: Option<TranscriptCandidatesSection>
```

## Function Signatures (contract — ARCH §4 is binding)

```rust
// C1 seam (infra/session.rs)
fn take_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<(String, TranscriptSnapshot)>;
// C2b primitive (TranscriptBuffer)
fn snapshot(&self) -> TranscriptSnapshot;   // byte copy + metadata under buffer lock; poison-recovers per #4764
// C3 selection (distill/select.rs) — pure, no I/O, no lock
fn select_candidates(bytes: &[u8], session_id: &str, base_offset: u64, session_cap: usize) -> Vec<TranscriptCandidate>;
// C5 reconstruction (distill/reconstruct.rs)
fn reconstruct_from_observations(session_id: &str, obs: &[ObservationRecord], session_cap: usize) -> Vec<TranscriptCandidate>;
// C8 held store (infra/transcript_hold.rs) — Wave B
fn hold_on_drain(&self, session_id: &str, buf: Arc<Mutex<TranscriptBuffer>>);
fn readopt(&self, session_id: &str) -> Option<Arc<Mutex<TranscriptBuffer>>>;
fn sweep_expired(&self, ttl: Duration) -> Vec<TranscriptPurgeRecord>;
fn purge_held_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptPurgeRecord>;
// C6 helper (mcp/distill_handler.rs)
fn distill_before_purge(registry, feature_cycle, &observations, cfg) -> Option<TranscriptCandidatesSection>;
```

## Constraints (binding on delivery)

1. **Lock discipline (C1):** microsecond holds; Arc-clone under registry lock, byte copy under buffer lock, ALL parsing/marker matching after every lock release. No I/O or parse under any lock (NFR-1, AC-01).
2. **Secrets posture (C2, #4721):** candidates are transient response content; the crt-033 memoization persist (`store_cycle_review` → `cycle_review_index`, #3793) is the trap. Attach outside the memoized struct; metadata-only `Debug`; content-free audit (AC-06).
3. **Four success returns (C3, #4750):** one helper at all four `result.is_ok()` purge sites (`tools.rs:2110/2236/2925/3027`); exhaustiveness test against a fifth.
4. **One buffer content reader (C4, vnc-030 ADR-007 §4 / #700):** the seam's `snapshot()` is the second and LAST production content reader (existing: PreCompact `contiguous_tail`, `listener.rs:1834-1838`). #700 reuses the snapshot; no third reader.
5. **Named seam only (C5, #4742):** modify `clear_transcripts_for_feature` + caller; no parallel purge/snapshot machinery.
6. **No generation server-side (C6):** ONNX embeddings + cross-encoder only; rules select, never extract.
7. **Claude Code JSONL only (C7):** unknown/corrupt lines → skip-with-count, never error/panic; operate on `&[u8]`; tolerate truncated final line (untrusted client-disk input, AC-V-FUZZ).
8. **Reconstruction is a fidelity floor (C8):** 0.81 ceiling, DEC-weakest; provenance labeling mandatory.
9. **Tail-window-equivalence (C9, #4740/#4764):** buffer is NOT lossless; full-content equality only below the 4 MiB cap. Fallback trigger and distillation window designed against ADR-002 semantics; cite **#4764 active**, not the superseded #4746.
10. **500-line rule (C10):** `tools.rs`/`session.rs`/`listener.rs` over the limit (#693) — new logic in new focused modules, thin wiring only.
11. **Wire contract untouched (C11):** no client change, no new wire field (OQ-1 = Option B server-only).
12. **4 MiB buffer cap stands (C12):** no escalation; elision visibility is the guard.
13. **Cite, don't rework vnc-030 precedence (C13):** minimal diffs to `drain_and_signal_session` / `clear_transcripts_for_feature` against vnc-030 ADR-007 §2. Note: ADR-007 (#4819) shows a stale `deprecated` label, but its §2 close/sweep interface and §4 single-reader pin are **binding per merged code (PR #702) and #700** — cite the contract, not the label.
14. **Per-turn drain reality (C14):** acceptance tests simulate the real lifecycle (register → deltas → drain → deltas → re-register → review), not the happy single-turn path. Option B's hold makes the primary path non-empty (AC-11).

## Dependencies

- **Crates:** `unimatrix-observe` (selection/reconstruct modules beside `synthesis.rs`/`phase_narrative.rs`; pure), `unimatrix-server` (handler/registry/drain/purge/config wiring), `unimatrix-core` (`ObservationRecord`), a regex-class crate only (AC-13 / NFR-6 — no new heavyweight runtime dep; `cargo audit` passes).
- **Shipped predecessor interfaces:** vnc-025 `TranscriptBuffer` / `clear_transcripts_for_feature` / `purge_cycle_transcripts` (#670); vnc-030 contractual attribution + close/sweep precedence (ADR-007 §2, #4819, #699 / PR #702); crt-033 cycle-review memoization (#3793).
- **Decisions consulted:** #4742 (named seam / take-shaped), #4750 (four returns), #4740/**#4764** (tail-window-equivalence — #4764 active; vnc-025 ADR-008's superseded entry is #4746, which the SCOPE body still cites — use #4764), #4721 (secrets posture), #4819 (cross-feature seam contracts §2/§4), #4799 (per-turn drain starvation), #3793/#3800 (memoization persist + cache-hit deserialize), #3753 (use pre-cloned snapshot, never re-acquire a lock).
- **Lifecycle references:** #981 (NULL/mis-set `feature_cycle` silently breaks retrospective — cite for re-adoption fail-loud, SR-02); #3359 (threshold/window mismatch over-fires — SR-08).
- **Downstream dependent:** #700 (review-time MARKER recovery) consumes the snapshot seam (Constraint 4).

## NOT in Scope

- Server-side LLM/semantic extraction or family classification — rules select, hints advisory only.
- Changes to the 23 detection rules; their inputs stay bit-identical (two-pipe boundary, AC-09).
- Multi-provider transcript parsing — Claude Code JSONL only. Sidechain / sub-agent transcripts — main-thread only.
- Excerpt windowing, spawn-prompt channel, tool/bash channels.
- Decision→outcome linking, #556 declared-but-never-closed grounding, cross-session cycle stitching.
- Wall-clock anomaly explanation, auto-drafted lesson/ADR text.
- Buffer cap changes / periodic distill-and-truncate — 4 MiB stands.
- Persistence of candidates or raw transcript in any form.
- Client / wire changes; Option A close-reason wire field off the table.
- Re-implementing vnc-030 declared-beats-vote precedence — cite, don't rework.
- Review-time MARKER recovery (#700) — crt-052 ships the consumable seam only.
- Full registry lifecycle redesign — per-turn drain / register-overwrite amnesia remain named-not-fixed except the narrow Option B continuity remedy.
- `topic_source` as a hard filter — soft ordering preference only (SR-06).

## Alignment Status

ALIGNMENT-REPORT.md: **PASS** (5 PASS, 1 WARN, 0 VARIANCE, 0 FAIL). Strategic goal `goal:self-learning`
(#4677), confirmed on #689. No variances require human approval.

- **Vision Alignment — PASS.** Advances self-learning (recovers the 65% / 28-of-43 uncurated session value);
  honors secrets-posture (#8/#4721), graceful degradation (#5), in-memory hot path (#7), single-binary (#6),
  no server-side generation.
- **Milestone Fit — PASS.** Next-up after vnc-027/vnc-030 (both merged); builds only on shipped interfaces;
  #700 deferred, ships only the consumable seam.
- **Scope Additions — WARN (no approval needed).** The two held-buffer config knobs
  (`transcript_hold_max_sessions`, `transcript_hold_ttl_secs`) and Wave A/B staging are SCOPE-derived (the
  concrete mechanism Goal 8 + SR-01 demand), not net-new scope. Noted for awareness only.
- **Architecture / Spec / Risk Consistency — PASS.** Integration-surface table binding and consistent across
  ARCH ↔ SPEC ↔ RISK; the only nit is the `TranscriptSnapshot` vs `SessionTranscriptSnapshot` naming fork,
  pinned to `TranscriptSnapshot` above.
