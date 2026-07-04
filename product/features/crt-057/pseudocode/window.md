# Component: `Window` [NEW] + Cross-Plane Clock Normalization

File: `unimatrix-observe/src/types.rs` (type); clock helper in
`unimatrix-server/src/mcp/distill_handler.rs` (used by retrieve_scoped_candidates).

## Purpose

Express the ±radius around `anchor`/`match` hits, and own the server-side cross-plane clock
normalization so the caller queries in its own units and never sees Plane B's storage clock
(ADR-006, FR-17/FR-18, AC-08/AC-18, R-05).

## Type + default

```
#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct Window {                     // serde: { "millis"?: u64, "blocks"?: u32 }
    millis: Option<u64>,            // time radius for ts-bearing candidates
    blocks: Option<u32>,            // byte_offset block radius for ts:None candidates
}
const DEFAULT_WINDOW_MILLIS: u64 = 120_000   // ±2 min (ADR-006 §Decision, OQ-2)
const DEFAULT_WINDOW_BLOCKS: u32 = 3         // ±3 candidate blocks

fn Window::effective(w: Option<&Window>) -> (u64 /*millis*/, u32 /*blocks*/):
    millis = w.and_then(|x| x.millis).unwrap_or(DEFAULT_WINDOW_MILLIS)
    blocks = w.and_then(|x| x.blocks).unwrap_or(DEFAULT_WINDOW_BLOCKS)
    return (millis, blocks)
```

Design note (resolves OQ-3): `Window` carries BOTH a time radius and a block radius because a single
session mixes ts-bearing and ts:None candidates; a caller who thinks only in time still gets the block
fallback (default 3) so ts:None candidates never silently drop. Unspecified fields fall to the
canonical defaults, not to zero (a zero-width window would re-introduce the silent-miss failure). Both
default to the ADR-006 constants; caller-overridable; final selection is still bounded by the existing
per-cycle cap.

## Clock normalization (named boundary helper — R-05, #3385/#3372)

Route EVERY Plane A↔B timestamp comparison through one named helper so the unit mismatch cannot silently
recur:

```
// Plane B TranscriptCandidate.ts is Option<String> (JSONL, e.g. ISO-8601).
// Plane A EvidenceRecord.ts / cycle_events bounds are u64 epoch-millis.
fn candidate_epoch_ms(ts: &Option<String>) -> Option<u64>:
    match ts:
        None    => None                       # ts:None → caller must use byte_offset fallback
        Some(s) => parse_iso8601_to_epoch_ms(s).ok()   # total: parse failure → None, treated as ts:None
```

### Windowed join (NEVER exact) — ts-bearing candidates

```
fn window_contains(bounds: (lo_ms, hi_ms), window: Option<&Window>, candidate) -> bool:
    (millis, blocks) = Window::effective(window)
    match candidate_epoch_ms(&candidate.ts):
        Some(c_ms) => (bounds.lo_ms - millis) <= c_ms <= (bounds.hi_ms + millis)   # saturating sub
        None       => byte_offset_within_blocks(bounds, blocks, candidate)          # ts:None fallback
```

### Byte-offset fallback — ts:None candidates (AC-07)

A ts:None candidate is included when its `byte_offset` is within `blocks` candidate-blocks of the
nearest in-window ts-bearing candidate of the SAME session. Implementation: within a session, order
candidates by `byte_offset`; find the block-index range covered by the ts-window; include ts:None
candidates whose block index is within `±blocks` of that range. Flag included-via-fallback candidates so
the consumer sees the fallback fired (surface in `SessionSearchStatus`/response — see
distill-before-purge.md).

### phase_contains (self-bounding — ignores window)

```
fn phase_contains(bounds: (phase_start_ms, phase_end_ms), candidate) -> bool:
    match candidate_epoch_ms(&candidate.ts):
        Some(c_ms) => phase_start_ms <= c_ms <= phase_end_ms       # NO window padding
        None       => byte_offset_within_blocks(bounds, DEFAULT_WINDOW_BLOCKS, candidate)  # ts:None still not dropped
```

## Error handling

- Timestamp parse failure → `None` (candidate treated as ts:None, routed to byte_offset fallback). Never
  panics, never drops silently.
- Saturating arithmetic on `lo_ms - millis` (avoid u64 underflow at epoch origin in tests).

## Key test scenarios (explicit fixed offsets, never `now_ts()` — #4195/#4236)

- Skewed-clock join: anchor at Plane A `ts=T`; candidate JSONL `ts` offset by a realistic skew within
  ±120 000 ms → selected. An exact-match join would miss it — assert found (AC-08, R-05 sc.1).
- Boundary triple per #4236: candidate just-inside / on / just-outside `[lo−millis, hi+millis]` →
  in/in/out.
- ts:None inside byte-proximity → included + flagged; ts:None outside `±blocks` → excluded (AC-07, R-05 sc.2).
- `window` omitted → ±120 000 ms / ±3 blocks applied (AC-18); caller override honored under cap.
- Self-bounding `phase` ignores a supplied `window` (R-09 sc.4).
- No test path supplies a Plane-B storage timestamp as a query unit (R-05 sc.4).
