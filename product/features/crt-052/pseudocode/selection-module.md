# C3 — Candidate Selection Module (PURE)

**Target source:** `unimatrix-observe/src/distill/{mod,jsonl,markers,select}.rs`
**Wave:** A — **NO reference to `transcript_hold.rs`; no I/O, no lock, no `tracing`.**
**ADRs:** ADR-003 (pure module), ADR-002 (input bytes + base_offset). **Risks:** R-10, R-12, R-15, R-20.
**AC:** AC-02, AC-03, AC-V-FUZZ. **Constraints:** 6 (no generation), 7 (untrusted JSONL), 10 (500-line).
**Sequencing:** after C4; independently fixture-testable.

## Purpose

Pure, no-I/O, no-lock module: snapshot bytes → Claude Code JSONL parse → keep user/assistant text
blocks → match four marker families → keep matched blocks WHOLE → dedup → per-session cap → ordered
`Vec<TranscriptCandidate>`. Untrusted-input-hardened (skip-with-count, never panic). Rules SELECT; the
agent EXTRACTS (Constraint 6 — no server-side generation/classification beyond advisory hints).

## Module layout (Constraint 10 — keep each file under 500 lines)

- `mod.rs` — re-exports `select_candidates`; module doc. Per-cycle aggregate cap is NOT here (C6 owns it).
- `jsonl.rs` — untrusted parse → `Vec<ParsedBlock>`.
- `markers.rs` — four marker families (~50 regex), `OnceLock` set; `match_families(text) -> Vec<FamilyHint>`.
- `select.rs` — `select_candidates` entry; orchestrates jsonl → markers → dedup → cap → order.

## `jsonl.rs` — Untrusted Parse (FR-14 / Constraint 7 / R-10 / AC-V-FUZZ)

```
struct ParsedBlock {
    role:        Role            // User | Assistant (others dropped)
    text:        String          // concatenated text segments of the message
    ts:          Option<String>  // record timestamp if present and parseable
    byte_offset: u64             // in-snapshot offset of this line's first byte (array-relative here;
                                 //   C3 adds base_offset to make it logical — R-12)
}

fn parse_blocks(bytes: &[u8]) -> (Vec<ParsedBlock>, skip_count: u64):
    blocks = []
    skip   = 0
    offset = 0
    for line in split_on_newline(bytes):          // operate on &[u8]; do NOT require valid UTF-8 upfront
        line_offset = offset
        offset += line.len() + 1                  // track in-snapshot byte offset per line
        if line is empty: continue

        // --- skip-with-count, NEVER panic, NEVER return Err (R-10) ---
        text_str = match str::from_utf8(line):    // non-UTF-8 line -> skip
            Ok(s)  => s
            Err(_) => { skip += 1; continue }
        if line.len() > MAX_LINE_BYTES:           // oversized line guard (resource exhaustion) -> skip
            { skip += 1; continue }
        rec = match serde_json::from_str(text_str):   // truncated/garbage/embedded-NUL JSON -> skip
            Ok(v)  => v
            Err(_) => { skip += 1; continue }         // tolerate truncated FINAL line this way

        // --- record-type filter: keep only user/assistant TEXT blocks ---
        role = match rec.type/role:
            "user"      => User
            "assistant" => Assistant
            _           => { /* tool_use, tool_result, thinking, command-noise, unknown */ continue }
        // (unknown record type is dropped silently here — it's a known-format skip, not a parse error;
        //  count it toward skip only if it failed to parse. Unknown-but-valid-JSON type => drop, no count.)

        text = extract_text_segments(rec)         // concatenate text parts; ignore tool_use/thinking parts
        if text is empty after extraction: continue

        // bound nested/huge JSON damage: extract_text_segments must not recurse unboundedly
        //   (billion-laughs / deeply-nested guard) — cap depth/size, skip-with-count on breach.
        blocks.push(ParsedBlock { role, text, ts: rec.timestamp_opt, byte_offset: line_offset })
    return (blocks, skip)
```

Hardening invariants (R-10, AC-V-FUZZ — security merge gate):
- Never `unwrap`/`expect`/`panic!` on content-derived values.
- Truncated JSON, non-UTF-8, oversized line, unknown record type, embedded NUL → skip-with-count.
- Truncated FINAL line (ring-tail/hole boundary) is the common real case → tolerated via the per-line
  parse-or-skip.
- Deeply nested / gigantic field → bounded handling (depth/size cap), skip-with-count, no OOM.

## `markers.rs` — Four Marker Families (FR-3 / AC-13 / NFR-6)

```
// ~50 regex patterns ported from ass-070 extractor.py, grouped into four families.
// Built ONCE in a OnceLock; regex-class crate only (no heavyweight runtime dep, cargo audit clean).
static FAMILY_SET: OnceLock<FamilyPatterns> = ...

struct FamilyPatterns {
    decision:  RegexSet     // decision phrases
    rework:    RegexSet     // rework signals
    lesson:    RegexSet     // lesson markers
    phasegate: RegexSet     // phase / gate markers
}

fn match_families(text: &str) -> Vec<FamilyHint>:
    hints = []
    if FAMILY_SET.decision.is_match(text):  hints.push(Decision)
    if FAMILY_SET.rework.is_match(text):    hints.push(Rework)
    if FAMILY_SET.lesson.is_match(text):    hints.push(Lesson)
    if FAMILY_SET.phasegate.is_match(text): hints.push(PhaseGate)
    return hints     // ADVISORY only; non-empty result means "this block is a candidate"
```

Hints are advisory; the server never classifies authoritatively (Constraint 6 / Non-Goal). The agent
re-classifies (C10).

## `select.rs` — Entry Point (ARCH §4 — binding signature)

```
fn select_candidates(bytes: &[u8], session_id: &str, base_offset: u64, session_cap: usize)
    -> Vec<TranscriptCandidate>:

    (blocks, _skip) = parse_blocks(bytes)        // skip_count is informational; not surfaced per-block

    matched = []
    for b in blocks:
        hints = match_families(&b.text)
        if hints.is_empty(): continue            // only marker-matched blocks become candidates
        matched.push(TranscriptCandidate {
            session_id: session_id.to_string(),
            byte_offset: base_offset + b.byte_offset,   // LOGICAL stream offset (R-12)
            ts: b.ts,
            family_hints: hints,                 // non-empty by construction
            text: b.text,                        // WHOLE block, unwindowed (ass-070: windowing loses context)
        })

    // dedup: identical (session_id, byte_offset, text) collapse to one (same block matched twice)
    matched = dedup_stable(matched)

    // order chronologically: (ts, session_id, byte_offset). None ts sorts after Some (stable).
    matched.sort_by_key(|c| (c.ts.clone(), c.session_id.clone(), c.byte_offset))

    // per-session volume cap (session_cap bytes, default 24 KB). Deterministic keep-earliest:
    //   iterate ordered candidates, accumulate text.len(), STOP including once adding the next would
    //   exceed session_cap. The DROPPED count is returned to the caller (C6) so AC-08 can surface it.
    //   NOTE: select_candidates returns only Vec<TranscriptCandidate>; the per-session dropped count is
    //   recomputable by C6 (matched.len() before cap vs after) OR see contract note below.
    capped = keep_earliest_within(matched, session_cap)
    return capped
```

### Contract note — per-session cap drop count (flag)

ARCH §4 fixes the signature as `-> Vec<TranscriptCandidate>` (no count out-param). AC-08 needs the
per-session cap-drop count surfaced. Two options for the leader/architect:
(a) C6 computes the drop by re-running the cap accounting (it has bytes + cap), or
(b) widen the return. This pseudocode assumes **(a)** to honor the pinned signature: C6 derives
`dropped_candidates` from the pre-cap vs post-cap candidate set. Flagged in the final report.

## Logical Offset (R-12)

`byte_offset = base_offset + in_snapshot_offset`. With `base_offset == 0` (non-overflow) it equals the
in-snapshot offset; with `base_offset > 0` (ring-tail overflow) it is a logical stream position,
meaningful across elision. Tested at the elision boundary.

## Performance (NFR-2 / AC-12)

Pure Rust over in-memory bytes; rule pass over a 4 MiB buffer < 50 ms off-lock (ass-070 single-digit ms
estimate). `RegexSet` built once via `OnceLock` — no per-call compilation.

## Data Flow

- **Input:** `&[u8]` (snapshot bytes), `session_id`, `base_offset`, `session_cap` (from C9 config).
- **Output:** ordered, deduped, per-session-capped `Vec<TranscriptCandidate>` (Primary provenance,
  assigned by C6).
- **Consumer:** C6 (aggregates, per-cycle cap, attaches loss/provenance).

## Error Handling

No `Result` carries content; the only output is `Vec<TranscriptCandidate>` (possibly empty). The parser
NEVER returns `Err` and NEVER panics (R-10). Malformed input → skip-with-count → fewer candidates.

## Fixture Independence (AC-03 / R-20 / OQ-6)

The committed labeled corpus MUST be authored independently of the ported regex set (anchors-before-port
OR different author) with a provenance header — a test/review gate, not this module's code. Recall
≥ 0.90 block-level, selected volume ≤ 10% of raw bytes (NFR-3).

## Key Test Scenarios

- AC-02: tool_use/tool_result/thinking/command-noise dropped; user/assistant text kept; matched blocks
  whole; dedup; per-session cap truncates at the knob; ordering + `family_hints` populated.
- AC-V-FUZZ (merge gate): truncated JSON, non-UTF-8, oversized line, unknown record type, embedded NUL,
  truncated final line, deeply-nested JSON → skip-with-count, no `Err`, no panic.
- R-12: `base_offset > 0` → `byte_offset == base_offset + in_snapshot_offset`; `base_offset == 0` →
  equals in-snapshot offset; stable across an elision boundary.
- AC-03: ≥ 0.90 recall, ≤ 10% volume on the independent fixture; provenance header present (R-20).
- AC-12: 4 MiB rule pass < 50 ms.
