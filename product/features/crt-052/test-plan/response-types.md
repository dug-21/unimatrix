# Test Plan — C4 Candidate / Response Types

**Component**: `unimatrix-observe/src/types.rs` — `TranscriptCandidate`, `FamilyHint`,
`CandidateProvenance`, `SessionLossInfo`, `TranscriptCandidatesSection`; additive optional response
field. **ADRs**: ADR-004 (outside memoized struct), ADR-007 (loss visibility). **Wave**: A.
**Tests live in**: `#[cfg(test)] mod tests` in `types.rs`. **Merge gates**: content-leak (AC-06 (a)).

## Unit Test Expectations — type shape & serde
- `test_transcript_candidates_section_serde_roundtrip` — round-trip `TranscriptCandidatesSection`
  (candidates + loss) through serde; fields stable.
- `test_response_field_omitted_when_none` (AC-04) — the additive response field
  `#[serde(skip_serializing_if = "Option::is_none")] transcript_candidates: Option<...>` is OMITTED
  from JSON when `None` (absent, NOT `null`, NOT empty array).
- `test_candidate_fields_populated` — `TranscriptCandidate` carries `session_id`, logical
  `byte_offset`, `ts: Option<String>`, non-empty `family_hints: Vec<FamilyHint>`, `text`.
- `test_family_hint_variants` — `FamilyHint` is exactly `{Decision, Rework, Lesson, PhaseGate}`.
- `test_provenance_variants` — `CandidateProvenance` is exactly `{Primary, Reconstructed}`.

## Content-leak — structural (AC-06(a), **merge gate**, R-04)
- `test_retrospective_report_has_no_candidate_field` — compile-level / structural assertion:
  `RetrospectiveReport` (`types.rs:381`, the memoized type) has NO `transcript_candidates` field and
  NO candidate-bearing field. The leak is structurally impossible (ADR-004): candidates live only on
  the response, attached at assembly level, never on the persisted struct. This is the load-bearing
  compile-level half of the content-leak merge gate.

## Content-leak — Debug (AC-06(e), R-19)
- `test_candidate_debug_present_text_is_intentional` — `TranscriptCandidate.text` IS the candidate
  content the agent consumes; its Debug may show text (it is response data, not the secrets-posture
  snapshot type). Document the boundary: the no-content-Debug rule applies to `TranscriptSnapshot` /
  `HeldBuffer` (snapshot-types.md / held-buffer-store.md), NOT to the candidate value the response
  intentionally returns. (Guards against an over-broad grep gate false-positive.)
- `test_session_loss_info_debug_metadata_only` — `SessionLossInfo` Debug is metadata
  (`session_id, elided_bytes, has_holes, provenance`) — no transcript bytes.

## Loss visibility (AC-08, ADR-007) — type-level support
(Assembly-level population is tested in distill-handler.md; here the types must SUPPORT it.)
- `test_session_loss_info_shape` — `SessionLossInfo { session_id, elided_bytes, has_holes, provenance }`.
- `test_section_two_parallel_collections` — `TranscriptCandidatesSection { candidates, loss }` holds
  the two parallel collections; a reconstructed session with zero candidates can still carry a loss
  row (so loss is never invisible).
- `test_provenance_is_per_session` — provenance is per-session (whole-session either/or, consistent
  with ADR-006), never per-candidate-mixed within a session.

## Assertions Summary (concrete)
- `transcript_candidates` JSON key is absent when no candidates (serde skip), present otherwise.
- `RetrospectiveReport` cannot carry candidates — proven at compile, the structural merge-gate half.
- `SessionLossInfo` Debug is content-free; `TranscriptCandidate.text` Debug is intentional response data.
- The section's `loss` and `candidates` are independent collections (loss may exist with zero
  candidates for a reconstructed/lossy session).
