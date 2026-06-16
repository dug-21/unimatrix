# Component 3 — `apply_delta` fold call (both routes) + buffer wiring

**File**: `crates/unimatrix-server/src/infra/session_transcript.rs` (modify)
**Seam**: `apply_delta` at `:150`; `TranscriptBuffer` struct at `:44`; `new` at `:136`; existing `clear()` at `:318`.
**ADRs**: ADR-001 (fold in buffer, both routes by construction), ADR-006 (survival), ADR-009 (believable-zero guard basis).

## Purpose

Embed the `ActivityCounters` accumulator and the shared `SignatureScanner` into `TranscriptBuffer`, and add the single fold call after the merge in `apply_delta`. Because the accumulator lives in the buffer and both the registered and held routes resolve to the *same* `Arc<Mutex<TranscriptBuffer>>` (see Component 5 / `session.rs:388-401`), the fold runs on both routes with no route-specific code (FR-B2, AC-06). This is the structural defense against the believable-zero trap (#750/#5025).

## Struct changes

```
struct TranscriptBuffer {
    ... existing fields (data, base_offset, high_water @ :53, holes, elided_bytes) ...
    activity: ActivityCounters,         // NEW — embedded fold accumulator (Component 1)
    scanner:  Arc<SignatureScanner>,    // NEW — shared compiled RegexSet (Component 2)
}
```

`scanner` is `Arc`-shared (one compile at startup, cheap clone per buffer). It does not change the buffer's content-opacity: it holds compiled regexes, never transcript bytes.

## Constructor changes

`TranscriptBuffer::new` (`:136`) gains the scanner parameter. EVERY construction site must thread the shared scanner through (see "Construction sites" below).

```
fn new(max_bytes: usize, scanner: Arc<SignatureScanner>) -> Self
    return TranscriptBuffer {
        ... existing init (high_water: 0, etc.) ...
        activity: ActivityCounters::new(),
        scanner,
    }
```

## The fold call

`apply_delta` (`:150`) current shape (verified): overflow guard at the top (`:154` clip/no-state-change), then the merge + `high_water` update (`:163`), with a zero-length-delta no-op branch (`:165`). Add the fold AFTER the merge completes, on the path where the delta is actually accepted.

```
fn apply_delta(&mut self, offset: u64, bytes: &[u8])
    // ── existing overflow guard ──
    if delta would overflow max_bytes:
        // clip, no state change, no high_water update (existing :154 behavior)
        return                                  // FOLD MUST NOT run on the rejected/clipped path
                                                // (a clipped delta is not merged; counting it would
                                                //  over-count bytes_total beyond what was accepted).
    // ── existing merge + high_water update (:163) ──
    ... merge bytes at offset; self.high_water = self.high_water.max(end) ...
    // ── existing zero-length no-op note (:165): high_water already updated ──

    // ── NEW: the fold (ADR-001). Runs under the buffer lock already held; no new lock. ──
    self.activity.fold(bytes, &self.scanner)    // bytes is the delta payload as received
```

Placement decisions (load-bearing):
- **After the merge, on the accepted path only.** A clipped/overflow-rejected delta returns early and is NOT folded — `bytes_total` reflects accepted bytes, consistent with what the buffer merged. (If the test plan later decides clipped bytes should count, that is a contract change negotiated with crt-055 first — default: count accepted bytes only.)
- **Zero-length delta**: reaches the fold; `fold` does `bytes_total += 0`, `delta_count += 1`, scan of empty bytes matches nothing. No panic (edge case, FR-B3).
- **`bytes` = the delta payload** as passed to `apply_delta` (the same slice the merge consumed), so `bytes.len()` is the payload length (FR-B3: "each delta's payload byte length"). NOT the buffer's total length.

## `clear()` must not reset the accumulator

`clear()` (`:318`, stream-resume) sets `base_offset = high_water` and keeps `high_water`/`elided_bytes`. It MUST NOT touch `self.activity` — the fold survives a stream-resume `clear()` (ADR-006, FR-B9). Add an explicit comment at `clear()` stating the accumulator is intentionally preserved, so a future edit does not "helpfully" zero it (that would reintroduce the believable-zero class).

## Construction sites (thread the scanner through)

Find every `TranscriptBuffer::new(...)` call and pass the shared `Arc<SignatureScanner>`. Expected sites (confirm by grep at implementation):
- The registered-session path that creates a buffer on first delta (`session.rs`).
- The crt-052 transcript-hold path that constructs/holds buffers (`infra/transcript_hold.rs`).
- Any test constructors (use `SignatureScanner::empty()` or a small test scanner).

The shared scanner is built once at startup (Component 9 → Component 2) and carried to wherever buffers are created — likely stored on `SessionRegistry` and/or `TranscriptHold` and passed into `new`. The exact carrier is an implementation detail; the constraint is: **one compiled scanner, shared, threaded into every `new`.**

## Error handling

- The fold is infallible (Component 1) — no error path added to `apply_delta`.
- A poisoned buffer mutex is handled at the read surface (Component 4), not here.

## Key test scenarios (hints)

- Registered route: applying deltas advances `bytes_total`/`delta_count`/`class_counts` (AC-05).
- **Mandatory held-route regression guard (AC-06)**: drive drain→hold→re-adopt for a representative TS-client cycle; deltas after drain (on the held `Arc` via `held_arc_for_session`) increment the SAME accumulator the registered route did — assert continuity across the drain boundary, not just two non-zero reads. Negative-mutation: removing the fold call must turn the test RED (a test still green without the held-route fold is invalid — pattern #3624).
- Clipped/overflow delta does NOT advance `bytes_total` (folded on accepted path only).
- Zero-length delta advances `delta_count` only; no panic.
- `clear()` preserves the accumulator (apply deltas → snapshot non-zero → `clear()` → snapshot still non-zero).
- Every `TranscriptBuffer::new` call site compiles with the new scanner param (build-level).
