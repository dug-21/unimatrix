## ADR-004: transcript_delta Frames Are Never Queued — Failed Deltas Re-Derive from the Offset on the Next Spawn

### Context

SR-06 (High/Medium): queued `transcript_delta` frames would persist raw conversation
bytes (potentially secrets) unencrypted in `~/.unimatrix/{hash}/`. ass-069 Q4 set the
server-side posture as "never disk-spill raw transcript" (principle 8); a client-side
queue file containing transcript bytes would reintroduce exactly the exposure the server
design eliminated. Meanwhile AC-15 says fire-and-forget frames are enqueued on send
failure — and deltas are fire-and-forget. But deltas have a property no other frame has:
**they are re-derivable**. The transcript file itself is the durable source, and F2's
merge is offset-bounded and idempotent (ass-069 Q1), so re-shipping a span is free.

### Decision

`transcript_delta` frames are exempt from the disk queue (ADR-003 carve-out):

- On delta send **success**: advance `last_offset` to the end of the shipped span —
  uniformly `declared offset + bytes.length` (boundary-trimmed end for normal frames;
  `file_len` for elided frames, which are end-anchored per ADR-008 — AC-07).
- On delta send **failure** (unreachable, timeout, non-2xx): do **not** advance
  `last_offset`, do **not** enqueue. The next fire-and-forget spawn re-reads
  `[last_offset, file_len)` — a fresh, possibly larger span — and ships that.
- The carrying event's own frame still queues normally on failure (ADR-003); the two
  POSTs remain independent (ADR-007, AC-09).

The transcript file is the queue: durable, already on disk under the host CLI's
ownership, and read-only to us. Self-healing is strictly better than queue replay for
this frame type — a replayed stale delta could only deliver the same bytes the next
live delta already covers.

Accepted losses (degradation contract, ass-069 Q1/Q6):
- If the **final** event of a session (Stop/SessionClose) fails its delta, the tail
  beyond `last_offset` is never shipped — server-side observation reconstruction is the
  floor beneath streaming (re-scoped #670).
- An outage spanning heavy transcript growth means the eventual catch-up delta exceeds
  64 KiB and elides the middle (head 48 KiB + tail 12 KiB + marker, end-anchored per
  ADR-008 so the F2 hole forms behind the content) — same loss profile a dropped queued
  delta would have had.

### Consequences

- Easier: zero transcript bytes at rest on the client — SR-06 is eliminated for delta
  content rather than mitigated; the queue mini-spec (ADR-003) stays small because its
  worst-case payload is tool-event metadata; delivery of transcript content is actually
  *better* than queue replay (always ships the freshest span).
- Harder: AC-15's wording ("fire-and-forget frames are enqueued") needs a spec carve-out
  for `transcript_delta` — flagged as an open question for the spec writer; the Layer 2
  parity/drop tests must assert offset-non-advance on failure rather than queue presence.
- The `last_offset` file is now the single recovery datum for transcript continuity; its
  atomic-rename write discipline (ADR-003) is load-bearing.
