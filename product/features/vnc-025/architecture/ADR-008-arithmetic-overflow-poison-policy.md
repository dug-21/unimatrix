## ADR-008: Checked Offset Arithmetic (Drop-Whole on Overflow) + Treat-as-Empty Poison Recovery, Preserving Always-Ack

### Context

`offset` is an attacker-controlled u64 on an authenticated but untrusted wire
(`TranscriptDeltaPayload`, frozen). `offset + bytes.len()` computed naively panics in debug
builds near `u64::MAX` and wraps silently in release, corrupting span math. A panic inside
`apply_delta` or `contiguous_tail` executes under the per-session buffer mutex (ADR-001),
poisoning it — every later `lock()` fails, bricking merges and PreCompact for that session
(R-02/R-06, per-session DoS). ADR-002 specifies merge semantics but not arithmetic soundness;
ADR-003 froze the always-Ack contract but not what dispatch does when the lock is poisoned.
The risk strategy (R-06.2) requires the poison policy be written down, not implicit.
Pattern #734 (server-resilience) already prohibits panicking lock acquisition in async
server code.

### Decision

Two layers — the first makes poisoning unreachable from the wire, the second makes it
non-bricking if a future bug reaches it anyway.

**Layer 1 — no-panic arithmetic inside `TranscriptBuffer`:**

- The delta end is computed as `offset.checked_add(bytes.len() as u64)`. `None` (overflow)
  ⇒ **the whole delta is silently dropped**: no partial write, no state change, no
  `high_water` update, no `elided_bytes` accounting (the bytes never entered the span;
  `elided_bytes` counts only content dropped *from* the span). No log line — the
  fire-and-forget contract already drops malformed payloads silently, and AC-12's grep gate
  bans `tracing` in the new module. Drop-whole over partial-clip: a delta ending at
  `u64::MAX` is unreachable by any legitimate transcript and partial writes would complicate
  the AC-02 convergence argument for zero benefit.
- All remaining internal arithmetic on u64 offsets uses `checked_*`/`saturating_*` — in
  particular ring-tail base advancement is `end.saturating_sub(max_bytes as u64)` and
  clip math saturates at `base_offset`.
- **u64→usize conversions occur only on span-relative values already proven ≤ `max_bytes`**
  (ring-tail runs first, bounding the span at the cap, a usize). This makes every `as usize`
  in the module provably lossless; the invariant is documented at each conversion site.
  No raw `offset as usize` anywhere.
- Contract (load-bearing for layer 2): **no input reachable from the wire — any
  `offset: u64`, any `bytes` ≤ 1 MiB frame ceiling — can panic inside `TranscriptBuffer`**
  (R-02 coverage requirement; verified by the fuzz-ish randomized (offset, len) test).

**Layer 2 — treat-as-empty poison recovery at every buffer-mutex lock site:**

- No `lock().unwrap()` on the buffer mutex anywhere. Every site uses
  `lock().unwrap_or_else(|poisoned| poisoned.into_inner())` and, on the poison path,
  **`clear()`s the buffer and calls `Mutex::clear_poison()` before proceeding** — a panic
  mid-mutation may have left `data`/`holes`/`base_offset` mutually inconsistent, and empty
  is the only state with guaranteed invariants; clearing the poison flag is mandatory,
  otherwise it persists and every later lock re-triggers treat-as-empty recovery, breaking
  R-06.2's merge-resumes-and-accumulates. Serving a possibly-corrupt tail is also an SR-02
  hazard; dropping the bytes is the safe direction.

> *Amended 2026-06-06: Layer 2 recovery extended with `Mutex::clear_poison()` —
> validated by implementation (pattern #4748), which correctly deviated from the original text.*
- Resulting behavior per site: dispatch merge — recovered-empty buffer, delta applies
  fresh, `Ack` (always-Ack preserved, ADR-003); PreCompact read — degrades to the
  empty-buffer path (no transcript block prepended); purge/drain/sweep — `into_inner()` +
  `clear()` is what they do anyway; `bytes_purged` reported from the recovered state,
  best-effort.
- The session is therefore never bricked: one poisoning event costs at most the buffered
  transcript content of that session (acceptable — principle 8, crt-052 reconstructs),
  never its liveness.

### Consequences

- Easier: R-02 and R-06 close together; the no-panic contract plus recovery means no input
  sequence can deny PreCompact for a session; review reduces to "no bare `unwrap()` on the
  buffer mutex, no unchecked offset arithmetic" — grep-able.
- Harder: the poison path is near-dead code (layer 1 makes it unreachable from the wire)
  yet must still be tested — R-06.2 requires an explicit poisoned-mutex test (poison via a
  deliberately panicking closure in a test helper, then assert merge-resumes-empty and
  PreCompact-degrades-empty). Drop-whole on overflow means a byte-perfect adversarial delta
  straddling `u64::MAX` loses its in-range prefix too — irrelevant for legitimate clients,
  documented here so the spec doesn't "fix" it into partial-clip complexity.
- Cross-references: ADR-001 (mutex shape), ADR-002 (merge semantics this hardens),
  ADR-003 (always-Ack contract preserved on the poison path), pattern #734, pattern #4748
  (clear_poison amendment).
