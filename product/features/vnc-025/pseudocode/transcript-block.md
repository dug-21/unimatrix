# Pseudocode: transcript-block (`uds/transcript_block.rs` — NEW module, code MOVED from hook.rs)

ADR: ADR-005. FRs: FR-17, FR-18, FR-19, FR-21. Risks: R-09 (parity), R-14 (move regression).

## Purpose

Single shared transcript-block extraction core: the local hook's existing JSONL→exchange-turn
formatting, moved verbatim-where-possible out of `hook.rs`, plus one new entry point for the
server (`from_bytes`). Parity between local hook and server-built PreCompact block is
structural — one core, two thin front-ends.

## What Moves (verbatim where possible — R-14: behavior unchanged)

From `hook.rs` to `uds/transcript_block.rs`, with visibility raised to `pub` (or `pub(crate)`)
as needed by re-imports:

| Item | Current location | Notes |
|------|-----------------|-------|
| `MAX_PRECOMPACT_BYTES: usize = 3000` | `hook.rs:39` | `pub const`; value PINNED (R-14.2) |
| `TAIL_MULTIPLIER: usize = 4` | `hook.rs:50` | `pub const`; value PINNED; window = 12,000 |
| `enum ExchangeTurn` | `hook.rs:1113` | UserText / AssistantText / ToolPair — body unchanged |
| `fn build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn>` | `hook.rs:1205` | body unchanged |
| `fn format_turn(turn: &ExchangeTurn) -> String` | `hook.rs:1361` | body unchanged |
| `pub fn extract_transcript_block(path: &str) -> Option<String>` | `hook.rs:1383` | body unchanged |
| `pub fn prepend_transcript(transcript: Option<&str>, briefing: &str) -> String` | `hook.rs:1442` | body unchanged |

`hook.rs` call sites re-import (`use crate::uds::transcript_block::{extract_transcript_block,
prepend_transcript};` or `super::transcript_block::...`):
- `hook.rs:220` — `.and_then(|p| extract_transcript_block(p))`
- `hook.rs:252` — same
- `hook.rs:295` — `prepend_transcript(transcript_block.as_deref(), content)`

The `hook.rs` test module: tests covering moved items either move with them into the new
module's `#[cfg(test)]` block or stay and import — either way the pre-move test-NAME inventory
must match post-move (R-14.1, pattern #3253: no silently dropped tests). Constants test
(`hook.rs:3945-3947` pins `3000`/`4`) moves to the new module.

Also move the private helpers/`use` items the moved functions depend on (e.g. `Seek`,
`SeekFrom`, `BufReader` imports; any JSONL-line parsing helpers used only by
`build_exchange_pairs`). `MAX_INJECTION_BYTES` (referenced by the constants test's `assert_ne`)
stays in `hook.rs` — adjust that one assertion's import direction, not its meaning.

## New Function

### `pub fn extract_transcript_block_from_bytes(bytes: &[u8]) -> Option<String>`

Mirrors `extract_transcript_block(path)` exactly, minus the file-tail seek (the caller —
dispatch-wiring's PreCompact integration — already provides only the tail window via
`contiguous_tail`):

```
// Input: raw JSONL transcript-file bytes (the F3 delta-content contract; offsets are
// file byte offsets). May begin mid-line — contiguous_tail starts wherever the window
// lands, exactly like the path variant's seek landing mid-line.

text = String::from_utf8_lossy(bytes)             // tolerate invalid UTF-8: lossy, never error
                                                  // (buffer API is &[u8]; a mid-line window can
                                                  // split a multi-byte char — lossy mirrors a
                                                  // parse-failure line being filtered)
raw_lines: Vec<&str> = text.lines().collect()
// A partial first line fails the JSON parse inside build_exchange_pairs and is filtered —
// the SAME behavior the path variant exhibits when its seek lands mid-line (ADR-005).

turns = build_exchange_pairs(&raw_lines)

// Budget loop — IDENTICAL to the path variant (shared private helper preferred):
output_parts = []; bytes_used = 0; exchange_count = 0
for turn in &turns:
    turn_text = format_turn(turn)
    if bytes_used + turn_text.len() > MAX_PRECOMPACT_BYTES: break
    bytes_used += turn_text.len()
    if turn is ExchangeTurn::UserText: exchange_count += 1
    output_parts.push(turn_text)

if output_parts.is_empty(): return None

header = format!("=== Recent conversation (last {} exchanges) ===", exchange_count)
footer = "=== End recent conversation ==="
return Some(format!("{}\n{}\n{}", header, output_parts.join("\n"), footer))
```

**Refactor rule**: extract the shared core (`lines → turns → budget loop → header/body/footer`)
into one private function, e.g. `fn block_from_lines(lines: &[&str]) -> Option<String>`, called
by both `extract_transcript_block` (after file-open + seek + read-lines) and
`extract_transcript_block_from_bytes` (after lossy decode + split). This makes parity literal,
not parallel. The path variant's observable behavior must remain bit-identical (its existing
tests pass unmodified).

## Data Flow

- In (path variant): transcript file path → seek to last 12,000 bytes → lines.
- In (bytes variant): `contiguous_tail(MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER)` output —
  ≤ 12,000 contiguous bytes, never spanning a hole, no zero-fill (guaranteed by
  transcript-buffer FR-19; this module can trust its input).
- Out: `Option<String>` block ≤ ~3 KB body + header/footer; `None` when no complete turn fits.
- `prepend_transcript`: unchanged 4-case combine (both / transcript-only / briefing-only /
  neither). No elision marker anywhere (ADR-002/OQ-3 — elision is buffer metadata, invisible
  here).

## Error Handling

- No `Result`, no panic: malformed JSONL lines are filtered (existing behavior); invalid UTF-8
  is lossy-decoded; empty/whitespace input → `None`.
- No `tracing` calls touching content (AC-12 grep gate covers this module too).

## Key Test Scenarios (R-09, R-14, R-13)

1. Golden parity (HARD GATE, #3426): fixture JSONL transcript; expected =
   `extract_transcript_block(path)`; actual = stream the same file bytes as shuffled +
   duplicated deltas into a TranscriptBuffer → `extract_transcript_block_from_bytes(
   contiguous_tail(12_000))`. Byte-for-byte equality; no hand-written expectation.
2. Mid-line tail start: window beginning mid-JSONL-line filters the partial line identically
   to the path variant's mid-line seek.
3. Constants pinned in the new module: `MAX_PRECOMPACT_BYTES == 3000`, `TAIL_MULTIPLIER == 4`
   (R-14.2).
4. Pre/post move test-name inventory identical (R-14.1).
5. `from_bytes` on: empty slice → None; all-malformed lines → None; invalid UTF-8 bytes →
   no panic, lossy handling.
6. Block budget bound (R-13.1): adversarially large buffer content → block body still
   ≤ MAX_PRECOMPACT_BYTES, wrapped in the same header/footer. (Content itself is untrusted by
   design — identical exposure to the local hook reading a local file; document-and-accept.)
