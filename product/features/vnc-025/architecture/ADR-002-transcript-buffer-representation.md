## ADR-002: Contiguous-Span Buffer with Hole Ranges, Ring-Tail Overflow, Metadata-Only Elision, Content-Opaque by Construction

### Context

The merge must be idempotent and order-independent (AC-02: out-of-order, duplicate, and
overlapping deltas converge), bounded (AC-07: 4 MiB accumulated cap, ring-tail, resolved
decision 1), and must never serve NUL-filled holes in the PreCompact tail (resolved
decision 2 — tail-contiguity check, representation encapsulated). A naive "reset on gap"
scheme fails AC-02: arrival order `[50,100) then [0,50)` would yield different content than
`[0,50) then [50,100)`. Separately, SR-02 requires the type to be content-opaque — no
`Debug`/`Display` over bytes, no content in any return or error path — because in-memory +
purge IS the secrets guarantee (#4721); there is no redactor behind it.

### Decision

`TranscriptBuffer` in new module `infra/session_transcript.rs`:

```rust
pub struct TranscriptBuffer {
    base_offset: u64,        // logical offset of data[0]
    data: Vec<u8>,           // spans [base_offset, base_offset + data.len())
    holes: Vec<(u64, u64)>,  // unwritten sub-ranges within the span (zero-filled in data)
    high_water: u64,         // max(offset + len) ever seen — monotonic, survives clipping
    elided_bytes: u64,       // bytes dropped by ring-tail advancement or below-base clipping
    max_bytes: usize,        // cap, injected at construction (ADR-006)
}
```

- `apply_delta(offset, bytes)`: write bytes into the span, extending `data` forward as needed;
  a write landing inside an existing hole shrinks/splits/removes that hole; bytes below
  `base_offset` are clipped (counted in `elided_bytes`); duplicates and overlaps are in-place
  rewrites (idempotent). A delta starting beyond the current span end creates a zero-filled
  hole range — convergence: final state depends only on the set of covered ranges, not
  arrival order (AC-02).
- **Ring-tail**: whenever the span would exceed `max_bytes`, advance `base_offset` (drop
  head bytes, drop holes now below base, add to `elided_bytes`). Allocation is therefore
  always ≤ cap, even for a delta whose offset jumps far ahead. `high_water` is never reduced.
- **Elision is metadata, not bytes**: no marker is spliced into `data` (spliced bytes would
  corrupt offset math and JSONL line parsing). `elided_bytes` + a non-zero `base_offset` ARE
  the elision record; readers that care surface it from metadata.
- **Bounded metadata**: `holes` is capped at 64 ranges; a delta that would create a 65th
  collapses the buffer to the newest contiguous segment (old span counted as elided). This
  bounds memory and CPU against pathological sparse-delta clients.
- `contiguous_tail(window) -> Option<Vec<u8>>`: returns up to `window` bytes from the end of
  the span, truncated at the most recent hole boundary — never crosses a hole, never returns
  zero-fill (resolved decision 2). `None` when empty.
- **Content opacity (SR-02)**: manual `impl Debug` prints `{ len, base_offset, high_water,
  holes: n, elided_bytes }` only; no `Display`; `apply_delta` returns `()`; no `Result` in the
  API can carry bytes; the only content-bearing output is `contiguous_tail`, consumed solely
  by the PreCompact block builder (ADR-005). AC-12's grep gate on `tracing` in new modules
  enforces the rest.

AC-02 × overflow caveat (assumption A1, surfaced as open question 1 in ARCHITECTURE.md):
order-independence is exact below the cap; once ring-tail advances `base_offset`, a
late-arriving head delta is clipped, so cap-crossing sequences converge on the final tail
window rather than full content. Full-content convergence under overflow would require
covered-range replay buffering — speculative design for crt-052, rejected per resolved
decision 2. The representation stays encapsulated so range tracking is a local retrofit.

### Consequences

- Easier: AC-02 holds by construction below the cap; PreCompact can never serve holes;
  memory is hard-bounded (span ≤ cap, holes ≤ 64 ranges); crt-052 retrofits inside one module.
- Harder: hole bookkeeping is the most intricate code in the feature — it needs the densest
  property-style tests (shuffle/duplicate/overlap fixtures from the ass-069 PoC); the
  tail-window-equivalence phrasing of AC-02-under-overflow must be carried into the spec.
- Cross-references: ADR-001 (field shape), ADR-005 (sole reader), ADR-006 (cap injection).
