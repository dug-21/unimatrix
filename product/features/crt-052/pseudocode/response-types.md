# C4 — Candidate / Response Types

**Target source:** `unimatrix-observe/src/types.rs` (additive; beside `RetrospectiveReport` @ `:381`)
**Wave:** A — **NO reference to `transcript_hold.rs`.**
**ADRs:** ADR-004 (outside memoized struct), ADR-007 (loss/provenance). **Risks:** R-04, R-15.
**AC:** AC-04, AC-06, AC-08. **Sequencing:** after C2, before C3/C5/C6.

## Purpose

Define the response-transient candidate types and the additive optional response field. These types
are NEVER persisted (AC-06): the candidate section is attached at response-assembly level, OUTSIDE the
memoized `RetrospectiveReport` (ADR-004).

## New Types (ARCH §4 — binding)

```
struct TranscriptCandidate {
    session_id:   String
    byte_offset:  u64                  // LOGICAL = snapshot.base_offset + in_snapshot_offset (R-12)
    ts:           Option<String>       // block timestamp from JSONL record; ordering key
    family_hints: Vec<FamilyHint>      // advisory, non-empty; server never authoritative
    text:         String               // the whole matched user/assistant block, unwindowed
}

enum FamilyHint { Decision, Rework, Lesson, PhaseGate }   // advisory only (Non-Goal: server classifies)

enum CandidateProvenance { Primary, Reconstructed }

struct SessionLossInfo {
    session_id:         String
    elided_bytes:       u64                  // from TranscriptSnapshot.elided_bytes
    has_holes:          bool                 // from TranscriptSnapshot.holes (non-empty)
    provenance:         CandidateProvenance  // Primary (buffer) | Reconstructed (observations)
    dropped_candidates: u64                  // CONTRACT ADDITION — see note; cap-forced drop count (AC-08)
}

struct TranscriptCandidatesSection {
    candidates: Vec<TranscriptCandidate>
    loss:       Vec<SessionLossInfo>
}
```

### Additive response field (ADR-004 / FR-5 / AC-04)

On the cycle-review RESPONSE struct (the wire/MCP result the handler assembles), NOT on
`RetrospectiveReport`:

```
#[serde(skip_serializing_if = "Option::is_none")]
transcript_candidates: Option<TranscriptCandidatesSection>
```

- Absent (omitted from JSON, not null/empty) when no session yields candidates (AC-04).
- `RetrospectiveReport` (the memoized type written by `store_cycle_review()` → `cycle_review_index`,
  #3793) gains **NO** candidate field. The leak is structurally impossible because the persisted type
  has no slot (ADR-004). Reviewers police that no future migration folds candidates onto it.

### `Debug` (R-19, AC-06)

`TranscriptCandidate.text` MAY appear in `Debug` (RATIFIED at Gate 3a, ADR-007 — supersedes the earlier
metadata-only-Debug-for-`TranscriptCandidate` proposal and closes this file's pseudocode↔test-plan
contradiction). `text` IS the response content the agent consumes; the R-19/AC-06 metadata-only-Debug
rule targets the SNAPSHOT and held-buffer types (`TranscriptSnapshot`/`HoleInfo` per ADR-002,
`HeldBuffer` per ADR-008) to prevent raw-buffer-content leak — it does NOT apply to this response value,
which structurally cannot reach a persisted/log surface (ADR-004; the AC-06 leak gate tests
SQL/log/audit/persisted surfaces, where candidates never land). `TranscriptCandidate` therefore MAY
`derive(Debug)`; `SessionLossInfo`, `FamilyHint`, `CandidateProvenance` carry no content and may
`derive(Debug)`. The `test_candidate_debug_present_text_is_intentional` test-plan position is the
governing one.

## Contract Note — `dropped_candidates` field (RATIFIED at Gate 3a)

AC-08 requires that cap-forced candidate truncation (per-session OR per-cycle aggregate cap) is
surfaced, never silent. The original ARCH §4 / ADR-007 `SessionLossInfo` listed only `{session_id,
elided_bytes, has_holes, provenance}`, which could not surface aggregate-cap drops. The
`dropped_candidates: u64` field is RATIFIED (ADR-007 + ARCH §4 updated): a content-free count that
rides the same response-transient never-persisted path, the minimal surfacing to meet AC-08. C6
populates it (it holds both caps and the pre-/post-cap counts).

## Ordering (FR / SPEC §Domain Models)

`candidates` is ordered chronologically by `(ts, session_id, byte_offset)`. C3 orders within a session;
C6 merges across sessions and re-sorts the union with the same key for a stable, deterministic order
(R-15 — the per-cycle cap truncation depends on this being deterministic).

## SessionLossInfo population rule (ADR-007 / AC-08)

A session appears in `loss` whenever ANY of: `elided_bytes > 0`, `has_holes == true`,
`provenance == Reconstructed`, OR `dropped_candidates > 0`. A clean Primary session with no loss and no
cap-drop is OMITTED (silence means "nothing to report"). A Reconstructed session with zero candidates
still warrants a `loss` row so the loss is visible. `provenance` derivation is the SAME predicate
ADR-006 uses (driven by C6 from the fallback decision), never re-computed (ADR-007).

## Data Flow

- **Produced by:** C3 (`Vec<TranscriptCandidate>`, Primary), C5 (`Vec<TranscriptCandidate>`,
  Reconstructed), C6 (assembles `TranscriptCandidatesSection` + `SessionLossInfo`).
- **Consumed by:** the cycle-review handler at response-assembly (C6/tools.rs), then the agent (C10).

## Error Handling

Pure value types — no fallible construction, no panics. Serde `skip_serializing_if` guarantees absence,
not null, when `None`.

## Key Test Scenarios

- AC-04: `None` → `transcript_candidates` omitted from JSON entirely (serde round-trip).
- AC-04: no-transcript cycle review → existing response fields byte-identical to pre-crt-052 (golden diff).
- AC-06 structural: `RetrospectiveReport` has no candidate field (compile-level).
- AC-06: `Debug` of `TranscriptCandidate` prints `text.len()`, never `text`.
- AC-08: a session with `elided_bytes > 0` / holes / Reconstructed / cap-drop appears in `loss`; a
  clean Primary session is omitted.
- Ordering: candidates sort stably by `(ts, session_id, byte_offset)` (feeds R-15 determinism).
