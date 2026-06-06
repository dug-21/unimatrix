# Pseudocode: transcript-buffer (`infra/session_transcript.rs` — NEW module, ≤500 lines)

ADRs: ADR-002 (representation), ADR-007 (key seam), ADR-008 (arithmetic + poison).
FRs: FR-01, FR-02, FR-05, FR-11, FR-19, FR-20, NFR-01, NFR-09.

## Purpose

Per-session, in-memory, never-persisted accumulator of opaque transcript bytes. Idempotent
offset-bounded merge; ring-tail overflow; hole tracking; contiguous-tail reader for PreCompact.
Content-opaque by construction. This module is a pure state machine — no I/O, no `tracing`
(AC-12 grep gate), no locks (locking is the caller's job — registry-wiring component).

## Module Contents

```
const DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES: usize = 4_194_304   // 4 MiB; shared with config-knob default
const MAX_HOLE_RANGES: usize = 64                              // ADR-002 bounded metadata (tunable; the bound is the requirement)

pub struct TranscriptBuffer { base_offset, data, holes, high_water, elided_bytes, max_bytes }  // see OVERVIEW.md
pub struct TranscriptPurgeRecord { pub session_id: String, pub bytes_purged: u64 }
pub fn session_key(tenant, project, session_id) -> String
impl TranscriptBuffer { new, apply_delta, contiguous_tail, len, is_empty, high_water, elided_bytes, clear }
impl fmt::Debug for TranscriptBuffer   // manual, metadata-only
// NO Display impl. NO derive(Debug). NO serde derives. apply_delta returns ().
```

## Struct Invariants (document at struct definition; every method preserves them)

- I1: `data.len() <= max_bytes` at all times (ring-tail runs before any extension).
- I2: span = `[base_offset, base_offset + data.len())`; `holes` are disjoint, sorted ascending,
  strictly inside the span, and `holes.len() <= MAX_HOLE_RANGES` after every `apply_delta`.
- I3: `high_water` is monotonic non-decreasing; tracks what was *sent*, not what is retained.
- I4: hole regions in `data` are zero-filled but NEVER served (`contiguous_tail` stops at the
  last hole end).
- I5: u64→usize conversions occur ONLY on span-relative values already proven `<= max_bytes`
  (document this invariant in a comment at each conversion site — ADR-008 review gate).

## Functions

### `pub fn new(max_bytes: usize) -> TranscriptBuffer`

```
return TranscriptBuffer {
    base_offset: 0, data: Vec::new(), holes: Vec::new(),
    high_water: 0, elided_bytes: 0, max_bytes,
}
```

### `pub fn apply_delta(&mut self, offset: u64, bytes: &[u8])`

Returns `()`. Never panics for any `(offset: u64, bytes ≤ 1 MiB)` — NFR-09 contract.

```
len_u64 = bytes.len() as u64

// -- ADR-008 Layer 1: drop-whole on end overflow. DELIBERATE: no partial clip,
// -- no state change, no high_water update, no elided accounting, no log line.
end = offset.checked_add(len_u64)
if end is None: return
end = end.unwrap()

// -- high_water is updated for every non-overflowing delta, including len-0 and
// -- below-floor deltas (FR-02: "high_water still updated").
high_water = max(high_water, end)

// -- Zero-length delta: defined no-op beyond the high_water update (edge case pinned).
if len_u64 == 0: return

// -- Step 1: ring-tail BEFORE writing (I1). required_base is the lowest base that
// -- lets [.., end) fit within max_bytes.
required_base = end.saturating_sub(max_bytes as u64)
if required_base > base_offset:
    advance_base(required_base)            // private helper below; updates elided/holes/data

// -- Step 2: clip the incoming delta against the (possibly advanced) floor.
if end <= base_offset:
    // entirely below floor — defined no-op: clipped, counted, high_water already updated (FR-02)
    elided_bytes = elided_bytes.saturating_add(len_u64)
    return
write_offset = offset
write_bytes  = bytes
if offset < base_offset:
    clip = base_offset - offset                    // safe: offset < base_offset
    elided_bytes = elided_bytes.saturating_add(clip)
    write_bytes  = &bytes[clip as usize ..]        // clip < len_u64 <= 1 MiB — fits usize (I5)
    write_offset = base_offset

// -- INVARIANT here: base_offset <= write_offset, and end <= base_offset + max_bytes
// -- (because base_offset >= required_base = end - max_bytes). So all span-relative
// -- indices below are <= max_bytes (I5).

// -- Step 3: extend span, creating a hole if the delta starts past the current end.
span_end = base_offset + data.len() as u64         // <= base_offset + max_bytes; no overflow
                                                   // concern beyond end (checked above) — use
                                                   // saturating_add + comment anyway
if write_offset > span_end:
    holes.push((span_end, write_offset))           // new gap; normalized in Step 6
if end > span_end:
    data.resize((end - base_offset) as usize, 0)   // span-relative, proven <= max_bytes (I5)
                                                   // zero-fills both the gap and the write region

// -- Step 4: in-place write (idempotent: duplicates/overlaps are rewrites — FR-02)
rel = (write_offset - base_offset) as usize        // span-relative (I5)
data[rel .. rel + write_bytes.len()].copy_from_slice(write_bytes)

// -- Step 5: hole surgery — remove [write_offset, end) from `holes`.
subtract_range_from_holes(write_offset, end)       // private helper below

// -- Step 6: bounded metadata (ADR-002). If Step 3's push or Step 5's split exceeded
// -- the cap, collapse to the newest contiguous segment.
if holes.len() > MAX_HOLE_RANGES:
    collapse_to_newest()                           // private helper below
```

### private `fn advance_base(&mut self, new_base: u64)`

Precondition: `new_base > base_offset`. Drops head content; elision accounting counts only
*received* bytes (hole bytes are zero-fill that was never received — R-03.4 "never
double-counted when a hole is dropped below base").

```
span_end = base_offset + data.len() as u64
if new_base >= span_end:
    // whole existing span dropped
    received = data.len() as u64 - total_hole_bytes()        // sum of (e - s) over holes
    elided_bytes = elided_bytes.saturating_add(received)
    data.clear(); holes.clear()
    base_offset = new_base
else:
    drop_len = (new_base - base_offset) as usize             // < data.len() <= max_bytes (I5)
    hole_bytes_below = sum over holes of overlap_len(hole, [base_offset, new_base))
    elided_bytes = elided_bytes.saturating_add(drop_len as u64 - hole_bytes_below)
    data.drain(0 .. drop_len)
    // hole adjustment: drop holes entirely below new_base; a hole straddling new_base
    // is truncated to (new_base, hole_end)
    holes.retain_and_truncate(new_base)
    base_offset = new_base
```

### private `fn subtract_range_from_holes(&mut self, start: u64, end: u64)`

Four mutation classes per hole `(hs, he)` overlapping `[start, end)` (R-01.2 test classes):

```
for each hole (hs, he) in holes (rebuild list):
    if he <= start or hs >= end:   keep unchanged          // no overlap
    elif start <= hs and end >= he: remove                  // (a) fully filled
    elif start <= hs:               keep (end, he)          // (b) shrunk from left
    elif end >= he:                 keep (hs, start)        // (b) shrunk from right
    else:                           keep (hs, start), (end, he)  // (c) split in two
// a single write may span multiple holes (d) — the per-hole rules above compose
// result stays sorted + disjoint by construction
```

### private `fn collapse_to_newest(&mut self)`

ADR-002: collapse the buffer to the newest contiguous segment; old span counted as elided.

```
last_hole_end = holes.last().1                     // holes non-empty when called
// received bytes being dropped: everything from base_offset to last_hole_end, minus hole bytes
dropped_received = (last_hole_end - base_offset) - total_hole_bytes()
elided_bytes = elided_bytes.saturating_add(dropped_received)
data.drain(0 .. (last_hole_end - base_offset) as usize)    // span-relative (I5)
holes.clear()
base_offset = last_hole_end
// post: single contiguous segment [last_hole_end, span_end); invariants I1..I4 hold
```

### `pub fn contiguous_tail(&self, window: usize) -> Option<Vec<u8>>`

Never crosses a hole, never returns zero-fill (FR-19). `None` when empty or window == 0.

```
if data.is_empty() or window == 0: return None
span_end = base_offset + data.len() as u64
tail_floor = match holes.last(): Some((_, he)) => he, None => base_offset
avail = (span_end - tail_floor) as usize           // span-relative (I5); >= 1 by I2 (holes
                                                   // are strictly inside the span)
take = min(window, avail)
start_rel = data.len() - take
return Some(data[start_rel ..].to_vec())           // ≤ window bytes copied (≤ 12,000 for PreCompact)
```

### `pub fn clear(&mut self) -> u64`

Returns bytes purged (span length — the value ADR-004 audits as `bytes=<n>`). Post-clear
semantics pinned for crt-052 (R-10.3):

```
purged = data.len() as u64
data.clear()
holes.clear()
base_offset = high_water      // pinned: a resumed stream at offsets >= high_water continues
                              // cleanly (no giant hole); deltas below high_water after a clear
                              // are defined no-ops (clipped + counted, same as ring-tail floor)
// high_water unchanged (monotonic, I3); elided_bytes unchanged (lifetime counter — clear()
// is a purge, not an elision; the purged count is *returned*, not added to elided_bytes)
return purged
```

### Metadata accessors

```
pub fn len(&self) -> usize          { data.len() }       // span length; 0 ⇒ empty ⇒ purge emits nothing
pub fn is_empty(&self) -> bool      { data.is_empty() }
pub fn high_water(&self) -> u64     { high_water }
pub fn elided_bytes(&self) -> u64   { elided_bytes }
```

### `impl fmt::Debug for TranscriptBuffer` (manual — SR-02/ADR-002)

```
write "TranscriptBuffer {{ len: {}, base_offset: {}, high_water: {}, holes: {}, elided_bytes: {} }}"
    data.len(), base_offset, high_water, holes.len(), elided_bytes
// NEVER any byte of `data`. SessionState keeps derive(Debug) — this impl is what it prints.
```

### `pub fn session_key(_tenant: &str, _project: &str, session_id: &str) -> String` (ADR-007)

```
/// Enterprise seam: collapses the (tenant, project, session) composite dimension to the
/// registry's string key. OSS: tenant is always "default"; returns session_id unchanged
/// (transport namespacing via the existing `http-` prefix is orthogonal, applied earlier
/// in prefix_session_id). Enterprise re-key changes THIS function only; no call-site re-key.
/// LOAD-BEARING despite being degenerate — do not "simplify" away (ADR-007).
return session_id.to_string()
```

## Error Handling

- No `Result` anywhere in the public API — nothing can carry bytes (SR-02). `apply_delta`
  returns `()`; the only failure mode is the silent drop-whole on offset overflow.
- No panics reachable from the wire: end computed with `checked_add`; all other u64 math
  `saturating_*`; all u64→usize casts span-relative and proven `<= max_bytes` (I5).
- Mutex poisoning is handled by *callers* (registry-wiring/dispatch-wiring) — this module
  contains no locks.

## Key Test Scenarios (hints for tester — R-01/R-02/R-03/R-15)

1. Permutation convergence below cap: fixed covering delta set, shuffled orders + duplicates +
   partial overlaps → identical content + identical `high_water` (derive expectations
   programmatically from the covered-range set, #2984; reuse ass-069 PoC fixtures).
2. Hole surgery: fill / shrink-left / shrink-right / split / multi-hole-span — assert hole
   list + content after each.
3. Below-floor delta after ring-tail advance: no content change, `elided_bytes` += len,
   `high_water` updated.
4. `apply_delta(u64::MAX - 10, [0u8; 100])` — no panic, NO state change at all (not even
   `high_water`).
5. Far-offset jump (`1 << 40`, small payload): allocation stays ≤ cap; `base_offset` advances;
   prior received bytes counted as elided.
6. Cap-crossing sequences in multiple orders: final `contiguous_tail(window)` identical
   (tail-window equivalence); full content NOT asserted equal.
7. 1 MiB delta into a 4 MiB-cap and into a 64 KiB-cap buffer — both merge, small cap ring-tails.
8. 65th hole → collapse-to-newest: correctness, elision count, no panic, memory bounded.
9. `contiguous_tail`: window > len, window 0, window landing exactly on a hole boundary; never
   any zero-fill byte in output.
10. `clear()` → returns span len; post-clear resumed stream at high offsets merges cleanly
    (base = high_water pinned); `Debug` output contains metadata only (sentinel-string test).
11. Fuzz-ish randomized (offset, len) pairs incl. near-`u64::MAX` — no panic (NFR-09 named
    verification).
12. Zero-length delta at any offset: only `high_water` moves.
13. Cap exactly equal to one delta's size; delta exactly at cap boundary (off-by-one).
