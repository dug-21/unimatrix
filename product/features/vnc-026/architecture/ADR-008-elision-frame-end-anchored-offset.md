## ADR-008: Elided Delta Frames Are End-Anchored — Declared Offset = file_len − bytes.length

### Context

R-06 (High) flagged a client/server disagreement around elision, deferred until the
vnc-025 (F2) server buffer merged. F2 merged via PR #692; this decision pins the
semantics from the merged code as-is (`crates/unimatrix-server/src/infra/session_transcript.rs`).

When the client truncates an oversized span `[last_offset, file_len)` to
head 48 KiB + `…[N bytes elided]…` marker + tail 12 KiB (FR-08), it ships
`bytes.length ≈ 61,461` bytes for a span larger than 65,536 — the shipped bytes are
always fewer than the span. The originally spec'd frame shape (R-06 scenario 2:
`offset = last_offset`, the span start) produces this merged-F2 behavior, verified
against `apply_delta` (session_transcript.rs:94-174):

1. **At apply time: no hole.** The write extends the span contiguously from the prior
   span end (`last_offset`). `high_water = last_offset + bytes.length` — *less than*
   `file_len`, so server coverage and client `last_offset` disagree by
   `gap = N_elided − marker_len`.
2. **Tail bytes land at wrong logical offsets** — buffer positions
   `[last_offset + 49,152 + marker_len, end)` instead of their true file positions
   `[file_len − 12,288, file_len)`.
3. **The next delta (at `file_len`) creates a phantom hole** `(end, file_len)`
   (Step 3, session_transcript.rs:148-153) — zero-filled, never fillable (elided bytes
   are never re-sent, ADR-004).
4. **PreCompact starves.** `contiguous_tail` (session_transcript.rs:179-194) floors at
   the last hole's end = `file_len`, so the entire 61 KiB catch-up becomes permanently
   unservable the moment the session continues; restoration right after the
   highest-loss event serves only the (possibly tiny) post-elision delta. NUL bytes are
   never served (I4/FR-19 hold) — the failure is starvation, not corruption.
5. Each elision adds one permanent hole; 65 elisions trigger the
   `MAX_HOLE_RANGES` collapse (session_transcript.rs:28, 171-173, 311-326).

The server behaves exactly per its contract (vnc-025 ADR-002: metadata-only elision,
holes never served). The defect is purely the client's frame geometry. No F2 rework
(C-07) is required.

### Decision

**Elided frames are end-anchored.** When FR-08 truncation applies, the client declares:

```
offset = file_len − bytes.length        // bytes = head ++ marker ++ tail (raw UTF-8 lengths)
bytes  = head(48 KiB) ++ "…[N bytes elided]…" ++ tail(12 KiB)
```

so the frame **ends exactly at `file_len`**. `last_offset` advances to
`offset + bytes.length = file_len` — ADR-004's success rule becomes uniform ("advance
to the end of the shipped span") with no truncation special case. Non-elided frames are
unchanged (`offset = last_offset`).

Merged-F2 consequences of an end-anchored elided frame, pinned as testable assertions
for the Layer-2 helper (RISK-TEST-STRATEGY note 3):

- a. **Hole forms behind the content at apply time**: `holes == [(last_offset,
  file_len − bytes.length)]`, size `N_elided − marker_len` — a true record of the
  elided region's location (front-shifted by `marker_len`). `high_water == file_len`
  (server coverage and client `last_offset` agree). Server `elided_bytes` is unchanged
  (client-side elision is invisible to the server counter — it counts only ring-tail
  and below-base drops).
- b. **PreCompact tail**: `contiguous_tail(12000)` returns the last 12,000 bytes of the
  shipped frame — pure client-tail content (the 12,288-byte tail exceeds the 12,000-byte
  window, so the marker normally sits outside the window), never zero-fill, never
  crossing the hole. If the marker does enter a window, the JSONL block builder
  (`extract_transcript_block_from_bytes`, uds/transcript_block.rs:391) filters the
  non-parsing line(s) it touches — the marker is for raw-content fidelity, not the
  restoration block.
- c. **Subsequent deltas extend contiguously** at `file_len`: no further holes, and
  `contiguous_tail` windows cross the elision seam naturally (W5-with-a-hole passes).
  The client's tail bytes occupy their TRUE file offsets `[file_len − 12,288, file_len)`;
  only head + marker are displaced forward by `gap`, in a region the client never
  writes again (monotonic `last_offset`; concurrent-spawn last-writer-wins races are
  already accepted under FR-11).
- d. **Ring-tail interplay**: a catch-up whose `file_len` jumps more than
  `transcript_buffer_max_bytes` past the buffer base advances the base first
  (session_transcript.rs:114-119) — allocation stays ≤ cap; no client concern.

Rejected alternatives:

- **Span-start anchor (original shape)** — phantom hole + permanent PreCompact
  starvation, per Context.
- **Two frames at true offsets (head @ last_offset, tail @ file_len − 12,288), no
  marker** — also hole-correct, but doubles the POST count, breaks ADR-007's
  one-second-POST shape, and loses the in-band elision signal in the raw bytes.

### Consequences

- Easier: no F2 rework — the merged buffer is compatible by construction; ADR-004's
  offset-advance rule loses its special case; PreCompact restoration after elision
  serves the full 61 KiB catch-up immediately; exactly one hole per elision, created
  eagerly and located meaningfully.
- Harder: head bytes are knowingly declared at displaced offsets (content-opaque server
  never notices; documented here so nobody "fixes" it back); SPECIFICATION FR-08 and
  RISK-TEST-STRATEGY R-06 scenario 2 currently say "offset is span start" and must be
  updated by their owners.
- Cross-references: ADR-004 (offset advance), ADR-007 (carrier), vnc-025 ADR-002
  (server buffer representation, metadata-only elision).

### Amendment (2026-06-08 — vnc-026 retro, human-approved)

Wording correction to match shipped behavior (Gate 3a WARN A resolution): elided frames
anchor at `effectiveEnd`, not the literal `file_len`. `effectiveEnd` = `file_len` backed
off ≤3 bytes when the file ends mid-UTF-8 character; for well-formed JSONL it equals
`file_len`. Declared `offset = effectiveEnd − bytes.length`; the frame ends exactly at
`effectiveEnd`. Implemented and tested in `packages/unimatrix/lib/hook-client/delta.js`.
All four pinned server-state assertions (a–d) hold against `effectiveEnd` in place of
`file_len`.
