# Agent Report: crt-028-agent-2-spec

**Agent**: Specification Writer
**Feature**: crt-028 — WA-5 PreCompact Transcript Restoration
**Output**: `product/features/crt-028/specification/SPECIFICATION.md`

## Completion Status

SPECIFICATION.md written. All 15 ACs from SCOPE.md are present and expanded with
verification methods. Self-check passed.

## Key Decisions Made

**D-SPEC-01: Exchange pair requires non-empty user text.**
SCOPE.md D-2 specifies user turns contribute `type: "text"` blocks. If a user turn
has no text blocks (only `tool_result`), there is no user text for the pair. The spec
(FR-02.3, domain model) requires non-empty user text for a pair to be emitted. This
prevents empty `[User]` sections.

**D-SPEC-02: `read_transcript_block` returns `Option<String>`, never propagates errors.**
FR-06.6 formalizes the degradation boundary as a function boundary. This decomposition
(one function for I/O + parsing, one for budget-filling, one for write path) directly
implements the SR-01+SR-07 recommendation from the risk assessment.

**D-SPEC-03: Seek window is `MAX_PRECOMPACT_BYTES * 4`.**
SR-02 recommends a byte cap before parsing. The multiplier of 4 is a reasonable
raw-to-extracted ratio (raw JSONL with full tool results is ~4x larger than the
extracted transcript block). At 3000 bytes budget, this caps file I/O to ~12KB.

**D-SPEC-04: OQ-2 resolved as "read before transport, prepend in response handler."**
FR-05.4 specifies the prepend goes in the PreCompact response handler branch (after
`transport.request()` returns), not in `write_stdout`. This avoids modifying
`write_stdout` (AC-14 invariant) while keeping the extraction logic near its use site.

**D-SPEC-05: Empty assistant text with non-empty tool pairs is an open question.**
OQ-SPEC-1 is raised because the spec needed to define this edge case for FR-02.4 but
the scope does not settle it. The architect must clarify.

**D-SPEC-06: All 7 FRs map cleanly to SCOPE.md goals.**
FR-01 through FR-06 map to Goals 1-4 and D-1 through D-7. FR-07 maps to Goal 6 (GH #354).
FR-08 maps to Goal 7 (GH #355). Coverage is complete.

## Open Questions for Architect

**OQ-SPEC-1**: Exchange pair with empty `asst_text` but non-empty `tool_pairs` — emit
or suppress? Affects FR-02.4 and the domain model `ExchangePair` definition. The spec
currently requires non-empty user text but does not require non-empty `asst_text`.

**OQ-SPEC-2**: Stdlib behavior of `std::io::Seek::seek(SeekFrom::End(-N))` when N >
file size. On Linux this clamps to offset 0. The architect should add explicit handling
rather than relying on platform behavior.

## Self-Check

- [x] All 15 ACs from SCOPE.md present (AC-01 through AC-15), each with verification method
- [x] Every functional requirement is testable
- [x] NFRs include measurable targets (50ms, 300 bytes, 3000 bytes)
- [x] Domain Models section defines TranscriptRecord, ExchangePair, ToolPair, TranscriptBlock
- [x] Ubiquitous Language section present with 8 terms
- [x] NOT in scope section is explicit (8 exclusions)
- [x] Output file is in `product/features/crt-028/specification/` only
- [x] No TBD sections — unknowns raised as OQ-SPEC-1 and OQ-SPEC-2
- [x] Scope Risk Traceability section maps all 7 SRs to spec mitigations
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for hook PreCompact transcript extraction — entry #3331 confirmed crt-028 pattern is already recorded.
- Queried: `/uni-query-patterns` for JSONL reverse scan byte budget — entry #243 confirmed ADR-002 sync I/O constraint.
- Queried: `/uni-query-patterns` for security allowlist acceptance criteria — no conflicting patterns found; GH #354 scoped as standalone security AC per SR-05.
