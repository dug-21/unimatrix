# Agent Report: crt-028-agent-1-pseudocode

## Status

COMPLETE

## Files Produced

- `product/features/crt-028/pseudocode/OVERVIEW.md`
- `product/features/crt-028/pseudocode/hook.md`
- `product/features/crt-028/pseudocode/listener.md`
- `product/features/crt-028/pseudocode/index_briefing.md`

## Components Covered

| Component | File | Change |
|-----------|------|--------|
| hook.rs | `crates/unimatrix-server/src/uds/hook.rs` | New constants, enum, 4 functions, run() modification |
| listener.rs | `crates/unimatrix-server/src/uds/listener.rs` | New `sanitize_observation_source`, single replacement site |
| index_briefing.rs | `crates/unimatrix-server/src/services/index_briefing.rs` | Doc comment + regression test |

## Interface Decisions Implemented

All interfaces traced directly to architecture documents. None invented:

| Item | Source |
|------|--------|
| `extract_transcript_block(path: &str) -> Option<String>` | ARCHITECTURE.md Integration Surface |
| `build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn>` | ARCHITECTURE.md Integration Surface |
| `prepend_transcript(transcript: Option<&str>, briefing: &str) -> String` | ARCHITECTURE.md Integration Surface |
| `extract_key_param(tool_name: &str, input: &serde_json::Value) -> String` | ARCHITECTURE.md Key-Param Map |
| `sanitize_observation_source(source: Option<&str>) -> String` | ARCHITECTURE.md GH #354 Fix Design, ADR-004 |
| `ExchangeTurn` enum variants | IMPLEMENTATION-BRIEF.md Data Structures |
| Constants (MAX_PRECOMPACT_BYTES=3000, TAIL_MULTIPLIER=4, etc.) | ARCHITECTURE.md Constants table |

## Open Questions / Gaps Found

### OQ-A: `get_content_array` helper — message shape ambiguity

The JSONL format emitted by Claude Code uses `{"type":"...", "message": {"content":[...]}}`.
The pseudocode implements a two-shape fallback (`message.content` first, then `content` at
top level). The architecture documents only the `message.content` shape with a sample.
If Claude Code always uses `message.content`, the fallback is dead code. If it sometimes
uses the raw API format, the fallback is needed. Implementation agent should verify against
real transcript files before committing; the fallback adds no risk (fail-safe) but adds
unnecessary complexity if unused.

### OQ-B: `format_turn` blank-line grouping between exchange pairs

The pseudocode emits each turn as a separate string and joins with `"\n"`. The spec
(FR-02.3) says "a blank line between pairs". The output format block in ARCHITECTURE.md
shows blank lines between exchange pairs. The pseudocode does not explicitly insert
the blank line between the last ToolPair of one exchange and the `[User]` of the next.
Implementation agent should ensure blank lines appear between complete exchange pairs,
not just between individual turns. One approach: insert `""` as a separator entry in
`output_parts` after each complete exchange group (after the last ToolPair for that
assistant turn).

### OQ-C: `exchange_count` in header — counting convention

The header `=== Recent conversation (last N exchanges) ===` uses `exchange_count`.
The pseudocode increments `exchange_count` on each `UserText` turn. This is correct
for standard exchanges. However, in a session that begins with tool-call-only assistant
turns (OQ-SPEC-1 case), the count may be off if `ToolPair` turns appear without a
preceding `UserText` in the budget window. This is a cosmetic issue only — the count
is informational. Implementation agent may increment on `UserText` turns only, which
is the most natural interpretation.

## Known Limitations (from Architecture)

- Field denylist for `extract_key_param` fallback (R-09): deferred. The fallback may select
  a sensitive field for unknown tools. Documented in hook.md pseudocode. Follow-up recommended.
- `TAIL_MULTIPLIER = 4` may be insufficient for thinking-heavy sessions (R-02). This is an
  accepted limitation with a defined degradation path (fewer pairs returned).

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `hook.rs PreCompact transcript extraction patterns`
  -- found entry #3331 confirming the PreCompact hook-side transcript read pattern for
  WA-5 crt-028. Pattern matches architecture exactly. No deviations.
- Queried: `/uni-query-patterns` for `crt-028 architectural decisions` (category: decision)
  -- found entries #3333, #3334, #3335, #3336 (ADR-001 through ADR-004 crt-028).
  All four ADRs found in Unimatrix; content matches ADR files. No gaps.
- Deviations from established patterns: none. hook.rs pseudocode follows the existing
  `truncate_utf8` reuse pattern, `inner()` closure for degradation boundary, and
  `Option<String>` return type for graceful failure — all consistent with existing
  `hook.rs` patterns.
