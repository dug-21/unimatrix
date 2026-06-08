# crt-052 Pseudocode Overview — Transcript-Fed Cycle Review Distillation

GH #689. Source-of-truth: ARCHITECTURE.md §2/§3/§4 (binding integration surface), ADR-001..009,
SPECIFICATION.md (FR/NFR/AC), RISK-TEST-STRATEGY.md (R-01..R-20). Where a name or signature appears
in ARCH §4 / brief "Data Structures" / "Function Signatures", it is used verbatim here. The pinned
snapshot type is `TranscriptSnapshot` — never `SessionTranscriptSnapshot`.

## Components (ARCH §2 C1..C10)

| C | File | Target source | Wave |
|---|------|----------------|------|
| C1 | snapshot-seam.md | `unimatrix-server/src/infra/session.rs` (`take_transcripts_for_feature`) | A |
| C2 | snapshot-types.md | `unimatrix-server/src/infra/session_transcript.rs` (`TranscriptSnapshot`, `HoleInfo`, `snapshot()`) | A |
| C3 | selection-module.md | `unimatrix-observe/src/distill/{mod,jsonl,markers,select}.rs` | A |
| C4 | response-types.md | `unimatrix-observe/src/types.rs` (candidate/section types + additive response field) | A |
| C5 | reconstruct.md | `unimatrix-observe/src/distill/reconstruct.rs` | A |
| C6 | distill-handler.md | `unimatrix-server/src/mcp/distill_handler.rs` + thin wiring in `mcp/tools.rs` | A |
| C7 | retention-gate.md | `unimatrix-server/src/server.rs` (`purge_cycle_transcripts` match) | A |
| C8 | held-buffer-store.md | `unimatrix-server/src/infra/transcript_hold.rs` + minimal diffs to `session.rs`/`listener.rs` | **B** |
| C9 | config-knobs.md | `unimatrix-server/src/infra/config.rs` (`RetentionConfig`) | A/B |
| C10 | consumer-guidance.md | `.claude/skills/uni-retro` + cycle-review protocol step | A |

## Data Flow (ARCH §3)

```
context_cycle_review handler (tools.rs — 4 success returns @ :2110 / :2236 / :2925 / :3027)
  │ result.is_ok() at each site (pattern #4750)
  ▼
[C6] distill_before_purge(registry, feature_cycle, &observations, cfg)
  ├─(1) [C7] exhaustive match cfg.transcript_retention
  │         PurgeOnCycleClose => proceed ; RetainDays(_) => return None
  ├─(2) [C1] registry.take_transcripts_for_feature(feature_cycle)
  │         Phase 1 (registry lock): Arc-clone attributed buffers (registered ∪ held under Wave B)
  │         Phase 2 (per-buffer lock): buf.snapshot() -> TranscriptSnapshot   [C2]
  │         returns Vec<(session_id, TranscriptSnapshot)>  ── ALL PARSING AFTER THIS POINT
  ├─(3) per snapshot: fallback_trigger(snap)?  [ADR-006 predicate, shared with C7-loss]
  │         no  -> [C3] select_candidates(&snap.bytes, sid, snap.base_offset, session_cap) -> Primary
  │         yes -> [C5] reconstruct_from_observations(sid, &obs, session_cap) -> Reconstructed
  ├─(4) aggregate; enforce per-cycle cap (deterministic chronological keep-earliest);
  │         build SessionLossInfo per session (elided/holes/provenance + cap-drop)  [C4/ADR-007]
  ├─(5) return Option<TranscriptCandidatesSection>
  ▼
handler attaches section at RESPONSE-ASSEMBLY level (NOT onto memoized RetrospectiveReport)  [C4/ADR-004]
  ▼
purge_cycle_transcripts(...) fires AFTER distill (existing behavior; Wave B adds purge_held_for_feature)
```

Wave B (C8) changes only WHERE the bytes live at step (2): held buffers survive the per-turn drain,
so the snapshot is non-empty for multi-turn sessions. The seam contract (C1/C2) is byte-identical with
and without Wave B.

## Shared Types (defined in C2 + C4; used across components)

```
// C2 — infra/session_transcript.rs (Wave A)
TranscriptSnapshot { bytes: Vec<u8>, base_offset: u64, high_water: u64,
                     elided_bytes: u64, holes: Vec<HoleInfo> }   // manual metadata-only Debug
HoleInfo { start: u64, end: u64 }

// C4 — unimatrix-observe/src/types.rs (Wave A)
TranscriptCandidate { session_id: String, byte_offset: u64, ts: Option<String>,
                      family_hints: Vec<FamilyHint>, text: String }
FamilyHint = Decision | Rework | Lesson | PhaseGate     // advisory only, non-empty per candidate
CandidateProvenance = Primary | Reconstructed
SessionLossInfo { session_id: String, elided_bytes: u64, has_holes: bool,
                  provenance: CandidateProvenance, dropped_candidates: u64 }
TranscriptCandidatesSection { candidates: Vec<TranscriptCandidate>, loss: Vec<SessionLossInfo> }
```

Note on `provenance`: ADR-007's `SessionLossInfo` carries it. The brief's `TranscriptCandidate` struct
does NOT carry a `provenance` field (provenance is per-session, surfaced via `SessionLossInfo`); SPEC
§Domain Models lists `provenance` on the candidate as advisory only. C4 follows the brief/ARCH §4
struct (no per-candidate provenance field) and surfaces provenance per session in `SessionLossInfo`.
`dropped_candidates` carries the cap-forced truncation count (AC-08 — no silent aggregate-cap drop);
flagged as a contract addition in the C4 file.

## Lock-Discipline Summary (Constraint 1 / NFR-1 / AC-01 / R-08, pattern #3753)

Two phases, both microsecond-class, NO I/O or parse under any lock:

1. **Registry lock (C1 phase 1):** linear scan `state.feature.as_deref() == Some(feature_cycle)`;
   `Arc::clone` each matching buffer handle; collect `(session_id, Arc<Mutex<TranscriptBuffer>>)`;
   **release registry lock.**
2. **Per-buffer lock (C1 phase 2 + C2 `snapshot()`):** for each Arc, take buffer lock, byte-copy the
   contiguous span + read metadata (`snapshot()`), release. Poison-recovers per #4764
   (`unwrap_or_else(|p| p.into_inner())`, treat-as-empty + `clear_poison`).
3. **After every lock released:** JSONL parse, marker match, dedup, caps (C3/C5/C6). The byte copy is
   the ONLY content-bearing work under any lock. Re-acquiring a lock in a later step is forbidden
   (#3753) — downstream steps read the owned snapshot, never the handle.

`snapshot()` (C2) is the **second and last** production buffer-content reader; the first is PreCompact
`contiguous_tail` (`listener.rs:1834-1838`). No third reader (Constraint 4, AC-V-SEAM, R-06). #700
reuses `snapshot()`.

## Wave A / Wave B Dependency-Direction Map (ADR-009 / R-11)

```
Wave A (provable pipeline, safe revert target):
  C1 seam ── calls ──> C2 snapshot()        [neither references transcript_hold.rs]
  C6 helper ─ calls ─> C1, C3, C5, C7
  C3 select ─ uses ──> C4 types
  C5 reconstruct ── uses ──> C4 types, ObservationRecord
  C7 gate ── used by ──> C6
  C9 config ── read by ──> C6, C7  (Wave A knobs only)

Wave B (continuity remedy, layered on top):
  C8 transcript_hold.rs  ── depended on by ──> C1 (held scan), session.rs drain, listener.rs delta route
  C9 adds hold knobs (transcript_hold_max_sessions / _ttl_secs)

INVARIANT (R-11, hard merge-gate dependency assertion):
  NO Wave A module (C1 source EXCEPT its held-scan branch, C2, C3, C4, C5, C6, C7) may have a
  compile-time `use`/path reference to transcript_hold.rs.
  C1 is the ONLY seam touching the hold, and only via an injected/optional hold handle so the
  held-scan branch is severable: with Wave B reverted, C1 scans registered buffers only and the
  pipeline degrades cleanly to the C5 reconstruction fallback (AC-07). Reverting C8 + the C1
  held-scan branch + the drain/listener diffs leaves Wave A compiling, tests passing, shipping
  degraded.
```

Per-component files restate their wave and their `transcript_hold.rs` reference status explicitly.

## Sequencing Constraints (build order)

1. C2 (snapshot types + primitive) — everything depends on `TranscriptSnapshot`.
2. C4 (candidate/section/response types) — C3, C5, C6 depend on them.
3. C9 (config knobs) — C6, C7 read them.
4. C3 + C5 (pure observe modules) — independently unit-testable on fixtures.
5. C1 (seam) — depends on C2.
6. C7 (retention gate) — server-side.
7. C6 (handler glue) — depends on C1, C3, C5, C7, C9; wires the four returns.
8. C10 (consumer guidance) — independent doc work.
9. **Wave B last:** C8 (held store) + C1 held-scan branch + drain/listener diffs + C9 hold knobs.
