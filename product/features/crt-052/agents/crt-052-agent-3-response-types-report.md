# Agent Report — crt-052 C4 Response Types

**Agent:** crt-052-agent-3-response-types
**Component:** C4 — candidate/response types (`crates/unimatrix-observe/src/types.rs`)
**Wave:** A

## Files Modified
- `crates/unimatrix-observe/src/types.rs`

## What Shipped
Added the crt-052 Wave A response-transient types (names match ARCH §4 exactly):
- `TranscriptCandidate { session_id, byte_offset (LOGICAL), ts: Option<String>, family_hints: Vec<FamilyHint>, text }`
- `enum FamilyHint { Decision, Rework, Lesson, PhaseGate }` (serde snake_case; `phase_gate`)
- `enum CandidateProvenance { Primary, Reconstructed }`
- `SessionLossInfo { session_id, elided_bytes, has_holes, provenance, dropped_candidates }` (ratified AC-08 count)
- `TranscriptCandidatesSection { candidates, loss }`

## Contract Notes / Decisions Honored
- Additive field `transcript_candidates: Option<TranscriptCandidatesSection>` is attached at
  RESPONSE-ASSEMBLY level by C6 (tools.rs), NOT on `RetrospectiveReport` (ADR-004). types.rs owns the
  types only — there is no assembly response struct in this crate. AC-04 omit-when-None is proven here
  via an `AssemblyResponseProbe` test struct mirroring the exact `#[serde(skip_serializing_if = "Option::is_none")]`
  attribute the assembly site uses.
- `RetrospectiveReport` deliberately gains NO candidate field; structural leak gate proven at serde level
  (`test_retrospective_report_has_no_candidate_field`).
- Provenance is per-session (`SessionLossInfo`), NOT per-candidate — verified `TranscriptCandidate` has no
  provenance field (`test_provenance_is_per_session`).
- `TranscriptCandidate` derives `Debug` and intentionally shows `text` (Gate 3a ADR-007 ruling); R-19
  metadata-only-Debug rule targets snapshot/held-buffer types only.
- Zero reference to `transcript_hold.rs` (Wave A / R-11).

## Tests
11 new tests added, all passing. Full module: 49 passed / 0 failed (402 filtered).
- `test_transcript_candidates_section_serde_roundtrip`
- `test_response_field_omitted_when_none` (AC-04)
- `test_candidate_fields_populated`
- `test_family_hint_variants`, `test_provenance_variants`
- `test_retrospective_report_has_no_candidate_field` (AC-06(a) merge gate)
- `test_candidate_debug_present_text_is_intentional` (AC-06(e))
- `test_session_loss_info_debug_metadata_only`
- `test_session_loss_info_shape`
- `test_section_two_parallel_collections`
- `test_provenance_is_per_session`

Build: clean. Clippy: zero warnings referencing types.rs (pre-existing warnings in other modules/deps unchanged). fmt applied.

## Issues / Blockers
None.

## Downstream Notes for C6 (handler)
- C6 owns `dropped_candidates` population (it holds both caps and pre/post-cap counts).
- C6 re-sorts the cross-session candidate union by `(ts, session_id, byte_offset)` for R-15 determinism;
  per-session ordering comes from C3.
- C6 attaches the `Option<TranscriptCandidatesSection>` field at tools.rs assembly with the same
  `#[serde(skip_serializing_if = "Option::is_none")]` attribute used in the C4 probe test.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) -- found #3255 (serde(default) alone does
  not omit None; pair with skip_serializing_if — applied), and ADR entries #4847/#4848/#4849 (crt-052
  ADR-001/002/003). context_briefing not separately needed; searches were sufficient.
- Stored: nothing novel to store -- the load-bearing decisions (assembly-level attach outside memoized
  struct, per-session provenance, intentional candidate-text Debug, no-slot structural leak gate) are all
  already ratified in crt-052 ADR-004/ADR-007 and the serde-skip gotcha is captured in pattern #3255.
  This component was a faithful implementation of those decisions with no new runtime-invisible trap.
