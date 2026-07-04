# Test Plan — `Window` type + default `[NEW]`

**Type:** `Window` (±N events / ±T millis); default **±120 000 ms / ±3 candidate blocks**
**Risks:** R-05 (High) · **ACs:** AC-18, AC-08 (support), AC-07 (support)

> Clock/window tests use **explicit fixed offsets** and on/inside/outside boundaries (#4195/#4236), never
> `now_ts()`. The cross-plane join is **windowed, never exact**. Over-inclusion is the safe direction;
> precision is not load-bearing.

---

## R-05 — window default + override (AC-18)
- `test_window_default_applied_when_omitted` — `anchor`/`match` supplied, `window` omitted → default
  **±120 000 ms** for ts-bearing candidates and **±3 candidate blocks** (`byte_offset` proximity) for
  `ts:None`. Assert the selection bounds. (AC-18.)
- `test_window_override_honored_under_cap` — a caller-supplied `window` overrides the default and is bounded
  by the existing per-cycle cap. (AC-18.)
- `test_phase_self_bounding_ignores_window` — a `window` supplied alongside `phase` has NO effect (phase is
  self-bounding). (AC-18; ties `transcript-scope.md`.)

## R-05 — boundary correctness (feeds `distill-before-purge.md` windowed-join tests)
- `test_ts_bearing_boundary_triple` — for a candidate at the window edge: just-inside → included, exactly on
  the boundary → included, just-outside → excluded. Explicit fixed offsets. (#4236.)
- `test_byte_offset_block_window_triple` — for a `ts:None` candidate: ±3 blocks inside → included, at the
  3-block edge → included, 4 blocks out → excluded.

## Serde / shape
- `test_window_events_vs_millis_variants` — the `Window` type expresses ±N events and ±T millis; both
  deserialize and resolve. (SPEC OQ-3 — exact enum/struct shape is a pseudocode detail; bind the fixtures to
  whatever the pseudocode fixes.)

**Coverage requirement:** window default + override + phase-ignores-window; explicit-offset on/inside/outside
boundary cases; no `now_ts()` anywhere; no exact-timestamp-match join.
