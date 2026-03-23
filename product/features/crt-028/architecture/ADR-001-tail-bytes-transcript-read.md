## ADR-001: Tail-Bytes Read Strategy for Transcript I/O

### Context

The PreCompact hook operates within a sub-50ms total latency budget (HOOK_TIMEOUT = 40ms
for the server round-trip, ~10ms margin for process startup and hash computation). The
transcript file at `~/.claude/projects/{slug}/{session-uuid}.jsonl` grows throughout the
session: a long session with many tool calls can easily accumulate 1–5 MB.

SR-02 from the risk assessment identifies this as a medium-severity risk: reading the full
file before parsing is a latency risk even with the fail-open degradation contract. A 1 MB
file read at 500 MB/s takes ~2ms on SSD; a 5 MB file takes ~10ms — consuming the entire
margin budget before the server round-trip begins.

Two strategies were considered:

**Option A: Full file read.** Read the entire file into memory, parse all JSONL lines,
reverse-iterate from the end. Simple to implement, guarantees no line truncation at the
start of the read window.

Rejected: For sessions with thousands of tool calls (common in long delivery sessions), the
file can exceed 5 MB. Full-file reads violate the latency budget and allocate memory
proportional to the session's total history — most of which is irrelevant to the "last k
exchanges" goal.

**Option B: Tail-bytes read with fixed window.** Seek to `max(0, file_end - TAIL_WINDOW_BYTES)`
before reading. `TAIL_WINDOW_BYTES = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER` where
`TAIL_MULTIPLIER = 4`. For `MAX_PRECOMPACT_BYTES = 3000`, this reads at most 12,000 bytes.

Selected. The multiplier accounts for the raw-to-extracted ratio:
- JSON envelope overhead per record: ~200 bytes
- `thinking` blocks (skipped entirely): 500–2000 bytes each
- `tool_result` full content (compressed to 300-byte snippet): 200–5000 bytes
- Extracted text per turn: 50–300 bytes

A 4× factor means extracting 3,000 bytes of output requires ~12,000 bytes of raw input.
For files smaller than 12 KB, `SeekFrom::Start(0)` is used instead (no truncation).

**First-line discard**: When seeking mid-file, the first line in the read window is almost
certainly a truncated JSON record. It will fail `serde_json::from_str` and be silently
skipped by the fail-open parser (AC-08). No special handling is needed — the skip is free.

**Seek implementation**: `std::fs::File` with `std::io::Seek` trait. No tokio, no async.
The hook process has no tokio runtime (ADR-002 crt-027 constraint; all I/O is
`std::io`-based synchronous).

```rust
use std::io::{BufRead, BufReader, Seek, SeekFrom};

const MAX_PRECOMPACT_BYTES: usize = 3000;
const TAIL_MULTIPLIER: usize = 4;

fn extract_transcript_block(path: &str) -> Option<String> {
    let inner = || -> Option<String> {
        let mut file = std::fs::File::open(path).ok()?;
        let file_len = file.metadata().ok()?.len();
        let window = (MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER) as u64;

        if file_len > window {
            file.seek(SeekFrom::End(-(window as i64))).ok()?;
        }
        // else: file smaller than window, read from start (no seek needed)

        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
        // ... parse lines, extract pairs, format block
    };
    inner()
}
```

### Decision

Use a tail-bytes read strategy with `TAIL_WINDOW_BYTES = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER`
(= 12,000 bytes at current constants). Seek to `SeekFrom::End(-(window))` when
`file_len > window`; otherwise read from start. The first line after a mid-file seek is
discarded implicitly by the fail-open JSONL parser.

Both constants (`MAX_PRECOMPACT_BYTES` and `TAIL_MULTIPLIER`) are named compile-time
constants in `uds/hook.rs`. `MAX_PRECOMPACT_BYTES` is documented as a tunable for a future
config pass (SR-03 acknowledgment); `TAIL_MULTIPLIER` may need adjustment if empirical
testing shows 4× is insufficient for thinking-heavy sessions.

### Consequences

- Maximum I/O per PreCompact invocation: 12,000 bytes regardless of session length. Latency
  impact: ~0.1–1ms on SSD. Well within budget.
- Memory allocation: bounded to the window size, not the full file size.
- For files smaller than 12 KB: full file is read (no truncation, no first-line discard).
- For files larger than 12 KB: the first line of the window may be partial and is silently
  skipped. All subsequent lines are complete JSONL records.
- If the extracted window contains no parseable user/assistant pairs (e.g., the last 12 KB
  is all tool_result blocks without matching assistant messages), `extract_transcript_block`
  returns `None` and the briefing is written without a transcript block (AC-09).
- `TAIL_MULTIPLIER = 4` is conservative. Sessions with many thinking blocks may find the
  window contains fewer parseable exchange pairs than desired. The degradation path (fewer
  turns included) is acceptable; no error is surfaced.
