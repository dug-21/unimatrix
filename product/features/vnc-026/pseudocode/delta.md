# delta.js — Transcript Delta Streaming (ADR-004 / ADR-007 / ADR-008)

## Purpose
On fire-and-forget spawns with a non-empty `transcript_path`: stat the file, read the
unshipped span `[last_offset, file_len)`, ship one `transcript_delta` RecordEvent as a
separate, concurrent POST, and advance the persisted offset by the uniform rule
`declared offset + byteLength(bytes)`. Deltas are NEVER queued; failure = offset
non-advance + re-derive next spawn. Zero transcript bytes at rest.

## Constants
```
DELTA_SOFT_CAP = 65_536      // raw span bytes
HEAD_BYTES     = 49_152      // 48 KiB
TAIL_BYTES     = 12_288      // 12 KiB
BODY_GUARD     = 1_048_576   // post-serialization (C-02 / SR-04)
```

## Functions

### maybeSendDelta(transcriptPath, sessionId, provider, config) -> Promise<DeltaOutcome>
`DeltaOutcome = {attempted:false, reason} | {attempted:true, send:SendResult}` — index.js
feeds `attempted:true` outcomes into the breadcrumb aggregation.
```
async function maybeSendDelta(transcriptPath, sessionId, provider, config):
  key  = state.sanitizeSessionKey(sessionId)
  last = state.readOffset(config.stateDir, key)          // corrupt/missing/negative → 0 (see state.md)

  try: fileLen = fs.statSync(transcriptPath).size        // ONE fstat (ADR-007: cheap gate)
  catch: return {attempted:false, reason:"stat"}         // missing/dir/TOCTOU → ship nothing, offset unchanged

  if fileLen < last:                                     // rewrite guard (A-4 / FR-11)
    state.writeOffset(config.stateDir, key, fileLen)     // reset; NEVER a negative span
    return {attempted:false, reason:"rewind"}
  if fileLen === last: return {attempted:false, reason:"unchanged"}   // AC-06: no POST

  frame = buildDeltaFrame(transcriptPath, last, fileLen, sessionId, provider)
  if frame === null: return {attempted:false, reason:"empty_span"}    // e.g. span entirely mid-char

  send = await transport.post(config, null, { sync:false, bodyBuf: frame.bodyBuf })
  if send.ok:
    state.writeOffset(config.stateDir, key, frame.offset + frame.byteLen)  // UNIFORM advance
    // normal frame: = boundary-trimmed span end; elided frame: = effectiveEnd (≈ fileLen)
  // failure: do NOT advance, do NOT enqueue (ADR-004) — next FNF spawn re-derives
  return {attempted:true, send}
```

### buildDeltaFrame(path, last, fileLen, sessionId, provider) -> {bodyBuf, offset, byteLen} | null
```
function buildDeltaFrame(path, last, fileLen, sessionId, provider):
  try: fd = fs.openSync(path, "r") catch: return null
  try:
    span = fileLen - last
    if span <= DELTA_SOFT_CAP:
      // ---- normal frame: declared offset = last (span start) ----
      raw = readAt(fd, last, span)                       // positioned fs.readSync; short read → use actual
      end = trimEndToUtf8Boundary(raw)                   // back off ≤3 bytes to last complete char
      if end === 0: return null                          // grew by continuation bytes only:
                                                         // ship nothing, offset unchanged (R-04 scenario 2)
      bytes = raw.subarray(0, end).toString("utf8")
      return assemble(last, bytes, sessionId, provider)
    else:
      // ---- elided frame: END-ANCHORED (ADR-008) ----
      // 1. Anchor: effectiveEnd = fileLen backed off ≤3 bytes so the tail's last char is
      //    complete (file may end mid-write). Almost always === fileLen for JSONL.
      tailProbe    = readAt(fd, fileLen - TAIL_BYTES, TAIL_BYTES)
      keptLen      = trimEndToUtf8Boundary(tailProbe)            // bytes kept from probe start
      effectiveEnd = (fileLen - TAIL_BYTES) + keptLen            // == fileLen when file ends on a boundary
      // 2. Tail: start at effectiveEnd - TAIL_BYTES, advanced ≤3 bytes FORWARD to the
      //    next char boundary (tail start may land mid-char).
      tailStartRaw = effectiveEnd - TAIL_BYTES
      tailBuf  = readAt(fd, tailStartRaw, TAIL_BYTES)            // ends at effectiveEnd
      s        = trimStartToUtf8Boundary(tailBuf)                // skip ≤3 leading continuation bytes
      tailStr  = tailBuf.subarray(s).toString("utf8")            // tail bytes sit at TRUE offsets
                                                                 // [tailStartRaw+s, effectiveEnd)
      // 3. Head: 48 KiB from the span start, end-trimmed to a boundary.
      headBuf  = readAt(fd, last, HEAD_BYTES)
      e        = trimEndToUtf8Boundary(headBuf)
      headStr  = headBuf.subarray(0, e).toString("utf8")
      // 4. Marker: N = raw bytes NOT shipped between head end and tail start.
      nElided  = (tailStartRaw + s) - (last + e)
      marker   = "…[" + nElided + " bytes elided]…"              // U+2026 each end (3 bytes)
      bytes    = headStr + marker + tailStr
      // 5. End-anchored declaration: frame ends exactly at effectiveEnd.
      offset   = effectiveEnd - Buffer.byteLength(bytes, "utf8")
      return assemble(offset, bytes, sessionId, provider)
      // NEVER offset = last for an elided frame (phantom hole + PreCompact starvation —
      // ADR-008 Context; R-06). Pinned F2 consequences hold with fileLen ≡ effectiveEnd:
      // hole behind content at (last, effectiveEnd − byteLen), high_water == effectiveEnd,
      // seam-crossing contiguous_tail, no NULs served.
  finally: fs.closeSync(fd)
  on any read error: return null                         // ship nothing, offset unchanged

function assemble(offset, bytes, sessionId, provider):
  byteLen = Buffer.byteLength(bytes, "utf8")
  frame = Object.assign({ type:"RecordEvent" },
            implantEvent("transcript_delta", sessionId, { offset, bytes },
                         /*topic_signal*/ null, provider))
  // topic_signal omitted, provider present (ImplantEvent omit-when-null rule, OVERVIEW);
  // payload matches TranscriptDeltaPayload {offset, bytes} exactly — no new wire surface.
  // session_id RAW — server mints http-{session_id}; client never prefixes.
  bodyBuf = Buffer.from(JSON.stringify(frame), "utf8")
  if bodyBuf.length > BODY_GUARD:                        // SR-04 post-serialization backstop
    // theoretically unreachable (raw cap 64 KiB × ≤6x JSON escape inflation ≈ 384 KiB);
    // one defensive rebuild with halved budgets, then give up:
    rebuild once with HEAD_BYTES/2, TAIL_BYTES/2 (same end-anchored math)
    if still > BODY_GUARD: return null                   // ship nothing — never throw, never 413
  return { bodyBuf, offset, byteLen }

function trimEndToUtf8Boundary(buf):       // returns kept byte length from the start
  // Scan back at most 3 bytes from buf.length to the last byte that STARTS a char
  // (byte & 0xC0 !== 0x80). If no start byte within 4 positions, the window is
  // mid-sequence garbage → keep buf.length as-is (content-opaque tolerance).
  i = buf.length - 1
  while i >= 0 and i >= buf.length - 4 and (buf[i] & 0xC0) === 0x80: i -= 1
  if i < 0: return 0                         // whole buffer is continuation bytes — ship nothing
                                             // (span grew by 1-3 trailing bytes of one char, R-04 sc.2)
  if i < buf.length - 4: return buf.length   // >3 continuation bytes = not valid UTF-8 anyway:
                                             // content-opaque tolerance, keep everything
  charLen = utf8SeqLen(buf[i])               // 1 for <0x80, 2 for 0xC0-, 3 for 0xE0-, 4 for 0xF0-;
                                             // invalid lead byte → treat as 1 (lossy-tolerant)
  return (i + charLen <= buf.length) ? buf.length : i        // complete char → keep all; else cut before it

function trimStartToUtf8Boundary(buf):     // first index whose byte is NOT a continuation (max skip 3)
  i = 0
  while i < buf.length and i < 4 and (buf[i] & 0xC0) === 0x80: i += 1
  return i
```

## Offset Persistence (delegated to state.js)
- `offsets/{session_key}.json` = `{ "offset": N, "updated": nowSecs }`, temp+rename.
- Concurrent spawns: last-writer-wins; worst case a re-shipped span deduped by F2's
  idempotent offset-bounded merge (R-05 — accepted).
- Monotonic except the rewrite-guard reset.

## Error Handling
- Every fs call wrapped; any failure → `{attempted:false}`; offset NEVER advances on an
  unshipped or failed span. The function never throws and never rejects.
- Content-opaque: works identically on non-JSONL/binary transcript files (still caps,
  trims at byte boundaries — UTF-8 trims degrade to ≤3-byte adjustments on arbitrary
  bytes; lossy decode of genuinely invalid UTF-8 inside the span is accepted, the server
  is content-opaque) — never throws (edge-case list).
- No stderr from this module except via index.js outcome handling.

## Key Test Scenarios
1. AC-06: grow → POST with declared `offset == last`, persisted offset == trimmed span
   end; hold → no POST; assert persisted VALUES not just POST presence (R-04).
2. UTF-8 trims: file ending mid-2/3/4-byte char; span entirely inside one char → ship
   nothing, offset unchanged; next spawn boundary-clean (R-04 property test:
   `sum(shipped spans) == contiguous prefix`).
3. AC-07 elision: >64 KiB span → ONE frame, `bytes = head ++ "…[N bytes elided]…" ++ tail`,
   declared `offset == effectiveEnd − byteLength(bytes)` (NOT span start),
   `offset + byteLength == effectiveEnd`, offset advances to effectiveEnd, elided bytes
   never re-sent; escape-heavy content stays < 1 MiB serialized.
4. Elision boundary interaction: cap cut landing mid-char at head-end and tail-start;
   file ending mid-char during elision (effectiveEnd < fileLen by ≤3).
5. Rewrite guard: shrink file → offset reset to fileLen, nothing shipped, no negative span.
6. ADR-004: delta send failure → offset non-advance + NO queue file appears (amended
   AC-15); next spawn re-derives a larger span; R-07 livelock probe (permanent 413/401
   on delta path) → bounded per-spawn cost (1 fstat + 1 POST), carrying events unaffected.
7. Layer-2 pinned assertions against merged F2 (R-06): hole behind content, high_water,
   seam-crossing contiguous_tail, no NULs served.
8. TOCTOU: file deleted between stat and open/read → `{attempted:false}`, offset unchanged.
